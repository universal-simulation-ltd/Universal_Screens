// M8d browser transport test: RoomTransport (the viewer) pairs with a host over
// the real rendezvous Durable Object and relays postcard frames both ways.
//
// Needs the rendezvous Worker running — from the opensource-portal repo:
//   npx wrangler dev --ip 127.0.0.1 --port 8788
// then:  WS=ws://127.0.0.1:8788 node apps/web/room.test.mjs
//
// Uses Node's global WebSocket (Node 22+). Decode is a passthrough so no WASM.

import { RoomTransport } from "./src/room.js";

const WS = process.env.WS || "ws://127.0.0.1:8788";
const CODE = "V" + Math.floor(Math.random() * 9000 + 1000);
const DOWN = new Uint8Array([1, 2, 3, 4, 250, 0, 99]); // host -> viewer (a "Message")
const UP = new Uint8Array([7, 7, 7, 42]); // viewer -> host (an "Input")

let failures = 0;
const A = (c, m) => { console.log((c ? "PASS" : "FAIL") + ": " + m); if (!c) failures++; };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const eq = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);

// A minimal "host": joins as sender, sends DOWN once paired, records what it gets.
function fakeHost() {
  const ws = new WebSocket(`${WS}/screens/room?code=${CODE}&role=sender`);
  ws.binaryType = "arraybuffer";
  const got = [];
  ws.addEventListener("message", (e) => {
    if (typeof e.data === "string") {
      if (JSON.parse(e.data).type === "paired") ws.send(DOWN); // host streams a frame
    } else {
      got.push(new Uint8Array(e.data)); // an upstream Input from the viewer
    }
  });
  return { ws, got, ready: new Promise((res) => ws.addEventListener("open", res, { once: true })) };
}

try {
  let paired = false;
  const received = [];
  const rt = new RoomTransport(WS, CODE, (bytes) => bytes); // passthrough decode
  rt.onPaired = () => { paired = true; };
  rt.onMessage = (m) => received.push(m);
  rt.connect();
  await sleep(150); // let the viewer join + reach "waiting"

  const host = fakeHost();
  await host.ready;
  await sleep(250); // pair + exchange the DOWN frame

  A(paired, "viewer received 'paired' from the room");
  A(received.length === 1 && eq(received[0], DOWN), "host frame relayed to the viewer + decoded");

  // Upstream: the viewer sends an Input; the host should receive it verbatim.
  rt.send(UP);
  await sleep(200);
  A(host.got.length === 1 && eq(host.got[0], UP), "viewer Input relayed to the host");

  rt.close();
  host.ws.close();
  await sleep(80);
} catch (e) {
  console.log("FAIL: threw — " + e.message);
  failures++;
}

console.log(failures === 0 ? "\nALL PASSED" : `\n${failures} FAILED`);
process.exit(failures ? 1 : 0);
