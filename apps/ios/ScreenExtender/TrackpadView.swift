import SwiftUI
import UIKit

/// Trackpad mode: the phone acts as a touchpad over a control-only (no-video)
/// session. One-finger drag moves the cursor (relative), a tap left-clicks,
/// two-finger drag scrolls, and a two-finger tap right-clicks. Matches Android.
struct TrackpadView: View {
    let session: ExtenderSession
    let onDisconnect: () -> Void
    let onSwitchMode: () -> Void

    @State private var sensitivity: Float = ConnectionStore.loadSensitivity()

    var body: some View {
        VStack(spacing: 0) {
            header
            TrackpadSurface(session: session, sensitivity: sensitivity)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color(uiColor: .secondarySystemBackground))
                .overlay(alignment: .center) {
                    Text("Trackpad\n\nDrag to move  •  tap to click\nTwo fingers: scroll  •  two-finger tap: right-click")
                        .multilineTextAlignment(.center)
                        .foregroundStyle(.secondary)
                        .allowsHitTesting(false)
                }
            controls
        }
    }

    private var header: some View {
        HStack {
            Button("Trackpad") { onSwitchMode() }.font(.headline)
            Spacer()
            Button("Disconnect", action: onDisconnect)
        }
        .padding(8)
    }

    private var controls: some View {
        VStack(spacing: 8) {
            HStack {
                Text(String(format: "Pointer speed: %.1f×", sensitivity)).font(.caption)
                Spacer()
            }
            Slider(value: $sensitivity, in: 0.5...4.0) { editing in
                if !editing { ConnectionStore.saveSensitivity(sensitivity) }
            }
            HStack(spacing: 8) {
                Button("Left click")  { click(0) }.frame(maxWidth: .infinity).buttonStyle(.bordered)
                Button("Right click") { click(1) }.frame(maxWidth: .infinity).buttonStyle(.bordered)
            }
        }
        .padding(8)
    }

    private func click(_ button: Int32) {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        session.sendMouseButton(button: button, pressed: true)
        session.sendMouseButton(button: button, pressed: false)
    }
}

// MARK: - Gesture surface

/// Bridges UIView multi-touch into SwiftUI. Reads `sensitivity` dynamically so
/// the slider takes effect without restarting gesture recognition.
private struct TrackpadSurface: UIViewRepresentable {
    let session: ExtenderSession
    let sensitivity: Float

    func makeUIView(context: Context) -> TrackpadUIView {
        let view = TrackpadUIView()
        view.session = session
        view.sensitivity = sensitivity
        return view
    }

    func updateUIView(_ uiView: TrackpadUIView, context: Context) {
        uiView.sensitivity = sensitivity
    }
}

final class TrackpadUIView: UIView {
    var session: ExtenderSession?
    var sensitivity: Float = 1.0

    private let scrollDivisor: Float = 40
    private let tapSlop: Float = 16

    private var lastPositions: [UITouch: CGPoint] = [:]
    private var totalMoved: Float = 0
    private var maxPointers = 0

    override init(frame: CGRect) {
        super.init(frame: frame)
        isMultipleTouchEnabled = true
    }

    required init?(coder: NSCoder) { fatalError() }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        // Reset on first finger of a new gesture (no fingers were down before).
        let activeBefore = (event?.allTouches ?? []).filter {
            $0.phase != .ended && $0.phase != .cancelled && !touches.contains($0)
        }.count
        if activeBefore == 0 {
            lastPositions.removeAll()
            totalMoved = 0
            maxPointers = 0
        }
        for touch in touches {
            lastPositions[touch] = touch.location(in: self)
        }
        let activeNow = (event?.allTouches ?? []).filter {
            $0.phase != .ended && $0.phase != .cancelled
        }.count
        maxPointers = max(maxPointers, activeNow)
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        let active = (event?.allTouches ?? []).filter {
            $0.phase != .ended && $0.phase != .cancelled
        }
        maxPointers = max(maxPointers, active.count)

        // Average the delta across all moving touches (centroid tracking).
        var dx: Float = 0; var dy: Float = 0; var count = 0
        for touch in touches {
            let pos = touch.location(in: self)
            if let last = lastPositions[touch] {
                dx += Float(pos.x - last.x)
                dy += Float(pos.y - last.y)
                count += 1
            }
            lastPositions[touch] = pos
        }
        guard count > 0 else { return }
        dx /= Float(count); dy /= Float(count)
        totalMoved += abs(dx) + abs(dy)

        if maxPointers >= 2 {
            session?.sendScroll(dx: dx / scrollDivisor * sensitivity,
                                dy: -dy / scrollDivisor * sensitivity)
        } else {
            session?.sendMouseMoveRelative(dx: dx * sensitivity, dy: dy * sensitivity)
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        let remaining = (event?.allTouches ?? []).filter {
            $0.phase != .ended && $0.phase != .cancelled
        }.count
        if remaining == 0, totalMoved < tapSlop {
            let button: Int32 = maxPointers >= 2 ? 1 : 0
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            session?.sendMouseButton(button: button, pressed: true)
            session?.sendMouseButton(button: button, pressed: false)
        }
        for touch in touches { lastPositions.removeValue(forKey: touch) }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        lastPositions.removeAll(); totalMoved = 0; maxPointers = 0
    }
}
