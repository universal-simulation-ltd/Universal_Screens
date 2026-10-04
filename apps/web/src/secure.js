// The encrypted browser leg: a Noise tunnel between this tab and the host, so
// neither the LAN bridge nor the cloud relay can read the mirrored screen or the
// keystrokes going back — and, with a current host, pairing that a recording
// cannot be used to guess the PIN from.
//
// The cryptography is the same Rust the native clients use, compiled to WASM
// (`crates/protocol-wasm/src/tunnel.rs` over `crates/transport/src/session.rs`).
// This file is the plumbing around it.
//
// Two handshakes:
//
// - **v2** (`kind: "pair"` or `"known"`): SPAKE2 over the PIN then Noise, or
//   reconnecting with this tab's remembered key and no PIN. Several round trips,
//   so the channel sends its own replies through `onSend`.
// - **v1** (`kind: "v1"`): the PIN-keyed `NNpsk0` tunnel, for a v0.3 host only.
//   ⚠️ A recording of it lets the PIN be guessed offline.
//
// ⚠️ Two things the bridge stops doing once a connection is encrypted, and this
// has to do instead:
//
//   1. Add the 4-byte little-endian length prefix to every outgoing message. The
//      host reads a *stream*, not messages.
//   2. Re-assemble that stream on the way back. A Noise record boundary has
//      nothing to do with a message boundary.
//
// Neither is visible in a small test — one WS message usually happens to carry
// one record carrying one message — right up until a 200 KB keyframe arrives.
import * as pairing from "./pairing.js";

/// Wraps a WebSocket in the tunnel. Feed it what arrives, ask it for whole
/// protocol messages back.
///
/// Lifecycle: `new` → set `onSend` → send `firstMessage` → push every inbound
/// chunk through `receive()` → once `open` is true, `seal()` what you send and
/// read whole message bodies from what `receive()` returns. If `receive()`
/// throws, the handshake failed: `failure` says why.
export class SecureChannel {
  /**
   * @param {object} protocol  the WASM module (see wasm.js)
   * @param {number | {kind: "v1"|"pair"|"known", pin?: number, keys?: object, hostKeys?: string[]}} opts
   *   a bare number is a v1 channel keyed by that PIN (0 = none, still encrypted)
   */
  constructor(protocol, opts) {
    if (typeof opts === "number") opts = { kind: "v1", pin: opts };
    this.protocol = protocol;
    this.kind = opts.kind;
    this.tunnel = null;
    this.reader = new protocol.FrameReader();
    /// Where a v2 handshake's replies go: set it to the socket's `send`.
    this.onSend = null;
    /// Called once when a v2 tunnel opens (after `hostKeyHex`/`paired` are set).
    this.onDone = null;
    /// The host's long-term key once a v2 handshake is done.
    this.hostKeyHex = null;
    /// True when this v2 connection paired with the PIN (not by key).
    this.paired = false;
    if (this.kind === "v1") {
      this.handshake = new protocol.Handshake((opts.pin ?? 0) >>> 0);
      // Handshake reply bytes seen so far. The reply is a u16-LE-prefixed
      // message and may arrive in pieces, so it cannot be handed over until it
      // is whole.
      this.pending = new Uint8Array(0);
    } else if (this.kind === "pair") {
      this.v2 = protocol.PairingHandshake.pair((opts.pin ?? 0) >>> 0, opts.keys);
    } else {
      this.v2 = protocol.PairingHandshake.reconnect(opts.keys, opts.hostKeys ?? []);
    }
  }

  /// The bytes to send before anything else. Sending anything ahead of these
  /// makes the host treat the connection as a legacy plaintext peer.
  get firstMessage() {
    return this.v2 ? this.v2.firstMessage : this.handshake.first_message;
  }

  /// True once the tunnel is live.
  get open() {
    return this.tunnel !== null;
  }

  /// Why a v2 handshake failed: "wrong-code", "host-locked", "not-paired",
  /// "unknown-host", "older-host", "unsupported" — or null.
  get failure() {
    return this.v2?.failure ?? null;
  }

  /// Seconds the host asked us to wait, when `failure` is "host-locked".
  get lockedSeconds() {
    return this.v2?.lockedSeconds ?? 0;
  }

  /// The failure a close before `open` means, for a v2 handshake: "older-host"
  /// before any reply, "wrong-code" after our proof of the PIN. Null for v1,
  /// whose close has always just meant "probably the PIN".
  closedFailure() {
    return this.v2 && !this.open ? this.v2.closedFailure() ?? null : null;
  }

  /**
   * Take one inbound chunk. Returns the whole protocol message bodies it
   * completed — empty while the handshake is still in progress.
   * @param {Uint8Array} bytes
   * @returns {Uint8Array[]}
   */
  receive(bytes) {
    if (!this.tunnel) {
      if (this.v2) {
        // Throws on a failed handshake; `failure` then says why.
        const out = this.v2.feed(bytes);
        if (out.length) this.onSend?.(out);
        if (!this.v2.done) return [];
        this.hostKeyHex = this.v2.hostKeyHex ?? null;
        this.paired = this.v2.paired;
        this.tunnel = this.v2.takeTunnel();
        this.onDone?.();
        // The tunnel may already hold bytes that arrived with the host's
        // verdict (its HostInfo, say).
        return this.drain();
      }
      this.pending = concat(this.pending, bytes);
      // u16-LE length, then the body. Wait for both.
      if (this.pending.length < 2) return [];
      const len = this.pending[0] | (this.pending[1] << 8);
      if (this.pending.length < 2 + len) return [];
      // ⚠️ A wrong PIN fails HERE, as an authentication failure — the PIN keys
      // the AEAD, so there is no distinguishable "bad PIN" reply to look for.
      this.tunnel = this.handshake.finish(this.pending.subarray(0, 2 + len));
      const rest = this.pending.subarray(2 + len);
      this.pending = new Uint8Array(0);
      return rest.length ? this.receive(rest) : [];
    }

    this.tunnel.feed(bytes);
    return this.drain();
  }

  drain() {
    const plain = this.tunnel.take();
    if (plain.length) this.reader.push(plain);
    const out = [];
    for (;;) {
      const body = this.reader.next();
      if (body === undefined) break;
      out.push(body);
    }
    return out;
  }

  /**
   * Frame and encrypt one protocol message body for sending.
   * @param {Uint8Array} body
   * @returns {Uint8Array}
   */
  seal(body) {
    if (!this.tunnel) throw new Error("the tunnel is not open yet");
    return this.tunnel.seal(this.protocol.frame(body));
  }
}

/**
 * Decide and build the channel for a host, from what it said it can do, and
 * wire up what to remember when a v2 handshake succeeds.
 *
 * Returns a `SecureChannel`, `null` for plaintext (an older host), or the
 * string `"refuse"` when a host that spoke v2 before now offers less — see
 * `pairing.plan`.
 *
 * @param {object} protocol  the WASM module
 * @param {{hostHandshake: number, pin: number, target: string}} opts
 */
export function openChannel(protocol, { hostHandshake, pin, target }) {
  const trusted = pairing.trustedHosts();
  const choice = pairing.plan({ hostHandshake, pin, trusted, pinned: pairing.isPinnedV2(target) });
  if (choice === "refuse") return "refuse";
  if (choice === "plain") return null;
  if (choice === "v1") return new SecureChannel(protocol, { kind: "v1", pin });
  const channel = new SecureChannel(protocol, { kind: choice, pin, keys: pairing.keys(protocol), hostKeys: trusted });
  channel.onDone = () => {
    pairing.markV2(target);
    // A pairing over PIN 0 (a host that lets anyone in) proves nothing about
    // who answered, so it earns no trust.
    if (channel.paired && pin) pairing.rememberHost(channel.hostKeyHex);
  };
  return channel;
}

/// What to tell a person about a failed connection, by failure code.
export function describeFailure(code, lockedSeconds = 0) {
  switch (code) {
    case "wrong-code":
      return "The PIN didn't match the one the host shows.";
    case "host-locked":
      return `The host is pausing after too many wrong PINs. Try again in ${humanWait(lockedSeconds)}.`;
    case "not-paired":
    case "unknown-host":
      return "This browser isn't paired with that computer yet, or the computer forgot it. Enter the PIN it shows and connect again.";
    case "older-host":
      return "That computer runs an older Universal Screens that can't pair with this page. Update it, then connect again.";
    case "unsupported":
      return "That computer runs a newer Universal Screens. Reload this page, then connect again.";
    case "downgrade":
      return "This computer used the newer, safer pairing with this browser before, and now offers only the older kind. Not connecting: someone may be interfering with the connection, or the computer was given an older Universal Screens.";
    default:
      return null;
  }
}

function humanWait(seconds) {
  if (seconds >= 90) return `${Math.ceil(seconds / 60)} minutes`;
  return seconds === 1 ? "1 second" : `${Math.max(1, seconds)} seconds`;
}

function concat(a, b) {
  const out = new Uint8Array(a.length + b.length);
  out.set(a, 0);
  out.set(b, a.length);
  return out;
}

/// The subprotocol a tab offers to ask a LAN bridge "do you relay an encrypted
/// connection verbatim?". Must match `E2EE_SUBPROTOCOL` in crates/web-bridge.
///
/// ⚠️ An old bridge does not echo it, which is exactly the point: the tab learns
/// the answer from the handshake, with no timeout and no reconnect. It cannot be
/// used on the room path, where the WebSocket terminates at Cloudflare rather
/// than at the host.
export const E2EE_SUBPROTOCOL = "usscreens-e2ee.v1";

/// "…and the host behind me pairs with SPAKE2." Offered first; a v0.3 bridge
/// does not know it and answers v1. Must match `E2EE_SUBPROTOCOL_V2` in
/// crates/web-bridge.
export const E2EE_SUBPROTOCOL_V2 = "usscreens-e2ee.v2";

/// How long a tab waits, after pairing in a rendezvous room, for the host to
/// announce that it can relay verbatim.
///
/// Only ever paid against an **older host**, which never announces: a current
/// one sends its signal immediately on pairing, one relay hop away. Long enough
/// to survive a slow round trip, short enough not to look like a hang.
export const CAPS_WAIT_MS = 1500;
