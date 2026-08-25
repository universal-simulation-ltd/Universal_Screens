//! Linux clicker host: accept a client, then inject its input events into the
//! local desktop through the kernel's **uinput** device (see [`inject`]).
//!
//! A phone can drive PowerPoint, LibreOffice Impress or a PDF on this machine,
//! see live slide previews of it, and pick which window the keys land in. What
//! it cannot do here is *mirror*: there is no H.264 video stream yet.
//!
//! ⚠️ **Previews and the window picker are X11 only** — Stage 2 of
//! `docs/LINUX-HOST.md`. Injection (uinput) is one implementation that works
//! under X11 and every Wayland compositor alike; capture is not, and Wayland's
//! is a different job again (portal + PipeWire, Stage 3). On Wayland both
//! degrade rather than fail: [`capture`] reports why, and [`winlist`] returns an
//! empty list because Wayland has no window-enumeration protocol at all.
//!
//! Run: `extender-host-linux [BIND_ADDR]` (default `0.0.0.0:9000`), or with no
//! argument for the GUI host window.
//!
//! Linux-only (uses uinput); will not compile on other platforms.

mod capture;
mod discovery;
mod firewall;
mod gui;
mod inject;
mod qr;
mod wifi;
mod winlist;
/// Live-X-server tests; see the module docs for how to run them and for why a
/// skip is made loud rather than silent.
#[cfg(test)]
mod x11_tests;

use std::io;
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use extender_protocol::{
    self as protocol, CaptureMode, ClientHello, ClientPlatform, Input, Message,
};
use extender_transport::{self as transport, Conn};

use crate::inject::Injector;

/// Slide previews: the same cap and quality the Windows and macOS hosts use, so
/// a phone gets an identically sized tile whichever host it is driving.
const SNAPSHOT_MAX_DIM: u32 = 1000;
const SNAPSHOT_QUALITY: u8 = 70;
/// Wait this long after a key before capturing, so the slide has redrawn.
const SNAPSHOT_DELAY: Duration = Duration::from_millis(350);
/// Pre-scan: per-page settle time, and a safety cap on the page count.
const SCAN_PAGE_DELAY: Duration = Duration::from_millis(250);
const SCAN_MAX_PAGES: u32 = 500;
/// Wait after raising a window before sending F5, so it is focused first.
const FOCUS_SETTLE: Duration = Duration::from_millis(250);

/// HID usage ids for the keys the deck scan taps. The clicker's own key handling
/// is the client's job; these are the two this host originates.
const HID_HOME: u32 = 0x4A;
const HID_PAGE_DOWN: u32 = 0x4E;
const HID_F5: u32 = 0x3E;

/// A request from the input loop to the snapshot thread. Identical in shape to
/// the Windows host's, because it answers the same three protocol messages.
enum SnapReq {
    /// A slide-changing key was injected (HID usage id); refresh the preview and
    /// move the tracked page index accordingly.
    Key(u32),
    /// Pre-scan the open document into the slide cache for next-slide look-ahead.
    Scan,
    /// (Re)send the window list.
    ListWindows,
    /// Raise a window, optionally starting its slideshow once it is focused.
    FocusWindow(i64, bool),
}

/// The injector, shared because the snapshot thread originates keystrokes of its
/// own (the deck scan's Home/PageDown, and F5 after raising a window) while the
/// input loop is injecting the client's.
type SharedInjector = Arc<Mutex<Option<Injector>>>;

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
/// WARNING: **A mirror/second-screen request is served as a clicker, not
/// refused.** The protocol's own note on [`CaptureMode::ControlOnly`] says a
/// host that can't do a mode may fall back - and the client has already drawn
/// its mode picker by the time it connects. Injecting input while sending no
/// video degrades to a working remote control; rejecting the session gives the
/// user a dead app and no reason. X11 capture (Stage 2) gives this host slide
/// previews, but not yet the H.264 stream a mirror needs.
fn serve(stream: Conn, mode: CaptureMode) -> Result<(), Box<dyn std::error::Error>> {
    let _ = stream.set_nodelay(true); // disable Nagle - low latency for input

    let mut writer = stream.try_clone()?;
    let name = host_name();
    let _ = protocol::write_framed(&mut writer, &Message::HostInfo { os: "linux".into(), name });

    if mode != CaptureMode::ControlOnly {
        eprintln!(
            "client asked for {mode:?}; this host has no video stream yet - serving as a \
             clicker with slide previews (see docs/LINUX-HOST.md)"
        );
    }
    serve_clicker(stream, writer)
}

/// Clicker: inject input, and drive slide previews, the deck scan and the window
/// picker on a dedicated thread.
///
/// WARNING: the capture runs on its own thread for the same reason the Windows
/// twin's does. A grab plus a JPEG encode is tens of milliseconds, and doing it
/// inline would stall the next keystroke behind the preview of the last one -
/// and a clicker's whole value is that the key lands instantly.
fn serve_clicker(stream: Conn, writer: Conn) -> Result<(), Box<dyn std::error::Error>> {
    let injector: SharedInjector = Arc::new(Mutex::new(match Injector::new() {
        Ok(i) => Some(i),
        Err(e) => {
            // Report it once per session rather than per keystroke, and keep the
            // session alive so the client shows "connected" and the GUI's uinput
            // warning is what explains the silence.
            eprintln!("cannot open uinput ({e}) - input will not be injected");
            None
        }
    }));

    let (snap_tx, snap_rx) = mpsc::channel::<SnapReq>();
    let snap_injector = Arc::clone(&injector);
    let snap_thread = thread::spawn(move || snapshot_loop(writer, &snap_rx, &snap_injector));

    let mut input = stream;
    while let Ok(event) = protocol::read_framed::<_, Input>(&mut input) {
        // Control requests are answered by the snapshot thread, not injected.
        match event {
            Input::ScanDeck => {
                let _ = snap_tx.send(SnapReq::Scan);
                continue;
            }
            Input::ListWindows => {
                let _ = snap_tx.send(SnapReq::ListWindows);
                continue;
            }
            Input::FocusWindow { id, start_show } => {
                let _ = snap_tx.send(SnapReq::FocusWindow(id, start_show));
                continue;
            }
            _ => {}
        }
        // A key press or committed text can change the slide; refresh the preview
        // after injecting it (Text reports code 0 = no page-index change).
        let nav_code = match &event {
            Input::Key { code, pressed: true } => Some(*code),
            Input::Text { .. } => Some(0),
            _ => None,
        };
        if let Ok(mut guard) = injector.lock() {
            if let Some(i) = guard.as_mut() {
                i.inject(event);
            }
        }
        if let Some(code) = nav_code {
            let _ = snap_tx.send(SnapReq::Key(code));
        }
    }

    drop(snap_tx); // unblock the snapshot thread so it exits
    let _ = snap_thread.join();

    if let Ok(guard) = injector.lock() {
        if let Some(dropped) = guard.as_ref().map(Injector::dropped_chars).filter(|&d| d > 0) {
            eprintln!(
                "note: {dropped} character(s) from the phone's keyboard could not be typed \
                 (non-ASCII - see Injector::text in inject.rs)"
            );
        }
    }
    Ok(())
}

/// Press and release one key by HID usage id, from the snapshot thread.
fn tap(injector: &SharedInjector, hid: u32) {
    let Ok(mut guard) = injector.lock() else { return };
    let Some(i) = guard.as_mut() else { return };
    i.inject(Input::Key { code: hid, pressed: true });
    i.inject(Input::Key { code: hid, pressed: false });
}

/// Drive slide previews on a dedicated thread: the current slide (a live capture)
/// on connect and after each slide-changing key, plus the adjacent slides from
/// the pre-scan cache. Owns the page index and the cache. Exits when the client
/// disconnects (a socket write fails) or the input loop drops the sender.
fn snapshot_loop(mut writer: Conn, rx: &Receiver<SnapReq>, injector: &SharedInjector) {
    let mut cache: Vec<(u32, u32, Vec<u8>)> = Vec::new();
    let mut idx: i32 = 0;

    // Say once, up front, which capture path this machine got - or why it got
    // none. Same reasoning as the uinput check: a silent no-preview session is
    // indistinguishable from a broken one.
    match capture::status() {
        Ok(backend) => println!("slide previews: {backend}"),
        Err(reason) => eprintln!("slide previews unavailable: {reason}"),
    }

    if send_previews(&mut writer, &cache, idx).is_err() {
        return; // client already gone
    }
    if send_window_list(&mut writer).is_err() {
        return;
    }
    loop {
        let Ok(first) = rx.recv() else { return };
        let mut scan = false;
        let mut refresh = false;
        if handle_req(&mut writer, injector, &cache, &mut idx, &mut scan, &mut refresh, first)
            .is_err()
        {
            return;
        }
        // Let the slide redraw / the window come forward, coalescing a burst of
        // keys into a single capture.
        thread::sleep(SNAPSHOT_DELAY);
        while let Ok(req) = rx.try_recv() {
            if handle_req(&mut writer, injector, &cache, &mut idx, &mut scan, &mut refresh, req)
                .is_err()
            {
                return;
            }
        }
        let result = if scan {
            scan_deck(&mut writer, injector, &mut cache, &mut idx)
        } else if refresh {
            send_previews(&mut writer, &cache, idx)
        } else {
            Ok(())
        };
        if result.is_err() {
            return;
        }
    }
}

/// Apply one snapshot-thread request: window list/focus act immediately, while
/// Key/Scan set flags so the loop coalesces a burst into a single capture.
fn handle_req(
    writer: &mut Conn,
    injector: &SharedInjector,
    cache: &[(u32, u32, Vec<u8>)],
    idx: &mut i32,
    scan: &mut bool,
    refresh: &mut bool,
    req: SnapReq,
) -> io::Result<()> {
    match req {
        SnapReq::Scan => *scan = true,
        SnapReq::Key(code) => {
            apply_index(idx, code, cache.len());
            *refresh = true;
        }
        SnapReq::ListWindows => send_window_list(writer)?,
        SnapReq::FocusWindow(id, start_show) => {
            winlist::focus_window(id);
            if start_show {
                // Once it is focused, start its slideshow / presenter mode.
                thread::sleep(FOCUS_SETTLE);
                tap(injector, HID_F5);
            }
            *refresh = true; // the newly-focused window becomes the next preview
        }
    }
    Ok(())
}

/// Send the host's open windows so the client can offer a focus picker.
///
/// WARNING: an empty list is sent rather than nothing at all. The client waits
/// for a `WindowList` before drawing the picker, so silence is an apparent hang;
/// empty renders as "no windows" and the user moves on. That is the permanent
/// answer on Wayland, which has no window-enumeration protocol - see [`winlist`].
fn send_window_list(writer: &mut Conn) -> io::Result<()> {
    let windows = winlist::list_windows();
    println!("sent window list ({} windows)", windows.len());
    protocol::write_framed(writer, &Message::WindowList { windows })
}

/// Update the tracked page index from an injected key (HID usage), clamping to
/// the known page range so it self-heals at the document ends.
fn apply_index(idx: &mut i32, code: u32, pages: usize) {
    match code {
        0x4E | 0x4F | 0x51 => *idx += 1, // PageDown / Right / Down
        0x4B | 0x50 | 0x52 => *idx -= 1, // PageUp / Left / Up
        0x4A => *idx = 0,                // Home
        0x4D => *idx = pages as i32 - 1, // End
        _ => {}                          // Text or other: stay put
    }
    let last = pages as i32 - 1;
    *idx = (*idx).max(0);
    if last >= 0 {
        *idx = (*idx).min(last);
    }
}

/// Send the current slide (a fresh live capture, slot 0) plus the previous and
/// next slides (from the pre-scan `cache`, slots -1 / +1, or empty markers so the
/// client clears those tiles). Only a socket write error returns `Err` (the
/// client is gone); a capture failure is logged and skipped.
fn send_previews(writer: &mut Conn, cache: &[(u32, u32, Vec<u8>)], idx: i32) -> io::Result<()> {
    if let Some((width, height, data)) =
        capture::capture_primary_jpeg(SNAPSHOT_MAX_DIM, SNAPSHOT_QUALITY)
    {
        let bytes = data.len();
        protocol::write_framed(writer, &Message::Snapshot { width, height, slot: 0, data })?;
        println!("sent current preview {width}x{height} ({bytes} B JPEG)");
    }
    // Adjacent slides from the cache, or empty markers so the client clears them.
    for (slot, offset) in [(-1, -1i32), (1, 1i32)] {
        let cached = usize::try_from(idx + offset).ok().and_then(|i| cache.get(i));
        let msg = match cached {
            Some((w, h, jpeg)) => {
                Message::Snapshot { width: *w, height: *h, slot, data: jpeg.clone() }
            }
            None => Message::Snapshot { width: 0, height: 0, slot, data: Vec::new() },
        };
        protocol::write_framed(writer, &msg)?;
    }
    Ok(())
}

/// Pre-scan the open document into `cache`: jump to the first page, page to the
/// end capturing each page (stopping when a page repeats - i.e. PageDown did
/// nothing), then return to the start. Resets `idx` to 0 and sends the first
/// page's preview. The user must keep the document focused throughout.
fn scan_deck(
    writer: &mut Conn,
    injector: &SharedInjector,
    cache: &mut Vec<(u32, u32, Vec<u8>)>,
    idx: &mut i32,
) -> io::Result<()> {
    // WARNING: without capture there are no pages to cache, and an unguarded
    // scan would spend two minutes tapping PageDown 500 times through the user's
    // live document to build nothing. This is the Wayland path.
    if !capture::is_available() {
        println!("deck scan skipped: no screen capture on this session");
        *idx = 0;
        cache.clear();
        return send_previews(writer, cache, *idx);
    }
    println!("scanning deck...");
    cache.clear();
    tap(injector, HID_HOME);
    thread::sleep(SNAPSHOT_DELAY);

    if let Some(page) = capture::capture_primary_jpeg(SNAPSHOT_MAX_DIM, SNAPSHOT_QUALITY) {
        cache.push(page);
    }
    for _ in 1..SCAN_MAX_PAGES {
        tap(injector, HID_PAGE_DOWN);
        thread::sleep(SCAN_PAGE_DELAY);
        let Some(page) = capture::capture_primary_jpeg(SNAPSHOT_MAX_DIM, SNAPSHOT_QUALITY) else {
            break;
        };
        // A page identical to the previous one means PageDown did nothing: the end.
        if cache.last().is_some_and(|prev| prev.2 == page.2) {
            break;
        }
        cache.push(page);
    }

    tap(injector, HID_HOME); // back to the start
    thread::sleep(SNAPSHOT_DELAY);
    *idx = 0;
    println!("scanned {} pages", cache.len());
    send_previews(writer, cache, *idx)
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
