package com.universalsim.extender

import org.json.JSONArray
import org.json.JSONObject
import java.util.Calendar
import java.util.Locale
import java.util.TimeZone

/**
 * Saved computers as a file: "Save saved computers to a file" on one device,
 * "Open a file of saved computers" on another. It replaced the optional
 * Universal ID sign-in, whose only job was carrying this list between devices
 * (James, 2026-10-20: "avoid logins except where it really improves
 * experience … introduce an export / import function").
 *
 * The file is the suite's transfer envelope (Docs_UNI_SIM/login-free-plan.md;
 * the reference reader is `@unisim/sdk/transfer`), written out here by hand:
 *
 *   { format: "unisim-transfer", version: 1, product: "screens",
 *     kind: "saved-machines", schema: 1, exportedAt, app: { name, platform },
 *     data: { machines: [{ addr, hostname, os, custom_name, last_connected }] } }
 *
 * The browser client writes and reads the very same file
 * (`apps/web/src/savedFile.js`); keep the two in step.
 *
 * ⚠️ **Never the PIN.** `SavedConnection` on this client DOES hold one, and it
 * is the Noise pre-shared key. Every mapping below names its five fields one
 * by one for exactly that reason; do not replace one with a whole-object
 * serialiser. `mode` and `hidden` are this phone's own and stay here too.
 *
 * ⚠️ **An opened file is untrusted.** Anyone can write one. Every field is
 * type-checked and length-capped, the address must look like host:port, the
 * count is capped, and a password-protected file is refused (this app has no
 * password support).
 */
object SavedFile {
    const val PRODUCT = "screens"
    const val KIND = "saved-machines"
    const val SCHEMA = 1
    /** A list of LAN addresses is tiny; anything near this is not one. */
    const val MAX_BYTES = 1024 * 1024
    const val MAX_MACHINES = 200
    const val MIME = "application/json"

    private const val MAX_ADDR = 260
    private const val MAX_HOSTNAME = 255
    private const val MAX_OS = 32
    private const val MAX_NAME = 100

    /** One machine as the file carries it. */
    data class Machine(
        val addr: String,
        val hostname: String,
        val os: String,
        val customName: String,
        val lastConnected: Long,
    )

    sealed class Opened {
        data class Ok(val machines: List<Machine>, val skipped: Int) : Opened()
        data class Failed(val reason: String) : Opened() {
            val message: String get() = errorText(reason)
        }
    }

    data class Merged(val list: List<SavedConnection>, val added: Int, val updated: Int)

    /** The error copy, in the SDK's English wording (transferErrorText). */
    fun errorText(reason: String): String = when (reason) {
        "not-a-transfer-file" -> "This isn't a file this app saved. Choose the .json file you saved from the app."
        "wrong-product" -> "This file was saved by a different app."
        "unknown-kind" -> "This file holds something this app can't open."
        "too-new" -> "This file was saved by a newer version of the app. Update this one, then try again."
        "too-big" -> "This file is too big to open."
        "needs-password" -> "This file has a password, and Universal Screens can't open password-protected files."
        "too-many" -> "This file holds more than $MAX_MACHINES computers, which is more than Universal Screens keeps."
        "empty" -> "This file has no saved computers in it that Universal Screens can use."
        else -> "This file is damaged and can't be opened."
    }

    /** `saved-computers-YYYY-MM-DD.json`. */
    fun fileName(nowMs: Long = System.currentTimeMillis()): String =
        "saved-computers-${iso(nowMs).substring(0, 10)}.json"

    /** The file for this phone's saved list. */
    fun pack(list: List<SavedConnection>, nowMs: Long = System.currentTimeMillis()): String {
        val machines = JSONArray()
        list.forEach { c ->
            // The five fields, by name. NOT pin, mode or hidden.
            machines.put(
                JSONObject()
                    .put("addr", c.addr)
                    .put("hostname", c.hostname)
                    .put("os", c.os)
                    .put("custom_name", c.customName)
                    .put("last_connected", iso(if (c.lastConnected > 0) c.lastConnected else nowMs)),
            )
        }
        return JSONObject()
            .put("format", "unisim-transfer")
            .put("version", 1)
            .put("product", PRODUCT)
            .put("kind", KIND)
            .put("schema", SCHEMA)
            .put("exportedAt", iso(nowMs))
            .put("app", JSONObject().put("name", "Universal Screens").put("platform", "android"))
            .put("data", JSONObject().put("machines", machines))
            .toString(2)
    }

    private val ADDR = Regex("""^(\[[0-9A-Fa-f:.%a-zA-Z]+\]|[A-Za-z0-9](?:[A-Za-z0-9.\-_]*[A-Za-z0-9])?):(\d{1,5})$""")

    /** host:port — a name or IPv4 address, or a bracketed IPv6 one; port 1–65535. */
    fun isSaneAddr(addr: String): Boolean {
        if (addr.isEmpty() || addr.length > MAX_ADDR) return false
        val m = ADDR.matchEntire(addr) ?: return false
        val port = m.groupValues[2].toIntOrNull() ?: return false
        if (port !in 1..65535) return false
        val host = m.groupValues[1]
        return !host.startsWith("[") || host.contains(':')
    }

    /** A plain one-line string, or "" when absent; null when wrong. */
    private fun cleanText(o: JSONObject, key: String, max: Int): String? {
        if (!o.has(key) || o.isNull(key)) return ""
        val v = o.get(key) as? String ?: return null
        val s = v.filter { it >= ' ' && it != '\u007f' }.trim()
        return if (s.length > max) null else s
    }

    /** One machine from the file, or null if any field is wrong. */
    fun cleanMachine(any: Any?, nowMs: Long = System.currentTimeMillis()): Machine? {
        val o = any as? JSONObject ?: return null
        val addr = (o.opt("addr") as? String)?.trim() ?: return null
        if (!isSaneAddr(addr)) return null
        val hostname = cleanText(o, "hostname", MAX_HOSTNAME) ?: return null
        val custom = cleanText(o, "custom_name", MAX_NAME) ?: return null
        val osRaw = cleanText(o, "os", MAX_OS) ?: return null
        // An OS is a short lower-case word; anything else only picked the
        // row's glyph, so it is dropped rather than refused.
        val os = if (osRaw.all { it in 'a'..'z' || it in '0'..'9' || it == '_' || it == '-' }) osRaw else ""
        var last = 0L
        if (o.has("last_connected") && !o.isNull("last_connected")) {
            val s = o.get("last_connected") as? String ?: return null
            // A date in the future would pin the row to the top forever.
            parseIso(s)?.let { if (it > 0) last = minOf(it, nowMs) }
        }
        return Machine(addr, hostname, os, custom, last)
    }

    /** Opens a file's text. Never throws. */
    fun read(text: String, nowMs: Long = System.currentTimeMillis()): Opened {
        if (text.length > MAX_BYTES) return Opened.Failed("too-big")
        val raw = try {
            JSONObject(text.removePrefix("﻿"))
        } catch (_: Exception) {
            return Opened.Failed("not-a-transfer-file")
        }
        if (raw.optString("format") != "unisim-transfer") return Opened.Failed("not-a-transfer-file")
        val version = raw.opt("version") as? Number ?: return Opened.Failed("too-new")
        if (version.toDouble() > 1) return Opened.Failed("too-new")
        if (raw.opt("product") != PRODUCT) return Opened.Failed("wrong-product")
        if (raw.opt("kind") != KIND) return Opened.Failed("unknown-kind")
        val schema = raw.opt("schema") as? Number ?: return Opened.Failed("damaged")
        if (schema.toDouble() != Math.floor(schema.toDouble()) || schema.toDouble() < 1) return Opened.Failed("damaged")
        if (schema.toDouble() > SCHEMA) return Opened.Failed("too-new")
        if (raw.has("encrypted")) return Opened.Failed("needs-password")
        val data = raw.opt("data") as? JSONObject ?: return Opened.Failed("damaged")
        val machines = data.opt("machines") as? JSONArray ?: return Opened.Failed("damaged")
        if (machines.length() > MAX_MACHINES) return Opened.Failed("too-many")

        val byAddr = LinkedHashMap<String, Machine>()
        var skipped = 0
        for (i in 0 until machines.length()) {
            val m = cleanMachine(machines.opt(i), nowMs)
            if (m == null) {
                skipped++
                continue
            }
            // The same address twice in one file: keep the most recent.
            val prev = byAddr[m.addr]
            if (prev == null || m.lastConnected > prev.lastConnected) byAddr[m.addr] = m
        }
        if (byAddr.isEmpty()) return Opened.Failed("empty")
        return Opened.Ok(byAddr.values.toList(), skipped)
    }

    /**
     * Merges opened machines into this phone's list by address — no
     * duplicates. A machine already here keeps its entry, and ALWAYS its own
     * pin, mode and hidden; the file's labels win only when the file's copy
     * was used more recently. A new machine comes in with no PIN and no
     * remembered mode, so its first tap asks for both.
     */
    fun merge(local: List<SavedConnection>, incoming: List<Machine>): Merged {
        val merged = LinkedHashMap<String, SavedConnection>()
        local.forEach { merged[it.addr] = it }
        var added = 0
        var updated = 0
        for (m in incoming) {
            val mine = merged[m.addr]
            if (mine == null) {
                merged[m.addr] = SavedConnection(
                    addr = m.addr,
                    hostname = m.hostname,
                    os = m.os,
                    mode = "",
                    customName = m.customName,
                    lastConnected = m.lastConnected,
                )
                added++
            } else if (m.lastConnected > mine.lastConnected) {
                merged[m.addr] = mine.copy(
                    hostname = m.hostname.ifBlank { mine.hostname },
                    os = m.os.ifBlank { mine.os },
                    customName = m.customName,
                    lastConnected = m.lastConnected,
                )
                updated++
            }
        }
        return Merged(merged.values.sortedByDescending { it.lastConnected }, added, updated)
    }

    // ── Dates, by hand: java.time needs API 26 and this app runs from 24 ──

    private fun utc(): Calendar = Calendar.getInstance(TimeZone.getTimeZone("UTC"), Locale.ROOT)

    /** `2026-10-20T10:00:00.000Z`. */
    fun iso(ms: Long): String {
        val c = utc().apply { timeInMillis = ms }
        return String.format(
            Locale.ROOT, "%04d-%02d-%02dT%02d:%02d:%02d.%03dZ",
            c.get(Calendar.YEAR), c.get(Calendar.MONTH) + 1, c.get(Calendar.DAY_OF_MONTH),
            c.get(Calendar.HOUR_OF_DAY), c.get(Calendar.MINUTE), c.get(Calendar.SECOND), c.get(Calendar.MILLISECOND),
        )
    }

    private val ISO = Regex("""^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,9}))?)?(Z|[+-]\d{2}:?\d{2})$""")

    /** Milliseconds since 1970 for an ISO 8601 date-time with a zone, or null. */
    fun parseIso(s: String): Long? {
        val m = ISO.matchEntire(s.trim()) ?: return null
        val g = m.groupValues
        val month = g[2].toInt()
        val day = g[3].toInt()
        val hour = g[4].toInt()
        val minute = g[5].toInt()
        val second = g[6].ifEmpty { "0" }.toInt()
        if (month !in 1..12 || day !in 1..31 || hour > 23 || minute > 59 || second > 59) return null
        val millis = g[7].ifEmpty { "0" }.padEnd(3, '0').take(3).toInt()
        val c = utc().apply {
            isLenient = false
            clear()
            set(g[1].toInt(), month - 1, day, hour, minute, second)
            set(Calendar.MILLISECOND, millis)
        }
        var ms = runCatching { c.timeInMillis }.getOrNull() ?: return null
        val zone = g[8]
        if (zone != "Z") {
            val sign = if (zone[0] == '-') -1 else 1
            val digits = zone.substring(1).replace(":", "")
            val offset = (digits.substring(0, 2).toInt() * 60 + digits.substring(2, 4).toInt()) * 60_000L
            ms -= sign * offset
        }
        return ms
    }
}

/** Up to [limit] bytes from the stream (InputStream.readNBytes needs API 33). */
fun java.io.InputStream.readNBytesCompat(limit: Int): ByteArray {
    val out = java.io.ByteArrayOutputStream()
    val buf = ByteArray(8192)
    while (out.size() < limit) {
        val n = read(buf, 0, minOf(buf.size, limit - out.size()))
        if (n < 0) break
        out.write(buf, 0, n)
    }
    return out.toByteArray()
}
