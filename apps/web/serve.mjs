// Tiny static dev server for the browser client — serves this directory with the
// MIME types WebCodecs/WASM need (notably application/wasm). No deps.
// Run: node apps/web/serve.mjs [port]   (default 8791)
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL(".", import.meta.url));
const PORT = Number(process.argv[2]) || 8791;

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
  ".json": "application/json",
  ".css": "text/css; charset=utf-8",
  ".map": "application/json",
};

createServer(async (req, res) => {
  try {
    const url = decodeURIComponent(req.url.split("?")[0]);
    let rel = normalize(url).replace(/^(\.\.[/\\])+/, "");
    if (rel === "/" || rel === "\\" || rel === "") rel = "index.html";
    const path = join(ROOT, rel);
    if (!path.startsWith(ROOT)) { res.writeHead(403).end("forbidden"); return; }
    const body = await readFile(path);
    res.writeHead(200, { "content-type": MIME[extname(path)] ?? "application/octet-stream" });
    res.end(body);
  } catch {
    res.writeHead(404).end("not found");
  }
}).listen(PORT, () => console.log(`browser client on http://localhost:${PORT}`));
