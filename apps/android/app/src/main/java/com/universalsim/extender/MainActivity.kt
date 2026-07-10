package com.universalsim.extender

import android.content.Context
import android.content.Intent
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Bundle
import android.graphics.SurfaceTexture
import android.view.HapticFeedbackConstants
import android.view.Surface
import android.view.TextureView
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.gestures.detectTapGestures
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
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.changedToDown
import androidx.compose.ui.input.pointer.changedToUp
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.abs
import androidx.compose.ui.viewinterop.AndroidView
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import kotlin.concurrent.thread

/** The three ways to use the app; they differ only in UI + whether they stream. */
enum class Mode { FULL_CONTROL, VIEWER, CLICKER, TRACKPAD, SECOND_SCREEN }

/** A connection target decoded from a scanned QR or a deep link (App Link). */
data class ConnectPayload(
    val addr: String,    // "ip:port"
    val pin: Int,
    val ssid: String?,
    val pass: String?,
    val auth: String,
)

/**
 * Parse any of the host's connect payloads into a [ConnectPayload], or null if the
 * text isn't a recognised connect code. Three shapes are accepted:
 *
 *  • `https://opensource.unisim.co.uk/screens/connect?host=&port=&pin=#ssid=&auth=&pass=`
 *    — the App-Link URL the host now encodes. Host + PIN are in the query; any
 *    Wi-Fi credentials ride in the fragment (kept client-side by browsers, so a
 *    plain camera that lands on the web page never leaks the password to the
 *    server). Both query and fragment are read here.
 *  • `unisimscreens://connect?host=&port=&pin=&ssid=&pass=&auth=` — the legacy
 *    custom scheme (older hosts), everything in the query.
 *  • `ip:port?pin=NNNN` — the older bare host QR.
 */
fun parseConnectPayload(text: String): ConnectPayload? {
    val t = text.trim()
    val isHttps = t.startsWith("https://", ignoreCase = true)
    if (isHttps || t.startsWith("unisimscreens://", ignoreCase = true)) {
        val uri = Uri.parse(t)
        // For https, only `…/screens/connect` is a connect code (not the marketing
        // `/screens` page, which also opens the app via the same App-Link filter).
        if (isHttps && uri.path?.startsWith("/screens/connect") != true) return null
        val p = uriParams(uri)
        val host = p["host"].orEmpty()
        if (host.isEmpty()) return null
        return ConnectPayload(
            addr = "$host:${p["port"] ?: "9000"}",
            pin = p["pin"]?.filter { it.isDigit() }?.toIntOrNull() ?: 0,
            ssid = p["ssid"]?.takeIf { it.isNotEmpty() },
            pass = p["pass"]?.takeIf { it.isNotEmpty() },
            auth = p["auth"] ?: "WPA",
        )
    }
    // Bare "ip:port?pin=NNNN" host QR.
    val q = t.indexOf("?pin=")
    if (q >= 0) {
        return ConnectPayload(
            addr = t.substring(0, q),
            pin = t.substring(q + 5).filter { it.isDigit() }.toIntOrNull() ?: 0,
            ssid = null, pass = null, auth = "WPA",
        )
    }
    return null
}

/**
 * Extract a "cast to a browser" pairing code from a connect URL, or null. The
 * receiver page's QR encodes `…/screens/connect?code=<CODE>&role=sender`; the
 * legacy `unisimscreens://connect?code=…` scheme is also accepted. A code is
 * 4–8 letters/digits and routes to the cast flow (no host/Wi-Fi involved).
 */
fun parseRoomCode(text: String): String? {
    val t = text.trim()
    val isHttps = t.startsWith("https://", ignoreCase = true)
    if (!isHttps && !t.startsWith("unisimscreens://", ignoreCase = true)) return null
    val uri = Uri.parse(t)
    if (isHttps && uri.path?.startsWith("/screens/connect") != true) return null
    val code = uriParams(uri)["code"]?.uppercase() ?: return null
    return if (code.matches(Regex("^[A-Z0-9]{4,8}$"))) code else null
}

/** Merge a URI's query params with any `k=v&k=v` in its fragment (query wins). The
 *  connect URL keeps Wi-Fi creds in the fragment, so we re-parse it as a query. */
private fun uriParams(uri: Uri): Map<String, String> {
    val out = HashMap<String, String>()
    uri.encodedFragment?.let { frag ->
        val f = Uri.parse("x://x?$frag")
        for (k in f.queryParameterNames) f.getQueryParameter(k)?.let { out[k] = it }
    }
    for (k in uri.queryParameterNames) uri.getQueryParameter(k)?.let { out[k] = it }
    return out
}

/**
 * Act on a decoded [payload]: if it carries Wi-Fi creds (and the OS supports
 * app-initiated joins) join that network first, then hand (addr, pin) to [onReady]
 * to pick a mode. [onStatus] surfaces the "Joining…" / failure line (null clears it).
 * Shared by the in-app scanner and the App-Link deep-link path.
 */
fun handleConnectPayload(
    context: Context,
    payload: ConnectPayload,
    onStatus: (String?) -> Unit,
    onReady: (String, Int) -> Unit,
) {
    val ssid = payload.ssid
    if (!ssid.isNullOrEmpty() && WifiConnect.isSupported()) {
        onStatus("Joining “$ssid”…")
        WifiConnect.join(context, ssid, payload.pass, payload.auth) { ok ->
            onStatus(if (ok) null else "Couldn't join Wi-Fi — join it manually, then connect.")
            if (ok) onReady(payload.addr, payload.pin) // joined → choose a mode
        }
    } else {
        // Already on the network (or Android < 10): go straight to mode pick.
        onReady(payload.addr, payload.pin)
    }
}

class MainActivity : ComponentActivity() {
    // The deep link the app was opened with — a scanned `…/screens/connect` App
    // Link (or the legacy `unisimscreens://` scheme). Consumed once by AppRoot,
    // which then resets it to null. Held as Compose state so onNewIntent (the app
    // was already open) recomposes and connects too.
    private val deepLink = mutableStateOf<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        deepLink.value = intent?.dataString
        setContent {
            val link by deepLink
            UniversalScreensTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    AppRoot(deepLink = link, onDeepLinkHandled = { deepLink.value = null })
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        deepLink.value = intent.dataString
    }
}

@Composable
fun AppRoot(deepLink: String? = null, onDeepLinkHandled: () -> Unit = {}) {
    val context = LocalContext.current
    var session by remember { mutableStateOf<ExtenderSession?>(null) }
    var mode by remember { mutableStateOf(Mode.CLICKER) }
    var currentAddr by remember { mutableStateOf("") }
    var currentPin by remember { mutableStateOf(0) }
    var status by remember { mutableStateOf("") }
    // Credentials gathered (addr, pin) and awaiting a mode choice; null = not picking.
    var pending by remember { mutableStateOf<Pair<String, Int>?>(null) }
    // True while a connection attempt is in flight (shows the Connecting screen).
    var connecting by remember { mutableStateOf(false) }
    // Non-null when "casting to a browser": the rendezvous code we're paired on.
    // Takes over the whole UI (CastFlow) and is independent of the host session.
    var castCode by remember { mutableStateOf<String?>(null) }

    // A deep link (scanned `…/screens/connect` App Link, or the legacy
    // `unisimscreens://` scheme) opens us straight here: parse it, optionally join
    // the host's Wi-Fi, then jump to the mode picker — the same path as an in-app
    // scan. Consumed once (onDeepLinkHandled resets it).
    LaunchedEffect(deepLink) {
        val link = deepLink ?: return@LaunchedEffect
        onDeepLinkHandled()
        // A "cast to a browser" code (…/screens/connect?code=…) routes to the
        // rendezvous flow instead of a host connection.
        parseRoomCode(link)?.let { castCode = it; return@LaunchedEffect }
        val payload = parseConnectPayload(link) ?: return@LaunchedEffect
        handleConnectPayload(
            context,
            payload,
            onStatus = { status = it ?: "" },
            onReady = { addr, pin -> pending = addr to pin },
        )
    }

    // chosenMode + whether to remember it for this host (so saved rows can skip
    // the picker next time). When `remember` is false we still save the host for
    // quick reconnect, but with no mode — tapping it re-asks.
    val doConnect: (String, Mode, Int, Boolean) -> Unit = { addr, chosenMode, pin, rememberMode ->
        mode = chosenMode
        currentAddr = addr
        currentPin = pin
        connecting = true
        status = ""
        // Clicker/Trackpad need no video; Second screen extends (virtual display);
        // the rest mirror the primary.
        val capture = when (chosenMode) {
            Mode.CLICKER, Mode.TRACKPAD -> ExtenderSession.MODE_CONTROL_ONLY
            Mode.SECOND_SCREEN -> ExtenderSession.MODE_VIRTUAL
            else -> ExtenderSession.MODE_MIRROR
        }
        // The screen this phone adds on the host is labelled with this name (parity
        // with the iOS client); read before the thread so it's off the gesture path.
        val deviceName = ConnectionStore.loadDeviceName(context)
        thread {
            // Width/height advertise the phone panel; the host mirrors at its own
            // native size, so exact values here are not critical.
            val s = ExtenderSession.connect(addr, 1920, 1080, capture, pin, deviceName)
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
        // "Cast to a browser" takes over the whole UI while active.
        castCode != null -> CastFlow(code = castCode!!, onExit = { castCode = null })
        live != null -> {
            // In the streaming modes, tapping the video hides the top bar so the
            // picture can fill the screen; tap again to bring it back.
            val streaming =
                mode == Mode.VIEWER || mode == Mode.FULL_CONTROL || mode == Mode.SECOND_SCREEN
            var chrome by remember(live) { mutableStateOf(true) }

            // Top bar (current mode + Disconnect). For streaming modes it floats as
            // a translucent gradient overlay over the video (matching the iPhone /
            // web viewer) so the picture keeps the full height; for the control
            // modes (Clicker / Trackpad) it sits above the content in normal flow.
            val topBar: @Composable (Boolean) -> Unit = { overlay ->
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .then(
                            if (overlay)
                                Modifier.background(
                                    Brush.verticalGradient(
                                        listOf(Color.Black.copy(alpha = 0.55f), Color.Transparent),
                                    ),
                                )
                            else Modifier,
                        )
                        .statusBarsPadding()
                        .padding(8.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    // Tap the mode chip to go back and pick a different one for this host.
                    ModeChip(mode, onClick = {
                        live.close()
                        session = null
                        pending = currentAddr to currentPin
                    })
                    DisconnectButton(onClick = {
                        live.close()
                        session = null
                    })
                }
            }

            if (streaming) {
                Box(modifier = Modifier.fillMaxSize()) {
                    when (mode) {
                        Mode.VIEWER, Mode.SECOND_SCREEN ->
                            StreamScreen(live, currentAddr, forwardInput = false) { chrome = !chrome }
                        Mode.FULL_CONTROL ->
                            StreamScreen(live, currentAddr, forwardInput = true) { chrome = !chrome }
                        else -> {}
                    }
                    if (chrome) {
                        Box(modifier = Modifier.align(Alignment.TopCenter).fillMaxWidth()) {
                            topBar(true)
                        }
                    }
                }
            } else {
                Column(modifier = Modifier.fillMaxSize()) {
                    topBar(false)
                    when (mode) {
                        Mode.CLICKER -> ClickerScreen(live, currentAddr)
                        Mode.TRACKPAD -> TrackpadScreen(live)
                        else -> {}
                    }
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
                onCast = { code -> castCode = code },
            )
        }
    }
}

/** The current-mode chip in the connected header: an orange-tint capsule with the
 *  mode's emoji + label and a ▾, tapped to go back and re-pick. Mirrors the iOS
 *  `ConnectedHeader` chip. */
@Composable
private fun ModeChip(mode: Mode, onClick: () -> Unit) {
    Surface(
        onClick = onClick,
        shape = RoundedCornerShape(50),
        color = MaterialTheme.colorScheme.primaryContainer,
        contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 14.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(mode.emoji(), fontSize = 15.sp)
            Text(mode.label(), style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.SemiBold)
            Text("▾", fontWeight = FontWeight.Bold)
        }
    }
}

/** A destructive (red) Disconnect button, shared by the connected header and cast. */
@Composable
private fun DisconnectButton(onClick: () -> Unit) {
    Button(
        onClick = onClick,
        colors = ButtonDefaults.buttonColors(
            containerColor = MaterialTheme.colorScheme.error,
            contentColor = MaterialTheme.colorScheme.onError,
        ),
    ) { Text("Disconnect") }
}

/** A full-screen "Connecting…" placeholder with the app logo + a spinner, shown
 *  while a session is being established so the user never bounces back home. */
@Composable
fun ConnectingScreen(addr: String) {
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Image(
            painter = painterResource(R.drawable.app_icon),
            contentDescription = null,
            modifier = Modifier.size(88.dp).clip(RoundedCornerShape(20.dp)),
        )
        Spacer(Modifier.height(20.dp))
        CircularProgressIndicator()
        Spacer(Modifier.height(16.dp))
        Text("Connecting…", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
        if (addr.isNotEmpty()) {
            Text(addr, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@Composable
fun ConnectScreen(
    status: String,
    onPrepare: (addr: String, pin: Int) -> Unit,
    onConnect: (addr: String, mode: Mode, pin: Int) -> Unit,
    onCast: (String) -> Unit = {},
) {
    val context = LocalContext.current
    var addr by remember { mutableStateOf("127.0.0.1:9000") }
    var pin by remember { mutableStateOf("") }
    var deviceName by remember { mutableStateOf(ConnectionStore.loadDeviceName(context)) }
    var saved by remember { mutableStateOf(ConnectionStore.load(context)) }
    var showHidden by remember { mutableStateOf(false) }
    var joinStatus by remember { mutableStateOf<String?>(null) }
    var showAdvanced by remember { mutableStateOf(false) }
    // "Cast to a browser": manual code entry (the QR/deep-link path skips this).
    var castDialog by remember { mutableStateOf(false) }
    var castDraft by remember { mutableStateOf("") }
    // Saved-host rename: the host being renamed (drives the dialog) + the draft.
    var renaming by remember { mutableStateOf<SavedConnection?>(null) }
    var renameDraft by remember { mutableStateOf("") }
    fun reload() { saved = ConnectionStore.load(context) }

    // Scan the host's Step-2 QR. It now encodes an https `…/screens/connect` URL
    // (a plain phone camera lands on a help page; the app deep-links straight in),
    // but the parser also still accepts the legacy `unisimscreens://connect?…`
    // scheme and the bare `ip:port?pin=NNNN` host QR — see [parseConnectPayload].
    val scanLauncher = rememberLauncherForActivityResult(ScanContract()) { result ->
        val text = result.contents ?: return@rememberLauncherForActivityResult
        // A receiver's "cast" code routes to the browser-cast flow.
        parseRoomCode(text)?.let { onCast(it); return@rememberLauncherForActivityResult }
        val payload = parseConnectPayload(text)
        if (payload != null) {
            addr = payload.addr
            pin = payload.pin.toString().padStart(4, '0')
            handleConnectPayload(
                context,
                payload,
                onStatus = { joinStatus = it },
                onReady = { a, p -> onPrepare(a, p) },
            )
        } else {
            // Not a recognised connect code — drop the raw text into the address box.
            addr = text
        }
    }

    val startScan: () -> Unit = {
        val options = ScanOptions()
            .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
            // The prompt/caption is drawn by PortraitCaptureActivity's overlay, so
            // leave zxing's own status text empty (its view is hidden anyway).
            .setPrompt("")
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
        // The app icon above the primary action — tapping it also opens the scanner.
        Image(
            painter = painterResource(R.drawable.app_icon),
            contentDescription = "Scan to connect",
            modifier = Modifier
                .size(116.dp)
                .clip(RoundedCornerShape(26.dp))
                .clickable(onClick = startScan),
        )
        Text(
            "Universal Screens",
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.Bold,
        )
        Button(onClick = startScan, modifier = Modifier.fillMaxWidth()) {
            Text("Scan to connect", style = MaterialTheme.typography.titleMedium)
        }
        Text(
            "Point at the host's Step 2 QR — it joins this PC's Wi-Fi and connects.",
            style = MaterialTheme.typography.bodySmall,
        )

        // Cast to a browser: this phone becomes the remote for a browser tab.
        OutlinedButton(onClick = { castDraft = ""; castDialog = true }, modifier = Modifier.fillMaxWidth()) {
            Text("Cast to a browser screen")
        }

        if (visible.isNotEmpty()) {
            Text(
                "SAVED HOSTS",
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.fillMaxWidth(),
            )
            visible.forEach { c ->
                SavedConnectionRow(
                    conn = c,
                    onConnect = {
                        // Remembered mode → connect straight away; otherwise re-ask.
                        val m = runCatching { Mode.valueOf(c.mode) }.getOrNull()
                        if (m != null) onConnect(c.addr, m, c.pin) else onPrepare(c.addr, c.pin)
                    },
                    onRename = { renameDraft = c.customName; renaming = c },
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
                value = deviceName,
                onValueChange = { deviceName = it; ConnectionStore.saveDeviceName(context, it) },
                label = { Text("This device's name") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Text(
                "Labels the screen this phone adds on the host.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
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

        // "Cast to a browser" code entry (the QR / deep-link path skips this).
        if (castDialog) {
            AlertDialog(
                onDismissRequest = { castDialog = false },
                title = { Text("Cast to a browser") },
                text = {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text("Open opensource.unisim.co.uk/screens/receive on the screen you want to drive, then enter the code it shows (or scan its QR from this screen).")
                        OutlinedTextField(
                            value = castDraft,
                            onValueChange = { castDraft = it.uppercase().filter { c -> c.isLetterOrDigit() }.take(8) },
                            label = { Text("Code") },
                            singleLine = true,
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                },
                confirmButton = {
                    TextButton(
                        enabled = castDraft.length in 4..8,
                        onClick = { castDialog = false; onCast(castDraft) },
                    ) { Text("Connect") }
                },
                dismissButton = {
                    TextButton(onClick = { castDialog = false }) { Text("Cancel") }
                },
            )
        }

        // Rename dialog for a saved host.
        renaming?.let { target ->
            AlertDialog(
                onDismissRequest = { renaming = null },
                title = { Text("Rename host") },
                text = {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text("Give this saved host a friendly name. Leave blank to reset to its device name.")
                        OutlinedTextField(
                            value = renameDraft,
                            onValueChange = { renameDraft = it },
                            singleLine = true,
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                },
                confirmButton = {
                    TextButton(onClick = {
                        ConnectionStore.setCustomName(context, target.addr, renameDraft)
                        renaming = null
                        reload()
                    }) { Text("Save") }
                },
                dismissButton = {
                    TextButton(onClick = { renaming = null }) { Text("Cancel") }
                },
            )
        }
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
        verticalArrangement = Arrangement.spacedBy(10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            "How do you want to use it?",
            style = MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.Bold,
            textAlign = TextAlign.Center,
        )
        Text(addr, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.height(4.dp))

        listOf(Mode.CLICKER, Mode.VIEWER, Mode.FULL_CONTROL, Mode.TRACKPAD, Mode.SECOND_SCREEN).forEach { m ->
            ModeOption(m) { onPick(m, rememberChoice) }
        }

        Spacer(Modifier.height(4.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Switch(checked = rememberChoice, onCheckedChange = { rememberChoice = it })
            Spacer(Modifier.width(8.dp))
            Text("Remember next time?")
        }

        TextButton(onClick = onBack) { Text("Back") }
    }
}

/** One selectable mode: a card row with an orange-tint emoji chip, title + subtitle
 *  and a chevron — the Android take on the iOS `ModeOption`. */
@Composable
private fun ModeOption(mode: Mode, onClick: () -> Unit) {
    Surface(
        onClick = onClick,
        shape = RoundedCornerShape(16.dp),
        color = MaterialTheme.colorScheme.surface,
        tonalElevation = 1.dp,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            modifier = Modifier.padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Box(
                modifier = Modifier
                    .size(44.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .background(MaterialTheme.colorScheme.primaryContainer),
                contentAlignment = Alignment.Center,
            ) { Text(mode.emoji(), fontSize = 22.sp) }
            Column(modifier = Modifier.weight(1f)) {
                Text(mode.label(), style = MaterialTheme.typography.titleMedium)
                Text(
                    mode.subtitle(),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Text("›", style = MaterialTheme.typography.titleLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

/** One saved host: a card — tap to quick-connect (in its remembered mode); an
 *  overflow menu holds Rename / Hide / Delete (mirrors the iOS ellipsis menu). */
@Composable
private fun SavedConnectionRow(
    conn: SavedConnection,
    onConnect: () -> Unit,
    onRename: () -> Unit,
    onToggleHide: () -> Unit,
    onDelete: () -> Unit,
) {
    var menuOpen by remember { mutableStateOf(false) }
    Surface(
        shape = RoundedCornerShape(16.dp),
        color = MaterialTheme.colorScheme.surface,
        tonalElevation = 1.dp,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 6.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Row(
                modifier = Modifier
                    .weight(1f)
                    .clip(RoundedCornerShape(12.dp))
                    .clickable(onClick = onConnect)
                    .padding(8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Box(
                    modifier = Modifier
                        .size(44.dp)
                        .clip(RoundedCornerShape(12.dp))
                        .background(MaterialTheme.colorScheme.primaryContainer),
                    contentAlignment = Alignment.Center,
                ) { Text(deviceEmoji(conn.os), fontSize = 22.sp) }
                Column {
                    // Friendly name with the host in brackets, e.g. "Office Mac
                    // (Kyjams-iMac)"; else just the hostname (or address).
                    val base = conn.hostname.ifEmpty { conn.addr }
                    val title = if (conn.customName.isBlank()) base else "${conn.customName} ($base)"
                    Text(title, style = MaterialTheme.typography.bodyLarge)
                    // Show the remembered mode only if there is one; otherwise just the
                    // address (tapping re-asks for the mode).
                    val sub = if (conn.mode.isBlank()) conn.addr else "${conn.addr}  ·  ${modeLabel(conn.mode)}"
                    Text(sub, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            Box {
                TextButton(onClick = { menuOpen = true }) { Text("⋯", fontSize = 22.sp) }
                DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                    DropdownMenuItem(text = { Text("Rename") }, onClick = { menuOpen = false; onRename() })
                    DropdownMenuItem(
                        text = { Text(if (conn.hidden) "Unhide" else "Hide") },
                        onClick = { menuOpen = false; onToggleHide() },
                    )
                    DropdownMenuItem(text = { Text("Delete") }, onClick = { menuOpen = false; onDelete() })
                }
            }
        }
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
    // When locked, every control on this screen is disabled so a stray touch can't
    // fire a key; only the central lock toggle stays live so it can be unlocked.
    var locked by remember { mutableStateOf(false) }
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
            Button(onClick = { session.scanDeck(); scanned = true }, enabled = !locked) {
                Text(if (scanned) "Rescan deck" else "Scan deck")
            }
            Box {
                Button(onClick = { session.listWindows(); windowMenuOpen = true }, enabled = !locked) {
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
            Switch(checked = startShowOnFocus, onCheckedChange = { startShowOnFocus = it }, enabled = !locked)
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
                BigButton("◀  Prev", enabled = !locked) { session.tapKey(HidKeys.PAGE_UP) }
            }
            // The lock sits in the middle of the two nav buttons — a direct tap
            // toggles it; a tap-and-swipe is ignored.
            LockToggle(locked = locked) { locked = !locked }
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                PreviewTile(nextPreview, dim = false, label = "Next slide")
                BigButton("Next  ▶", enabled = !locked) { session.tapKey(HidKeys.PAGE_DOWN) }
            }
        }
        // Keep the remote uncluttered: the secondary actions hide behind a toggle.
        TextButton(onClick = { showMore = !showMore }, enabled = !locked) {
            Text(if (showMore) "Fewer options" else "More options")
        }
        if (showMore) {
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                Button(onClick = { session.tapKey(HidKeys.HOME) }, enabled = !locked) { Text("First") }
                Button(onClick = { session.tapKey(HidKeys.END) }, enabled = !locked) { Text("Last") }
            }
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                // No universal "blank" key: PowerPoint uses B (black), Keynote / Google
                // Slides use '.' — so expose both (see docs/M6-presentation-clicker.md).
                Button(onClick = { session.tapKey(HidKeys.B) }, enabled = !locked) { Text("Blank (PPT)") }
                Button(onClick = { session.tapKey(HidKeys.PERIOD) }, enabled = !locked) { Text("Blank (.)") }
            }
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                Button(onClick = { session.tapKey(HidKeys.F5) }, enabled = !locked) { Text("Start (F5)") }
                Button(onClick = { session.tapKey(HidKeys.ESCAPE) }, enabled = !locked) { Text("End (Esc)") }
            }
        }
    }
}

@Composable
private fun BigButton(label: String, enabled: Boolean = true, onClick: () -> Unit) {
    Button(
        onClick = onClick,
        enabled = enabled,
        modifier = Modifier.size(width = 150.dp, height = 90.dp),
    ) {
        Text(label, style = MaterialTheme.typography.titleLarge)
    }
}

/**
 * A lock toggle that guards a screen against accidental presses. Locked → a closed
 * padlock on a yellow background; unlocked → just an open padlock on a transparent
 * background. It reacts only to a clean, direct tap: a tap that turns into a swipe
 * (or any drag passing over it) is ignored, and the touch is consumed so it never
 * leaks through to a gesture surface underneath (e.g. the trackpad).
 */
@Composable
private fun LockToggle(locked: Boolean, onToggle: () -> Unit) {
    val tapSlop = 16f
    Box(
        modifier = Modifier
            .size(64.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(if (locked) Color(0xFFFFD600) else Color.Transparent)
            .pointerInput(Unit) {
                awaitEachGesture {
                    val down = awaitFirstDown()
                    down.consume()
                    var moved = 0f
                    while (true) {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull { it.id == down.id }
                        if (change != null) {
                            moved += (change.position - change.previousPosition).getDistance()
                            change.consume()
                        }
                        if (event.changes.none { it.pressed }) break
                    }
                    if (moved < tapSlop) onToggle()
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(if (locked) "🔒" else "🔓", fontSize = 30.sp)
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
 * Streams the host's screen via MediaCodec into a TextureView (transformable, so
 * pinch-zoom + pan work). When [forwardInput] is true (Remote control), touches
 * are forwarded as absolute pointer input normalized to the video area. In Mirror
 * they drive pinch-zoom + drag-to-pan, and a tap toggles the top bar.
 */
@Composable
fun StreamScreen(
    session: ExtenderSession,
    addr: String,
    forwardInput: Boolean,
    onToggleChrome: () -> Unit,
) {
    val context = LocalContext.current
    // The host's screen aspect ratio (width/height), learned from StreamStart, so
    // we can letterbox instead of stretching.
    var videoAspect by remember { mutableStateOf<Float?>(null) }
    // Mirror zoom/pan.
    var scale by remember { mutableStateOf(1f) }
    var offset by remember { mutableStateOf(Offset.Zero) }

    // Size the video area to the host's aspect ratio (centred → letterboxed) once
    // known; touch input is normalised to that area so remote control maps right.
    var viewModifier =
        if (videoAspect != null) Modifier.aspectRatio(videoAspect!!) else Modifier.fillMaxSize()
    viewModifier = if (forwardInput) {
        viewModifier.pointerInput(session) {
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
    } else {
        // Mirror: pinch to zoom, drag to pan (when zoomed), tap toggles the bar.
        viewModifier
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
                translationX = offset.x
                translationY = offset.y
                clip = true
            }
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown()
                    var moved = false
                    while (true) {
                        val event = awaitPointerEvent()
                        val zoom = event.calculateZoom()
                        val pan = event.calculatePan()
                        if (zoom != 1f || pan != Offset.Zero) {
                            moved = true
                            scale = (scale * zoom).coerceIn(1f, 5f)
                            offset = if (scale > 1f) {
                                val maxX = size.width * (scale - 1f) / 2f
                                val maxY = size.height * (scale - 1f) / 2f
                                Offset(
                                    (offset.x + pan.x).coerceIn(-maxX, maxX),
                                    (offset.y + pan.y).coerceIn(-maxY, maxY),
                                )
                            } else {
                                Offset.Zero
                            }
                            event.changes.forEach { it.consume() }
                        }
                        if (event.changes.none { it.pressed }) break
                    }
                    if (!moved) onToggleChrome() // a clean tap toggles the bar
                }
            }
    }

    Box(modifier = Modifier.fillMaxSize().background(Color.Black), contentAlignment = Alignment.Center) {
        AndroidView(modifier = viewModifier, factory = { ctx ->
            TextureView(ctx).apply {
                surfaceTextureListener = object : TextureView.SurfaceTextureListener {
                    private var decoder: VideoDecoder? = null

                    override fun onSurfaceTextureAvailable(st: SurfaceTexture, w: Int, h: Int) {
                        val surface = Surface(st)
                        session.startPump(object : ExtenderSession.FrameSink {
                            override fun onStart(width: Int, height: Int, codec: Int, csd: ByteArray) {
                                if (height > 0) {
                                    runOnUi { videoAspect = width.toFloat() / height.toFloat() }
                                }
                                decoder = VideoDecoder(width, height, codec, csd, surface)
                            }

                            override fun onFrame(data: ByteArray, keyframe: Boolean, ptsValue: Long) {
                                // Host streams at ~20 fps; approximate a microsecond PTS.
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

                    override fun onSurfaceTextureSizeChanged(st: SurfaceTexture, w: Int, h: Int) {}
                    override fun onSurfaceTextureDestroyed(st: SurfaceTexture): Boolean {
                        decoder?.release()
                        decoder = null
                        return true
                    }

                    override fun onSurfaceTextureUpdated(st: SurfaceTexture) {}
                }
            }
        })

        if (forwardInput) {
            // Taps are forwarded as control, so the bar can't be toggled by tapping
            // the screen. This dim, always-present handle does it: press and hold to
            // show/hide the bar. Its own touches are consumed (no stray host click).
            Box(
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(12.dp)
                    .size(48.dp)
                    .pointerInput(Unit) { detectTapGestures(onLongPress = { onToggleChrome() }) },
                contentAlignment = Alignment.Center,
            ) {
                Image(
                    painter = painterResource(R.drawable.app_icon),
                    contentDescription = "Press and hold to show or hide the controls",
                    modifier = Modifier.size(44.dp),
                    alpha = 0.4f,
                )
            }
        }
    }
}

/**
 * Trackpad: the phone becomes a touchpad over a control-only (no-video) session.
 * One-finger drag moves the cursor (relative), a tap left-clicks, two-finger drag
 * scrolls, and a two-finger tap right-clicks. Explicit buttons sit below.
 */
@Composable
fun TrackpadScreen(session: InputTarget) {
    val context = LocalContext.current
    val view = LocalView.current
    // Pointer-speed multiplier, persisted app-wide so it survives reconnects. Read
    // inside the gesture loop so changes take effect without restarting pointerInput.
    var sensitivity by remember { mutableStateOf(ConnectionStore.loadSensitivity(context)) }
    // When locked, the pad ignores all touches and the buttons/slider are disabled,
    // so a stray hand can't move the cursor; only the central lock toggle stays live.
    var locked by remember { mutableStateOf(false) }
    // Click-and-drag is offered two ways: a "tap-and-a-half" gesture (tap, then
    // tap-hold-move) and the Drag-lock button below, which holds the left button
    // down so a plain one-finger move drags. dragLock is read inside the gesture
    // loop (snapshot state) so toggling it takes effect without restarting input.
    var dragLock by remember { mutableStateOf(false) }
    var lastTapUp by remember { mutableStateOf(0L) }
    val scrollDivisor = 40f
    val tapSlop = 16f
    val doubleTapWindowMs = 300L
    // A click with a little haptic tick, like a real trackpad. button: 0=L, 1=R.
    fun click(button: Int) {
        view.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY)
        session.sendMouseButton(button, true)
        session.sendMouseButton(button, false)
    }
    // Hold (or release) the left button so one-finger moves drag.
    fun setDragLock(on: Boolean) {
        if (on == dragLock) return
        view.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY)
        session.sendMouseButton(0, on)
        dragLock = on
    }
    // Safety net: never leave a button stuck down if we navigate away mid-drag.
    DisposableEffect(Unit) {
        onDispose { if (dragLock) session.sendMouseButton(0, false) }
    }
    Column(modifier = Modifier.fillMaxSize().padding(8.dp)) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .background(MaterialTheme.colorScheme.surfaceVariant)
                .pointerInput(session, locked) {
                    if (locked) return@pointerInput
                    awaitEachGesture {
                        awaitFirstDown()
                        // A gesture that closely follows a tap is a "tap-and-a-half":
                        // a one-finger move turns into a drag (press now, release on lift).
                        val tapAndHalf = System.currentTimeMillis() - lastTapUp < doubleTapWindowMs
                        var moved = 0f
                        var maxPointers = 1
                        var pressedForDrag = false
                        while (true) {
                            val event = awaitPointerEvent()
                            maxPointers = maxOf(maxPointers, event.changes.count { it.pressed })
                            val pan = event.calculatePan()
                            if (pan != Offset.Zero) {
                                moved += abs(pan.x) + abs(pan.y)
                                if (maxPointers >= 2) {
                                    // Two fingers → scroll (natural direction).
                                    session.sendScroll(pan.x / scrollDivisor * sensitivity, -pan.y / scrollDivisor * sensitivity)
                                } else {
                                    // Begin a tap-and-a-half drag once the move clears the tap slop.
                                    if (tapAndHalf && !dragLock && !pressedForDrag && moved >= tapSlop) {
                                        view.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY)
                                        session.sendMouseButton(0, true)
                                        pressedForDrag = true
                                    }
                                    session.sendMouseMoveRelative(pan.x * sensitivity, pan.y * sensitivity)
                                }
                                event.changes.forEach { it.consume() }
                            }
                            if (event.changes.none { it.pressed }) break
                        }
                        when {
                            // End a tap-and-a-half drag on lift.
                            pressedForDrag -> session.sendMouseButton(0, false)
                            // Drag-lock keeps the button held across lifts — don't click, don't drop.
                            dragLock -> {}
                            // A near-stationary lift is a click (two fingers = right).
                            moved < tapSlop -> {
                                click(if (maxPointers >= 2) 1 else 0)
                                lastTapUp = System.currentTimeMillis()
                            }
                        }
                    }
                },
            contentAlignment = Alignment.Center,
        ) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                // The lock sits in the middle of the pad — a direct tap toggles it;
                // its touches are consumed so they never move the cursor.
                LockToggle(locked = locked) { locked = !locked; if (locked) setDragLock(false) }
                Spacer(Modifier.height(16.dp))
                Text(
                    if (locked) "Locked — tap the lock to unlock"
                    else if (dragLock) "Dragging — move to drag\nTap “Drop” to release"
                    else "Trackpad\n\nDrag to move • tap to click • double-tap-drag to drag\nTwo fingers: scroll • two-finger tap: right-click",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                )
            }
        }
        Spacer(Modifier.height(8.dp))
        Text(
            "Pointer speed: ${"%.1f".format(sensitivity)}×",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface,
        )
        Slider(
            value = sensitivity,
            onValueChange = { sensitivity = it },
            onValueChangeFinished = { ConnectionStore.saveSensitivity(context, sensitivity) },
            valueRange = 0.5f..4f,
            enabled = !locked,
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(8.dp))
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = { click(0) }, enabled = !locked, modifier = Modifier.weight(1f)) { Text("Left click") }
            // Drag lock: hold the left button so a one-finger move drags; tap again to drop.
            if (dragLock) {
                Button(
                    onClick = { setDragLock(false) },
                    enabled = !locked,
                    colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.tertiary),
                    modifier = Modifier.weight(1f),
                ) { Text("Drop") }
            } else {
                OutlinedButton(onClick = { setDragLock(true) }, enabled = !locked, modifier = Modifier.weight(1f)) { Text("Drag") }
            }
            Button(onClick = { click(1) }, enabled = !locked, modifier = Modifier.weight(1f)) { Text("Right click") }
        }
    }
}

/**
 * "Cast to a browser": this phone joins a receiver tab's rendezvous room by
 * [code] (as the sender) and drives it. Owns the [RoomSession] lifecycle; reuses
 * [TrackpadScreen] and adds a lightweight clicker. [onExit] returns home.
 *
 * Requires the rendezvous Worker to be deployed (opensource-portal) — until then
 * it will sit on "Connecting…".
 */
@Composable
fun CastFlow(code: String, onExit: () -> Unit) {
    var session by remember { mutableStateOf<RoomSession?>(null) }
    var paired by remember { mutableStateOf(false) }
    var status by remember { mutableStateOf("Connecting…") }
    var castMode by remember { mutableStateOf<Mode?>(null) } // TRACKPAD | CLICKER; null = picking

    // Open the room once; tear it down when we leave.
    DisposableEffect(code) {
        val s = RoomSession.connect(code, object : RoomSession.Listener {
            override fun onStatus(text: String) { status = text }
            override fun onPaired(peerRole: String?) { paired = true; status = "Connected" }
            override fun onPeerLeft() { paired = false; status = "Receiver left — waiting…" }
            override fun onClosed(reason: String) { status = reason }
        })
        session = s
        onDispose { s.close() }
    }

    val s = session
    Column(modifier = Modifier.fillMaxSize()) {
        Row(
            modifier = Modifier.fillMaxWidth().statusBarsPadding().padding(8.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                "Casting · $code",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.primary,
            )
            DisconnectButton(onClick = onExit)
        }

        if (s == null || !paired) {
            Column(
                modifier = Modifier.fillMaxSize().padding(24.dp),
                verticalArrangement = Arrangement.Center,
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                CircularProgressIndicator()
                Spacer(Modifier.height(16.dp))
                Text(status, style = MaterialTheme.typography.titleMedium)
                Text("Code $code — open …/screens/receive on the screen", style = MaterialTheme.typography.bodySmall)
            }
        } else if (castMode == null) {
            CastModePicker(onPick = { m ->
                castMode = m
                s.hello(if (m == Mode.CLICKER) "clicker" else "trackpad")
            })
        } else if (castMode == Mode.CLICKER) {
            CastClickerScreen(s)
        } else {
            TrackpadScreen(s)
        }
    }
}

/** Cast control choices: only the no-video modes make sense to a browser tab. */
@Composable
fun CastModePicker(onPick: (Mode) -> Unit) {
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("How do you want to drive it?", style = MaterialTheme.typography.titleLarge)
        Button(onClick = { onPick(Mode.TRACKPAD) }, modifier = Modifier.fillMaxWidth()) { Text("Trackpad") }
        Button(onClick = { onPick(Mode.CLICKER) }, modifier = Modifier.fillMaxWidth()) { Text("Clicker") }
    }
}

/** A minimal presentation clicker for cast mode — the buttons drive the browser
 *  receiver's slide deck (no host, so no deck preview / window focus). */
@Composable
fun CastClickerScreen(target: InputTarget) {
    Column(
        modifier = Modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
    ) {
        Text(
            "Clicker",
            style = MaterialTheme.typography.titleMedium,
            textAlign = TextAlign.Center,
            modifier = Modifier.fillMaxWidth(),
        )
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Button(onClick = { target.tapKey(HidKeys.PAGE_UP) }, modifier = Modifier.weight(1f).height(72.dp)) { Text("◀  Prev") }
            Button(onClick = { target.tapKey(HidKeys.PAGE_DOWN) }, modifier = Modifier.weight(1f).height(72.dp)) { Text("Next  ▶") }
        }
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Button(onClick = { target.tapKey(HidKeys.HOME) }, modifier = Modifier.weight(1f)) { Text("First") }
            Button(onClick = { target.tapKey(HidKeys.END) }, modifier = Modifier.weight(1f)) { Text("Last") }
        }
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Button(onClick = { target.tapKey(HidKeys.B) }, modifier = Modifier.weight(1f)) { Text("Blank") }
            Button(onClick = { target.tapKey(HidKeys.F5) }, modifier = Modifier.weight(1f)) { Text("Start (F5)") }
        }
    }
}

/** Run [block] on the UI thread (helper for posting connect results back). */
private fun runOnUi(block: () -> Unit) {
    android.os.Handler(android.os.Looper.getMainLooper()).post(block)
}
