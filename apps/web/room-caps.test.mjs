// The browser half of the room's encryption negotiation.
//
// `crates/web-bridge` proves the *host* announces `{"type":"caps","e2ee":true}`
// on pairing. This proves what the tab does with that — and, more importantly,
// what it does when the announcement never comes, which is the case that keeps
// every already-installed host working.
//
// ⚠️ No server and no WASM: `globalThis.WebSocket` is stubbed and the secure
// channel is a spy. A real room needs the portal Worker (`room.test.mjs`) and a
// real tunnel needs the bridge (`secure.test.mjs`); this covers the decision
// logic between them, which neither of those can reach deterministically — you
// cannot ask a real host to be an old one.
//
// Run: node apps/web/room-caps.test.mjs

let failures = 0;
const A = (cond, msg) => {
  console.log((cond ? "PASS" : "FAIL") + ": " + msg);
  if (!cond) failures++;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// --- a WebSocket stand-in the test drives by hand ----------------------------
class FakeSocket {
  static last = null;
  constructor(url) {
    this.url = url;
    this.readyState = 1; // OPEN
    this.sent = [];
    this.binaryType = "";
    FakeSocket.last = this;
    queueMicrotask(() => this.onopen?.());
  }
  send(data) {
    this.sent.push(data);
  }
  close() {
    this.readyState = 3;
    this.onclose?.();
  }
  /// Deliver a rendezvous signal (text) from the room.
  signal(obj) {
    this.onmessage?.({ data: JSON.stringify(obj) });
  }
  /// Deliver binary from the peer.
  binary(bytes) {
    this.onmessage?.({ data: bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) });
  }
}
globalThis.WebSocket = FakeSocket;
globalThis.WebSocket.OPEN = 1;

const { RoomTransport } = await import("./src/room.js");

/// A SecureChannel stand-in: opens as soon as it is handed anything.
function fakeSecure(pin) {
  return {
    pin,
    opened: false,
    firstMessage: new Uint8Array([0x55, 0x53, 0x43, 0x52, 0x01, 0xaa]),
    get open() {
      return this.opened;
    },
    receive(_bytes) {
      this.opened = true;
      return [];
    },
    seal(body) {
      // Something recognisably not the plaintext.
      return new Uint8Array([0xff, ...body]);
    },
  };
}

// --- 1. host announces caps -> the tab encrypts ------------------------------
{
  let made = null;
  const rt = new RoomTransport("ws://room.invalid", "ABCD", (b) => b, {
    secure: (pin) => (made = fakeSecure(pin)),
    capsWaitMs: 500,
  });
  let paired = false;
  rt.onPaired = () => { paired = true; };
  rt.connect(4321);
  await sleep(5);

  const ws = FakeSocket.last;
  ws.signal({ type: "caps", e2ee: true });
  ws.signal({ type: "paired", peerRole: "sender" });
  await sleep(30);

  A(made !== null, "a host that announces caps gets an encrypted tunnel");
  A(made?.pin === 4321, "the tunnel is keyed with the PIN passed to connect()");
  A(ws.sent.length === 1 && ws.sent[0][0] === 0x55, "the handshake goes out first, preamble and all");
  A(!paired, "onPaired is withheld until the tunnel is open — nothing may be sent before it");

  ws.binary(new Uint8Array([1, 2, 3])); // the host's handshake reply
  await sleep(30);
  A(paired, "once the tunnel opens, onPaired fires and the hello can go");
  A(rt.encrypted === true, "and the session reports itself as encrypted");

  rt.send(new Uint8Array([7, 7]));
  A(ws.sent.at(-1)[0] === 0xff, "what it sends afterwards is sealed, not plaintext");
}

// --- 2. an older host announces nothing -> plaintext, and it still works ------
{
  let made = null;
  const rt = new RoomTransport("ws://room.invalid", "ABCD", (b) => b, {
    secure: (pin) => (made = fakeSecure(pin)),
    capsWaitMs: 120, // the real 1.5s, shortened so the test is quick
  });
  let paired = false;
  rt.onPaired = () => { paired = true; };
  rt.connect(4321);
  await sleep(5);

  const ws = FakeSocket.last;
  ws.signal({ type: "paired", peerRole: "sender" });
  A(!paired, "the tab waits before deciding — it cannot know yet");

  await sleep(250); // longer than capsWaitMs
  A(made === null, "no announcement means no tunnel");
  A(paired, "but the session still pairs, so an older host keeps working");
  A(rt.encrypted === false, "and it reports itself as NOT encrypted rather than implying otherwise");

  rt.send(new Uint8Array([7, 7]));
  A(ws.sent.at(-1)[0] === 7, "what it sends is the plaintext body, as before");
}

// --- 3. no secure factory at all (the plaintext room test's shape) -----------
{
  const rt = new RoomTransport("ws://room.invalid", "ABCD", (b) => b);
  let paired = false;
  rt.onPaired = () => { paired = true; };
  rt.connect();
  await sleep(5);
  FakeSocket.last.signal({ type: "caps", e2ee: true });
  FakeSocket.last.signal({ type: "paired", peerRole: "sender" });
  await sleep(20);
  A(paired, "with no secure factory the tab pairs immediately, caps or not");
  A(rt.encrypted === false, "and stays plaintext — the caller opted out");
}

console.log(failures ? `\n${failures} check(s) failed` : "\nall checks passed");
process.exitCode = failures ? 1 : 0;
