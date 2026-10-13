// The knowledge base's data side, for the browser client: which languages
// there are, how to read a bundle, and how to split an article body into
// blocks. Pure — no DOM — so knowledge.test.mjs runs it in Node.
//
// ⚠️ The articles are NOT in this file. They are `../knowledge/<lang>.md`,
// the one copy every client reads: this page fetches it, the desktop hosts
// embed it (crates/host-ui/src/knowledge.rs, include_str!), and the Android
// app packs the same folder as assets (Knowledge.kt). Keep the three parsers
// in step; the format is kept trivial so that is easy.
//
// Bundle format: anything before the first `---` line is a note to
// translators. Each article then opens with `key: value` lines (id, group,
// title, summary) closed by `---`, and its body runs to the next `---`.
//
// The research and sources are `../knowledge/sources.md`, one source per
// block of `key: value` lines (parseSources), shared the same way.
//
// Body format: the closed mini-markdown the SDK's parseArticleBody reads —
// blank-line paragraphs, `## ` headings, `- ` bullets, `1. ` steps and
// `**bold**`. The reader builds elements from the blocks with textContent,
// never innerHTML, so nothing in an article can inject markup.

export const LANGUAGES = [
  { code: "en", label: "English" },
  { code: "fr", label: "Français" },
  { code: "es", label: "Español" },
  { code: "it", label: "Italiano" },
  { code: "de", label: "Deutsch" },
  { code: "pt-BR", label: "Português (Brasil)" },
  { code: "pt-PT", label: "Português (Portugal)" },
  { code: "tr", label: "Türkçe" },
];

// The reader's own words — the SDK's kb.* strings, as in every other
// Universal App's reader. The app itself is English-only, so the reader
// carries its own language picker.
export const UI = {
  en: { title: "Knowledge base", back: "All articles", close: "Close", loading: "Loading…", failed: "The articles couldn't be loaded. Check your connection and try again.", language: "Language",
    intro: "Plain-English guides to how {app} works, with the research and standards behind them.", read: "Read", sourcesOne: "1 source", sourcesMany: "{n} sources", sources: "Research and sources", download: "Download this guide (PDF)", publisher: "Publisher", readSource: "Read at source", previous: "Previous", next: "Next",
    kind: { paper: "Research paper", standard: "Standard", report: "Report", guidance: "Guidance", law: "Law" } },
  fr: { title: "Base de connaissances", back: "Tous les articles", close: "Fermer", loading: "Chargement…", failed: "Impossible de charger les articles. Vérifiez votre connexion et réessayez.", language: "Langue",
    intro: "Des guides en langage clair sur le fonctionnement de {app}, avec les recherches et les normes sur lesquelles ils s'appuient.", read: "Lire", sourcesOne: "1 source", sourcesMany: "{n} sources", sources: "Recherches et sources", download: "Télécharger ce guide (PDF)", publisher: "Éditeur", readSource: "Lire à la source", previous: "Précédent", next: "Suivant",
    kind: { paper: "Article de recherche", standard: "Norme", report: "Rapport", guidance: "Recommandations", law: "Législation" } },
  es: { title: "Base de conocimientos", back: "Todos los artículos", close: "Cerrar", loading: "Cargando…", failed: "No se han podido cargar los artículos. Compruebe su conexión e inténtelo de nuevo.", language: "Idioma",
    intro: "Guías claras sobre cómo funciona {app}, con la investigación y las normas en que se basan.", read: "Leer", sourcesOne: "1 fuente", sourcesMany: "{n} fuentes", sources: "Investigación y fuentes", download: "Descargar esta guía (PDF)", publisher: "Editor", readSource: "Leer en la fuente", previous: "Anterior", next: "Siguiente",
    kind: { paper: "Artículo de investigación", standard: "Norma", report: "Informe", guidance: "Guía", law: "Legislación" } },
  it: { title: "Knowledge base", back: "Tutti gli articoli", close: "Chiudi", loading: "Caricamento…", failed: "Impossibile caricare gli articoli. Controlla la connessione e riprova.", language: "Lingua",
    intro: "Guide chiare su come funziona {app}, con le ricerche e gli standard su cui si basano.", read: "Leggi", sourcesOne: "1 fonte", sourcesMany: "{n} fonti", sources: "Ricerche e fonti", download: "Scarica questa guida (PDF)", publisher: "Editore", readSource: "Leggi alla fonte", previous: "Precedente", next: "Successivo",
    kind: { paper: "Articolo di ricerca", standard: "Standard", report: "Rapporto", guidance: "Linee guida", law: "Normativa" } },
  de: { title: "Wissensdatenbank", back: "Alle Artikel", close: "Schließen", loading: "Wird geladen…", failed: "Die Artikel konnten nicht geladen werden. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.", language: "Sprache",
    intro: "Verständliche Anleitungen dazu, wie {app} funktioniert, mit der Forschung und den Normen dahinter.", read: "Lesen", sourcesOne: "1 Quelle", sourcesMany: "{n} Quellen", sources: "Forschung und Quellen", download: "Diese Anleitung herunterladen (PDF)", publisher: "Herausgeber", readSource: "An der Quelle lesen", previous: "Zurück", next: "Weiter",
    kind: { paper: "Fachartikel", standard: "Norm", report: "Bericht", guidance: "Leitfaden", law: "Rechtsvorschrift" } },
  "pt-BR": { title: "Base de conhecimento", back: "Todos os artigos", close: "Fechar", loading: "Carregando…", failed: "Não foi possível carregar os artigos. Verifique sua conexão e tente novamente.", language: "Idioma",
    intro: "Guias claros sobre como o {app} funciona, com as pesquisas e as normas em que se baseiam.", read: "Ler", sourcesOne: "1 fonte", sourcesMany: "{n} fontes", sources: "Pesquisas e fontes", download: "Baixar este guia (PDF)", publisher: "Editora", readSource: "Ler na fonte", previous: "Anterior", next: "Próximo",
    kind: { paper: "Artigo científico", standard: "Norma", report: "Relatório", guidance: "Orientações", law: "Legislação" } },
  "pt-PT": { title: "Base de conhecimento", back: "Todos os artigos", close: "Fechar", loading: "A carregar…", failed: "Não foi possível carregar os artigos. Verifique a ligação e tente novamente.", language: "Idioma",
    intro: "Guias claros sobre o funcionamento da {app}, com a investigação e as normas em que se baseiam.", read: "Ler", sourcesOne: "1 fonte", sourcesMany: "{n} fontes", sources: "Investigação e fontes", download: "Transferir este guia (PDF)", publisher: "Editor", readSource: "Ler na fonte", previous: "Anterior", next: "Seguinte",
    kind: { paper: "Artigo de investigação", standard: "Norma", report: "Relatório", guidance: "Orientações", law: "Legislação" } },
  tr: { title: "Bilgi bankası", back: "Tüm makaleler", close: "Kapat", loading: "Yükleniyor…", failed: "Makaleler yüklenemedi. Bağlantınızı kontrol edip tekrar deneyin.", language: "Dil",
    intro: "{app} uygulamasının nasıl çalıştığına dair anlaşılır rehberler ve dayandıkları araştırmalar ile standartlar.", read: "Oku", sourcesOne: "1 kaynak", sourcesMany: "{n} kaynak", sources: "Araştırma ve kaynaklar", download: "Bu rehberi indir (PDF)", publisher: "Yayıncı", readSource: "Kaynağında oku", previous: "Önceki", next: "Sonraki",
    kind: { paper: "Araştırma makalesi", standard: "Standart", report: "Rapor", guidance: "Kılavuz", law: "Mevzuat" } },
};

/** "3 sources", in the reader's language. */
export function sourcesCount(ui, n) {
  return (n === 1 ? ui.sourcesOne : ui.sourcesMany).replace("{n}", String(n));
}

// ── The library: guide PDFs and hosted papers ───────────────────────────────
// Served by the opensource portal, never bundled (the same place the other
// Universal Apps' readers link). One guide per article and language, printed
// by universal-platform's packages/sdk/scripts/kb-pdfs.mjs through
// scripts/kb-pdfs-adapter.mjs here; re-run it after editing an article or a
// source, and commit the PDFs to the portal.
export const KB_LIBRARY_URL = "https://opensource.unisim.co.uk/kb";
export const KB_PRODUCT = "screens";

/** The guide PDF for an article in a language (English for anything unknown). */
export function guideUrl(code, id) {
  const lang = LANGUAGES.some((l) => l.code === code) ? code : "en";
  return `${KB_LIBRARY_URL}/${KB_PRODUCT}/${lang}/${encodeURIComponent(id)}.pdf`;
}

/** A source's hosted copy (`papers/…`, relative to the library) as a URL. */
export function libraryUrl(path) {
  return /^https?:\/\//.test(path) ? path : `${KB_LIBRARY_URL}/${String(path).replace(/^\//, "")}`;
}

/** Best match for the device's languages; English when nothing fits. */
export function detectLanguage(prefs) {
  for (const raw of prefs ?? []) {
    const tag = String(raw).toLowerCase();
    if (tag === "pt-br") return "pt-BR";
    if (tag.startsWith("pt")) return "pt-PT";
    const base = tag.split("-")[0];
    const hit = LANGUAGES.find((l) => l.code === base);
    if (hit) return hit.code;
  }
  return "en";
}

/** Split a bundle into articles: [{ id, group, title, summary, body }]. */
export function parseBundle(src) {
  const lines = String(src).replace(/\r\n?/g, "\n").split("\n");
  let i = 0;
  while (i < lines.length && lines[i].trim() !== "---") i++; // the translators' note
  i++;
  const out = [];
  while (i < lines.length) {
    const a = { id: "", group: "", title: "", summary: "", body: "" };
    let sawAny = false;
    let closed = false;
    for (; i < lines.length; i++) {
      if (lines[i].trim() === "---") { closed = true; i++; break; }
      const at = lines[i].indexOf(":");
      if (at > 0) {
        sawAny = true;
        const key = lines[i].slice(0, at).trim();
        if (key in a) a[key] = lines[i].slice(at + 1).trim();
      }
    }
    if (!sawAny || !closed) break;
    const body = [];
    for (; i < lines.length; i++) {
      if (lines[i].trim() === "---") { i++; break; }
      body.push(lines[i]);
    }
    a.body = body.join("\n").trim();
    out.push(a);
  }
  return out;
}

const SOURCE_KEYS = ["article", "kind", "title", "authors", "publisher", "year", "href", "pdf", "licence"];
const KINDS = ["paper", "standard", "report", "guidance", "law"];

/**
 * Split `knowledge/sources.md` into { [article id]: [source, …] }, in file
 * order. One source per block of `key: value` lines, each closed by `---`;
 * anything before the first `---` is a note. A block without an article,
 * title or http(s) link, or with an unknown kind, is dropped.
 */
export function parseSources(src) {
  const lines = String(src).replace(/\r\n?/g, "\n").split("\n");
  let i = 0;
  while (i < lines.length && lines[i].trim() !== "---") i++;
  i++;
  const out = {};
  let cur = {};
  const flush = () => {
    const { article, kind, title, href } = cur;
    if (article && title && KINDS.includes(kind) && /^https?:\/\//.test(href ?? "")) {
      const s = { kind, title, href };
      for (const k of ["authors", "publisher", "pdf", "licence"]) if (cur[k]) s[k] = cur[k];
      if (/^\d{4}$/.test(cur.year ?? "")) s.year = Number(cur.year);
      (out[article] ??= []).push(s);
    }
    cur = {};
  };
  for (; i < lines.length; i++) {
    if (lines[i].trim() === "---") { flush(); continue; }
    const at = lines[i].indexOf(":");
    if (at > 0) {
      const key = lines[i].slice(0, at).trim();
      if (SOURCE_KEYS.includes(key)) cur[key] = lines[i].slice(at + 1).trim();
    }
  }
  flush();
  return out;
}

let sourcesCache = null;

/** Fetch and parse the sources once. {} when they can't be loaded: the
 *  articles still read without them. */
export async function loadSources(fetchImpl = globalThis.fetch) {
  if (sourcesCache) return sourcesCache;
  try {
    const res = await fetchImpl(new URL("../knowledge/sources.md", import.meta.url));
    if (!res.ok) return {};
    sourcesCache = parseSources(await res.text());
    return sourcesCache;
  } catch {
    return {};
  }
}

/** Splits an article body into blocks. A port of the SDK's parseArticleBody. */
export function parseArticleBody(body) {
  const blocks = [];
  const lines = String(body).replace(/\r\n?/g, "\n").split("\n");
  let para = [];
  const flush = () => {
    if (para.length) blocks.push({ kind: "p", text: para.join(" ") });
    para = [];
  };
  for (const raw of lines) {
    const line = raw.trim();
    if (!line) { flush(); continue; }
    const h = /^#{2,3}\s+(.*)$/.exec(line);
    const ul = /^[-•*]\s+(.*)$/.exec(line);
    const ol = /^\d+[.)]\s+(.*)$/.exec(line);
    if (h) { flush(); blocks.push({ kind: "h", text: h[1] ?? "" }); continue; }
    if (ul || ol) {
      flush();
      const kind = ul ? "ul" : "ol";
      const text = (ul ?? ol)[1] ?? "";
      const last = blocks[blocks.length - 1];
      if (last && last.kind === kind) last.items.push(text);
      else blocks.push({ kind, items: [text] });
      continue;
    }
    // A wrapped line directly under a bullet continues that bullet.
    const last = blocks[blocks.length - 1];
    if (!para.length && last && (last.kind === "ul" || last.kind === "ol") && /^\s/.test(raw)) {
      last.items[last.items.length - 1] = `${last.items[last.items.length - 1] ?? ""} ${line}`;
      continue;
    }
    para.push(line);
  }
  flush();
  return blocks;
}

/** `**bold**` runs and plain text, in order. Everything else stays literal. */
export function inlineRuns(text) {
  return String(text)
    .split(/(\*\*[^*]+\*\*)/g)
    .filter((part) => part !== "")
    .map((part) =>
      /^\*\*[^*]+\*\*$/.test(part) ? { bold: true, text: part.slice(2, -2) } : { bold: false, text: part },
    );
}

const cache = new Map();

/** Fetch and parse a language's bundle (English for anything unknown). */
export async function loadArticles(code, fetchImpl = globalThis.fetch) {
  const lang = LANGUAGES.some((l) => l.code === code) ? code : "en";
  if (cache.has(lang)) return cache.get(lang);
  const res = await fetchImpl(new URL(`../knowledge/${lang}.md`, import.meta.url));
  if (!res.ok) throw new Error(`knowledge/${lang}.md: HTTP ${res.status}`);
  const articles = parseBundle(await res.text());
  if (!articles.length) throw new Error(`knowledge/${lang}.md: no articles`);
  cache.set(lang, articles);
  return articles;
}
