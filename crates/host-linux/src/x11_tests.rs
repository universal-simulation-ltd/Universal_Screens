//! End-to-end capture and window-picker tests against a **live X server**,
//! asserting real pixels rather than bytes this repo made up.
//!
//! The unit tests in [`crate::capture`] feed the decoder buffers constructed
//! here, so they prove the arithmetic and nothing at all about the server. That
//! is the shape of test that let a broken HEIC decoder ship elsewhere in this
//! suite: a fixture generated through the same assumption it is checking. These
//! tests paint the root window a colour whose three channels *differ* and read
//! it back, so a B/R swap, a big-endian misread or a stride error cannot pass.
//!
//! They also cover what a container genuinely can prove about the Linux host —
//! unlike Stage 1's uinput injection, which needs a real desktop. An X server is
//! a process, so `Xvfb` is not a stand-in for the real thing here; it *is* an X
//! server, running the same protocol a desktop one does.
//!
//! ## Running
//!
//! `DISPLAY` must point at a server: `Xvfb :99 -screen 0 1280x800x24`, with
//! `xsetroot` (from `x11-xserver-utils`) on PATH to paint the root. Nothing else
//! is needed — the window-picker test creates its own window rather than
//! depending on a desktop app being installed.
//!
//! ⚠️ **A skipped test that reports success is worse than no test.** Without a
//! server these skip — but `SCREENS_REQUIRE_X11=1` turns skipping into a
//! *failure*, so a harness that quietly stopped exercising the capture path
//! cannot look like a clean pass. The container run sets it.

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ChangeWindowAttributesAux, ConnectionExt as _};
use x11rb::rust_connection::RustConnection;

use crate::{capture, winlist};

/// The root colour the tests paint and expect back. Deliberately three
/// different channel values: any swap between R, G and B changes the answer.
const R: u8 = 0x30;
const G: u8 = 0x50;
const B: u8 = 0xA0;

/// True when this run insists the X11 tests really execute.
fn required() -> bool {
    std::env::var("SCREENS_REQUIRE_X11").is_ok_and(|v| v != "0")
}

/// WARNING: these tests share one X display, which is global mutable state - the
/// window-picker test maps a window, and a capture test running concurrently
/// would photograph it. `cargo test` runs tests as threads in one process, so
/// they must take turns. (Without this lock the suite produced a convincing
/// "capture is broken" failure that was only two tests colliding.)
static X11: Mutex<()> = Mutex::new(());

/// A held display: the turn-taking lock, plus the connection that painted the
/// root, kept open for the life of the test.
///
/// WARNING: the connection must outlive the grab. X frees a client's resources
/// when it disconnects, so painting from a short-lived helper is not reliably
/// still on screen afterwards - which is exactly how `xsetroot -solid` behaves
/// under Xvfb here: it exits 0 and the framebuffer stays black. Every capture
/// test polled a black screen and blamed the decoder.
struct Display {
    _guard: MutexGuard<'static, ()>,
    _conn: RustConnection,
}

/// Take the display lock, paint the root, and hand back a live handle - or
/// `None` when this machine has no usable X server, in which case the test
/// returns without asserting.
///
/// Under `SCREENS_REQUIRE_X11` a would-be skip becomes a failure instead, so a
/// harness that quietly stopped exercising the capture path cannot pass clean.
fn x11_ready() -> Option<Display> {
    // A panicking test poisons the lock; the display itself is fine, so carry on
    // with it rather than cascading one failure into five.
    let guard = X11.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

    if !std::env::var("DISPLAY").is_ok_and(|d| !d.is_empty()) {
        assert!(
            !required(),
            "SCREENS_REQUIRE_X11 is set but DISPLAY is not - the X11 tests did not run"
        );
        eprintln!("skipping: no DISPLAY (set SCREENS_REQUIRE_X11=1 to make this a failure)");
        return None;
    }

    let Ok((conn, screen_num)) = x11rb::connect(None) else {
        assert!(!required(), "SCREENS_REQUIRE_X11 is set but no X server would accept a connection");
        eprintln!("skipping: no X server on DISPLAY");
        return None;
    };
    let screen = conn.setup().roots[screen_num].clone();

    // The pixel value the way every X client packs a TrueColor 24-bit one. The
    // decoder under test instead derives its channels from the visual's masks,
    // so the two arrive at the answer by different routes: if this server's
    // masks were not the standard ones, the assertions would FAIL rather than
    // quietly agree with a matching mistake.
    let pixel = (u32::from(R) << 16) | (u32::from(G) << 8) | u32::from(B);
    conn.change_window_attributes(
        screen.root,
        &ChangeWindowAttributesAux::new().background_pixel(pixel),
    )
    .expect("set root background")
    .check()
    .expect("root background applied");
    // Width/height 0 means "to the far edge", i.e. the whole root.
    conn.clear_area(false, screen.root, 0, 0, 0, 0).expect("clear root").check().expect("root cleared");

    // `.check()` above already round-trips, so the server has processed the
    // repaint before any grab happens - no sleeping, no polling.
    Some(Display { _guard: guard, _conn: conn })
}

#[test]
fn a_captured_root_window_has_the_colour_it_was_painted() {
    let Some(_display) = x11_ready() else { return };
    let (w, h, bgra) =
        capture::grab_primary_bgra().expect("an X server is present, so a grab must succeed");

    assert!(w > 0 && h > 0, "non-empty geometry");
    assert_eq!(
        bgra.len(),
        w as usize * h as usize * 4,
        "tightly packed BGRA: any scanline padding must already be stripped"
    );

    // Sample several places, not just the origin: a stride bug reads row 0
    // correctly and drifts further wrong down the frame.
    for (x, y) in [(0, 0), (w / 2, h / 2), (w - 1, h - 1), (w - 1, 0), (0, h - 1)] {
        let o = (y as usize * w as usize + x as usize) * 4;
        let px = (bgra[o], bgra[o + 1], bgra[o + 2], bgra[o + 3]);
        assert_eq!(
            px,
            (B, G, R, 0xFF),
            "pixel at ({x},{y}) should be the painted colour, as B,G,R,A"
        );
    }
}

/// The JPEG the client actually receives, decoded again — proving the whole
/// chain (grab → BGRA → RGB → downscale → encode) keeps the colour, not just
/// the grab. A channel swap in `bgra_to_jpeg` would survive the test above.
#[test]
fn the_encoded_preview_still_carries_the_painted_colour() {
    let Some(_display) = x11_ready() else { return };
    let (w, h, jpeg) = capture::capture_primary_jpeg(200, 90)
        .expect("an X server is present, so a capture must succeed");
    assert!(w <= 200 && h <= 200, "downscaled to the cap, got {w}x{h}");
    assert!(jpeg.starts_with(&[0xFF, 0xD8]), "JPEG SOI marker");

    let decoded = image::load_from_memory(&jpeg).expect("re-decodes").to_rgb8();
    let centre = decoded.get_pixel(decoded.width() / 2, decoded.height() / 2).0;
    // JPEG is lossy, so allow a tolerance — but one far tighter than the gap
    // between any two of the three channels, which is what a swap would move.
    for (got, want, name) in [(centre[0], R, "red"), (centre[1], G, "green"), (centre[2], B, "blue")]
    {
        assert!(
            got.abs_diff(want) <= 8,
            "{name} channel: got {got}, expected about {want} (full pixel {centre:?})"
        );
    }
}

/// A live server must report a working backend, and name which one.
#[test]
fn a_live_server_reports_a_working_backend() {
    let Some(_display) = x11_ready() else { return };
    let status = capture::status().expect("an X server is present, so status must be Ok");
    assert!(status.contains("X11"), "the description names the path: {status}");
    assert!(capture::is_available());
}

/// Two grabs in a row must both work: the SHM segment is attached once and
/// reused, so a lifetime bug there shows up on the *second* call, not the first.
#[test]
fn repeated_grabs_reuse_the_connection_and_stay_correct() {
    let Some(_display) = x11_ready() else { return };
    let (w1, h1, a) = capture::grab_primary_bgra().expect("first grab");
    let (w2, h2, b) = capture::grab_primary_bgra().expect("second grab");
    assert_eq!((w1, h1), (w2, h2), "same geometry");
    assert_eq!(a.len(), b.len(), "same buffer size");

    // ⚠️ Never `assert_eq!` two framebuffers: a mismatch prints megabytes of
    // bytes. Report where they first differ, and how many pixels did.
    if let Some(at) = a.iter().zip(&b).position(|(x, y)| x != y) {
        let differing = (0..a.len() / 4).filter(|i| a[i * 4..i * 4 + 4] != b[i * 4..i * 4 + 4]).count();
        panic!(
            "two grabs of a still screen differ: first at byte {at} (pixel {}),              {differing} of {} pixels differ",
            at / 4,
            a.len() / 4
        );
    }
}

/// ⚠️ **The two grab paths must agree pixel for pixel.** The MIT-SHM path was
/// chosen on a *speed* measurement — 11× — with nothing checking what it
/// actually wrote. A fast wrong frame looks exactly like a fast right one, so
/// this compares the same screen through both and fails on the first byte that
/// differs.
#[test]
fn shm_and_getimage_capture_the_same_screen() {
    let Some(_display) = x11_ready() else { return };

    let mut shm = capture::Capturer::open(true).expect("a capturer with SHM preferred");
    let mut plain = capture::Capturer::open(false).expect("a capturer without SHM");
    assert_eq!(plain.backend(), capture::Backend::GetImage, "the flag must be honoured");

    let (sw, sh, a) = shm.grab_bgra().expect("shm-preferred grab");
    let (pw, ph, b) = plain.grab_bgra().expect("getimage grab");
    assert_eq!((sw, sh), (pw, ph), "same geometry from both paths");

    if let Some(at) = a.iter().zip(&b).position(|(x, y)| x != y) {
        let differing = (0..a.len() / 4).filter(|i| a[i * 4..i * 4 + 4] != b[i * 4..i * 4 + 4]).count();
        let (pixel, row, total) = (at / 4, at / 4 / sw as usize, a.len() / 4);
        panic!(
            "{:?} and GetImage disagree: first at byte {at} (pixel {pixel}, row {row}),              {differing} of {total} pixels differ",
            shm.backend()
        );
    }
    // And whichever path this machine offers, the colour must still be right.
    assert_eq!(a.first_chunk::<4>(), Some(&[B, G, R, 0xFF]), "top-left is the painted colour");
}

/// The window picker against a real server, with a window this test creates
/// itself rather than an `xmessage` that may not be installed — which also lets
/// it assert the *title* round-trips, not merely that something was found.
///
/// Under Xvfb there is no window manager, so this exercises the `query_tree`
/// fallback specifically: the path a minimal session takes when nothing
/// publishes `_NET_CLIENT_LIST`.
#[test]
fn the_window_picker_finds_a_mapped_titled_window_and_keeps_its_title() {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, CreateWindowAux, PropMode, WindowClass};
    use x11rb::wrapper::ConnectionExt as _;

    let Some(_display) = x11_ready() else { return };
    // A non-ASCII title, so the UTF-8 `_NET_WM_NAME` path is the one under test —
    // the legacy Latin-1 `WM_NAME` fallback could not carry this correctly.
    const TITLE: &str = "UNI·SIM probe — slidedeck";

    let (conn, screen_num) = x11rb::connect(None).expect("an X server is present");
    let screen = &conn.setup().roots[screen_num];
    let win = conn.generate_id().expect("a window id");
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        win,
        screen.root,
        10,
        10,
        320,
        200,
        0,
        WindowClass::INPUT_OUTPUT,
        screen.root_visual,
        // Default `override_redirect` is false, which is what makes this window
        // pickable — the flag menus and tooltips set to opt out of management.
        &CreateWindowAux::new().background_pixel(screen.white_pixel),
    )
    .expect("create_window");

    let utf8 = conn.intern_atom(false, b"UTF8_STRING").unwrap().reply().unwrap().atom;
    let net_wm_name = conn.intern_atom(false, b"_NET_WM_NAME").unwrap().reply().unwrap().atom;
    conn.change_property8(PropMode::REPLACE, win, net_wm_name, utf8, TITLE.as_bytes())
        .expect("set _NET_WM_NAME");
    conn.change_property8(
        PropMode::REPLACE,
        win,
        u32::from(AtomEnum::WM_NAME),
        u32::from(AtomEnum::STRING),
        b"fallback",
    )
    .expect("set WM_NAME");
    conn.map_window(win).expect("map_window");
    conn.flush().expect("flush");

    // Give the server time to make it viewable before another connection looks.
    std::thread::sleep(Duration::from_millis(300));

    let windows = winlist::list_windows();
    let found = windows.iter().find(|(_, t)| t == TITLE);
    assert!(
        found.is_some(),
        "expected the created window titled {TITLE:?} in the list, got {windows:?}"
    );
    // ⚠️ `_NET_WM_NAME` must win over `WM_NAME`: reading the legacy property
    // instead would hand the client "fallback", and would mangle any accented
    // character in a real application's title.
    assert_ne!(found.unwrap().1, "fallback", "the UTF-8 title must win over the legacy one");

    // Raising it must not panic, WM or no WM.
    winlist::focus_window(found.unwrap().0);

    // An unmapped window must drop straight back out of the list.
    conn.unmap_window(win).expect("unmap");
    conn.flush().expect("flush");
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !winlist::list_windows().iter().any(|(_, t)| t == TITLE),
        "an unmapped window is not pickable"
    );

    conn.destroy_window(win).expect("destroy");
    let _ = conn.flush();
}

/// The whole mirror, end to end: paint the root, run [`crate::stream`] against a
/// loopback socket, and **decode what comes out** with the same openh264 the
/// desktop client uses — then check the picture is the colour that was painted.
///
/// ⚠️ This is the test the Stage 2a work could not have: every earlier capture
/// test stops at BGRA or JPEG. A mirror can fail in four more places after that
/// — a wrong `StreamStart`, parameter sets sent in the wrong form, AVCC lengths
/// off by the prefix, or a channel swap in the BGRA→YUV conversion — and every
/// one of them produces a stream that *arrives* and is either garbage or the
/// wrong colour. Bytes flowing is not a working mirror.
///
/// It uses the *client's* helpers to rebuild the access unit, so a host that
/// framed the stream in a way only the host could read fails here.
#[test]
fn the_mirror_streams_decodable_frames_of_the_painted_screen() {
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use extender_protocol::{self as protocol, Codec, Message};
    use extender_transport::Conn;
    // `dimensions`/`rgb8_len` come from the trait, not the struct.
    use openh264::formats::YUVSource;

    let Some(_display) = x11_ready() else { return };

    // A loopback pair standing in for a connected client. Plaintext: the Noise
    // handshake is the transport crate's business and is tested there, and
    // wrapping it here would test encryption rather than video.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let mut client = TcpStream::connect(addr).expect("connect loopback");
    let (server, _) = listener.accept().expect("accept loopback");
    // Bound every read: a mirror that silently sends nothing must fail the test
    // rather than hang the whole suite with no output.
    client
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("read timeout");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_worker = Arc::clone(&stop);
    let host = std::thread::spawn(move || crate::stream::run(Conn::Plain(server), &stop_worker, &crate::stream::StreamRequest::mirror()));

    // 1. The stream must open with geometry a client can size a surface from.
    let start: Message = protocol::read_framed(&mut client).expect("StreamStart arrives");
    let Message::StreamStart { width, height, codec, parameter_sets } = start else {
        panic!("the first message must be StreamStart, got {start:?}");
    };
    assert_eq!(codec, Codec::H264);
    assert!(width > 0 && height > 0, "non-empty geometry: {width}x{height}");
    assert_eq!(width % 2, 0, "H.264 needs even dimensions, got width {width}");
    assert_eq!(height % 2, 0, "H.264 needs even dimensions, got height {height}");
    assert!(width.max(height) <= 1280, "the long side is capped, got {width}x{height}");
    assert!(!parameter_sets.is_empty(), "SPS/PPS must open the stream");

    // 2. Decode frames until the decoder yields a picture. openh264 can need
    //    more than one access unit before it emits one, so this is a loop with a
    //    bound rather than a single decode.
    let mut decoder = openh264::decoder::Decoder::new().expect("decoder");
    let sps_pps = protocol::annex_b_parameter_sets(&parameter_sets);
    let mut picture = None;
    for _ in 0..30 {
        let msg: Message = protocol::read_framed(&mut client).expect("a Frame arrives");
        let Message::Frame { data, keyframe, .. } = msg else {
            continue; // not video (nothing else is sent in mirror mode, but be exact)
        };
        let mut au = if keyframe { sps_pps.clone() } else { Vec::new() };
        protocol::append_annex_b(&mut au, &data);
        if let Ok(Some(yuv)) = decoder.decode(&au) {
            let (w, h) = yuv.dimensions();
            let mut rgb = vec![0u8; yuv.rgb8_len()];
            yuv.write_rgb8(&mut rgb);
            picture = Some((w, h, rgb));
            break;
        }
    }
    let (dw, dh, rgb) = picture.expect("openh264 must produce a picture from the host's stream");
    assert_eq!((dw as u32, dh as u32), (width, height), "decoded size matches StreamStart");

    // 3. And it must be the colour the root was painted. Sample away from the
    //    edges: H.264 pads to macroblocks, so the last few rows are the least
    //    trustworthy place to judge a channel swap from.
    let (x, y) = (dw / 2, dh / 2);
    let o = (y * dw + x) * 3;
    let centre = (rgb[o], rgb[o + 1], rgb[o + 2]);
    // Lossy at 4:2:0 plus a BT.601 round trip, so a tolerance — but 24 is far
    // less than the 32 and 80 that separate the three painted channels, which is
    // what any swap would have to move.
    for (got, want, name) in [(centre.0, R, "red"), (centre.1, G, "green"), (centre.2, B, "blue")] {
        assert!(
            got.abs_diff(want) <= 24,
            "{name} channel: got {got}, expected about {want} (full pixel {centre:?})"
        );
    }

    // Stop the host, then close our end so a thread parked in a socket write
    // cannot outlive the test.
    stop.store(true, Ordering::Relaxed);
    let _ = client.shutdown(std::net::Shutdown::Both);
    let mut drain = [0u8; 4096];
    while let Ok(n) = client.read(&mut drain) {
        if n == 0 {
            break;
        }
    }
    host.join().expect("the stream thread exits");
}

// ---------------------------------------------------------------------------
// The second screen (`crate::vdisplay`)
// ---------------------------------------------------------------------------

/// Second-screen colour: distinct from the root's `R`/`G`/`B` in every channel,
/// so a capture that ignored the region's offset and photographed the desktop
/// instead fails on the pixels rather than on the size.
const S_R: u8 = 0xC0;
const S_G: u8 = 0x20;
const S_B: u8 = 0x70;

/// True when this run insists the second-screen tests really execute.
///
/// ⚠️ A separate switch from `SCREENS_REQUIRE_X11`, because they need a
/// **different** X server. Xvfb fixes its framebuffer at the size it was started
/// with (measured: `maximum` equals `current` in its RandR size range), so the
/// desktop cannot grow and a second screen cannot be made — the capture tests
/// run there perfectly well. `scripts/test-linux-x11.sh` starts an Xorg with the
/// `dummy` driver for these, and sets this.
fn randr_required() -> bool {
    std::env::var("SCREENS_REQUIRE_RANDR").is_ok_and(|v| v != "0")
}

/// Whether this server's framebuffer can grow at all, plus its current size.
/// `None` — a skip — when it cannot, which is Xvfb and any server at its limit.
fn resizable_root(conn: &RustConnection, root: u32) -> Option<(u16, u16)> {
    use x11rb::protocol::randr::ConnectionExt as _;
    use x11rb::protocol::xproto::ConnectionExt as _;

    let ver = conn.randr_query_version(1, 5).ok()?.reply().ok()?;
    if (ver.major_version, ver.minor_version) < (1, 5) {
        return None;
    }
    let range = conn.randr_get_screen_size_range(root).ok()?.reply().ok()?;
    let geom = conn.get_geometry(root).ok()?.reply().ok()?;
    (range.max_width > geom.width && range.max_height >= geom.height)
        .then_some((geom.width, geom.height))
}

/// Take the display and confirm it can be extended, or skip — loudly under
/// `SCREENS_REQUIRE_RANDR`, for the same reason as [`x11_ready`].
fn randr_ready() -> Option<(Display, RustConnection, u32, (u16, u16))> {
    let display = x11_ready()?;
    let Ok((conn, screen_num)) = x11rb::connect(None) else {
        assert!(!randr_required(), "SCREENS_REQUIRE_RANDR is set but no X server would connect");
        return None;
    };
    let root = conn.setup().roots[screen_num].root;
    match resizable_root(&conn, root) {
        Some(size) => Some((display, conn, root, size)),
        None => {
            assert!(
                !randr_required(),
                "SCREENS_REQUIRE_RANDR is set but this X server's framebuffer cannot grow - the \
                 second-screen tests did not run (Xvfb is fixed at its start-up size; use the \
                 Xorg dummy driver, as scripts/test-linux-x11.sh does)"
            );
            eprintln!(
                "skipping: this X server cannot resize its framebuffer (set \
                 SCREENS_REQUIRE_RANDR=1 to make this a failure)"
            );
            None
        }
    }
}

/// Every monitor the server currently lists, by name.
fn monitor_names(conn: &RustConnection, root: u32) -> Vec<String> {
    use x11rb::protocol::randr::ConnectionExt as _;
    use x11rb::protocol::xproto::ConnectionExt as _;

    let Some(reply) =
        conn.randr_get_monitors(root, false).ok().and_then(|c| c.reply().ok())
    else {
        return Vec::new();
    };
    reply
        .monitors
        .iter()
        .filter_map(|m| {
            let name = conn.get_atom_name(m.name).ok()?.reply().ok()?.name;
            String::from_utf8(name).ok()
        })
        .collect()
}

/// The whole lifecycle in one test, because the half nobody would notice is the
/// *end*: a host that grew the desktop and never shrank it again leaves the user
/// with a 3000-pixel-wide screen and no idea why.
#[test]
fn a_second_screen_extends_the_desktop_and_puts_it_back_afterwards() {
    use x11rb::protocol::xproto::ConnectionExt as _;

    let Some((_display, conn, root, (w0, h0))) = randr_ready() else { return };

    let before = monitor_names(&conn, root);
    assert!(
        !before.iter().any(|n| n == "Universal-Screens"),
        "a leftover monitor from an earlier run would make this test pass for the wrong reason: \
         {before:?}"
    );

    {
        let vs = crate::vdisplay::VirtualScreen::create(800, 600, "test phone")
            .expect("this server can resize, so a second screen must be creatable");

        // It goes to the RIGHT of the existing desktop, at the full requested
        // size: overlapping the real desktop would mean the phone showing the
        // user's own windows back to them.
        let region = vs.region();
        assert_eq!(
            (region.x, region.y, region.width, region.height),
            (i16::try_from(w0).unwrap(), 0, 800, 600),
            "the new area starts where the old desktop ended"
        );

        let geom = conn.get_geometry(root).unwrap().reply().unwrap();
        assert_eq!(geom.width, w0 + 800, "the framebuffer grew by exactly the client's width");
        assert_eq!(geom.height, h0.max(600), "and is tall enough for the client");

        // ⚠️ Being *listed as a monitor* is the whole difference between a
        // second screen and a rectangle we photograph: it is what makes a window
        // manager maximise into the area and a toolkit treat it as a display.
        assert!(
            monitor_names(&conn, root).iter().any(|n| n == "Universal-Screens"),
            "the new area must be a RandR monitor, not just extra framebuffer"
        );
    }

    // Dropped: the desktop must be exactly as it was found.
    let geom = conn.get_geometry(root).unwrap().reply().unwrap();
    assert_eq!(
        (geom.width, geom.height),
        (w0, h0),
        "the desktop must be back to its original size once the client goes"
    );
    assert!(
        !monitor_names(&conn, root).iter().any(|n| n == "Universal-Screens"),
        "the monitor must be deleted too, not just the framebuffer shrunk"
    );
}

/// The offset is the thing this can get wrong: `shm_get_image`/`get_image` take
/// x and y, and passing 0 for them compiles, runs, and streams the user's own
/// desktop to the phone that asked for an empty second screen.
#[test]
fn the_second_screen_captures_its_own_area_and_not_the_desktop() {
    use x11rb::protocol::xproto::{ConnectionExt as _, CreateWindowAux, WindowClass};

    let Some((_display, conn, root, _)) = randr_ready() else { return };

    let vs = crate::vdisplay::VirtualScreen::create(800, 600, "test phone")
        .expect("a second screen on a resizable server");
    let region = vs.region();

    // Fill the new area with a colour the desktop does not have. Override
    // redirect: there is no window manager here, and a managed window could be
    // placed somewhere else entirely.
    let pixel = (u32::from(S_R) << 16) | (u32::from(S_G) << 8) | u32::from(S_B);
    let win = conn.generate_id().expect("a window id");
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        win,
        root,
        region.x,
        region.y,
        region.width,
        region.height,
        0,
        WindowClass::INPUT_OUTPUT,
        conn.setup().roots[0].root_visual,
        &CreateWindowAux::new().background_pixel(pixel).override_redirect(1),
    )
    .expect("create_window");
    conn.map_window(win).expect("map_window");
    conn.flush().expect("flush");
    std::thread::sleep(Duration::from_millis(300));

    let (w, h, bgra) =
        capture::grab_bgra_of(Some(region)).expect("the second screen's area must be capturable");
    assert_eq!((w, h), (800, 600), "a region grab returns the region's size, not the desktop's");

    for (x, y) in [(1, 1), (w / 2, h / 2), (w - 2, h - 2)] {
        let o = (y as usize * w as usize + x as usize) * 4;
        assert_eq!(
            (bgra[o], bgra[o + 1], bgra[o + 2]),
            (S_B, S_G, S_R),
            "pixel at ({x},{y}) is the second screen's colour, not the desktop's ({B},{G},{R})"
        );
    }

    // And the desktop itself is untouched — the region grab is not a resize of
    // the whole-root one.
    let (_, _, root_bgra) = capture::grab_primary_bgra().expect("the desktop is still capturable");
    assert_eq!(
        (root_bgra[0], root_bgra[1], root_bgra[2]),
        (B, G, R),
        "the desktop's own top-left pixel is unchanged"
    );

    conn.destroy_window(win).expect("destroy");
    let _ = conn.flush();
}

/// End to end, the way the phone sees it: ask [`crate::stream`] for a second
/// screen and check the video that comes back is the size the *client* asked
/// for, not the size of this desktop.
///
/// ⚠️ That one assertion is the whole test. A host that quietly mirrored instead
/// — which is exactly what this host did until the second screen existed, and
/// what it still does on a server that cannot resize — sends a stream sized from
/// the desktop. The two are indistinguishable in every other way: same codec,
/// same framing, same message order.
#[test]
fn asking_for_a_second_screen_streams_the_second_screen() {
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use extender_protocol::{self as protocol, Message};
    use extender_transport::Conn;

    let Some((_display, conn, root, (w0, h0))) = randr_ready() else { return };
    // A size that cannot be confused with this desktop's, and that survives the
    // encoder's 1280-px cap unscaled so the assertion is exact.
    const CLIENT: (u32, u32) = (720, 1180);
    assert_ne!((u32::from(w0), u32::from(h0)), CLIENT, "the test size must differ from the desktop");

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let mut client = TcpStream::connect(addr).expect("connect loopback");
    let (server, _) = listener.accept().expect("accept loopback");
    client.set_read_timeout(Some(Duration::from_secs(30))).expect("read timeout");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_worker = Arc::clone(&stop);
    let req = crate::stream::StreamRequest {
        second_screen: true,
        client_size: CLIENT,
        label: "test phone".to_owned(),
    };
    let host =
        std::thread::spawn(move || crate::stream::run(Conn::Plain(server), &stop_worker, &req));

    let start: Message = protocol::read_framed(&mut client).expect("StreamStart arrives");
    let Message::StreamStart { width, height, .. } = start else {
        panic!("the first message must be StreamStart, got {start:?}");
    };
    assert_eq!(
        (width, height),
        CLIENT,
        "the stream is the second screen the client asked for, not a mirror of this desktop \
         ({w0}x{h0})"
    );

    stop.store(true, Ordering::Relaxed);
    let _ = client.shutdown(std::net::Shutdown::Both);
    let mut drain = [0u8; 4096];
    while let Ok(n) = client.read(&mut drain) {
        if n == 0 {
            break;
        }
    }
    host.join().expect("the stream thread exits");

    // ⚠️ And the desktop is back afterwards. The virtual screen is owned by the
    // stream, so a leak here would be invisible to every other assertion and
    // would leave a real user's display permanently wider.
    use x11rb::protocol::xproto::ConnectionExt as _;
    let geom = conn.get_geometry(root).unwrap().reply().unwrap();
    assert_eq!(
        (geom.width, geom.height),
        (w0, h0),
        "ending the stream must put the desktop back"
    );
}
