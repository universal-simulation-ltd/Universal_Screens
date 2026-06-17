import Foundation

/// Swift wrapper over the C FFI in `extender_ffi.h` (the `extender-mobile-ffi`
/// crate). Owns the opaque session pointer. Connect off the main thread — it does
/// blocking network I/O.
///
/// The clicker only sends input, so this shell doesn't pump downstream events.
/// The streaming (viewer / full-control) modes will add an event-pump that feeds
/// `VideoToolbox`; see `startDrain()` for the skeleton.
final class ExtenderSession {
    /// Mirrors `EXTENDER_CAPTURE_*` in `extender_ffi.h`.
    enum CaptureMode: UInt32 {
        case virtualDisplay = 0
        case mirror = 1
        case controlOnly = 2
    }

    private var handle: OpaquePointer?

    private init(handle: OpaquePointer) { self.handle = handle }

    /// Blocking connect; returns `nil` on failure. Call off the main thread.
    static func connect(
        addr: String,
        width: UInt32 = 1920,
        height: UInt32 = 1080,
        mode: CaptureMode
    ) -> ExtenderSession? {
        let handle = addr.withCString {
            extender_session_connect($0, width, height, mode.rawValue)
        }
        guard let handle else { return nil }
        return ExtenderSession(handle: handle)
    }

    // MARK: - Input

    /// A key tap: down then up. The clicker's workhorse.
    func tapKey(_ hid: UInt32) {
        guard let handle else { return }
        extender_send_key(handle, hid, true)
        extender_send_key(handle, hid, false)
    }

    func sendKey(_ hid: UInt32, pressed: Bool) {
        guard let handle else { return }
        extender_send_key(handle, hid, pressed)
    }

    func sendText(_ text: String) {
        guard let handle else { return }
        text.withCString { extender_send_text(handle, $0) }
    }

    func sendTouch(id: UInt32, phase: ExtenderTouchPhase, x: Float, y: Float) {
        guard let handle else { return }
        extender_send_touch(handle, id, phase, x, y)
    }

    // MARK: - Lifecycle

    func close() {
        guard let handle else { return }
        extender_session_free(handle)
        self.handle = nil
    }

    deinit { close() }

    // MARK: - Downstream (video modes — TODO)

    /// Skeleton for the streaming modes: drain events on a background thread and
    /// hand Start/Frame buffers (Annex-B) to a VideoToolbox decoder. Unused by the
    /// clicker. NOTE: stop/free ordering must be handled before wiring this up so
    /// the pump isn't blocked in `extender_session_next_event` when the session is
    /// freed.
    func startDrain() {
        // Intentionally not implemented in the shell.
    }
}
