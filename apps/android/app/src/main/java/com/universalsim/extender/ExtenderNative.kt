package com.universalsim.extender

/**
 * JNI declarations — one external fun per exported symbol in the Rust crate
 * `extender-android-jni` (`Java_com_universalsim_extender_ExtenderNative_*`).
 * Keep the package + class name in sync with that crate.
 *
 * Prefer using [ExtenderSession] over calling these directly.
 */
object ExtenderNative {
    init {
        System.loadLibrary("extender_mobile")
    }

    /** captureMode: 0 = virtual second screen, 1 = mirror the host's primary display, 2 = control-only (clicker). */
    external fun nativeConnect(addr: String, width: Int, height: Int, captureMode: Int): Long
    external fun nativeFree(handle: Long)

    /** Advance the stream; returns 0 = Start, 1 = Frame, -1 = ended. */
    external fun nativeNextEvent(handle: Long): Int
    external fun nativeEventWidth(handle: Long): Int
    external fun nativeEventHeight(handle: Long): Int
    external fun nativeEventCodec(handle: Long): Int
    external fun nativeEventKeyframe(handle: Long): Boolean
    external fun nativeEventPts(handle: Long): Long
    external fun nativeEventData(handle: Long): ByteArray?

    external fun nativeSendKey(handle: Long, hidCode: Int, pressed: Boolean)
    external fun nativeSendMouseMove(handle: Long, x: Float, y: Float)
    external fun nativeSendMouseButton(handle: Long, button: Int, pressed: Boolean)
    external fun nativeSendScroll(handle: Long, dx: Float, dy: Float)
    external fun nativeSendTouch(handle: Long, id: Int, phase: Int, x: Float, y: Float)
    external fun nativeSendSecondaryClick(handle: Long, x: Float, y: Float)
    external fun nativeSendText(handle: Long, text: String)

    /** Ask the host to pre-scan the open document for next-slide look-ahead. */
    external fun nativeScanDeck(handle: Long)

    /** Ask the host to (re)send its list of open windows. */
    external fun nativeListWindows(handle: Long)

    /** Bring the host window with [id] to the foreground; [startShow] also sends F5. */
    external fun nativeFocusWindow(handle: Long, id: Long, startShow: Boolean)
}
