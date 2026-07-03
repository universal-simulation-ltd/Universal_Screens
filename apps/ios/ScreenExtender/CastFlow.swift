import SwiftUI
import UIKit

/// "Cast to a browser": this phone joins a receiver tab's rendezvous room by its
/// short `code` (as the sender) and drives it. Owns the `RoomSession` lifecycle;
/// reuses `TrackpadView` and adds a lightweight clicker. `onExit` returns home.
///
/// Requires the rendezvous Worker to be deployed (opensource-portal) — until then
/// it sits on "Connecting…". Mirrors the Android `CastFlow`.
struct CastFlow: View {
    let code: String
    let onExit: () -> Void

    @StateObject private var controller = CastController()
    /// .trackpad | .clicker once picked; nil = still choosing.
    @State private var castMode: Mode?

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Label("Casting · \(code)", systemImage: "rectangle.on.rectangle.angled")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Color.brandOrange)
                Spacer()
                Button(role: .destructive, action: onExit) {
                    Text("Disconnect").fontWeight(.medium)
                }
                .buttonStyle(.bordered)
                .tint(.red)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            Divider()
            content
        }
        .onAppear { controller.start(code: code) }
        .onDisappear { controller.stop() }
    }

    @ViewBuilder private var content: some View {
        if let session = controller.session, controller.paired {
            if let castMode {
                switch castMode {
                case .clicker:
                    CastClickerView(target: session)
                default:
                    // Reuse the full trackpad; "switch mode" returns to the picker.
                    TrackpadView(session: session,
                                 onDisconnect: onExit,
                                 onSwitchMode: { self.castMode = nil })
                }
            } else {
                CastModePicker { picked in
                    castMode = picked
                    session.hello(mode: picked == .clicker ? "clicker" : "trackpad")
                }
            }
        } else {
            waiting
        }
    }

    private var waiting: some View {
        VStack(spacing: 16) {
            ProgressView()
            Text(controller.status).font(.title3.weight(.semibold))
            Text("Code \(code) — open opensource.unisim.co.uk/screens/receive on the screen you want to drive.")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemGroupedBackground).ignoresSafeArea())
    }
}

/// Owns the `RoomSession` and republishes its state for SwiftUI. Connecting once
/// (guarded by `started`) survives the re-renders `@StateObject` shields against.
final class CastController: ObservableObject {
    @Published var paired = false
    @Published var status = "Connecting…"
    private(set) var session: RoomSession?
    private var started = false

    func start(code: String) {
        guard !started else { return }
        started = true
        var listener = RoomSession.Listener()
        listener.onStatus = { [weak self] in self?.status = $0 }
        listener.onPaired = { [weak self] _ in self?.paired = true; self?.status = "Connected" }
        listener.onPeerLeft = { [weak self] in self?.paired = false; self?.status = "Receiver left — waiting…" }
        listener.onClosed = { [weak self] in self?.paired = false; self?.status = $0 }
        session = RoomSession.connect(code: code, listener: listener)
    }

    func stop() {
        session?.close()
        session = nil
    }

    deinit { stop() }
}

/// Cast control choices: only the no-video modes make sense to a browser tab.
struct CastModePicker: View {
    let onPick: (Mode) -> Void

    var body: some View {
        VStack(spacing: 16) {
            Text("How do you want to drive it?")
                .font(.title2.bold())
                .multilineTextAlignment(.center)
            Button { onPick(.trackpad) } label: {
                Label("Trackpad", systemImage: Mode.trackpad.systemImage)
                    .frame(maxWidth: .infinity).padding(.vertical, 6)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            Button { onPick(.clicker) } label: {
                Label("Clicker", systemImage: Mode.clicker.systemImage)
                    .frame(maxWidth: .infinity).padding(.vertical, 6)
            }
            .buttonStyle(.bordered)
            .controlSize(.large)
        }
        .padding(24)
        .frame(maxWidth: 420)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemGroupedBackground).ignoresSafeArea())
    }
}

/// A minimal presentation clicker for cast mode — the buttons drive the browser
/// receiver's slide deck (no host, so no deck preview / window focus).
struct CastClickerView: View {
    let target: InputTarget

    var body: some View {
        VStack(spacing: 16) {
            HStack(spacing: 16) {
                bigButton("◀  Prev") { tap(HidKeys.pageUp) }
                bigButton("Next  ▶") { tap(HidKeys.pageDown) }
            }
            HStack(spacing: 12) {
                Button("First") { tap(HidKeys.home) }.frame(maxWidth: .infinity).buttonStyle(.bordered)
                Button("Last")  { tap(HidKeys.end) }.frame(maxWidth: .infinity).buttonStyle(.bordered)
            }
            HStack(spacing: 12) {
                Button("Blank")     { tap(HidKeys.b) }.frame(maxWidth: .infinity).buttonStyle(.bordered)
                Button("Start (F5)") { tap(HidKeys.f5) }.frame(maxWidth: .infinity).buttonStyle(.bordered)
            }
        }
        .padding(24)
        .frame(maxWidth: 520)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
        .background(Color(.systemGroupedBackground).ignoresSafeArea())
    }

    private func bigButton(_ label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(label).font(.title2).frame(maxWidth: .infinity).frame(height: 88)
        }
        .buttonStyle(.borderedProminent)
    }

    private func tap(_ hid: UInt32) {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        target.tapKey(hid)
    }
}
