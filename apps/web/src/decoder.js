// H.264 decode via WebCodecs. Configured from a StreamStart's SPS/PPS (the host
// emits AVCC frames + raw parameter sets), so each Frame's bytes feed straight
// through as an EncodedVideoChunk in `avcC` mode — no Annex-B conversion. The
// avcC box + codec string are built by the WASM shim (single source of truth).
import { protocol } from "./wasm.js";

export class H264Decoder {
  /// `onFrame(VideoFrame)` receives each decoded frame (and OWNS closing it).
  constructor(onFrame, onError) {
    this.onFrame = onFrame;
    this.onError = onError;
    this.decoder = null;
    this.waitingForKey = true;
  }

  /// (Re)configure from a StreamStart `DecodedMessage`. Returns the codec string.
  configureFromStreamStart(streamStart) {
    if (streamStart.parameter_set_count < 2) {
      throw new Error("StreamStart has no SPS/PPS to build a decoder config");
    }
    const sps = streamStart.parameter_set(0);
    const pps = streamStart.parameter_set(1);
    return this.configure(protocol.avc_codec_string(sps), protocol.avcc_description(sps, pps));
  }

  /// (Re)configure with an explicit codec string + avcC description.
  configure(codec, description) {
    this.close();
    this.decoder = new VideoDecoder({
      output: (frame) => this.onFrame(frame),
      error: (e) => this.onError?.(e),
    });
    this.decoder.configure({ codec, description, optimizeForLatency: true });
    this.waitingForKey = true;
    return codec;
  }

  /// Decode one Frame `DecodedMessage`. Drops delta frames until the first
  /// keyframe so the decoder never starts mid-GOP.
  decodeFrame(frame) {
    if (!this.decoder || this.decoder.state !== "configured") return;
    const key = frame.keyframe;
    if (this.waitingForKey && !key) return;
    this.waitingForKey = false;
    this.decoder.decode(
      new EncodedVideoChunk({
        type: key ? "key" : "delta",
        timestamp: frame.timestamp_micros,
        data: frame.data, // Uint8Array (AVCC); EncodedVideoChunk copies it
      }),
    );
  }

  close() {
    if (this.decoder && this.decoder.state !== "closed") {
      try { this.decoder.close(); } catch { /* already closing */ }
    }
    this.decoder = null;
  }
}
