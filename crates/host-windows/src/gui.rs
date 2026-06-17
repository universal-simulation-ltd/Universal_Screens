//! The host control window (GUI mode). Aimed at a non-technical user: on launch
//! it starts listening on the first free port and shows the address + branded QR
//! to use on the phone. Advanced bits (port, manual start/stop, auto-start
//! preference) live under a collapsed "More options" section.

use std::net::{TcpListener, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui;

use crate::{serve_loop, HostEvent};

/// Port scan starts here when no specific port is set.
const BASE_PORT: u16 = 9000;

/// Launch the host window. Detaches the inherited console so a double-click (or
/// `cargo run` with no args) doesn't leave a stray terminal behind.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let _ = windows::Win32::System::Console::FreeConsole();
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 620.0])
            .with_title("Screen Extender — Host"),
        ..Default::default()
    };
    eframe::run_native(
        "Screen Extender Host",
        options,
        Box::new(|cc| {
            // Light theme on a pastel-orange background (UNI·SIM brand tint).
            let pastel = egui::Color32::from_rgb(255, 235, 214);
            let mut visuals = egui::Visuals::light();
            visuals.panel_fill = pastel;
            visuals.window_fill = pastel;
            cc.egui_ctx.set_visuals(visuals);

            let mut app = HostApp::new(cc);
            // Connect automatically the first time (and every launch until the user
            // opts out under More options).
            if app.auto_connect {
                app.start(&cc.egui_ctx);
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| e.to_string().into())
}

struct HostApp {
    /// Start listening automatically on launch (persisted).
    auto_connect: bool,
    /// Advanced port override (persisted); empty = pick the first free port.
    port: String,
    running: bool,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<String>>,
    /// The `ip:port` to use on the phone, set once listening.
    address: Option<String>,
    qr: Option<egui::TextureHandle>,
}

impl HostApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let storage = cc.storage;
        Self {
            auto_connect: storage
                .and_then(|s| eframe::get_value(s, "auto_connect"))
                .unwrap_or(true),
            port: storage.and_then(|s| eframe::get_value(s, "port")).unwrap_or_default(),
            running: false,
            stop: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new("Not started".to_owned())),
            address: None,
            qr: None,
        }
    }

    /// Bind a port (the override if set, else the first free one from 9000) and
    /// start serving on a background thread.
    fn start(&mut self, ctx: &egui::Context) {
        self.stop(); // tear down any previous run first

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
        let ctx = ctx.clone();
        thread::spawn(move || {
            serve_loop(&listener, &stop, &|event| {
                *status.lock().unwrap() = match event {
                    HostEvent::Waiting => "Waiting for your phone…".to_owned(),
                    HostEvent::Connected(peer) => format!("Connected to {peer}"),
                    HostEvent::Disconnected(peer) => format!("{peer} disconnected — waiting…"),
                    HostEvent::Error(msg) => msg,
                };
                ctx.request_repaint();
            });
            *status.lock().unwrap() = "Stopped".to_owned();
            ctx.request_repaint();
        });

        let ip = primary_lan_ip().unwrap_or_else(|| "127.0.0.1".to_owned());
        self.address = Some(format!("{ip}:{port}"));
        self.qr = None; // rebuilt lazily for the new address
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
        eframe::set_value(storage, "port", &self.port);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.heading("Connect your phone");
                ui.label("Open Screen Extender on your phone and scan this code.");
                ui.add_space(12.0);

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
                                    egui::vec2(240.0, 240.0),
                                ))
                                .rounding(16.0),
                            );
                        }
                        ui.add_space(8.0);
                        ui.label("…or type this address:");
                        ui.heading(&address);
                    }
                } else {
                    ui.add_space(40.0);
                    ui.label("Not connected.");
                    if ui.button("Start").clicked() {
                        self.start(ctx);
                    }
                }

                ui.add_space(10.0);
                ui.label(format!("Status: {}", self.status.lock().unwrap()));
            });

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
                });
        });
    }
}

/// Bind the first free port at or after `start` (scanning a small range), so the
/// host "just works" even when the default port is taken. Returns the bound
/// listener and its port.
fn first_free_port(start: u16) -> Option<(TcpListener, u16)> {
    (start..start.saturating_add(50))
        .find_map(|port| TcpListener::bind(("0.0.0.0", port)).ok().map(|l| (l, port)))
}

/// The IP of the default-route interface (no packets are sent; `connect` on UDP
/// just selects the local interface). `None` if unavailable.
fn primary_lan_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}
