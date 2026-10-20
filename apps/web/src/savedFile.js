// Saved computers as a file: "Save saved computers to a file" on one device,
// "Open a file of saved computers" on another. It replaced the optional
// Universal ID sign-in, whose only job was carrying this list between devices
// (James, 2026-10-20: "avoid logins except where it really improves
// experience … introduce an export / import function").
//
// The file is the suite's transfer envelope (Docs_UNI_SIM/login-free-plan.md;
// the reference reader is `@unisim/sdk/transfer`). This client has no npm and
// no bundler, so the envelope is written out here by hand, to the same spec:
//
//   { format: "unisim-transfer", version: 1, product: "screens",
//     kind: "saved-machines", schema: 1, exportedAt, app: { name, platform },
//     data: { machines: [{ addr, hostname, os, custom_name, last_connected }] } }
//
// The Android app writes and reads the very same file (SavedFile.kt); keep the
// two in step.
//
// ⚠️ **Never the PIN.** It is the Noise pre-shared key. The browser's saved
// list has no PIN to begin with, but every mapping below still names its five
// fields one by one, so nothing extra can ride along.
//
// ⚠️ **An opened file is untrusted.** Anyone can write one. Every field is
// type-checked and length-capped, the address must look like host:port, the
// count is capped, and the rows are drawn with textContent (client.js).

export const PRODUCT = "screens";
export const KIND = "saved-machines";
export const SCHEMA = 1;
/** A list of LAN addresses is tiny; anything near this is not one. */
export const MAX_BYTES = 1024 * 1024;
export const MAX_MACHINES = 200;
export const ACCEPT = ".json,application/json";

const MAX_ADDR = 260;
const MAX_HOSTNAME = 255;
const MAX_OS = 32;
const MAX_NAME = 100;

/** The error copy, in the SDK's English wording (transferErrorText). */
export const ERRORS = {
  "not-a-transfer-file": "This isn't a file this app saved. Choose the .json file you saved from the app.",
  "wrong-product": "This file was saved by a different app.",
  "unknown-kind": "This file holds something this page can't open.",
  "too-new": "This file was saved by a newer version of the app. Update this one, then try again.",
  "too-big": "This file is too big to open.",
  "needs-password": "This file has a password, and Universal Screens can't open password-protected files.",
  damaged: "This file is damaged and can't be opened.",
  "too-many": `This file holds more than ${MAX_MACHINES} computers, which is more than Universal Screens keeps.`,
  empty: "This file has no saved computers in it that Universal Screens can use.",
};

/**
 * Builds the file for this device's saved list (saved.js entries:
 * addr, hostname, os, customName, lastConnected in ms).
 */
export function packSavedMachines(list, now = new Date()) {
  const envelope = {
    format: "unisim-transfer",
    version: 1,
    product: PRODUCT,
    kind: KIND,
    schema: SCHEMA,
    exportedAt: now.toISOString(),
    app: { name: "Universal Screens", platform: "web" },
    data: {
      machines: list.map((h) => ({
        addr: String(h.addr ?? ""),
        hostname: String(h.hostname ?? ""),
        os: String(h.os ?? ""),
        custom_name: String(h.customName ?? ""),
        last_connected: new Date(h.lastConnected > 0 ? h.lastConnected : now.getTime()).toISOString(),
      })),
    },
  };
  return {
    text: JSON.stringify(envelope, null, 2),
    fileName: `saved-computers-${now.toISOString().slice(0, 10)}.json`,
    envelope,
  };
}

/** host:port — a name or IPv4 address, or a bracketed IPv6 one; port 1–65535. */
export function isSaneAddr(addr) {
  if (typeof addr !== "string" || addr.length === 0 || addr.length > MAX_ADDR) return false;
  const m = /^(\[[0-9A-Fa-f:.%a-zA-Z]+\]|[A-Za-z0-9](?:[A-Za-z0-9.\-_]*[A-Za-z0-9])?):(\d{1,5})$/.exec(addr);
  if (!m) return false;
  const port = Number(m[2]);
  if (port < 1 || port > 65535) return false;
  if (m[1].startsWith("[")) return m[1].includes(":");
  return true;
}

/** A plain one-line string: no control characters, trimmed, capped. */
function cleanText(v, max) {
  if (v === undefined || v === null) return "";
  if (typeof v !== "string") return null;
  // eslint-disable-next-line no-control-regex
  const s = v.replace(/[\u0000-\u001f\u007f]/g, "").trim();
  return s.length > max ? null : s;
}

/** One machine from the file, or null if any field is wrong. */
export function cleanMachine(m) {
  if (typeof m !== "object" || m === null || Array.isArray(m)) return null;
  const addr = typeof m.addr === "string" ? m.addr.trim() : "";
  if (!isSaneAddr(addr)) return null;
  const hostname = cleanText(m.hostname, MAX_HOSTNAME);
  const custom = cleanText(m.custom_name, MAX_NAME);
  const osRaw = cleanText(m.os, MAX_OS);
  if (hostname === null || custom === null || osRaw === null) return null;
  // An OS is a short lower-case word ("windows", "macos", "linux"); anything
  // else is dropped rather than refused — it only picks the row's glyph.
  const os = /^[a-z0-9_-]*$/.test(osRaw) ? osRaw : "";
  let lastConnected = 0;
  if (m.last_connected !== undefined && m.last_connected !== null) {
    if (typeof m.last_connected !== "string") return null;
    const t = Date.parse(m.last_connected);
    // A date in the future would pin the row to the top forever.
    if (Number.isFinite(t) && t > 0) lastConnected = Math.min(t, Date.now());
  }
  return { addr, hostname, os, customName: custom, lastConnected };
}

/**
 * Opens a file's text. Never throws: `{ ok: true, machines, skipped }` or
 * `{ ok: false, reason, message }`.
 */
export function readSavedMachines(text) {
  const fail = (reason) => ({ ok: false, reason, message: ERRORS[reason] });
  if (typeof text !== "string") return fail("damaged");
  if (text.length > MAX_BYTES) return fail("too-big");
  let raw;
  try {
    raw = JSON.parse(text.replace(/^﻿/, ""));
  } catch {
    return fail("not-a-transfer-file");
  }
  const isRecord = (v) => typeof v === "object" && v !== null && !Array.isArray(v);
  if (!isRecord(raw) || raw.format !== "unisim-transfer") return fail("not-a-transfer-file");
  if (typeof raw.version !== "number" || raw.version > 1) return fail("too-new");
  if (raw.product !== PRODUCT) return fail("wrong-product");
  if (raw.kind !== KIND) return fail("unknown-kind");
  if (typeof raw.schema !== "number" || !Number.isInteger(raw.schema) || raw.schema < 1) return fail("damaged");
  if (raw.schema > SCHEMA) return fail("too-new");
  if (raw.encrypted !== undefined) return fail("needs-password");
  if (!isRecord(raw.data) || !Array.isArray(raw.data.machines)) return fail("damaged");
  if (raw.data.machines.length > MAX_MACHINES) return fail("too-many");

  const byAddr = new Map();
  let skipped = 0;
  for (const m of raw.data.machines) {
    const c = cleanMachine(m);
    if (!c) { skipped++; continue; }
    // The same address twice in one file: keep the most recent.
    const prev = byAddr.get(c.addr);
    if (!prev || c.lastConnected > prev.lastConnected) byAddr.set(c.addr, c);
  }
  if (byAddr.size === 0) return fail("empty");
  return { ok: true, machines: [...byAddr.values()], skipped };
}

/**
 * Merges opened machines into the local list by address — no duplicates.
 * A machine already here keeps its entry; the file's labels win only when the
 * file's copy was used more recently. Returns `{ list, added, updated }`.
 */
export function mergeSavedMachines(local, incoming) {
  const merged = new Map(local.map((h) => [h.addr, { ...h }]));
  let added = 0;
  let updated = 0;
  for (const m of incoming) {
    const mine = merged.get(m.addr);
    if (!mine) {
      merged.set(m.addr, { ...m });
      added++;
    } else if (m.lastConnected > (mine.lastConnected ?? 0)) {
      merged.set(m.addr, {
        ...mine,
        hostname: m.hostname || mine.hostname,
        os: m.os || mine.os,
        customName: m.customName,
        lastConnected: m.lastConnected,
      });
      updated++;
    }
  }
  const list = [...merged.values()].sort((a, b) => b.lastConnected - a.lastConnected);
  return { list, added, updated };
}
