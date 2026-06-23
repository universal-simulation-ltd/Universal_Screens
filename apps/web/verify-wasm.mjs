// M7b verification: load the real wasm-pack artifact in Node and check it against
// the canonical `postcard` bytes printed by
// `cargo test -p extender-web-bridge --test canonical_bytes -- --nocapture`.
// Run: node apps/web/verify-wasm.mjs   (after `wasm-pack build ... --out-dir pkg`)

import { readFile } from "node:fs/promises";
import init, {
  protocol_version, encode_hello, encode_mouse_move,
  decode_message, avc_codec_string, avcc_description,
} from "./pkg/extender_protocol.js";

const wasmBytes = await readFile(new URL("./pkg/extender_protocol_bg.wasm", import.meta.url));
await init({ module_or_path: wasmBytes });

const hex = (u8) => [...u8].map((b) => b.toString(16).padStart(2, "0")).join("");
const fromHex = (h) => Uint8Array.from(h.match(/../g).map((x) => parseInt(x, 16)));

let failures = 0;
function check(name, actual, expected) {
  const ok = JSON.stringify(actual) === JSON.stringify(expected);
  console.log(`${ok ? "ok  " : "FAIL"}  ${name}` + (ok ? "" : `\n        expected ${JSON.stringify(expected)}\n        got      ${JSON.stringify(actual)}`));
  if (!ok) failures++;
}

// Canonical bytes (from the Rust test) the wasm must reproduce / parse.
check("protocol_version", protocol_version(), 10);
check("encode_hello", hex(encode_hello(10, 1920, 1080, 1, 0, 4321)), "0a800fb8080100e121");
check("encode_mouse_move", hex(encode_mouse_move(0.5, 0.5)), "000000003f0000003f");

const ss = decode_message(fromHex("00800fb8080002046742c01f0468ce3c80"));
check("StreamStart.kind", ss.kind, "StreamStart");
check("StreamStart.dims", [ss.width, ss.height], [1920, 1080]);
check("StreamStart.codec", ss.codec, "H264");
check("StreamStart.parameter_set_count", ss.parameter_set_count, 2);
check("StreamStart.parameter_set(0)", hex(ss.parameter_set(0)), "6742c01f");
check("StreamStart.parameter_set(1)", hex(ss.parameter_set(1)), "68ce3c80");

const fr = decode_message(fromHex("01f2c001780104deadbeef"));
check("Frame.kind", fr.kind, "Frame");
check("Frame.keyframe", fr.keyframe, true);
check("Frame.timestamp_micros", fr.timestamp_micros, (12345 * 1_000_000) / 60);
check("Frame.data", hex(fr.data), "deadbeef");

// WebCodecs config helpers derived from the StreamStart SPS/PPS.
check("avc_codec_string", avc_codec_string(fromHex("6742c01f")), "avc1.42c01f");
check("avcc_description", hex(avcc_description(fromHex("6742c01f"), fromHex("68ce3c80"))),
  "0142c01fffe100046742c01f01000468ce3c80");

// Error paths throw (the JS sees the String message).
try { encode_hello(10, 1, 1, 99, 0, 0); check("bad capture mode throws", false, true); }
catch { check("bad capture mode throws", true, true); }

console.log(failures === 0 ? "\nALL OK" : `\n${failures} FAILURE(S)`);
process.exit(failures === 0 ? 0 : 1);
