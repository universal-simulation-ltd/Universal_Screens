//! The host control window (GUI mode). Aimed at a non-technical user: on launch
//! it starts listening on the first free port and shows the address + branded QR
//! to use on the phone. It also shows this PC's identity and a list of recent
//! connections, each tagged with the device's platform icon. Advanced bits (port,
//! manual start/stop, auto-start preference) live under a collapsed "More options".

use std::net::{TcpListener, UdpSocket};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIcon, SendMessageW, WM_SETICON};

use crate::{serve_loop, HostEvent};

// The window chrome and helpers this host shares with the macOS one. See that
// crate's docs for what deliberately stays forked (HostApp, RecentConn,
// best_lan_ip) and why APP_VERSION must not move.
use extender_host_ui::{
    about_panel, connect_url, device_icon, first_free_port, gen_pin, gen_room_code, nearby_orbit,
    paint_brand_strip, platform_display, platform_tag, sep_dot, style_navbar, DeviceKind,
    BASE_PORT, BRAND, CHANGELOG, CHANGELOG_URL, OPENSOURCE_ROOT, OPENSOURCE_URL, RECENT_MAX,
    SIBLING_APPS,
};

/// This app's version, surfaced in the changelog popup.
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");


/// Launch the host window. Detaches the inherited console so a double-click (or
/// `cargo run` with no args) doesn't leave a stray terminal behind.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let _ = windows::Win32::System::Console::FreeConsole();
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([440.0, 720.0])
        // Keep a real title so the taskbar button / hover shows the name; the
        // caption *text* is then painted the same colour as the caption fill (in
        // set_title_bar) so it's invisible in the header, and the small caption
        // icon is blanked in update() — leaving just clean window chrome.
        .with_title("Universal Screens");
    if let Some(rgba) = crate::qr::app_icon_rgba(64) {
        viewport = viewport.with_icon(egui::IconData { rgba, width: 64, height: 64 });
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "Universal Screens Host",
        options,
        Box::new(|cc| {
            let mut app = HostApp::new(cc);
            if app.auto_connect {
                app.start(&cc.egui_ctx);
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| e.to_string().into())
}

/// A remembered client connection (most-recent first), for the GUI list.
#[derive(Clone, Serialize, Deserialize)]
struct RecentConn {
    /// Lowercase platform tag: "windows" | "macos" | "linux" | "android" | "ios".
    platform: String,
    /// The client's IP (port stripped).
    peer: String,
}

struct HostApp {
    auto_connect: bool,
    /// Theme override: None = follow the OS, Some(true) = dark, Some(false) = light.
    dark_mode: Option<bool>,
    /// 4-digit pairing code a client must present to connect (persisted).
    pin: u32,
    /// Reveal the "This PC: …" detail (toggled by clicking the OS icon).
    show_pc_info: bool,
    /// Last theme we painted the title bar for (re-applied on change).
    caption_dark: Option<bool>,
    port: String,
    running: bool,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<String>>,
    recent: Arc<Mutex<Vec<RecentConn>>>,
    address: Option<String>,
    /// The UNI·SIM mark, lazily uploaded for the navbar changelog icon + QR.
    logo: Option<egui::TextureHandle>,
    /// The Universal Screens app icon, lazily uploaded for the navbar product logo.
    app_logo: Option<egui::TextureHandle>,
    /// The signed-out profile disc — painted once, then cached like the logos.
    avatar: Option<egui::TextureHandle>,
    /// This PC's current Wi-Fi network (for the "join this network" step), if any.
    wifi: Option<crate::wifi::WifiInfo>,
    /// The Wi-Fi join QR, lazily built from `wifi`.
    wifi_qr: Option<egui::TextureHandle>,
    /// The one-scan combined QR (join Wi-Fi + connect), for the app's scanner.
    combined_qr: Option<egui::TextureHandle>,
    /// Reveal the Wi-Fi password (toggled by clicking the masked value).
    wifi_show_password: bool,
    /// Reveal the pairing PIN (toggled by clicking it); it's in the QR regardless.
    show_pin: bool,
    /// Whether an inbound firewall rule for the port exists (checked on start);
    /// None until the host first listens.
    firewall_ok: Option<bool>,
    /// "Cast to a browser": the code typed from a receiver tab, and the status of
    /// the dial-the-room bridge (shared with its background thread).
    cast_code: String,
    cast_status: Arc<Mutex<String>>,
    /// "Remote access": a room code this host publishes for someone on another
    /// network to reach it (the inverse of Cast — we mint the code and dial the
    /// rendezvous as sender). Minted lazily; status shared with the dial thread.
    remote_code: String,
    remote_status: Arc<Mutex<String>>,
    remote_active: bool,
    /// LAN peers discovered via UDP multicast beacon (PC → PC / PC → Mac).
    discovered_peers: Arc<Mutex<Vec<crate::discovery::DiscoveredPeer>>>,
    /// Stop flag for the always-on listener thread (set only when the app exits).
    listener_stop: Arc<AtomicBool>,
    /// Our own LAN IP, shared with the listener so it can filter out our beacon.
    own_ip: Arc<Mutex<Option<String>>>,
    /// Stop flag for the beacon sender (set in stop(), started fresh in start()).
    beacon_stop: Arc<AtomicBool>,
    /// DNS-SD advertisement (`_usscreens._tcp`) so phones can browse for this
    /// host — registered while serving, withdrawn in stop()/on_exit().
    mdns_ad: Option<crate::discovery::MdnsAd>,
    /// The connection QR the user tapped to enlarge for easier scanning (None =
    /// not enlarged). `qr_zoom_armed` guards against the very click that opened
    /// the overlay also closing it on the same frame.
    qr_zoom: Option<egui::TextureId>,
    qr_zoom_armed: bool,
}

impl HostApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let storage = cc.storage;
        let recent: Vec<RecentConn> =
            storage.and_then(|s| eframe::get_value(s, "recent")).unwrap_or_default();
        // Reuse the stored PIN, or mint one on first run.
        let mut pin: u32 = storage.and_then(|s| eframe::get_value(s, "pin_code")).unwrap_or(0);
        if pin == 0 {
            pin = gen_pin();
        }
        // Always-on LAN listener: nearby hosts appear even before this PC starts
        // serving, so a PC → PC / PC → Mac connection needs no QR scan.
        let discovered_peers = Arc::new(Mutex::new(Vec::new()));
        let listener_stop = Arc::new(AtomicBool::new(false));
        let own_ip: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        crate::discovery::start_listener(
            discovered_peers.clone(),
            listener_stop.clone(),
            cc.egui_ctx.clone(),
            own_ip.clone(),
        );
        Self {
            pin,
            show_pc_info: false,
            caption_dark: None,
            auto_connect: storage
                .and_then(|s| eframe::get_value(s, "auto_connect"))
                .unwrap_or(true),
            dark_mode: storage.and_then(|s| eframe::get_value(s, "dark_mode")).unwrap_or(None),
            port: storage.and_then(|s| eframe::get_value(s, "port")).unwrap_or_default(),
            running: false,
            stop: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new("Not started".to_owned())),
            recent: Arc::new(Mutex::new(recent)),
            address: None,
            logo: None,
            app_logo: None,
            avatar: None,
            wifi: crate::wifi::current_wifi(),
            wifi_qr: None,
            combined_qr: None,
            wifi_show_password: false,
            show_pin: false,
            firewall_ok: None,
            cast_code: String::new(),
            cast_status: Arc::new(Mutex::new(String::new())),
            remote_code: String::new(),
            remote_status: Arc::new(Mutex::new(String::new())),
            remote_active: false,
            discovered_peers,
            listener_stop,
            own_ip,
            beacon_stop: Arc::new(AtomicBool::new(true)), // starts in stopped state
            mdns_ad: None,
            qr_zoom: None,
            qr_zoom_armed: false,
        }
    }

    /// The enlarged-QR overlay: a dimmed backdrop with the tapped QR blown up as
    /// large as the window allows, centred, so it's easy to scan from a distance.
    /// A click anywhere (or Escape) closes it.
    fn show_qr_overlay(&mut self, ctx: &egui::Context) {
        let Some(tex) = self.qr_zoom else { return };
        let screen = ctx.screen_rect();
        // As big as fits (leaving a margin), clamped so it never gets silly.
        let side = (screen.width().min(screen.height()) - 72.0).clamp(220.0, 560.0);
        egui::Area::new(egui::Id::new("qr_zoom_area"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                // Dim the whole window on *this* layer first, so the card below is
                // drawn on top of it (same-layer z-order follows draw order — a
                // separate backdrop layer can sort above the Area instead).
                ui.ctx()
                    .layer_painter(ui.layer_id())
                    .rect_filled(screen, 0.0, egui::Color32::from_black_alpha(210));

                egui::Frame::none()
                    .fill(egui::Color32::WHITE)
                    .rounding(18.0)
                    .inner_margin(egui::Margin::same(16.0))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add(
                                egui::Image::from_texture(egui::load::SizedTexture::new(
                                    tex,
                                    egui::vec2(side, side),
                                ))
                                .rounding(12.0),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("Tap anywhere to close")
                                    .color(egui::Color32::from_gray(110))
                                    .size(13.0),
                            );
                        });
                    });
            });

        if !self.qr_zoom_armed {
            // Skip the frame that opened it, so the opening click isn't also read
            // as the closing click.
            self.qr_zoom_armed = true;
            return;
        }
        if ctx.input(|i| i.pointer.any_click() || i.key_pressed(egui::Key::Escape)) {
            self.qr_zoom = None;
        }
    }

    fn start(&mut self, ctx: &egui::Context) {
        self.stop();

        let bound = match self.port.trim() {
            "" => first_free_port(BASE_PORT),
            text => match text.parse::<u16>() {
                Ok(p) => match TcpListener::bind(("0.0.0.0", p)) {
                    Ok(listener) => Some((listener, p)),
                    Err(e) => {
                        *self.status.lock().unwrap() = format!("Could not use port {p}: {e}");
                        None
                    }
                },
                Err(_) => {
                    *self.status.lock().unwrap() = "Invalid port".to_owned();
                    None
                }
            },
        };
        let Some((listener, port)) = bound else {
            return;
        };

        self.stop = Arc::new(AtomicBool::new(false));
        *self.status.lock().unwrap() = "Waiting for your phone…".to_owned();
        let stop = self.stop.clone();
        let status = self.status.clone();
        let recent = self.recent.clone();
        let ctx = ctx.clone();
        let pin = self.pin;
        thread::spawn(move || {
            serve_loop(&listener, &stop, pin, &|event| {
                match event {
                    HostEvent::Waiting => {
                        *status.lock().unwrap() = "Waiting for your phone…".to_owned();
                    }
                    HostEvent::Connected { peer, platform } => {
                        let ip = peer.rsplit_once(':').map_or(peer.clone(), |(a, _)| a.to_owned());
                        let mut list = recent.lock().unwrap();
                        list.retain(|c| c.peer != ip);
                        list.insert(0, RecentConn { platform: platform_tag(platform).to_owned(), peer: ip });
                        list.truncate(RECENT_MAX);
                        *status.lock().unwrap() = format!("Connected: {peer}");
                    }
                    HostEvent::Disconnected(peer) => {
                        *status.lock().unwrap() = format!("{peer} disconnected — waiting…");
                    }
                    HostEvent::Error(msg) => *status.lock().unwrap() = msg,
                }
                ctx.request_repaint();
            });
            *status.lock().unwrap() = "Stopped".to_owned();
            ctx.request_repaint();
        });

        let ip = best_lan_ip().unwrap_or_else(|| "127.0.0.1".to_owned());
        self.address = Some(format!("{ip}:{port}"));
        // Re-read the network so the address/QR/Wi-Fi details reflect the current
        // connection (e.g. after switching to a phone hotspot and restarting).
        self.wifi = crate::wifi::current_wifi();
        self.wifi_qr = None;
        self.combined_qr = None;
        self.firewall_ok = Some(crate::firewall::rule_present(port));
        self.running = true;

        // Tell the listener our own IP so it can ignore our own beacons, then
        // start broadcasting so other hosts on the LAN can find this PC.
        *self.own_ip.lock().unwrap() = Some(ip.clone());
        self.beacon_stop.store(true, Ordering::Relaxed);
        let beacon_stop = Arc::new(AtomicBool::new(false));
        self.beacon_stop = beacon_stop.clone();
        crate::discovery::start_beacon(host_name(), port, beacon_stop);
        // And advertise over DNS-SD so the phone apps' host browsers (Android
        // NSD / iOS Bonjour) list this PC under their own "Nearby".
        self.mdns_ad = crate::discovery::advertise_mdns(&host_name(), port).ok();
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.beacon_stop.store(true, Ordering::Relaxed);
        if let Some(ad) = self.mdns_ad.take() {
            ad.shutdown();
        }
        *self.own_ip.lock().unwrap() = None;
        self.running = false;
        self.address = None;
        self.combined_qr = None;
    }

    /// One scan *in the app* to join this PC's Wi-Fi and connect: the
    /// combined QR (network + host + PIN), the network name, a reveal-on-click
    /// password, and a manual address/PIN fallback for phones already on the network.
    fn show_connect(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        step_header(ui, "Universal Screens", "Scan to connect");
        scan_subheader(ui, "Scan directly in the Universal Screens App");

        // Which QR (if any) was tapped this frame to enlarge. Applied to
        // `self.qr_zoom` after the borrow of `self.wifi` below is released.
        let mut zoom_clicked: Option<egui::TextureId> = None;

        if let Some(wifi) = &self.wifi {
            // The one-scan connect QR (the app joins this Wi-Fi *and* connects).
            // Falls back to a plain Wi-Fi-join QR before the host is listening.
            // It encodes an https `…/screens/connect` URL: scanned *in the app* it
            // deep-links straight in (Android App Link / iOS Universal Link) and
            // pairs from the query params; scanned with a *plain phone camera* it
            // lands on the friendly "scan this inside the app" page instead of a
            // dead `unisimscreens://` link (the "wrong way round" fix). Step 2 QRs
            // carry the Universal Screens app icon (not the UNI·SIM mark) so they
            // read as "scan these in the app".
            if self.running && self.address.is_some() {
                if self.combined_qr.is_none() {
                    if let Some(addr) = &self.address {
                        let payload = connect_url(addr, self.pin, Some(crate::wifi::as_qr(wifi)));
                        if let Some(image) = crate::qr::branded_qr_app(&payload) {
                            self.combined_qr = Some(ctx.load_texture(
                                "combined_qr",
                                image,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                }
                if let Some(id) = self.combined_qr.as_ref().map(egui::TextureHandle::id) {
                    if qr_clickable(ui, id, 200.0) {
                        zoom_clicked = Some(id);
                    }
                }
                ui.small("In the app, tap Scan and point it here — joins this Wi-Fi and connects.");
            } else {
                if self.wifi_qr.is_none() {
                    if let Some(image) = crate::qr::branded_qr_app(&wifi.qr_payload()) {
                        self.wifi_qr =
                            Some(ctx.load_texture("wifi_qr", image, egui::TextureOptions::LINEAR));
                    }
                }
                if let Some(id) = self.wifi_qr.as_ref().map(egui::TextureHandle::id) {
                    if qr_clickable(ui, id, 190.0) {
                        zoom_clicked = Some(id);
                    }
                }
                ui.small("Scan to join this PC's Wi-Fi.");
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new(format!("Network: {}", wifi.ssid)).strong());
            if let Some(masked) = wifi.masked_password() {
                let shown = if self.wifi_show_password {
                    wifi.password.clone().unwrap_or_default()
                } else {
                    masked
                };
                let hint =
                    if self.wifi_show_password { "Click to hide" } else { "Click to reveal" };
                let resp = ui
                    .add(egui::Label::new(format!("Password: {shown}")).sense(egui::Sense::click()))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(hint);
                if resp.clicked() {
                    self.wifi_show_password = !self.wifi_show_password;
                }
            } else {
                ui.label("Password: (open network)");
            }
        } else {
            // No Wi-Fi detected (Ethernet or hotspot). Still show the connect QR
            // so the phone can scan it — it just won't auto-join a network first.
            if self.running {
                if self.combined_qr.is_none() {
                    if let Some(addr) = &self.address {
                        let payload = connect_url(addr, self.pin, None);
                        if let Some(image) = crate::qr::branded_qr_app(&payload) {
                            self.combined_qr = Some(
                                ctx.load_texture("combined_qr", image, egui::TextureOptions::LINEAR),
                            );
                        }
                    }
                }
                if let Some(id) = self.combined_qr.as_ref().map(egui::TextureHandle::id) {
                    if qr_clickable(ui, id, 200.0) {
                        zoom_clicked = Some(id);
                    }
                }
                ui.small("In the app, tap Scan and point it here — make sure your phone is already on the same network.");
            } else {
                ui.small("This PC isn't on Wi-Fi — put your phone on the same network, then use the address in More details.");
            }
        }

        // The `self.wifi` borrow above has ended — now safe to record the enlarge.
        if let Some(tex) = zoom_clicked {
            self.qr_zoom = Some(tex);
            self.qr_zoom_armed = false;
        }

        // Nearby — other Universal Screens hosts discovered on the LAN via UDP
        // multicast. Primary use case: PC → PC / PC → Mac (no camera to scan a QR).
        // Drawn as the portal-style orbit: this PC in the centre, each peer a
        // node circling it — click a node to connect.
        let nearby = self.discovered_peers.lock().unwrap().clone();
        if !nearby.is_empty() {
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Nearby").strong());
            if let Some(peer) = nearby_orbit(ui, &nearby, "This PC") {
                let url = connect_url(&format!("{}:{}", peer.addr, peer.port), 0, None);
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
        }

        // Everything secondary lives here: the firewall fix, the manual
        // address/PIN, status, and recent connections.
        ui.add_space(8.0);
        egui::CollapsingHeader::new("More details").default_open(false).show(ui, |ui| {
            // A one-click firewall fix when inbound looks blocked — the usual
            // reason a phone on Wi-Fi can't reach the host. Lives in here now so
            // the connect step stays clean; still surfaced first inside the panel.
            if self.running && self.firewall_ok == Some(false) {
                if let Some(address) = self.address.clone() {
                    ui.colored_label(BRAND, "Windows Firewall may block phones on Wi-Fi.");
                    if ui.button("Allow through firewall").clicked() {
                        if let Some(port) =
                            address.rsplit_once(':').and_then(|(_, p)| p.parse::<u16>().ok())
                        {
                            crate::firewall::request_allow(port);
                            self.firewall_ok = Some(true); // optimistic; UAC adds it
                        }
                    }
                    ui.add_space(6.0);
                    ui.separator();
                }
            }

            if self.running {
                if let Some(address) = self.address.clone() {
                    ui.small("Already on the network? Type the address and PIN:");
                    ui.heading(&address);
                    // PIN masked until clicked (it's baked into the QR regardless).
                    let pin_text = if self.show_pin {
                        format!("PIN {:04}", self.pin)
                    } else {
                        "PIN ••••".to_owned()
                    };
                    let hint = if self.show_pin { "Click to hide" } else { "Click to reveal" };
                    let resp = ui
                        .add(
                            egui::Label::new(egui::RichText::new(pin_text).heading())
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(hint);
                    if resp.clicked() {
                        self.show_pin = !self.show_pin;
                    }
                }
            } else {
                ui.label("Not connected.");
                if ui.button("Start").clicked() {
                    self.start(ctx);
                }
            }

            ui.add_space(6.0);
            ui.label(format!("Status: {}", self.status.lock().unwrap()));

            let recent = self.recent.lock().unwrap().clone();
            if !recent.is_empty() {
                ui.add_space(6.0);
                ui.separator();
                ui.label("Recent connections");
                for conn in recent.iter().take(3) {
                    ui.horizontal(|ui| {
                        device_icon(ui, DeviceKind::from_tag(&conn.platform), 18.0);
                        ui.label(format!("{} · {}", platform_display(&conn.platform), conn.peer));
                    });
                }
            }
        });

        ui.add_space(8.0);
        egui::CollapsingHeader::new("Cast to a browser screen").default_open(false).show(ui, |ui| {
            ui.small(
                "Open opensource.unisim.co.uk/screens/receive on another screen, \
                 then enter the code it shows here:",
            );
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.cast_code).hint_text("CODE").desired_width(90.0));
                let can_cast = self.running && self.cast_code.trim().len() >= 4;
                if ui.add_enabled(can_cast, egui::Button::new("Cast")).clicked() {
                    // Bridge our own listener to the rendezvous room on a thread —
                    // dial_room blocks until the cast ends; status flows back via the Arc.
                    let code = self.cast_code.trim().to_uppercase();
                    let port = self
                        .address
                        .as_deref()
                        .and_then(|a| a.rsplit_once(':').map(|(_, p)| p.to_owned()))
                        .unwrap_or_else(|| "9000".to_owned());
                    let host_addr = format!("127.0.0.1:{port}");
                    let cast_status = self.cast_status.clone();
                    let ctx2 = ctx.clone();
                    *cast_status.lock().unwrap() = "Connecting to the browser…".to_owned();
                    thread::spawn(move || {
                        let res =
                            extender_web_bridge::dial_room(extender_web_bridge::DEFAULT_ROOM_URL, &code, &host_addr);
                        *cast_status.lock().unwrap() = match res {
                            Ok(()) => "Cast ended.".to_owned(),
                            Err(e) => format!("Cast failed: {e}"),
                        };
                        ctx2.request_repaint();
                    });
                }
            });
            if !self.running {
                ui.small("Start the host first, then cast.");
            }
            let status = self.cast_status.lock().unwrap().clone();
            if !status.is_empty() {
                ui.label(status);
            }
        });

        ui.add_space(8.0);
        egui::CollapsingHeader::new("Remote access (other networks)").default_open(false).show(ui, |ui| {
            ui.small(
                "Let someone on a different network reach this PC. Share the code below; \
                 they open opensource.unisim.co.uk/screens and enter it under \
                 “Remote (across networks)”.",
            );
            ui.add_space(4.0);
            ui.colored_label(BRAND, "⚠ Relayed through the cloud — slower than a local connection.");
            ui.add_space(4.0);

            if self.remote_active {
                ui.horizontal(|ui| {
                    ui.label("Your code:");
                    ui.label(egui::RichText::new(&self.remote_code).heading().strong());
                    if ui.small_button("Copy").clicked() {
                        ui.ctx().copy_text(self.remote_code.clone());
                    }
                });
            } else {
                let can_start = self.running;
                if ui.add_enabled(can_start, egui::Button::new("Enable remote access")).clicked() {
                    // Mint a code and dial the rendezvous as sender on a thread —
                    // dial_room blocks until the remote leaves; status via the Arc.
                    let code = gen_room_code();
                    self.remote_code = code.clone();
                    self.remote_active = true;
                    let port = self
                        .address
                        .as_deref()
                        .and_then(|a| a.rsplit_once(':').map(|(_, p)| p.to_owned()))
                        .unwrap_or_else(|| "9000".to_owned());
                    let host_addr = format!("127.0.0.1:{port}");
                    let remote_status = self.remote_status.clone();
                    let ctx2 = ctx.clone();
                    *remote_status.lock().unwrap() = "Waiting for the remote to connect…".to_owned();
                    thread::spawn(move || {
                        let res = extender_web_bridge::dial_room(
                            extender_web_bridge::DEFAULT_ROOM_URL,
                            &code,
                            &host_addr,
                        );
                        *remote_status.lock().unwrap() = match res {
                            Ok(()) => "Remote session ended.".to_owned(),
                            Err(e) => format!("Remote access failed: {e}"),
                        };
                        ctx2.request_repaint();
                    });
                }
                if !self.running {
                    ui.small("Start the host first, then enable remote access.");
                }
            }
            let status = self.remote_status.lock().unwrap().clone();
            if !status.is_empty() {
                ui.label(status);
            }
        });
    }

    /// The Universal navbar row: product logo + "Geek Apps" switcher, an Actions
    /// menu (host controls), then a Profile menu (theme/language) and the UNI·SIM
    /// mark with a changelog popup. Mirrors the suite's web navbar layout.
    fn show_navbar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if self.logo.is_none() {
            if let Some(img) = crate::qr::logo_image(48) {
                self.logo =
                    Some(ctx.load_texture("unisim-logo", img, egui::TextureOptions::LINEAR));
            }
        }
        if self.app_logo.is_none() {
            if let Some(img) = crate::qr::app_icon_image(64) {
                self.app_logo =
                    Some(ctx.load_texture("app-logo", img, egui::TextureOptions::LINEAR));
            }
        }
        if self.avatar.is_none() {
            self.avatar = Some(ctx.load_texture(
                "profile-avatar",
                crate::qr::profile_avatar_image(48),
                egui::TextureOptions::LINEAR,
            ));
        }
        let logo = self.logo.as_ref().map(eframe::egui::TextureHandle::id);
        let app_logo = self.app_logo.as_ref().map(eframe::egui::TextureHandle::id);
        let avatar = self.avatar.as_ref().map(eframe::egui::TextureHandle::id);
        let dark = ui.visuals().dark_mode;
        style_navbar(ui, dark);

        egui::menu::bar(ui, |ui| {
            // Left: product mark + "Universal Screens" → the Geek Apps switcher.
            if let Some(id) = app_logo {
                ui.add(egui::Image::from_texture(egui::load::SizedTexture::new(
                    id,
                    egui::vec2(24.0, 24.0),
                )));
            }
            ui.menu_button(egui::RichText::new("Universal Screens").strong().size(15.0), |ui| {
                ui.label(egui::RichText::new("Geeky").strong());
                ui.label(egui::RichText::new("UNI·SIM open-source").weak().small());
                ui.separator();
                let _ = ui.selectable_label(true, "Universal Screens — this app");
                for (name, path, blurb) in SIBLING_APPS {
                    ui.hyperlink_to(
                        egui::RichText::new(*name).strong(),
                        format!("{OPENSOURCE_ROOT}/{path}"),
                    );
                    ui.label(egui::RichText::new(*blurb).weak().small());
                }
                ui.separator();
                ui.hyperlink_to("Browse the suite ↗", OPENSOURCE_URL);
            });

            sep_dot(ui, dark);

            // Actions: host controls (moved out of the old "More options").
            ui.menu_button("Actions", |ui| {
                if self.running {
                    // Restart re-reads the network (new Wi-Fi / IP) and rebinds in
                    // one click — start() stops first. (Closing the window stops it.)
                    if ui.button("🔄  Restart host").clicked() {
                        self.start(ctx);
                        ui.close_menu();
                    }
                } else if ui.button("▶  Start hosting").clicked() {
                    self.start(ctx);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("🔁  Regenerate PIN").clicked() {
                    self.pin = gen_pin();
                    if self.running {
                        self.start(ctx); // restart so the new PIN is enforced
                    }
                    ui.close_menu();
                }
                ui.horizontal(|ui| {
                    ui.label("Port");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.port)
                            .hint_text("auto")
                            .desired_width(56.0),
                    );
                    if ui.button("Apply").clicked() {
                        self.start(ctx);
                    }
                });
                ui.small("Blank = first free port.");
                ui.separator();
                let has_recent = !self.recent.lock().unwrap().is_empty();
                if ui
                    .add_enabled(has_recent, egui::Button::new("🗑  Clear recent connections"))
                    .clicked()
                {
                    self.recent.lock().unwrap().clear();
                    ui.close_menu();
                }
                ui.separator();
                // Advanced — the same category the web apps gained on
                // 2026-08-29, in the same place in the same menu, holding the
                // same "About this app". The panel itself is shared (host-ui),
                // so the two hosts cannot drift apart on what the app claims.
                //
                // ⚠️ APP_VERSION is passed IN. It is env!("CARGO_PKG_VERSION"),
                // which resolves against the crate being compiled — read inside
                // host-ui it would report host-ui's version, not this host's.
                ui.menu_button("⚙  Advanced", |ui| {
                    ui.menu_button("ℹ  About this app", |ui| {
                        about_panel(ui, APP_VERSION);
                    });
                });
            });

            // Right side: Profile (settings) and the UNI·SIM changelog mark.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // UNI·SIM mark → "what's new" popup.
                let mark = if let Some(id) = logo {
                    ui.add(
                        egui::ImageButton::new(egui::load::SizedTexture::new(
                            id,
                            egui::vec2(20.0, 20.0),
                        ))
                        .frame(false),
                    )
                } else {
                    ui.button("UNI·SIM")
                }
                .on_hover_text("What's new");
                let popup_id = ui.make_persistent_id("changelog_popup");
                if mark.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(popup_id));
                }
                egui::popup::popup_below_widget(
                    ui,
                    popup_id,
                    &mark,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        ui.set_min_width(230.0);
                        ui.label(
                            egui::RichText::new(format!("Universal Screens v{APP_VERSION}")).strong(),
                        );
                        ui.separator();
                        for line in CHANGELOG {
                            ui.label(*line);
                        }
                        ui.separator();
                        // The list above is a build-time snapshot; this is the
                        // live feed the web apps' ChangelogMenu reads. Without
                        // it the menu silently goes stale between releases and
                        // nothing on screen admits that.
                        ui.hyperlink_to("See all changes ↗", CHANGELOG_URL);
                    },
                );

                sep_dot(ui, dark);

                // Security: a lock that opens an honest summary of what is and
                // isn't protected.
                let lock = ui.button("🔒").on_hover_text("Security");
                let lock_popup = ui.make_persistent_id("security_popup");
                if lock.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(lock_popup));
                }
                egui::popup::popup_below_widget(
                    ui,
                    lock_popup,
                    &lock,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        ui.set_max_width(320.0);
                        ui.label(egui::RichText::new("🔒  Security").strong().size(15.0));
                        ui.separator();
                        ui.label(egui::RichText::new("What's protected").strong());
                        ui.label("• A 4-digit pairing PIN is required to connect — scanning the QR fills it in automatically.");
                        ui.label("• The host only accepts connections while this window is open; close it to stop.");
                        // ⚠️ CORRECTED 2026-08-29. These two bullets said
                        // "Traffic is sent unencrypted over your local network
                        // (no TLS)" and "the PIN is a basic gate, not
                        // encryption". That was written 2026-06-17 and stopped
                        // being true on 2026-08-26, when the Noise tunnel
                        // landed (crates/transport) and made the PIN the
                        // pre-shared key. The copy understated the app rather
                        // than overstating it, which is why nothing caught it —
                        // but it flatly contradicted the new About panel two
                        // items along in the same menu.
                        ui.label("• The connection is encrypted end to end, with your PIN as the key — nobody else on the network can read the screen or the keystrokes.");
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Not fully locked down").strong());
                        ui.label("• The PIN is what the encryption is built on, so anyone who has it (or sees the QR) can connect and control this PC.");
                        ui.label("• An old client that predates the encryption can still connect in plaintext; the host logs a warning when one does.");
                        ui.label("• Wrong PINs aren't rate-limited or locked out, and there's no login or per-device approval.");
                        ui.label("• The host listens on all network interfaces on its port.");
                        ui.add_space(6.0);
                        ui.small("Tip: regenerate the PIN (Actions menu) after showing your screen to others.");
                    },
                );

                sep_dot(ui, dark);
                // The trigger is the avatar disc rather than the word "Profile":
                // the suite's web chrome puts UserProfile behind a circular
                // avatar, and this bar is meant to read as that same navbar.
                // menu_custom_button, not menu_button, because the trigger is a
                // frameless image — menu_button would draw a button box round it.
                let profile_btn = match avatar {
                    Some(id) => egui::Button::image(egui::Image::from_texture(
                        egui::load::SizedTexture::new(id, egui::vec2(24.0, 24.0)),
                    ))
                    .frame(false),
                    // Only reachable if the texture upload failed. A word is a
                    // worse trigger than a disc but an infinitely better one
                    // than nothing at all, which is what an Image with no
                    // texture would render.
                    None => egui::Button::new("Profile").frame(false),
                };
                egui::menu::menu_custom_button(ui, profile_btn, |ui| {
                    // Dark mode reflects the effective theme and pins it once toggled.
                    let mut dark = self.dark_mode.unwrap_or(ui.visuals().dark_mode);
                    if ui.checkbox(&mut dark, "🌙  Dark mode").changed() {
                        self.dark_mode = Some(dark);
                    }
                    if ui.button("Follow system theme").clicked() {
                        self.dark_mode = None;
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.menu_button("🌐  Language", |ui| {
                        let _ = ui.selectable_label(true, "English");
                        ui.add_enabled(false, egui::Button::new("More coming soon"));
                    });
                    ui.separator();
                    let mut dont = !self.auto_connect;
                    if ui.checkbox(&mut dont, "Don't connect automatically").changed() {
                        self.auto_connect = !dont;
                    }
                })
                .response
                // The disc carries no text, so without this the control is
                // unnameable — nothing on hover, nothing for assistive tech.
                .on_hover_text("Profile & settings");
            });
        });
    }
}

impl eframe::App for HostApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "auto_connect", &self.auto_connect);
        eframe::set_value(storage, "dark_mode", &self.dark_mode);
        eframe::set_value(storage, "pin_code", &self.pin);
        eframe::set_value(storage, "port", &self.port);
        eframe::set_value(storage, "recent", &*self.recent.lock().unwrap());
    }

    // Don't restore egui's own memory (it would carry a stale theme over our
    // pastel visuals); we still persist our own values via `save`.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    // Stop the background LAN discovery threads on a clean exit (the beacon is
    // already stopped when serving stops; the listener runs for the whole app).
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.listener_stop.store(true, Ordering::Relaxed);
        self.beacon_stop.store(true, Ordering::Relaxed);
        // Withdraw the DNS-SD advertisement so browsers drop us straight away
        // instead of waiting out the record TTL.
        if let Some(ad) = self.mdns_ad.take() {
            ad.shutdown();
        }
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Follow the OS theme by default; honour the user's override if set.
        ctx.set_theme(match self.dark_mode {
            Some(true) => egui::ThemePreference::Dark,
            Some(false) => egui::ThemePreference::Light,
            None => egui::ThemePreference::System,
        });

        // Colour the title bar to match the app (not the OS), when the theme changes.
        let dark = ctx.style().visuals.dark_mode;
        if let Some(hwnd) = window_hwnd(frame) {
            if self.caption_dark != Some(dark) {
                self.caption_dark = Some(dark);
                set_title_bar(hwnd, ctx.style().visuals.panel_fill, dark);
            }
            // Re-assert the transparent caption (small) icon every frame: winit/
            // eframe re-applies the window icon (which feeds the taskbar) after our
            // first frame, which would otherwise restore the icon in the title bar.
            strip_title_chrome(hwnd);
        }

        // The UNI·SIM brand strip ("light bar") is painted on a foreground layer
        // across the very top (see paint_brand_strip) — no panel, so no seam line.
        paint_brand_strip(ctx);

        // The Universal navbar — mirrors the @unisim/sdk web navbar so this host
        // and the opensource.unisim.co.uk/screens page feel like one product:
        // white bar, ~56px tall, with a slate bottom border.
        let border = if dark {
            egui::Color32::from_rgb(0x33, 0x3a, 0x46)
        } else {
            egui::Color32::from_rgb(0xe2, 0xe8, 0xf0) // slate-200
        };
        egui::TopBottomPanel::top("navbar")
            .exact_height(56.0)
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin { left: 16.0, right: 12.0, top: 6.0, bottom: 6.0 })
                    .stroke(egui::Stroke::NONE),
            )
            .show_separator_line(false)
            .show(ctx, |ui| {
                self.show_navbar(ctx, ui);
                // A 1px slate bottom border spanning the full width, like the web bar.
                let y = ui.max_rect().bottom() + 6.0;
                let screen = ctx.screen_rect();
                ui.painter().hline(screen.x_range(), y, egui::Stroke::new(1.0, border));
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                // This PC: a centred OS icon; click it to reveal the machine name.
                ui.vertical_centered(|ui| {
                    let resp = device_icon(ui, DeviceKind::Laptop, 28.0)
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text("Click to show this PC's name");
                    if resp.clicked() {
                        self.show_pc_info = !self.show_pc_info;
                    }
                    if self.show_pc_info {
                        ui.label(format!("This PC: Windows · {}", host_name()));
                    }
                });

                ui.vertical_centered(|ui| {
                    self.show_connect(ctx, ui);
                });
            });
        });

        // Draw the enlarged-QR overlay last so it sits above the whole window.
        self.show_qr_overlay(ctx);
    }
}





/// A bold "STEP N" eyebrow (brand orange) + a large, heavy title for the connect
/// flow. Sized up deliberately so the two steps read at a glance.
fn step_header(ui: &mut egui::Ui, step: &str, title: &str) {
    ui.add_space(8.0);
    ui.label(egui::RichText::new(step.to_uppercase()).color(BRAND).strong().size(15.0));
    ui.label(egui::RichText::new(title).strong().size(26.0));
    ui.add_space(6.0);
}

/// The bold "how to scan this step" sub-instruction, sized between the title and
/// the small body hint so it stands out (item: make the scan line a lot bolder).
fn scan_subheader(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).strong().size(15.0));
    ui.add_space(2.0);
}



/// The Win32 `HWND` behind the eframe window, if available.
fn window_hwnd(frame: &eframe::Frame) -> Option<HWND> {
    match frame.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut core::ffi::c_void)),
        _ => None,
    }
}

/// Recolour the window's title bar (DWM) to match the app theme: caption fill =
/// the app background, with the light/dark variant for the system buttons/text.
/// Best-effort (Windows 11); ignored on older Windows.
fn set_title_bar(hwnd: HWND, bg: egui::Color32, dark: bool) {
    let immersive: i32 = i32::from(dark);
    let caption = COLORREF(
        u32::from(bg.r()) | (u32::from(bg.g()) << 8) | (u32::from(bg.b()) << 16),
    );
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            std::ptr::addr_of!(immersive).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            std::ptr::addr_of!(caption).cast(),
            std::mem::size_of::<COLORREF>() as u32,
        );
        // Paint the caption *text* the same colour as the caption fill, so the
        // window title is invisible in the header but still set (the taskbar
        // button / hover shows the name).
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TEXT_COLOR,
            std::ptr::addr_of!(caption).cast(),
            std::mem::size_of::<COLORREF>() as u32,
        );
    }
}

/// Blank the *caption* (small) icon while leaving the big icon — which the
/// taskbar and Alt-Tab use — as the UNI·SIM logo set via `with_icon`. On Windows
/// 11's DWM caption, clearing the icon to null reveals the system's generic icon,
/// so we set an explicit 1×1 fully-transparent icon (created once) for ICON_SMALL.
/// Best-effort; quietly ignored if creation fails.
fn strip_title_chrome(hwnd: HWND) {
    static TRANSPARENT_ICON: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let icon = *TRANSPARENT_ICON.get_or_init(|| unsafe {
        // Monochrome 1×1: AND mask = 1 (transparent), XOR = 0. Rows are
        // DWORD-aligned, so one byte of data padded to 4.
        let and_mask = [0xFFu8; 4];
        let xor_mask = [0x00u8; 4];
        CreateIcon(None, 1, 1, 1, 1, and_mask.as_ptr(), xor_mask.as_ptr())
            .map(|h| h.0 as usize)
            .unwrap_or(0)
    });
    unsafe {
        // ICON_SMALL (0) → transparent; ICON_BIG keeps the taskbar logo.
        let _ = SendMessageW(hwnd, WM_SETICON, Some(WPARAM(0)), Some(LPARAM(icon as isize)));
    }
}



/// This machine's name (`COMPUTERNAME`), or a fallback.
fn host_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "this PC".to_owned())
}




/// The best LAN address for a phone to reach: prefer a DHCP-assigned private IPv4
/// (a real Wi-Fi/Ethernet adapter), which sidesteps VPN tunnels, WSL/Hyper-V
/// virtual adapters and APIPA link-local addresses. Falls back to the default-
/// route address. This matters when a VPN (e.g. ProtonVPN) owns the default
/// route: the route-based pick would otherwise hand out the unreachable VPN IP.
fn best_lan_ip() -> Option<String> {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let ps = "(Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue | \
        Where-Object { $_.PrefixOrigin -eq 'Dhcp' -and $_.IPAddress -notlike '169.254.*' } | \
        Select-Object -ExpandProperty IPAddress -First 1)";
    if let Ok(out) = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-Command", ps])
        .output()
    {
        let ip = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if ip.parse::<std::net::Ipv4Addr>().is_ok() {
            return Some(ip);
        }
    }
    primary_lan_ip()
}

/// The IP of the default-route interface (no packets are sent). `None` if down.
fn primary_lan_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

// ---- device icons (drawn with the egui painter — no asset/brand-logo deps) ----



/// Render the connection QR at `size`, clickable to pop it up full-window for
/// easier scanning (the phone can be held further back). Returns whether it was
/// clicked this frame. A free function (not a method) so it can be called while
/// `self.wifi` is borrowed in `show_connect`.
fn qr_clickable(ui: &mut egui::Ui, tex: egui::TextureId, size: f32) -> bool {
    ui.add(
        egui::Image::from_texture(egui::load::SizedTexture::new(tex, egui::vec2(size, size)))
            .rounding(14.0),
    )
    .interact(egui::Sense::click())
    .on_hover_cursor(egui::CursorIcon::PointingHand)
    .on_hover_text("Click to enlarge for scanning")
    .clicked()
}



