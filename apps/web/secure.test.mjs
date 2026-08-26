// End-to-end proof of the ENCRYPTED browser leg, with the real browser code in
// the chain: this file drives `src/secure.js` and the wasm-pack artifact over a
// real WebSocket, through the real bridge, to the shipped `transport::accept`.
//
// The Rust tests prove every pair of those; only this proves the whole chain
// including the JavaScript. It is also the only place the WASM `Handshake` /
// `Tunnel` bindings are exercised as a browser would use them.
//
// Run (from the repo root, after building the shim):
//   wasm-pack build crates/protocol-wasm --dev --target web \
//     --out-dir ../../apps/web/pkg --out-name extender_protocol
//   node apps/web/secure.test.mjs
//
// Node 22+ (global WebSocket). Spawns `cargo run --example e2ee_testbed`, so the
// first run pays a compile.

import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import init, * as protocol from "./pkg/extender_protocol.js";
import { SecureChannel, E2EE_SUBPROTOCOL } from "./src/secure.js";

const PIN = 4321;
const REPO = fileURLToPath(new URL("../../", import.meta.url));

let failures = 0;
const A = (cond, msg) => {
  console.log((cond ? "PASS" : "FAIL") + ": " + msg);
  if (!cond) failures++;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const eq = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);

await init({ module_or_path: await readFile(new URL("./pkg/extender_protocol_bg.wasm", import.meta.url)) });

// --- the testbed: a real bridge in front of a real transport::accept ---------
// ⚠️ No `shell: true`. It runs, but it makes the child a *shell* wrapping cargo,
// so `kill()` reaches the shell and leaves the testbed alive — verified: two
// orphaned `e2ee_testbed` processes after two runs, each still holding a port.
// Node also warns that arguments are concatenated rather than escaped. `cargo`
// is a real executable on PATH, so spawn finds it without a shell.
const child = spawn(
  "cargo",
  ["run", "--quiet", "-p", "extender-web-bridge", "--example", "e2ee_testbed", "--", String(PIN)],
  { cwd: REPO, stdio: ["ignore", "pipe", "inherit"] },
);
process.on("exit", () => child.kill());

const wsPort = await new Promise((resolve, reject) => {
  let buf = "";
  const timer = setTimeout(() => reject(new Error("the testbed never printed WS_PORT")), 180000);
  child.stdout.on("data", (d) => {
    buf += d.toString();
    const m = buf.match(/WS_PORT=(\d+)/);
    if (m) {
      clearTimeout(timer);
      resolve(Number(m[1]));
    }
  });
  child.on("exit", (code) => reject(new Error(`testbed exited early (${code})`)));
});

/// Open one session, exactly as `src/transport.js` does.
function connect({ offerE2ee = true, pin = PIN } = {}) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`ws://127.0.0.1:${wsPort}/`, offerE2ee ? [E2EE_SUBPROTOCOL] : []);
    ws.binaryType = "arraybuffer";
    const inbox = [];
    let secure = null;
    ws.onerror = (e) => reject(e);
    ws.onopen = () => {
      if (ws.protocol === E2EE_SUBPROTOCOL) {
        secure = new SecureChannel(protocol, pin);
        ws.send(secure.firstMessage);
      }
      resolve({
        ws,
        get encrypted() { return secure !== null; },
        get open() { return secure ? secure.open : true; },
        inbox,
        send(body) {
          ws.send(secure ? secure.seal(body) : body);
        },
      });
    };
    ws.onmessage = (ev) => {
      const bytes = new Uint8Array(ev.data);
      if (!secure) { inbox.push(bytes); return; }
      for (const body of secure.receive(bytes)) inbox.push(body);
    };
  });
}

const waitFor = async (fn, ms = 10000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (fn()) return true;
    await sleep(10);
  }
  return false;
};

// --- 1. the negotiation ------------------------------------------------------
const session = await connect();
A(session.encrypted, "the bridge echoed the E2EE subprotocol, so the tab encrypts");
A(await waitFor(() => session.open), "the Noise handshake completed through the bridge");

// --- 2. a real protocol message survives the round trip ----------------------
const hello = protocol.encode_hello(protocol.protocol_version(), 2560, 1440, 1, 0, PIN);
session.send(hello);
A(await waitFor(() => session.inbox.length > 0), "the host echoed a message back through the tunnel");
A(eq(session.inbox[0], hello), "and it is byte-identical to what was sent");

// --- 3. the framing the bridge stopped doing ---------------------------------
// ⚠️ The case that a single small message would never catch: a payload far
// larger than one Noise record, which must be split, reassembled and de-framed.
const big = new Uint8Array(200_000);
for (let i = 0; i < big.length; i++) big[i] = i & 0xff;
session.inbox.length = 0;
session.send(big);
A(await waitFor(() => session.inbox.length > 0, 20000), "a 200 KB payload came back (many records, many chunks)");
A(session.inbox[0] && eq(session.inbox[0], big), "and it reassembled byte-for-byte");

// --- 4. several messages in flight keep their order and boundaries -----------
session.inbox.length = 0;
const burst = [1, 2, 3, 4, 5].map((n) => new Uint8Array([n, n, n]));
for (const m of burst) session.send(m);
A(await waitFor(() => session.inbox.length === burst.length), "a burst of five messages arrived as five");
A(burst.every((m, i) => session.inbox[i] && eq(session.inbox[i], m)), "in order, each with its own boundary");

// --- 5. the wrong PIN cannot open a tunnel -----------------------------------
const wrong = await connect({ pin: PIN + 1 });
A(wrong.encrypted, "the wrong-PIN session still negotiated encryption");
A(!(await waitFor(() => wrong.open, 3000)), "but the handshake never completes with the wrong PIN");
wrong.ws.close();

// --- 6. a tab that doesn't ask stays plaintext, and still works ---------------
const plain = await connect({ offerE2ee: false });
A(!plain.encrypted, "a tab that offers no subprotocol is not encrypted");
plain.send(hello);
A(await waitFor(() => plain.inbox.length > 0), "and the plaintext path still round-trips (older hosts keep working)");
plain.ws.close();

// ⚠️ Tear down in this order, and do NOT call `process.exit()` here. Killing the
// child and exiting in the same tick aborts Node on Windows inside libuv
// ("!(handle->flags & UV_HANDLE_CLOSING)") — the run printed every PASS and then
// crashed, which any CI would read as a failure. Close the sockets, let the
// child's stdout go, set an exit code, and let Node unwind on its own.
session.ws.close();
child.stdout.destroy();
child.kill();
console.log(failures ? `\n${failures} check(s) failed` : "\nall checks passed");
process.exitCode = failures ? 1 : 0;
