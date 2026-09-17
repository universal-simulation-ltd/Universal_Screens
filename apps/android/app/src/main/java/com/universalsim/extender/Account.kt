package com.universalsim.extender

import android.content.Context
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

/**
 * A Universal ID on the phone — sign in, stay signed in, and let the saved
 * machines follow the account (James, 2026-09-17: "go ahead with screens
 * account", choosing "Identity + saved hosts follow you").
 *
 * The browser client's `apps/web/src/account.js` is the same thing in the same
 * order; keep the two in step. GoTrue's REST API written out by hand, because
 * Screens has no `@unisim/sdk` on any client:
 *
 *   POST /auth/v1/otp     { email, create_user: false }   → code emailed
 *   POST /auth/v1/verify  { email, token, type: email }   → session
 *   POST /auth/v1/token?grant_type=refresh_token          → fresh session
 *
 * **LOGIN ONLY.** A Universal ID is created on the hub (app.unisim.co.uk),
 * never here, so a typo cannot mint a stray account.
 *
 * ⚠️ **The PIN is never uploaded.** `SavedConnection` on this client DOES hold
 * a pin — unlike the browser's — and `screens_saved_hosts` (migration 0178) has
 * no column for it, deliberately: it is the Noise pre-shared key. Every mapping
 * below names its fields one by one for exactly that reason. Do not replace one
 * with a whole-object serialiser. `mode` and `hidden` are this client's own and
 * stay local too.
 *
 * ⚠️ Signing in is optional and changes nothing about connecting. Screens is
 * used on networks with no route out as a matter of course, so every call here
 * is best-effort and silent.
 */
object Account {
    private const val SUPABASE_URL = "https://rygfxgalojojppxmhddo.supabase.co"
    private const val SUPABASE_ANON =
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6InJ5Z2Z4Z2Fsb2pvanBweG1oZGRvIiwicm9sZSI6ImFub24iLCJpYXQiOjE3Nzg3NTY4MjUsImV4cCI6MjA5NDMzMjgyNX0.hLy_vt9vY_rdPKF3nL32yAuMCD604E3CH5VM7D7CaNE"

    private const val PREFS = "unisim_suite"
    private const val KEY_ACCESS = "auth:access"
    private const val KEY_REFRESH = "auth:refresh"
    private const val KEY_EXPIRES = "auth:expires"
    private const val KEY_EMAIL = "auth:email"
    private const val KEY_USER = "auth:user"

    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    /** The signed-in address, or null. Reads storage only. */
    fun email(context: Context): String? =
        prefs(context).getString(KEY_EMAIL, null)?.takeIf { it.isNotBlank() }

    fun userId(context: Context): String? =
        prefs(context).getString(KEY_USER, null)?.takeIf { it.isNotBlank() }

    private fun store(context: Context, session: JSONObject?) {
        val e = prefs(context).edit()
        if (session == null) {
            e.remove(KEY_ACCESS).remove(KEY_REFRESH).remove(KEY_EXPIRES).remove(KEY_EMAIL).remove(KEY_USER)
        } else {
            val user = session.optJSONObject("user")
            e.putString(KEY_ACCESS, session.optString("access_token"))
                .putString(KEY_REFRESH, session.optString("refresh_token"))
                // A minute of slack, so nothing is sent with a token about to expire.
                .putLong(KEY_EXPIRES, System.currentTimeMillis() + (session.optLong("expires_in", 3600) - 60) * 1000)
            if (user != null) {
                e.putString(KEY_EMAIL, user.optString("email")).putString(KEY_USER, user.optString("id"))
            }
        }
        e.apply()
    }

    /** POST to GoTrue. Returns the body, or throws with GoTrue's own message. */
    private fun auth(path: String, body: String): JSONObject {
        val conn = (URL("$SUPABASE_URL/auth/v1/$path").openConnection() as HttpURLConnection).apply {
            requestMethod = "POST"
            connectTimeout = 15_000
            readTimeout = 15_000
            doOutput = true
            setRequestProperty("apikey", SUPABASE_ANON)
            setRequestProperty("Content-Type", "application/json")
        }
        conn.outputStream.use { it.write(body.toByteArray()) }
        val ok = conn.responseCode in 200..299
        val text = (if (ok) conn.inputStream else conn.errorStream)?.bufferedReader()?.use { it.readText() } ?: ""
        conn.disconnect()
        val json = runCatching { JSONObject(text) }.getOrDefault(JSONObject())
        if (!ok) {
            val msg = json.optString("error_description").ifBlank { json.optString("msg") }
                .ifBlank { json.optString("message") }.ifBlank { "Sign-in failed (${conn.responseCode})" }
            throw IllegalStateException(msg)
        }
        return json
    }

    /** Email a 6-digit code to an existing Universal ID. */
    suspend fun sendCode(email: String) = withContext(Dispatchers.IO) {
        try {
            auth("otp", JSONObject().put("email", email.trim()).put("create_user", false).toString())
            Unit
        } catch (e: IllegalStateException) {
            // With create_user false, GoTrue answers an unknown address with
            // "Signups not allowed for otp". Say what that means here.
            if (e.message?.contains("not allowed", ignoreCase = true) == true) {
                throw IllegalStateException(
                    "No Universal ID for that email. Create one free at app.unisim.co.uk, then sign in here.",
                )
            }
            throw e
        }
    }

    /** Exchange the emailed code for a session. */
    suspend fun verifyCode(context: Context, email: String, code: String) = withContext(Dispatchers.IO) {
        val session = auth(
            "verify",
            JSONObject().put("email", email.trim()).put("token", code.trim()).put("type", "email").toString(),
        )
        if (session.optString("access_token").isBlank()) throw IllegalStateException("That code was not accepted.")
        store(context, session)
    }

    /** A usable access token, refreshing when due. Null when signed out. */
    suspend fun accessToken(context: Context): String? = withContext(Dispatchers.IO) {
        val p = prefs(context)
        val refresh = p.getString(KEY_REFRESH, null) ?: return@withContext null
        val access = p.getString(KEY_ACCESS, null)
        if (access != null && p.getLong(KEY_EXPIRES, 0) > System.currentTimeMillis()) return@withContext access
        try {
            val next = auth("token?grant_type=refresh_token", JSONObject().put("refresh_token", refresh).toString())
            store(context, next)
            next.optString("access_token").takeIf { it.isNotBlank() }
        } catch (_: Exception) {
            // A refused refresh means the session is gone for good (signed out
            // elsewhere, account deleted) — don't keep a dead one.
            store(context, null)
            null
        }
    }

    suspend fun signOut(context: Context) = withContext(Dispatchers.IO) {
        val token = prefs(context).getString(KEY_ACCESS, null)
        store(context, null)
        if (token != null) {
            runCatching {
                val conn = (URL("$SUPABASE_URL/auth/v1/logout").openConnection() as HttpURLConnection).apply {
                    requestMethod = "POST"
                    connectTimeout = 10_000
                    setRequestProperty("apikey", SUPABASE_ANON)
                    setRequestProperty("Authorization", "Bearer $token")
                }
                conn.responseCode
                conn.disconnect()
            }
        }
        Unit
    }

    /** An authorised REST call. Null when signed out or the call fails. */
    private fun rest(token: String, path: String, method: String = "GET", body: String? = null, prefer: String? = null): String? =
        runCatching {
            val conn = (URL("$SUPABASE_URL/rest/v1/$path").openConnection() as HttpURLConnection).apply {
                requestMethod = method
                connectTimeout = 15_000
                readTimeout = 15_000
                setRequestProperty("apikey", SUPABASE_ANON)
                setRequestProperty("Authorization", "Bearer $token")
                setRequestProperty("Content-Type", "application/json")
                prefer?.let { setRequestProperty("Prefer", it) }
                if (body != null) doOutput = true
            }
            if (body != null) conn.outputStream.use { it.write(body.toByteArray()) }
            val ok = conn.responseCode in 200..299
            val text = if (ok) conn.inputStream.bufferedReader().use { it.readText() } else null
            conn.disconnect()
            text
        }.getOrNull()

    /** The five fields that may leave the device. NOT pin, mode or hidden. */
    private fun row(userId: String, c: SavedConnection): JSONObject = JSONObject()
        .put("user_id", userId)
        .put("addr", c.addr)
        .put("hostname", c.hostname)
        .put("os", c.os)
        .put("custom_name", c.customName)
        .put("last_connected", iso(c.lastConnected))

    private fun iso(ms: Long): String {
        val millis = if (ms > 0) ms else System.currentTimeMillis()
        return java.time.Instant.ofEpochMilli(millis).toString()
    }

    /**
     * Merge this phone's saved machines with the account's — most recently
     * connected wins per address — write back whatever the account was missing,
     * and save the result. Returns true when the list changed.
     *
     * ⚠️ A remote row never overwrites this phone's `pin`, `mode` or `hidden`:
     * they are not synced, and losing the pin would mean retyping it to
     * reconnect to a machine the phone already knew.
     */
    suspend fun syncSaved(context: Context): Boolean = withContext(Dispatchers.IO) {
        val token = accessToken(context) ?: return@withContext false
        val userId = userId(context) ?: return@withContext false
        val body = rest(token, "screens_saved_hosts?select=addr,hostname,os,custom_name,last_connected")
            ?: return@withContext false

        val local = ConnectionStore.load(context)
        val merged = LinkedHashMap<String, SavedConnection>()
        local.forEach { merged[it.addr] = it }

        runCatching {
            val arr = JSONArray(body)
            for (i in 0 until arr.length()) {
                val o = arr.getJSONObject(i)
                val addr = o.getString("addr")
                val remoteAt = runCatching { java.time.Instant.parse(o.getString("last_connected")).toEpochMilli() }
                    .getOrDefault(0L)
                val mine = merged[addr]
                if (mine == null) {
                    merged[addr] = SavedConnection(
                        addr = addr,
                        hostname = o.optString("hostname"),
                        os = o.optString("os"),
                        customName = o.optString("custom_name"),
                        lastConnected = remoteAt,
                    )
                } else if (remoteAt > mine.lastConnected) {
                    // Labels from the account, pin/mode/hidden kept from here.
                    merged[addr] = mine.copy(
                        hostname = o.optString("hostname").ifBlank { mine.hostname },
                        os = o.optString("os").ifBlank { mine.os },
                        customName = o.optString("custom_name"),
                        lastConnected = remoteAt,
                    )
                }
            }
        }

        // Whatever the account did not have, or had staler than this phone.
        val remoteAddrs = runCatching {
            val arr = JSONArray(body)
            (0 until arr.length()).associate {
                val o = arr.getJSONObject(it)
                o.getString("addr") to (runCatching { java.time.Instant.parse(o.getString("last_connected")).toEpochMilli() }.getOrDefault(0L))
            }
        }.getOrDefault(emptyMap())
        val push = local.filter { (remoteAddrs[it.addr] ?: -1) < it.lastConnected }
        if (push.isNotEmpty()) {
            val arr = JSONArray()
            push.forEach { arr.put(row(userId, it)) }
            rest(
                token, "screens_saved_hosts?on_conflict=user_id,addr",
                method = "POST", body = arr.toString(), prefer = "resolution=merge-duplicates",
            )
        }

        val result = merged.values.sortedByDescending { it.lastConnected }
        ConnectionStore.replaceAll(context, result)
        result != local
    }

    /** Save (or refresh) one machine against the account. No-op signed out. */
    suspend fun remember(context: Context, addr: String) = withContext(Dispatchers.IO) {
        val token = accessToken(context) ?: return@withContext
        val userId = userId(context) ?: return@withContext
        val c = ConnectionStore.load(context).find { it.addr == addr } ?: return@withContext
        rest(
            token, "screens_saved_hosts?on_conflict=user_id,addr",
            method = "POST", body = JSONArray().put(row(userId, c)).toString(),
            prefer = "resolution=merge-duplicates",
        )
        Unit
    }

    /** Drop one machine from the account. No-op signed out. */
    suspend fun forget(context: Context, addr: String) = withContext(Dispatchers.IO) {
        val token = accessToken(context) ?: return@withContext
        rest(token, "screens_saved_hosts?addr=eq.${URLEncoder.encode(addr, "UTF-8")}", method = "DELETE")
        Unit
    }
}

/**
 * For the fire-and-forget mirrors — remembering or forgetting a machine against
 * the account as the user does it here. Deliberately NOT a composable's scope:
 * the write must not be cancelled because the screen it started from went away,
 * and `SupervisorJob` keeps one failed call from taking the next one with it.
 */
val accountScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
