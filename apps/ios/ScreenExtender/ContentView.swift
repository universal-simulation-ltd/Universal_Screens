import SwiftUI

/// Root view: show the connect screen until a session is live, then the clicker.
struct ContentView: View {
    @State private var session: ExtenderSession?
    @State private var addr = "127.0.0.1:9000"
    @State private var status = ""

    var body: some View {
        if let session {
            ClickerView(session: session) {
                session.close()
                self.session = nil
            }
        } else {
            ConnectView(addr: $addr, status: status, onConnect: connect)
        }
    }

    private func connect() {
        status = "connecting…"
        let target = addr
        // Blocking I/O — connect off the main thread.
        DispatchQueue.global(qos: .userInitiated).async {
            // The clicker uses control-only (input only, no video). Viewer /
            // full-control come with the VideoToolbox decode path later.
            let s = ExtenderSession.connect(addr: target, mode: .controlOnly)
            DispatchQueue.main.async {
                session = s
                status = (s == nil) ? "connection failed" : ""
            }
        }
    }
}
