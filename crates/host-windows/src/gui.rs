//! The host control window (GUI mode): shows the address + QR code to enter on
//! the phone, the listening / connected status, and a Start/Stop control. Runs the
//! same [`crate::serve_loop`] on a background thread.

use std::net::{TcpListener, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui;

use crate::{serve_loop, HostEvent};

/// Launch the host window. Detaches the inherited console first so a double-click
/// (or `cargo run` with no args) doesn't leave a stray terminal behind.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let _ = windows::Win32::System::Console::FreeConsole();
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 600.0])
            .with_title("Screen Extender — Host"),
        ..Default::default()
    };
    eframe::run_native(
        "Screen Extender Host",
        options,
        Box::new(|_cc| Ok(Box::<HostApp>::default())),
    )
    .map_err(|e| e.to_string().into())
}

struct HostApp {
    port: String,
    running: bool,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<String>>,
    /// The `ip:port` to enter on the phone, set once listening.
    address: Option<String>,
    qr: Option<egui::TextureHandle>,
}

impl Default for HostApp {
    fn default() -> Self {
        Self {
            port: "9000".to_owned(),
            running: false,
            stop: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new("Stopped".to_owned())),
            address: None,
            qr: None,
        }
    }
}

impl HostApp {
    fn start(&mut self, ctx: &egui::Context) {
        let Ok(port) = self.port.trim().parse::<u16>() else {
            *self.status.lock().unwrap() = "Invalid port".to_owned();
            return;
        };
        let listener = match TcpListener::bind(("0.0.0.0", port)) {
            Ok(l) => l,
            Err(e) => {
                *self.status.lock().unwrap() = format!("Could not bind port {port}: {e}");
                return;
            }
        };

        self.stop = Arc::new(AtomicBool::new(false));
        *self.status.lock().unwrap() = "Listening — waiting for a client".to_owned();
        let stop = self.stop.clone();
        let status = self.status.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            serve_loop(&listener, &stop, &|event| {
                *status.lock().unwrap() = match event {
                    HostEvent::Waiting => "Listening — waiting for a client".to_owned(),
                    HostEvent::Connected(peer) => format!("Connected: {peer}"),
                    HostEvent::Disconnected(peer) => format!("Disconnected: {peer} (listening)"),
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Screen Extender — Host");
            ui.label("Run this on the PC; connect to it from the phone app.");
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Port:");
                ui.add_enabled(
                    !self.running,
                    egui::TextEdit::singleline(&mut self.port).desired_width(70.0),
                );
                if self.running {
                    if ui.button("Stop").clicked() {
                        self.stop();
                    }
                } else if ui.button("Start").clicked() {
                    self.start(ctx);
                }
            });

            ui.add_space(8.0);
            ui.label(format!("Status: {}", self.status.lock().unwrap()));

            if self.running {
                if let Some(address) = self.address.clone() {
                    ui.add_space(12.0);
                    ui.label("On the phone, connect to:");
                    ui.heading(&address);
                    ui.label("or scan this QR in the app:");
                    ui.add_space(6.0);
                    if self.qr.is_none() {
                        if let Some(image) = qr_image(&address) {
                            self.qr = Some(ctx.load_texture("qr", image, egui::TextureOptions::NEAREST));
                        }
                    }
                    if let Some(qr) = &self.qr {
                        ui.image(egui::load::SizedTexture::new(qr.id(), egui::vec2(220.0, 220.0)));
                    }
                }
            }
        });
    }
}

/// The IP of the default-route interface (no packets are sent; `connect` on UDP
/// just selects the local interface). Falls back to `None` if unavailable.
fn primary_lan_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

/// Render `text` to a black-on-white QR `ColorImage` (with a quiet-zone border),
/// scaled up so it stays crisp. `None` if the text won't fit in a QR code.
fn qr_image(text: &str) -> Option<egui::ColorImage> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    let modules = code.width();
    let colors = code.to_colors();
    let quiet = 4usize;
    let scale = 6usize;
    let dim = (modules + quiet * 2) * scale;
    let mut pixels = vec![egui::Color32::WHITE; dim * dim];
    for y in 0..modules {
        for x in 0..modules {
            if colors[y * modules + x] == qrcode::Color::Dark {
                for dy in 0..scale {
                    for dx in 0..scale {
                        let px = (x + quiet) * scale + dx;
                        let py = (y + quiet) * scale + dy;
                        pixels[py * dim + px] = egui::Color32::BLACK;
                    }
                }
            }
        }
    }
    Some(egui::ColorImage { size: [dim, dim], pixels })
}
