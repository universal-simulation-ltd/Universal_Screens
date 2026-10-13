// The knowledge base, shaped for universal-platform's guide-PDF printer
// (packages/sdk/scripts/kb-pdfs.mjs), which expects a module exporting
// `loadArticles(language)` with each article's `sources` attached — what the
// React apps' src/knowledge/index.ts export. Screens' articles are Markdown
// (apps/web/knowledge/<lang>.md) and its sources are knowledge/sources.md, so
// this reads them with the browser client's own parsers: a PDF can never say
// something the readers do not.
//
// From universal-platform, after `npm run build -w packages/sdk`:
//   node packages/sdk/scripts/kb-pdfs.mjs ../../Universal_Apps/Universal_Screens screens \
//     --entry=../../Universal_Apps/Universal_Screens/scripts/kb-pdfs-adapter.mjs
// Then commit the PDFs (opensource-portal/public/kb/screens/) to the portal.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { LANGUAGES, parseBundle, parseSources } from "../apps/web/src/knowledge.js";

// kb-pdfs bundles this file, so locate the knowledge folder from the
// environment it sets, falling back to this file's own place in the repo.
const here = dirname(fileURLToPath(import.meta.url));
const dir = process.env.KB_PDFS_APP_DIR
  ? join(process.env.KB_PDFS_APP_DIR, "apps/web/knowledge")
  : join(here, "../apps/web/knowledge");

export async function loadArticles(language) {
  const lang = LANGUAGES.some((l) => l.code === language) ? language : "en";
  const sources = parseSources(readFileSync(join(dir, "sources.md"), "utf8"));
  return parseBundle(readFileSync(join(dir, `${lang}.md`), "utf8"))
    .map((a) => ({ ...a, group: a.group || undefined, summary: a.summary || undefined, sources: sources[a.id] }));
}
