import Foundation

/// The input surface a control UI (Trackpad / clicker) drives. Decouples those
/// screens from *how* the input travels:
///
///  • `ExtenderSession` sends it to a native host over the binary protocol (TCP).
///  • `RoomSession` serialises it to JSON and relays it to a browser receiver
///    through the cloud rendezvous ("cast to a browser", M8c).
///
/// Mirrors the Android `InputTarget` interface — deliberately the smallest set of
/// methods the reusable `TrackpadView`/`TrackpadUIView` and the clicker buttons
/// need, so the trackpad gesture code is shared across both transports for free.
protocol InputTarget: AnyObject {
    /// Relative cursor move (trackpad).
    func sendMouseMoveRelative(dx: Float, dy: Float)

    /// Mouse button down/up. `button`: 0 = left, 1 = right, 2 = middle.
    func sendMouseButton(button: Int32, pressed: Bool)

    /// Scroll by a delta.
    func sendScroll(dx: Float, dy: Float)

    /// A key tap (down then up) by USB-HID usage id — the clicker's workhorse.
    func tapKey(_ hid: UInt32)
}

/// `ExtenderSession` already exposes these exact signatures (native host path), so
/// conformance is free.
extension ExtenderSession: InputTarget {}
