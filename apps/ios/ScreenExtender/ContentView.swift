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

/// Extract a "cast to a browser" pairing code from a connect URL, or nil. The
/// receiver page's QR encodes `…/screens/connect?code=<CODE>&role=sender`; the
/// legacy `unisimscreens://connect?code=…` scheme is also accepted. A code is
/// 4–8 letters/digits and routes to the cast flow (no host/Wi-Fi involved).
/// Mirrors the Android `parseRoomCode`.
func parseRoomCode(_ text: String) -> String? {
    let t = text.trimmingCharacters(in: .whitespaces)
    let isHttps = t.lowercased().hasPrefix("https://")
    let isCustom = t.lowercased().hasPrefix("unisimscreens://")
    guard isHttps || isCustom, let comps = URLComponents(string: t) else { return nil }
    if isHttps, !comps.path.hasPrefix("/screens/connect") { return nil }
    guard let code = comps.queryItems?
        .first(where: { $0.name == "code" })?.value?.uppercased() else { return nil }
    let valid = code.range(of: "^[A-Z0-9]{4,8}$", options: .regularExpression) != nil
    return valid ? code : nil
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
    /// Non-nil when "casting to a browser": the rendezvous code we're paired on.
    /// Takes over the whole UI (CastFlow), independent of the host session.
    @State private var castCode: String?

    var body: some View {
        content.onOpenURL { url in handleDeepLink(url) }
    }

    @ViewBuilder private var content: some View {
        if let castCode {
            CastFlow(code: castCode, onExit: { self.castCode = nil })
        } else if let session {
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
                onConnect: { addr, m, pin in doConnect(addr, m, pin, true) },
                onCast: { code in castCode = code }
            )
        }
    }

    @ViewBuilder private func connectedView(_ live: ExtenderSession) -> some View {
        let repick = { live.close(); session = nil; pending = (currentAddr, currentPin) }
        switch mode {
        case .clicker:
            ClickerView(session: live, addr: currentAddr, onDisconnect: disconnect, onSwitchMode: repick)
        case .trackpad:
            TrackpadView(session: live, onDisconnect: disconnect, onSwitchMode: repick)
        case .viewer:
            StreamView(session: live, addr: currentAddr, mode: .viewer, forwardInput: false, onDisconnect: disconnect, onSwitchMode: repick)
        case .control:
            StreamView(session: live, addr: currentAddr, mode: .control, forwardInput: true, onDisconnect: disconnect, onSwitchMode: repick)
        case .secondScreen:
            StreamView(session: live, addr: currentAddr, mode: .secondScreen, forwardInput: false, onDisconnect: disconnect, onSwitchMode: repick)
        }
    }

    // MARK: - Deep links

    private func handleDeepLink(_ url: URL) {
        // A "cast to a browser" code (…/screens/connect?code=…) routes to the
        // rendezvous flow instead of a host connection.
        if let code = parseRoomCode(url.absoluteString) { castCode = code; return }
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
        let deviceName = ConnectionStore.effectiveDeviceName()
        DispatchQueue.global(qos: .userInitiated).async {
            let s = ExtenderSession.connect(addr: addr, mode: capture, pin: UInt32(pin),
                                            deviceName: deviceName)
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

    private let modes: [Mode] = [.clicker, .viewer, .control, .trackpad, .secondScreen]

    var body: some View {
        GeometryReader { geo in
            ScrollView {
                VStack(spacing: 20) {
                    VStack(spacing: 4) {
                        Text("How do you want to use it?")
                            .font(.title2.bold())
                            .multilineTextAlignment(.center)
                        Text(addr)
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }

                    VStack(spacing: 10) {
                        ForEach(modes, id: \.self) { mode in
                            ModeOption(mode) { onPick(mode, rememberChoice) }
                        }
                    }

                    Toggle("Remember next time?", isOn: $rememberChoice)
                        .padding(.horizontal, 4)

                    Button("Back", action: onBack)
                        .font(.subheadline)
                }
                .padding(24)
                .frame(maxWidth: 520)
                .frame(maxWidth: .infinity)
                .frame(minHeight: geo.size.height, alignment: .center)
            }
        }
        .background(Color(.systemGroupedBackground).ignoresSafeArea())
    }
}

private struct ModeOption: View {
    let mode: Mode
    let action: () -> Void

    init(_ mode: Mode, action: @escaping () -> Void) {
        self.mode = mode; self.action = action
    }

    var body: some View {
        Button(action: action) {
            HStack(spacing: 14) {
                Image(systemName: mode.systemImage)
                    .font(.title2)
                    .foregroundStyle(Color.brandOrange)
                    .frame(width: 44, height: 44)
                    .background(Color.brandOrange.opacity(0.12), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                VStack(alignment: .leading, spacing: 2) {
                    Text(mode.label).font(.headline).foregroundStyle(.primary)
                    Text(mode.subtitle).font(.caption).foregroundStyle(.secondary)
                        .multilineTextAlignment(.leading)
                }
                Spacer(minLength: 4)
                Image(systemName: "chevron.right")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.tertiary)
            }
            .padding(12)
            .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Connecting spinner

struct ConnectingScreen: View {
    let addr: String
    var body: some View {
        VStack(spacing: 18) {
            Image("AppLogo")
                .resizable().scaledToFit()
                .frame(width: 88, height: 88)
                .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
            ProgressView()
            Text("Connecting…").font(.title3.weight(.semibold))
            if !addr.isEmpty { Text(addr).font(.caption).foregroundStyle(.secondary) }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemGroupedBackground).ignoresSafeArea())
    }
}
