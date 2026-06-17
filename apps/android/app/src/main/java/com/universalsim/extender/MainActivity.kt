package com.universalsim.extender

import android.graphics.BitmapFactory
import android.os.Bundle
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.compose.foundation.Image
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.changedToDown
import androidx.compose.ui.input.pointer.changedToUp
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
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
    val context = LocalContext.current
    var session by remember { mutableStateOf<ExtenderSession?>(null) }
    var mode by remember { mutableStateOf(Mode.CLICKER) }
    var currentAddr by remember { mutableStateOf("") }
    var status by remember { mutableStateOf("") }

    val live = session
    if (live == null) {
        ConnectScreen(status) { addr, chosenMode, pin ->
            mode = chosenMode
            currentAddr = addr
            status = "connecting…"
            // Clicker needs no video (control-only); the others mirror the screen.
            val capture = if (chosenMode == Mode.CLICKER) {
                ExtenderSession.MODE_CONTROL_ONLY
            } else {
                ExtenderSession.MODE_MIRROR
            }
            thread {
                // Width/height advertise the phone panel; the host mirrors at its
                // own native size, so exact values here are not critical.
                val s = ExtenderSession.connect(addr, 1920, 1080, capture, pin)
                runOnUi {
                    // Remember a successful host for quick reconnect; its OS/name
                    // fill in once HostInfo arrives (see the screen sinks below).
                    if (s != null) ConnectionStore.remember(context, addr, chosenMode.name, pin)
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
                Mode.CLICKER -> ClickerScreen(live, currentAddr)
                Mode.VIEWER -> StreamScreen(live, currentAddr, forwardInput = false)
                Mode.FULL_CONTROL -> StreamScreen(live, currentAddr, forwardInput = true)
            }
        }
    }
}

@Composable
fun ConnectScreen(status: String, onConnect: (addr: String, mode: Mode, pin: Int) -> Unit) {
    val context = LocalContext.current
    var addr by remember { mutableStateOf("127.0.0.1:9000") }
    var pin by remember { mutableStateOf("") }
    var saved by remember { mutableStateOf(ConnectionStore.load(context)) }
    var showHidden by remember { mutableStateOf(false) }
    fun reload() { saved = ConnectionStore.load(context) }

    // Scan the host's QR (from the host window). It encodes "ip:port?pin=NNNN";
    // a scan fills the fields and auto-connects as a clicker (scan = pair & go).
    val scanLauncher = rememberLauncherForActivityResult(ScanContract()) { result ->
        val text = result.contents ?: return@rememberLauncherForActivityResult
        val q = text.indexOf("?pin=")
        if (q >= 0) {
            addr = text.substring(0, q)
            val p = text.substring(q + 5).filter { it.isDigit() }
            pin = p
            onConnect(addr, Mode.CLICKER, p.toIntOrNull() ?: 0)
        } else {
            addr = text
        }
    }

    val visible = saved.filter { showHidden || !it.hidden }.sortedByDescending { it.lastConnected }
    val hasHidden = saved.any { it.hidden }

    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("Screen Extender", style = MaterialTheme.typography.headlineMedium)

        if (visible.isNotEmpty()) {
            Text("Saved hosts", style = MaterialTheme.typography.titleMedium)
            visible.forEach { c ->
                SavedConnectionRow(
                    conn = c,
                    onConnect = {
                        onConnect(c.addr, runCatching { Mode.valueOf(c.mode) }.getOrDefault(Mode.CLICKER), c.pin)
                    },
                    onToggleHide = { ConnectionStore.setHidden(context, c.addr, !c.hidden); reload() },
                    onDelete = { ConnectionStore.delete(context, c.addr); reload() },
                )
            }
        }
        if (hasHidden) {
            TextButton(onClick = { showHidden = !showHidden }) {
                Text(if (showHidden) "Hide hidden" else "Show hidden")
            }
        }

        Text("New connection", style = MaterialTheme.typography.titleMedium)
        OutlinedTextField(
            value = addr,
            onValueChange = { addr = it },
            label = { Text("Host  (ip:port)") },
            trailingIcon = {
                TextButton(onClick = {
                    val options = ScanOptions()
                        .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                        .setPrompt("Scan the host QR")
                        .setBeepEnabled(false)
                        .setOrientationLocked(false)
                    scanLauncher.launch(options)
                }) { Text("Scan QR") }
            },
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = pin,
            onValueChange = { pin = it.filter { c -> c.isDigit() }.take(4) },
            label = { Text("PIN (from the host)") },
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            modifier = Modifier.fillMaxWidth(),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            val code = pin.toIntOrNull() ?: 0
            Button(onClick = { onConnect(addr, Mode.CLICKER, code) }) { Text("Clicker") }
            Button(onClick = { onConnect(addr, Mode.VIEWER, code) }) { Text("Viewer") }
            Button(onClick = { onConnect(addr, Mode.FULL_CONTROL, code) }) { Text("Control") }
        }
        if (status.isNotEmpty()) Text(status)
    }
}

/** One saved host: tap to quick-connect (in its remembered mode); hide or delete. */
@Composable
private fun SavedConnectionRow(
    conn: SavedConnection,
    onConnect: () -> Unit,
    onToggleHide: () -> Unit,
    onDelete: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Button(onClick = onConnect, modifier = Modifier.weight(1f)) {
            Text(deviceEmoji(conn.os), fontSize = 22.sp)
            Spacer(Modifier.width(10.dp))
            Column(horizontalAlignment = Alignment.Start) {
                Text(conn.hostname.ifEmpty { conn.addr })
                Text(
                    "${conn.addr}  ·  ${modeLabel(conn.mode)}",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
        TextButton(onClick = onToggleHide) { Text(if (conn.hidden) "Unhide" else "Hide") }
        TextButton(onClick = onDelete) { Text("Delete") }
    }
}

/**
 * Presentation remote: each button taps a key on the host. A control-only host
 * (the Windows clicker host) also pushes a still JPEG preview of the current
 * slide after each tap, shown at the top.
 */
@Composable
fun ClickerScreen(session: ExtenderSession, addr: String) {
    val context = LocalContext.current
    var preview by remember { mutableStateOf<ImageBitmap?>(null) }
    var nextPreview by remember { mutableStateOf<ImageBitmap?>(null) }
    var prevPreview by remember { mutableStateOf<ImageBitmap?>(null) }
    var scanned by remember { mutableStateOf(false) }
    var windowList by remember { mutableStateOf<List<Pair<Long, String>>>(emptyList()) }
    var windowMenuOpen by remember { mutableStateOf(false) }
    var showMore by remember { mutableStateOf(false) }
    var startShowOnFocus by remember { mutableStateOf(true) }
    DisposableEffect(session) {
        session.startPump(object : ExtenderSession.FrameSink {
            override fun onStart(width: Int, height: Int, codec: Int, csd: ByteArray) {}
            override fun onFrame(data: ByteArray, keyframe: Boolean, ptsValue: Long) {}
            override fun onSnapshot(width: Int, height: Int, slot: Int, jpeg: ByteArray) {
                // Empty jpeg for an adjacent slot means "no slide there" -> clear it.
                val bmp = if (jpeg.isEmpty()) null else BitmapFactory.decodeByteArray(jpeg, 0, jpeg.size)
                runOnUi {
                    when {
                        slot < 0 -> prevPreview = bmp?.asImageBitmap()
                        slot > 0 -> nextPreview = bmp?.asImageBitmap()
                        bmp != null -> preview = bmp.asImageBitmap()
                    }
                }
            }
            override fun onHostInfo(os: String, name: String) {
                ConnectionStore.setIdentity(context, addr, os, name)
            }
            override fun onWindowList(windows: List<Pair<Long, String>>) {
                runOnUi { windowList = windows }
            }
            override fun onEnded() {}
        })
        onDispose { }
    }
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        val slide = preview
        if (slide != null) {
            Image(
                bitmap = slide,
                contentDescription = "Current slide",
                contentScale = ContentScale.Fit,
                modifier = Modifier.fillMaxWidth().aspectRatio(16f / 9f),
            )
        } else {
            Text("Waiting for slide preview…", style = MaterialTheme.typography.bodyMedium)
        }
        // Build (or rebuild) the look-ahead cache, and pick which host window gets
        // the keystrokes (in case the document lost focus).
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Button(onClick = { session.scanDeck(); scanned = true }) {
                Text(if (scanned) "Rescan deck" else "Scan deck")
            }
            Box {
                Button(onClick = { session.listWindows(); windowMenuOpen = true }) {
                    Text("Focus window ▾")
                }
                DropdownMenu(expanded = windowMenuOpen, onDismissRequest = { windowMenuOpen = false }) {
                    if (windowList.isEmpty()) {
                        DropdownMenuItem(text = { Text("No windows") }, onClick = { windowMenuOpen = false })
                    } else {
                        windowList.forEach { (id, title) ->
                            DropdownMenuItem(
                                text = { Text(title, maxLines = 1) },
                                onClick = { session.focusWindow(id, startShowOnFocus); windowMenuOpen = false },
                            )
                        }
                    }
                }
            }
        }
        // When set, focusing a window also starts its slideshow (F5). Turn off for
        // PDFs in a browser, where F5 reloads the page.
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Switch(checked = startShowOnFocus, onCheckedChange = { startShowOnFocus = it })
            Text("Start show on focus (F5)", style = MaterialTheme.typography.bodySmall)
        }
        if (!scanned) {
            Text(
                "Tap Scan deck to preview the previous/next slides (keep the document focused).",
                style = MaterialTheme.typography.bodySmall,
            )
        }
        // Prev / Next, each with its slide preview above it. The previous slide is
        // dimmed so the focus stays on what's coming next.
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceEvenly,
            verticalAlignment = Alignment.Bottom,
        ) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                PreviewTile(prevPreview, dim = true, label = "Previous slide")
                BigButton("◀  Prev") { session.tapKey(HidKeys.PAGE_UP) }
            }
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                PreviewTile(nextPreview, dim = false, label = "Next slide")
                BigButton("Next  ▶") { session.tapKey(HidKeys.PAGE_DOWN) }
            }
        }
        // Keep the remote uncluttered: the secondary actions hide behind a toggle.
        TextButton(onClick = { showMore = !showMore }) {
            Text(if (showMore) "Fewer options" else "More options")
        }
        if (showMore) {
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                Button(onClick = { session.tapKey(HidKeys.HOME) }) { Text("First") }
                Button(onClick = { session.tapKey(HidKeys.END) }) { Text("Last") }
            }
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                // No universal "blank" key: PowerPoint uses B (black), Keynote / Google
                // Slides use '.' — so expose both (see docs/M6-presentation-clicker.md).
                Button(onClick = { session.tapKey(HidKeys.B) }) { Text("Blank (PPT)") }
                Button(onClick = { session.tapKey(HidKeys.PERIOD) }) { Text("Blank (.)") }
            }
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                Button(onClick = { session.tapKey(HidKeys.F5) }) { Text("Start (F5)") }
                Button(onClick = { session.tapKey(HidKeys.ESCAPE) }) { Text("End (Esc)") }
            }
        }
    }
}

@Composable
private fun BigButton(label: String, onClick: () -> Unit) {
    Button(onClick = onClick, modifier = Modifier.size(width = 150.dp, height = 90.dp)) {
        Text(label, style = MaterialTheme.typography.titleLarge)
    }
}

/** A 150dp-wide slide thumbnail above a nav button. When there's no slide (the
 *  ends of the deck, or before a scan) it shows the ScreenExtender icon as a
 *  placeholder. [dim] fades the previous-slide preview so the next one stands out. */
@Composable
private fun PreviewTile(bitmap: ImageBitmap?, dim: Boolean, label: String) {
    val mod = Modifier.width(150.dp).aspectRatio(16f / 9f)
    if (bitmap != null) {
        Image(
            bitmap = bitmap,
            contentDescription = label,
            contentScale = ContentScale.Fit,
            alpha = if (dim) 0.4f else 1f,
            modifier = mod,
        )
    } else {
        Box(mod, contentAlignment = Alignment.Center) {
            Image(
                painter = painterResource(R.drawable.ic_screenextender),
                contentDescription = "Screen Extender",
                modifier = Modifier.size(40.dp),
                alpha = 0.3f,
            )
        }
    }
}

/**
 * Streams the host's screen via MediaCodec into a SurfaceView. When
 * [forwardInput] is true (Full control), touches are forwarded as absolute
 * pointer input normalized to the view; in Viewer they're ignored.
 */
@Composable
fun StreamScreen(session: ExtenderSession, addr: String, forwardInput: Boolean) {
    val context = LocalContext.current
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

                            override fun onHostInfo(os: String, name: String) {
                                ConnectionStore.setIdentity(context, addr, os, name)
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
