import SwiftUI
import UIKit

/// Trackpad mode: the phone acts as a touchpad over a control-only (no-video)
/// session. One-finger drag moves the cursor (relative), a tap left-clicks,
/// two-finger drag scrolls, and a two-finger tap right-clicks. Click-and-drag
/// works two ways: a "tap-and-a-half" gesture (tap, then tap-hold-move) and the
/// Drag-lock button, which holds the left button so a one-finger move drags.
/// Matches Android.
struct TrackpadView: View {
    let session: ExtenderSession
    let onDisconnect: () -> Void
    let onSwitchMode: () -> Void

    @State private var sensitivity: Float = ConnectionStore.loadSensitivity()
    /// When locked, the pad ignores all touches and the buttons/slider are disabled,
    /// so a stray hand can't move the cursor; only the central lock toggle stays live.
    @State private var locked = false
    /// When set, the left button is held down so a one-finger move drags; tapping
    /// "Drop" releases it. The surface reads this to suppress the tap-to-click.
    @State private var dragLock = false

    var body: some View {
        VStack(spacing: 0) {
            ConnectedHeader(mode: .trackpad, onSwitchMode: onSwitchMode, onDisconnect: onDisconnect)
            Divider()
            TrackpadSurface(session: session, sensitivity: sensitivity, dragLock: dragLock)
                .allowsHitTesting(!locked)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color(uiColor: .secondarySystemBackground))
                .overlay(alignment: .center) {
                    VStack(spacing: 16) {
                        // The lock sits in the middle of the pad — a direct tap toggles
                        // it; its touches are captured so they never move the cursor.
                        LockToggle(locked: locked) {
                            locked.toggle()
                            if locked { setDragLock(false) }
                        }
                        Text(locked
                             ? "Locked — tap the lock to unlock"
                             : dragLock
                                ? "Dragging — move to drag\nTap “Drop” to release"
                                : "Trackpad\n\nDrag to move  •  tap to click  •  double-tap-drag to drag\nTwo fingers: scroll  •  two-finger tap: right-click")
                            .multilineTextAlignment(.center)
                            .foregroundStyle(.secondary)
                            .allowsHitTesting(false)
                    }
                }
            controls
        }
        // Safety net: never leave a button stuck down if we navigate away mid-drag.
        .onDisappear { if dragLock { session.sendMouseButton(button: 0, pressed: false) } }
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
                if dragLock {
                    Button("Drop") { setDragLock(false) }
                        .frame(maxWidth: .infinity).buttonStyle(.borderedProminent)
                } else {
                    Button("Drag") { setDragLock(true) }
                        .frame(maxWidth: .infinity).buttonStyle(.bordered)
                }
                Button("Right click") { click(1) }.frame(maxWidth: .infinity).buttonStyle(.bordered)
            }
        }
        .padding(8)
        .disabled(locked)
    }

    private func click(_ button: Int32) {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        session.sendMouseButton(button: button, pressed: true)
        session.sendMouseButton(button: button, pressed: false)
    }

    // Hold (or release) the left button so one-finger moves drag.
    private func setDragLock(_ on: Bool) {
        guard on != dragLock else { return }
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        session.sendMouseButton(button: 0, pressed: on)
        dragLock = on
    }
}

// MARK: - Gesture surface

/// Bridges UIView multi-touch into SwiftUI. Reads `sensitivity` dynamically so
/// the slider takes effect without restarting gesture recognition.
private struct TrackpadSurface: UIViewRepresentable {
    let session: ExtenderSession
    let sensitivity: Float
    let dragLock: Bool

    func makeUIView(context: Context) -> TrackpadUIView {
        let view = TrackpadUIView()
        view.session = session
        view.sensitivity = sensitivity
        view.dragLock = dragLock
        return view
    }

    func updateUIView(_ uiView: TrackpadUIView, context: Context) {
        uiView.sensitivity = sensitivity
        uiView.dragLock = dragLock
    }
}

final class TrackpadUIView: UIView {
    var session: ExtenderSession?
    var sensitivity: Float = 1.0
    /// Driven by the Drag-lock button: the left button is held outside the pad,
    /// so a one-finger move drags and a stationary lift must not emit a click.
    var dragLock: Bool = false

    private let scrollDivisor: Float = 40
    private let tapSlop: Float = 16
    private let doubleTapWindow: CFTimeInterval = 0.3

    private var lastPositions: [UITouch: CGPoint] = [:]
    private var totalMoved: Float = 0
    private var maxPointers = 0
    /// A gesture that closely follows a tap can become a "tap-and-a-half" drag.
    private var lastTapUp: CFTimeInterval = 0
    private var tapAndHalf = false
    private var pressedForDrag = false

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
            pressedForDrag = false
            tapAndHalf = CACurrentMediaTime() - lastTapUp < doubleTapWindow
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
            // Begin a tap-and-a-half drag once the move clears the tap slop.
            if tapAndHalf, !dragLock, !pressedForDrag, totalMoved >= tapSlop {
                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                session?.sendMouseButton(button: 0, pressed: true)
                pressedForDrag = true
            }
            session?.sendMouseMoveRelative(dx: dx * sensitivity, dy: dy * sensitivity)
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        let remaining = (event?.allTouches ?? []).filter {
            $0.phase != .ended && $0.phase != .cancelled
        }.count
        if remaining == 0 {
            if pressedForDrag {
                // End a tap-and-a-half drag on lift.
                session?.sendMouseButton(button: 0, pressed: false)
            } else if !dragLock, totalMoved < tapSlop {
                // A near-stationary lift is a click (two fingers = right). Drag-lock
                // keeps the button held across lifts, so it skips this.
                let button: Int32 = maxPointers >= 2 ? 1 : 0
                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                session?.sendMouseButton(button: button, pressed: true)
                session?.sendMouseButton(button: button, pressed: false)
                lastTapUp = CACurrentMediaTime()
            }
        }
        for touch in touches { lastPositions.removeValue(forKey: touch) }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        if pressedForDrag { session?.sendMouseButton(button: 0, pressed: false) }
        lastPositions.removeAll(); totalMoved = 0; maxPointers = 0; pressedForDrag = false
    }
}
