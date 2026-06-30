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

export class RoomTransport {
  /**
   * @param {string} roomBase  rendezvous origin, e.g. "wss://opensource.unisim.co.uk"
   * @param {string} code      the receiver's pairing code
   * @param {(bytes: Uint8Array) => any} decode  postcard Message decoder
   */
  constructor(roomBase, code, decode) {
    this.url = `${roomBase.replace(/\/$/, "")}/screens/room?code=${encodeURIComponent(code)}&role=receiver`;
    this.decode = decode;
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

  connect() {
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
          case "paired": this.paired = true; this.onPaired?.(sig.peerRole ?? null); break;
          case "peer-left": this.paired = false; this.onPeerLeft?.(); break;
          default: break;
        }
        return;
      }
      try {
        this.onMessage?.(this.decode(new Uint8Array(ev.data)));
      } catch (e) {
        this.onError?.(e);
      }
    };
  }

  /** Send raw upstream bytes (a `protocol.encode_*` result: ClientHello / Input). */
  send(bytes) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) this.ws.send(bytes);
  }

  get connected() {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  close() {
    this.ws?.close();
  }
}
