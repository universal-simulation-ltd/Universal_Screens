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
use extender_protocol::ClientPlatform;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIcon, SendMessageW, WM_SETICON};

use crate::{serve_loop, HostEvent};

/// Port scan starts here when no specific port is set.
const BASE_PORT: u16 = 9000;
/// UNI·SIM brand orange, for the top brand strip (the "light bar").
const BRAND: egui::Color32 = egui::Color32::from_rgb(0xe0, 0x55, 0x04);
/// How many recent connections to remember.
const RECENT_MAX: usize = 8;
/// The open-source suite page this app belongs to (navbar "Geek Apps" link).
const OPENSOURCE_URL: &str = "https://opensource.unisim.co.uk/screens";
/// This app's version, surfaced in the changelog popup.
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Short "what's new", newest first, shown under the UNI·SIM mark.
const CHANGELOG: &[&str] = &[
    "• Universal navbar: apps, actions & profile menus",
    "• Pairing PIN embedded in the connection QR",
    "• Title bar & light bar follow the app theme",
    "• Slide preview, deck scan & window picker",
];

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
    qr: Option<egui::TextureHandle>,
    /// The UNI·SIM mark, lazily uploaded for the navbar changelog icon + QR.
    logo: Option<egui::TextureHandle>,
    /// The Universal Screens app icon, lazily uploaded for the navbar product logo.
    app_logo: Option<egui::TextureHandle>,
    /// This PC's current Wi-Fi network (for the "join this network" step), if any.
    wifi: Option<crate::wifi::WifiInfo>,
    /// The Wi-Fi join QR, lazily built from `wifi`.
    wifi_qr: Option<egui::TextureHandle>,
    /// The one-scan combined QR (join Wi-Fi + connect), for the app's scanner.
    combined_qr: Option<egui::TextureHandle>,
    /// Reveal the Wi-Fi password (toggled by clicking the masked value).
    wifi_show_password: bool,
    /// Hide the Wi-Fi QR and show the details for manual entry instead.
    wifi_manual: bool,
    /// Wizard position: false = step 1 (Wi-Fi), true = step 2 (connect phone).
    show_step2: bool,
    /// Reveal the pairing PIN (toggled by clicking it); it's in the QR regardless.
    show_pin: bool,
    /// Whether an inbound firewall rule for the port exists (checked on start);
    /// None until the host first listens.
    firewall_ok: Option<bool>,
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
            qr: None,
            logo: None,
            app_logo: None,
            wifi: crate::wifi::current_wifi(),
            wifi_qr: None,
            combined_qr: None,
            wifi_show_password: false,
            wifi_manual: false,
            show_step2: false,
            show_pin: false,
            firewall_ok: None,
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
        self.qr = None;
        self.combined_qr = None;
        self.firewall_ok = Some(crate::firewall::rule_present(port));
        self.running = true;
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.running = false;
        self.address = None;
        self.qr = None;
        self.combined_qr = None;
    }

    /// Step 1 — get the phone onto the same network as this PC: a Wi-Fi "join"
    /// QR, plus the network name and a reveal-on-click password (or a manual
    /// fallback). Skipped with a note when this PC isn't on Wi-Fi.
    fn step1_wifi(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        step_header(ui, "Step 1", "Connect to the same Wi-Fi");
        let Some(wifi) = &self.wifi else {
            ui.label("This PC isn't on Wi-Fi.");
            ui.small("Connect your phone to the same network or router as this PC.");
            return;
        };
        if !self.wifi_manual {
            // Prefer the one-scan combined QR (the app joins this Wi-Fi *and*
            // connects). Falls back to a plain Wi-Fi-join QR if the host isn't
            // listening yet (so there's no address to embed).
            if self.running && self.address.is_some() {
                if self.combined_qr.is_none() {
                    if let Some(addr) = &self.address {
                        let payload = combined_payload(wifi, addr, self.pin);
                        if let Some(image) = crate::qr::branded_qr(&payload) {
                            self.combined_qr = Some(ctx.load_texture(
                                "combined_qr",
                                image,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                }
                if let Some(qr) = &self.combined_qr {
                    ui.add(
                        egui::Image::from_texture(egui::load::SizedTexture::new(
                            qr.id(),
                            egui::vec2(200.0, 200.0),
                        ))
                        .rounding(14.0),
                    );
                }
                ui.small("In the Universal Screens app, tap Scan — joins this Wi-Fi and connects in one step.");
            } else {
                if self.wifi_qr.is_none() {
                    if let Some(image) = crate::qr::branded_qr(&wifi.qr_payload()) {
                        self.wifi_qr =
                            Some(ctx.load_texture("wifi_qr", image, egui::TextureOptions::LINEAR));
                    }
                }
                if let Some(qr) = &self.wifi_qr {
                    ui.add(
                        egui::Image::from_texture(egui::load::SizedTexture::new(
                            qr.id(),
                            egui::vec2(190.0, 190.0),
                        ))
                        .rounding(14.0),
                    );
                }
                ui.small("Scan to join this PC's Wi-Fi.");
            }
            ui.add_space(4.0);
        }
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
            ui.label("Password: (open network)");
        }
        let mut manual = self.wifi_manual;
        if ui.checkbox(&mut manual, "Enter Wi-Fi details manually").changed() {
            self.wifi_manual = manual;
            if manual {
                self.wifi_show_password = true; // reveal so it can be typed in
            }
        }
    }

    /// Step 2 — point the phone's Universal Screens app at this host: a QR carrying
    /// the address + PIN, or the address/PIN to type.
    fn step2_phone(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        step_header(ui, "Step 2", "Connect your phone");
        ui.label("Open Universal Screens on your phone and scan this code.");
        ui.add_space(8.0);
        if self.running {
            if let Some(address) = self.address.clone() {
                if self.qr.is_none() {
                    // The QR carries the address + PIN so a scan auto-pairs.
                    let payload = format!("{address}?pin={:04}", self.pin);
                    if let Some(image) = crate::qr::branded_qr(&payload) {
                        self.qr =
                            Some(ctx.load_texture("qr", image, egui::TextureOptions::LINEAR));
                    }
                }
                if let Some(qr) = &self.qr {
                    ui.add(
                        egui::Image::from_texture(egui::load::SizedTexture::new(
                            qr.id(),
                            egui::vec2(190.0, 190.0),
                        ))
                        .rounding(14.0),
                    );
                }
                // Firewall: phones on Wi-Fi can't reach the host unless inbound is
                // allowed (loopback/USB works regardless). Offer a one-click fix.
                if self.firewall_ok == Some(false) {
                    ui.add_space(6.0);
                    ui.colored_label(BRAND, "Windows Firewall may block phones on Wi-Fi.");
                    if ui.button("Allow through firewall").clicked() {
                        if let Some(port) = address
                            .rsplit_once(':')
                            .and_then(|(_, p)| p.parse::<u16>().ok())
                        {
                            crate::firewall::request_allow(port);
                            self.firewall_ok = Some(true); // optimistic; UAC adds it
                        }
                    }
                }

                ui.add_space(6.0);
                ui.small("…or type the address and PIN:");
                ui.heading(&address);
                // PIN masked until clicked (it's baked into the QR regardless).
                let pin_text =
                    if self.show_pin { format!("PIN {:04}", self.pin) } else { "PIN ••••".to_owned() };
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
            ui.add_space(20.0);
            ui.label("Not connected.");
            if ui.button("Start").clicked() {
                self.start(ctx);
            }
        }
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
        let logo = self.logo.as_ref().map(eframe::egui::TextureHandle::id);
        let app_logo = self.app_logo.as_ref().map(eframe::egui::TextureHandle::id);
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
                ui.label(egui::RichText::new("Geek Apps").strong());
                ui.label(egui::RichText::new("UNI·SIM open-source").weak().small());
                ui.separator();
                let _ = ui.selectable_label(true, "Universal Screens — this app");
                ui.add_enabled(false, egui::Button::new("Universal QR  (soon)"));
                ui.add_enabled(false, egui::Button::new("More apps  (soon)"));
                ui.separator();
                ui.hyperlink_to("Browse the suite ↗", OPENSOURCE_URL);
            });

            sep_dot(ui, dark);

            // Actions: host controls (moved out of the old "More options").
            ui.menu_button("Actions", |ui| {
                if self.running {
                    if ui.button("⏹  Stop hosting").clicked() {
                        self.stop();
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
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Not fully locked down").strong());
                        ui.label("• Traffic is sent unencrypted over your local network (no TLS). Only use it on networks you trust.");
                        ui.label("• The PIN is a basic gate, not encryption: anyone on the same network who has the PIN (or sees the QR) can control this PC.");
                        ui.label("• Wrong PINs aren't rate-limited or locked out, and there's no login or per-device approval.");
                        ui.label("• The host listens on all network interfaces on its port.");
                        ui.add_space(6.0);
                        ui.small("Tip: regenerate the PIN (Actions menu) after showing your screen to others.");
                    },
                );

                sep_dot(ui, dark);
                ui.menu_button("Profile", |ui| {
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
                });
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
                    if self.show_step2 {
                        self.step2_phone(ctx, ui);
                        ui.add_space(8.0);
                        ui.label(format!("Status: {}", self.status.lock().unwrap()));
                        ui.add_space(8.0);
                        if ui.button("Back to Wi-Fi step").clicked() {
                            self.show_step2 = false;
                        }
                    } else {
                        self.step1_wifi(ctx, ui);
                        ui.add_space(14.0);
                        // Confirm Step 1 → reveal Step 2.
                        let next = ui.add(
                            egui::Button::new(
                                egui::RichText::new("✔  I'm connected — next")
                                    .color(egui::Color32::WHITE)
                                    .size(15.0),
                            )
                            .fill(BRAND)
                            .min_size(egui::vec2(220.0, 36.0))
                            .rounding(10.0),
                        );
                        if next.clicked() {
                            self.show_step2 = true;
                        }
                    }
                });

                // Recent connections.
                let recent = self.recent.lock().unwrap().clone();
                if !recent.is_empty() {
                    ui.add_space(10.0);
                    ui.separator();
                    ui.label("Recent connections");
                    for conn in &recent {
                        ui.horizontal(|ui| {
                            device_icon(ui, DeviceKind::from_tag(&conn.platform), 20.0);
                            ui.label(format!(
                                "{} · {}",
                                platform_display(&conn.platform),
                                conn.peer
                            ));
                        });
                    }
                }
            });
        });
    }
}

/// Paint the UNI·SIM brand strip: a horizontal gradient (transparent → orange →
/// transparent) with a gentle ~2.4s opacity pulse, matching the suite's
/// `UniversalBar`. Drawn on a foreground layer across the very top of the window
/// (not a panel), so there's no panel seam/line beneath it. Edges fade out so it
/// reads on any background.
fn paint_brand_strip(ctx: &egui::Context) {
    let screen = ctx.screen_rect();
    let rect = egui::Rect::from_min_max(screen.min, egui::pos2(screen.max.x, screen.min.y + 5.0));

    // Subtle pulse between 0.35 and 1.0 opacity.
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
        v(xl, y0, clear),  // 0
        v(xl, y1, clear),  // 1
        v(xc, y0, orange), // 2
        v(xc, y1, orange), // 3
        v(xr, y0, clear),  // 4
        v(xr, y1, clear),  // 5
    ]);
    mesh.indices.extend([0, 1, 2, 2, 1, 3, 2, 3, 4, 4, 3, 5]);

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("brand_strip"),
    ));
    painter.add(egui::Shape::mesh(mesh));
    ctx.request_repaint(); // keep the pulse animating
}

/// The one-scan combined payload: a custom-scheme URI the Universal Screens app
/// recognises, carrying the host address + PIN *and* the Wi-Fi credentials so a
/// single scan joins the network and connects. The phone's system camera can't
/// act on it — it's for the in-app scanner.
fn combined_payload(wifi: &crate::wifi::WifiInfo, host: &str, pin: u32) -> String {
    let (ip, port) = host.rsplit_once(':').unwrap_or((host, "9000"));
    let mut s = format!(
        "unisimscreens://connect?host={}&port={}&pin={:04}&ssid={}&auth={}",
        pe(ip),
        pe(port),
        pin,
        pe(&wifi.ssid),
        pe(&wifi.auth),
    );
    if let Some(p) = &wifi.password {
        s.push_str("&pass=");
        s.push_str(&pe(p));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pe_encodes_reserved_keeps_unreserved() {
        assert_eq!(pe("a b/c?d=e&f"), "a%20b%2Fc%3Fd%3De%26f");
        assert_eq!(pe("Safe-1._~"), "Safe-1._~");
    }

    #[test]
    fn combined_payload_carries_wifi_and_host() {
        let w = crate::wifi::WifiInfo {
            ssid: "My Net".to_owned(),
            password: Some("p@ss".to_owned()),
            auth: "WPA".to_owned(),
        };
        let p = combined_payload(&w, "10.0.0.5:9100", 1234);
        assert!(p.starts_with("unisimscreens://connect?"));
        assert!(p.contains("host=10.0.0.5"), "{p}");
        assert!(p.contains("port=9100"), "{p}");
        assert!(p.contains("pin=1234"), "{p}");
        assert!(p.contains("ssid=My%20Net"), "{p}");
        assert!(p.contains("pass=p%40ss"), "{p}");
        assert!(p.contains("auth=WPA"), "{p}");
    }

    #[test]
    fn combined_payload_open_network_omits_pass() {
        let w = crate::wifi::WifiInfo { ssid: "Cafe".to_owned(), password: None, auth: "nopass".to_owned() };
        let p = combined_payload(&w, "192.168.0.2:9000", 7);
        assert!(p.contains("pin=0007"), "{p}");
        assert!(!p.contains("pass="), "{p}");
    }
}

/// A "STEP N" eyebrow (brand orange) + title heading for the connect flow.
fn step_header(ui: &mut egui::Ui, step: &str, title: &str) {
    ui.add_space(8.0);
    ui.label(egui::RichText::new(step.to_uppercase()).color(BRAND).strong().size(12.0));
    ui.heading(title);
    ui.add_space(6.0);
}

/// Style the navbar like the web `UniversalNavBar`: flat text "links" (slate, not
/// boxed buttons) with a subtle rounded brand-tinted hover, and rounded dropdowns.
fn style_navbar(ui: &mut egui::Ui, dark: bool) {
    let link = if dark {
        egui::Color32::from_rgb(0xcb, 0xd5, 0xe1) // slate-300
    } else {
        egui::Color32::from_rgb(0x37, 0x41, 0x51) // slate-700
    };
    let hover = if dark {
        egui::Color32::from_rgb(0xf8, 0xfa, 0xfc)
    } else {
        egui::Color32::from_rgb(0x0f, 0x17, 0x2a) // slate-900
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

/// A muted "·" separator between navbar items, matching the web bar.
fn sep_dot(ui: &mut egui::Ui, dark: bool) {
    let c = if dark {
        egui::Color32::from_rgb(0x47, 0x55, 0x69)
    } else {
        egui::Color32::from_rgb(0xcb, 0xd5, 0xe1) // slate-300
    };
    ui.label(egui::RichText::new("·").color(c).size(16.0));
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

/// Mint a 4-digit pairing PIN (1000–9999) from the system clock. Not a crypto
/// secret — it only gates "someone who merely knows the IP" (the link isn't
/// encrypted; see the README security note).
fn gen_pin() -> u32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    1000 + (nanos % 9000)
}

/// This machine's name (`COMPUTERNAME`), or a fallback.
fn host_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "this PC".to_owned())
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

/// Bind the first free port at or after `start`, so the host "just works" even
/// when the default port is taken. Returns the bound listener and its port.
fn first_free_port(start: u16) -> Option<(TcpListener, u16)> {
    (start..start.saturating_add(50))
        .find_map(|port| TcpListener::bind(("0.0.0.0", port)).ok().map(|l| (l, port)))
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

#[derive(Clone, Copy)]
enum DeviceKind {
    Windows,
    Mac,
    Android,
    Ios,
    Laptop,
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

/// Draw a small monochrome device glyph inline in the current layout. Returns the
/// (clickable) response so callers can make it interactive.
fn device_icon(ui: &mut egui::Ui, kind: DeviceKind, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
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
                p.rect_filled(
                    r(cx * (cell + gap), cy * (cell + gap), cell, cell),
                    1.0,
                    color,
                );
            }
        }
        DeviceKind::Laptop => {
            p.rect_stroke(r(0.15, 0.14, 0.70, 0.50), 1.0, stroke);
            p.rect_filled(r(0.06, 0.66, 0.88, 0.10), 2.0, color); // base bar
        }
        DeviceKind::Mac => {
            p.rect_stroke(r(0.12, 0.10, 0.76, 0.52), 2.0, stroke); // monitor
            p.rect_filled(r(0.45, 0.62, 0.10, 0.12), 0.0, color); // neck
            p.rect_filled(r(0.30, 0.74, 0.40, 0.06), 1.0, color); // foot
        }
        DeviceKind::Android => {
            p.line_segment([at(0.33, 0.12), at(0.40, 0.27)], stroke); // antennae
            p.line_segment([at(0.67, 0.12), at(0.60, 0.27)], stroke);
            p.rect_filled(
                r(0.25, 0.27, 0.50, 0.46),
                egui::Rounding { nw: size * 0.22, ne: size * 0.22, sw: 0.0, se: 0.0 },
                color,
            );
            p.circle_filled(at(0.40, 0.41), size * 0.035, egui::Color32::WHITE); // eyes
            p.circle_filled(at(0.60, 0.41), size * 0.035, egui::Color32::WHITE);
        }
        DeviceKind::Ios => {
            p.rect_stroke(r(0.30, 0.10, 0.40, 0.80), size * 0.12, stroke); // phone
            p.line_segment([at(0.43, 0.82), at(0.57, 0.82)], stroke); // home bar
        }
        DeviceKind::Other => {
            p.rect_stroke(r(0.15, 0.18, 0.70, 0.52), 2.0, stroke); // generic monitor
            p.rect_filled(r(0.38, 0.74, 0.24, 0.06), 1.0, color);
        }
    }
    response
}
