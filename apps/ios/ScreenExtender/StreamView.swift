import AVFoundation
import SwiftUI
import UIKit

/// Streams the host's screen (Mirror / Remote control / Second screen), at parity
/// with the Android `StreamScreen`:
///
///  • the picture is **letterboxed** to the host's aspect ratio (learned from the
///    Start event) instead of stretched;
///  • **Mirror** supports pinch-to-zoom (1–5×) + drag-to-pan, and a tap toggles the
///    top bar so the picture can fill the screen;
///  • **Remote control** lays a trackpad over the picture (`RemoteSurfaceView`):
///    one finger moves the pointer, a quick tap clicks, two fingers zoom and pan
///    the picture on the phone (or scroll the host at 1×) without ever clicking,
///    and Left / Right buttons sit along the bottom — double-tap Left to hold it
///    down for selecting text. A dim press-and-hold handle toggles the top bar;
///  • a `ConnectedHeader` (mode chip → re-pick, + Disconnect) overlays the top when
///    shown, floating over the video on a translucent gradient (matching Android).
struct StreamView: View {
    let session: ExtenderSession
    let addr: String
    let mode: Mode
    let forwardInput: Bool
    let onDisconnect: () -> Void
    var onSwitchMode: (() -> Void)? = nil

    /// Whether the top bar is shown (a tap in Mirror / a hold on the handle in Control).
    @State private var chrome = true
    /// The host's screen aspect ratio (w/h), learned from Start, so we letterbox.
    @State private var videoAspect: CGFloat?
    /// The stream's size in pixels, also from Start: Remote control turns a
    /// finger's travel across the picture into that many host pixels.
    @State private var videoSize: CGSize = .zero
    /// Committed zoom + pan. Mirror layers its live gesture deltas on top; in
    /// Remote control `RemoteSurfaceView` writes them directly.
    @State private var scale: CGFloat = 1
    @State private var offset: CGSize = .zero
    @GestureState private var pinch: CGFloat = 1
    @GestureState private var pan: CGSize = .zero
    /// Remote control: the left button is being held (double-tap Left), so a
    /// one-finger move drags — for selecting text or moving a window.
    @State private var leftHeld = false
    /// A single tap on Left waits briefly for a second tap before it clicks.
    @State private var pendingLeft: DispatchWorkItem?
    /// The gesture hint above the buttons, shown for the first few seconds.
    @State private var showHint = true

    /// How long a tap on Left waits for a second tap, which would hold it instead.
    private let doubleTapWindow: TimeInterval = 0.3

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .top) {
                Color.black.ignoresSafeArea()
                picture(in: geo.size).ignoresSafeArea()
                if forwardInput {
                    RemoteSurface(session: session, videoSize: videoSize, videoAspect: videoAspect,
                                  scale: scale, offset: offset, leftHeld: leftHeld) { s, o in
                        scale = s
                        offset = o
                    }
                    .ignoresSafeArea()
                    controlHandle
                    buttonBar
                }
                if chrome { header }
            }
        }
        .onDisappear {
            // Safety net: never leave the left button stuck down on the host.
            pendingLeft?.cancel()
            if leftHeld { session.sendMouseButton(button: 0, pressed: false) }
        }
    }

    // MARK: - Video

    @ViewBuilder private func picture(in size: CGSize) -> some View {
        let effScale = forwardInput ? scale : clampScale(scale * pinch)
        let effOffset = forwardInput ? offset
            : clampOffset(CGSize(width: offset.width + pan.width, height: offset.height + pan.height),
                          scale: effScale, container: size)

        let layer = VideoLayerView(session: session) { w, h in
                videoSize = CGSize(width: w, height: h)
                videoAspect = CGFloat(w) / CGFloat(h)
            }
            .aspectRatio(videoAspect, contentMode: .fit)
            .scaleEffect(effScale)
            .offset(effOffset)
            .frame(maxWidth: .infinity, maxHeight: .infinity)

        if forwardInput {
            // Control: RemoteSurfaceView, laid over the top, takes every touch.
            layer.allowsHitTesting(false)
        } else {
            // Mirror: pinch-zoom + drag-pan; a clean tap toggles the top bar.
            layer
                .contentShape(Rectangle())
                .gesture(zoomPanGesture(container: size))
                .onTapGesture { withAnimation(.easeInOut(duration: 0.2)) { chrome.toggle() } }
        }
    }

    private func zoomPanGesture(container: CGSize) -> some Gesture {
        let magnify = MagnificationGesture()
            .updating($pinch) { value, state, _ in state = value }
            .onEnded { value in
                scale = clampScale(scale * value)
                if scale <= 1 { offset = .zero }
            }
        let drag = DragGesture()
            .updating($pan) { value, state, _ in state = value.translation }
            .onEnded { value in
                offset = clampOffset(
                    CGSize(width: offset.width + value.translation.width,
                           height: offset.height + value.translation.height),
                    scale: scale, container: container)
            }
        return magnify.simultaneously(with: drag)
    }

    private func clampScale(_ s: CGFloat) -> CGFloat { min(max(s, 1), 5) }

    /// Keep the panned picture within the zoomed bounds (mirrors the Android clamp:
    /// `size * (scale - 1) / 2`). No pan when not zoomed.
    private func clampOffset(_ o: CGSize, scale: CGFloat, container: CGSize) -> CGSize {
        guard scale > 1 else { return .zero }
        let maxX = container.width * (scale - 1) / 2
        let maxY = container.height * (scale - 1) / 2
        return CGSize(width: min(max(o.width, -maxX), maxX),
                      height: min(max(o.height, -maxY), maxY))
    }

    // MARK: - Overlays

    private var header: some View {
        ConnectedHeader(mode: mode, onSwitchMode: onSwitchMode, onDisconnect: onDisconnect)
            .background(
                LinearGradient(colors: [.black.opacity(0.55), .clear],
                               startPoint: .top, endPoint: .bottom)
            )
            .transition(.opacity)
    }

    /// Remote control turns touches into pointer input, so the bar can't be toggled
    /// by tapping the picture. This dim, always-present handle does it: press and
    /// hold to show/hide the bar. Only the handle itself captures touches — the rest
    /// of the screen still drives the host.
    private var controlHandle: some View {
        VStack {
            HStack {
                Spacer()
                Image("AppLogo")
                    .resizable().scaledToFit()
                    .frame(width: 44, height: 44)
                    .opacity(0.4)
                    .padding(12)
                    .contentShape(Rectangle())
                    .onLongPressGesture(minimumDuration: 0.4) {
                        withAnimation(.easeInOut(duration: 0.2)) { chrome.toggle() }
                    }
                    .accessibilityLabel("Press and hold to show or hide the controls")
            }
            Spacer()
        }
    }

    /// Left / Right mouse buttons, plus a zoom chip while zoomed. Empty space in
    /// this stack isn't hit-testable, so touches there still reach the surface.
    private var buttonBar: some View {
        VStack(spacing: 8) {
            Spacer()
            if scale > 1.01 {
                Button {
                    withAnimation(.easeInOut(duration: 0.2)) { scale = 1; offset = .zero }
                } label: {
                    Text(String(format: "%.1f× · Reset zoom", scale))
                        .font(.footnote.weight(.medium))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 12).padding(.vertical, 6)
                        .background(Color.black.opacity(0.6), in: Capsule())
                }
                .buttonStyle(.plain)
            }
            if showHint && !leftHeld {
                Text("Drag to move · tap to click · two fingers to zoom or scroll")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(0.8))
                    .padding(.horizontal, 10).padding(.vertical, 4)
                    .background(Color.black.opacity(0.45), in: Capsule())
                    .allowsHitTesting(false)
                    .transition(.opacity)
            }
            HStack(spacing: 8) {
                mouseButton(leftHeld ? "Left held" : "Left",
                            hint: leftHeld ? "Tap to let go" : "Double-tap to hold",
                            active: leftHeld, action: leftTap)
                mouseButton("Right", hint: nil, active: false) { click(1) }
            }
        }
        .padding(.horizontal, 8)
        .padding(.bottom, 8)
        .onAppear {
            DispatchQueue.main.asyncAfter(deadline: .now() + 6) {
                withAnimation(.easeInOut(duration: 0.4)) { showHint = false }
            }
        }
    }

    private func mouseButton(_ title: String, hint: String?, active: Bool,
                             action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(spacing: 2) {
                Text(title).font(.body.weight(.semibold))
                if let hint { Text(hint).font(.caption2).opacity(0.8) }
            }
            .foregroundStyle(.white)
            .frame(maxWidth: .infinity, minHeight: 56)
            .background(active ? Color.brandOrange : Color.black.opacity(0.6),
                        in: RoundedRectangle(cornerRadius: 12))
            .contentShape(RoundedRectangle(cornerRadius: 12))
        }
        .buttonStyle(.plain)
    }

    // MARK: - Mouse buttons

    private func click(_ button: Int32) {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        session.sendMouseButton(button: button, pressed: true)
        session.sendMouseButton(button: button, pressed: false)
    }

    private func setLeftHeld(_ on: Bool) {
        guard on != leftHeld else { return }
        UIImpactFeedbackGenerator(style: .medium).impactOccurred()
        session.sendMouseButton(button: 0, pressed: on)
        leftHeld = on
    }

    /// One tap clicks — held back a moment in case a second tap follows, because
    /// click-then-press would reach the host as a double-click (which maximises a
    /// window you meant to drag). A second tap in time holds the button instead,
    /// and a tap while it's held lets go.
    private func leftTap() {
        if leftHeld {
            setLeftHeld(false)
        } else if let pending = pendingLeft {
            pending.cancel()
            pendingLeft = nil
            setLeftHeld(true)
        } else {
            let work = DispatchWorkItem {
                pendingLeft = nil
                click(0)
            }
            pendingLeft = work
            DispatchQueue.main.asyncAfter(deadline: .now() + doubleTapWindow, execute: work)
        }
    }
}

/// Bridges the `AVSampleBufferDisplayLayer`-backed UIView into SwiftUI, starts the
/// event pump feeding the decoder, and reports the host's stream size on Start.
private struct VideoLayerView: UIViewRepresentable {
    let session: ExtenderSession
    /// Called on the main queue with the stream's width/height once it starts.
    let onStart: (Int, Int) -> Void

    func makeUIView(context: Context) -> SampleBufferView {
        let view = SampleBufferView()
        // The picture never takes touches: Mirror's gestures are SwiftUI's, laid
        // over it, and Remote control has its own surface on top.
        view.isUserInteractionEnabled = false

        var sink = ExtenderSession.Sink()
        sink.onStart = { [weak view] w, h, codec, csd in
            // Layer work must be on the main thread.
            DispatchQueue.main.async {
                if w > 0, h > 0 { onStart(Int(w), Int(h)) }
                view?.makeDecoder(codec: codec, csd: csd)
            }
        }
        sink.onFrame = { [weak view] data, keyframe, pts in
            view?.decoder?.decode(annexB: data, keyframe: keyframe, ptsValue: pts)
        }
        session.startPump(sink)
        return view
    }

    func updateUIView(_ uiView: SampleBufferView, context: Context) {}
}

/// A UIView whose backing layer is an `AVSampleBufferDisplayLayer`.
final class SampleBufferView: UIView {
    override class var layerClass: AnyClass { AVSampleBufferDisplayLayer.self }
    private var displayLayer: AVSampleBufferDisplayLayer { layer as! AVSampleBufferDisplayLayer }

    var decoder: VideoDecoder?

    func makeDecoder(codec: Int, csd: Data) {
        displayLayer.videoGravity = .resizeAspect
        let decoder = VideoDecoder(layer: displayLayer, codec: codec)
        decoder.setParameterSets(csd)
        self.decoder = decoder
    }
}

// MARK: - Remote control surface

private struct RemoteSurface: UIViewRepresentable {
    let session: ExtenderSession
    let videoSize: CGSize
    let videoAspect: CGFloat?
    let scale: CGFloat
    let offset: CGSize
    let leftHeld: Bool
    let onTransform: (CGFloat, CGSize) -> Void

    func makeUIView(context: Context) -> RemoteSurfaceView {
        let view = RemoteSurfaceView()
        view.session = session
        update(view)
        return view
    }

    func updateUIView(_ uiView: RemoteSurfaceView, context: Context) { update(uiView) }

    private func update(_ view: RemoteSurfaceView) {
        view.videoSize = videoSize
        view.videoAspect = videoAspect
        view.leftHeld = leftHeld
        view.onTransform = onTransform
        // Mid-gesture the view's own zoom is the truth (SwiftUI's copy lags a
        // frame behind); otherwise take SwiftUI's, e.g. after "Reset zoom".
        if !view.inGesture {
            view.scale = scale
            view.offset = offset
        }
    }
}

/// Remote control over the streamed picture. Nothing a finger does here clicks
/// unless it is meant to:
///
///  • **one finger** moves the pointer — relative, like a trackpad laid over the
///    picture, scaled so the pointer crosses as much of the host's screen as the
///    finger crosses of the picture (so it gets finer as you zoom in);
///  • a **quick tap** left-clicks (skipped while Left is held), a quick
///    **two-finger tap** right-clicks;
///  • **two fingers** pinch to zoom and drag to pan the picture on the phone only
///    — or, at 1×, drag to scroll the host. Nothing is clicked while zooming or
///    panning. Which one a gesture is gets decided early and kept.
///
/// It is all relative motion, buttons and scroll, which every host injects;
/// touches and absolute positions reach only the Mac host, so they aren't used.
final class RemoteSurfaceView: UIView {
    var session: ExtenderSession?
    var videoSize: CGSize = .zero
    var videoAspect: CGFloat?
    var leftHeld = false
    var onTransform: ((CGFloat, CGSize) -> Void)?
    var scale: CGFloat = 1
    var offset: CGSize = .zero
    /// True while any finger is down.
    private(set) var inGesture = false

    private enum TwoFinger { case undecided, view, scroll }

    private let tapSlop: CGFloat = 16
    private let tapMaxDuration: CFTimeInterval = 0.3
    /// Span change (as a fraction) that makes a two-finger gesture a zoom.
    private let zoomThreshold: CGFloat = 0.06
    /// Finger travel that makes a two-finger gesture a pan / scroll.
    private let travelThreshold: CGFloat = 24
    private let scrollDivisor: CGFloat = 40

    private var lastPositions: [UITouch: CGPoint] = [:]
    private var startTime: CFTimeInterval = 0
    private var moved: CGFloat = 0
    private var maxPointers = 0
    private var twoFinger = TwoFinger.undecided
    private var zoomSoFar: CGFloat = 1
    private var travel: CGFloat = 0
    /// Sub-pixel remainders, so slow or zoomed-in motion isn't rounded away (the
    /// Windows and Linux hosts inject whole pixels).
    private var carryX: CGFloat = 0
    private var carryY: CGFloat = 0

    override init(frame: CGRect) {
        super.init(frame: frame)
        isMultipleTouchEnabled = true
        backgroundColor = .clear
    }

    required init?(coder: NSCoder) { fatalError() }

    private func active(_ event: UIEvent?) -> [UITouch] {
        (event?.allTouches ?? []).filter { $0.phase != .ended && $0.phase != .cancelled }
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        let before = active(event).filter { !touches.contains($0) }.count
        if before == 0 {
            // First finger of a new gesture.
            lastPositions.removeAll()
            startTime = CACurrentMediaTime()
            moved = 0
            maxPointers = 0
            twoFinger = .undecided
            zoomSoFar = 1
            travel = 0
            inGesture = true
        }
        for touch in touches { lastPositions[touch] = touch.location(in: self) }
        maxPointers = max(maxPointers, active(event).count)
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        let live = active(event).filter { lastPositions[$0] != nil }
        maxPointers = max(maxPointers, live.count)
        defer { for touch in live { lastPositions[touch] = touch.location(in: self) } }

        if maxPointers >= 2 {
            // Once a second finger has landed, the gesture is about the picture
            // (or scrolling) until every finger lifts — never pointer motion.
            guard live.count >= 2 else { return }
            let now = centroidAndSpan(live.map { $0.location(in: self) })
            let before = centroidAndSpan(live.compactMap { lastPositions[$0] })
            let pan = CGSize(width: now.centroid.x - before.centroid.x,
                             height: now.centroid.y - before.centroid.y)
            let zoom = before.span > 0 ? now.span / before.span : 1
            zoomSoFar *= zoom
            travel += hypot(pan.width, pan.height)
            moved += abs(pan.width) + abs(pan.height)
            if twoFinger == .undecided {
                if abs(zoomSoFar - 1) > zoomThreshold {
                    twoFinger = .view
                } else if travel > travelThreshold {
                    twoFinger = scale > 1 ? .view : .scroll
                }
            }
            switch twoFinger {
            case .view:
                zoomPan(zoom: zoom, pan: pan, focus: now.centroid)
            case .scroll:
                // Natural direction, like the Trackpad mode.
                session?.sendScroll(dx: Float(pan.width / scrollDivisor),
                                    dy: Float(-pan.height / scrollDivisor))
            case .undecided:
                break
            }
        } else if let touch = live.first, let last = lastPositions[touch] {
            let pos = touch.location(in: self)
            let dx = pos.x - last.x
            let dy = pos.y - last.y
            moved += abs(dx) + abs(dy)
            movePointer(dx: dx, dy: dy)
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        for touch in touches { lastPositions.removeValue(forKey: touch) }
        guard active(event).isEmpty else { return }
        inGesture = false
        // A quick, near-stationary touch that never became a zoom/pan/scroll is a
        // click: two fingers = right. Left is skipped while it's held down.
        let quick = CACurrentMediaTime() - startTime < tapMaxDuration
        if quick, moved < tapSlop, twoFinger == .undecided {
            if maxPointers >= 2 {
                click(1)
            } else if !leftHeld {
                click(0)
            }
        }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        for touch in touches { lastPositions.removeValue(forKey: touch) }
        if active(event).isEmpty {
            lastPositions.removeAll()
            inGesture = false
        }
    }

    private func click(_ button: Int32) {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        session?.sendMouseButton(button: button, pressed: true)
        session?.sendMouseButton(button: button, pressed: false)
    }

    /// The picture's on-screen size at 1× — aspect-fit into this view, which covers
    /// the same area as the picture's container.
    private func fittedSize() -> CGSize {
        guard let aspect = videoAspect, aspect > 0, bounds.height > 0 else { return bounds.size }
        if bounds.width / bounds.height > aspect {
            return CGSize(width: bounds.height * aspect, height: bounds.height)
        }
        return CGSize(width: bounds.width, height: bounds.width / aspect)
    }

    private func movePointer(dx: CGFloat, dy: CGFloat) {
        // Host pixels per point of finger travel at the current zoom.
        let shown = fittedSize().width * scale
        let k = videoSize.width > 0 && shown > 0 ? videoSize.width / shown : 1
        carryX += dx * k
        carryY += dy * k
        let ix = carryX.rounded(.towardZero)
        let iy = carryY.rounded(.towardZero)
        guard ix != 0 || iy != 0 else { return }
        carryX -= ix
        carryY -= iy
        session?.sendMouseMoveRelative(dx: Float(ix), dy: Float(iy))
    }

    /// Zoom by `zoom` about `focus` (so the point under the fingers stays put), pan
    /// by `pan`, and keep the picture covering the screen along any axis where it
    /// overflows it.
    private func zoomPan(zoom: CGFloat, pan: CGSize, focus: CGPoint) {
        let s0 = scale
        let s1 = min(max(s0 * zoom, 1), 5)
        let f = CGSize(width: focus.x - bounds.midX, height: focus.y - bounds.midY)
        let ratio = s1 / s0
        var o = CGSize(width: f.width - (f.width - offset.width) * ratio + pan.width,
                       height: f.height - (f.height - offset.height) * ratio + pan.height)
        let fit = fittedSize()
        let maxX = max(0, (fit.width * s1 - bounds.width) / 2)
        let maxY = max(0, (fit.height * s1 - bounds.height) / 2)
        o.width = min(max(o.width, -maxX), maxX)
        o.height = min(max(o.height, -maxY), maxY)
        scale = s1
        offset = o
        onTransform?(s1, o)
    }

    /// The centroid of `points` and their mean distance from it.
    private func centroidAndSpan(_ points: [CGPoint]) -> (centroid: CGPoint, span: CGFloat) {
        guard !points.isEmpty else { return (.zero, 0) }
        let n = CGFloat(points.count)
        let c = CGPoint(x: points.map(\.x).reduce(0, +) / n, y: points.map(\.y).reduce(0, +) / n)
        let span = points.map { hypot($0.x - c.x, $0.y - c.y) }.reduce(0, +) / n
        return (c, span)
    }
}
