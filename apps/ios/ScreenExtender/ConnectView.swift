import SwiftUI

/// Enter the host's `ip:port` and connect as a clicker.
struct ConnectView: View {
    @Binding var addr: String
    let status: String
    let onConnect: () -> Void

    var body: some View {
        VStack(spacing: 20) {
            Text("Screen Extender")
                .font(.largeTitle).bold()
            Text("Presentation clicker")
                .foregroundStyle(.secondary)

            TextField("Host (ip:port)", text: $addr)
                .textFieldStyle(.roundedBorder)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .keyboardType(.URL)

            Button("Connect", action: onConnect)
                .buttonStyle(.borderedProminent)

            if !status.isEmpty {
                Text(status).foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(24)
    }
}
