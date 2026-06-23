// Browser `KeyboardEvent.code` → USB-HID keyboard usage id. The 4th copy of the
// same map: the native client's `key_to_hid` (crates/client), `HidKeys.kt`,
// `HidKeys.swift`. Keep them in sync (M7 doc flags a single generated source as
// future work). The host maps the HID usage id to its OS keycode.
//
// Browser `.code` is the physical-key identifier (layout-independent), matching
// what the desktop client derives from winit's `KeyCode`. Printable text / IME
// commits do NOT come through here — they use `Input::Text` (M7d).
export const HID = {
  // Letters
  KeyA: 0x04, KeyB: 0x05, KeyC: 0x06, KeyD: 0x07, KeyE: 0x08, KeyF: 0x09,
  KeyG: 0x0a, KeyH: 0x0b, KeyI: 0x0c, KeyJ: 0x0d, KeyK: 0x0e, KeyL: 0x0f,
  KeyM: 0x10, KeyN: 0x11, KeyO: 0x12, KeyP: 0x13, KeyQ: 0x14, KeyR: 0x15,
  KeyS: 0x16, KeyT: 0x17, KeyU: 0x18, KeyV: 0x19, KeyW: 0x1a, KeyX: 0x1b,
  KeyY: 0x1c, KeyZ: 0x1d,
  // Digits (top row)
  Digit1: 0x1e, Digit2: 0x1f, Digit3: 0x20, Digit4: 0x21, Digit5: 0x22,
  Digit6: 0x23, Digit7: 0x24, Digit8: 0x25, Digit9: 0x26, Digit0: 0x27,
  // Editing / whitespace
  Enter: 0x28, Escape: 0x29, Backspace: 0x2a, Tab: 0x2b, Space: 0x2c,
  // Punctuation
  Minus: 0x2d, Equal: 0x2e, BracketLeft: 0x2f, BracketRight: 0x30,
  Backslash: 0x31, Semicolon: 0x33, Quote: 0x34, Backquote: 0x35,
  Comma: 0x36, Period: 0x37, Slash: 0x38, CapsLock: 0x39,
  // Navigation
  ArrowRight: 0x4f, ArrowLeft: 0x50, ArrowDown: 0x51, ArrowUp: 0x52,
  PageUp: 0x4b, PageDown: 0x4e, Home: 0x4a, End: 0x4d, Insert: 0x49, Delete: 0x4c,
  // Function row
  F1: 0x3a, F2: 0x3b, F3: 0x3c, F4: 0x3d, F5: 0x3e, F6: 0x3f,
  F7: 0x40, F8: 0x41, F9: 0x42, F10: 0x43, F11: 0x44, F12: 0x45,
  // Modifiers (browser uses Meta* where winit has Super*)
  ControlLeft: 0xe0, ShiftLeft: 0xe1, AltLeft: 0xe2, MetaLeft: 0xe3,
  ControlRight: 0xe4, ShiftRight: 0xe5, AltRight: 0xe6, MetaRight: 0xe7,
};

/// HID usage id for a `KeyboardEvent.code`, or undefined if unmapped.
export function hidFor(code) {
  return HID[code];
}
