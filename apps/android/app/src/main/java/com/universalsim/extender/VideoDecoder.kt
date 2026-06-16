package com.universalsim.extender

import android.media.MediaCodec
import android.media.MediaFormat
import android.view.Surface
import java.nio.ByteBuffer

/**
 * Annex-B H.264/HEVC → [Surface] via `MediaCodec`. Construct on a Start event
 * (passing the Annex-B parameter sets as `csd`), then feed each frame.
 *
 * Scaffold: the dequeue loop here is intentionally simple (synchronous, render
 * immediately). For production, prefer the async `MediaCodec.Callback` and pace
 * presentation with the frame PTS. Needs on-device testing.
 */
class VideoDecoder(
    width: Int,
    height: Int,
    codecTag: Int,
    csd: ByteArray,
    surface: Surface,
) {
    private val codec: MediaCodec
    private val info = MediaCodec.BufferInfo()

    init {
        val mime = if (codecTag == 1) MediaFormat.MIMETYPE_VIDEO_HEVC else MediaFormat.MIMETYPE_VIDEO_AVC
        val format = MediaFormat.createVideoFormat(mime, width, height)
        // csd-0 carries the Annex-B parameter sets (SPS/PPS for H.264).
        format.setByteBuffer("csd-0", ByteBuffer.wrap(csd))
        codec = MediaCodec.createDecoderByType(mime)
        codec.configure(format, surface, null, 0)
        codec.start()
    }

    /** Queue one Annex-B access unit and drain any decoded frames to the surface. */
    fun decode(annexB: ByteArray, ptsUs: Long) {
        val inIndex = codec.dequeueInputBuffer(10_000)
        if (inIndex >= 0) {
            val buffer = codec.getInputBuffer(inIndex)!!
            buffer.clear()
            buffer.put(annexB)
            codec.queueInputBuffer(inIndex, 0, annexB.size, ptsUs, 0)
        }
        var outIndex = codec.dequeueOutputBuffer(info, 0)
        while (outIndex >= 0) {
            codec.releaseOutputBuffer(outIndex, true) // render to the surface
            outIndex = codec.dequeueOutputBuffer(info, 0)
        }
    }

    fun release() {
        try {
            codec.stop()
        } catch (_: Exception) {
        }
        codec.release()
    }
}
