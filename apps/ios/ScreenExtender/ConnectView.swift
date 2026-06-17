import SwiftUI

/// Pick a saved host or enter a new `ip:port` and connect in a chosen mode.
struct ConnectView: View {
    let status: String
    let onConnect: (String, Mode) -> Void

    @State private var addr = "127.0.0.1:9000"
    @State private var saved: [SavedConnection] = ConnectionStore.load()
    @State private var showHidden = false

    private var visible: [SavedConnection] {
        saved.filter { showHidden || !$0.hidden }.sorted { $0.lastConnected > $1.lastConnected }
    }

    var body: some View {
        List {
            Section {
                Text("Screen Extender").font(.largeTitle).bold()
                Text("Presentation clicker").foregroundStyle(.secondary)
            }

            if !visible.isEmpty {
                Section("Saved hosts") {
                    ForEach(visible) { host in savedRow(host) }
                }
            }
            if saved.contains(where: \.hidden) {
                Button(showHidden ? "Hide hidden" : "Show hidden") { showHidden.toggle() }
            }

            Section("New connection") {
                TextField("Host (ip:port)", text: $addr)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .keyboardType(.URL)
                HStack {
                    Button("Clicker") { onConnect(addr, .clicker) }
                    Spacer()
                    Button("Viewer") { onConnect(addr, .viewer) }
                    Spacer()
                    Button("Control") { onConnect(addr, .control) }
                }
                .buttonStyle(.bordered)
                .disabled(addr.isEmpty)
            }

            if !status.isEmpty {
                Text(status).foregroundStyle(.secondary)
            }
        }
        .onAppear { saved = ConnectionStore.load() }
    }

    private func savedRow(_ host: SavedConnection) -> some View {
        Button {
            onConnect(host.addr, .clicker)
        } label: {
            HStack(spacing: 12) {
                Image(systemName: deviceSymbol(host.os)).font(.title2).frame(width: 32)
                VStack(alignment: .leading) {
                    Text(host.hostname.isEmpty ? host.addr : host.hostname)
                    Text(host.addr).font(.caption).foregroundStyle(.secondary)
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
