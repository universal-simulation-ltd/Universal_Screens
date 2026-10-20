// Saved computers as a file (src/savedFile.js): the envelope it writes, what
// it refuses, what it cleans, and how an opened file merges. No browser, no
// network. Run: node apps/web/saved-file.test.mjs
//
// ⚠️ Keep the "never the PIN" case: the PIN is the Noise pre-shared key, and
// the day someone adds `pin` to a saved entry, this is what stops it being
// written to a file.

import assert from "node:assert/strict";
import {
  ERRORS, MAX_MACHINES, isSaneAddr, mergeSavedMachines, packSavedMachines, readSavedMachines,
} from "./src/savedFile.js";

let failures = 0;
const check = (name, fn) => {
  try {
    fn();
    console.log(`  ok   ${name}`);
  } catch (err) {
    failures++;
    console.log(` FAIL  ${name} — ${err.message}`);
  }
};

const NOW = new Date("2026-10-20T10:00:00.000Z");
const local = [
  { addr: "192.168.1.20:9000", hostname: "Kyjams-iMac", os: "macos", customName: "Office Mac", lastConnected: Date.parse("2026-10-19T09:00:00Z"), pin: 1234 },
  { addr: "192.168.1.30:9000", hostname: "", os: "", customName: "", lastConnected: 0 },
];
const envelope = (data, extra = {}) => JSON.stringify({
  format: "unisim-transfer", version: 1, product: "screens", kind: "saved-machines", schema: 1,
  exportedAt: NOW.toISOString(), app: { name: "Universal Screens", platform: "android" }, data, ...extra,
});

console.log("\nsaving");
{
  const { text, fileName, envelope: env } = packSavedMachines(local, NOW);
  const parsed = JSON.parse(text);
  check("the suite envelope", () => {
    assert.equal(parsed.format, "unisim-transfer");
    assert.equal(parsed.version, 1);
    assert.equal(parsed.product, "screens");
    assert.equal(parsed.kind, "saved-machines");
    assert.equal(parsed.schema, 1);
    assert.equal(parsed.exportedAt, NOW.toISOString());
    assert.deepEqual(parsed.app, { name: "Universal Screens", platform: "web" });
  });
  check("the five fields the sync stored, by their column names", () => {
    assert.deepEqual(parsed.data.machines[0], {
      addr: "192.168.1.20:9000", hostname: "Kyjams-iMac", os: "macos",
      custom_name: "Office Mac", last_connected: "2026-10-19T09:00:00.000Z",
    });
  });
  check("never the PIN", () => {
    assert.ok(!/pin/i.test(text), "a pin made it into the file");
    for (const m of env.data.machines) assert.deepEqual(Object.keys(m).sort(), ["addr", "custom_name", "hostname", "last_connected", "os"]);
  });
  check("a dated file name", () => assert.equal(fileName, "saved-computers-2026-10-20.json"));
  check("round-trips", () => {
    const r = readSavedMachines(text);
    assert.equal(r.ok, true);
    assert.equal(r.machines.length, 2);
    assert.equal(r.machines[0].customName, "Office Mac");
  });
}

console.log("\nrefusing the wrong file, with the plain words");
{
  const reason = (text) => readSavedMachines(text).reason;
  check("not JSON", () => assert.equal(reason("hello"), "not-a-transfer-file"));
  check("JSON, not ours", () => assert.equal(reason('{"machines":[]}'), "not-a-transfer-file"));
  check("another product", () => assert.equal(reason(envelope({ machines: [] }, { product: "workplace" })), "wrong-product"));
  check("another kind", () => assert.equal(reason(envelope({ machines: [] }, { kind: "settings" })), "unknown-kind"));
  check("a newer schema", () => assert.equal(reason(envelope({ machines: [] }, { schema: 2 })), "too-new"));
  check("a newer envelope", () => assert.equal(reason(envelope({ machines: [] }, { version: 2 })), "too-new"));
  check("a password-protected file", () => {
    const text = JSON.stringify({ format: "unisim-transfer", version: 1, product: "screens", kind: "saved-machines", schema: 1, encrypted: { kdf: "PBKDF2-SHA256" } });
    assert.equal(reason(text), "needs-password");
  });
  check("no machines array", () => assert.equal(reason(envelope({ hosts: [] })), "damaged"));
  check("too big", () => assert.equal(reason(" ".repeat(1024 * 1024 + 1)), "too-big"));
  check("too many", () => {
    const machines = Array.from({ length: MAX_MACHINES + 1 }, (_, i) => ({ addr: `10.0.${i >> 8}.${i & 255}:9000` }));
    assert.equal(reason(envelope({ machines })), "too-many");
  });
  check("nothing usable", () => assert.equal(reason(envelope({ machines: [{ addr: "<script>" }] })), "empty"));
  check("each reason has words", () => {
    const r = readSavedMachines("hello");
    assert.equal(r.message, ERRORS["not-a-transfer-file"]);
  });
}

console.log("\nchecking every field");
{
  check("addresses", () => {
    for (const ok of ["192.168.1.20:9000", "mac.local:9002", "[fe80::1]:9000", "127.0.0.1:1"]) assert.ok(isSaneAddr(ok), ok);
    for (const bad of ["192.168.1.20", ":9000", "a b:9000", "host:0", "host:65536", "<img>:9000", "x".repeat(300) + ":1", "[]:9000", 42])
      assert.ok(!isSaneAddr(bad), String(bad));
  });
  const r = readSavedMachines(envelope({
    machines: [
      { addr: "10.0.0.1:9000", hostname: "PC\u0007", os: "windows", custom_name: " Den ", last_connected: "2026-10-01T00:00:00Z" },
      { addr: "10.0.0.2:9000", hostname: 7 },                               // wrong type
      { addr: "10.0.0.3:9000", custom_name: "x".repeat(101) },              // too long
      { addr: "not an address" },
      "a string",
      { addr: "10.0.0.4:9000", os: "Windows <b>", last_connected: "2999-01-01T00:00:00Z" },
      { addr: "10.0.0.1:9000", hostname: "older", last_connected: "2026-01-01T00:00:00Z" }, // duplicate, older
    ],
  }));
  check("good rows kept, bad rows counted", () => {
    assert.equal(r.ok, true);
    assert.deepEqual(r.machines.map((m) => m.addr), ["10.0.0.1:9000", "10.0.0.4:9000"]);
    assert.equal(r.skipped, 4);
  });
  check("control characters stripped, text trimmed", () => {
    assert.equal(r.machines[0].hostname, "PC");
    assert.equal(r.machines[0].customName, "Den");
  });
  check("a duplicate address keeps the most recent", () => assert.equal(r.machines[0].hostname, "PC"));
  check("an odd OS is dropped, a future date is clamped to now", () => {
    assert.equal(r.machines[1].os, "");
    assert.ok(r.machines[1].lastConnected <= Date.now());
  });
}

console.log("\nmerging by address");
{
  const incoming = [
    { addr: "192.168.1.20:9000", hostname: "Kyjams-iMac", os: "macos", customName: "Stale name", lastConnected: Date.parse("2026-10-01T00:00:00Z") },
    { addr: "192.168.1.30:9000", hostname: "Linux box", os: "linux", customName: "Lab", lastConnected: Date.parse("2026-10-18T00:00:00Z") },
    { addr: "10.0.0.9:9000", hostname: "New", os: "windows", customName: "", lastConnected: Date.parse("2026-10-17T00:00:00Z") },
  ];
  const { list, added, updated } = mergeSavedMachines(local, incoming);
  check("no duplicates", () => assert.equal(new Set(list.map((h) => h.addr)).size, list.length));
  check("one added, one updated", () => { assert.equal(added, 1); assert.equal(updated, 1); });
  check("an older copy never overwrites a newer one", () =>
    assert.equal(list.find((h) => h.addr === "192.168.1.20:9000").customName, "Office Mac"));
  check("a newer copy brings its labels", () => {
    const h = list.find((x) => x.addr === "192.168.1.30:9000");
    assert.equal(h.hostname, "Linux box");
    assert.equal(h.customName, "Lab");
  });
  check("most recent first", () => assert.equal(list[0].addr, "192.168.1.20:9000"));
  check("a second open of the same file changes nothing", () => {
    const again = mergeSavedMachines(list, incoming);
    assert.equal(again.added, 0);
    assert.equal(again.updated, 0);
  });
}

console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
