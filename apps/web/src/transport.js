// WebSocket transport to the host (via crates/web-bridge). See
// docs/M7-browser-client.md and docs/M10-transport-encryption.md.
//
// Three shapes on one socket, chosen by the handshake:
//
// - **v2** (the bridge echoed `usscreens-e2ee.v2`): the tab pairs with the host
//   itself — SPAKE2 over the PIN, then Noise — or reconnects by remembered key
//   with no PIN. The bridge relays bytes verbatim. Framing is the tab's job —
//   see secure.js.
// - **v1** (a v0.3 bridge, which echoes only `usscreens-e2ee.v1`): the older
//   PIN-keyed tunnel, the best that host can do. Refused for a host this tab has
//   seen speak v2 (see `pairing.plan`).
// - **Plaintext** (an older bridge still): each WS binary message is one bare
//   `postcard` body and the bridge adds the length prefix, exactly as before.
//
// ⚠️ The fall-back is not a nicety. The bridge ships INSIDE the host binary, so
// the machine on the other end may be a year old while this page is minutes old.
import { protocol } from "./wasm.js";
import { openChannel, E2EE_SUBPROTOCOL, E2EE_SUBPROTOCOL_V2 } from "./secure.js";

export class Transport {
  /// `addr` is the bridge `host:port` (it speaks `ws://`). `targetHost`, when
  /// set, asks the bridge to proxy to that discovered host (`ip:port`) instead
  /// of its default — the "Nearby" click path. The bridge refuses targets it
  /// hasn't itself discovered.
  constructor(addr, targetHost = null) {
    this.addr = addr;
    this.targetHost = targetHost;
    this.ws = null;
    this.secure = null; // a SecureChannel once the bridge agreed to encrypt
    this.onMessage = null; // (DecodedMessage) => void
    this.onOpen = null;
    this.onClose = null;
    this.onError = null;
    /// Set once the socket is open: true when this session is end-to-end
    /// encrypted to the host. The UI says so, because "encrypted" is a promise
    /// worth being exact about.
    this.encrypted = false;
    /// The PIN this session's tunnel is keyed with.
    ///
    /// ⚠️ It has to be known at `connect()`, not at `sendHello()`: the handshake
    /// starts the moment the socket opens, which is before any hello exists. The
    /// hello carries the PIN as well and the host checks both — the tunnel binds
    /// it cryptographically, the hello check is the older gate kept on top.
    this.pin = 0;
    /// What the host can do: 2 = pairs with SPAKE2, 1 = v0.3 (v1 tunnel), 0 = older.
    this.handshake = 0;
    /// Why the attempt failed, when it did and we know: a `describeFailure` code.
    this.failure = null;
    /// Seconds to wait when `failure` is "host-locked".
    this.lockedSeconds = 0;
  }

  /**
   * Open the socket. `pin` is what the user typed (0 = none): it pairs with a
   * current host, keys the v1 tunnel for an older one, and rides in the hello.
   *
   * `onOpen` fires once it is safe to send the hello — immediately for
   * plaintext, and only after the handshake for an encrypted session.
   * ⚠️ It used to fire straight after the first handshake message went out,
   * when `send()` still had to drop everything — so on the LAN path the
   * encrypted session's hello was silently discarded and the host timed out.
   * @param {number} pin
   */
  connect(pin = 0) {
    this.pin = Number(pin) || 0;
    this.failure = null;
    this.lockedSeconds = 0;
    this.handshake = 0;
    const target = this.targetHost ?? this.addr;
    const query = this.targetHost ? `?host=${encodeURIComponent(this.targetHost)}` : "";
    // Offering the subprotocols is the whole negotiation: a current bridge
    // answers v2, a v0.3 one v1, an older one nothing — and `ws.protocol` tells
    // us which happened before we send a byte.
    this.ws = new WebSocket(`ws://${this.addr}/${query}`, [E2EE_SUBPROTOCOL_V2, E2EE_SUBPROTOCOL]);
    this.ws.binaryType = "arraybuffer";
    this.ws.onopen = () => {
      this.handshake =
        this.ws.protocol === E2EE_SUBPROTOCOL_V2 ? 2 : this.ws.protocol === E2EE_SUBPROTOCOL ? 1 : 0;
      const channel = openChannel(protocol, { hostHandshake: this.handshake, pin: this.pin, target });
      if (channel === "refuse") {
        this.failure = "downgrade";
        this.ws.close();
        return;
      }
      if (!channel) {
        this.onOpen?.();
        return;
      }
      this.secure = channel;
      this.encrypted = true;
      channel.onSend = (bytes) => this.ws.send(bytes);
      this.ws.send(channel.firstMessage);
    };
    this.ws.onclose = () => {
      if (this.secure && !this.secure.open && !this.failure) {
        this.failure = this.secure.closedFailure();
      }
      this.onClose?.();
    };
    this.ws.onerror = (e) => this.onError?.(e);
    this.ws.onmessage = (ev) => {
      const bytes = new Uint8Array(ev.data);
      if (!this.secure) {
        try {
          this.onMessage?.(protocol.decode_message(bytes));
        } catch (e) {
          this.onError?.(e);
        }
        return;
      }
      const wasOpen = this.secure.open;
      let bodies;
      try {
        bodies = this.secure.receive(bytes);
      } catch (e) {
        // The handshake failed. Say why if we can, and end the attempt.
        this.failure = this.secure.failure ?? "handshake";
        this.lockedSeconds = this.secure.lockedSeconds;
        this.onError?.(e);
        this.ws.close();
        return;
      }
      if (!wasOpen && this.secure.open) this.onOpen?.();
      for (const body of bodies) {
        try {
          this.onMessage?.(protocol.decode_message(body));
        } catch (e) {
          this.onError?.(e);
        }
      }
    };
  }

  /// Send the first upstream message. `encode` is the WASM `protocol` object
  /// (kept as a parameter so this shares a signature with `RoomTransport`).
  /// `captureMode` is the u8 code (0 extend / 1 mirror / 2 control-only);
  /// platform is fixed to 0 (browser).
  sendHello(encode, { width, height, captureMode, pin }) {
    const p = Number(pin) || 0;
    if (this.secure?.kind === "v1" && p !== this.pin) {
      // Keying the tunnel with one PIN and announcing another would fail as an
      // unreadable handshake, which is a maddening thing to debug. Say it here.
      throw new Error(`hello PIN ${p} differs from the PIN this tunnel was keyed with (${this.pin})`);
    }
    this.send(encode.encode_hello(encode.protocol_version(), width, height, captureMode, 0, p));
  }

  /// Forward raw encoded `Input` bytes (from a `protocol.encode_*` call).
  send(bytes) {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
    // ⚠️ Before the tunnel finishes its handshake there is nowhere to put a
    // message. Nothing real is sent this early: `onOpen` waits for the tunnel.
    if (this.secure && !this.secure.open) return;
    this.ws.send(this.secure ? this.secure.seal(bytes) : bytes);
  }

  /// Resolve once it is safe to send — immediately when plaintext, or after the
  /// tunnel's handshake completes.
  async whenReady() {
    if (!this.secure) return;
    while (!this.secure.open) {
      if (!this.connected) throw new Error("connection closed during the handshake");
      await new Promise((r) => setTimeout(r, 10));
    }
  }

  get connected() {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  close() {
    this.ws?.close();
  }
}
