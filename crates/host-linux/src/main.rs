//! Linux clicker host: accept a client, then inject its input events into the
//! local desktop through the kernel's **uinput** device (see [`inject`]).
//!
//! This is deliberately the *control-only* half of the app — the same shape as
//! the Windows host before it grew capture. A phone can drive PowerPoint,
//! LibreOffice Impress or a PDF on this machine; there is no video stream, no
//! slide preview, and no window picker. `docs/LINUX-HOST.md` §7 is why: capture
//! on Linux forks into an X11 implementation and a Wayland/PipeWire one, while
//! uinput injection is a single implementation that works everywhere, so the
//! clicker is the whole product minus the fork.
//!
//! Run: `extender-host-linux [BIND_ADDR]` (default `0.0.0.0:9000`), or with no
//! argument for the GUI host window.
//!
//! Linux-only (uses uinput); will not compile on other platforms.

mod discovery;
mod firewall;
mod gui;
mod inject;
mod qr;
mod wifi;

use std::io;
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use extender_protocol::{
    self as protocol, CaptureMode, ClientHello, ClientPlatform, Input, Message,
};
use extender_transport::{self as transport, Conn};

use crate::inject::Injector;

/// A lifecycle event from the accept loop, for the CLI logger or the GUI window.
pub(crate) enum HostEvent {
    /// Listening and idle (no client).
    Waiting,
    /// A client connected: its address and the device platform from its hello.
    Connected { peer: String, platform: ClientPlatform },
    /// The client disconnected (its address).
    Disconnected(String),
    /// A non-fatal accept error.
    Error(String),
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // An explicit bind address (used by scripts) runs headless on the console;
    // no argument launches the GUI host window.
    match std::env::args().nth(1) {
        Some(addr) if addr != "--gui" => run_cli(&addr),
        _ => gui::run(),
    }
}

/// Headless console host: bind, then serve clients forever, logging to stdout.
fn run_cli(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Fail loudly and early rather than accepting a client and then dropping
    // every keystroke on the floor — the failure mode this check exists for.
    if let Some(problem) = inject::uinput_status().problem() {
        eprintln!("warning: {problem}");
        eprintln!("         the host will still accept clients, but nothing will be injected.");
    }

    let listener = TcpListener::bind(addr)?;
    println!(
        "universal-screens linux host listening on {} (protocol v{})",
        listener.local_addr()?,
        protocol::PROTOCOL_VERSION
    );
    let stop = AtomicBool::new(false);
    serve_loop(&listener, &stop, 0, &|event| match event {
        HostEvent::Waiting => println!("waiting for a client…"),
        HostEvent::Connected { peer, platform } => println!("client {peer} connected ({platform:?})"),
        HostEvent::Disconnected(peer) => println!("client {peer} disconnected"),
        HostEvent::Error(msg) => eprintln!("{msg}"),
    });
    Ok(())
}

/// Accept and serve clients until `stop` is set. Identical in shape to the
/// Windows and macOS hosts: Noise first (keyed by the pairing PIN), then the
/// plaintext-inside-the-tunnel hello, then the session.
pub(crate) fn serve_loop(
    listener: &TcpListener,
    stop: &AtomicBool,
    expected_pin: u32,
    on_event: &(dyn Fn(HostEvent) + Sync),
) {
    let _ = listener.set_nonblocking(true);
    on_event(HostEvent::Waiting);
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, peer_addr)) => {
                let _ = stream.set_nonblocking(false); // blocking reads for the session
                let peer = peer_addr.to_string();
                match transport::accept(stream, expected_pin) {
                    Ok(mut conn) => {
                        if !conn.is_encrypted() {
                            eprintln!(
                                "warning: client {peer} connected without transport encryption (plaintext)"
                            );
                        }
                        if let Some((platform, mode)) = read_hello(&mut conn, &peer, expected_pin) {
                            on_event(HostEvent::Connected { peer: peer.clone(), platform });
                            if let Err(e) = serve(conn, mode) {
                                on_event(HostEvent::Error(format!("session with {peer} ended: {e}")));
                            }
                            on_event(HostEvent::Disconnected(peer));
                        }
                    }
                    Err(e) => {
                        on_event(HostEvent::Error(format!("handshake with {peer} failed: {e}")));
                    }
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

/// Read and log the client's [`ClientHello`], tolerating a protocol-version skew
/// the way the other hosts do. Returns the client's [`ClientPlatform`] and
/// requested mode, or `None` (and logs) on a missing or garbled hello.
fn read_hello(
    stream: &mut Conn,
    peer: &str,
    expected_pin: u32,
) -> Option<(ClientPlatform, CaptureMode)> {
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
    println!(
        "client {peer} hello: {}x{}, mode {:?}, platform {:?}",
        hello.width, hello.height, hello.capture_mode, hello.platform
    );
    Some((hello.platform, hello.capture_mode))
}

/// Serve one client until it disconnects.
///
/// ⚠️ **A mirror/second-screen request is served as a clicker, not refused.** The
/// protocol's own note on [`CaptureMode::ControlOnly`] says a host that can't do
/// a mode may fall back — and the client has already drawn its mode picker by the
/// time it connects. Injecting input while sending no video degrades to a working
/// remote control; rejecting the session gives the user a dead app and no reason.
fn serve(stream: Conn, mode: CaptureMode) -> Result<(), Box<dyn std::error::Error>> {
    let _ = stream.set_nodelay(true); // disable Nagle — low latency for input

    let mut writer = stream.try_clone()?;
    let name = host_name();
    let _ = protocol::write_framed(&mut writer, &Message::HostInfo { os: "linux".into(), name });

    if mode != CaptureMode::ControlOnly {
        eprintln!(
            "client asked for {mode:?}; this host has no capture backend — serving input only \
             (see docs/LINUX-HOST.md)"
        );
    }
    serve_clicker(stream, writer)
}

/// Clicker: inject input until the client disconnects.
///
/// Simpler than the Windows twin because there is nothing to capture: no
/// snapshot thread, no deck scan, no page index. The control requests that would
/// drive those still have to be *answered* rather than ignored — see below.
fn serve_clicker(stream: Conn, mut writer: Conn) -> Result<(), Box<dyn std::error::Error>> {
    let mut injector = match Injector::new() {
        Ok(i) => Some(i),
        Err(e) => {
            // Report it once per session rather than per keystroke, and keep the
            // session alive so the client shows "connected" and the GUI's uinput
            // warning is what explains the silence.
            eprintln!("cannot open uinput ({e}) — input will not be injected");
            None
        }
    };

    let mut input = stream;
    while let Ok(event) = protocol::read_framed::<_, Input>(&mut input) {
        match event {
            // ⚠️ Answer the window picker with an empty list rather than staying
            // silent. The client waits for a `WindowList` before drawing the
            // picker, so silence is an apparent hang; an empty list renders as
            // "no windows" and the user moves on. Wayland has no window
            // enumeration protocol at all, and Stage 2's X11 EWMH path would only
            // work on half the machines — so empty is the honest answer for both.
            Input::ListWindows => {
                let _ = protocol::write_framed(&mut writer, &Message::WindowList { windows: vec![] });
                continue;
            }
            // Nothing to focus without a window list, and nothing to scan without
            // capture. The protocol permits a host to ignore both.
            Input::FocusWindow { .. } | Input::ScanDeck => continue,
            _ => {}
        }
        if let Some(injector) = injector.as_mut() {
            injector.inject(event);
        }
    }

    if let Some(injector) = injector {
        let dropped = injector.dropped_chars();
        if dropped > 0 {
            eprintln!(
                "note: {dropped} character(s) from the phone's keyboard could not be typed \
                 (non-ASCII — see Injector::text in inject.rs)"
            );
        }
    }
    Ok(())
}

/// This machine's name, for the client's saved-connection label. `HOSTNAME` is a
/// shell variable rather than an exported one on many systems, so the kernel's
/// value is read directly first.
pub(crate) fn host_name() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_owned())
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "this PC".to_owned())
}
