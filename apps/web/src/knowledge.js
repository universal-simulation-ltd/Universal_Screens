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
  en: { title: "Knowledge base", back: "All articles", close: "Close", loading: "Loading…", failed: "The articles couldn't be loaded. Check your connection and try again.", language: "Language" },
  fr: { title: "Base de connaissances", back: "Tous les articles", close: "Fermer", loading: "Chargement…", failed: "Impossible de charger les articles. Vérifiez votre connexion et réessayez.", language: "Langue" },
  es: { title: "Base de conocimientos", back: "Todos los artículos", close: "Cerrar", loading: "Cargando…", failed: "No se han podido cargar los artículos. Compruebe su conexión e inténtelo de nuevo.", language: "Idioma" },
  it: { title: "Knowledge base", back: "Tutti gli articoli", close: "Chiudi", loading: "Caricamento…", failed: "Impossibile caricare gli articoli. Controlla la connessione e riprova.", language: "Lingua" },
  de: { title: "Wissensdatenbank", back: "Alle Artikel", close: "Schließen", loading: "Wird geladen…", failed: "Die Artikel konnten nicht geladen werden. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.", language: "Sprache" },
  "pt-BR": { title: "Base de conhecimento", back: "Todos os artigos", close: "Fechar", loading: "Carregando…", failed: "Não foi possível carregar os artigos. Verifique sua conexão e tente novamente.", language: "Idioma" },
  "pt-PT": { title: "Base de conhecimento", back: "Todos os artigos", close: "Fechar", loading: "A carregar…", failed: "Não foi possível carregar os artigos. Verifique a ligação e tente novamente.", language: "Idioma" },
  tr: { title: "Bilgi bankası", back: "Tüm makaleler", close: "Kapat", loading: "Yükleniyor…", failed: "Makaleler yüklenemedi. Bağlantınızı kontrol edip tekrar deneyin.", language: "Dil" },
};

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
