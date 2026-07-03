import SwiftUI
import UIKit

/// Home screen: scan to connect, or pick a saved host. A centred hero (logo +
/// primary action) mirrors the Android client, with saved hosts as cards below.
/// `onPrepare(addr, pin)` → show mode picker.
/// `onConnect(addr, mode, pin)` → connect directly (saved host with remembered mode).
struct ConnectView: View {
    let status: String
    let onPrepare: (String, Int) -> Void
    let onConnect: (String, Mode, Int) -> Void
    /// A "cast to a browser" pairing code (scanned, deep-linked, or typed).
    var onCast: (String) -> Void = { _ in }

    @State private var addr = "127.0.0.1:9000"
    @State private var pin = ""
    @State private var deviceName = ConnectionStore.loadDeviceName()
    @State private var saved: [SavedConnection] = ConnectionStore.load()
    @State private var showHidden = false
    @State private var showAdvanced = false
    @State private var showScanner = false
    // "Cast to a browser": manual code entry (the QR / deep-link path skips this).
    @State private var showCast = false
    @State private var castDraft = ""
    // Saved-host rename: the host being renamed (drives the alert) + the draft.
    @State private var renameTarget: SavedConnection?
    @State private var renameDraft = ""

    private var visible: [SavedConnection] {
        saved.filter { showHidden || !$0.hidden }.sorted { $0.lastConnected > $1.lastConnected }
    }

    var body: some View {
        GeometryReader { geo in
            ScrollView {
                VStack(spacing: 24) {
                    hero
                    if !visible.isEmpty { savedHosts }
                    if saved.contains(where: \.hidden) {
                        Button(showHidden ? "Hide hidden hosts" : "Show hidden hosts") {
                            withAnimation { showHidden.toggle() }
                        }
                        .font(.footnote)
                    }
                    advanced
                    if !status.isEmpty {
                        Text(status).font(.footnote).foregroundStyle(.secondary)
                    }
                }
                .padding(24)
                .frame(maxWidth: 520)
                .frame(maxWidth: .infinity)
                // Centre the hero when it fits; grow + scroll when hosts overflow.
                .frame(minHeight: geo.size.height, alignment: .center)
            }
        }
        .background(Color(.systemGroupedBackground).ignoresSafeArea())
        .onAppear { saved = ConnectionStore.load() }
        .alert("Rename host", isPresented: Binding(
            get: { renameTarget != nil },
            set: { if !$0 { renameTarget = nil } }
        )) {
            TextField("Name", text: $renameDraft)
            Button("Save") {
                if let t = renameTarget {
                    ConnectionStore.setCustomName(addr: t.addr, renameDraft)
                    saved = ConnectionStore.load()
                }
                renameTarget = nil
            }
            Button("Cancel", role: .cancel) { renameTarget = nil }
        } message: {
            Text("Give this saved host a friendly name. Leave blank to reset to its device name.")
        }
        .sheet(isPresented: $showScanner) {
            QRScannerView { text in
                showScanner = false
                // A receiver's "cast" code routes to the browser-cast flow.
                if let code = parseRoomCode(text) {
                    onCast(code)
                } else if let payload = parseConnectPayload(text) {
                    addr = payload.addr
                    pin = String(format: "%04d", payload.pin)
                    onPrepare(payload.addr, payload.pin)
                } else {
                    addr = text
                }
            }
        }
        .alert("Cast to a browser", isPresented: $showCast) {
            TextField("Code (e.g. 7Q4K)", text: $castDraft)
                .textInputAutocapitalization(.characters)
                .autocorrectionDisabled()
            Button("Connect") {
                let code = castDraft.uppercased().filter { $0.isLetter || $0.isNumber }
                if (4...8).contains(code.count) { onCast(code) }
            }
            Button("Cancel", role: .cancel) { }
        } message: {
            Text("Open opensource.unisim.co.uk/screens/receive on the screen you want to drive, then enter the code it shows (or scan its QR).")
        }
    }

    // MARK: - Hero

    private var hero: some View {
        VStack(spacing: 16) {
            Button { showScanner = true } label: {
                Image("AppLogo")
                    .resizable()
                    .scaledToFit()
                    .frame(width: 116, height: 116)
                    .clipShape(RoundedRectangle(cornerRadius: 26, style: .continuous))
                    .shadow(color: .black.opacity(0.18), radius: 10, y: 4)
                    .accessibilityLabel("Scan to connect")
            }
            .buttonStyle(.plain)
            .padding(.top, 8)

            Text("Universal Screens")
                .font(.largeTitle.bold())

            Button { showScanner = true } label: {
                Label("Scan to connect", systemImage: "qrcode.viewfinder")
                    .font(.title3.weight(.semibold))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 6)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)

            Text("Point at the host's QR code — it joins this PC's Wi-Fi and connects.")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

            // Cast to a browser: this phone becomes the remote for a browser tab
            // showing …/screens/receive. Scanning that page's QR skips this button.
            Button { castDraft = ""; showCast = true } label: {
                Label("Cast to a browser screen", systemImage: "rectangle.on.rectangle.angled")
                    .font(.subheadline.weight(.semibold))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 4)
            }
            .buttonStyle(.bordered)
            .controlSize(.large)
        }
    }

    // MARK: - Saved hosts

    private var savedHosts: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("SAVED HOSTS")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
                .padding(.leading, 4)
            VStack(spacing: 10) {
                ForEach(visible) { host in savedRow(host) }
            }
        }
    }

    /// Row title: the user's friendly name with the host in brackets, e.g.
    /// "Office Mac (Kyjams-iMac)"; else just the hostname (or address).
    private func savedTitle(_ host: SavedConnection) -> String {
        let base = host.hostname.isEmpty ? host.addr : host.hostname
        let name = host.customName.trimmingCharacters(in: .whitespacesAndNewlines)
        return name.isEmpty ? base : "\(name) (\(base))"
    }

    private func savedRow(_ host: SavedConnection) -> some View {
        Button {
            if let m = Mode(rawValue: host.mode) {
                onConnect(host.addr, m, host.pin)
            } else {
                onPrepare(host.addr, host.pin)
            }
        } label: {
            HStack(spacing: 14) {
                Text(deviceEmoji(host.os))
                    .font(.title2)
                    .frame(width: 44, height: 44)
                    .background(Color.brandOrange.opacity(0.12), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                VStack(alignment: .leading, spacing: 2) {
                    Text(savedTitle(host))
                        .font(.body.weight(.medium))
                        .foregroundStyle(.primary)
                    Text(host.mode.isEmpty ? host.addr : "\(host.addr)  ·  \(modeLabel(host.mode))")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 8)
                Menu {
                    Button {
                        renameDraft = host.customName
                        renameTarget = host
                    } label: {
                        Label("Rename", systemImage: "pencil")
                    }
                    Button {
                        ConnectionStore.setHidden(addr: host.addr, !host.hidden)
                        saved = ConnectionStore.load()
                    } label: {
                        Label(host.hidden ? "Unhide" : "Hide", systemImage: host.hidden ? "eye" : "eye.slash")
                    }
                    Button(role: .destructive) {
                        ConnectionStore.delete(addr: host.addr)
                        saved = ConnectionStore.load()
                    } label: { Label("Delete", systemImage: "trash") }
                } label: {
                    Image(systemName: "ellipsis")
                        .font(.body)
                        .foregroundStyle(.secondary)
                        .frame(width: 32, height: 44)
                        .contentShape(Rectangle())
                }
            }
            .padding(12)
            .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(.plain)
    }

    // MARK: - Advanced (manual entry)

    private var advanced: some View {
        VStack(spacing: 12) {
            Button {
                withAnimation { showAdvanced.toggle() }
            } label: {
                HStack {
                    Text("Advanced")
                    Image(systemName: showAdvanced ? "chevron.down" : "chevron.right")
                        .font(.caption.weight(.semibold))
                }
                .font(.subheadline)
                .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)

            if showAdvanced {
                VStack(spacing: 12) {
                    VStack(alignment: .leading, spacing: 4) {
                        TextField(UIDevice.current.name, text: $deviceName)
                            .autocorrectionDisabled()
                            .textFieldStyle(.roundedBorder)
                            .onChange(of: deviceName) { _, v in ConnectionStore.saveDeviceName(v) }
                        Text("This device's name — labels the screen this phone adds on the host.")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    TextField("Host (ip:port)", text: $addr)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                        .keyboardType(.URL)
                        .textFieldStyle(.roundedBorder)
                    TextField("PIN (from the host)", text: $pin)
                        .keyboardType(.numberPad)
                        .textFieldStyle(.roundedBorder)
                        .onChange(of: pin) { _, v in pin = String(v.filter(\.isNumber).prefix(4)) }
                    Button {
                        onPrepare(addr, Int(pin) ?? 0)
                    } label: {
                        Text("Connect").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                    .disabled(addr.isEmpty)
                }
                .padding(14)
                .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            }
        }
    }
}
