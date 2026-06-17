package com.universalsim.extender

import android.graphics.BitmapFactory
import android.os.Bundle
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
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
    // Credentials gathered (addr, pin) and awaiting a mode choice; null = not picking.
    var pending by remember { mutableStateOf<Pair<String, Int>?>(null) }
    // True while a connection attempt is in flight (shows the Connecting screen).
    var connecting by remember { mutableStateOf(false) }

    // chosenMode + whether to remember it for this host (so saved rows can skip
    // the picker next time). When `remember` is false we still save the host for
    // quick reconnect, but with no mode — tapping it re-asks.
    val doConnect: (String, Mode, Int, Boolean) -> Unit = { addr, chosenMode, pin, rememberMode ->
        mode = chosenMode
        currentAddr = addr
        connecting = true
        status = ""
        // Clicker needs no video (control-only); the others mirror the screen.
        val capture = if (chosenMode == Mode.CLICKER) {
            ExtenderSession.MODE_CONTROL_ONLY
        } else {
            ExtenderSession.MODE_MIRROR
        }
        thread {
            // Width/height advertise the phone panel; the host mirrors at its own
            // native size, so exact values here are not critical.
            val s = ExtenderSession.connect(addr, 1920, 1080, capture, pin)
            runOnUi {
                connecting = false
                if (s != null) {
                    // Remember the host for quick reconnect; store the mode only if
                    // the user asked to. OS/name fill in once HostInfo arrives.
                    ConnectionStore.remember(context, addr, if (rememberMode) chosenMode.name else "", pin)
                }
                session = s
                status = if (s == null) "connection failed" else ""
            }
        }
    }

    val live = session
    when {
        live != null -> {
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
        // Connecting: a dedicated spinner screen (don't flash the home page).
        connecting -> ConnectingScreen(currentAddr)
        // After address + PIN are gathered (scan or manual), pick what to do.
        pending != null -> {
            val (pAddr, pPin) = pending!!
            ModePickerScreen(
                addr = pAddr,
                onPick = { chosen, rememberMode ->
                    pending = null
                    doConnect(pAddr, chosen, pPin, rememberMode)
                },
                onBack = { pending = null },
            )
        }
        else -> {
            ConnectScreen(
                status = status,
                onPrepare = { addr, pin -> pending = addr to pin },
                onConnect = { addr, m, pin -> doConnect(addr, m, pin, true) },
            )
        }
    }
}

/** A full-screen "Connecting…" placeholder with a spinner, shown while a session
 *  is being established so the user never bounces back to the home page. */
@Composable
fun ConnectingScreen(addr: String) {
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        CircularProgressIndicator()
        Spacer(Modifier.height(16.dp))
        Text("Connecting…", style = MaterialTheme.typography.titleMedium)
        if (addr.isNotEmpty()) {
            Text(addr, style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
fun ConnectScreen(
    status: String,
    onPrepare: (addr: String, pin: Int) -> Unit,
    onConnect: (addr: String, mode: Mode, pin: Int) -> Unit,
) {
    val context = LocalContext.current
    var addr by remember { mutableStateOf("127.0.0.1:9000") }
    var pin by remember { mutableStateOf("") }
    var saved by remember { mutableStateOf(ConnectionStore.load(context)) }
    var showHidden by remember { mutableStateOf(false) }
    var joinStatus by remember { mutableStateOf<String?>(null) }
    var showAdvanced by remember { mutableStateOf(false) }
    fun reload() { saved = ConnectionStore.load(context) }

    // Scan the host's QR. Two formats:
    //  • "ip:port?pin=NNNN"          — host QR: fill fields and connect.
    //  • "unisimscreens://connect?…" — combined QR: join the host's Wi-Fi first
    //    (one-tap system dialog) and then connect, all from one scan.
    val scanLauncher = rememberLauncherForActivityResult(ScanContract()) { result ->
        val text = result.contents ?: return@rememberLauncherForActivityResult
        if (text.startsWith("unisimscreens://")) {
            val uri = android.net.Uri.parse(text)
            val host = uri.getQueryParameter("host").orEmpty()
            val port = uri.getQueryParameter("port") ?: "9000"
            val code = uri.getQueryParameter("pin")?.filter { it.isDigit() }?.toIntOrNull() ?: 0
            val ssid = uri.getQueryParameter("ssid")
            val pass = uri.getQueryParameter("pass")
            val auth = uri.getQueryParameter("auth") ?: "WPA"
            val target = "$host:$port"
            addr = target
            pin = code.toString().padStart(4, '0')
            if (!ssid.isNullOrEmpty() && WifiConnect.isSupported()) {
                joinStatus = "Joining “$ssid”…"
                WifiConnect.join(context, ssid, pass, auth) { ok ->
                    joinStatus = if (ok) null else "Couldn't join Wi-Fi — join it manually, then connect."
                    if (ok) onPrepare(target, code) // joined → choose a mode
                }
            } else {
                // Already on the network (or Android < 10): go straight to mode pick.
                onPrepare(target, code)
            }
        } else {
            val q = text.indexOf("?pin=")
            if (q >= 0) {
                addr = text.substring(0, q)
                val p = text.substring(q + 5).filter { it.isDigit() }
                pin = p
                onPrepare(addr, p.toIntOrNull() ?: 0)
            } else {
                addr = text
            }
        }
    }

    val startScan: () -> Unit = {
        val options = ScanOptions()
            .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
            .setPrompt("Point at the host's Step 2 QR")
            .setBeepEnabled(false)
            .setOrientationLocked(false)
            .setCaptureActivity(PortraitCaptureActivity::class.java)
        scanLauncher.launch(options)
    }

    val visible = saved.filter { showHidden || !it.hidden }.sortedByDescending { it.lastConnected }
    val hasHidden = saved.any { it.hidden }

    Column(
        // Centre the content vertically so the camera + Scan button land mid-screen
        // (easy to reach with a thumb); it still scrolls if there are many hosts.
        modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("Universal Screens", style = MaterialTheme.typography.headlineMedium)

        // The app icon above the primary action — tapping it also opens the scanner.
        Image(
            painter = painterResource(R.drawable.app_icon),
            contentDescription = "Scan to connect",
            modifier = Modifier.size(140.dp).clickable(onClick = startScan),
        )
        Button(onClick = startScan, modifier = Modifier.fillMaxWidth()) {
            Text("Scan to connect", style = MaterialTheme.typography.titleMedium)
        }
        Text(
            "Point at the host's Step 2 QR — it joins this PC's Wi-Fi and connects.",
            style = MaterialTheme.typography.bodySmall,
        )

        if (visible.isNotEmpty()) {
            Text("Saved hosts", style = MaterialTheme.typography.titleMedium)
            visible.forEach { c ->
                SavedConnectionRow(
                    conn = c,
                    onConnect = {
                        // Remembered mode → connect straight away; otherwise re-ask.
                        val m = runCatching { Mode.valueOf(c.mode) }.getOrNull()
                        if (m != null) onConnect(c.addr, m, c.pin) else onPrepare(c.addr, c.pin)
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

        // Manual entry is tucked away — most people just scan.
        TextButton(onClick = { showAdvanced = !showAdvanced }) {
            Text(if (showAdvanced) "Advanced ▾" else "Advanced ▸")
        }
        if (showAdvanced) {
            OutlinedTextField(
                value = addr,
                onValueChange = { addr = it },
                label = { Text("Host  (ip:port)") },
                trailingIcon = {
                    TextButton(onClick = startScan) { Text("Scan QR") }
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
            Button(
                onClick = { onPrepare(addr, pin.toIntOrNull() ?: 0) },
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Connect") }
        }
        joinStatus?.let { Text(it) }
        if (status.isNotEmpty()) Text(status)
    }
}

/**
 * After the address + PIN are known (scan or manual), choose how to use the host.
 * Implemented modes connect on tap; the rest are shown greyed as "coming soon".
 */
@Composable
fun ModePickerScreen(addr: String, onPick: (Mode, Boolean) -> Unit, onBack: () -> Unit) {
    var rememberChoice by remember { mutableStateOf(false) }
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("Universal Screens", style = MaterialTheme.typography.headlineMedium)
        Text("Host: $addr", style = MaterialTheme.typography.bodyMedium)
        Text("How do you want to use it?", style = MaterialTheme.typography.titleMedium)

        ModeOption("Clicker", "Presentation remote — next/previous, blank, slide previews") {
            onPick(Mode.CLICKER, rememberChoice)
        }
        ModeOption("Mirror", "Watch the host's screen (view only)") {
            onPick(Mode.VIEWER, rememberChoice)
        }
        ModeOption("Remote control", "See the screen and control it (mouse + keys)") {
            onPick(Mode.FULL_CONTROL, rememberChoice)
        }
        ModeOption("Trackpad", "Coming soon", enabled = false) {}
        ModeOption("Second screen", "Use this phone as an extra display — coming soon", enabled = false) {}

        Row(verticalAlignment = Alignment.CenterVertically) {
            Switch(checked = rememberChoice, onCheckedChange = { rememberChoice = it })
            Spacer(Modifier.width(8.dp))
            Text("Remember next time?")
        }

        TextButton(onClick = onBack) { Text("Back") }
    }
}

/** One selectable mode: a full-width button with a title + description. */
@Composable
private fun ModeOption(
    title: String,
    subtitle: String,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    Button(onClick = onClick, enabled = enabled, modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.fillMaxWidth(), horizontalAlignment = Alignment.Start) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            Text(subtitle, style = MaterialTheme.typography.bodySmall)
        }
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
                // Show the remembered mode only if there is one; otherwise just the
                // address (tapping re-asks for the mode).
                val sub = if (conn.mode.isBlank()) conn.addr else "${conn.addr}  ·  ${modeLabel(conn.mode)}"
                Text(sub, style = MaterialTheme.typography.bodySmall)
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
        // Always reserve the 16:9 slot so the layout doesn't jump when the first
        // preview arrives; show a spinner placeholder until then.
        Box(
            modifier = Modifier.fillMaxWidth().aspectRatio(16f / 9f),
            contentAlignment = Alignment.Center,
        ) {
            val slide = preview
            if (slide != null) {
                Image(
                    bitmap = slide,
                    contentDescription = "Current slide",
                    contentScale = ContentScale.Fit,
                    modifier = Modifier.fillMaxSize(),
                )
            } else {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    CircularProgressIndicator()
                    Spacer(Modifier.height(8.dp))
                    Text("Waiting for slide preview…", style = MaterialTheme.typography.bodySmall)
                }
            }
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
 *  ends of the deck, or before a scan) it shows the Universal Screens icon as a
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
                painter = painterResource(R.drawable.app_icon),
                contentDescription = "Universal Screens",
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
