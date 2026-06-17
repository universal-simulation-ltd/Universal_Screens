import SwiftUI

/// Presentation remote: each button taps a key on the host. Mirrors the Android
/// `ClickerScreen` (without the slide-preview / window-picker, which need FFI
/// events this C ABI doesn't expose yet — see apps/ios/README.md).
struct ClickerView: View {
    let session: ExtenderSession
    let onDisconnect: () -> Void
    @State private var showMore = false

    var body: some View {
        VStack(spacing: 20) {
            HStack {
                Text("Clicker").font(.headline)
                Spacer()
                Button("Disconnect", action: onDisconnect)
            }

            HStack(spacing: 16) {
                bigButton("◀  Prev") { session.tapKey(HidKeys.pageUp) }
                bigButton("Next  ▶") { session.tapKey(HidKeys.pageDown) }
            }

            Button(showMore ? "Fewer options" : "More options") { showMore.toggle() }

            if showMore {
                HStack(spacing: 12) {
                    Button("First") { session.tapKey(HidKeys.home) }
                    Button("Last") { session.tapKey(HidKeys.end) }
                }
                HStack(spacing: 12) {
                    // No universal blank key: PowerPoint uses B, Keynote/Slides '.'.
                    Button("Blank (PPT)") { session.tapKey(HidKeys.b) }
                    Button("Blank (.)") { session.tapKey(HidKeys.period) }
                }
                HStack(spacing: 12) {
                    Button("Start (F5)") { session.tapKey(HidKeys.f5) }
                    Button("End (Esc)") { session.tapKey(HidKeys.escape) }
                }
            }
            Spacer()
        }
        .padding(24)
    }

    private func bigButton(_ label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(label)
                .font(.title2)
                .frame(width: 150, height: 90)
        }
        .buttonStyle(.borderedProminent)
    }
}
