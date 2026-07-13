//! The host control window (GUI mode). Mirrors the Windows host's layout and
//! UX so both platforms feel like one product: brand strip, Universal navbar,
//! two-step wizard (get the app → scan to connect), recent connections.
//!
//! macOS-specific changes vs. the Windows host:
//!   - No DWM title-bar recolouring (macOS uses standard window chrome).
//!   - No firewall management (macOS prompts automatically on first listen).
//!   - Wi-Fi password not extracted (keychain dialog deferred to a later version).

use std::net::{TcpListener, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui;
use extender_protocol::ClientPlatform;
use serde::{Deserialize, Serialize};

use crate::{serve_loop, HostEvent};

const BASE_PORT: u16 = 9000;
const BRAND: egui::Color32 = egui::Color32::from_rgb(0xe0, 0x55, 0x04);
const RECENT_MAX: usize = 8;
const OPENSOURCE_URL: &str = "https://opensource.unisim.co.uk/screens";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const CHANGELOG: &[&str] = &[
    "• One-step connect — any camera scans to connect",
    "• LAN discovery — nearby hosts appear automatically",
    "• macOS GUI host — same wizard as Windows",
    "• 4-digit pairing PIN in every connect QR",
    "• Universal navbar with Actions & Profile menus",
];

#[derive(Clone, Serialize, Deserialize)]
struct RecentConn {
    platform: String,
    peer: String,
    /// Optional friendly name the user gave this saved connection. Shown as the
    /// main label (with the device/IP underneath) when set. `#[serde(default)]`
    /// keeps recents saved before this field existed loadable.
    #[serde(default)]
    name: Option<String>,
}

struct HostApp {
    auto_connect: bool,
    dark_mode: Option<bool>,
    pin: u32,
    show_pc_info: bool,
    port: String,
    running: bool,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<String>>,
    recent: Arc<Mutex<Vec<RecentConn>>>,
    address: Option<String>,
    logo: Option<egui::TextureHandle>,
    app_logo: Option<egui::TextureHandle>,
    wifi: Option<crate::wifi::WifiInfo>,
    combined_qr: Option<egui::TextureHandle>,
    wifi_show_password: bool,
    show_pin: bool,
    /// LAN peers discovered via UDP multicast beacon.
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
    /// Virtual displays created for second-screen sessions — listed in the GUI so
    /// the user can rename / remove them. Shared with the serve loop.
    vdisplays: Arc<Mutex<crate::host::VDisplays>>,
    /// In-progress friendly-name edit in the Virtual displays panel.
    rename_draft: String,
    /// The display id whose inline rename editor is open (None = closed). The
    /// editor isn't shown by default — it opens when "Rename" is clicked.
    renaming_id: Option<u32>,
    /// The recent-connection peer (IP) whose inline rename editor is open.
    renaming_peer: Option<String>,
    /// "Cast to a browser": the code typed from a receiver tab, and the status of
    /// the dial-the-room bridge (shared with its background thread).
    cast_code: String,
    cast_status: Arc<Mutex<String>>,
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
        let mut pin: u32 =
            storage.and_then(|s| eframe::get_value(s, "pin_code")).unwrap_or(0);
        if pin == 0 {
            pin = gen_pin();
        }
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
            wifi: crate::wifi::current_wifi(),
            combined_qr: None,
            wifi_show_password: false,
            show_pin: false,
            discovered_peers,
            listener_stop,
            own_ip,
            beacon_stop: Arc::new(AtomicBool::new(true)), // starts in stopped state
            mdns_ad: None,
            vdisplays: Arc::new(Mutex::new(crate::host::VDisplays::default())),
            rename_draft: String::new(),
            renaming_id: None,
            renaming_peer: None,
            cast_code: String::new(),
            cast_status: Arc::new(Mutex::new(String::new())),
            qr_zoom: None,
            qr_zoom_armed: false,
        }
    }

    /// Render the connection QR at `size`, clickable to pop it up full-window for
    /// easier scanning (the phone can be held further back). TextureId is `Copy`,
    /// so callers pass the id and avoid borrowing `self.combined_qr` across this
    /// `&mut self` call.
    fn qr_image(&mut self, ui: &mut egui::Ui, tex: egui::TextureId, size: f32) {
        let resp = ui
            .add(
                egui::Image::from_texture(egui::load::SizedTexture::new(
                    tex,
                    egui::vec2(size, size),
                ))
                .rounding(14.0),
            )
            .interact(egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Click to enlarge for scanning");
        if resp.clicked() {
            self.qr_zoom = Some(tex);
            self.qr_zoom_armed = false;
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
                        *self.status.lock().unwrap() =
                            format!("Could not use port {p}: {e}");
                        None
                    }
                },
                Err(_) => {
                    *self.status.lock().unwrap() = "Invalid port".to_owned();
                    None
                }
            },
        };
        let Some((listener, port)) = bound else { return };

        self.stop = Arc::new(AtomicBool::new(false));
        *self.status.lock().unwrap() = "Waiting for your phone…".to_owned();
        let stop = self.stop.clone();
        let status = self.status.clone();
        let recent = self.recent.clone();
        let ctx = ctx.clone();
        let pin = self.pin;
        let vdisplays = self.vdisplays.clone();
        thread::spawn(move || {
            serve_loop(&listener, &stop, pin, &vdisplays, &|event| {
                match event {
                    HostEvent::Waiting => {
                        *status.lock().unwrap() = "Waiting for your phone…".to_owned();
                    }
                    HostEvent::Connected { peer, platform } => {
                        let ip =
                            peer.rsplit_once(':').map_or(peer.clone(), |(a, _)| a.to_owned());
                        let mut list = recent.lock().unwrap();
                        // Preserve any friendly name the user gave this peer when it
                        // reconnects (we re-insert it at the top).
                        let prior_name = list.iter().find(|c| c.peer == ip).and_then(|c| c.name.clone());
                        list.retain(|c| c.peer != ip);
                        list.insert(
                            0,
                            RecentConn {
                                platform: platform_tag(platform).to_owned(),
                                peer: ip,
                                name: prior_name,
                            },
                        );
                        list.truncate(RECENT_MAX);
                        *status.lock().unwrap() = format!("Connected: {peer}");
                    }
                    HostEvent::Disconnected(peer) => {
                        *status.lock().unwrap() =
                            format!("{peer} disconnected — waiting…");
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
        self.wifi = crate::wifi::current_wifi();
        self.combined_qr = None;
        self.running = true;

        // Tell the listener our own IP so it can ignore our own beacons.
        *self.own_ip.lock().unwrap() = Some(ip.clone());
        // Stop the previous beacon (if any) and start a fresh one.
        self.beacon_stop.store(true, Ordering::Relaxed);
        let beacon_stop = Arc::new(AtomicBool::new(false));
        self.beacon_stop = beacon_stop.clone();
        crate::discovery::start_beacon(crate::host_name(), port, beacon_stop);
        // And advertise over DNS-SD so the phone apps' host browsers (Android
        // NSD / iOS Bonjour) list this Mac under their own "Nearby".
        self.mdns_ad = crate::discovery::advertise_mdns(&crate::host_name(), port).ok();
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

    /// Single connect step: one QR that works for any camera (opens the site →
    /// deep-links into the app or shows the download page) and for the in-app
    /// scanner (connects directly). Wi-Fi details shown as text below the QR.
    fn show_connect(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Scan to connect").strong().size(26.0));
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Use your phone camera or the Universal Screens app")
                .strong()
                .size(15.0),
        );
        ui.add_space(10.0);

        if self.running && self.address.is_some() {
            if self.combined_qr.is_none() {
                if let Some(addr) = &self.address {
                    let url = connect_url(addr, self.pin, self.wifi.as_ref());
                    if let Some(image) = crate::qr::branded_qr_app(&url) {
                        self.combined_qr = Some(ctx.load_texture(
                            "combined_qr",
                            image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
            }
            if let Some(id) = self.combined_qr.as_ref().map(egui::TextureHandle::id) {
                self.qr_image(ui, id, 200.0);
            }
        } else if !self.running {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("▶  Start hosting")
                            .color(egui::Color32::WHITE)
                            .size(15.0),
                    )
                    .fill(BRAND)
                    .min_size(egui::vec2(180.0, 36.0))
                    .rounding(10.0),
                )
                .clicked()
            {
                self.start(ctx);
            }
        }

        if let Some(wifi) = &self.wifi {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(format!("Network: {}", wifi.ssid)).strong());
            if let Some(masked) = wifi.masked_password() {
                let shown = if self.wifi_show_password {
                    wifi.password.clone().unwrap_or_default()
                } else {
                    masked
                };
                let hint = if self.wifi_show_password { "Click to hide" } else { "Click to reveal" };
                let resp = ui
                    .add(egui::Label::new(format!("Password: {shown}")).sense(egui::Sense::click()))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(hint);
                if resp.clicked() {
                    self.wifi_show_password = !self.wifi_show_password;
                }
            } else {
                ui.label("Password: (tap to join / open network)");
            }
        }

        // Nearby — other Universal Screens hosts discovered on the LAN via UDP
        // multicast. Primary use case: Mac → Mac / Mac → PC (no camera to scan a
        // QR). Drawn as the portal-style orbit: this Mac in the centre, each
        // peer a node circling it — click a node to connect.
        let nearby = self.discovered_peers.lock().unwrap().clone();
        if !nearby.is_empty() {
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Nearby").strong());
            if let Some(peer) = nearby_orbit(ui, &nearby, "This Mac") {
                let url = connect_url(&format!("{}:{}", peer.addr, peer.port), 0, None);
                let _ = std::process::Command::new("open").arg(url).spawn();
            }
        }

        ui.add_space(8.0);
        egui::CollapsingHeader::new("More details").default_open(false).show(ui, |ui| {
            if self.running {
                if let Some(address) = self.address.clone() {
                    ui.small("Already on the network? Type the address and PIN:");
                    ui.heading(&address);
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
                ui.label(format!("Status: {}", self.status.lock().unwrap()));
            }

            ui.add_space(6.0);

            let recent = self.recent.lock().unwrap().clone();
            if !recent.is_empty() {
                ui.separator();
                ui.label("Recent connections");
                let mut rename_peer: Option<(String, String)> = None; // (peer, new name)
                for conn in recent.iter().take(3) {
                    // Main label is the friendly name (with device in brackets) when
                    // set, else the device type; the IP shows underneath.
                    let label = match &conn.name {
                        Some(n) if !n.trim().is_empty() => {
                            format!("{} ({})", n.trim(), platform_display(&conn.platform))
                        }
                        _ => platform_display(&conn.platform).to_string(),
                    };
                    ui.horizontal(|ui| {
                        device_icon(ui, DeviceKind::from_tag(&conn.platform), 18.0);
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(label.as_str()).strong());
                            ui.small(conn.peer.as_str());
                        });
                        // Rename opens an inline editor (closed by default), same as
                        // the virtual-displays panel.
                        if ui.button("Rename").clicked() {
                            if self.renaming_peer.as_deref() == Some(conn.peer.as_str()) {
                                self.renaming_peer = None;
                            } else {
                                self.renaming_peer = Some(conn.peer.clone());
                                self.renaming_id = None; // close any display editor
                                self.rename_draft = conn.name.clone().unwrap_or_default();
                            }
                        }
                    });
                    if self.renaming_peer.as_deref() == Some(conn.peer.as_str()) {
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut self.rename_draft);
                            if ui.button("Apply").clicked() {
                                rename_peer = Some((conn.peer.clone(), self.rename_draft.clone()));
                            }
                            if ui.button("Cancel").clicked() {
                                self.renaming_peer = None;
                            }
                        });
                        ui.small("Leave blank to reset to the device name.");
                    }
                }
                if let Some((peer, new_name)) = rename_peer {
                    let trimmed = new_name.trim();
                    let mut list = self.recent.lock().unwrap();
                    if let Some(c) = list.iter_mut().find(|c| c.peer == peer) {
                        c.name = if trimmed.is_empty() { None } else { Some(trimmed.to_owned()) };
                    }
                    self.renaming_peer = None;
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
    }

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
        let logo = self.logo.as_ref().map(eframe::egui::TextureHandle::id);
        let app_logo = self.app_logo.as_ref().map(eframe::egui::TextureHandle::id);
        let dark = ui.visuals().dark_mode;
        style_navbar(ui, dark);

        egui::menu::bar(ui, |ui| {
            if let Some(id) = app_logo {
                ui.add(egui::Image::from_texture(egui::load::SizedTexture::new(
                    id,
                    egui::vec2(24.0, 24.0),
                )));
            }
            ui.menu_button(
                egui::RichText::new("Universal Screens").strong().size(15.0),
                |ui| {
                    ui.label(egui::RichText::new("Geek Apps").strong());
                    ui.label(egui::RichText::new("UNI·SIM open-source").weak().small());
                    ui.separator();
                    let _ = ui.selectable_label(true, "Universal Screens — this app");
                    ui.add_enabled(false, egui::Button::new("Universal QR  (soon)"));
                    ui.add_enabled(false, egui::Button::new("More apps  (soon)"));
                    ui.separator();
                    ui.hyperlink_to("Browse the suite ↗", OPENSOURCE_URL);
                },
            );

            sep_dot(ui, dark);

            ui.menu_button("Actions", |ui| {
                if self.running {
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
                        self.start(ctx);
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
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                            egui::RichText::new(format!("Universal Screens v{APP_VERSION}"))
                                .strong(),
                        );
                        ui.separator();
                        for line in CHANGELOG {
                            ui.label(*line);
                        }
                    },
                );

                sep_dot(ui, dark);
                ui.menu_button("Profile", |ui| {
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
                });
            });
        });
    }

    /// Lists the virtual displays created for second-screen sessions, and lets the
    /// user give them a friendly name (overriding the per-device label) or remove
    /// them — straight from the Mac, as the backlog asked.
    fn show_virtual_displays(&mut self, ui: &mut egui::Ui) {
        // Snapshot under the lock so the UI never holds it while drawing.
        let (entries, friendly): (Vec<(u32, String, String, (u32, u32))>, Option<String>) = {
            let s = self.vdisplays.lock().unwrap();
            (
                s.entries
                    .iter()
                    .map(|d| (d.id, d.name.clone(), d.device_base.clone(), d.size))
                    .collect(),
                s.friendly_name.clone(),
            )
        };

        ui.add_space(12.0);
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("Virtual displays ({})", entries.len())).strong(),
        )
        .default_open(!entries.is_empty())
        .show(ui, |ui| {
            let mut remove_id: Option<u32> = None;
            let mut apply_name = false;

            if entries.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "None yet. Connect a phone in Second-screen mode to create one.",
                    )
                    .weak(),
                );
            }
            for (id, actual_name, device_base, (w, h)) in &entries {
                // The row's main name reflects the friendly override live, e.g.
                // "Screen (iPhone)". The actual macOS display name catches up on
                // the next reconnect (a CGVirtualDisplay can't be renamed live).
                let label = crate::host::resolved_name(friendly.as_deref(), device_base);
                let pending = label != *actual_name;
                ui.horizontal(|ui| {
                    ui.label("🖥");
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(label.as_str()).strong());
                        ui.small(format!("{w}×{h} · id {id}"));
                    });
                    // Rename opens an inline editor (closed by default); a second
                    // click closes it again.
                    if ui.button("Rename").clicked() {
                        if self.renaming_id == Some(*id) {
                            self.renaming_id = None;
                        } else {
                            self.renaming_id = Some(*id);
                            // Pre-fill with the friendly part only, so re-renaming
                            // never nests the "(device)" brackets.
                            self.rename_draft = friendly.clone().unwrap_or_default();
                        }
                    }
                    if ui.button("Remove").clicked() {
                        remove_id = Some(*id);
                        if self.renaming_id == Some(*id) {
                            self.renaming_id = None;
                        }
                    }
                });

                // Inline rename editor for this display — only when opened.
                if self.renaming_id == Some(*id) {
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.rename_draft);
                        if ui.button("Apply").clicked() {
                            apply_name = true;
                        }
                        if ui.button("Cancel").clicked() {
                            self.renaming_id = None;
                        }
                    });
                    ui.small("Leave blank to reset to the device name.");
                } else if pending {
                    ui.small(
                        egui::RichText::new("Reconnect the phone to apply the new name.").weak(),
                    );
                }
            }

            if let Some(id) = remove_id {
                crate::host::remove_display(&self.vdisplays, id);
            }
            if apply_name {
                crate::host::set_friendly_name(&self.vdisplays, Some(self.rename_draft.clone()));
                self.renaming_id = None;
            }
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

    fn persist_egui_memory(&self) -> bool {
        false
    }

    // Stop the background discovery threads and withdraw the DNS-SD
    // advertisement on a clean exit, so browsers drop us straight away instead
    // of waiting out the record TTL. (Mirrors the Windows host.)
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.listener_stop.store(true, Ordering::Relaxed);
        self.beacon_stop.store(true, Ordering::Relaxed);
        if let Some(ad) = self.mdns_ad.take() {
            ad.shutdown();
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_theme(match self.dark_mode {
            Some(true) => egui::ThemePreference::Dark,
            Some(false) => egui::ThemePreference::Light,
            None => egui::ThemePreference::System,
        });

        paint_brand_strip(ctx);

        let dark = ctx.style().visuals.dark_mode;
        let border = if dark {
            egui::Color32::from_rgb(0x33, 0x3a, 0x46)
        } else {
            egui::Color32::from_rgb(0xe2, 0xe8, 0xf0)
        };
        egui::TopBottomPanel::top("navbar")
            .exact_height(56.0)
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin {
                        left: 16.0,
                        right: 12.0,
                        top: 6.0,
                        bottom: 6.0,
                    })
                    .stroke(egui::Stroke::NONE),
            )
            .show_separator_line(false)
            .show(ctx, |ui| {
                self.show_navbar(ctx, ui);
                let y = ui.max_rect().bottom() + 6.0;
                let screen = ctx.screen_rect();
                ui.painter().hline(screen.x_range(), y, egui::Stroke::new(1.0, border));
            });

        // Footer: "With ❤ from UNISIM.co.uk" + security info button.
        egui::TopBottomPanel::bottom("footer")
            .exact_height(36.0)
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin { left: 16.0, right: 12.0, top: 6.0, bottom: 6.0 })
                    .stroke(egui::Stroke::NONE),
            )
            .show_separator_line(false)
            .show(ctx, |ui| {
                let y = ui.max_rect().top() - 6.0;
                let screen = ctx.screen_rect();
                ui.painter().hline(screen.x_range(), y, egui::Stroke::new(1.0, border));

                let muted = if dark {
                    egui::Color32::from_rgb(0x9a, 0x9a, 0xa6)
                } else {
                    egui::Color32::from_rgb(0x6b, 0x6b, 0x76)
                };
                ui.horizontal(|ui| {
                    let btn_w = 28.0;
                    ui.add_space(btn_w);
                    let remaining = ui.available_width() - btn_w;
                    ui.allocate_ui(egui::vec2(remaining, ui.available_height()), |ui| {
                        ui.centered_and_justified(|ui| {
                            ui.hyperlink_to(
                                egui::RichText::new("With \u{2764} from UNISIM.co.uk")
                                    .color(muted)
                                    .size(12.0),
                                "https://opensource.unisim.co.uk",
                            );
                        });
                    });
                    let lock = ui.button("\u{1f512}").on_hover_text("Security");
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
                            ui.label(
                                egui::RichText::new("\u{1f512}  Security").strong().size(15.0),
                            );
                            ui.separator();
                            ui.label(egui::RichText::new("What's protected").strong());
                            ui.label("• A 4-digit pairing PIN is required to connect — scanning the QR fills it in automatically.");
                            ui.label("• The host only accepts connections while this window is open; close it to stop.");
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new("Not fully locked down").strong());
                            ui.label("• Traffic is sent unencrypted over your local network (no TLS). Only use it on networks you trust.");
                            ui.label("• The PIN is a basic gate, not encryption: anyone on the same network who has the PIN (or sees the QR) can control this Mac.");
                            ui.label("• Wrong PINs aren't rate-limited or locked out, and there's no per-device approval.");
                            ui.label("• The host listens on all network interfaces on its port.");
                            ui.add_space(6.0);
                            ui.small("Tip: regenerate the PIN (Actions menu) after sharing your screen.");
                        },
                    );
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    let resp = device_icon(ui, DeviceKind::Mac, 28.0)
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text("Click to show this Mac's name");
                    if resp.clicked() {
                        self.show_pc_info = !self.show_pc_info;
                    }
                    if self.show_pc_info {
                        ui.label(format!("This Mac: macOS · {}", crate::host_name()));
                    }
                });

                ui.vertical_centered(|ui| {
                    self.show_connect(ctx, ui);
                });

                self.show_virtual_displays(ui);
            });
        });

        // Draw the enlarged-QR overlay last so it sits above the whole window.
        self.show_qr_overlay(ctx);
    }
}

/// Launch the GUI host window.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([440.0, 720.0])
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

fn paint_brand_strip(ctx: &egui::Context) {
    let screen = ctx.screen_rect();
    let rect =
        egui::Rect::from_min_max(screen.min, egui::pos2(screen.max.x, screen.min.y + 5.0));
    let t = ctx.input(|i| i.time);
    let pulse = 0.35 + 0.65 * (0.5 + 0.5 * (t * std::f64::consts::TAU / 2.4).sin());
    let alpha = (pulse * 255.0) as u8;
    let orange = egui::Color32::from_rgba_unmultiplied(BRAND.r(), BRAND.g(), BRAND.b(), alpha);
    let clear = egui::Color32::from_rgba_unmultiplied(BRAND.r(), BRAND.g(), BRAND.b(), 0);

    let (y0, y1) = (rect.top(), rect.bottom());
    let (xl, xc, xr) = (rect.left(), rect.center().x, rect.right());
    let v = |x: f32, y: f32, c: egui::Color32| egui::epaint::Vertex {
        pos: egui::pos2(x, y),
        uv: egui::epaint::WHITE_UV,
        color: c,
    };
    let mut mesh = egui::Mesh::default();
    mesh.vertices.extend([
        v(xl, y0, clear),
        v(xl, y1, clear),
        v(xc, y0, orange),
        v(xc, y1, orange),
        v(xr, y0, clear),
        v(xr, y1, clear),
    ]);
    mesh.indices.extend([0, 1, 2, 2, 1, 3, 2, 3, 4, 4, 3, 5]);

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("brand_strip"),
    ));
    painter.add(egui::Shape::mesh(mesh));
    ctx.request_repaint();
}

fn connect_url(host: &str, pin: u32, wifi: Option<&crate::wifi::WifiInfo>) -> String {
    let (ip, port) = host.rsplit_once(':').unwrap_or((host, "9000"));
    let mut s = format!(
        "https://opensource.unisim.co.uk/screens/connect?host={}&port={}&pin={:04}",
        pe(ip),
        pe(port),
        pin,
    );
    if let Some(wifi) = wifi {
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

fn gen_pin() -> u32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    1000 + (nanos % 9000)
}

fn first_free_port(start: u16) -> Option<(TcpListener, u16)> {
    (start..start.saturating_add(50))
        .find_map(|p| TcpListener::bind(("0.0.0.0", p)).ok().map(|l| (l, p)))
}

fn best_lan_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

fn style_navbar(ui: &mut egui::Ui, dark: bool) {
    let link = if dark {
        egui::Color32::from_rgb(0xcb, 0xd5, 0xe1)
    } else {
        egui::Color32::from_rgb(0x37, 0x41, 0x51)
    };
    let hover = if dark {
        egui::Color32::from_rgb(0xf8, 0xfa, 0xfc)
    } else {
        egui::Color32::from_rgb(0x0f, 0x17, 0x2a)
    };
    let tint = egui::Color32::from_rgba_unmultiplied(
        BRAND.r(),
        BRAND.g(),
        BRAND.b(),
        if dark { 46 } else { 26 },
    );
    let round = egui::Rounding::same(8.0);

    let s = ui.style_mut();
    s.spacing.button_padding = egui::vec2(10.0, 6.0);
    s.spacing.item_spacing.x = 6.0;
    s.visuals.menu_rounding = egui::Rounding::same(10.0);
    if let Some(font) = s.text_styles.get_mut(&egui::TextStyle::Button) {
        font.size = 14.0;
    }

    let w = &mut s.visuals.widgets;
    for v in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        v.bg_stroke = egui::Stroke::NONE;
        v.rounding = round;
        v.expansion = 0.0;
    }
    w.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    w.inactive.bg_fill = egui::Color32::TRANSPARENT;
    w.inactive.fg_stroke.color = link;
    for v in [&mut w.hovered, &mut w.active, &mut w.open] {
        v.weak_bg_fill = tint;
        v.bg_fill = tint;
        v.fg_stroke.color = hover;
    }
}

fn sep_dot(ui: &mut egui::Ui, dark: bool) {
    let c = if dark {
        egui::Color32::from_rgb(0x47, 0x55, 0x69)
    } else {
        egui::Color32::from_rgb(0xcb, 0xd5, 0xe1)
    };
    ui.label(egui::RichText::new("·").color(c).size(16.0));
}

fn platform_tag(p: ClientPlatform) -> &'static str {
    match p {
        ClientPlatform::Windows => "windows",
        ClientPlatform::Macos => "macos",
        ClientPlatform::Linux => "linux",
        ClientPlatform::Android => "android",
        ClientPlatform::Ios => "ios",
        ClientPlatform::Unknown => "unknown",
    }
}

fn platform_display(tag: &str) -> &str {
    match tag {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        "android" => "Android",
        "ios" => "iOS",
        _ => "Unknown device",
    }
}

#[derive(Clone, Copy)]
enum DeviceKind {
    Windows,
    Mac,
    Android,
    Ios,
    Other,
}

impl DeviceKind {
    fn from_tag(tag: &str) -> Self {
        match tag {
            "windows" => Self::Windows,
            "macos" => Self::Mac,
            "android" => Self::Android,
            "ios" => Self::Ios,
            _ => Self::Other,
        }
    }
}

/// Draw the "Nearby" hosts as an orbit: this Mac at the centre with a soft glow
/// + dashed ring, each discovered peer a node circling it (portal-style). The
/// nodes rotate slowly; hovering the area pauses them so a node is easy to
/// click. `centre_label` names the local machine ("This PC" / "This Mac").
/// Returns the peer whose node was clicked this frame, if any. Mirrors the
/// Windows host's `nearby_orbit`.
fn nearby_orbit(
    ui: &mut egui::Ui,
    peers: &[crate::discovery::DiscoveredPeer],
    centre_label: &str,
) -> Option<crate::discovery::DiscoveredPeer> {
    let width = ui.available_width();
    let height = 210.0_f32;
    let (rect, _resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let centre = rect.center();
    let radius = (height * 0.34).min(width * 0.3);

    let dark = ui.visuals().dark_mode;
    let ink = if dark { egui::Color32::from_gray(220) } else { egui::Color32::from_gray(40) };
    let muted = if dark { egui::Color32::from_gray(130) } else { egui::Color32::from_gray(140) };
    let card = ui.visuals().extreme_bg_color;

    // Dashed orbit ring.
    let ring_segments = 44;
    for i in 0..ring_segments {
        if i % 2 != 0 {
            continue; // gaps make the dashes
        }
        let a0 = std::f32::consts::TAU * (i as f32) / (ring_segments as f32);
        let a1 = std::f32::consts::TAU * (i as f32 + 1.0) / (ring_segments as f32);
        painter.line_segment(
            [
                centre + radius * egui::vec2(a0.cos(), a0.sin()),
                centre + radius * egui::vec2(a1.cos(), a1.sin()),
            ],
            egui::Stroke::new(1.2, muted.gamma_multiply(0.5)),
        );
    }

    // Pulsing glow behind the centre.
    let t = ui.input(|i| i.time) as f32;
    let pulse = 0.5 + 0.5 * (t * 1.6).sin();
    let glow = egui::Color32::from_rgba_unmultiplied(BRAND.r(), BRAND.g(), BRAND.b(), (26.0 + 20.0 * pulse) as u8);
    painter.circle_filled(centre, 34.0 + 5.0 * pulse, glow);

    // Centre node: this machine.
    painter.circle_filled(centre, 26.0, card);
    painter.circle_stroke(centre, 26.0, egui::Stroke::new(1.5, BRAND));
    painter.text(centre - egui::vec2(0.0, 4.0), egui::Align2::CENTER_CENTER, "🖥", egui::FontId::proportional(20.0), ink);
    painter.text(centre + egui::vec2(0.0, 15.0), egui::Align2::CENTER_CENTER, centre_label, egui::FontId::proportional(9.0), muted);

    // Orbiting peer nodes. A slow global rotation, evenly spread; pause on hover
    // so a moving node stays clickable.
    let hovered_area = ui.rect_contains_pointer(rect);
    let spin = if hovered_area { 0.0 } else { t * 0.35 }; // radians
    let mut clicked = None;
    let node_r = 22.0;

    for (i, peer) in peers.iter().enumerate() {
        let angle = spin + std::f32::consts::TAU * (i as f32) / (peers.len() as f32) - std::f32::consts::FRAC_PI_2;
        let pos = centre + radius * egui::vec2(angle.cos(), angle.sin());
        let node_rect = egui::Rect::from_center_size(pos, egui::vec2(node_r * 2.0, node_r * 2.0));
        let id = ui.id().with(("orbit_peer", i));
        let resp = ui.interact(node_rect, id, egui::Sense::click());
        let hot = resp.hovered();
        if hot {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        painter.circle_filled(pos, node_r, card);
        painter.circle_stroke(pos, node_r, egui::Stroke::new(if hot { 2.0 } else { 1.2 }, if hot { BRAND } else { muted }));
        painter.text(pos - egui::vec2(0.0, 3.0), egui::Align2::CENTER_CENTER, "📡", egui::FontId::proportional(16.0), ink);

        // Label pill under the node — the name, plus the address on hover.
        let name = truncate_label(&peer.name, 16);
        painter.text(
            pos + egui::vec2(0.0, node_r + 9.0),
            egui::Align2::CENTER_CENTER,
            &name,
            egui::FontId::proportional(11.0),
            ink,
        );
        if hot {
            painter.text(
                pos + egui::vec2(0.0, node_r + 22.0),
                egui::Align2::CENTER_CENTER,
                format!("{}:{}  ·  click to connect", peer.addr, peer.port),
                egui::FontId::proportional(9.5),
                muted,
            );
            resp.clone().on_hover_text(format!("Connect to {} ({}:{})", peer.name, peer.addr, peer.port));
        }
        if resp.clicked() {
            clicked = Some(peer.clone());
        }
    }

    // Keep the animation going while nothing else is repainting.
    if !hovered_area {
        ui.ctx().request_repaint();
    }
    clicked
}

/// Truncate a label to `max` chars with an ellipsis, so a long machine name
/// doesn't overrun an orbit node.
fn truncate_label(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn device_icon(ui: &mut egui::Ui, kind: DeviceKind, size: f32) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    let p = ui.painter();
    let color = egui::Color32::from_rgb(55, 55, 70);
    let stroke = egui::Stroke::new((size * 0.07).max(1.2), color);
    let at = |fx: f32, fy: f32| rect.min + egui::vec2(fx * size, fy * size);
    let r = |fx: f32, fy: f32, fw: f32, fh: f32| {
        egui::Rect::from_min_size(at(fx, fy), egui::vec2(fw * size, fh * size))
    };

    match kind {
        DeviceKind::Windows => {
            let gap = 0.10;
            let cell = (1.0 - gap) / 2.0;
            for (cx, cy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                p.rect_filled(r(cx * (cell + gap), cy * (cell + gap), cell, cell), 1.0, color);
            }
        }
        DeviceKind::Mac => {
            p.rect_stroke(r(0.12, 0.10, 0.76, 0.52), 2.0, stroke);
            p.rect_filled(r(0.45, 0.62, 0.10, 0.12), 0.0, color);
            p.rect_filled(r(0.30, 0.74, 0.40, 0.06), 1.0, color);
        }
        DeviceKind::Android => {
            p.line_segment([at(0.33, 0.12), at(0.40, 0.27)], stroke);
            p.line_segment([at(0.67, 0.12), at(0.60, 0.27)], stroke);
            p.rect_filled(
                r(0.25, 0.27, 0.50, 0.46),
                egui::Rounding { nw: size * 0.22, ne: size * 0.22, sw: 0.0, se: 0.0 },
                color,
            );
            p.circle_filled(at(0.40, 0.41), size * 0.035, egui::Color32::WHITE);
            p.circle_filled(at(0.60, 0.41), size * 0.035, egui::Color32::WHITE);
        }
        DeviceKind::Ios => {
            p.rect_stroke(r(0.30, 0.10, 0.40, 0.80), size * 0.12, stroke);
            p.line_segment([at(0.43, 0.82), at(0.57, 0.82)], stroke);
        }
        DeviceKind::Other => {
            p.rect_stroke(r(0.15, 0.18, 0.70, 0.52), 2.0, stroke);
            p.rect_filled(r(0.38, 0.74, 0.24, 0.06), 1.0, color);
        }
    }
    response
}
