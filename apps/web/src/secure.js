// The encrypted browser leg: a PIN-keyed Noise tunnel between this tab and the
// host, so neither the LAN bridge nor the cloud relay can read the mirrored
// screen or the keystrokes going back.
//
// The cryptography is the same Rust the native clients use, compiled to WASM
// (`crates/protocol-wasm/src/tunnel.rs` over `crates/transport/src/session.rs`).
// This file is the plumbing around it.
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

/// Wraps a WebSocket in the Noise tunnel. Feed it what arrives, ask it for
/// whole protocol messages back.
///
/// Lifecycle: `new` → send `firstMessage` → push every inbound chunk through
/// `receive()` → once `open` is true, `seal()` what you send and read whole
/// message bodies from what `receive()` returns.
export class SecureChannel {
  /**
   * @param {object} protocol  the WASM module (see wasm.js)
   * @param {number} pin       the 4-digit pairing PIN (0 = none, still encrypted)
   */
  constructor(protocol, pin) {
    this.protocol = protocol;
    this.handshake = new protocol.Handshake(pin >>> 0);
    this.tunnel = null;
    this.reader = new protocol.FrameReader();
    // Handshake reply bytes seen so far. The reply is a u16-LE-prefixed message
    // and may arrive in pieces, so it cannot be handed over until it is whole.
    this.pending = new Uint8Array(0);
  }

  /// The bytes to send before anything else. Sending anything ahead of these
  /// makes the host treat the connection as a legacy plaintext peer.
  get firstMessage() {
    return this.handshake.first_message;
  }

  /// True once the tunnel is live.
  get open() {
    return this.tunnel !== null;
  }

  /**
   * Take one inbound chunk. Returns the whole protocol message bodies it
   * completed — empty while the handshake is still in progress.
   * @param {Uint8Array} bytes
   * @returns {Uint8Array[]}
   */
  receive(bytes) {
    if (!this.tunnel) {
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

/// How long a tab waits, after pairing in a rendezvous room, for the host to
/// announce that it can relay verbatim.
///
/// Only ever paid against an **older host**, which never announces: a current
/// one sends its signal immediately on pairing, one relay hop away. Long enough
/// to survive a slow round trip, short enough not to look like a hang.
export const CAPS_WAIT_MS = 1500;
