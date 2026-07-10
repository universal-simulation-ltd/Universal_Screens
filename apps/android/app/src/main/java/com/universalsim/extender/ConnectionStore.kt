package com.universalsim.extender

import android.content.Context
import android.os.Build
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
    val pin: Int = 0, // host pairing code (0 = none)
    val hidden: Boolean = false,
    val lastConnected: Long = 0L,
    val customName: String = "", // user-given friendly name ("" = use hostname)
)

/**
 * Persists saved connections in SharedPreferences as a JSON array, one entry per
 * `host:port`. The list is small, so each mutation does a simple load/edit/save.
 */
object ConnectionStore {
    private const val PREFS = "connections"
    private const val KEY = "saved"
    private const val KEY_SENSITIVITY = "trackpadSensitivity"
    private const val KEY_DEVICE_NAME = "deviceName"

    /** This device's human-readable name, sent to the host so the extra screen this
     *  phone adds is labelled (e.g. "James's phone"). Defaults to the hardware model
     *  until the user overrides it in Advanced. Matches the iOS client. */
    fun loadDeviceName(context: Context): String =
        prefs(context).getString(KEY_DEVICE_NAME, null)?.takeIf { it.isNotBlank() } ?: Build.MODEL

    fun saveDeviceName(context: Context, value: String) {
        prefs(context).edit().putString(KEY_DEVICE_NAME, value.trim()).apply()
    }

    /** Trackpad pointer-speed multiplier, persisted app-wide (not per-host). */
    fun loadSensitivity(context: Context): Float =
        prefs(context).getFloat(KEY_SENSITIVITY, 1.6f)

    fun saveSensitivity(context: Context, value: Float) {
        prefs(context).edit().putFloat(KEY_SENSITIVITY, value).apply()
    }

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
                    pin = o.optInt("pin", 0),
                    hidden = o.optBoolean("hidden", false),
                    lastConnected = o.optLong("lastConnected", 0L),
                    customName = o.optString("customName"),
                )
            }
        }.getOrDefault(emptyList())
    }

    /** Insert or update the entry for [addr], stamping it most-recently-used. */
    fun remember(context: Context, addr: String, mode: String, pin: Int) {
        val list = load(context).toMutableList()
        val existing = list.find { it.addr == addr } ?: SavedConnection(addr)
        list.removeAll { it.addr == addr }
        list.add(existing.copy(mode = mode, pin = pin, hidden = false, lastConnected = System.currentTimeMillis()))
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

    /** Set (or clear, with "") the user's friendly name for a saved host. */
    fun setCustomName(context: Context, addr: String, name: String) {
        save(context, load(context).map { if (it.addr == addr) it.copy(customName = name.trim()) else it })
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
                    .put("pin", c.pin)
                    .put("hidden", c.hidden)
                    .put("lastConnected", c.lastConnected)
                    .put("customName", c.customName),
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

/** Short human label for a stored [Mode] name; falls back to the raw text for an
 *  empty/unknown mode (matches iOS's `modeLabel`). Covers all five modes — the old
 *  version rendered raw enum names for Trackpad / Second screen. */
fun modeLabel(mode: String): String =
    runCatching { Mode.valueOf(mode).label() }.getOrDefault(mode)
