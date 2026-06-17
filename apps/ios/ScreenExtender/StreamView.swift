import AVFoundation
import SwiftUI
import UIKit

/// Streams the host's screen (viewer / full-control). Decodes with VideoToolbox
/// into an `AVSampleBufferDisplayLayer`; when `forwardInput` is true, touches are
/// sent back as normalized pointer input.
///
/// > Unverified scaffold — not compiled or run (no Xcode/Mac here).
struct StreamView: View {
    let session: ExtenderSession
    let addr: String
    let forwardInput: Bool
    let onDisconnect: () -> Void

    var body: some View {
        ZStack(alignment: .topTrailing) {
            VideoLayerView(session: session, forwardInput: forwardInput)
                .ignoresSafeArea()
            Button("Disconnect", action: onDisconnect)
                .padding()
                .background(.ultraThinMaterial, in: Capsule())
                .padding()
        }
    }
}

/// Bridges the `AVSampleBufferDisplayLayer`-backed UIView into SwiftUI and starts
/// the event pump feeding the decoder.
private struct VideoLayerView: UIViewRepresentable {
    let session: ExtenderSession
    let forwardInput: Bool

    func makeUIView(context: Context) -> SampleBufferView {
        let view = SampleBufferView()
        view.session = session
        view.forwardInput = forwardInput

        var sink = ExtenderSession.Sink()
        sink.onStart = { [weak view] _, _, codec, csd in
            // Layer touch must be on the main thread.
            DispatchQueue.main.async { view?.makeDecoder(codec: codec, csd: csd) }
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

    // MARK: - Touch forwarding (full-control), normalized [0,1] from top-left

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
