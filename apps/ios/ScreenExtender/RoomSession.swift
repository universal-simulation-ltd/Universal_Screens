import Foundation

/// "Cast to a browser": joins a receiver tab's rendezvous room over a WebSocket
/// (as `role=sender`) and relays control input to it as JSON control frames — the
/// shared protocol in `opensource-portal/public/screens/control.js`. The browser
/// tab is the receiver/screen; this phone is the source/remote.
///
/// Implements `InputTarget` so the existing `TrackpadView` and the cast clicker
/// drive it exactly like a host `ExtenderSession`. Mirrors the Android
/// `RoomSession`: connect without blocking, `Listener` callbacks on the main queue.
///
/// Requires the rendezvous Worker (`wsBase`) to be deployed (opensource-portal).
final class RoomSession: InputTarget {

    struct Listener {
        var onStatus: (String) -> Void = { _ in }
        var onPaired: (_ peerRole: String?) -> Void = { _ in }
        var onPeerLeft: () -> Void = {}
        var onClosed: (_ reason: String) -> Void = { _ in }
    }

    /// The rendezvous lives on the Universal Screens site Worker.
    static let wsBase = "wss://opensource.unisim.co.uk"

    private let task: URLSessionWebSocketTask
    private let listener: Listener
    private var closed = false

    private init(task: URLSessionWebSocketTask, listener: Listener) {
        self.task = task
        self.listener = listener
    }

    /// Join `code` as the sender. Returns immediately; `listener` fires on the main queue.
    static func connect(code: String, listener: Listener) -> RoomSession {
        let normalised = code.uppercased()
        let url = URL(string: "\(wsBase)/screens/room?code=\(normalised)&role=sender")!
        let task = URLSession(configuration: .default).webSocketTask(with: url)
        let room = RoomSession(task: task, listener: listener)
        task.resume()
        DispatchQueue.main.async { listener.onStatus("Connected — waiting for the receiver…") }
        room.receiveLoop()
        room.schedulePing()
        return room
    }

    /// Tell the receiver what kind of controller this is ("trackpad" | "clicker").
    func hello(mode: String) { send(["t": "hello", "mode": mode]) }

    // MARK: - InputTarget (serialise to the control protocol)

    func sendMouseMoveRelative(dx: Float, dy: Float) {
        send(["t": "move", "dx": Double(dx), "dy": Double(dy)])
    }

    func sendMouseButton(button: Int32, pressed: Bool) {
        send(["t": "btn", "b": Int(button), "down": pressed])
    }

    func sendScroll(dx: Float, dy: Float) {
        send(["t": "scroll", "dx": Double(dx), "dy": Double(dy)])
    }

    func tapKey(_ hid: UInt32) {
        guard let k = keyName(hid) else { return }
        send(["t": "key", "k": k])
    }

    func close() {
        guard !closed else { return }
        closed = true
        task.cancel(with: .normalClosure, reason: nil)
    }

    // MARK: - Internals

    /// Only room signals (`waiting`/`paired`/`peer-left`) travel sender-ward; the
    /// receiver never sends control frames back, so a `type` key is all we read.
    private func handle(_ text: String) {
        guard let data = text.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let type = obj["type"] as? String else { return }
        switch type {
        case "paired":
            let peer = (obj["peerRole"] as? String).flatMap { $0.isEmpty ? nil : $0 }
            DispatchQueue.main.async { self.listener.onPaired(peer) }
        case "waiting":
            DispatchQueue.main.async { self.listener.onStatus("Connected — waiting for the receiver…") }
        case "peer-left":
            DispatchQueue.main.async { self.listener.onPeerLeft() }
        default:
            break
        }
    }

    private func receiveLoop() {
        task.receive { [weak self] result in
            guard let self, !self.closed else { return }
            switch result {
            case .failure(let error):
                self.fail(error.localizedDescription)
            case .success(let message):
                if case let .string(text) = message { self.handle(text) }
                self.receiveLoop()
            }
        }
    }

    /// Keep the (hibernatable) DO socket alive, matching Android's 20s ping.
    private func schedulePing() {
        DispatchQueue.global().asyncAfter(deadline: .now() + 20) { [weak self] in
            guard let self, !self.closed else { return }
            self.task.sendPing { _ in }
            self.schedulePing()
        }
    }

    private func fail(_ reason: String) {
        guard !closed else { return }
        closed = true
        let text = reason.isEmpty ? "Disconnected" : reason
        DispatchQueue.main.async { self.listener.onClosed(text) }
    }

    private func send(_ obj: [String: Any]) {
        guard !closed,
              let data = try? JSONSerialization.data(withJSONObject: obj),
              let text = String(data: data, encoding: .utf8) else { return }
        task.send(.string(text)) { _ in }
    }

    /// Map the clicker's HID usage ids onto the receiver's deck actions.
    private func keyName(_ hid: UInt32) -> String? {
        switch hid {
        case HidKeys.pageDown: return "next"
        case HidKeys.pageUp:   return "prev"
        case HidKeys.home:     return "first"
        case HidKeys.end:      return "last"
        case HidKeys.b:        return "blankB"
        case HidKeys.period:   return "blankDot"
        case HidKeys.f5:       return "startF5"
        case HidKeys.escape:   return "endEsc"
        default:               return nil
        }
    }

    deinit { close() }
}
