// WebSocket transport to the host (via crates/web-bridge). Each WS binary
// message is one `postcard` body: downstream → a decoded `Message`, upstream →
// the bytes from a `protocol.encode_*` call. See docs/M7-browser-client.md.
import { protocol } from "./wasm.js";

export class Transport {
  /// `addr` is the bridge `host:port` (it speaks `ws://`). `targetHost`, when
  /// set, asks the bridge to proxy to that discovered host (`ip:port`) instead
  /// of its default — the "Nearby" click path. The bridge refuses targets it
  /// hasn't itself discovered.
  constructor(addr, targetHost = null) {
    this.addr = addr;
    this.targetHost = targetHost;
    this.ws = null;
    this.onMessage = null; // (DecodedMessage) => void
    this.onOpen = null;
    this.onClose = null;
    this.onError = null;
  }

  connect() {
    const query = this.targetHost ? `?host=${encodeURIComponent(this.targetHost)}` : "";
    this.ws = new WebSocket(`ws://${this.addr}/${query}`);
    this.ws.binaryType = "arraybuffer";
    this.ws.onopen = () => this.onOpen?.();
    this.ws.onclose = () => this.onClose?.();
    this.ws.onerror = (e) => this.onError?.(e);
    this.ws.onmessage = (ev) => {
      try {
        this.onMessage?.(protocol.decode_message(new Uint8Array(ev.data)));
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
    this.send(encode.encode_hello(encode.protocol_version(), width, height, captureMode, 0, pin));
  }

  /// Forward raw encoded `Input` bytes (from a `protocol.encode_*` call).
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
