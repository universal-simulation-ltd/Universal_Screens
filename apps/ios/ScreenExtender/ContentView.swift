import SwiftUI

/// The five ways to use the app; differ in UI + what the session streams.
enum Mode: String {
    case clicker, viewer, control, trackpad, secondScreen
}

/// Where a connection attempt has got to. Drives the full-screen feedback shown
/// between tapping Connect and either landing in a mode or coming back home.
enum ConnectPhase {
    case idle, connecting, connected, failed
}

/// The least time "Connecting…" stays up. A host that refuses at once (wrong PIN,
/// nothing listening) would otherwise flash the whole attempt past in a frame or
/// two, which is what made a failure so easy to miss.
private let minConnectingSeconds: TimeInterval = 1.2

/// How long "Connected" is held before the mode's own UI takes over.
private let connectedHoldSeconds: TimeInterval = 1.1

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
    /// Where the current attempt is: idle (no feedback), connecting (the spinner),
    /// connected (the tick, held for a beat), failed (the cross, until dismissed).
    @State private var phase: ConnectPhase = .idle
    /// Whether the last attempt asked to remember its mode, so "Try again" on the
    /// failure screen can repeat it exactly without re-scanning or re-picking.
    @State private var lastRememberMode = true
    /// Bumped whenever a connect attempt starts, is cancelled, or is superseded. A
    /// background connect stamps the value it began with and, when it returns, drops
    /// its result if the value has since moved on — so tapping Cancel (or starting a
    /// newer attempt) abandons a still-in-flight connect and closes any late session.
    @State private var attempt = 0
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
            // The mode's UI is built straight away (so a stream starts decoding on
            // its first keyframe), with the "Connected" confirmation laid over the
            // top of it for its brief hold.
            ZStack {
                connectedView(session)
                if phase == .connected {
                    ConnectionStatusScreen(phase: .connected, addr: currentAddr, mode: mode)
                }
            }
        } else if phase == .connecting {
            ConnectionStatusScreen(phase: .connecting, addr: currentAddr, mode: mode, onCancel: {
                // Abandon the in-flight attempt (its result is discarded when it
                // returns) and drop back to the connect screen.
                attempt += 1
                phase = .idle
                status = ""
            })
        } else if phase == .failed {
            // The failure stays on screen until the user picks — retry the very
            // same attempt, or go back home.
            ConnectionStatusScreen(
                phase: .failed,
                addr: currentAddr,
                mode: mode,
                onRetry: { doConnect(currentAddr, mode, currentPin, lastRememberMode) },
                onHome: { phase = .idle }
            )
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
        lastRememberMode = rememberMode
        phase = .connecting
        status = ""
        attempt += 1
        let myAttempt = attempt
        let capture: ExtenderSession.CaptureMode = switch chosen {
        case .clicker, .trackpad: .controlOnly
        case .secondScreen: .virtualDisplay
        default: .mirror
        }
        let deviceName = ConnectionStore.effectiveDeviceName()
        let startedAt = Date()
        DispatchQueue.global(qos: .userInitiated).async {
            let s = ExtenderSession.connect(addr: addr, mode: capture, pin: UInt32(pin),
                                            deviceName: deviceName)
            // Hold "Connecting…" for a beat before showing the verdict, so both
            // verdicts are legible however fast the answer came back.
            let remaining = minConnectingSeconds - Date().timeIntervalSince(startedAt)
            if remaining > 0 { Thread.sleep(forTimeInterval: remaining) }
            DispatchQueue.main.async {
                // Cancelled or superseded while connecting → drop this session so a
                // late-arriving connection never hijacks the UI (and its socket is
                // closed rather than leaked).
                guard myAttempt == attempt else {
                    s?.close()
                    return
                }
                if s != nil {
                    ConnectionStore.remember(addr: addr,
                                            mode: rememberMode ? chosen.rawValue : "",
                                            pin: pin)
                }
                session = s
                // The verdict is a screen of its own now, not a line of small text
                // on the home page that an attempt could scroll straight past.
                phase = s == nil ? .failed : .connected
                guard s != nil else { return }
                // "Connected" is a confirmation, not a resting state: hold it for a
                // beat, then fall to idle, which uncovers the mode's own UI (the
                // clicker's buttons, the trackpad, the stream) built underneath it.
                DispatchQueue.main.asyncAfter(deadline: .now() + connectedHoldSeconds) {
                    if phase == .connected, myAttempt == attempt { phase = .idle }
                }
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
    @State private var showMore = false

    // Most-likely modes for a phone first; extend/Second screen is unlikely on a
    // phone acting as the receiver, so it tucks into a collapsed "More options".
    private let primaryModes: [Mode] = [.clicker, .trackpad, .viewer, .control]
    private let moreModes: [Mode] = [.secondScreen]

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
                        ForEach(primaryModes, id: \.self) { mode in
                            ModeOption(mode) { onPick(mode, rememberChoice) }
                        }

                        Button {
                            withAnimation { showMore.toggle() }
                        } label: {
                            HStack {
                                Text("More options")
                                Image(systemName: showMore ? "chevron.down" : "chevron.right")
                                    .font(.caption.weight(.semibold))
                            }
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                        }
                        .buttonStyle(.plain)

                        if showMore {
                            ForEach(moreModes, id: \.self) { mode in
                                ModeOption(mode) { onPick(mode, rememberChoice) }
                            }
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

// MARK: - Connection feedback

/// The whole story of a connection attempt, full screen: the app breathing under a
/// spinner while it tries, then a green tick ("Connected", held for a beat before
/// the mode takes over) or a red cross ("Connection failed", which stays put until
/// the user retries or goes home).
///
/// It is one screen for all three so a failure lands exactly where the eye already
/// is. The old flow dropped straight back to the home page with "connection failed"
/// as a line of small text under the saved hosts, which was easy to miss entirely —
/// especially when a refused connection came back in a few milliseconds.
///
/// The handlers default to no-ops so each phase can pass only the ones it shows.
struct ConnectionStatusScreen: View {
    let phase: ConnectPhase
    let addr: String
    let mode: Mode
    /// Backs out of a connection that's taking too long (e.g. the host is off the
    /// network), instead of waiting out the connect timeout.
    var onCancel: () -> Void = {}
    /// Repeats the attempt that just failed, unchanged.
    var onRetry: () -> Void = {}
    /// Gives up and returns to the connect screen.
    var onHome: () -> Void = {}

    /// Drives the logo's breathe (connecting) and the badge's pop (the verdicts);
    /// both are flipped on in `.onAppear` so the animation runs on entry.
    @State private var pulsing = false
    @State private var popped = false

    var body: some View {
        VStack(spacing: 0) {
            switch phase {
            case .connecting:
                // A slow breathe on the logo, so the wait reads as something in
                // progress at a glance, before the spinner is even noticed.
                Image("AppLogo")
                    .resizable().scaledToFit()
                    .frame(width: 88, height: 88)
                    .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
                    .scaleEffect(pulsing ? 1.06 : 0.94)
                    .animation(.easeInOut(duration: 0.9).repeatForever(autoreverses: true),
                               value: pulsing)
                    .onAppear { pulsing = true }
                Spacer().frame(height: 20)
                ProgressView()
            case .connected:
                badge("checkmark", .green)
            case .failed:
                badge("xmark", .red)
            case .idle:
                EmptyView()
            }

            Spacer().frame(height: 16)
            Text(title)
                .font(.title2.bold())
                .multilineTextAlignment(.center)
            if !addr.isEmpty {
                Spacer().frame(height: 4)
                Text(addr).font(.caption).foregroundStyle(.secondary)
            }
            if phase == .connected {
                Spacer().frame(height: 6)
                Text("Opening \(mode.label)…").font(.subheadline).foregroundStyle(.secondary)
            }
            if phase == .failed {
                Spacer().frame(height: 12)
                Text("Check that the host app is running and showing its code, that the "
                     + "PIN matches, and that both devices are on the same Wi-Fi.")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                Spacer().frame(height: 28)
                Button("Try again", action: onRetry)
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                Spacer().frame(height: 10)
                Button("Back to home", action: onHome)
                    .buttonStyle(.bordered)
                    .controlSize(.large)
            }
            if phase == .connecting {
                Spacer().frame(height: 28)
                Button("Cancel", action: onCancel)
                    .buttonStyle(.bordered)
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemGroupedBackground).ignoresSafeArea())
    }

    private var title: String {
        switch phase {
        case .connected: "Connected"
        case .failed:    "Connection failed"
        default:         "Connecting…"
        }
    }

    /// The round tick / cross a verdict leads with; it pops in so the result reads
    /// as something that just happened rather than a screen that was always there.
    private func badge(_ symbol: String, _ tint: Color) -> some View {
        Image(systemName: symbol)
            .font(.system(size: 40, weight: .bold))
            .foregroundStyle(.white)
            .frame(width: 88, height: 88)
            .background(tint, in: Circle())
            .scaleEffect(popped ? 1 : 0.5)
            .animation(.spring(response: 0.35, dampingFraction: 0.55), value: popped)
            .onAppear { popped = true }
    }
}
