package com.universalsim.extender

import android.os.Bundle
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.changedToDown
import androidx.compose.ui.input.pointer.changedToUp
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import kotlin.concurrent.thread

/** The three ways to use the app; they differ only in UI + whether they stream. */
enum class Mode { FULL_CONTROL, VIEWER, CLICKER }

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) { AppRoot() }
            }
        }
    }
}

@Composable
fun AppRoot() {
    var session by remember { mutableStateOf<ExtenderSession?>(null) }
    var mode by remember { mutableStateOf(Mode.CLICKER) }
    var status by remember { mutableStateOf("") }

    val live = session
    if (live == null) {
        ConnectScreen(status) { addr, chosenMode ->
            mode = chosenMode
            status = "connecting…"
            // Clicker doesn't need video; the others mirror the host's screen.
            val capture = ExtenderSession.MODE_MIRROR
            thread {
                // Width/height advertise the phone panel; the host mirrors at its
                // own native size, so exact values here are not critical.
                val s = ExtenderSession.connect(addr, 1920, 1080, capture)
                runOnUi {
                    session = s
                    status = if (s == null) "connection failed" else ""
                }
            }
        }
    } else {
        Column(modifier = Modifier.fillMaxSize()) {
            Row(modifier = Modifier.fillMaxWidth().padding(8.dp), horizontalArrangement = Arrangement.SpaceBetween) {
                Text("Mode: $mode", style = MaterialTheme.typography.titleMedium)
                Button(onClick = {
                    live.close()
                    session = null
                }) { Text("Disconnect") }
            }
            when (mode) {
                Mode.CLICKER -> ClickerScreen(live)
                Mode.VIEWER -> StreamScreen(live, forwardInput = false)
                Mode.FULL_CONTROL -> StreamScreen(live, forwardInput = true)
            }
        }
    }
}

@Composable
fun ConnectScreen(status: String, onConnect: (addr: String, mode: Mode) -> Unit) {
    var addr by remember { mutableStateOf("192.168.1.42:9000") }
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("ExtenderScreen", style = MaterialTheme.typography.headlineMedium)
        OutlinedTextField(
            value = addr,
            onValueChange = { addr = it },
            label = { Text("Mac host  (ip:9000)") },
            modifier = Modifier.fillMaxWidth(),
        )
        Text("Connect as:", style = MaterialTheme.typography.titleMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = { onConnect(addr, Mode.CLICKER) }) { Text("Clicker") }
            Button(onClick = { onConnect(addr, Mode.VIEWER) }) { Text("Viewer") }
            Button(onClick = { onConnect(addr, Mode.FULL_CONTROL) }) { Text("Control") }
        }
        if (status.isNotEmpty()) Text(status)
    }
}

/** Presentation remote: each button taps a key on the host. */
@Composable
fun ClickerScreen(session: ExtenderSession) {
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(20.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
            BigButton("◀  Prev") { session.tapKey(HidKeys.PAGE_UP) }
            BigButton("Next  ▶") { session.tapKey(HidKeys.PAGE_DOWN) }
        }
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
            Button(onClick = { session.tapKey(HidKeys.HOME) }) { Text("First") }
            Button(onClick = { session.tapKey(HidKeys.END) }) { Text("Last") }
            Button(onClick = { session.tapKey(HidKeys.PERIOD) }) { Text("Blank") }
        }
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
            Button(onClick = { session.tapKey(HidKeys.F5) }) { Text("Start (F5)") }
            Button(onClick = { session.tapKey(HidKeys.ESCAPE) }) { Text("End (Esc)") }
        }
    }
}

@Composable
private fun BigButton(label: String, onClick: () -> Unit) {
    Button(onClick = onClick, modifier = Modifier.size(width = 150.dp, height = 90.dp)) {
        Text(label, style = MaterialTheme.typography.titleLarge)
    }
}

/**
 * Streams the host's screen via MediaCodec into a SurfaceView. When
 * [forwardInput] is true (Full control), touches are forwarded as absolute
 * pointer input normalized to the view; in Viewer they're ignored.
 */
@Composable
fun StreamScreen(session: ExtenderSession, forwardInput: Boolean) {
    var inputModifier = Modifier.fillMaxSize()
    if (forwardInput) {
        inputModifier = inputModifier.pointerInput(session) {
            val w = size.width.toFloat()
            val h = size.height.toFloat()
            awaitPointerEventScope {
                while (true) {
                    val change = awaitPointerEvent().changes.firstOrNull() ?: continue
                    val nx = (change.position.x / w).coerceIn(0f, 1f)
                    val ny = (change.position.y / h).coerceIn(0f, 1f)
                    val phase = when {
                        change.changedToDown() -> TouchPhase.BEGAN
                        change.changedToUp() -> TouchPhase.ENDED
                        else -> TouchPhase.MOVED
                    }
                    session.sendTouch(0, phase, nx, ny)
                    change.consume()
                }
            }
        }
    }

    Box(modifier = inputModifier) {
        AndroidView(factory = { context ->
            SurfaceView(context).apply {
                holder.addCallback(object : SurfaceHolder.Callback {
                    private var decoder: VideoDecoder? = null

                    override fun surfaceCreated(holder: SurfaceHolder) {
                        session.startPump(object : ExtenderSession.FrameSink {
                            override fun onStart(width: Int, height: Int, codec: Int, csd: ByteArray) {
                                decoder = VideoDecoder(width, height, codec, csd, holder.surface)
                            }

                            override fun onFrame(data: ByteArray, keyframe: Boolean, ptsValue: Long) {
                                // Host streams at 60 fps; approximate a microsecond PTS.
                                decoder?.decode(data, ptsValue * 16_666)
                            }

                            override fun onEnded() {
                                decoder?.release()
                                decoder = null
                            }
                        })
                    }

                    override fun surfaceChanged(h: SurfaceHolder, f: Int, w: Int, ht: Int) {}
                    override fun surfaceDestroyed(holder: SurfaceHolder) {
                        decoder?.release()
                        decoder = null
                    }
                })
            }
        })
    }
}

/** Run [block] on the UI thread (helper for posting connect results back). */
private fun runOnUi(block: () -> Unit) {
    android.os.Handler(android.os.Looper.getMainLooper()).post(block)
}
