//! The host control window (GUI mode). Aimed at a non-technical user: on launch
//! it starts listening on the first free port and shows the address + branded QR
//! to use on the phone. It also shows this PC's identity and a list of recent
//! connections, each tagged with the device's platform icon. Advanced bits (port,
//! manual start/stop, auto-start preference) live under a collapsed "More options".

use std::net::{TcpListener, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui;
use extender_protocol::ClientPlatform;
use serde::{Deserialize, Serialize};

use crate::{serve_loop, HostEvent};

/// Port scan starts here when no specific port is set.
const BASE_PORT: u16 = 9000;
/// UNI·SIM brand orange, for the top brand strip (the "light bar").
const BRAND: egui::Color32 = egui::Color32::from_rgb(0xe0, 0x55, 0x04);
/// How many recent connections to remember.
const RECENT_MAX: usize = 8;

/// Launch the host window. Detaches the inherited console so a double-click (or
/// `cargo run` with no args) doesn't leave a stray terminal behind.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let _ = windows::Win32::System::Console::FreeConsole();
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([440.0, 720.0])
            .with_title("Screen Extender — Host"),
        ..Default::default()
    };
    eframe::run_native(
        "Screen Extender Host",
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
    port: String,
    running: bool,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<String>>,
    recent: Arc<Mutex<Vec<RecentConn>>>,
    address: Option<String>,
    qr: Option<egui::TextureHandle>,
}

impl HostApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let storage = cc.storage;
        let recent: Vec<RecentConn> =
            storage.and_then(|s| eframe::get_value(s, "recent")).unwrap_or_default();
        Self {
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
        thread::spawn(move || {
            serve_loop(&listener, &stop, &|event| {
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

        let ip = primary_lan_ip().unwrap_or_else(|| "127.0.0.1".to_owned());
        self.address = Some(format!("{ip}:{port}"));
        self.qr = None;
        self.running = true;
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.running = false;
        self.address = None;
        self.qr = None;
    }
}

impl eframe::App for HostApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "auto_connect", &self.auto_connect);
        eframe::set_value(storage, "dark_mode", &self.dark_mode);
        eframe::set_value(storage, "port", &self.port);
        eframe::set_value(storage, "recent", &*self.recent.lock().unwrap());
    }

    // Don't restore egui's own memory (it would carry a stale theme over our
    // pastel visuals); we still persist our own values via `save`.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Follow the OS theme by default; honour the user's override if set.
        ctx.set_theme(match self.dark_mode {
            Some(true) => egui::ThemePreference::Dark,
            Some(false) => egui::ThemePreference::Light,
            None => egui::ThemePreference::System,
        });

        // The UNI·SIM brand strip — a thin gradient "light bar" across the top.
        // Fill it with the theme background so it tracks light/dark mode.
        let strip_bg = ctx.style().visuals.panel_fill;
        egui::TopBottomPanel::top("brand_strip")
            .exact_height(5.0)
            .frame(egui::Frame::none().fill(strip_bg))
            .show_separator_line(false)
            .show(ctx, |ui| paint_brand_strip(ui));

        egui::CentralPanel::default().show(ctx, |ui| {
            // This PC's identity.
            ui.horizontal(|ui| {
                device_icon(ui, DeviceKind::Laptop, 22.0);
                ui.label(format!("This PC: Windows · {}", host_name()));
            });
            ui.separator();

            ui.vertical_centered(|ui| {
                ui.add_space(4.0);
                ui.heading("Connect your phone");
                ui.label("Open Screen Extender on your phone and scan this code.");
                ui.add_space(10.0);

                if self.running {
                    if let Some(address) = self.address.clone() {
                        if self.qr.is_none() {
                            if let Some(image) = crate::qr::branded_qr(&address) {
                                self.qr =
                                    Some(ctx.load_texture("qr", image, egui::TextureOptions::LINEAR));
                            }
                        }
                        if let Some(qr) = &self.qr {
                            ui.add(
                                egui::Image::from_texture(egui::load::SizedTexture::new(
                                    qr.id(),
                                    egui::vec2(232.0, 232.0),
                                ))
                                .rounding(16.0),
                            );
                        }
                        ui.add_space(6.0);
                        ui.label("…or type this address:");
                        ui.heading(&address);
                    }
                } else {
                    ui.add_space(30.0);
                    ui.label("Not connected.");
                    if ui.button("Start").clicked() {
                        self.start(ctx);
                    }
                }
                ui.add_space(8.0);
                ui.label(format!("Status: {}", self.status.lock().unwrap()));
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
                        ui.label(format!("{} · {}", platform_display(&conn.platform), conn.peer));
                    });
                }
            }

            ui.add_space(12.0);
            egui::CollapsingHeader::new("More options")
                .default_open(false)
                .show(ui, |ui| {
                    let mut dont_auto = !self.auto_connect;
                    if ui
                        .checkbox(&mut dont_auto, "Don't connect automatically on launch")
                        .changed()
                    {
                        self.auto_connect = !dont_auto;
                    }

                    // Dark mode: defaults to following the OS; the checkbox shows
                    // the effective state and pins it once toggled.
                    let mut dark = self.dark_mode.unwrap_or(ui.visuals().dark_mode);
                    if ui.checkbox(&mut dark, "Dark mode").changed() {
                        self.dark_mode = Some(dark);
                    }
                    ui.horizontal(|ui| {
                        ui.label("Port:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.port)
                                .hint_text("auto")
                                .desired_width(70.0),
                        );
                        if ui.button("Apply").clicked() {
                            self.start(ctx);
                        }
                    });
                    ui.small("Leave blank to use the first free port automatically.");
                    ui.add_space(4.0);
                    if self.running {
                        if ui.button("Stop").clicked() {
                            self.stop();
                        }
                    } else if ui.button("Start").clicked() {
                        self.start(ctx);
                    }
                    if !self.recent.lock().unwrap().is_empty()
                        && ui.button("Clear recent connections").clicked()
                    {
                        self.recent.lock().unwrap().clear();
                    }
                });
        });
    }
}

/// Paint the UNI·SIM brand strip: a horizontal gradient (transparent → orange →
/// transparent) with a gentle ~2.4s opacity pulse, matching the suite's
/// `UniversalBar`. Edges fade out so it reads on any background.
fn paint_brand_strip(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    // Subtle pulse between 0.35 and 1.0 opacity.
    let t = ui.input(|i| i.time);
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
    ui.painter().add(egui::Shape::mesh(mesh));

    ui.ctx().request_repaint(); // keep the pulse animating
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

/// Draw a small monochrome device glyph inline in the current layout.
fn device_icon(ui: &mut egui::Ui, kind: DeviceKind, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
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
}
