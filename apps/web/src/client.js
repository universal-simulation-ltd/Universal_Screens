// Browser client — connect screen (mobile-app parity: icon, wordmark, saved
// connections, glyph/label/blurb mode rows) → session screen (canvas + controls).
// Pipeline: Transport (WebSocket) → H264Decoder (WebCodecs) → CanvasRenderer,
// with InputController forwarding mouse/keyboard/touch/gestures.
import { ready, protocol } from "./wasm.js";
import { Transport } from "./transport.js";
import { H264Decoder } from "./decoder.js";
import { CanvasRenderer } from "./renderer.js";
import { InputController } from "./input.js";
import * as saved from "./saved.js";

const $ = (id) => document.getElementById(id);

// The three ways to use it, mirroring the mobile apps' mode picker. `capture` is
// the protocol CaptureMode code; `video` whether the host streams; `input`
// whether we forward control. Remote control leads (the "browser as a computer"
// headline), then view-only mirror, then the clicker.
const MODES = [
  { id: "control", glyph: "🕹️", label: "Remote control", blurb: "See the screen and control it (mouse + keys)", capture: 1, video: true, input: true },
  { id: "viewer", glyph: "🪞", label: "Mirror screen", blurb: "Watch the host's screen (view only)", capture: 1, video: true, input: false },
  { id: "clicker", glyph: "👆", label: "Clicker", blurb: "Presentation remote — arrows / Page keys drive slides", capture: 2, video: false, input: true },
];

let transport = null;
let decoder = null;
let renderer = null;
let input = null;
let activeMode = null;
let activeAddr = "";

function log(msg, cls = "") {
  const el = $("log");
  if (!el) return;
  const t = new Date().toISOString().slice(11, 23);
  const line = document.createElement("div");
  if (cls) line.className = cls;
  line.textContent = `${t}  ${msg}`;
  el.appendChild(line);
  el.scrollTop = el.scrollHeight;
}

const status = (msg, cls = "") => { const el = $("connect-status"); el.textContent = msg; el.className = cls; };

// ---- connect screen --------------------------------------------------------

function renderModes() {
  const box = $("modes");
  box.innerHTML = "";
  for (const m of MODES) {
    const btn = document.createElement("button");
    btn.className = "row-btn";
    btn.innerHTML = `<span class="glyph">${m.glyph}</span><span class="body"><div class="title">${m.label}</div><div class="blurb">${m.blurb}</div></span>`;
    btn.addEventListener("click", () => connect($("addr").value.trim(), m));
    box.appendChild(btn);
  }
}

const DEVICE_GLYPH = { windows: "🪟", macos: "🍎", linux: "🐧", android: "🤖", ios: "📱" };

function renderSaved() {
  const list = saved.load();
  $("saved-section").hidden = list.length === 0;
  const box = $("saved-list");
  box.innerHTML = "";
  for (const h of list) {
    const row = document.createElement("button");
    row.className = "row-btn";
    // Title: the user's friendly name with the host in brackets, e.g.
    // "Office Mac (Kyjams-iMac)"; else just the hostname (or address).
    const base = h.hostname || h.addr;
    const cn = (h.customName ?? "").trim();
    const title = cn ? `${cn} (${base})` : base;
    row.innerHTML = `<span class="glyph">${DEVICE_GLYPH[h.os] ?? "🖥️"}</span><span class="body"><div class="title">${title}</div><div class="blurb">${h.addr}</div></span><span class="ren" title="Rename">✎</span><span class="del" title="Forget">×</span>`;
    row.addEventListener("click", (e) => {
      if (e.target.classList.contains("del")) { saved.remove(h.addr); renderSaved(); return; }
      if (e.target.classList.contains("ren")) {
        const next = prompt("Friendly name for this host (leave blank to reset to its device name):", cn);
        if (next !== null) { saved.setCustomName(h.addr, next); renderSaved(); }
        return;
      }
      connect(h.addr, MODES[0]); // reconnect in Remote control
    });
    box.appendChild(row);
  }
}

// ---- session ---------------------------------------------------------------

async function connect(addr, mode) {
  if (!addr) { status("Enter a bridge host:port first.", "err"); return; }
  await ready();
  activeMode = mode;
  activeAddr = addr;

  document.body.classList.add("in-session");
  $("host-label").textContent = `Connecting to ${addr}…`;
  $("btn-lock").hidden = !(mode.video && mode.input);
  $("video-hint").style.display = mode.video ? "none" : "flex";
  $("video-hint").textContent = mode.video ? "" : "Clicker mode — no video. Use your keyboard (arrows / Page Up·Down, F5, Esc).";
  $("log").innerHTML = "";

  const canvas = $("screen");
  renderer = new CanvasRenderer(canvas);
  decoder = new H264Decoder((frame) => renderer.draw(frame), (e) => log(`decoder error: ${e}`, "err"));
  transport = new Transport(addr);
  input = new InputController(transport, renderer, canvas);

  transport.onOpen = () => {
    saved.touch(addr, Date.now());
    const dpr = window.devicePixelRatio || 1;
    transport.sendHello({
      width: Math.round(window.screen.width * dpr),
      height: Math.round(window.screen.height * dpr),
      captureMode: mode.capture,
      pin: Number($("pin").value) || 0,
    });
    $("host-label").textContent = addr;
    input.setMode({ enabled: mode.input, pointerInput: mode.video && mode.input });
    input.attach();
    log(`connected — ${mode.label} (protocol v${protocol.protocol_version()})`, "ok");
  };
  transport.onClose = () => { log("host disconnected", "dim"); disconnect(); };
  transport.onError = (e) => log(`error: ${e?.message ?? e} (is the bridge running?)`, "err");
  transport.onMessage = onMessage;
  transport.connect();
}

function onMessage(m) {
  switch (m.kind) {
    case "StreamStart":
      try {
        const codec = decoder.configureFromStreamStart(m);
        log(`stream ${m.width}×${m.height} ${m.codec} → decoder ready (${codec})`, "ok");
      } catch (e) { log(`cannot configure decoder: ${e}`, "err"); }
      break;
    case "Frame":
      decoder.decodeFrame(m);
      break;
    case "HostInfo":
      saved.label(activeAddr, m.name, m.os);
      $("host-label").textContent = m.name ? `${m.name} (${m.os})` : activeAddr;
      break;
    case "WindowList": log(`window list: ${m.window_count} window(s)`, "dim"); break;
    case "Snapshot": log(`snapshot slot ${m.slot}`, "dim"); break;
    default: break;
  }
}

function disconnect() {
  input?.detach();
  decoder?.close();
  transport?.close();
  transport = decoder = renderer = input = null;
  if (document.fullscreenElement) document.exitFullscreen?.();
  document.body.classList.remove("in-session");
  renderSaved();
}

// ---- decode self-test (host-independent; see M7c) --------------------------

function parseAvcc(d) {
  let o = 5;
  const numSps = d[o++] & 0x1f;
  let sps;
  for (let i = 0; i < numSps; i++) { const len = (d[o] << 8) | d[o + 1]; o += 2; if (i === 0) sps = d.slice(o, o + len); o += len; }
  const numPps = d[o++];
  let pps;
  for (let i = 0; i < numPps; i++) { const len = (d[o] << 8) | d[o + 1]; o += 2; if (i === 0) pps = d.slice(o, o + len); o += len; }
  return { sps, pps };
}

export async function runDecodePipelineSelfTest() {
  await ready();
  if (!("VideoEncoder" in window) || !("VideoDecoder" in window)) {
    return { ok: false, reason: "WebCodecs unavailable in this browser" };
  }
  const W = 320, H = 240;
  const src = new OffscreenCanvas(W, H);
  const c = src.getContext("2d");
  c.fillStyle = "#0a84ff"; c.fillRect(0, 0, W, H);
  c.fillStyle = "#fff"; c.font = "28px sans-serif"; c.fillText("decode OK", 70, 130);

  let description = null;
  const chunks = [];
  const enc = new VideoEncoder({
    output: (chunk, meta) => { if (meta?.decoderConfig?.description) description = new Uint8Array(meta.decoderConfig.description); chunks.push(chunk); },
    error: (e) => { throw e; },
  });
  enc.configure({ codec: "avc1.42001f", width: W, height: H, avc: { format: "avc" } });
  const srcFrame = new VideoFrame(src, { timestamp: 0 });
  enc.encode(srcFrame, { keyFrame: true });
  srcFrame.close();
  await enc.flush();
  enc.close();
  if (!description || chunks.length === 0) return { ok: false, reason: "encoder produced no avcC config" };

  const { sps, pps } = parseAvcc(description);
  const codec = protocol.avc_codec_string(sps);
  const rebuilt = protocol.avcc_description(sps, pps);
  const decoded = await new Promise((resolve, reject) => {
    const out = [];
    const dec = new VideoDecoder({ output: (f) => out.push(f), error: reject });
    dec.configure({ codec, description: rebuilt, optimizeForLatency: true });
    const buf = new Uint8Array(chunks[0].byteLength);
    chunks[0].copyTo(buf);
    dec.decode(new EncodedVideoChunk({ type: "key", timestamp: 0, data: buf }));
    dec.flush().then(() => resolve(out)).catch(reject);
  });
  if (decoded.length === 0) return { ok: false, reason: "decoder produced no frame" };
  const f = decoded[0];
  const result = { ok: f.displayWidth === W && f.displayHeight === H, codec, frameSize: [f.displayWidth, f.displayHeight] };
  f.close();
  return result;
}

// ---- boot ------------------------------------------------------------------

export function boot() {
  renderModes();
  renderSaved();

  $("saved-toggle").addEventListener("click", () => {
    const t = $("saved-toggle"), l = $("saved-list");
    const open = l.hidden;
    l.hidden = !open;
    t.classList.toggle("open", open);
  });
  $("btn-disconnect").addEventListener("click", disconnect);
  $("btn-fullscreen").addEventListener("click", () => {
    if (document.fullscreenElement) document.exitFullscreen?.();
    else $("session-view").requestFullscreen?.();
  });
  $("btn-lock").addEventListener("click", () => input?.lockPointer());
  $("selftest").addEventListener("click", async () => {
    status("running decode self-test…");
    try {
      const r = await runDecodePipelineSelfTest();
      status(r.ok ? `self-test passed — decoded ${r.frameSize.join("×")} via ${r.codec}` : `self-test failed — ${r.reason}`, r.ok ? "ok" : "err");
    } catch (e) { status(`self-test error: ${e?.message ?? e}`, "err"); }
  });

  ready()
    .then(() => status(`Ready (protocol v${protocol.protocol_version()}). Start the bridge, then pick a mode.`))
    .catch((e) => status(`WASM init failed: ${e}`, "err"));

  window.runDecodePipelineSelfTest = runDecodePipelineSelfTest;
}
