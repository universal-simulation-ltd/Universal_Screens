import Network
import SwiftUI
import UIKit

// MARK: - Nearby (Bonjour host browsing)

/// A serving desktop host discovered over Bonjour. `addr` is `ip:port`, ready
/// for the connect flow.
struct NearbyHost: Identifiable, Equatable {
    let id: String // Bonjour service (instance) name
    let name: String
    let addr: String
}

/// Browses for serving desktop hosts. They advertise `_usscreens._tcp` over
/// DNS-SD while serving (the shared `extender-discovery` crate) — Bonjour
/// browsing needs no special entitlement, unlike joining a raw multicast group
/// (which is why iOS discovery is DNS-SD, not the hosts' custom UDP beacon).
///
/// `NWBrowser` finds the service instances; each is resolved to `ip:port` with
/// `NetService` — SRV/A queries over mDNS only, so the host's single-client
/// accept loop is never touched by a probe connection. (`NetService` is
/// deprecated in favour of `NWConnection`, but `NWConnection` *connects* to
/// resolve; the query-only path is exactly what we want here.) The service
/// type is declared in Info.plist under `NSBonjourServices`.
final class NearbyBrowser: NSObject, ObservableObject, NetServiceDelegate {
    @Published var hosts: [NearbyHost] = []

    private var browser: NWBrowser?
    private var resolving: [String: NetService] = [:] // service name → in-flight

    func start() {
        guard browser == nil else { return }
        let b = NWBrowser(for: .bonjour(type: "_usscreens._tcp", domain: nil), using: .tcp)
        b.browseResultsChangedHandler = { [weak self] results, _ in
            DispatchQueue.main.async { self?.sync(results) }
        }
        b.start(queue: .main)
        browser = b
    }

    func stop() {
        browser?.cancel()
        browser = nil
        for svc in resolving.values { svc.stop() }
        resolving.removeAll()
        hosts.removeAll()
    }

    /// Reconcile the browse results: resolve new services, drop vanished ones
    /// (a host that stops serving withdraws its advertisement).
    private func sync(_ results: Set<NWBrowser.Result>) {
        var present = Set<String>()
        for result in results {
            guard case let .service(name, type, domain, _) = result.endpoint else { continue }
            present.insert(name)
            if hosts.contains(where: { $0.id == name }) || resolving[name] != nil { continue }
            let svc = NetService(domain: domain, type: type, name: name)
            svc.delegate = self
            resolving[name] = svc
            svc.resolve(withTimeout: 5)
        }
        hosts.removeAll { !present.contains($0.id) }
        for (name, svc) in resolving where !present.contains(name) {
            svc.stop()
            resolving[name] = nil
        }
    }

    func netServiceDidResolveAddress(_ sender: NetService) {
        resolving[sender.name] = nil
        guard sender.port > 0, let addresses = sender.addresses else { return }
        // Prefer an IPv4 address (the desktop hosts serve on their v4 LAN IP).
        for data in addresses {
            let ip: String? = data.withUnsafeBytes { raw -> String? in
                guard let base = raw.baseAddress,
                      base.assumingMemoryBound(to: sockaddr.self).pointee.sa_family == sa_family_t(AF_INET)
                else { return nil }
                var sin = base.assumingMemoryBound(to: sockaddr_in.self).pointee
                var buf = [CChar](repeating: 0, count: Int(INET_ADDRSTRLEN))
                inet_ntop(AF_INET, &sin.sin_addr, &buf, socklen_t(INET_ADDRSTRLEN))
                return String(cString: buf)
            }
            if let ip {
                let host = NearbyHost(id: sender.name, name: sender.name, addr: "\(ip):\(sender.port)")
                hosts.removeAll { $0.id == sender.name }
                hosts.append(host)
                return
            }
        }
    }

    func netService(_ sender: NetService, didNotResolve errorDict: [String: NSNumber]) {
        resolving[sender.name] = nil
    }
}

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
    // "Cast to a browser": inline manual code entry under Advanced (the QR /
    // deep-link path skips this and casts straight away).
    @State private var castDraft = ""
    // Saved-host rename: the host being renamed (drives the alert) + the draft.
    @State private var renameTarget: SavedConnection?
    @State private var renameDraft = ""
    // Nearby hosts, browsed over Bonjour while this screen is visible. Tapping
    // one asks for the host's PIN (the QR normally carries it; there's no QR in
    // this flow), then goes to the usual mode picker.
    @StateObject private var nearby = NearbyBrowser()
    @State private var nearbyTarget: NearbyHost?
    @State private var nearbyPin = ""

    private var visible: [SavedConnection] {
        saved.filter { showHidden || !$0.hidden }.sorted { $0.lastConnected > $1.lastConnected }
    }

    var body: some View {
        GeometryReader { geo in
            ScrollView {
                VStack(spacing: 24) {
                    hero
                    if !nearby.hosts.isEmpty { nearbyHosts }
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
        .onAppear {
            saved = ConnectionStore.load()
            nearby.start()
        }
        .onDisappear { nearby.stop() }
        // PIN prompt for a Nearby host (no QR in this flow, so the PIN — shown
        // on the host under "More details" — is typed instead).
        .alert("Connect to \(nearbyTarget?.name ?? "host")", isPresented: Binding(
            get: { nearbyTarget != nil },
            set: { if !$0 { nearbyTarget = nil } }
        )) {
            TextField("PIN", text: $nearbyPin)
                .keyboardType(.numberPad)
            Button("Connect") {
                if let t = nearbyTarget {
                    onPrepare(t.addr, Int(nearbyPin.filter(\.isNumber).prefix(4)) ?? 0)
                }
                nearbyTarget = nil
            }
            Button("Cancel", role: .cancel) { nearbyTarget = nil }
        } message: {
            Text("Enter the 4-digit PIN shown on the host (under “More details”).")
        }
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
        }
    }

    // MARK: - Nearby hosts

    private var nearbyHosts: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("NEARBY")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
                .padding(.leading, 4)
            VStack(spacing: 10) {
                ForEach(nearby.hosts) { host in nearbyRow(host) }
            }
        }
    }

    /// One Nearby host: same card look as a saved row, but discovery-fed — no
    /// overflow menu (nothing to rename/forget; it vanishes when the host stops).
    private func nearbyRow(_ host: NearbyHost) -> some View {
        Button {
            nearbyPin = ""
            nearbyTarget = host
        } label: {
            HStack(spacing: 14) {
                Text("📡")
                    .font(.title2)
                    .frame(width: 44, height: 44)
                    .background(Color.brandOrange.opacity(0.12), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                VStack(alignment: .leading, spacing: 2) {
                    Text(host.name)
                        .font(.body.weight(.medium))
                        .foregroundStyle(.primary)
                    Text(host.addr)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 8)
                Text("Connect")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Color.brandOrange)
            }
            .padding(12)
            .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(.plain)
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

    /// Row title (top line): the device name — the user's friendly name if set,
    /// else the host's machine name, else a friendly OS fallback (e.g. "Windows
    /// device"). Never the IP; that goes on the second line.
    private func savedTitle(_ host: SavedConnection) -> String {
        let name = host.customName.trimmingCharacters(in: .whitespacesAndNewlines)
        if !name.isEmpty { return name }
        if !host.hostname.isEmpty { return host.hostname }
        return deviceFallback(host.os)
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

                    // Cast to a browser: this phone becomes the remote for a browser
                    // tab showing …/screens/receive. Uncommon (people usually just
                    // scan the host's QR), so it lives here under Advanced — an
                    // inline "Web code" box + the button beneath it (no popup).
                    VStack(alignment: .leading, spacing: 4) {
                        TextField("Web code", text: $castDraft)
                            .textInputAutocapitalization(.characters)
                            .autocorrectionDisabled()
                            .textFieldStyle(.roundedBorder)
                            .onChange(of: castDraft) { _, v in
                                castDraft = String(v.uppercased().filter { $0.isLetter || $0.isNumber }.prefix(8))
                            }
                        Button {
                            if (4...8).contains(castDraft.count) { onCast(castDraft) }
                        } label: {
                            Label("Cast to a browser screen", systemImage: "rectangle.on.rectangle.angled")
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 4)
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.large)
                        .disabled(!(4...8).contains(castDraft.count))
                        Text("Open …/screens/receive on the screen you want to drive, then enter the code it shows here.")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(14)
                .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            }
        }
    }
}
