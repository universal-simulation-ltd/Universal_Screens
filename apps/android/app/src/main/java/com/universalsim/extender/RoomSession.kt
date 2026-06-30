package com.universalsim.extender

import android.os.Handler
import android.os.Looper
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/**
 * "Cast to a browser": joins a receiver tab's rendezvous room over a WebSocket
 * (as `role=sender`) and relays control input to it as JSON control frames — the
 * shared protocol in `opensource-portal/public/screens/control.js`. The browser
 * tab is the receiver/screen; this phone is the source/remote.
 *
 * Implements [InputTarget] so the existing [TrackpadScreen] and the cast clicker
 * drive it exactly like a host [ExtenderSession]. Lifecycle mirrors that class:
 * open without blocking the main thread; [Listener] callbacks are posted to the
 * UI thread.
 *
 * Requires the rendezvous Worker ([WS_BASE]) to be deployed (opensource-portal).
 */
class RoomSession private constructor(private val ws: WebSocket) : InputTarget {

    interface Listener {
        fun onStatus(text: String) {}
        fun onPaired(peerRole: String?) {}
        fun onPeerLeft() {}
        fun onClosed(reason: String) {}
    }

    companion object {
        /** The rendezvous lives on the Universal Screens site Worker. */
        const val WS_BASE = "wss://opensource.unisim.co.uk"

        private val client = OkHttpClient.Builder()
            .pingInterval(20, TimeUnit.SECONDS)
            .build()
        private val ui = Handler(Looper.getMainLooper())

        /** Join [code] as the sender. Returns immediately; [listener] fires on the UI thread. */
        fun connect(code: String, listener: Listener): RoomSession {
            val req = Request.Builder()
                .url("$WS_BASE/screens/room?code=${code.uppercase()}&role=sender")
                .build()
            val socket = client.newWebSocket(req, object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    ui.post { listener.onStatus("Connected — waiting for the receiver…") }
                }
                override fun onMessage(webSocket: WebSocket, text: String) {
                    val obj = runCatching { JSONObject(text) }.getOrNull() ?: return
                    when (obj.optString("type")) {
                        "paired" -> {
                            val peer = obj.optString("peerRole", "").ifEmpty { null }
                            ui.post { listener.onPaired(peer) }
                        }
                        "waiting" -> ui.post { listener.onStatus("Connected — waiting for the receiver…") }
                        "peer-left" -> ui.post { listener.onPeerLeft() }
                    }
                }
                override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                    webSocket.close(1000, null)
                }
                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    ui.post { listener.onClosed(reason.ifEmpty { "Disconnected" }) }
                }
                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    ui.post { listener.onClosed(t.message ?: "Connection error") }
                }
            })
            return RoomSession(socket)
        }
    }

    /** Tell the receiver what kind of controller this is ("trackpad" | "clicker"). */
    fun hello(mode: String) = sendFrame(JSONObject().put("t", "hello").put("mode", mode))

    // ---- InputTarget: serialise to the control protocol ----

    override fun sendMouseMoveRelative(dx: Float, dy: Float) =
        sendFrame(JSONObject().put("t", "move").put("dx", dx.toDouble()).put("dy", dy.toDouble()))

    override fun sendMouseButton(button: Int, pressed: Boolean) =
        sendFrame(JSONObject().put("t", "btn").put("b", button).put("down", pressed))

    override fun sendScroll(dx: Float, dy: Float) =
        sendFrame(JSONObject().put("t", "scroll").put("dx", dx.toDouble()).put("dy", dy.toDouble()))

    override fun tapKey(hid: Int) {
        val k = keyName(hid) ?: return
        sendFrame(JSONObject().put("t", "key").put("k", k))
    }

    fun close() {
        runCatching { ws.close(1000, "bye") }
    }

    private fun sendFrame(obj: JSONObject) {
        runCatching { ws.send(obj.toString()) }
    }

    /** Map the clicker's HID usage ids onto the receiver's deck actions. */
    private fun keyName(hid: Int): String? = when (hid) {
        HidKeys.PAGE_DOWN, HidKeys.ARROW_RIGHT -> "next"
        HidKeys.PAGE_UP, HidKeys.ARROW_LEFT -> "prev"
        HidKeys.HOME -> "first"
        HidKeys.END -> "last"
        HidKeys.B -> "blankB"
        HidKeys.PERIOD -> "blankDot"
        HidKeys.F5 -> "startF5"
        HidKeys.ESCAPE -> "endEsc"
        else -> null
    }
}
