import AVFoundation
import CoreMedia
import Foundation

/// Decodes an Annex-B H.264 / HEVC stream to an `AVSampleBufferDisplayLayer`,
/// the iOS counterpart of the Android `VideoDecoder` (MediaCodec).
///
/// The FFI hands out **Annex-B** (start-code-delimited NALs); VideoToolbox wants a
/// format description built from the raw parameter-set NALs plus **AVCC**
/// (length-prefixed) sample buffers, so this converts between the two.
///
/// > Unverified scaffold — authored without Xcode/a Mac, so not compiled or run.
/// > The conversion + format-description steps in particular want on-device testing.
final class VideoDecoder {
    private let layer: AVSampleBufferDisplayLayer
    private let codec: Int // 0 = H.264, 1 = HEVC
    private var parameterSets: [[UInt8]] = []
    private var formatDescription: CMFormatDescription?

    init(layer: AVSampleBufferDisplayLayer, codec: Int) {
        self.layer = layer
        self.codec = codec
    }

    /// On a Start event: keep the Annex-B parameter sets and build the format
    /// description (SPS/PPS for H.264; VPS/SPS/PPS for HEVC).
    func setParameterSets(_ annexBCsd: Data) {
        parameterSets = Self.splitAnnexB(annexBCsd)
        formatDescription = Self.makeFormatDescription(codec: codec, nalus: parameterSets)
    }

    /// On a Frame event: build a `CMSampleBuffer` from the Annex-B access unit and
    /// enqueue it for display. Parameter-set NALs in the frame are folded into the
    /// format description rather than the sample.
    func decode(annexB: Data, keyframe: Bool, ptsValue: Int64) {
        if formatDescription == nil {
            formatDescription = Self.makeFormatDescription(codec: codec, nalus: parameterSets)
        }
        guard let format = formatDescription else { return }

        let avcc = Self.annexBToAvcc(annexB)
        guard !avcc.isEmpty, let block = Self.makeBlockBuffer(avcc) else { return }

        var sampleSize = avcc.count
        var timing = CMSampleTimingInfo(
            duration: .invalid,
            presentationTimeStamp: CMTime(value: ptsValue, timescale: 60),
            decodeTimeStamp: .invalid
        )
        var sample: CMSampleBuffer?
        let status = CMSampleBufferCreate(
            allocator: kCFAllocatorDefault,
            dataBuffer: block,
            dataReady: true,
            makeDataReadyCallback: nil,
            refcon: nil,
            formatDescription: format,
            sampleCount: 1,
            sampleTimingEntryCount: 1,
            sampleTimingArray: &timing,
            sampleSizeEntryCount: 1,
            sampleSizeArray: &sampleSize,
            sampleBufferOut: &sample
        )
        guard status == noErr, let sample else { return }

        // Display immediately (the host streams in order; we don't reorder).
        if let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: true),
           let dict = CFArrayGetValueAtIndex(attachments, 0) {
            let dictRef = unsafeBitCast(dict, to: CFMutableDictionary.self)
            CFDictionarySetValue(
                dictRef,
                Unmanaged.passUnretained(kCMSampleAttachmentKey_DisplayImmediately).toOpaque(),
                Unmanaged.passUnretained(kCFBooleanTrue).toOpaque()
            )
        }
        layer.enqueue(sample)
    }

    // MARK: - Annex-B helpers

    /// Split an Annex-B buffer into its NAL unit payloads (start codes removed).
    static func splitAnnexB(_ data: Data) -> [[UInt8]] {
        let bytes = [UInt8](data)
        let n = bytes.count
        func startCode(at p: Int) -> Int {
            if p + 3 < n, bytes[p] == 0, bytes[p + 1] == 0, bytes[p + 2] == 0, bytes[p + 3] == 1 { return 4 }
            if p + 2 < n, bytes[p] == 0, bytes[p + 1] == 0, bytes[p + 2] == 1 { return 3 }
            return 0
        }
        var nalus: [[UInt8]] = []
        var start = -1
        var i = 0
        while i < n {
            let sc = startCode(at: i)
            if sc > 0 {
                if start >= 0, i > start { nalus.append(Array(bytes[start..<i])) }
                i += sc
                start = i
            } else {
                i += 1
            }
        }
        if start >= 0, start < n { nalus.append(Array(bytes[start..<n])) }
        return nalus
    }

    /// Convert an Annex-B access unit to AVCC (each NAL prefixed by a 4-byte
    /// big-endian length).
    static func annexBToAvcc(_ data: Data) -> Data {
        var out = Data()
        for nal in splitAnnexB(data) {
            var len = UInt32(nal.count).bigEndian
            withUnsafeBytes(of: &len) { out.append(contentsOf: $0) }
            out.append(contentsOf: nal)
        }
        return out
    }

    private static func makeBlockBuffer(_ data: Data) -> CMBlockBuffer? {
        var block: CMBlockBuffer?
        let status = CMBlockBufferCreateWithMemoryBlock(
            allocator: kCFAllocatorDefault,
            memoryBlock: nil,
            blockLength: data.count,
            blockAllocator: kCFAllocatorDefault,
            customBlockSource: nil,
            offsetToData: 0,
            dataLength: data.count,
            flags: 0,
            blockBufferOut: &block
        )
        guard status == kCMBlockBufferNoErr, let block else { return nil }
        let copied = data.withUnsafeBytes { raw in
            CMBlockBufferReplaceDataBytes(
                with: raw.baseAddress!,
                blockBuffer: block,
                offsetIntoDestination: 0,
                dataLength: data.count
            )
        }
        return copied == kCMBlockBufferNoErr ? block : nil
    }

    /// Build a `CMVideoFormatDescription` from the raw parameter-set NALs. Copies
    /// the NALs into one contiguous buffer so the pointer array stays valid for the
    /// call.
    private static func makeFormatDescription(codec: Int, nalus: [[UInt8]]) -> CMFormatDescription? {
        let minSets = (codec == 1) ? 3 : 2 // HEVC: VPS+SPS+PPS; H.264: SPS+PPS
        guard nalus.count >= minSets else { return nil }

        var blob: [UInt8] = []
        var offsets: [Int] = []
        for nal in nalus {
            offsets.append(blob.count)
            blob.append(contentsOf: nal)
        }
        let sizes = nalus.map { $0.count }

        var format: CMFormatDescription?
        blob.withUnsafeBufferPointer { buf in
            guard let base = buf.baseAddress else { return }
            let pointers: [UnsafePointer<UInt8>] = offsets.map { base + $0 }
            pointers.withUnsafeBufferPointer { pptr in
                sizes.withUnsafeBufferPointer { sptr in
                    if codec == 1 {
                        CMVideoFormatDescriptionCreateFromHEVCParameterSets(
                            allocator: kCFAllocatorDefault,
                            parameterSetCount: nalus.count,
                            parameterSetPointers: pptr.baseAddress!,
                            parameterSetSizes: sptr.baseAddress!,
                            nalUnitHeaderLength: 4,
                            extensions: nil,
                            formatDescriptionOut: &format
                        )
                    } else {
                        CMVideoFormatDescriptionCreateFromH264ParameterSets(
                            allocator: kCFAllocatorDefault,
                            parameterSetCount: nalus.count,
                            parameterSetPointers: pptr.baseAddress!,
                            parameterSetSizes: sptr.baseAddress!,
                            nalUnitHeaderLength: 4,
                            formatDescriptionOut: &format
                        )
                    }
                }
            }
        }
        return format
    }
}
