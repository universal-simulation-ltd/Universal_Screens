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
        // A scanned `…/screens/connect` Universal Link (or the legacy
        // `unisimscreens://` scheme) opens the app straight to a connection. Needs
        // the `applinks:opensource.unisim.co.uk` Associated-Domains entitlement +
        // the domain's apple-app-site-association — see web/README.md (deferred
        // until the iOS app is built; the parsing is wired here so it's ready).
        content.onOpenURL { url in handleDeepLink(url) }
    }

    @ViewBuilder private var content: some View {
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

    /// Connect from a deep link: pull "host:port" out and connect in the default
    /// (clicker) mode. NOTE: unlike the Android client, this scaffold doesn't yet
    /// use the PIN or join the host's Wi-Fi from the link — that parity is a TODO
    /// once the iOS app is fleshed out (the host already encodes both; see the
    /// fragment of the host's connect_url).
    private func handleDeepLink(_ url: URL) {
        guard let addr = connectAddr(from: url) else { return }
        connect(to: addr, mode: .clicker)
    }

    /// Extract "host:port" from a connect deep link, or nil if it isn't one.
    /// Accepts the https `…/screens/connect?host=&port=…` Universal Link and the
    /// legacy `unisimscreens://connect?host=&port=…` custom scheme.
    private func connectAddr(from url: URL) -> String? {
        let scheme = url.scheme?.lowercased()
        let isConnect = (scheme == "https" && url.path.hasPrefix("/screens/connect"))
            || (scheme == "unisimscreens")
        guard isConnect else { return nil }
        let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        guard let host = items.first(where: { $0.name == "host" })?.value, !host.isEmpty
        else { return nil }
        let port = items.first(where: { $0.name == "port" })?.value ?? "9000"
        return "\(host):\(port)"
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
