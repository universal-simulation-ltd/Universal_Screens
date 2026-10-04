// What this browser remembers about pairing: its own long-term key, the hosts
// it has paired with, and which hosts have ever spoken the v2 handshake.
//
// Pairing (v2) runs SPAKE2 over the PIN, so a recording of the connection gives
// nobody anything to guess the PIN against; the host and this tab then swap
// long-term keys and remember each other, and the next connection needs no PIN
// at all. See `crates/transport/src/handshake.rs`.
//
// ⚠️ localStorage is per origin and per browser profile, and anyone who can run
// script on this origin can read it — including this tab's secret key. The host's
// "Paired devices ▸ Forget" is the remedy. Every access is guarded: storage can
// be missing or throw (private windows, blocked site data), and the page must
// still connect without it — it just pairs with the PIN every time.
//
// This file imports no WASM, so the decision logic (`plan`) is testable in Node.

const KEY = "universal-screens.pairing";
/// Enough for anyone's machines; stops an endless list growing in storage.
const MAX_HOSTS = 64;

function read() {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "null");
    return v && typeof v === "object" ? v : {};
  } catch {
    return {};
  }
}

function write(v) {
  try {
    localStorage.setItem(KEY, JSON.stringify(v));
  } catch {
    // No storage: this tab simply pairs with the PIN next time too.
  }
}

/// This tab's long-term key pair (a WASM `PairingKeys`), created on first use.
export function keys(protocol) {
  const s = read();
  if (typeof s.secret === "string" && typeof s.public === "string") {
    try {
      return protocol.PairingKeys.fromHex(s.secret, s.public);
    } catch {
      // Corrupt: make a new one below. Hosts that knew the old one will ask
      // for the PIN once.
    }
  }
  const k = protocol.PairingKeys.generate();
  write({ ...s, secret: k.secretHex, public: k.publicHex });
  return k;
}

/// The public keys (hex) of every host this tab has paired with.
export function trustedHosts() {
  const hosts = read().hosts;
  return Array.isArray(hosts) ? hosts.filter((h) => typeof h === "string" && /^[0-9a-f]{64}$/.test(h)) : [];
}

/// Remember a host's key after pairing with its PIN.
export function rememberHost(hex) {
  if (typeof hex !== "string" || !/^[0-9a-f]{64}$/.test(hex)) return;
  const s = read();
  const hosts = trustedHosts().filter((h) => h !== hex);
  hosts.push(hex);
  write({ ...s, hosts: hosts.slice(-MAX_HOSTS) });
}

/// Note that `target` (a bridge address or `remote:CODE`) completed a v2
/// handshake — from now on this tab refuses to fall back to v1 for it.
export function markV2(target) {
  if (!target) return;
  const s = read();
  const seen = Array.isArray(s.v2) ? s.v2.filter((t) => t !== target) : [];
  seen.push(target);
  write({ ...s, v2: seen.slice(-MAX_HOSTS) });
}

/// Whether `target` has spoken v2 before.
export function isPinnedV2(target) {
  const seen = read().v2;
  return Array.isArray(seen) && seen.includes(target);
}

/// Forget every paired host on this browser (the key pair is kept).
export function forgetHosts() {
  const s = read();
  write({ ...s, hosts: [], v2: [] });
}

/**
 * Which handshake to run, from what the host said it can do.
 *
 * `hostHandshake`: 2 = the host pairs with SPAKE2 (its bridge answered
 * `usscreens-e2ee.v2`, or its room caps said `handshake: 2`); 1 = a v0.3 host,
 * which can only do the PIN-keyed v1 tunnel; 0 = an older host still, plaintext.
 *
 * Returns:
 * - `"pair"`  — v2, over the PIN;
 * - `"known"` — v2, by remembered key, no PIN (none typed, and this tab has
 *   paired before);
 * - `"v1"` / `"plain"` — what an older host can do;
 * - `"refuse"` — the host spoke v2 to this tab before and now offers less.
 *
 * ⚠️ The refusal is the point of remembering `markV2`. Falling back is what
 * keeps a v0.3 host usable; but a v1 handshake on the wire is exactly what lets
 * a recording be brute-forced for the PIN, so a host known to do better must
 * never be talked down to it — that is someone in the middle, or a reinstall
 * the user should know about.
 */
export function plan({ hostHandshake, pin, trusted, pinned }) {
  if (hostHandshake === 2) {
    if (pin) return "pair";
    return trusted.length ? "known" : "pair";
  }
  if (pinned) return "refuse";
  return hostHandshake === 1 ? "v1" : "plain";
}
