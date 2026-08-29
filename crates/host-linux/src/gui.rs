//! The Linux host's control window (egui/eframe).
//!
//! ⚠️ **This is deliberately NOT a third copy of `host-windows/src/gui.rs`.**
//! That file and its macOS twin are ~1,500 lines each and have already drifted
//! ~1,000 lines apart; `docs/LINUX-HOST.md` §6 argues the shared shell should be
//! extracted into a `host-ui` crate *before* a third host exists. Forking it
//! again here would have made that argument three times as expensive to act on,
//! so this window implements the connect flow and nothing else — no suite
//! navbar, no changelog popup, no profile disc, no nearby orbit. When `host-ui`
//! lands, this file adopts it rather than being reconciled with two others.
//!
//! What it does carry is the part a Linux user needs and the other two hosts
//! don't: an up-front `/dev/uinput` permission check. Without it the first
//! symptom of a missing udev rule is a phone that connects, says "connected",
//! and moves no slides.
//!
//! ⚠️ **It now takes exactly ONE thing from `host-ui`: `about_panel`** (added
//! 2026-08-29 with the suite-wide "About this app"). That is not the adoption
//! described above and does not start it — the chrome is still this file's own.
//! The panel is shared because it makes *claims about the product* (what
//! happens to your screen, the licence, the version), and a claim that can
//! drift between platforms is worse than a little duplication of layout.

use std::net::{TcpListener, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui;

use extender_host_ui::about_panel;

use crate::inject::{self, UinputStatus};
use crate::{capture, firewall, wifi, HostEvent};

/// Default listen port; the first free one at or after it is used.
const BASE_PORT: u16 = 9000;
/// UNI·SIM brand orange.
const BRAND: egui::Color32 = egui::Color32::from_rgb(0xe0, 0x55, 0x04);
const OPENSOURCE_URL: &str = "https://opensource.unisim.co.uk/screens";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([440.0, 760.0])
        .with_title("Universal Screens");
    if let Some(rgba) = crate::qr::app_icon_rgba(64) {
        viewport = viewport.with_icon(egui::IconData { rgba, width: 64, height: 64 });
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "Universal Screens Host",
        options,
        Box::new(|cc| Ok(Box::new(HostApp::new(cc)))),
    )
    .map_err(|e| e.to_string().into())
}

struct HostApp {
    /// Theme override. `Some(false)` = light, `Some(true)` = dark, `None` =
    /// follow the desktop. **Light is the default** until the user chooses
    /// otherwise — the suite-wide rule.
    dark_mode: Option<bool>,
    /// Set once the listener thread is running.
    running: bool,
    /// Signals the listener thread to stop.
    stop: Arc<AtomicBool>,
    /// The bound port and the LAN address a phone should dial.
    port: u16,
    ip: Option<String>,
    /// The 4-digit pairing PIN, regenerated each time the host starts.
    pin: u32,
    /// Latest lifecycle line from the accept loop.
    status: String,
    events: Option<Receiver<String>>,
    /// Checked when the window opens and again on each start.
    uinput: UinputStatus,
    /// Which capture backend this session got, or why it got none - decided once
    /// and cached, because it is an X server round trip and this is a per-frame
    /// UI. `Ok` means slide previews AND the H.264 mirror; `Err` means neither,
    /// which on a Wayland session is expected rather than broken.
    capture: Result<String, String>,
    firewall: firewall::FirewallState,
    wifi: Option<wifi::WifiInfo>,
    peers: Arc<Mutex<Vec<crate::discovery::DiscoveredPeer>>>,
    mdns_ad: Option<crate::discovery::MdnsAd>,
    /// Cached QR textures, keyed by the payload they were built from, so the
    /// codes aren't re-rasterised every frame.
    qr_cache: Vec<(String, egui::TextureHandle)>,
}

impl HostApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let storage = cc.storage;
        Self {
            dark_mode: storage
                .and_then(|s| eframe::get_value(s, "dark_mode"))
                .unwrap_or(Some(false)),
            running: false,
            stop: Arc::new(AtomicBool::new(false)),
            port: BASE_PORT,
            ip: best_lan_ip(),
            pin: gen_pin(),
            status: "Not started".to_owned(),
            events: None,
            uinput: inject::uinput_status(),
            capture: capture::status(),
            firewall: firewall::FirewallState::Inactive,
            wifi: wifi::current_wifi(),
            peers: Arc::new(Mutex::new(Vec::new())),
            mdns_ad: None,
            qr_cache: Vec::new(),
        }
    }

    /// Bind a port and start accepting, on a background thread.
    fn start(&mut self, ctx: &egui::Context) {
        let Some((listener, port)) = first_free_port(BASE_PORT) else {
            self.status = format!("Couldn't bind a port at or after {BASE_PORT}");
            return;
        };
        self.port = port;
        self.pin = gen_pin();
        self.ip = best_lan_ip();
        self.uinput = inject::uinput_status();
        self.capture = capture::status();
        self.firewall = firewall::state(port);
        self.wifi = wifi::current_wifi();
        self.stop = Arc::new(AtomicBool::new(false));

        let (tx, rx) = mpsc::channel::<String>();
        self.events = Some(rx);
        let stop = Arc::clone(&self.stop);
        let pin = self.pin;
        let ctx_for_thread = ctx.clone();
        thread::spawn(move || {
            crate::serve_loop(&listener, &stop, pin, &move |event| {
                report(&tx, &ctx_for_thread, event);
            });
        });

        // Discovery: the shared beacon + mDNS advertisement, exactly as the other
        // two hosts do it (the crate is platform-agnostic).
        crate::discovery::start_listener(
            Arc::clone(&self.peers),
            Arc::clone(&self.stop),
            ctx.clone(),
            Arc::new(Mutex::new(self.ip.clone())),
        );
        crate::discovery::start_beacon(crate::host_name(), port, Arc::clone(&self.stop));
        self.mdns_ad = crate::discovery::advertise_mdns(&crate::host_name(), port).ok();

        self.running = true;
        self.status = "Waiting for a client…".to_owned();
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Withdraw the mDNS advertisement rather than leaving a phone browsing
        // for a host that has gone.
        if let Some(ad) = self.mdns_ad.take() {
            ad.shutdown(); // returns nothing; failure here would only mean it was already gone
        }
        self.running = false;
        self.status = "Stopped".to_owned();
    }

    /// The address a phone dials, e.g. `192.168.1.20:9000`.
    fn host_addr(&self) -> String {
        format!("{}:{}", self.ip.as_deref().unwrap_or("0.0.0.0"), self.port)
    }

    /// Get (or build) the QR texture for `payload`.
    fn qr(&mut self, ctx: &egui::Context, payload: &str) -> Option<egui::TextureHandle> {
        if let Some((_, tex)) = self.qr_cache.iter().find(|(p, _)| p == payload) {
            return Some(tex.clone());
        }
        let image = crate::qr::branded_qr_app(payload)?;
        let tex = ctx.load_texture(payload, image, egui::TextureOptions::LINEAR);
        self.qr_cache.push((payload.to_owned(), tex.clone()));
        Some(tex)
    }
}

impl eframe::App for HostApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "dark_mode", &self.dark_mode);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_theme(match self.dark_mode {
            Some(true) => egui::ThemePreference::Dark,
            Some(false) => egui::ThemePreference::Light,
            None => egui::ThemePreference::System,
        });

        // Drain lifecycle lines from the accept thread.
        if let Some(rx) = &self.events {
            while let Ok(line) = rx.try_recv() {
                self.status = line;
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.heading(egui::RichText::new("Universal Screens").color(BRAND));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut dark = self.dark_mode.unwrap_or(ui.visuals().dark_mode);
                        if ui.checkbox(&mut dark, "🌙").changed() {
                            self.dark_mode = Some(dark);
                        }
                        ui.label(egui::RichText::new(format!("v{APP_VERSION}")).weak());
                        // Advanced ▸ About this app — the category every app in
                        // the suite gained on 2026-08-29. This window has no
                        // Actions menu to hang it off (it is deliberately lean;
                        // see the host-ui crate docs), so it lives beside the
                        // version, which is what a reader here is already
                        // looking at. Same shared panel as the other two hosts.
                        //
                        // ⚠️ APP_VERSION is passed IN — see host-ui.
                        ui.menu_button("⚙", |ui| {
                            ui.menu_button("ℹ  About this app", |ui| {
                                about_panel(ui, APP_VERSION);
                            });
                        })
                        .response
                        .on_hover_text("Advanced");
                    });
                });
                ui.label(
                    egui::RichText::new("Use your phone as a presentation clicker or trackpad")
                        .weak(),
                );
                ui.add_space(10.0);

                self.readiness_section(ui);
                ui.add_space(10.0);
                ui.separator();

                if self.running {
                    self.connect_section(ui, ctx);
                } else {
                    ui.add_space(14.0);
                    ui.vertical_centered(|ui| {
                        if ui
                            .add(egui::Button::new(
                                egui::RichText::new("  Start hosting  ").size(16.0),
                            ))
                            .clicked()
                        {
                            self.start(ctx);
                        }
                    });
                }

                ui.add_space(10.0);
                ui.separator();
                self.peers_section(ui);

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&self.status).weak());
                    if self.running {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Stop").clicked() {
                                self.stop();
                            }
                        });
                    }
                });
                ui.add_space(8.0);
                ui.hyperlink_to(
                    egui::RichText::new("With ♥ from UNISIM.co.uk").weak().small(),
                    "https://unisim.co.uk",
                );
            });
        });
    }
}

impl HostApp {
    /// The two things that stop a Linux host working, checked before the user
    /// discovers them the hard way.
    fn readiness_section(&mut self, ui: &mut egui::Ui) {
        // uinput — the one that produces a silent failure.
        match self.uinput.problem() {
            None => {
                ui.label(egui::RichText::new("✔ Input injection ready").color(ok_colour(ui)));
            }
            Some(problem) => {
                ui.colored_label(egui::Color32::from_rgb(0xc0, 0x39, 0x2b), format!("⚠ {problem}"));
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Without this the phone connects but nothing moves.")
                        .small()
                        .weak(),
                );
                ui.collapsing("How to fix it", |ui| {
                    ui.label("Install the udev rule, add yourself to the input group, then log out and back in:");
                    code_row(ui, &format!("echo '{}' | sudo tee {}", inject::UDEV_RULE, inject::UDEV_RULE_PATH));
                    code_row(ui, "sudo usermod -aG input $USER");
                    code_row(ui, "sudo udevadm control --reload-rules && sudo udevadm trigger");
                });
                if ui.button("Re-check").clicked() {
                    self.uinput = inject::uinput_status();
                }
            }
        }

        // Capture — slide previews and the mirror, together. Not an error when
        // it is missing: a Wayland session is a supported configuration that
        // gets a working clicker, and saying so is what stops it reading as a
        // fault. Same "say it once, up front" reasoning as the uinput check.
        ui.add_space(6.0);
        match &self.capture {
            Ok(backend) => {
                ui.label(
                    egui::RichText::new(format!("✔ Screen mirroring and previews ready ({backend})"))
                        .color(ok_colour(ui)),
                );
            }
            Err(reason) => {
                ui.label(egui::RichText::new(format!("• Screen mirroring off — {reason}")).weak());
                ui.label(
                    egui::RichText::new("The clicker and trackpad are unaffected.").small().weak(),
                );
            }
        }

        // Firewall — only worth a line when it's actually in the way.
        if self.running && !self.firewall.is_ok() {
            ui.add_space(6.0);
            ui.colored_label(
                egui::Color32::from_rgb(0xb8, 0x76, 0x0b),
                format!("⚠ {}", self.firewall.summary()),
            );
            if let Some(cmd) = self.firewall.fix_command(self.port) {
                code_row(ui, &cmd);
                if ui.button("Re-check").clicked() {
                    self.firewall = firewall::state(self.port);
                }
            }
        }
    }

    /// Steps 1 and 2 of the connect flow: get the app, then scan to connect.
    fn connect_section(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_space(10.0);
        step_header(ui, "1", "Get the app");
        if let Some(tex) = self.qr(ctx, OPENSOURCE_URL) {
            ui.vertical_centered(|ui| {
                ui.image((tex.id(), egui::vec2(150.0, 150.0)));
            });
        }
        ui.vertical_centered(|ui| {
            ui.hyperlink_to(egui::RichText::new(OPENSOURCE_URL).small(), OPENSOURCE_URL);
        });

        ui.add_space(14.0);
        step_header(ui, "2", "Scan to connect");
        let payload = connect_url(&self.host_addr(), self.pin, self.wifi.as_ref());
        if let Some(tex) = self.qr(ctx, &payload) {
            ui.vertical_centered(|ui| {
                ui.image((tex.id(), egui::vec2(190.0, 190.0)));
            });
        }
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(self.host_addr()).monospace().strong());
            ui.label(egui::RichText::new(format!("PIN {:04}", self.pin)).monospace().size(20.0));
        });

        ui.add_space(8.0);
        // ⚠️ The step-2 code above only joins the Wi-Fi for a phone that already
        // has the app (it reads the fragment and calls WifiNetworkSpecifier). A
        // phone using its plain camera needs the standard `WIFI:` payload, so
        // offer that separately — and only when nmcli actually gave up a key,
        // since a QR claiming "no password" for a WPA network just fails to join.
        match &self.wifi {
            Some(w) => {
                let key = w
                    .masked_password()
                    .map(|m| format!(" · key {m}"))
                    .unwrap_or_else(|| " · key not readable".to_owned());
                ui.label(egui::RichText::new(format!("Wi-Fi: {}{key}", w.ssid)).small().weak());
                if w.password.is_some() {
                    let payload = w.qr_payload();
                    ui.collapsing("Join this Wi-Fi (scan with the camera)", |ui| {
                        if let Some(tex) = self.qr(ctx, &payload) {
                            ui.vertical_centered(|ui| {
                                ui.image((tex.id(), egui::vec2(140.0, 140.0)));
                            });
                        }
                    });
                }
            }
            None => {
                ui.label(
                    egui::RichText::new(
                        "Wi-Fi network unknown — make sure the phone is on the same network.",
                    )
                    .small()
                    .weak(),
                );
            }
        }

        ui.add_space(6.0);
        // What this host can actually do depends on the session it is in, so
        // the line is built from the capture check rather than being a constant.
        // It used to read "input-only: screen mirroring does not work", which
        // Stage 2b made wrong on X11 and left right on Wayland.
        let capability = if self.capture.is_ok() {
            "Clicker, trackpad and screen mirroring all work on this session."
        } else {
            "Clicker and trackpad work; screen mirroring needs an X11 session."
        };
        ui.label(egui::RichText::new(capability).small().weak());
    }

    fn peers_section(&mut self, ui: &mut egui::Ui) {
        let peers = self.peers.lock().map(|p| p.clone()).unwrap_or_default();
        ui.add_space(8.0);
        ui.label(egui::RichText::new("NEARBY").small().strong().weak());
        if peers.is_empty() {
            ui.label(egui::RichText::new("No other hosts on this network").small().weak());
            return;
        }
        for peer in peers {
            ui.label(
                egui::RichText::new(format!("{}  ·  {}:{}", peer.name, peer.addr, peer.port))
                    .small(),
            );
        }
    }
}

/// Forward a lifecycle event to the GUI thread as a display string.
fn report(tx: &Sender<String>, ctx: &egui::Context, event: HostEvent) {
    let line = match event {
        HostEvent::Waiting => "Waiting for a client…".to_owned(),
        HostEvent::Connected { peer, platform } => {
            format!("Connected: {} ({peer})", platform_display(platform))
        }
        HostEvent::Disconnected(peer) => format!("Disconnected ({peer})"),
        HostEvent::Error(msg) => msg,
    };
    let _ = tx.send(line);
    ctx.request_repaint();
}

fn platform_display(p: extender_protocol::ClientPlatform) -> &'static str {
    use extender_protocol::ClientPlatform as P;
    match p {
        P::Windows => "Windows",
        P::Macos => "macOS",
        P::Linux => "Linux",
        P::Android => "Android",
        P::Ios => "iOS",
        P::Unknown => "Unknown device",
    }
}

fn ok_colour(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(0x5c, 0xb8, 0x5c)
    } else {
        egui::Color32::from_rgb(0x2d, 0x7a, 0x2d)
    }
}

fn step_header(ui: &mut egui::Ui, step: &str, title: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Step {step}")).color(BRAND).strong().small());
        ui.label(egui::RichText::new(title).strong());
    });
}

/// A monospace command with a copy button — the shape every fix in this window
/// takes, because none of them are things the app should run itself.
fn code_row(ui: &mut egui::Ui, cmd: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(cmd).monospace().small());
        if ui.small_button("Copy").clicked() {
            ui.output_mut(|o| o.copied_text = cmd.to_owned());
        }
    });
}

/// The deep link a phone scans: opens the app if installed (App Links), else the
/// download page. Identical in shape to the Windows host's, so one client build
/// reads both.
fn connect_url(host: &str, pin: u32, wifi: Option<&wifi::WifiInfo>) -> String {
    let (ip, port) = host.rsplit_once(':').unwrap_or((host, "9000"));
    let mut s = format!(
        "https://opensource.unisim.co.uk/screens/connect?host={}&port={}&pin={:04}",
        pe(ip),
        pe(port),
        pin,
    );
    if let Some(wifi) = wifi {
        // Wi-Fi creds go in the fragment so they never reach the web server.
        s.push_str("#ssid=");
        s.push_str(&pe(&wifi.ssid));
        s.push_str("&auth=");
        s.push_str(&pe(&wifi.auth));
        if let Some(p) = &wifi.password {
            s.push_str("&pass=");
            s.push_str(&pe(p));
        }
    }
    s
}

/// Percent-encode a query-string value (everything outside the RFC 3986
/// unreserved set), so SSIDs/passwords with spaces or symbols survive the QR.
fn pe(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A 4-digit pairing PIN, seeded from the clock (not a secret in the crypto
/// sense — it keys the Noise handshake, which is what actually protects the
/// session).
fn gen_pin() -> u32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    1000 + (nanos % 9000)
}

/// Bind the first free port at or after `start`, so the host "just works" even
/// when the default port is taken.
fn first_free_port(start: u16) -> Option<(TcpListener, u16)> {
    (start..start.saturating_add(50))
        .find_map(|port| TcpListener::bind(("0.0.0.0", port)).ok().map(|l| (l, port)))
}

/// The best LAN address for a phone to reach.
///
/// ⚠️ The default-route address is the wrong answer when a VPN owns the default
/// route (the Windows host hit exactly this with ProtonVPN) or when Docker's
/// bridge is up — both hand out an address the phone cannot reach. So real
/// interfaces are preferred first, by name, and the route trick is the fallback.
fn best_lan_ip() -> Option<String> {
    if let Ok(out) = std::process::Command::new("ip")
        .args(["-o", "-4", "addr", "show", "scope", "global"])
        .output()
    {
        if let Some(ip) = pick_lan_ip(&String::from_utf8_lossy(&out.stdout)) {
            return Some(ip);
        }
    }
    primary_lan_ip()
}

/// Interface-name prefixes that are never the way a phone reaches this machine.
const VIRTUAL_IFACES: &[&str] =
    &["docker", "veth", "br-", "virbr", "tun", "tap", "wg", "zt", "lo"];

/// Pick the first address on a real interface from `ip -o -4 addr show` output.
/// Split out from [`best_lan_ip`] so the filtering is testable without a NIC.
fn pick_lan_ip(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let _index = fields.next()?;
        let iface = fields.next()?;
        if VIRTUAL_IFACES.iter().any(|p| iface.starts_with(p)) {
            return None;
        }
        // … inet 192.168.1.20/24 brd …
        let addr = fields.skip_while(|f| *f != "inet").nth(1)?;
        let ip = addr.split('/').next()?;
        (ip.parse::<std::net::Ipv4Addr>().is_ok()).then(|| ip.to_owned())
    })
}

/// The IP of the default-route interface (no packets are sent). `None` if down.
fn primary_lan_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_url_carries_host_port_and_padded_pin() {
        let url = connect_url("192.168.1.20:9000", 42, None);
        assert!(url.contains("host=192.168.1.20"));
        assert!(url.contains("port=9000"));
        assert!(url.ends_with("pin=0042"), "{url}");
    }

    #[test]
    fn wifi_credentials_go_in_the_fragment() {
        let w = wifi::WifiInfo {
            ssid: "My Net".to_owned(),
            password: Some("p@ss word".to_owned()),
            auth: "WPA".to_owned(),
        };
        let url = connect_url("10.0.0.5:9001", 1234, Some(&w));
        let (query, fragment) = url.split_once('#').expect("credentials must be after the #");
        // The server must never see the key: nothing secret before the fragment.
        assert!(!query.contains("p%40ss"));
        assert!(fragment.contains("ssid=My%20Net"));
        assert!(fragment.contains("pass=p%40ss%20word"));
        assert!(fragment.contains("auth=WPA"));
    }

    #[test]
    fn percent_encoding_leaves_unreserved_characters_alone() {
        assert_eq!(pe("abc-123_x.y~z"), "abc-123_x.y~z");
        assert_eq!(pe("a b&c"), "a%20b%26c");
    }

    #[test]
    fn pin_is_always_four_digits() {
        for _ in 0..50 {
            let pin = gen_pin();
            assert!((1000..10000).contains(&pin), "{pin}");
        }
    }

    #[test]
    fn lan_ip_skips_docker_and_vpn_interfaces() {
        let out = "\
1: lo    inet 127.0.0.1/8 scope host lo\\       valid_lft forever
2: docker0    inet 172.17.0.1/16 brd 172.17.255.255 scope global docker0\\       valid_lft forever
3: tun0    inet 10.8.0.2/24 scope global tun0\\       valid_lft forever
4: wlan0    inet 192.168.1.20/24 brd 192.168.1.255 scope global dynamic wlan0\\       valid_lft 1234sec
";
        // Not the Docker bridge and not the VPN tunnel — the one the phone can reach.
        assert_eq!(pick_lan_ip(out), Some("192.168.1.20".to_owned()));
    }

    #[test]
    fn lan_ip_is_none_when_only_virtual_interfaces_exist() {
        let out = "2: docker0    inet 172.17.0.1/16 scope global docker0\\       valid_lft forever\n";
        assert_eq!(pick_lan_ip(out), None);
    }
}
