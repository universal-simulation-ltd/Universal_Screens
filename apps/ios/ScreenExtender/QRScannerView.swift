import AVFoundation
import SwiftUI

/// A live camera viewfinder that decodes QR codes and calls `onScan` with the
/// first result, then stops. Present as a full-screen sheet from the connect screen.
struct QRScannerView: View {
    let onScan: (String) -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        ZStack(alignment: .bottom) {
            QRCaptureView(onScan: { text in
                dismiss()
                onScan(text)
            })
            .ignoresSafeArea()
            Button("Cancel") { dismiss() }
                .padding()
                .background(.ultraThinMaterial, in: Capsule())
                .padding(.bottom, 40)
        }
    }
}

// MARK: - AVFoundation bridge

private struct QRCaptureView: UIViewRepresentable {
    let onScan: (String) -> Void

    func makeUIView(context: Context) -> QRCaptureUIView {
        let view = QRCaptureUIView()
        view.onScan = onScan
        return view
    }

    func updateUIView(_ uiView: QRCaptureUIView, context: Context) {}
}

final class QRCaptureUIView: UIView {
    var onScan: ((String) -> Void)?

    private let captureSession = AVCaptureSession()
    private var previewLayer: AVCaptureVideoPreviewLayer?
    private var started = false

    override func didMoveToWindow() {
        super.didMoveToWindow()
        guard window != nil, !started else { return }
        started = true
        setup()
    }

    private func setup() {
        guard let device = AVCaptureDevice.default(for: .video),
              let input = try? AVCaptureDeviceInput(device: device),
              captureSession.canAddInput(input) else { return }
        captureSession.addInput(input)

        let output = AVCaptureMetadataOutput()
        guard captureSession.canAddOutput(output) else { return }
        captureSession.addOutput(output)
        output.setMetadataObjectsDelegate(self, queue: .main)
        output.metadataObjectTypes = [.qr]

        let preview = AVCaptureVideoPreviewLayer(session: captureSession)
        preview.videoGravity = .resizeAspectFill
        preview.frame = bounds
        layer.insertSublayer(preview, at: 0)
        previewLayer = preview

        DispatchQueue.global(qos: .userInitiated).async { self.captureSession.startRunning() }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        previewLayer?.frame = bounds
    }
}

extension QRCaptureUIView: AVCaptureMetadataOutputObjectsDelegate {
    func metadataOutput(
        _ output: AVCaptureMetadataOutput,
        didOutput objects: [AVMetadataObject],
        from connection: AVCaptureConnection
    ) {
        guard let obj = objects.first as? AVMetadataMachineReadableCodeObject,
              let text = obj.stringValue, !text.isEmpty else { return }
        captureSession.stopRunning()
        onScan?(text)
    }
}
