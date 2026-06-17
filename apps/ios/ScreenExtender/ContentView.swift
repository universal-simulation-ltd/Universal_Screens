import SwiftUI

/// The three ways to use the app; they differ only in UI + whether they stream.
enum Mode {
    case clicker, viewer, control
}

/// Root view: show the connect screen until a session is live, then the chosen
/// mode's screen.
struct ContentView: View {
    @State private var session: ExtenderSession?
    @State private var mode: Mode = .clicker
    @State private var currentAddr = ""
    @State private var status = ""

    var body: some View {
        if let session {
            switch mode {
            case .clicker:
                ClickerView(session: session, addr: currentAddr, onDisconnect: disconnect)
            case .viewer:
                StreamView(session: session, addr: currentAddr, forwardInput: false, onDisconnect: disconnect)
            case .control:
                StreamView(session: session, addr: currentAddr, forwardInput: true, onDisconnect: disconnect)
            }
        } else {
            ConnectView(status: status) { addr, chosen in connect(to: addr, mode: chosen) }
        }
    }

    private func disconnect() {
        session?.close()
        session = nil
    }

    private func connect(to addr: String, mode chosen: Mode) {
        mode = chosen
        currentAddr = addr
        status = "connecting…"
        // Clicker is control-only (no video); viewer / control mirror the screen.
        let capture: ExtenderSession.CaptureMode = (chosen == .clicker) ? .controlOnly : .mirror
        DispatchQueue.global(qos: .userInitiated).async {
            let s = ExtenderSession.connect(addr: addr, mode: capture)
            DispatchQueue.main.async {
                if s != nil { ConnectionStore.remember(addr: addr) }
                session = s
                status = (s == nil) ? "connection failed" : ""
            }
        }
    }
}
