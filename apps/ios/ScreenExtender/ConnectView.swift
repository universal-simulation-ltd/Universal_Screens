import SwiftUI

/// Home screen: scan to connect, or pick a saved host.
/// `onPrepare(addr, pin)` → show mode picker.
/// `onConnect(addr, mode, pin)` → connect directly (saved host with remembered mode).
struct ConnectView: View {
    let status: String
    let onPrepare: (String, Int) -> Void
    let onConnect: (String, Mode, Int) -> Void

    @State private var addr = "127.0.0.1:9000"
    @State private var pin = ""
    @State private var saved: [SavedConnection] = ConnectionStore.load()
    @State private var showHidden = false
    @State private var showAdvanced = false
    @State private var showScanner = false

    private var visible: [SavedConnection] {
        saved.filter { showHidden || !$0.hidden }.sorted { $0.lastConnected > $1.lastConnected }
    }

    var body: some View {
        List {
            Section {
                Text("Universal Screens").font(.largeTitle).bold()
            }

            Section {
                Button(action: { showScanner = true }) {
                    Label("Scan to connect", systemImage: "qrcode.viewfinder")
                        .font(.title3)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .listRowInsets(.init())
                .listRowBackground(Color.clear)

                Text("Point at the host's QR code — it joins this PC's Wi-Fi and connects.")
                    .font(.footnote).foregroundStyle(.secondary)
            }

            if !visible.isEmpty {
                Section("Saved hosts") {
                    ForEach(visible) { host in savedRow(host) }
                }
            }
            if saved.contains(where: \.hidden) {
                Button(showHidden ? "Hide hidden" : "Show hidden") { showHidden.toggle() }
            }

            Section {
                Button(showAdvanced ? "Advanced ▾" : "Advanced ▸") { showAdvanced.toggle() }
                    .foregroundStyle(.secondary)
                if showAdvanced {
                    TextField("Host (ip:port)", text: $addr)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                        .keyboardType(.URL)
                    TextField("PIN (from the host)", text: $pin)
                        .keyboardType(.numberPad)
                        .onChange(of: pin) { _, v in pin = String(v.filter(\.isNumber).prefix(4)) }
                    Button("Connect") {
                        onPrepare(addr, Int(pin) ?? 0)
                    }
                    .disabled(addr.isEmpty)
                }
            }

            if !status.isEmpty {
                Text(status).foregroundStyle(.secondary)
            }
        }
        .onAppear { saved = ConnectionStore.load() }
        .sheet(isPresented: $showScanner) {
            QRScannerView { text in
                showScanner = false
                if let payload = parseConnectPayload(text) {
                    addr = payload.addr
                    pin = String(format: "%04d", payload.pin)
                    onPrepare(payload.addr, payload.pin)
                } else {
                    addr = text
                }
            }
        }
    }

    private func savedRow(_ host: SavedConnection) -> some View {
        Button {
            let m = Mode(rawValue: host.mode)
            if let m {
                onConnect(host.addr, m, host.pin)
            } else {
                onPrepare(host.addr, host.pin)
            }
        } label: {
            HStack(spacing: 12) {
                Image(systemName: deviceSymbol(host.os)).font(.title2).frame(width: 32)
                VStack(alignment: .leading) {
                    Text(host.hostname.isEmpty ? host.addr : host.hostname)
                    let sub = host.mode.isEmpty ? host.addr : "\(host.addr)  ·  \(modeLabel(host.mode))"
                    Text(sub).font(.caption).foregroundStyle(.secondary)
                }
            }
        }
        .swipeActions(edge: .trailing) {
            Button(role: .destructive) {
                ConnectionStore.delete(addr: host.addr)
                saved = ConnectionStore.load()
            } label: { Label("Delete", systemImage: "trash") }
            Button {
                ConnectionStore.setHidden(addr: host.addr, !host.hidden)
                saved = ConnectionStore.load()
            } label: { Label(host.hidden ? "Unhide" : "Hide", systemImage: "eye.slash") }
        }
    }
}

private func modeLabel(_ raw: String) -> String {
    switch Mode(rawValue: raw) {
    case .clicker:      return "Clicker"
    case .viewer:       return "Mirror"
    case .control:      return "Remote control"
    case .trackpad:     return "Trackpad"
    case .secondScreen: return "Second screen"
    case nil:           return raw
    }
}
