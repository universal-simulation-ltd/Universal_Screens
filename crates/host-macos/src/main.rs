//! macOS host: GUI window (no-arg) or headless TCP server (with a bind address).
//!
//! Run: cargo run -p extender-host-macos               (GUI, default 0.0.0.0:9000)
//!      cargo run -p extender-host-macos -- 0.0.0.0:9000  (headless)
//! Requires Screen Recording + Accessibility permissions on first run.

mod discovery;
mod gui;
mod host;
mod qr;
mod wifi;

use std::io;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use extender_protocol::{self as protocol, CaptureMode, ClientHello, ClientPlatform, Message};

const MAX_DIMENSION: u32 = 16384;

/// Lifecycle events from the accept loop — consumed by the GUI or the CLI logger.
pub(crate) enum HostEvent {
    Waiting,
    Connected { peer: String, platform: ClientPlatform },
    Disconnected(String),
    Error(String),
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1) {
        Some(addr) if addr != "--gui" => run_cli(&addr),
        _ => gui::run(),
    }
}

fn run_cli(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(addr)?;
    println!(
        "extender-host-macos listening on {addr} (protocol v{})",
        protocol::PROTOCOL_VERSION
    );
    let stop = AtomicBool::new(false);
    let vdisplays = Arc::new(Mutex::new(host::VDisplays::default()));
    serve_loop(&listener, &stop, 0, &vdisplays, &|event| match event {
        HostEvent::Waiting => println!("waiting for a client to connect..."),
        HostEvent::Connected { peer, platform } => {
            println!("client connected: {peer} ({platform:?})");
        }
        HostEvent::Disconnected(peer) => println!("client {peer} disconnected"),
        HostEvent::Error(msg) => eprintln!("{msg}"),
    });
    Ok(())
}

/// Accept and serve clients until `stop` is set. Non-blocking so the stop flag
/// is checked promptly between connections. A virtual display, once created, is
/// kept alive across reconnects and recreated only when the client needs a
/// different size.
pub(crate) fn serve_loop(
    listener: &TcpListener,
    stop: &AtomicBool,
    expected_pin: u32,
    vdisplays: &Arc<Mutex<host::VDisplays>>,
    on_event: &(dyn Fn(HostEvent) + Sync),
) {
    let _ = listener.set_nonblocking(true);
    on_event(HostEvent::Waiting);

    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((mut stream, peer_addr)) => {
                let _ = stream.set_nonblocking(false);
                let peer = peer_addr.to_string();
                if let Some((platform, mode, w, h, name)) =
                    read_hello(&mut stream, &peer, expected_pin)
                {
                    // Identify this host so the client can label/icon the connection.
                    if let Ok(mut writer) = stream.try_clone() {
                        let _ = protocol::write_framed(
                            &mut writer,
                            &Message::HostInfo {
                                os: "macos".into(),
                                name: host_name(),
                            },
                        );
                    }
                    on_event(HostEvent::Connected { peer: peer.clone(), platform });
                    let result = match mode {
                        CaptureMode::ControlOnly => host::serve_control_only(stream),
                        _ => host::serve_session(stream, mode, w, h, &name, vdisplays),
                    };
                    if let Err(e) = result {
                        on_event(HostEvent::Error(format!("session with {peer} ended: {e}")));
                    }
                    on_event(HostEvent::Disconnected(peer));
                }
                on_event(HostEvent::Waiting);
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => on_event(HostEvent::Error(format!("accept failed: {e}"))),
        }
    }
}

fn read_hello(
    stream: &mut TcpStream,
    peer: &str,
    expected_pin: u32,
) -> Option<(ClientPlatform, CaptureMode, u32, u32, String)> {
    let hello: ClientHello = match protocol::read_framed(stream) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("client {peer} sent no valid hello: {e}");
            return None;
        }
    };
    if hello.protocol_version != protocol::PROTOCOL_VERSION {
        eprintln!(
            "warning: client {peer} protocol v{} != host v{} — proceeding anyway",
            hello.protocol_version,
            protocol::PROTOCOL_VERSION
        );
    }
    if expected_pin != 0 && hello.pin != expected_pin {
        eprintln!("client {peer} rejected: wrong pairing PIN");
        return None;
    }
    if hello.width == 0
        || hello.height == 0
        || hello.width > MAX_DIMENSION
        || hello.height > MAX_DIMENSION
    {
        eprintln!(
            "client {peer} hello has implausible size {}x{}; skipping",
            hello.width, hello.height
        );
        return None;
    }
    // Label the virtual display by the device name the client supplied, falling
    // back to a generic platform label when it's empty.
    let display_name = if hello.device_name.trim().is_empty() {
        hello.platform.device_label().to_string()
    } else {
        hello.device_name.clone()
    };
    println!(
        "client {peer} hello: {}x{}, mode {:?}, platform {:?}, device {display_name:?}",
        hello.width, hello.height, hello.capture_mode, hello.platform
    );
    Some((
        hello.platform,
        hello.capture_mode,
        hello.width,
        hello.height,
        display_name,
    ))
}

pub(crate) fn host_name() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "this Mac".to_owned())
}
