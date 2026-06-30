package com.universalsim.extender

import kotlin.concurrent.thread

/**
 * Kotlin-friendly wrapper around a native session handle. Owns the event-pump
 * thread (for the video modes) and exposes input helpers (for all modes).
 *
 * Construct with [connect] off the main thread — it does blocking network I/O.
 */
class ExtenderSession private constructor(private var handle: Long) : InputTarget {

    /** Sink for downstream stream events, driven on the pump thread. */
    interface FrameSink {
        fun onStart(width: Int, height: Int, codec: Int, csd: ByteArray)
        fun onFrame(data: ByteArray, keyframe: Boolean, ptsValue: Long)
        /**
         * A still JPEG slide preview (clicker mode); default no-op for video sinks.
         * [slot] is the slide's offset from the current position: 0 = current,
         * -1 = previous, +1 = next. [jpeg] is empty when there's no slide there.
         */
        fun onSnapshot(width: Int, height: Int, slot: Int, jpeg: ByteArray) {}
        /** The host's identity (OS tag + machine name); default no-op. */
        fun onHostInfo(os: String, name: String) {}
        /** The host's open windows as (id, title) pairs (for the focus picker). */
        fun onWindowList(windows: List<Pair<Long, String>>) {}
        fun onEnded()
    }

    @Volatile private var pumping = false
    private var pump: Thread? = null

    companion object {
        const val MODE_VIRTUAL = 0
        const val MODE_MIRROR = 1
        const val MODE_CONTROL_ONLY = 2 // input only, no video (clicker)

        /** Blocking connect; returns null on failure. Call off the main thread. */
        fun connect(addr: String, width: Int, height: Int, captureMode: Int, pin: Int = 0): ExtenderSession? {
            val handle = ExtenderNative.nativeConnect(addr, width, height, captureMode, pin)
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
                    2 -> sink.onSnapshot(
                        ExtenderNative.nativeEventWidth(handle),
                        ExtenderNative.nativeEventHeight(handle),
                        ExtenderNative.nativeEventCodec(handle), // codec field carries the slot
                        ExtenderNative.nativeEventData(handle) ?: ByteArray(0),
                    )
                    3 -> {
                        // HostInfo payload is UTF-8 "os\nname".
                        val parts = String(ExtenderNative.nativeEventData(handle) ?: ByteArray(0))
                            .split("\n", limit = 2)
                        sink.onHostInfo(parts.getOrElse(0) { "" }, parts.getOrElse(1) { "" })
                    }
                    4 -> {
                        // WindowList payload is one "id\ttitle" line per window.
                        val text = String(ExtenderNative.nativeEventData(handle) ?: ByteArray(0))
                        val windows = if (text.isEmpty()) emptyList() else text.split("\n").mapNotNull { line ->
                            val tab = line.indexOf('\t')
                            val id = if (tab > 0) line.substring(0, tab).toLongOrNull() else null
                            if (id == null) null else id to line.substring(tab + 1)
                        }
                        sink.onWindowList(windows)
                    }
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
    override fun tapKey(hid: Int) {
        if (handle == 0L) return
        ExtenderNative.nativeSendKey(handle, hid, true)
        ExtenderNative.nativeSendKey(handle, hid, false)
    }

    fun sendKey(hid: Int, pressed: Boolean) = ifLive { ExtenderNative.nativeSendKey(handle, hid, pressed) }
    override fun sendMouseMoveRelative(dx: Float, dy: Float) = ifLive { ExtenderNative.nativeSendMouseMoveRelative(handle, dx, dy) }
    /** [button]: 0 = left, 1 = right, 2 = middle. */
    override fun sendMouseButton(button: Int, pressed: Boolean) = ifLive { ExtenderNative.nativeSendMouseButton(handle, button, pressed) }
    fun sendTouch(id: Int, phase: Int, x: Float, y: Float) = ifLive { ExtenderNative.nativeSendTouch(handle, id, phase, x, y) }
    fun sendSecondaryClick(x: Float, y: Float) = ifLive { ExtenderNative.nativeSendSecondaryClick(handle, x, y) }
    override fun sendScroll(dx: Float, dy: Float) = ifLive { ExtenderNative.nativeSendScroll(handle, dx, dy) }
    fun sendText(text: String) = ifLive { ExtenderNative.nativeSendText(handle, text) }
    /** Ask the host to pre-scan the deck so it can preview the next slide. */
    fun scanDeck() = ifLive { ExtenderNative.nativeScanDeck(handle) }
    /** Ask the host to (re)send its open-window list. */
    fun listWindows() = ifLive { ExtenderNative.nativeListWindows(handle) }
    /** Bring the host window with [id] to the foreground; [startShow] also starts its slideshow. */
    fun focusWindow(id: Long, startShow: Boolean) = ifLive { ExtenderNative.nativeFocusWindow(handle, id, startShow) }

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
