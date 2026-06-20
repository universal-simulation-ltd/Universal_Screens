import SwiftUI
import UIKit

/// One pickable host window.
private struct WindowItem: Identifiable {
    let id: Int64
    let title: String
}

/// Presentation remote, at parity with the Android clicker: current slide preview
/// on top, previous / next thumbnails above the Prev / Next buttons, a Scan-deck
/// pre-scan, a window picker, and the secondary actions behind "More options".
struct ClickerView: View {
    let session: ExtenderSession
    let addr: String
    let onDisconnect: () -> Void
    var onSwitchMode: (() -> Void)? = nil

    @State private var current: UIImage?
    @State private var previous: UIImage?
    @State private var next: UIImage?
    @State private var scanned = false
    @State private var windows: [WindowItem] = []
    @State private var startShowOnFocus = true
    @State private var showMore = false
    /// When locked, every control on this screen is disabled so a stray touch can't
    /// fire a key; only the central lock toggle stays live so it can be unlocked.
    @State private var locked = false

    var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                header

                previewImage(current, height: 200)
                    .overlay(alignment: .center) {
                        if current == nil { Text("Waiting for slide preview…").foregroundStyle(.secondary) }
                    }

                controlsRow.disabled(locked)

                Toggle("Start show on focus (F5)", isOn: $startShowOnFocus)
                    .font(.footnote)
                    .disabled(locked)

                navRow

                Button(showMore ? "Fewer options" : "More options") { showMore.toggle() }
                    .disabled(locked)
                if showMore { moreOptions.disabled(locked) }
            }
            .padding(24)
        }
        .onAppear {
            var sink = ExtenderSession.Sink()
            sink.onSnapshot = { slot, jpeg in
                let image = jpeg.isEmpty ? nil : UIImage(data: jpeg)
                if slot < 0 { previous = image }
                else if slot > 0 { next = image }
                else if let image { current = image }
            }
            sink.onWindowList = { list in windows = list.map { WindowItem(id: $0.id, title: $0.title) } }
            sink.onHostInfo = { os, name in ConnectionStore.setIdentity(addr: addr, os: os, hostname: name) }
            session.startPump(sink)
            session.listWindows()
        }
    }

    private var header: some View {
        HStack {
            Button("Clicker") { onSwitchMode?() }.font(.headline)
                .disabled(onSwitchMode == nil)
            Spacer()
            Button("Disconnect", action: onDisconnect)
        }
    }

    private var controlsRow: some View {
        HStack(spacing: 12) {
            Button(scanned ? "Rescan deck" : "Scan deck") {
                session.scanDeck()
                scanned = true
            }
            Menu("Focus window") {
                Button("Refresh") { session.listWindows() }
                ForEach(windows) { window in
                    Button(window.title) { session.focusWindow(id: window.id, startShow: startShowOnFocus) }
                }
            }
        }
    }

    private var navRow: some View {
        HStack(spacing: 24) {
            VStack {
                previewImage(previous, height: 84).opacity(0.4)
                bigButton("◀  Prev") { session.tapKey(HidKeys.pageUp) }.disabled(locked)
            }
            // The lock sits in the middle of the two nav buttons — a direct tap
            // toggles it; a swipe over it is ignored.
            LockToggle(locked: locked) { locked.toggle() }
            VStack {
                previewImage(next, height: 84)
                bigButton("Next  ▶") { session.tapKey(HidKeys.pageDown) }.disabled(locked)
            }
        }
    }

    private var moreOptions: some View {
        VStack(spacing: 12) {
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
    }

    /// A 16:9 slide thumbnail, or a placeholder icon when there's no slide.
    private func previewImage(_ image: UIImage?, height: CGFloat) -> some View {
        Group {
            if let image {
                Image(uiImage: image).resizable().scaledToFit()
            } else {
                Image(systemName: "rectangle.on.rectangle")
                    .resizable().scaledToFit().padding(height / 4)
                    .foregroundStyle(.tertiary)
            }
        }
        .frame(maxWidth: .infinity)
        .frame(height: height)
    }

    private func bigButton(_ label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(label).font(.title2).frame(width: 130, height: 80)
        }
        .buttonStyle(.borderedProminent)
    }
}

// MARK: - Lock toggle

/// A lock toggle that guards a screen against accidental presses. Locked → a closed
/// padlock on a yellow background; unlocked → just an open padlock on a transparent
/// background. `onTapGesture` fires only for a clean tap, so a swipe (a drag) over it
/// is ignored; the filled content shape captures the touch so it never leaks through
/// to a gesture surface underneath (e.g. the trackpad).
struct LockToggle: View {
    let locked: Bool
    let onToggle: () -> Void

    var body: some View {
        Image(systemName: locked ? "lock.fill" : "lock.open")
            .font(.system(size: 26))
            .foregroundStyle(locked ? Color.black : Color.secondary)
            .frame(width: 64, height: 64)
            .background(locked ? Color.yellow : Color.clear)
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .contentShape(Rectangle())
            .onTapGesture { onToggle() }
            .accessibilityLabel(locked ? "Locked. Tap to unlock." : "Unlocked. Tap to lock.")
    }
}
