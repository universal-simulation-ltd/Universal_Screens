// WebSocket transport to the host (via crates/web-bridge). See
// docs/M7-browser-client.md.
//
// Two shapes on one socket, chosen by the handshake:
//
// - **Encrypted** (the bridge echoed the E2EE subprotocol): the tab runs a
//   PIN-keyed Noise tunnel to the host itself, and the bridge relays bytes
//   verbatim. Framing is the tab's job — see secure.js.
// - **Plaintext** (an older bridge, which doesn't know the subprotocol): each WS
//   binary message is one bare `postcard` body and the bridge adds the length
//   prefix, exactly as before.
//
// ⚠️ The fall-back is not a nicety. The bridge ships INSIDE the host binary, so
// the machine on the other end may be a year old while this page is minutes old.
import { protocol } from "./wasm.js";
import { SecureChannel, E2EE_SUBPROTOCOL } from "./secure.js";

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
  }

  /**
   * Open the socket. `pin` keys the tunnel and must be the same PIN the hello
   * will carry.
   * @param {number} pin
   */
  connect(pin = 0) {
    this.pin = Number(pin) || 0;
    const query = this.targetHost ? `?host=${encodeURIComponent(this.targetHost)}` : "";
    // Offering the subprotocol is the whole negotiation: a bridge that can relay
    // an encrypted connection echoes it, an older one ignores it, and
    // `ws.protocol` tells us which happened before we send a byte.
    this.ws = new WebSocket(`ws://${this.addr}/${query}`, [E2EE_SUBPROTOCOL]);
    this.ws.binaryType = "arraybuffer";
    this.ws.onopen = () => {
      if (this.ws.protocol === E2EE_SUBPROTOCOL) {
        this.secure = new SecureChannel(protocol, this.pin);
        this.encrypted = true;
        this.ws.send(this.secure.firstMessage);
      }
      this.onOpen?.();
    };
    this.ws.onclose = () => this.onClose?.();
    this.ws.onerror = (e) => this.onError?.(e);
    this.ws.onmessage = (ev) => {
      try {
        const bytes = new Uint8Array(ev.data);
        if (!this.secure) {
          this.onMessage?.(protocol.decode_message(bytes));
          return;
        }
        for (const body of this.secure.receive(bytes)) {
          this.onMessage?.(protocol.decode_message(body));
        }
      } catch (e) {
        this.onError?.(e);
      }
    };
  }

  /// Send the first upstream message. `encode` is the WASM `protocol` object
  /// (kept as a parameter so this shares a signature with `RoomTransport`).
  /// `captureMode` is the u8 code (0 extend / 1 mirror / 2 control-only);
  /// platform is fixed to 0 (browser).
  sendHello(encode, { width, height, captureMode, pin }) {
    const p = Number(pin) || 0;
    if (this.secure && p !== this.pin) {
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
    // message. Dropping is right: the only thing sent this early is the hello,
    // and `sendHelloAndAttach` is called from `onPaired`/`onOpen` — see
    // `whenReady`.
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
