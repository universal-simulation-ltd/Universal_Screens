import SwiftUI

/// Root view: show the connect screen until a session is live, then the clicker.
struct ContentView: View {
    @State private var session: ExtenderSession?
    @State private var currentAddr = ""
    @State private var status = ""

    var body: some View {
        if let session {
            ClickerView(session: session, addr: currentAddr) {
                session.close()
                self.session = nil
            }
        } else {
            ConnectView(status: status) { addr in connect(to: addr) }
        }
    }

    private func connect(to addr: String) {
        currentAddr = addr
        status = "connecting…"
        // Blocking I/O — connect off the main thread.
        DispatchQueue.global(qos: .userInitiated).async {
            // The clicker uses control-only (input only, no video).
            let s = ExtenderSession.connect(addr: addr, mode: .controlOnly)
            DispatchQueue.main.async {
                // Remember a host that connected; its OS/name fill in from HostInfo.
                if s != nil { ConnectionStore.remember(addr: addr) }
                session = s
                status = (s == nil) ? "connection failed" : ""
            }
        }
    }
}
