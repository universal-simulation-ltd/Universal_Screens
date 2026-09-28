// The browser's reading of the shared knowledge base (knowledge/<lang>.md).
//
// `crates/host-ui/src/knowledge.rs` tests the DATA — every language has the
// same articles, no body strays outside the closed syntax. This tests the
// browser's PARSER against that same data and against the fixtures the Rust
// tests use, so the two readers cannot quietly disagree about the format.
//
// Run: node apps/web/knowledge.test.mjs

import { readFile } from "node:fs/promises";
import { LANGUAGES, UI, detectLanguage, inlineRuns, loadArticles, parseArticleBody, parseBundle } from "./src/knowledge.js";

let failures = 0;
const A = (cond, msg) => {
  console.log((cond ? "PASS" : "FAIL") + ": " + msg);
  if (!cond) failures++;
};
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

// loadArticles fetches `../knowledge/<lang>.md` relative to the module; serve
// it from disk.
const fileFetch = async (url) => {
  try {
    const text = await readFile(url, "utf8");
    return { ok: true, status: 200, text: async () => text };
  } catch {
    return { ok: false, status: 404, text: async () => "" };
  }
};

const en = await loadArticles("en", fileFetch);
A(en.length >= 5 && en.length <= 8, `English has 5–8 articles (${en.length})`);
A(en.every((a) => a.id && a.title && a.summary && a.group && a.body.length > 200), "every English article is complete");

for (const { code } of LANGUAGES) {
  const list = await loadArticles(code, fileFetch);
  A(same(list.map((a) => a.id), en.map((a) => a.id)), `${code}: same ids in the same order`);
  A(UI[code] && UI[code].title && UI[code].back, `${code}: reader words exist`);
  let ok = true;
  for (const a of list) {
    const blocks = parseArticleBody(a.body);
    if (!blocks.length) ok = false;
    for (const b of blocks) {
      for (const t of b.kind === "ul" || b.kind === "ol" ? b.items : [b.text]) {
        if (inlineRuns(t).some((r) => r.text.includes("**"))) ok = false;
      }
    }
  }
  A(ok, `${code}: every body parses with no stray **`);
}

A((await loadArticles("xx", fileFetch)) === en, "an unknown language falls back to English");

// The same fixtures as the Rust tests.
A(
  same(parseArticleBody("Intro line\nwraps here.\n\n## Head\n- one\n- two\n  continued\n\n1. first\n2) second\n\nEnd."), [
    { kind: "p", text: "Intro line wraps here." },
    { kind: "h", text: "Head" },
    { kind: "ul", items: ["one", "two continued"] },
    { kind: "ol", items: ["first", "second"] },
    { kind: "p", text: "End." },
  ]),
  "parseArticleBody matches the Rust port",
);
A(
  same(inlineRuns("a **b** c **d**"), [
    { bold: false, text: "a " }, { bold: true, text: "b" }, { bold: false, text: " c " }, { bold: true, text: "d" },
  ]),
  "inlineRuns splits bold runs",
);
A(same(inlineRuns("lone ** here"), [{ bold: false, text: "lone ** here" }]), "a lone ** stays literal");
const crlf = parseBundle("note: not an article\r\n\r\n---\r\nid: a\r\ntitle: T\r\n---\r\nBody\r\n");
A(crlf.length === 1 && crlf[0].id === "a" && crlf[0].body === "Body", "the note is skipped and CRLF is tolerated");

A(detectLanguage(["pt-BR"]) === "pt-BR", "pt-BR detected");
A(detectLanguage(["pt"]) === "pt-PT", "bare pt is Portugal");
A(detectLanguage(["de-AT", "en"]) === "de", "a regional tag finds its language");
A(detectLanguage(["ja", "nl"]) === "en", "nothing matching falls back to English");

console.log(failures ? `\n${failures} FAILED` : "\nall passed");
process.exit(failures ? 1 : 0);
