import SwiftUI

/// The five ways to use the app; differ in UI + what the session streams.
enum Mode: String {
    case clicker, viewer, control, trackpad, secondScreen
}

/// A connection target decoded from a scanned QR or a deep link.
struct ConnectPayload {
    var addr: String
    var pin: Int
    var ssid: String?
    var pass: String?
    var auth: String
}

/// Parse any connect payload into a `ConnectPayload`, or nil if unrecognised.
/// Accepts three shapes (matching the Android client):
///   • `https://opensource.unisim.co.uk/screens/connect?host=&port=&pin=#ssid=&auth=&pass=`
///   • `unisimscreens://connect?host=&port=&pin=&ssid=&pass=&auth=`
///   • `ip:port?pin=NNNN`  (legacy bare host QR)
func parseConnectPayload(_ text: String) -> ConnectPayload? {
    let t = text.trimmingCharacters(in: .whitespaces)
    let isHttps = t.lowercased().hasPrefix("https://")
    let isCustom = t.lowercased().hasPrefix("unisimscreens://")

    if isHttps || isCustom {
        guard let url = URL(string: t),
              let comps = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return nil }
        if isHttps, !(url.path.hasPrefix("/screens/connect")) { return nil }

        var params: [String: String] = [:]
        // Fragment carries Wi-Fi creds; re-parse it as a query string.
        if let frag = comps.percentEncodedFragment,
           let fc = URLComponents(string: "x://x?\(frag)") {
            for item in fc.queryItems ?? [] { if let v = item.value { params[item.name] = v } }
        }
        for item in comps.queryItems ?? [] { if let v = item.value { params[item.name] = v } }

        guard let host = params["host"], !host.isEmpty else { return nil }
        return ConnectPayload(
            addr: "\(host):\(params["port"] ?? "9000")",
            pin: Int(params["pin"]?.filter(\.isNumber) ?? "") ?? 0,
            ssid: params["ssid"].flatMap { $0.isEmpty ? nil : $0 },
            pass: params["pass"].flatMap { $0.isEmpty ? nil : $0 },
            auth: params["auth"] ?? "WPA"
        )
    }

    // Bare "ip:port?pin=NNNN"
    if let qRange = t.range(of: "?pin=") {
        return ConnectPayload(
            addr: String(t[..<qRange.lowerBound]),
            pin: Int(t[qRange.upperBound...].filter(\.isNumber)) ?? 0,
            ssid: nil, pass: nil, auth: "WPA"
        )
    }
    return nil
}

// MARK: - Root

struct ContentView: View {
    @State private var session: ExtenderSession?
    @State private var mode: Mode = .clicker
    @State private var currentAddr = ""
    @State private var currentPin: Int = 0
    @State private var status = ""
    /// Address + PIN gathered from a scan/deep-link, waiting for a mode choice.
    @State private var pending: (addr: String, pin: Int)?
    @State private var connecting = false

    var body: some View {
        content.onOpenURL { url in handleDeepLink(url) }
    }

    @ViewBuilder private var content: some View {
        if let session {
            connectedView(session)
        } else if connecting {
            ConnectingScreen(addr: currentAddr)
        } else if let (pAddr, pPin) = pending {
            ModePickerScreen(
                addr: pAddr,
                onPick: { chosen, rememberMode in
                    pending = nil
                    doConnect(pAddr, chosen, pPin, rememberMode)
                },
                onBack: { pending = nil }
            )
        } else {
            ConnectView(
                status: status,
                onPrepare: { addr, pin in pending = (addr, pin) },
                onConnect: { addr, m, pin in doConnect(addr, m, pin, true) }
            )
        }
    }

    @ViewBuilder private func connectedView(_ live: ExtenderSession) -> some View {
        let streaming = mode == .viewer || mode == .control || mode == .secondScreen
        VStack(spacing: 0) {
            if !streaming {
                EmptyView()
            }
            switch mode {
            case .clicker:
                ClickerView(
                    session: live, addr: currentAddr,
                    onDisconnect: disconnect,
                    onSwitchMode: { live.close(); session = nil; pending = (currentAddr, currentPin) }
                )
            case .trackpad:
                TrackpadView(
                    session: live,
                    onDisconnect: disconnect,
                    onSwitchMode: { live.close(); session = nil; pending = (currentAddr, currentPin) }
                )
            case .viewer:
                StreamView(session: live, addr: currentAddr, forwardInput: false, onDisconnect: disconnect)
            case .control:
                StreamView(session: live, addr: currentAddr, forwardInput: true, onDisconnect: disconnect)
            case .secondScreen:
                StreamView(session: live, addr: currentAddr, forwardInput: false, onDisconnect: disconnect)
            }
        }
    }

    // MARK: - Deep links

    private func handleDeepLink(_ url: URL) {
        guard let payload = parseConnectPayload(url.absoluteString) else { return }
        // Jump straight to mode picker (same path as an in-app scan).
        pending = (payload.addr, payload.pin)
    }

    // MARK: - Connect

    private func doConnect(_ addr: String, _ chosen: Mode, _ pin: Int, _ rememberMode: Bool) {
        mode = chosen
        currentAddr = addr
        currentPin = pin
        connecting = true
        status = ""
        let capture: ExtenderSession.CaptureMode = switch chosen {
        case .clicker, .trackpad: .controlOnly
        case .secondScreen: .virtualDisplay
        default: .mirror
        }
        DispatchQueue.global(qos: .userInitiated).async {
            let s = ExtenderSession.connect(addr: addr, mode: capture, pin: UInt32(pin))
            DispatchQueue.main.async {
                connecting = false
                if s != nil {
                    ConnectionStore.remember(addr: addr,
                                            mode: rememberMode ? chosen.rawValue : "",
                                            pin: pin)
                }
                session = s
                status = s == nil ? "connection failed" : ""
            }
        }
    }

    private func disconnect() {
        session?.close()
        session = nil
    }
}

// MARK: - Mode picker

struct ModePickerScreen: View {
    let addr: String
    let onPick: (Mode, Bool) -> Void
    let onBack: () -> Void

    @State private var rememberChoice = false

    var body: some View {
        List {
            Section {
                Text("Universal Screens").font(.largeTitle).bold()
                Text("Host: \(addr)").foregroundStyle(.secondary)
                Text("How do you want to use it?")
            }

            Section {
                ModeOption("Clicker",       subtitle: "Presentation remote — next/previous, blank, slide previews")  { onPick(.clicker,      rememberChoice) }
                ModeOption("Mirror",        subtitle: "Watch the host's screen (view only)")                         { onPick(.viewer,       rememberChoice) }
                ModeOption("Remote control",subtitle: "See the screen and control it (mouse + keys)")                { onPick(.control,      rememberChoice) }
                ModeOption("Trackpad",      subtitle: "Use the phone as a touchpad — move, tap, scroll")             { onPick(.trackpad,     rememberChoice) }
                ModeOption("Second screen", subtitle: "Use the phone as an extra display (needs a virtual-display driver on the PC)") { onPick(.secondScreen, rememberChoice) }
            }

            Section {
                Toggle("Remember next time?", isOn: $rememberChoice)
            }

            Section {
                Button("Back", action: onBack)
            }
        }
    }
}

private struct ModeOption: View {
    let title: String
    let subtitle: String
    let action: () -> Void

    init(_ title: String, subtitle: String, action: @escaping () -> Void) {
        self.title = title; self.subtitle = subtitle; self.action = action
    }

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.headline)
                Text(subtitle).font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}

// MARK: - Connecting spinner

struct ConnectingScreen: View {
    let addr: String
    var body: some View {
        VStack(spacing: 16) {
            ProgressView()
            Text("Connecting…").font(.title3)
            if !addr.isEmpty { Text(addr).font(.caption).foregroundStyle(.secondary) }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
