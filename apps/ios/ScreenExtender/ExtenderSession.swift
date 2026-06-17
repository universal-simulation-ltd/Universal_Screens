import Foundation

/// Swift wrapper over the C FFI in `extender_ffi.h` (the `extender-mobile-ffi`
/// crate). Owns the opaque session pointer. Connect off the main thread — it does
/// blocking network I/O.
///
/// For the clicker, `startPump` drains downstream events (slide previews, host
/// identity, window list) on a background thread and delivers them on the main
/// queue. The Start/Frame video events are ignored here (a future viewer mode
/// would feed them to VideoToolbox).
final class ExtenderSession {
    /// Mirrors `EXTENDER_CAPTURE_*` in `extender_ffi.h`.
    enum CaptureMode: UInt32 {
        case virtualDisplay = 0
        case mirror = 1
        case controlOnly = 2
    }

    private var handle: OpaquePointer?
    private var pumpThread: Thread?
    private var pumping = false

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

    // MARK: - Downstream events

    /// Sink for downstream events. The clicker callbacks (`onSnapshot` /
    /// `onHostInfo` / `onWindowList` / `onEnded`) are delivered on the main queue;
    /// the video callbacks (`onStart` / `onFrame`) are delivered on the pump thread
    /// so decoding stays off the main thread (they don't touch SwiftUI state).
    struct Sink {
        /// Stream start: `codec` is 0 = H.264, 1 = HEVC; `csd` = Annex-B param sets.
        var onStart: (_ width: Int, _ height: Int, _ codec: Int, _ csd: Data) -> Void = { _, _, _, _ in }
        /// One encoded frame: `data` = Annex-B NAL units.
        var onFrame: (_ data: Data, _ keyframe: Bool, _ ptsValue: Int64) -> Void = { _, _, _ in }
        /// A slide preview: `slot` is 0 = current, -1 = previous, +1 = next; `jpeg`
        /// is empty when there's no slide there.
        var onSnapshot: (_ slot: Int32, _ jpeg: Data) -> Void = { _, _ in }
        var onHostInfo: (_ os: String, _ name: String) -> Void = { _, _ in }
        var onWindowList: (_ windows: [(id: Int64, title: String)]) -> Void = { _ in }
        var onEnded: () -> Void = {}
    }

    /// Drain events on a background thread until the stream ends. Idempotent.
    func startPump(_ sink: Sink) {
        guard let handle, !pumping else { return }
        pumping = true
        let thread = Thread { [weak self] in
            while self?.pumping == true {
                guard let event = extender_session_next_event(handle) else { break }
                let kind = extender_event_kind(event)
                let data = Self.eventData(event)
                switch kind {
                case EXTENDER_EVENT_START:
                    let w = Int(extender_event_width(event))
                    let h = Int(extender_event_height(event))
                    let codec = Int(extender_event_codec(event))
                    sink.onStart(w, h, codec, data) // pump thread (decoder is off-main)
                case EXTENDER_EVENT_FRAME:
                    sink.onFrame(data, extender_event_keyframe(event), extender_event_pts_value(event))
                case EXTENDER_EVENT_SNAPSHOT:
                    let slot = extender_event_slot(event)
                    DispatchQueue.main.async { sink.onSnapshot(slot, data) }
                case EXTENDER_EVENT_HOSTINFO:
                    let (os, name) = Self.split2(data, separator: "\n")
                    DispatchQueue.main.async { sink.onHostInfo(os, name) }
                case EXTENDER_EVENT_WINDOWLIST:
                    let windows = Self.parseWindows(data)
                    DispatchQueue.main.async { sink.onWindowList(windows) }
                default:
                    break // unknown event kind
                }
                extender_event_free(event)
            }
            DispatchQueue.main.async { sink.onEnded() }
        }
        thread.name = "extender-pump"
        pumpThread = thread
        thread.start()
    }

    // MARK: - Input

    /// A key tap: down then up. The clicker's workhorse.
    func tapKey(_ hid: UInt32) {
        guard let handle else { return }
        extender_send_key(handle, hid, true)
        extender_send_key(handle, hid, false)
    }

    func sendText(_ text: String) {
        guard let handle else { return }
        text.withCString { extender_send_text(handle, $0) }
    }

    func sendTouch(id: UInt32, phase: ExtenderTouchPhase, x: Float, y: Float) {
        guard let handle else { return }
        extender_send_touch(handle, id, phase, x, y)
    }

    /// Pre-scan the open document so the host can preview adjacent slides.
    func scanDeck() {
        guard let handle else { return }
        extender_send_scan_deck(handle)
    }

    /// (Re)request the host's window list (a window-list event follows).
    func listWindows() {
        guard let handle else { return }
        extender_send_list_windows(handle)
    }

    /// Bring host window `id` to the foreground; `startShow` also starts its slideshow.
    func focusWindow(id: Int64, startShow: Bool) {
        guard let handle else { return }
        extender_send_focus_window(handle, id, startShow)
    }

    // MARK: - Lifecycle

    func close() {
        pumping = false
        guard let handle else { return }
        // NOTE: the pump may still be blocked in extender_session_next_event; the
        // session free shuts the socket so it returns and the thread exits. (Same
        // lifecycle caveat as the Android pump.)
        extender_session_free(handle)
        self.handle = nil
    }

    deinit { close() }

    // MARK: - Helpers

    private static func eventData(_ event: OpaquePointer) -> Data {
        var len = 0
        guard let ptr = extender_event_data(event, &len), len > 0 else { return Data() }
        return Data(bytes: ptr, count: len)
    }

    private static func split2(_ data: Data, separator: Character) -> (String, String) {
        let text = String(decoding: data, as: UTF8.self)
        let parts = text.split(separator: separator, maxSplits: 1, omittingEmptySubsequences: false)
        return (parts.first.map(String.init) ?? "", parts.count > 1 ? String(parts[1]) : "")
    }

    private static func parseWindows(_ data: Data) -> [(id: Int64, title: String)] {
        let text = String(decoding: data, as: UTF8.self)
        guard !text.isEmpty else { return [] }
        return text.split(separator: "\n").compactMap { line in
            guard let tab = line.firstIndex(of: "\t"), let id = Int64(line[..<tab]) else { return nil }
            return (id, String(line[line.index(after: tab)...]))
        }
    }
}
