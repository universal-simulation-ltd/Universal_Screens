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

    /** Where to keep the pairing keys, so a paired computer reconnects with no code. Call once at start with `filesDir.absolutePath`. */
    external fun nativeSetPairingDir(dir: String)

    /** The addresses of the computers this phone is paired with, one per line ("" = none). */
    external fun nativePairedHosts(): String

    /** "Forget this computer": drop the pairing with [addr] so its PIN is asked next time. True if one was dropped. */
    external fun nativeForgetHost(addr: String): Boolean

    /** captureMode: 0 = virtual second screen, 1 = mirror the host's primary display, 2 = control-only (clicker). pin: host pairing code (0 = none). deviceName: label for the screen this phone adds on the host ("" = host default). */
    external fun nativeConnect(addr: String, width: Int, height: Int, captureMode: Int, pin: Int, deviceName: String): Long
    external fun nativeFree(handle: Long)

    /** Why the last [nativeConnect] ON THIS THREAD returned 0 — a [ConnectProblem] code (0 = it didn't). */
    external fun nativeLastConnectProblem(): Int

    /** For [ConnectProblem.HOST_LOCKED]: seconds until the computer takes another PIN. */
    external fun nativeLastConnectWaitSeconds(): Int

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
    external fun nativeSendMouseMoveRelative(handle: Long, dx: Float, dy: Float)
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
