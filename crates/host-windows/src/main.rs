//! Windows clicker host: accept a client, then inject its input events into the
//! local desktop via `SendInput`. Unlike the macOS host this captures and streams
//! *no* video — it's the true [`CaptureMode::ControlOnly`] implementation (M6c),
//! so a phone can drive PowerPoint / a PDF on this laptop with no Mac involved.
//!
//! Run: cargo run -p extender-host-windows [-- BIND_ADDR]   (default 0.0.0.0:9000)
//! Windows-only (uses Win32 `SendInput`); will not compile on other platforms.

use std::mem::size_of;
use std::net::{TcpListener, TcpStream};

use extender_protocol::{self as protocol, Button, ClientHello, Input};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_HWHEEL,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS,
    VIRTUAL_KEY, VK_BACK, VK_CAPITAL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_F10, VK_F11,
    VK_F12, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_HOME, VK_INSERT,
    VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_NEXT, VK_OEM_1, VK_OEM_2, VK_OEM_3,
    VK_OEM_4, VK_OEM_5, VK_OEM_6, VK_OEM_7, VK_OEM_COMMA, VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS,
    VK_PRIOR, VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SPACE, VK_TAB,
    VK_UP,
};

const DEFAULT_ADDR: &str = "0.0.0.0:9000";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_ADDR.to_string());

    let listener = TcpListener::bind(&addr)?;
    println!(
        "extender-host-windows listening on {addr} (protocol v{}, input-only — no video)",
        protocol::PROTOCOL_VERSION
    );
    println!("waiting for a client to connect...");

    for incoming in listener.incoming() {
        let mut stream = match incoming {
            Ok(s) => s,
            Err(e) => {
                eprintln!("accept failed: {e}");
                continue;
            }
        };
        let peer = stream
            .peer_addr()
            .map_or_else(|_| "?".to_string(), |a| a.to_string());
        println!("client connected: {peer}");

        // Handshake: the client's first upstream message is its hello. We only
        // log the requested capture mode — this host always serves input-only and
        // never streams, regardless of what the client asked for.
        if read_hello(&mut stream, &peer).is_none() {
            println!("waiting for a client to connect...");
            continue;
        }

        match serve(stream) {
            Ok(()) => println!("client {peer} disconnected"),
            Err(e) => eprintln!("session with {peer} ended: {e}"),
        }
        println!("waiting for a client to connect...");
    }
    Ok(())
}

/// Read and log the client's [`ClientHello`], tolerating a protocol-version skew
/// the same way the macOS host does. Returns `None` (and logs) on a missing or
/// garbled hello, so the caller skips this client. The advertised size is
/// irrelevant here — without capture there's no display geometry to size.
fn read_hello(stream: &mut TcpStream, peer: &str) -> Option<()> {
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
    println!(
        "client {peer} hello: {}x{}, mode {:?}; serving input-only (no video)",
        hello.width, hello.height, hello.capture_mode
    );
    Some(())
}

/// Read input events from the client and inject them into the local desktop until
/// the client disconnects.
fn serve(mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    let _ = stream.set_nodelay(true); // disable Nagle — low latency for input
    while let Ok(input) = protocol::read_framed::<_, Input>(&mut stream) {
        inject(input);
    }
    Ok(())
}

/// Inject one input event via `SendInput`. Keyboard and a best-effort mouse path
/// are handled; pointer-position events are ignored because, with no capture,
/// there's no display geometry to map normalized coordinates into.
fn inject(input: Input) {
    match input {
        Input::Key { code, pressed } => {
            if let Some(vk) = hid_to_windows_vk(code) {
                send_key(vk, pressed);
            }
        }
        Input::Text { text } => send_text(&text),
        Input::MouseButton { button, pressed } => {
            let flags = match (button, pressed) {
                (Button::Left, true) => MOUSEEVENTF_LEFTDOWN,
                (Button::Left, false) => MOUSEEVENTF_LEFTUP,
                (Button::Right, true) => MOUSEEVENTF_RIGHTDOWN,
                (Button::Right, false) => MOUSEEVENTF_RIGHTUP,
                (Button::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
                (Button::Middle, false) => MOUSEEVENTF_MIDDLEUP,
            };
            send_mouse(flags, 0);
        }
        Input::Scroll { dx, dy } => {
            // One wheel notch is WHEEL_DELTA (120) units; treat a line as a notch.
            if dy != 0.0 {
                send_mouse(MOUSEEVENTF_WHEEL, (dy * 120.0) as i32);
            }
            if dx != 0.0 {
                send_mouse(MOUSEEVENTF_HWHEEL, (dx * 120.0) as i32);
            }
        }
        // TODO: MouseMove/MouseMoveRelative/Touch/Gesture need display geometry to
        // map normalized coordinates to screen pixels; this input-only host has no
        // capture and therefore no geometry, so pointer positioning is ignored.
        Input::MouseMove { .. }
        | Input::MouseMoveRelative { .. }
        | Input::Touch { .. }
        | Input::Gesture(_) => {}
    }
}

/// Send a single key down or up via `SendInput`, using the virtual-key code (the
/// system supplies the scan code). Extended keys (arrows, navigation cluster,
/// right-hand modifiers) carry `KEYEVENTF_EXTENDEDKEY` so apps see the real key
/// rather than its numpad twin.
fn send_key(vk: VIRTUAL_KEY, pressed: bool) {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if is_extended(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if !pressed {
        flags |= KEYEVENTF_KEYUP;
    }
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe {
        SendInput(&[input], size_of::<INPUT>() as i32);
    }
}

/// Type Unicode text by sending each UTF-16 code unit as a `KEYEVENTF_UNICODE`
/// key down + up. Surrogate pairs are emitted as two code units; Windows
/// reassembles them into the final character.
fn send_text(text: &str) {
    let mut inputs = Vec::new();
    for unit in text.encode_utf16() {
        for keyup in [false, true] {
            let mut flags = KEYEVENTF_UNICODE;
            if keyup {
                flags |= KEYEVENTF_KEYUP;
            }
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: unit,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
    }
    if !inputs.is_empty() {
        unsafe {
            SendInput(&inputs, size_of::<INPUT>() as i32);
        }
    }
}

/// Send one mouse event with the given flags and `mouse_data` (the wheel delta
/// for scroll events, otherwise 0). The pointer is left where it is (no movement
/// flag), so button/scroll events act at the current cursor position.
fn send_mouse(flags: MOUSE_EVENT_FLAGS, mouse_data: i32) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: mouse_data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe {
        SendInput(&[input], size_of::<INPUT>() as i32);
    }
}

/// Whether a virtual key is an "extended" key, which must carry
/// `KEYEVENTF_EXTENDEDKEY` for `SendInput` to deliver it as the intended key
/// (per the Win32 keyboard scan-code rules).
fn is_extended(vk: VIRTUAL_KEY) -> bool {
    matches!(
        vk,
        VK_RIGHT
            | VK_LEFT
            | VK_UP
            | VK_DOWN
            | VK_PRIOR
            | VK_NEXT
            | VK_HOME
            | VK_END
            | VK_INSERT
            | VK_DELETE
            | VK_RCONTROL
            | VK_RMENU
            | VK_LWIN
            | VK_RWIN
    )
}

/// Map a USB-HID keyboard usage id (the platform-neutral code carried on the
/// wire) to a Windows virtual-key code. Mirrors `hid_to_macos` in the macOS host
/// (`crates/host/src/main.rs`). Returns `None` for keys not yet mapped.
#[rustfmt::skip]
fn hid_to_windows_vk(usage: u32) -> Option<VIRTUAL_KEY> {
    let vk = match usage {
        // Letters a–z -> 'A'..'Z' (VK codes are the ASCII uppercase values).
        0x04..=0x1D => VIRTUAL_KEY(0x41 + (usage - 0x04) as u16),
        // Digits 1–9 -> '1'..'9'.
        0x1E..=0x26 => VIRTUAL_KEY(0x31 + (usage - 0x1E) as u16),
        // 0.
        0x27 => VIRTUAL_KEY(0x30),
        // Enter, Escape, Backspace, Tab, Space.
        0x28 => VK_RETURN, 0x29 => VK_ESCAPE, 0x2A => VK_BACK, 0x2B => VK_TAB, 0x2C => VK_SPACE,
        // Punctuation: - = [ ] \ ; ' ` , . /  and CapsLock (OEM virtual keys).
        0x2D => VK_OEM_MINUS, 0x2E => VK_OEM_PLUS, 0x2F => VK_OEM_4, 0x30 => VK_OEM_6,
        0x31 => VK_OEM_5, 0x33 => VK_OEM_1, 0x34 => VK_OEM_7, 0x35 => VK_OEM_3,
        0x36 => VK_OEM_COMMA, 0x37 => VK_OEM_PERIOD, 0x38 => VK_OEM_2, 0x39 => VK_CAPITAL,
        // Arrows: right, left, down, up.
        0x4F => VK_RIGHT, 0x50 => VK_LEFT, 0x51 => VK_DOWN, 0x52 => VK_UP,
        // Navigation: PageUp/PageDown (slide back/forward), Home, End, Insert, Delete(fwd).
        0x4B => VK_PRIOR, 0x4E => VK_NEXT, 0x4A => VK_HOME, 0x4D => VK_END,
        0x49 => VK_INSERT, 0x4C => VK_DELETE,
        // Function keys F1–F12.
        0x3A => VK_F1, 0x3B => VK_F2, 0x3C => VK_F3, 0x3D => VK_F4, 0x3E => VK_F5, 0x3F => VK_F6,
        0x40 => VK_F7, 0x41 => VK_F8, 0x42 => VK_F9, 0x43 => VK_F10, 0x44 => VK_F11, 0x45 => VK_F12,
        // Modifiers: L/R control, shift, alt, gui (Windows key).
        0xE0 => VK_LCONTROL, 0xE1 => VK_LSHIFT, 0xE2 => VK_LMENU, 0xE3 => VK_LWIN,
        0xE4 => VK_RCONTROL, 0xE5 => VK_RSHIFT, 0xE6 => VK_RMENU, 0xE7 => VK_RWIN,
        _ => return None,
    };
    Some(vk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_map_to_ascii_uppercase_vks() {
        assert_eq!(hid_to_windows_vk(0x04), Some(VIRTUAL_KEY(0x41))); // a -> 'A'
        assert_eq!(hid_to_windows_vk(0x1D), Some(VIRTUAL_KEY(0x5A))); // z -> 'Z'
                                                                      // 'B' is the PowerPoint screen-blank key.
        assert_eq!(hid_to_windows_vk(0x05), Some(VIRTUAL_KEY(0x42)));
    }

    #[test]
    fn digits_map_to_ascii_number_vks() {
        assert_eq!(hid_to_windows_vk(0x1E), Some(VIRTUAL_KEY(0x31))); // 1
        assert_eq!(hid_to_windows_vk(0x26), Some(VIRTUAL_KEY(0x39))); // 9
        assert_eq!(hid_to_windows_vk(0x27), Some(VIRTUAL_KEY(0x30))); // 0
    }

    #[test]
    fn clicker_navigation_keys_map() {
        assert_eq!(hid_to_windows_vk(0x4B), Some(VK_PRIOR)); // PageUp -> previous
        assert_eq!(hid_to_windows_vk(0x4E), Some(VK_NEXT)); // PageDown -> next
        assert_eq!(hid_to_windows_vk(0x4A), Some(VK_HOME));
        assert_eq!(hid_to_windows_vk(0x4D), Some(VK_END));
        assert_eq!(hid_to_windows_vk(0x4F), Some(VK_RIGHT));
        assert_eq!(hid_to_windows_vk(0x50), Some(VK_LEFT));
        assert_eq!(hid_to_windows_vk(0x29), Some(VK_ESCAPE)); // end slideshow
    }

    #[test]
    fn function_keys_map() {
        assert_eq!(hid_to_windows_vk(0x3E), Some(VK_F5)); // start slideshow
        assert_eq!(hid_to_windows_vk(0x3A), Some(VK_F1));
        assert_eq!(hid_to_windows_vk(0x45), Some(VK_F12));
    }

    #[test]
    fn blank_keys_map() {
        // '.' (Keynote/Slides blank) and 'b'/'w' (PowerPoint black/white).
        assert_eq!(hid_to_windows_vk(0x37), Some(VK_OEM_PERIOD));
        assert_eq!(hid_to_windows_vk(0x05), Some(VIRTUAL_KEY(0x42))); // b
        assert_eq!(hid_to_windows_vk(0x1A), Some(VIRTUAL_KEY(0x57))); // w
    }

    #[test]
    fn modifiers_map() {
        assert_eq!(hid_to_windows_vk(0xE0), Some(VK_LCONTROL));
        assert_eq!(hid_to_windows_vk(0xE1), Some(VK_LSHIFT));
        assert_eq!(hid_to_windows_vk(0xE2), Some(VK_LMENU));
        assert_eq!(hid_to_windows_vk(0xE3), Some(VK_LWIN));
        assert_eq!(hid_to_windows_vk(0xE7), Some(VK_RWIN));
    }

    #[test]
    fn unmapped_usage_returns_none() {
        assert_eq!(hid_to_windows_vk(0x00), None);
        assert_eq!(hid_to_windows_vk(0xFFFF), None);
    }

    #[test]
    fn arrows_and_nav_are_extended_but_letters_are_not() {
        assert!(is_extended(VK_RIGHT));
        assert!(is_extended(VK_NEXT));
        assert!(!is_extended(VIRTUAL_KEY(0x41))); // 'A'
        assert!(!is_extended(VK_F5));
    }
}
