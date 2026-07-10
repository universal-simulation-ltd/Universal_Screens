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
///  • **Remote control** forwards touches to the host as normalized pointer input,
///    so a dim press-and-hold handle (not a tap) toggles the bar;
///  • a `ConnectedHeader` (mode chip → re-pick, + Disconnect) overlays the top when
///    shown, floating over the video on a translucent gradient (matching Android).
///
/// > Unverified scaffold — authored without Xcode/a Mac, so not compiled or run.
/// > The gesture/decoder paths in particular want on-device testing.
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
    /// Committed zoom + pan (Mirror only); live gesture deltas layer on top.
    @State private var scale: CGFloat = 1
    @State private var offset: CGSize = .zero
    @GestureState private var pinch: CGFloat = 1
    @GestureState private var pan: CGSize = .zero

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .top) {
                Color.black.ignoresSafeArea()
                picture(in: geo.size).ignoresSafeArea()
                if forwardInput { controlHandle }
                if chrome { header }
            }
        }
    }

    // MARK: - Video

    @ViewBuilder private func picture(in size: CGSize) -> some View {
        let effScale = forwardInput ? 1 : clampScale(scale * pinch)
        let effOffset = forwardInput ? .zero
            : clampOffset(CGSize(width: offset.width + pan.width, height: offset.height + pan.height),
                          scale: effScale, container: size)

        let layer = VideoLayerView(session: session, forwardInput: forwardInput) { aspect in
                videoAspect = aspect
            }
            .aspectRatio(videoAspect, contentMode: .fit)
            .scaleEffect(effScale)
            .offset(effOffset)
            .frame(maxWidth: .infinity, maxHeight: .infinity)

        if forwardInput {
            // Control: the UIView forwards touches to the host; no zoom/pan/tap here.
            layer
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

    /// Remote control forwards every tap to the host, so the bar can't be toggled by
    /// tapping the picture. This dim, always-present handle does it: press and hold
    /// to show/hide the bar. Only the handle itself captures touches — the rest of
    /// the screen still forwards to the host.
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
}

/// Bridges the `AVSampleBufferDisplayLayer`-backed UIView into SwiftUI, starts the
/// event pump feeding the decoder, and reports the host's aspect ratio on Start.
private struct VideoLayerView: UIViewRepresentable {
    let session: ExtenderSession
    let forwardInput: Bool
    /// Called on the main queue with width/height once the stream starts.
    let onAspect: (CGFloat) -> Void

    func makeUIView(context: Context) -> SampleBufferView {
        let view = SampleBufferView()
        view.session = session
        view.forwardInput = forwardInput
        // In Mirror the UIView must not swallow touches, so SwiftUI's zoom/pan/tap
        // gestures layered above it receive them; in Remote control it forwards them.
        view.isUserInteractionEnabled = forwardInput

        var sink = ExtenderSession.Sink()
        sink.onStart = { [weak view] w, h, codec, csd in
            // Layer work must be on the main thread.
            DispatchQueue.main.async {
                if h > 0 { onAspect(CGFloat(w) / CGFloat(h)) }
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

    var session: ExtenderSession?
    var forwardInput = false
    var decoder: VideoDecoder?

    func makeDecoder(codec: Int, csd: Data) {
        displayLayer.videoGravity = .resizeAspect
        let decoder = VideoDecoder(layer: displayLayer, codec: codec)
        decoder.setParameterSets(csd)
        self.decoder = decoder
    }

    // MARK: - Touch forwarding (Remote control), normalized [0,1] from top-left

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) { forward(touches, EXTENDER_TOUCH_BEGAN) }
    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) { forward(touches, EXTENDER_TOUCH_MOVED) }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) { forward(touches, EXTENDER_TOUCH_ENDED) }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) { forward(touches, EXTENDER_TOUCH_CANCELLED) }

    private func forward(_ touches: Set<UITouch>, _ phase: ExtenderTouchPhase) {
        guard forwardInput, let session, let touch = touches.first, bounds.width > 0, bounds.height > 0 else { return }
        let point = touch.location(in: self)
        let x = Float((point.x / bounds.width).clamped01)
        let y = Float((point.y / bounds.height).clamped01)
        session.sendTouch(id: 0, phase: phase, x: x, y: y)
    }
}

private extension CGFloat {
    var clamped01: CGFloat { Swift.max(0, Swift.min(1, self)) }
}
