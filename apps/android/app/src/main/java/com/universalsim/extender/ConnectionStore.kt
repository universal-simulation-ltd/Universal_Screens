package com.universalsim.extender

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject

/**
 * A remembered host, shown on the connect screen for quick reconnect. `mode` is a
 * [Mode] name so a quick-connect reuses how you last used this host. `os` is
 * learned from the host's HostInfo after connecting (empty until then).
 */
data class SavedConnection(
    val addr: String,
    val hostname: String = "",
    val os: String = "", // "windows" | "macos" | "linux" | ""
    val mode: String = Mode.CLICKER.name,
    val hidden: Boolean = false,
    val lastConnected: Long = 0L,
)

/**
 * Persists saved connections in SharedPreferences as a JSON array, one entry per
 * `host:port`. The list is small, so each mutation does a simple load/edit/save.
 */
object ConnectionStore {
    private const val PREFS = "connections"
    private const val KEY = "saved"

    fun load(context: Context): List<SavedConnection> {
        val raw = prefs(context).getString(KEY, null) ?: return emptyList()
        return runCatching {
            val arr = JSONArray(raw)
            (0 until arr.length()).map { i ->
                val o = arr.getJSONObject(i)
                SavedConnection(
                    addr = o.getString("addr"),
                    hostname = o.optString("hostname"),
                    os = o.optString("os"),
                    mode = o.optString("mode", Mode.CLICKER.name),
                    hidden = o.optBoolean("hidden", false),
                    lastConnected = o.optLong("lastConnected", 0L),
                )
            }
        }.getOrDefault(emptyList())
    }

    /** Insert or update the entry for [addr], stamping it most-recently-used. */
    fun remember(context: Context, addr: String, mode: String) {
        val list = load(context).toMutableList()
        val existing = list.find { it.addr == addr } ?: SavedConnection(addr)
        list.removeAll { it.addr == addr }
        list.add(existing.copy(mode = mode, hidden = false, lastConnected = System.currentTimeMillis()))
        save(context, list)
    }

    /** Record the host identity (OS + machine name) learned from HostInfo. */
    fun setIdentity(context: Context, addr: String, os: String, hostname: String) {
        val list = load(context)
        if (list.none { it.addr == addr }) return
        save(context, list.map { if (it.addr == addr) it.copy(os = os, hostname = hostname) else it })
    }

    fun setHidden(context: Context, addr: String, hidden: Boolean) {
        save(context, load(context).map { if (it.addr == addr) it.copy(hidden = hidden) else it })
    }

    fun delete(context: Context, addr: String) {
        save(context, load(context).filterNot { it.addr == addr })
    }

    private fun save(context: Context, list: List<SavedConnection>) {
        val arr = JSONArray()
        list.forEach { c ->
            arr.put(
                JSONObject()
                    .put("addr", c.addr)
                    .put("hostname", c.hostname)
                    .put("os", c.os)
                    .put("mode", c.mode)
                    .put("hidden", c.hidden)
                    .put("lastConnected", c.lastConnected),
            )
        }
        prefs(context).edit().putString(KEY, arr.toString()).apply()
    }

    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
}

/** A glyph standing in for the host's OS, shown on saved-connection rows. */
fun deviceEmoji(os: String): String = when (os.lowercase()) {
    "windows" -> "🪟" // 🪟
    "macos", "mac" -> "🍏" // 🍎
    "linux" -> "🐧" // 🐧
    else -> "🖥️" // 🖥️
}

/** Short human label for a stored [Mode] name. */
fun modeLabel(mode: String): String = when (mode) {
    Mode.CLICKER.name -> "Clicker"
    Mode.VIEWER.name -> "Viewer"
    Mode.FULL_CONTROL.name -> "Control"
    else -> mode
}
