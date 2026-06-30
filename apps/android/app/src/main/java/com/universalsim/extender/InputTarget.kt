package com.universalsim.extender

/**
 * The input surface a control UI (Trackpad / clicker) drives. Decouples those
 * screens from *how* the input travels:
 *
 *  • [ExtenderSession] sends it to a native host over the binary protocol (TCP).
 *  • [RoomSession] serialises it to JSON and relays it to a browser receiver
 *    through the cloud rendezvous ("cast to a browser", M8c).
 *
 * It is deliberately the smallest set of methods the reusable [TrackpadScreen]
 * and the clicker buttons need — both implementors already had these exact
 * signatures, so reusing the trackpad gesture code across both is free.
 */
interface InputTarget {
    /** Relative cursor move (trackpad). */
    fun sendMouseMoveRelative(dx: Float, dy: Float)

    /** Mouse button down/up. [button]: 0 = left, 1 = right, 2 = middle. */
    fun sendMouseButton(button: Int, pressed: Boolean)

    /** Scroll by a delta. */
    fun sendScroll(dx: Float, dy: Float)

    /** A key tap (down then up) by USB-HID usage id — the clicker's workhorse. */
    fun tapKey(hid: Int)
}
