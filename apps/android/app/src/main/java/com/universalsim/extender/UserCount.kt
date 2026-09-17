package com.universalsim.extender

import android.content.Context
import androidx.compose.foundation.clickable
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import org.json.JSONArray
import java.net.HttpURLConnection
import java.net.URL
import java.util.UUID

/**
 * "There are X total users (Y live)" — the line every app in the suite shows
 * (James, 2026-09-17), for the native Android client.
 *
 * Screens has no `@unisim/sdk` and no Supabase client, so this is the SDK's
 * `presence.ts` written out against the REST endpoint, with `HttpURLConnection`
 * rather than a new dependency. Read that file before changing anything here:
 *
 *   * `app_presence_beat(product, install_id)` on open and every 45 s while
 *     this screen is on (migration 0175);
 *   * `app_user_counts(product)` → total + live, or `suite_user_counts()`
 *     (0177) once the line has been tapped for the whole-suite figure.
 *
 * The install id is a random UUID in SharedPreferences. Nothing else is sent:
 * not the host, the PIN, the room code, the device name, or anything about
 * what is on the screen. Screens is used on networks with no route out as a
 * matter of course, so every call here is best-effort and silent: when they
 * fail the line simply never appears.
 *
 * The anon key is a PUBLISHABLE key — it ships in every suite web bundle by
 * design, and Row-Level Security is the boundary: `app_presence` has RLS on
 * with no policies, so this key reaches it through those two functions and in
 * no other way.
 */
object UserCount {
    private const val SUPABASE_URL = "https://rygfxgalojojppxmhddo.supabase.co"
    private const val SUPABASE_ANON =
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6InJ5Z2Z4Z2Fsb2pvanBweG1oZGRvIiwicm9sZSI6ImFub24iLCJpYXQiOjE3Nzg3NTY4MjUsImV4cCI6MjA5NDMzMjgyNX0.hLy_vt9vY_rdPKF3nL32yAuMCD604E3CH5VM7D7CaNE"

    private const val PRODUCT = "screens"
    private const val PREFS = "unisim_suite"

    /** Same spelling as the SDK's localStorage keys, so the two read alike. */
    private const val INSTALL_ID_KEY = "unisim:install-id"
    private const val SCOPE_KEY = "unisim:user-count-scope"

    const val BEAT_MS = 45_000L

    data class Counts(val total: Long, val live: Long)

    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    fun installId(context: Context): String {
        val p = prefs(context)
        p.getString(INSTALL_ID_KEY, null)?.let { return it }
        val id = UUID.randomUUID().toString()
        p.edit().putString(INSTALL_ID_KEY, id).apply()
        return id
    }

    fun scope(context: Context): String =
        if (prefs(context).getString(SCOPE_KEY, "app") == "suite") "suite" else "app"

    fun setScope(context: Context, scope: String) {
        prefs(context).edit().putString(SCOPE_KEY, scope).apply()
    }

    /** POST one RPC. Returns the body, or null on any failure — never throws. */
    private fun rpc(fn: String, body: String): String? = runCatching {
        val conn = (URL("$SUPABASE_URL/rest/v1/rpc/$fn").openConnection() as HttpURLConnection).apply {
            requestMethod = "POST"
            connectTimeout = 10_000
            readTimeout = 10_000
            doOutput = true
            setRequestProperty("apikey", SUPABASE_ANON)
            setRequestProperty("Authorization", "Bearer $SUPABASE_ANON")
            setRequestProperty("Content-Type", "application/json")
        }
        conn.outputStream.use { it.write(body.toByteArray()) }
        val ok = conn.responseCode in 200..299
        val text = if (ok) conn.inputStream.bufferedReader().use { it.readText() } else null
        conn.disconnect()
        text
    }.getOrNull()

    suspend fun beat(context: Context): Boolean = withContext(Dispatchers.IO) {
        rpc(
            "app_presence_beat",
            """{"p_product":"$PRODUCT","p_install_id":"${installId(context)}"}""",
        ) != null
    }

    suspend fun counts(scope: String): Counts? = withContext(Dispatchers.IO) {
        val body = if (scope == "suite") {
            rpc("suite_user_counts", "{}")
        } else {
            rpc("app_user_counts", """{"p_product":"$PRODUCT"}""")
        } ?: return@withContext null
        runCatching {
            val row = JSONArray(body).optJSONObject(0) ?: return@runCatching null
            Counts(row.getLong("total"), row.getLong("live"))
        }.getOrNull()
    }

    fun format(counts: Counts, scope: String): String {
        val total = String.format("%,d", counts.total)
        val live = String.format("%,d", counts.live)
        val one = counts.total == 1L
        return if (scope == "suite") {
            "There ${if (one) "is" else "are"} a total of $total user${if (one) "" else "s"} across all UNI·SIM apps ($live live)"
        } else {
            "There ${if (one) "is" else "are"} $total total user${if (one) "" else "s"} ($live live)"
        }
    }
}

/**
 * The line itself — last thing under Advanced, where the browser client and the
 * desktop hosts put it too. Tap it for the whole suite and back. Renders
 * nothing until a real number arrives, which on a network with no route out is
 * never.
 */
@Composable
fun UserCountLine(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    var scope by remember { mutableStateOf(UserCount.scope(context)) }
    var text by remember { mutableStateOf<String?>(null) }

    LaunchedEffect(scope) {
        while (true) {
            val beaten = UserCount.beat(context)
            val counts = UserCount.counts(scope)
            if (counts != null && counts.total > 0) {
                // Whoever is reading this is using the app: a count that raced
                // the beat must not answer "(0 live)".
                val live = if (beaten) maxOf(counts.live, 1L) else counts.live
                text = UserCount.format(counts.copy(live = live), scope)
            }
            delay(UserCount.BEAT_MS)
        }
    }

    text?.let {
        Text(
            it,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
            modifier = modifier.clickable {
                scope = if (scope == "app") "suite" else "app"
                UserCount.setScope(context, scope)
            },
        )
    }
}
