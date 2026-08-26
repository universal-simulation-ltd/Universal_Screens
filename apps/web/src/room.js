// RoomTransport — the browser viewer's transport for "view a host over the cloud
// rendezvous" (M8d). It is the M7 `Transport` adapted to the rendezvous room:
// the host dials the same room (crates/web-bridge `dial_room`) and the Durable
// Object relays the *existing* `postcard` frames between the two, so the decode /
// render / input pipeline downstream is unchanged.
//
// The only new wrinkle vs M7's direct host WebSocket is that the room interleaves
// JSON *signal* frames (text: {type:"waiting"|"paired"|"peer-left"}) with the
// host's binary `postcard` frames. This class routes text → signal callbacks and
// binary → the decoder.
//
// Decode is injected (not imported) so this stays free of the WASM shim and is
// unit-testable in Node; the real client passes `protocol.decode_message`.
//
// ## Encryption
//
// The relay is a Cloudflare Durable Object that relays whatever it is given -
// so TLS protects the wire to Cloudflare, and NOT the stream from Cloudflare.
// When a `secure` factory is supplied and the host announces that it can relay
// verbatim, this class runs a PIN-keyed Noise tunnel end to end instead and the
// room sees only Noise records.
//
// ⚠️ The negotiation cannot be a WebSocket subprotocol here, the way it is on the
// LAN path: this socket terminates at *Cloudflare*, not at the host, so no
// header sent from here ever reaches the other end. The host announces instead,
// with a `{"type":"caps","e2ee":true}` signal the room relays verbatim - and an
// older host simply never sends one, which is why there is a wait rather than a
// certainty.

export class RoomTransport {
  /**
   * @param {string} roomBase  rendezvous origin, e.g. "wss://opensource.unisim.co.uk"
   * @param {string} code      the receiver's pairing code
   * @param {(bytes: Uint8Array) => any} decode  postcard Message decoder
   * @param {object} [opts]
   * @param {(pin: number) => object} [opts.secure]  builds a SecureChannel; omit
   *   to stay plaintext (the Node room test does, so it needs no WASM)
   * @param {number} [opts.capsWaitMs]  how long to wait for the host's caps
   *   signal after pairing before giving up on encryption
   */
  constructor(roomBase, code, decode, opts = {}) {
    this.url = `${roomBase.replace(/\/$/, "")}/screens/room?code=${encodeURIComponent(code)}&role=receiver`;
    this.decode = decode;
    this.makeSecure = opts.secure ?? null;
    this.capsWaitMs = opts.capsWaitMs ?? 1500;
    this.secure = null;
    this.encrypted = false;
    this.pin = 0;
    /// Set when the host announces it can relay verbatim.
    this.hostSupportsE2ee = false;
    this.ws = null;
    this.paired = false;
    // Callbacks (all optional):
    this.onOpen = null;       // ()    socket open (not yet paired)
    this.onWaiting = null;    // ()    in the room, waiting for the host
    this.onPaired = null;     // (peerRole)  host joined — safe to send hello
    this.onPeerLeft = null;   // ()    host dropped
    this.onMessage = null;    // (DecodedMessage)  a relayed host frame
    this.onClose = null;      // ()
    this.onError = null;      // (err)
  }

  /**
   * Join the room. `pin` keys the tunnel when the host turns out to support one,
   * and must match the PIN the hello will carry.
   * @param {number} pin
   */
  connect(pin = 0) {
    this.pin = Number(pin) || 0;
    this.ws = new WebSocket(this.url);
    this.ws.binaryType = "arraybuffer";
    this.ws.onopen = () => this.onOpen?.();
    this.ws.onclose = () => this.onClose?.();
    this.ws.onerror = (e) => this.onError?.(e);
    this.ws.onmessage = (ev) => {
      // Text → a rendezvous signal; binary → a relayed postcard frame.
      if (typeof ev.data === "string") {
        let sig;
        try { sig = JSON.parse(ev.data); } catch { return; }
        switch (sig?.type) {
          case "waiting": this.onWaiting?.(); break;
          case "paired": this.paired = true; this._onPairedSignal(sig.peerRole ?? null); break;
          case "peer-left": this.paired = false; this.onPeerLeft?.(); break;
          // The host's capability announcement. Unknown to older browsers, which
          // fall through to `default` and ignore it — that is what makes it safe
          // to add to a relay both sides already speak.
          case "caps": this.hostSupportsE2ee = sig.e2ee === true; break;
          default: break;
        }
        return;
      }
      try {
        const bytes = new Uint8Array(ev.data);
        if (!this.secure) {
          this.onMessage?.(this.decode(bytes));
          return;
        }
        for (const body of this.secure.receive(bytes)) {
          this.onMessage?.(this.decode(body));
        }
      } catch (e) {
        this.onError?.(e);
      }
    };
  }

  /** Send raw upstream bytes (a `protocol.encode_*` result: ClientHello / Input). */
  send(bytes) {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
    // Nothing can be sent between starting the tunnel and its reply arriving;
    // `onPaired` only fires once that is settled, so this drops nothing real.
    if (this.secure && !this.secure.open) return;
    this.ws.send(this.secure ? this.secure.seal(bytes) : bytes);
  }

  /**
   * Pairing happened. Decide — and, if encrypting, complete the handshake —
   * *before* telling the client, so `onPaired` means "safe to send the hello"
   * in both modes and the client needs no branch of its own.
   *
   * ⚠️ The wait exists only for older hosts. A current one announces its
   * capability immediately on pairing, one relay hop away; one that never
   * announces costs `capsWaitMs` once, and then the session runs plaintext
   * exactly as it did before.
   */
  async _onPairedSignal(peerRole) {
    if (this.makeSecure) {
      const deadline = Date.now() + this.capsWaitMs;
      while (!this.hostSupportsE2ee && Date.now() < deadline && this.connected) {
        await new Promise((r) => setTimeout(r, 25));
      }
      if (this.hostSupportsE2ee && this.connected) {
        this.secure = this.makeSecure(this.pin);
        this.encrypted = true;
        this.ws.send(this.secure.firstMessage);
        // The host answers through the relay; wait for the tunnel to open so
        // the hello that follows can actually be sealed.
        const hsDeadline = Date.now() + 10000;
        while (!this.secure.open && Date.now() < hsDeadline && this.connected) {
          await new Promise((r) => setTimeout(r, 10));
        }
        if (!this.secure.open) {
          this.onError?.(new Error("the encrypted handshake did not complete"));
          this.close();
          return;
        }
      }
    }
    this.onPaired?.(peerRole);
  }

  /**
   * Send the first upstream message (once paired). Mirrors `Transport.sendHello`
   * so the client treats a LAN bridge and a room the same way; `encode` is the
   * WASM `protocol` object (injected — this file stays WASM-free for tests).
   * `captureMode` is the u8 code; platform is fixed to 0 (browser).
   */
  sendHello(encode, { width, height, captureMode, pin }) {
    const p = Number(pin) || 0;
    if (this.secure && p !== this.pin) {
      throw new Error(`hello PIN ${p} differs from the PIN this tunnel was keyed with (${this.pin})`);
    }
    this.send(encode.encode_hello(encode.protocol_version(), width, height, captureMode, 0, p));
  }

  get connected() {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  close() {
    this.ws?.close();
  }
}
