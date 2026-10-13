// Advanced ▸ Knowledge base — the reader. The other Universal Apps open the
// SDK's React <KnowledgeBaseDialog>; this is the same page for a page with no
// framework (2026-10-12, after the suite's 2026-09-29 rollout): cards grouped
// and coloured by section, each counting its sources; then an article with a
// coloured hero, "Download this guide (PDF)", a Research and sources column
// and previous/next. Escape (and the back button) steps out of an article
// before Escape closes the dialog; click outside closes it. Bodies are built
// as elements with textContent from the closed mini-markdown
// (src/knowledge.js), never as HTML, and so are the sources.
//
// Kept apart from client.js on purpose: it needs neither the WASM codec nor a
// host, so the knowledge base still opens if either of those fails to load.

import {
  LANGUAGES, UI, detectLanguage, guideUrl, inlineRuns, libraryUrl, loadArticles, loadSources,
  parseArticleBody, sourcesCount,
} from "./knowledge.js";

const APP = "Universal Screens";

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
  let sources = {}; // article id → sources, the same in every language
  let request = 0;

  for (const l of LANGUAGES) {
    const o = el("option", "", l.label);
    o.value = l.code;
    o.lang = l.code;
    select.append(o);
  }

  /** Each group takes the next hue in turn, as in the SDK's reader. */
  function hueOf(a) {
    const groups = [];
    for (const x of articles) if (!groups.includes(x.group)) groups.push(x.group);
    return Math.max(0, groups.indexOf(a.group)) % 5;
  }

  function svgIcon(d, size = 14) {
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("width", String(size));
    svg.setAttribute("height", String(size));
    svg.setAttribute("aria-hidden", "true");
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", d);
    path.setAttribute("fill", "none");
    path.setAttribute("stroke", "currentColor");
    path.setAttribute("stroke-width", "2");
    path.setAttribute("stroke-linecap", "round");
    path.setAttribute("stroke-linejoin", "round");
    svg.append(path);
    return svg;
  }
  const DOWNLOAD = "M12 4v11m0 0l-4.5-4.5M12 15l4.5-4.5M5 20h14";

  function link(href, className, text) {
    const a = el("a", className, text);
    a.href = href;
    a.target = "_blank";
    a.rel = "noopener noreferrer";
    return a;
  }

  function cards(ui) {
    const frag = document.createDocumentFragment();
    frag.append(el("p", "kb-intro", ui.intro.replace("{app}", APP)));
    let grid = null;
    let group;
    for (const a of articles) {
      if (!grid || a.group !== group) {
        group = a.group;
        if (a.group) frag.append(el("p", "kb-group", a.group));
        grid = el("div", "kb-grid");
        frag.append(grid);
      }
      const card = el("button", `kb-cardlink kb-hue-${hueOf(a)}`);
      card.type = "button";
      card.append(el("span", "kb-title", a.title));
      if (a.summary) card.append(el("span", "kb-summary", a.summary));
      const foot = el("span", "kb-foot");
      const n = (sources[a.id] ?? []).length;
      foot.append(el("span", "", n ? sourcesCount(ui, n) : ""), el("span", "kb-read", `${ui.read} →`));
      card.append(foot);
      card.addEventListener("click", () => show(a.id));
      grid.append(card);
    }
    return frag;
  }

  function sourceItem(ui, s) {
    const li = el("li", "kb-source");
    li.append(el("span", "kb-kind", ui.kind[s.kind] ?? s.kind));
    li.append(link(s.href, "kb-source-title", s.title));
    const byline = [s.authors, s.publisher, s.year].filter(Boolean).join(" · ");
    if (byline) li.append(el("span", "kb-by", byline));
    const pills = el("span", "kb-pills");
    if (s.pdf) {
      const pdf = link(libraryUrl(s.pdf), "kb-pill kb-pill-strong", " PDF");
      pdf.prepend(svgIcon(DOWNLOAD, 11));
      if (s.licence) pdf.title = s.licence;
      pills.append(pdf);
    }
    pills.append(link(s.href, "kb-pill", `${s.pdf ? ui.publisher : ui.readSource} ↗`));
    li.append(pills);
    if (s.pdf && s.licence) li.append(el("span", "kb-licence", s.licence));
    return li;
  }

  function articleView(ui, article) {
    const index = articles.indexOf(article);
    const wrap = el("div", `kb-page kb-hue-${hueOf(article)}`);
    const hero = el("div", "kb-hero");
    if (article.group) hero.append(el("p", "kb-hero-group", article.group));
    const h = el("h3", "kb-hero-title", article.title);
    h.id = "kb-article-title";
    hero.append(h);
    if (article.summary) hero.append(el("p", "kb-hero-summary", article.summary));
    wrap.append(hero);

    const layout = el("div", "kb-layout");
    const main = el("div", "kb-article");
    for (const b of parseArticleBody(article.body)) {
      if (b.kind === "h") main.append(fillRuns(el("h4"), b.text));
      else if (b.kind === "p") main.append(fillRuns(el("p"), b.text));
      else {
        const list = el(b.kind);
        for (const item of b.items) list.append(fillRuns(el("li"), item));
        main.append(list);
      }
    }
    const aside = el("aside", "kb-aside");
    const guide = link(guideUrl(language, article.id), "kb-guide", ` ${ui.download}`);
    guide.prepend(svgIcon(DOWNLOAD));
    aside.append(guide);
    const list = sources[article.id] ?? [];
    if (list.length) {
      const box = el("div", "kb-sources");
      box.append(el("p", "kb-group", ui.sources));
      const ol = el("ol");
      for (const s of list) ol.append(sourceItem(ui, s));
      box.append(ol);
      aside.append(box);
    }
    layout.append(main, aside);
    wrap.append(layout);

    const prev = articles[index - 1];
    const next = articles[index + 1];
    if (prev || next) {
      const nav = el("nav", "kb-pager");
      for (const [a, label, cls] of [[prev, `← ${ui.previous}`, "kb-prev"], [next, `${ui.next} →`, "kb-next"]]) {
        if (!a) { nav.append(el("span")); continue; }
        const btn = el("button", `kb-pagelink ${cls}`);
        btn.type = "button";
        btn.append(el("span", "kb-pagelabel", label), el("span", "kb-pagetitle", a.title));
        btn.addEventListener("click", () => show(a.id));
        nav.append(btn);
      }
      wrap.append(nav);
    }
    return wrap;
  }

  function render() {
    const ui = UI[language] ?? UI.en;
    card.setAttribute("aria-label", `${ui.title} — ${APP}`);
    $("kb-close").setAttribute("aria-label", ui.close);
    $("kb-close").title = ui.close;
    $("kb-back-label").textContent = ui.back;
    $("kb-lang-label").textContent = ui.language;
    select.value = language;
    body.lang = language;
    body.replaceChildren();

    const article = current && Array.isArray(articles) ? articles.find((a) => a.id === current) : null;
    $("kb-back").hidden = !article;
    $("kb-lang-wrap").hidden = !!article;
    $("kb-title").textContent = ui.title;

    if (article) body.append(articleView(ui, article));
    else if (articles === "failed") body.append(el("p", "kb-status", ui.failed));
    else if (!articles) body.append(el("p", "kb-status", ui.loading));
    else body.append(cards(ui));
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
    Promise.all([loadArticles(language), loadSources()])
      .then(([a, s]) => { if (mine === request) { articles = a; sources = s; render(); } })
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
