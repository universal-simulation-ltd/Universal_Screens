package com.universalsim.extender

import kotlin.concurrent.thread

/**
 * Kotlin-friendly wrapper around a native session handle. Owns the event-pump
 * thread (for the video modes) and exposes input helpers (for all modes).
 *
 * Construct with [connect] off the main thread — it does blocking network I/O.
 */
class ExtenderSession private constructor(private var handle: Long) {

    /** Sink for downstream stream events, driven on the pump thread. */
    interface FrameSink {
        fun onStart(width: Int, height: Int, codec: Int, csd: ByteArray)
        fun onFrame(data: ByteArray, keyframe: Boolean, ptsValue: Long)
        fun onEnded()
    }

    @Volatile private var pumping = false
    private var pump: Thread? = null

    companion object {
        const val MODE_VIRTUAL = 0
        const val MODE_MIRROR = 1
        const val MODE_CONTROL_ONLY = 2 // input only, no video (clicker)

        /** Blocking connect; returns null on failure. Call off the main thread. */
        fun connect(addr: String, width: Int, height: Int, captureMode: Int): ExtenderSession? {
            val handle = ExtenderNative.nativeConnect(addr, width, height, captureMode)
            return if (handle != 0L) ExtenderSession(handle) else null
        }
    }

    /** Pump downstream events to [sink] on a background thread (video modes only). */
    fun startPump(sink: FrameSink) {
        if (pumping || handle == 0L) return
        pumping = true
        pump = thread(name = "extender-pump") {
            while (pumping) {
                when (ExtenderNative.nativeNextEvent(handle)) {
                    0 -> sink.onStart(
                        ExtenderNative.nativeEventWidth(handle),
                        ExtenderNative.nativeEventHeight(handle),
                        ExtenderNative.nativeEventCodec(handle),
                        ExtenderNative.nativeEventData(handle) ?: ByteArray(0),
                    )
                    1 -> sink.onFrame(
                        ExtenderNative.nativeEventData(handle) ?: ByteArray(0),
                        ExtenderNative.nativeEventKeyframe(handle),
                        ExtenderNative.nativeEventPts(handle),
                    )
                    else -> {
                        pumping = false
                        sink.onEnded()
                    }
                }
            }
        }
    }

    // ---- input ----

    /** A key tap: down then up. The clicker's workhorse. */
    fun tapKey(hid: Int) {
        if (handle == 0L) return
        ExtenderNative.nativeSendKey(handle, hid, true)
        ExtenderNative.nativeSendKey(handle, hid, false)
    }

    fun sendKey(hid: Int, pressed: Boolean) = ifLive { ExtenderNative.nativeSendKey(handle, hid, pressed) }
    fun sendTouch(id: Int, phase: Int, x: Float, y: Float) = ifLive { ExtenderNative.nativeSendTouch(handle, id, phase, x, y) }
    fun sendSecondaryClick(x: Float, y: Float) = ifLive { ExtenderNative.nativeSendSecondaryClick(handle, x, y) }
    fun sendScroll(dx: Float, dy: Float) = ifLive { ExtenderNative.nativeSendScroll(handle, dx, dy) }
    fun sendText(text: String) = ifLive { ExtenderNative.nativeSendText(handle, text) }

    private inline fun ifLive(block: () -> Unit) {
        if (handle != 0L) block()
    }

    fun close() {
        pumping = false
        pump?.join(500)
        pump = null
        if (handle != 0L) {
            ExtenderNative.nativeFree(handle)
            handle = 0L
        }
    }
}
