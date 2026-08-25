//! Input injection on Linux, through the kernel's **uinput** device.
//!
//! This is the Linux twin of the Windows host's `SendInput` block and the macOS
//! host's `CGEvent` posting. The choice of uinput over the alternatives is the
//! load-bearing decision in `docs/LINUX-HOST.md` §3, so it's worth restating:
//!
//! - **XTEST** works only under X11.
//! - **The XDG `RemoteDesktop` portal / libei** is the sanctioned Wayland route,
//!   but needs a compositor that implements it (GNOME 45+, KDE Plasma 6; wlroots
//!   partial) and opens a consent dialog before the first event.
//! - **uinput** creates a virtual input device in the kernel, *below* the display
//!   server. X11, every Wayland compositor, and even the login screen see it as
//!   an ordinary USB keyboard/mouse. One implementation, no display-server
//!   detection, no portal.
//!
//! The price is a permission: `/dev/uinput` is root-owned by default. The
//! packaged host ships a udev rule granting the `input` group write access — see
//! [`uinput_status`], which is what the GUI uses to explain the failure *before*
//! a client connects rather than after the first dead keypress.
//!
//! ⚠️ **uinput emits scancodes, not characters.** The compositor applies the
//! keyboard layout afterwards, so [`Injector::text`] has to spell text out in US
//! QWERTY positions — see its doc comment for what that costs.

use std::io;
use std::path::Path;

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, InputEvent, KeyCode, KeyEvent, RelativeAxisCode, RelativeAxisEvent};
use extender_protocol::{Button, Input};

/// What the kernel will let us do with `/dev/uinput` right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UinputStatus {
    /// Writable — injection will work.
    Ok,
    /// The device node doesn't exist: the `uinput` module isn't loaded.
    Missing,
    /// It exists but we can't open it for writing (the usual case: not in the
    /// `input` group, or the udev rule was never installed).
    Denied,
}

impl UinputStatus {
    /// A one-line explanation for the GUI, or `None` when everything is fine.
    pub fn problem(&self) -> Option<&'static str> {
        match self {
            UinputStatus::Ok => None,
            UinputStatus::Missing => {
                Some("/dev/uinput is missing — run: sudo modprobe uinput")
            }
            UinputStatus::Denied => Some(
                "No permission to write /dev/uinput — install the udev rule, \
                 then log out and back in",
            ),
        }
    }
}

/// Check `/dev/uinput` without creating a device, so the GUI can warn up front.
/// Opening it for write is the only honest test — the permission bits alone
/// don't account for ACLs a udev rule may have set.
pub fn uinput_status() -> UinputStatus {
    let path = Path::new("/dev/uinput");
    if !path.exists() {
        return UinputStatus::Missing;
    }
    match std::fs::OpenOptions::new().write(true).open(path) {
        Ok(_) => UinputStatus::Ok,
        Err(_) => UinputStatus::Denied,
    }
}

/// The udev rule the packaged host installs, and that `uinput_status` is telling
/// the user about when it reports [`UinputStatus::Denied`]. Kept here so the GUI,
/// the docs and the packaging script can't drift apart.
pub const UDEV_RULE: &str =
    "KERNEL==\"uinput\", GROUP=\"input\", MODE=\"0660\", OPTIONS+=\"static_node=uinput\"";
/// Where that rule belongs.
pub const UDEV_RULE_PATH: &str = "/etc/udev/rules.d/99-universal-screens-uinput.rules";

/// Two virtual devices: a keyboard and a pointer.
///
/// They're deliberately separate. libinput classifies a device by the events it
/// advertises, and a single node claiming both a full keyboard and relative axes
/// gets treated as neither cleanly — pointer acceleration in particular is only
/// applied to something that looks like a mouse.
pub struct Injector {
    keyboard: VirtualDevice,
    pointer: VirtualDevice,
    /// Characters [`Injector::text`] could not spell in US QWERTY, for logging.
    dropped_chars: usize,
}

impl Injector {
    /// Create both virtual devices. Fails if `/dev/uinput` isn't writable — call
    /// [`uinput_status`] first if you want to explain *why* rather than just
    /// report an error.
    pub fn new() -> io::Result<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        // Every key the HID map can produce, plus the modifiers, must be declared
        // up front: the kernel rejects an event for a key the device never
        // advertised, silently as far as the caller is concerned.
        for usage in 0..=0xE7u32 {
            if let Some(key) = hid_to_linux_key(usage) {
                keys.insert(key);
            }
        }
        let keyboard = VirtualDevice::builder()?
            .name("Universal Screens Keyboard")
            .with_keys(&keys)?
            .build()?;

        let mut buttons = AttributeSet::<KeyCode>::new();
        buttons.insert(KeyCode::BTN_LEFT);
        buttons.insert(KeyCode::BTN_RIGHT);
        buttons.insert(KeyCode::BTN_MIDDLE);
        let mut axes = AttributeSet::<RelativeAxisCode>::new();
        axes.insert(RelativeAxisCode::REL_X);
        axes.insert(RelativeAxisCode::REL_Y);
        axes.insert(RelativeAxisCode::REL_WHEEL);
        axes.insert(RelativeAxisCode::REL_HWHEEL);
        let pointer = VirtualDevice::builder()?
            .name("Universal Screens Pointer")
            .with_keys(&buttons)?
            .with_relative_axes(&axes)?
            .build()?;

        Ok(Self { keyboard, pointer, dropped_chars: 0 })
    }

    /// Inject one protocol event. Mirrors the Windows host's `inject`: keyboard
    /// and a best-effort mouse path, with absolute pointer positioning ignored
    /// because a control-only host has no display geometry to map into.
    pub fn inject(&mut self, input: Input) {
        match input {
            Input::Key { code, pressed } => {
                if let Some(key) = hid_to_linux_key(code) {
                    self.key(key, pressed);
                }
            }
            Input::Text { text } => self.text(&text),
            Input::MouseButton { button, pressed } => {
                let key = match button {
                    Button::Left => KeyCode::BTN_LEFT,
                    Button::Right => KeyCode::BTN_RIGHT,
                    Button::Middle => KeyCode::BTN_MIDDLE,
                };
                self.emit_pointer(&[KeyEvent::new(key, i32::from(pressed)).into()]);
            }
            Input::Scroll { dx, dy } => {
                // REL_WHEEL is in notches, and its sign matches the protocol's
                // (positive scrolls up) — unlike Windows, no 120-unit delta.
                let mut events: Vec<InputEvent> = Vec::new();
                if dy != 0.0 {
                    events.push(RelativeAxisEvent::new(RelativeAxisCode::REL_WHEEL, dy.round() as i32).into());
                }
                if dx != 0.0 {
                    events.push(RelativeAxisEvent::new(RelativeAxisCode::REL_HWHEEL, dx.round() as i32).into());
                }
                if !events.is_empty() {
                    self.emit_pointer(&events);
                }
            }
            Input::MouseMoveRelative { dx, dy } => {
                let (dx, dy) = (dx.round() as i32, dy.round() as i32);
                if dx == 0 && dy == 0 {
                    return;
                }
                self.emit_pointer(&[
                    RelativeAxisEvent::new(RelativeAxisCode::REL_X, dx).into(),
                    RelativeAxisEvent::new(RelativeAxisCode::REL_Y, dy).into(),
                ]);
            }
            // Absolute pointer positioning is ignored, and Stage 2b's mirror did
            // NOT change that: the Windows host ignores exactly these three too,
            // so remote control is driven by relative motion on all platforms
            // that have it. Honouring them would mean a second uinput device
            // with ABS_X/ABS_Y, which X11 and libinput read as a touchscreen
            // rather than a mouse — a behaviour change worth making deliberately
            // and on real hardware, not as a side effect of adding video.
            Input::MouseMove { .. } | Input::Touch { .. } | Input::Gesture(_) => {}
            // Control requests, handled by the caller rather than injected.
            Input::ScanDeck | Input::ListWindows | Input::FocusWindow { .. } => {}
        }
    }

    /// Press or release one key.
    fn key(&mut self, key: KeyCode, pressed: bool) {
        let event = KeyEvent::new(key, i32::from(pressed));
        let _ = self.keyboard.emit(&[event.into()]);
    }

    /// Tap a key (down then up).
    fn tap(&mut self, key: KeyCode) {
        self.key(key, true);
        self.key(key, false);
    }

    /// Type committed text from a soft keyboard / IME.
    ///
    /// ⚠️ **This is the one place the Linux host is genuinely weaker than the
    /// other two.** Windows has `KEYEVENTF_UNICODE` and macOS has
    /// `CGEventKeyboardSetUnicodeString`; both hand the OS a *character*. uinput
    /// can only hand the kernel a *scancode*, and the compositor then applies
    /// whatever keyboard layout the user has set. So this spells each character
    /// out in **US QWERTY positions**:
    ///
    /// - On a US layout it is exact.
    /// - On another layout the physical keys are right and the resulting
    ///   characters may not be (an AZERTY user asking for `a` gets `q`).
    /// - Anything outside printable ASCII can't be expressed at all and is
    ///   counted in `dropped_chars` rather than silently vanishing.
    ///
    /// For the clicker — the mode this host ships — text is a rare path: the
    /// keystrokes that matter are arrows, PageUp/PageDown and F5, which are
    /// layout-independent. Fixing it properly means either an XKB remap (X11
    /// only) or the portal/libei route (Wayland only), i.e. giving up the single
    /// implementation that makes this host cheap. Deliberately not done here.
    fn text(&mut self, text: &str) {
        for ch in text.chars() {
            match ascii_to_key(ch) {
                Some((key, shifted)) => {
                    if shifted {
                        self.key(KeyCode::KEY_LEFTSHIFT, true);
                        self.tap(key);
                        self.key(KeyCode::KEY_LEFTSHIFT, false);
                    } else {
                        self.tap(key);
                    }
                }
                None => self.dropped_chars += 1,
            }
        }
    }

    /// Emit pointer events. The kernel needs the caller to say where one logical
    /// motion ends, so a `SYN_REPORT` is appended — `emit` does that for us.
    fn emit_pointer(&mut self, events: &[InputEvent]) {
        let _ = self.pointer.emit(events);
    }

    /// How many characters [`Injector::text`] couldn't express, for the caller
    /// to log once at the end of a session rather than per keystroke.
    pub fn dropped_chars(&self) -> usize {
        self.dropped_chars
    }
}

/// Map a USB-HID keyboard usage id (the platform-neutral code carried on the
/// wire) to a Linux `KEY_*` code. Mirrors `hid_to_windows_vk` in the Windows host
/// and `hid_to_macos` in the macOS one.
///
/// ⚠️ Unlike the Windows map, **the letters are not a contiguous range**: Linux
/// keycodes follow the QWERTY *rows* (`KEY_Q` = 16, `KEY_A` = 30, `KEY_Z` = 44),
/// so `0x04 + n` arithmetic silently produces the wrong key. They're spelled out.
#[rustfmt::skip]
pub fn hid_to_linux_key(usage: u32) -> Option<KeyCode> {
    let key = match usage {
        // Letters a–z. Explicit: see the note above.
        0x04 => KeyCode::KEY_A, 0x05 => KeyCode::KEY_B, 0x06 => KeyCode::KEY_C, 0x07 => KeyCode::KEY_D,
        0x08 => KeyCode::KEY_E, 0x09 => KeyCode::KEY_F, 0x0A => KeyCode::KEY_G, 0x0B => KeyCode::KEY_H,
        0x0C => KeyCode::KEY_I, 0x0D => KeyCode::KEY_J, 0x0E => KeyCode::KEY_K, 0x0F => KeyCode::KEY_L,
        0x10 => KeyCode::KEY_M, 0x11 => KeyCode::KEY_N, 0x12 => KeyCode::KEY_O, 0x13 => KeyCode::KEY_P,
        0x14 => KeyCode::KEY_Q, 0x15 => KeyCode::KEY_R, 0x16 => KeyCode::KEY_S, 0x17 => KeyCode::KEY_T,
        0x18 => KeyCode::KEY_U, 0x19 => KeyCode::KEY_V, 0x1A => KeyCode::KEY_W, 0x1B => KeyCode::KEY_X,
        0x1C => KeyCode::KEY_Y, 0x1D => KeyCode::KEY_Z,
        // Digits 1–9 then 0 (KEY_1..KEY_9 are contiguous; KEY_0 follows KEY_9).
        0x1E => KeyCode::KEY_1, 0x1F => KeyCode::KEY_2, 0x20 => KeyCode::KEY_3, 0x21 => KeyCode::KEY_4,
        0x22 => KeyCode::KEY_5, 0x23 => KeyCode::KEY_6, 0x24 => KeyCode::KEY_7, 0x25 => KeyCode::KEY_8,
        0x26 => KeyCode::KEY_9, 0x27 => KeyCode::KEY_0,
        // Enter, Escape, Backspace, Tab, Space.
        0x28 => KeyCode::KEY_ENTER, 0x29 => KeyCode::KEY_ESC, 0x2A => KeyCode::KEY_BACKSPACE,
        0x2B => KeyCode::KEY_TAB, 0x2C => KeyCode::KEY_SPACE,
        // Punctuation: - = [ ] \ ; ' ` , . /  and CapsLock.
        0x2D => KeyCode::KEY_MINUS, 0x2E => KeyCode::KEY_EQUAL, 0x2F => KeyCode::KEY_LEFTBRACE,
        0x30 => KeyCode::KEY_RIGHTBRACE, 0x31 => KeyCode::KEY_BACKSLASH, 0x33 => KeyCode::KEY_SEMICOLON,
        0x34 => KeyCode::KEY_APOSTROPHE, 0x35 => KeyCode::KEY_GRAVE, 0x36 => KeyCode::KEY_COMMA,
        0x37 => KeyCode::KEY_DOT, 0x38 => KeyCode::KEY_SLASH, 0x39 => KeyCode::KEY_CAPSLOCK,
        // Function keys F1–F12.
        0x3A => KeyCode::KEY_F1, 0x3B => KeyCode::KEY_F2, 0x3C => KeyCode::KEY_F3, 0x3D => KeyCode::KEY_F4,
        0x3E => KeyCode::KEY_F5, 0x3F => KeyCode::KEY_F6, 0x40 => KeyCode::KEY_F7, 0x41 => KeyCode::KEY_F8,
        0x42 => KeyCode::KEY_F9, 0x43 => KeyCode::KEY_F10, 0x44 => KeyCode::KEY_F11, 0x45 => KeyCode::KEY_F12,
        // Navigation: Insert, Home, PageUp, Delete, End, PageDown.
        0x49 => KeyCode::KEY_INSERT, 0x4A => KeyCode::KEY_HOME, 0x4B => KeyCode::KEY_PAGEUP,
        0x4C => KeyCode::KEY_DELETE, 0x4D => KeyCode::KEY_END, 0x4E => KeyCode::KEY_PAGEDOWN,
        // Arrows: right, left, down, up.
        0x4F => KeyCode::KEY_RIGHT, 0x50 => KeyCode::KEY_LEFT, 0x51 => KeyCode::KEY_DOWN, 0x52 => KeyCode::KEY_UP,
        // Modifiers: L/R control, shift, alt, gui (the "super"/Windows key).
        0xE0 => KeyCode::KEY_LEFTCTRL, 0xE1 => KeyCode::KEY_LEFTSHIFT, 0xE2 => KeyCode::KEY_LEFTALT,
        0xE3 => KeyCode::KEY_LEFTMETA, 0xE4 => KeyCode::KEY_RIGHTCTRL, 0xE5 => KeyCode::KEY_RIGHTSHIFT,
        0xE6 => KeyCode::KEY_RIGHTALT, 0xE7 => KeyCode::KEY_RIGHTMETA,
        _ => return None,
    };
    Some(key)
}

/// Map a printable ASCII character to the US-QWERTY key that produces it, and
/// whether Shift is needed. `None` for anything else — see [`Injector::text`].
#[rustfmt::skip]
fn ascii_to_key(ch: char) -> Option<(KeyCode, bool)> {
    let pair = match ch {
        'a'..='z' => (hid_to_linux_key(0x04 + (ch as u32 - 'a' as u32))?, false),
        'A'..='Z' => (hid_to_linux_key(0x04 + (ch as u32 - 'A' as u32))?, true),
        '1'..='9' => (hid_to_linux_key(0x1E + (ch as u32 - '1' as u32))?, false),
        '0' => (KeyCode::KEY_0, false),
        ' ' => (KeyCode::KEY_SPACE, false),
        '\n' | '\r' => (KeyCode::KEY_ENTER, false),
        '\t' => (KeyCode::KEY_TAB, false),
        // Unshifted punctuation.
        '-' => (KeyCode::KEY_MINUS, false),      '=' => (KeyCode::KEY_EQUAL, false),
        '[' => (KeyCode::KEY_LEFTBRACE, false),  ']' => (KeyCode::KEY_RIGHTBRACE, false),
        '\\' => (KeyCode::KEY_BACKSLASH, false), ';' => (KeyCode::KEY_SEMICOLON, false),
        '\'' => (KeyCode::KEY_APOSTROPHE, false),'`' => (KeyCode::KEY_GRAVE, false),
        ',' => (KeyCode::KEY_COMMA, false),      '.' => (KeyCode::KEY_DOT, false),
        '/' => (KeyCode::KEY_SLASH, false),
        // Shifted punctuation, in US-QWERTY positions.
        '!' => (KeyCode::KEY_1, true),  '@' => (KeyCode::KEY_2, true),  '#' => (KeyCode::KEY_3, true),
        '$' => (KeyCode::KEY_4, true),  '%' => (KeyCode::KEY_5, true),  '^' => (KeyCode::KEY_6, true),
        '&' => (KeyCode::KEY_7, true),  '*' => (KeyCode::KEY_8, true),  '(' => (KeyCode::KEY_9, true),
        ')' => (KeyCode::KEY_0, true),  '_' => (KeyCode::KEY_MINUS, true),
        '+' => (KeyCode::KEY_EQUAL, true),       '{' => (KeyCode::KEY_LEFTBRACE, true),
        '}' => (KeyCode::KEY_RIGHTBRACE, true),  '|' => (KeyCode::KEY_BACKSLASH, true),
        ':' => (KeyCode::KEY_SEMICOLON, true),   '"' => (KeyCode::KEY_APOSTROPHE, true),
        '~' => (KeyCode::KEY_GRAVE, true),       '<' => (KeyCode::KEY_COMMA, true),
        '>' => (KeyCode::KEY_DOT, true),         '?' => (KeyCode::KEY_SLASH, true),
        _ => return None,
    };
    Some(pair)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_do_not_follow_the_hid_range_arithmetic() {
        // The whole point of spelling the table out: 0x04 + n would give KEY_A + n,
        // and KEY_A + 1 is KEY_S, not KEY_B.
        assert_eq!(hid_to_linux_key(0x04), Some(KeyCode::KEY_A));
        assert_eq!(hid_to_linux_key(0x05), Some(KeyCode::KEY_B)); // 'b' — PowerPoint blank
        assert_eq!(hid_to_linux_key(0x1D), Some(KeyCode::KEY_Z));
        assert_ne!(KeyCode::KEY_B.0, KeyCode::KEY_A.0 + 1);
    }

    #[test]
    fn digits_map_in_order_with_zero_last() {
        assert_eq!(hid_to_linux_key(0x1E), Some(KeyCode::KEY_1));
        assert_eq!(hid_to_linux_key(0x26), Some(KeyCode::KEY_9));
        assert_eq!(hid_to_linux_key(0x27), Some(KeyCode::KEY_0));
    }

    #[test]
    fn clicker_navigation_keys_map() {
        assert_eq!(hid_to_linux_key(0x4B), Some(KeyCode::KEY_PAGEUP)); // previous slide
        assert_eq!(hid_to_linux_key(0x4E), Some(KeyCode::KEY_PAGEDOWN)); // next slide
        assert_eq!(hid_to_linux_key(0x4F), Some(KeyCode::KEY_RIGHT));
        assert_eq!(hid_to_linux_key(0x50), Some(KeyCode::KEY_LEFT));
        assert_eq!(hid_to_linux_key(0x4A), Some(KeyCode::KEY_HOME));
        assert_eq!(hid_to_linux_key(0x4D), Some(KeyCode::KEY_END));
        assert_eq!(hid_to_linux_key(0x29), Some(KeyCode::KEY_ESC)); // end slideshow
        assert_eq!(hid_to_linux_key(0x3E), Some(KeyCode::KEY_F5)); // start slideshow
    }

    #[test]
    fn blank_keys_map() {
        // '.' (Keynote/Slides blank) and 'b'/'w' (PowerPoint black/white).
        assert_eq!(hid_to_linux_key(0x37), Some(KeyCode::KEY_DOT));
        assert_eq!(hid_to_linux_key(0x05), Some(KeyCode::KEY_B));
        assert_eq!(hid_to_linux_key(0x1A), Some(KeyCode::KEY_W));
    }

    #[test]
    fn modifiers_map_gui_to_meta() {
        assert_eq!(hid_to_linux_key(0xE0), Some(KeyCode::KEY_LEFTCTRL));
        assert_eq!(hid_to_linux_key(0xE1), Some(KeyCode::KEY_LEFTSHIFT));
        assert_eq!(hid_to_linux_key(0xE2), Some(KeyCode::KEY_LEFTALT));
        // HID "GUI" is the Windows key on Windows and Super/Meta here.
        assert_eq!(hid_to_linux_key(0xE3), Some(KeyCode::KEY_LEFTMETA));
        assert_eq!(hid_to_linux_key(0xE7), Some(KeyCode::KEY_RIGHTMETA));
    }

    #[test]
    fn unmapped_usage_returns_none() {
        assert_eq!(hid_to_linux_key(0x00), None);
        assert_eq!(hid_to_linux_key(0x32), None); // non-US hash — not mapped
        assert_eq!(hid_to_linux_key(0xFFFF), None);
    }

    #[test]
    fn text_spells_ascii_in_us_qwerty_positions() {
        assert_eq!(ascii_to_key('a'), Some((KeyCode::KEY_A, false)));
        assert_eq!(ascii_to_key('A'), Some((KeyCode::KEY_A, true)));
        assert_eq!(ascii_to_key('!'), Some((KeyCode::KEY_1, true)));
        assert_eq!(ascii_to_key('?'), Some((KeyCode::KEY_SLASH, true)));
        assert_eq!(ascii_to_key(' '), Some((KeyCode::KEY_SPACE, false)));
        assert_eq!(ascii_to_key('0'), Some((KeyCode::KEY_0, false)));
        assert_eq!(ascii_to_key(')'), Some((KeyCode::KEY_0, true)));
    }

    #[test]
    fn text_cannot_express_non_ascii() {
        // The documented limitation, asserted so it can't regress into silence.
        assert_eq!(ascii_to_key('é'), None);
        assert_eq!(ascii_to_key('€'), None);
        assert_eq!(ascii_to_key('👍'), None);
    }

    #[test]
    fn uinput_problem_text_is_actionable() {
        assert!(UinputStatus::Ok.problem().is_none());
        assert!(UinputStatus::Missing.problem().unwrap().contains("modprobe"));
        assert!(UinputStatus::Denied.problem().unwrap().contains("udev"));
    }
}
