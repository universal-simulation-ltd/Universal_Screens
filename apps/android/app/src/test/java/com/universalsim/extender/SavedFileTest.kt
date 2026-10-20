package com.universalsim.extender

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The saved-computers file (SavedFile.kt): the envelope it writes, what it
 * refuses, what it cleans, and how an opened file merges. The browser's
 * `apps/web/saved-file.test.mjs` checks the same file from the other side.
 *
 * ⚠️ Keep `neverThePin`: the PIN is the Noise pre-shared key.
 */
class SavedFileTest {
    private val now = SavedFile.parseIso("2026-10-20T10:00:00.000Z")!!

    private val local = listOf(
        SavedConnection(
            addr = "192.168.1.20:9000", hostname = "Kyjams-iMac", os = "macos", mode = "CLICKER",
            pin = 4821, lastConnected = SavedFile.parseIso("2026-10-19T09:00:00Z")!!, customName = "Office Mac",
        ),
        SavedConnection(addr = "192.168.1.30:9000"),
    )

    private fun envelope(machines: String, extra: String = ""): String =
        """{"format":"unisim-transfer","version":1,"product":"screens","kind":"saved-machines","schema":1,""" +
            """"exportedAt":"2026-10-20T10:00:00.000Z","app":{"name":"Universal Screens","platform":"web"}$extra,""" +
            """"data":{"machines":$machines}}"""

    private fun reason(text: String): String = (SavedFile.read(text, now) as SavedFile.Opened.Failed).reason

    @Test fun writesTheSuiteEnvelope() {
        val o = JSONObject(SavedFile.pack(local, now))
        assertEquals("unisim-transfer", o.getString("format"))
        assertEquals(1, o.getInt("version"))
        assertEquals("screens", o.getString("product"))
        assertEquals("saved-machines", o.getString("kind"))
        assertEquals(1, o.getInt("schema"))
        assertEquals("2026-10-20T10:00:00.000Z", o.getString("exportedAt"))
        assertEquals("android", o.getJSONObject("app").getString("platform"))
        val m = o.getJSONObject("data").getJSONArray("machines").getJSONObject(0)
        assertEquals(setOf("addr", "hostname", "os", "custom_name", "last_connected"), m.keySet())
        assertEquals("Office Mac", m.getString("custom_name"))
        assertEquals("2026-10-19T09:00:00.000Z", m.getString("last_connected"))
        assertEquals("saved-computers-2026-10-20.json", SavedFile.fileName(now))
    }

    @Test fun neverThePin() {
        val text = SavedFile.pack(local, now)
        assertFalse(text.contains("4821"))
        assertFalse(text.contains("pin", ignoreCase = true))
        assertFalse(text.contains("CLICKER"))
    }

    @Test fun roundTrips() {
        val r = SavedFile.read(SavedFile.pack(local, now), now) as SavedFile.Opened.Ok
        assertEquals(2, r.machines.size)
        assertEquals("Office Mac", r.machines[0].customName)
        assertEquals(0, r.skipped)
    }

    @Test fun refusesTheWrongFile() {
        assertEquals("not-a-transfer-file", reason("hello"))
        assertEquals("not-a-transfer-file", reason("""{"machines":[]}"""))
        assertEquals("wrong-product", reason(envelope("[]").replace("\"screens\"", "\"workplace\"")))
        assertEquals("unknown-kind", reason(envelope("[]").replace("saved-machines", "settings")))
        assertEquals("too-new", reason(envelope("[]").replace("\"schema\":1", "\"schema\":2")))
        assertEquals("too-new", reason(envelope("[]").replace("\"version\":1", "\"version\":2")))
        assertEquals("needs-password", reason(envelope("[]", ""","encrypted":{"kdf":"PBKDF2-SHA256"}""")))
        assertEquals("damaged", reason(envelope("{}")))
        assertEquals("too-big", reason(" ".repeat(SavedFile.MAX_BYTES + 1)))
        val many = (0..SavedFile.MAX_MACHINES).joinToString(",", "[", "]") { """{"addr":"10.0.${it / 256}.${it % 256}:9000"}""" }
        assertEquals("too-many", reason(envelope(many)))
        assertEquals("empty", reason(envelope("""[{"addr":"<script>"}]""")))
        assertEquals("This file was saved by a different app.", SavedFile.errorText("wrong-product"))
    }

    @Test fun checksTheAddress() {
        for (ok in listOf("192.168.1.20:9000", "mac.local:9002", "[fe80::1]:9000", "127.0.0.1:1")) {
            assertTrue(ok, SavedFile.isSaneAddr(ok))
        }
        for (bad in listOf("192.168.1.20", ":9000", "a b:9000", "host:0", "host:65536", "<img>:9000", "x".repeat(300) + ":1", "[]:9000")) {
            assertFalse(bad, SavedFile.isSaneAddr(bad))
        }
    }

    @Test fun checksEveryField() {
        val r = SavedFile.read(
            envelope(
                """[
                  {"addr":"10.0.0.1:9000","hostname":"PC\u0007","os":"windows","custom_name":" Den ","last_connected":"2026-10-01T00:00:00Z"},
                  {"addr":"10.0.0.2:9000","hostname":7},
                  {"addr":"10.0.0.3:9000","custom_name":"${"x".repeat(101)}"},
                  {"addr":"not an address"},
                  "a string",
                  {"addr":"10.0.0.4:9000","os":"Windows <b>","last_connected":"2999-01-01T00:00:00Z"},
                  {"addr":"10.0.0.1:9000","hostname":"older","last_connected":"2026-01-01T00:00:00+01:00"}
                ]""",
            ),
            now,
        ) as SavedFile.Opened.Ok
        assertEquals(listOf("10.0.0.1:9000", "10.0.0.4:9000"), r.machines.map { it.addr })
        assertEquals(4, r.skipped)
        assertEquals("PC", r.machines[0].hostname)
        assertEquals("Den", r.machines[0].customName)
        assertEquals("windows", r.machines[0].os)
        assertEquals("", r.machines[1].os)
        assertEquals(now, r.machines[1].lastConnected)
    }

    @Test fun mergesByAddressKeepingThePhonesOwnFields() {
        val incoming = listOf(
            SavedFile.Machine("192.168.1.20:9000", "Kyjams-iMac", "macos", "Stale name", SavedFile.parseIso("2026-10-01T00:00:00Z")!!),
            SavedFile.Machine("192.168.1.30:9000", "Linux box", "linux", "Lab", SavedFile.parseIso("2026-10-18T00:00:00Z")!!),
            SavedFile.Machine("10.0.0.9:9000", "New", "windows", "", SavedFile.parseIso("2026-10-17T00:00:00Z")!!),
        )
        val m = SavedFile.merge(local, incoming)
        assertEquals(3, m.list.size)
        assertEquals(1, m.added)
        assertEquals(1, m.updated)
        val mac = m.list.first { it.addr == "192.168.1.20:9000" }
        assertEquals("Office Mac", mac.customName) // an older copy never overwrites
        assertEquals(4821, mac.pin)
        assertEquals("CLICKER", mac.mode)
        val linux = m.list.first { it.addr == "192.168.1.30:9000" }
        assertEquals("Lab", linux.customName)
        val fresh = m.list.first { it.addr == "10.0.0.9:9000" }
        assertEquals(0, fresh.pin)
        assertEquals("", fresh.mode) // first tap asks how to use it
        assertEquals("192.168.1.20:9000", m.list[0].addr)
        val again = SavedFile.merge(m.list, incoming)
        assertEquals(0, again.added)
        assertEquals(0, again.updated)
    }

    @Test fun parsesDates() {
        assertEquals(0L, SavedFile.parseIso("1970-01-01T00:00:00Z"))
        assertEquals(3_600_000L, SavedFile.parseIso("1970-01-01T02:00:00+01:00"))
        assertEquals(1_500L, SavedFile.parseIso("1970-01-01T00:00:01.5Z"))
        assertEquals(null, SavedFile.parseIso("2026-13-01T00:00:00Z"))
        assertEquals(null, SavedFile.parseIso("yesterday"))
        assertEquals("2026-10-20T10:00:00.000Z", SavedFile.iso(now))
    }
}
