// Advanced ▸ Knowledge base — the reader. The other Universal Apps open the
// SDK's React <KnowledgeBaseDialog>; this is the same behaviour for a page
// with no framework: a list grouped by heading, then one article; Escape (and
// the back button) steps out of an article before Escape closes the dialog;
// click outside closes it. Bodies are built as elements with textContent from
// the closed mini-markdown (src/knowledge.js), never as HTML.
//
// Kept apart from client.js on purpose: it needs neither the WASM codec nor a
// host, so the knowledge base still opens if either of those fails to load.

import { LANGUAGES, UI, detectLanguage, inlineRuns, loadArticles, parseArticleBody } from "./knowledge.js";

const LANG_KEY = "universal-screens.kb-language";

function initialLanguage() {
  try {
    const saved = localStorage.getItem(LANG_KEY);
    if (saved && LANGUAGES.some((l) => l.code === saved)) return saved;
  } catch { /* storage blocked: follow the device */ }
  return detectLanguage(navigator.languages ?? [navigator.language]);
}

/** Append `text` to `el` with its **bold** runs as <strong>. */
function fillRuns(el, text) {
  for (const run of inlineRuns(text)) {
    if (run.bold) {
      const b = document.createElement("strong");
      b.textContent = run.text;
      el.append(b);
    } else {
      el.append(document.createTextNode(run.text));
    }
  }
  return el;
}

function el(tag, className, text) {
  const n = document.createElement(tag);
  if (className) n.className = className;
  if (text != null) n.textContent = text;
  return n;
}

export function mountKnowledgeBase() {
  const $ = (id) => document.getElementById(id);
  const backdrop = $("kb-backdrop");
  const card = $("kb-card");
  const body = $("kb-body");
  const select = $("kb-lang");
  const opener = $("kb-open");
  if (!backdrop || !opener) return;

  let language = initialLanguage();
  let articles = null;
  let current = null; // article id, or null for the list
  let request = 0;

  for (const l of LANGUAGES) {
    const o = el("option", "", l.label);
    o.value = l.code;
    o.lang = l.code;
    select.append(o);
  }

  function render() {
    const ui = UI[language] ?? UI.en;
    card.setAttribute("aria-label", `${ui.title} — Universal Screens`);
    $("kb-close").setAttribute("aria-label", ui.close);
    $("kb-close").title = ui.close;
    $("kb-back-label").textContent = ui.back;
    $("kb-lang-label").textContent = ui.language;
    select.value = language;
    body.lang = language;
    body.replaceChildren();

    const article = current && articles ? articles.find((a) => a.id === current) : null;
    $("kb-back").hidden = !article;
    $("kb-lang-wrap").hidden = !!article;
    $("kb-title").textContent = article ? article.title : ui.title;

    if (article) {
      const wrap = el("div", "kb-article");
      for (const b of parseArticleBody(article.body)) {
        if (b.kind === "h") wrap.append(fillRuns(el("h3"), b.text));
        else if (b.kind === "p") wrap.append(fillRuns(el("p"), b.text));
        else {
          const list = el(b.kind);
          for (const item of b.items) list.append(fillRuns(el("li"), item));
          wrap.append(list);
        }
      }
      body.append(wrap);
    } else if (articles === "failed") {
      body.append(el("p", "kb-status", ui.failed));
    } else if (!articles) {
      body.append(el("p", "kb-status", ui.loading));
    } else {
      let group;
      for (const a of articles) {
        if (a.group && a.group !== group) {
          body.append(el("p", "kb-group", a.group));
          group = a.group;
        }
        const row = el("button", "kb-row");
        row.type = "button";
        const text = el("span", "kb-text");
        text.append(el("span", "kb-title", a.title));
        if (a.summary) text.append(el("span", "kb-summary", a.summary));
        const chev = document.createElementNS("http://www.w3.org/2000/svg", "svg");
        chev.setAttribute("viewBox", "0 0 12 12");
        chev.setAttribute("width", "11");
        chev.setAttribute("height", "11");
        chev.setAttribute("aria-hidden", "true");
        chev.classList.add("kb-chev");
        const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
        path.setAttribute("d", "M4 2 L8 6 L4 10");
        path.setAttribute("fill", "none");
        path.setAttribute("stroke", "currentColor");
        path.setAttribute("stroke-width", "1.5");
        path.setAttribute("stroke-linecap", "round");
        path.setAttribute("stroke-linejoin", "round");
        chev.append(path);
        row.append(text, chev);
        row.addEventListener("click", () => show(a.id));
        body.append(row);
      }
    }
    body.scrollTop = 0;
  }

  function show(id) {
    current = id;
    render();
    // Focus follows the page turn, so a keyboard user lands on "back".
    if (id) $("kb-back").focus();
  }

  function load() {
    const mine = ++request;
    articles = null;
    render();
    loadArticles(language)
      .then((a) => { if (mine === request) { articles = a; render(); } })
      .catch(() => { if (mine === request) { articles = "failed"; render(); } });
  }

  function open() {
    current = null;
    backdrop.classList.add("open");
    load();
    card.focus();
  }

  function close() {
    backdrop.classList.remove("open");
    opener.focus();
  }

  opener.addEventListener("click", open);
  $("kb-close").addEventListener("click", close);
  $("kb-back").addEventListener("click", () => { show(null); card.focus(); });
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });
  select.addEventListener("change", () => {
    language = select.value;
    try { localStorage.setItem(LANG_KEY, language); } catch { /* this visit only */ }
    load();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape" || !backdrop.classList.contains("open")) return;
    e.preventDefault();
    if (current) { show(null); card.focus(); }
    else close();
  });
}
