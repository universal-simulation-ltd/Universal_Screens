//! The knowledge base — short articles on what Universal Screens is, how a
//! connection works and what leaves your network — and the desktop hosts'
//! reader for it.
//!
//! ⚠️ **One copy of the articles, for every client.** They live in
//! `apps/web/knowledge/<language>.md`, because the browser client has no build
//! step and can only fetch files inside its own folder. The hosts embed those
//! same files here with `include_str!`, and the Android app packs the same
//! folder as assets (`apps/android/app/build.gradle.kts`). Nothing is copied or
//! generated, so a translation fixed once is fixed everywhere.
//!
//! The bundle format is deliberately trivial so that three parsers (this one,
//! `apps/web/src/knowledge.js` and the Android `Knowledge.kt`) stay obviously
//! the same: each article opens with `key: value` lines between two `---`
//! lines, and its body runs to the next `---`. Anything before the first `---`
//! is a note to translators and is ignored.
//!
//! The body is the suite's closed mini-markdown, the one `@unisim/sdk`'s
//! `parseArticleBody` reads: blank-line paragraphs, `## ` headings, `- `
//! bullets, `1. ` steps and `**bold**`. Nothing else is interpreted, and every
//! reader renders it as widgets or elements, never as HTML.
//!
//! The research and sources behind each article are `knowledge/sources.md`,
//! the same in every language and read by all three clients the same way
//! (one source per `key: value` block). Every article also links its printed
//! guide on opensource.unisim.co.uk/kb/screens/<lang>/<id>.pdf, made from these
//! same files by universal-platform's `kb-pdfs.mjs` via
//! `scripts/kb-pdfs-adapter.mjs`.
//!
//! The tests below check the data for all clients at once: every language has
//! the same articles in the same order, and no body strays outside that syntax.

use eframe::egui;

/// The suite's eight knowledge-base languages, as (code, own name).
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("fr", "Français"),
    ("es", "Español"),
    ("it", "Italiano"),
    ("de", "Deutsch"),
    ("pt-BR", "Português (Brasil)"),
    ("pt-PT", "Português (Portugal)"),
    ("tr", "Türkçe"),
];

/// The eframe storage key for the reader's language. One key for every host.
pub const KB_LANGUAGE_KEY: &str = "kb_language";

/// The raw bundle for a language code; English for anything unknown.
pub fn bundle(code: &str) -> &'static str {
    match code {
        "fr" => include_str!("../../../apps/web/knowledge/fr.md"),
        "es" => include_str!("../../../apps/web/knowledge/es.md"),
        "it" => include_str!("../../../apps/web/knowledge/it.md"),
        "de" => include_str!("../../../apps/web/knowledge/de.md"),
        "pt-BR" => include_str!("../../../apps/web/knowledge/pt-BR.md"),
        "pt-PT" => include_str!("../../../apps/web/knowledge/pt-PT.md"),
        "tr" => include_str!("../../../apps/web/knowledge/tr.md"),
        _ => include_str!("../../../apps/web/knowledge/en.md"),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Article {
    /// Stable kebab-case slug, the same in every language.
    pub id: String,
    /// List heading ("The basics", …), translated.
    pub group: String,
    pub title: String,
    /// One line under the title in the list.
    pub summary: String,
    pub body: String,
}

/// Split a bundle into articles. See the module docs for the format.
pub fn parse_bundle(src: &str) -> Vec<Article> {
    let mut out: Vec<Article> = Vec::new();
    let mut lines = src.lines().map(|l| l.trim_end_matches('\r'));
    // Skip the translators' note.
    for line in lines.by_ref() {
        if line.trim() == "---" {
            break;
        }
    }
    loop {
        // Front matter, up to its closing `---`.
        let mut a = Article::default();
        let mut saw_any = false;
        let mut closed = false;
        for line in lines.by_ref() {
            if line.trim() == "---" {
                closed = true;
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                saw_any = true;
                let v = v.trim().to_owned();
                match k.trim() {
                    "id" => a.id = v,
                    "group" => a.group = v,
                    "title" => a.title = v,
                    "summary" => a.summary = v,
                    _ => {}
                }
            }
        }
        if !saw_any || !closed {
            break;
        }
        // Body, up to the next article's opening `---` or the end.
        let mut body: Vec<&str> = Vec::new();
        let mut more = false;
        for line in lines.by_ref() {
            if line.trim() == "---" {
                more = true;
                break;
            }
            body.push(line);
        }
        a.body = body.join("\n").trim().to_owned();
        out.push(a);
        if !more {
            break;
        }
    }
    out
}

/// Articles for a language code; English for anything unknown.
pub fn articles(code: &str) -> Vec<Article> {
    parse_bundle(bundle(code))
}

/// The research and sources behind the articles, the same in every language:
/// `apps/web/knowledge/sources.md`, which the browser client and the Android
/// app read too.
pub const SOURCES: &str = include_str!("../../../apps/web/knowledge/sources.md");

/// A paper, standard or guidance an article draws on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Source {
    /// The article id it belongs to.
    pub article: String,
    /// paper, standard, report, guidance or law.
    pub kind: String,
    pub title: String,
    pub authors: String,
    pub publisher: String,
    pub year: String,
    /// The canonical free link.
    pub href: String,
    /// Our hosted copy, relative to [`KB_LIBRARY_URL`] — only where the
    /// licence allows it.
    pub pdf: String,
    pub licence: String,
}

const KINDS: &[&str] = &["paper", "standard", "report", "guidance", "law"];

/// Split `sources.md` into sources, in file order: one per block of
/// `key: value` lines, each closed by `---`; anything before the first `---`
/// is a note. A block without an article, a title, a known kind or an
/// http(s) link is dropped. The same rules as `parseSources` in
/// `apps/web/src/knowledge.js`.
pub fn parse_sources(src: &str) -> Vec<Source> {
    let mut out = Vec::new();
    let mut lines = src.lines().map(|l| l.trim_end_matches('\r'));
    for line in lines.by_ref() {
        if line.trim() == "---" {
            break;
        }
    }
    let mut cur = Source::default();
    let flush = |cur: &mut Source, out: &mut Vec<Source>| {
        let s = std::mem::take(cur);
        let ok = !s.article.is_empty()
            && !s.title.is_empty()
            && KINDS.contains(&s.kind.as_str())
            && (s.href.starts_with("https://") || s.href.starts_with("http://"));
        if ok {
            out.push(s);
        }
    };
    for line in lines {
        if line.trim() == "---" {
            flush(&mut cur, &mut out);
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim().to_owned();
            match k.trim() {
                "article" => cur.article = v,
                "kind" => cur.kind = v,
                "title" => cur.title = v,
                "authors" => cur.authors = v,
                "publisher" => cur.publisher = v,
                "year" => cur.year = v,
                "href" => cur.href = v,
                "pdf" => cur.pdf = v,
                "licence" => cur.licence = v,
                _ => {}
            }
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// The sources for one article, in order.
pub fn sources_for(id: &str) -> Vec<Source> {
    parse_sources(SOURCES).into_iter().filter(|s| s.article == id).collect()
}

/// Where the suite's knowledge-base PDFs are served (the opensource portal).
pub const KB_LIBRARY_URL: &str = "https://opensource.unisim.co.uk/kb";

/// The printed guide for an article in a language (English for anything
/// unknown), made by universal-platform's `kb-pdfs.mjs`.
pub fn guide_url(code: &str, id: &str) -> String {
    let lang = if LANGUAGES.iter().any(|(c, _)| *c == code) { code } else { "en" };
    format!("{KB_LIBRARY_URL}/screens/{lang}/{id}.pdf")
}

/// A source's hosted copy (`papers/…`) as a URL.
pub fn library_url(path: &str) -> String {
    if path.starts_with("https://") || path.starts_with("http://") {
        path.to_owned()
    } else {
        format!("{KB_LIBRARY_URL}/{}", path.trim_start_matches('/'))
    }
}

/// One block of an article body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Heading(String),
    Paragraph(String),
    Bullets(Vec<String>),
    Steps(Vec<String>),
}

fn list_item(line: &str) -> Option<(bool, &str)> {
    for marker in ["- ", "• ", "* "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return Some((false, rest.trim_start()));
        }
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 {
        let rest = &line[digits..];
        let rest = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") "))?;
        return Some((true, rest.trim_start()));
    }
    None
}

/// Split a body into blocks. A port of the SDK's `parseArticleBody`.
pub fn parse_body(body: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    fn flush(blocks: &mut Vec<Block>, para: &mut Vec<&str>) {
        if !para.is_empty() {
            blocks.push(Block::Paragraph(para.join(" ")));
            para.clear();
        }
    }
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() {
            flush(&mut blocks, &mut para);
            continue;
        }
        let heading = line
            .strip_prefix("### ")
            .or_else(|| line.strip_prefix("## "))
            .map(str::trim);
        if let Some(h) = heading {
            flush(&mut blocks, &mut para);
            blocks.push(Block::Heading(h.to_owned()));
            continue;
        }
        if let Some((numbered, text)) = list_item(line) {
            flush(&mut blocks, &mut para);
            match (blocks.last_mut(), numbered) {
                (Some(Block::Bullets(items)), false) | (Some(Block::Steps(items)), true) => {
                    items.push(text.to_owned())
                }
                (_, false) => blocks.push(Block::Bullets(vec![text.to_owned()])),
                (_, true) => blocks.push(Block::Steps(vec![text.to_owned()])),
            }
            continue;
        }
        // A wrapped, indented line directly under a list item continues it.
        if para.is_empty() && raw.starts_with(char::is_whitespace) {
            if let Some(Block::Bullets(items) | Block::Steps(items)) = blocks.last_mut() {
                if let Some(last) = items.last_mut() {
                    last.push(' ');
                    last.push_str(line);
                    continue;
                }
            }
        }
        para.push(line);
    }
    flush(&mut blocks, &mut para);
    blocks
}

/// `**bold**` runs and plain text, in order, as (bold, text). Everything else
/// stays literal, including a lone `**` with no partner.
pub fn inline_runs(text: &str) -> Vec<(bool, &str)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("**") {
        let after = &rest[start + 2..];
        match after.find("**") {
            Some(end) if end > 0 && !after[..end].contains('*') => {
                if start > 0 {
                    out.push((false, &rest[..start]));
                }
                out.push((true, &after[..end]));
                rest = &after[end + 2..];
            }
            _ => break,
        }
    }
    if !rest.is_empty() {
        out.push((false, rest));
    }
    out
}

/// The reader's own words, per language — the SDK's `kb.*` strings, as in the
/// other Universal Apps' readers.
pub struct UiText {
    pub title: &'static str,
    pub back: &'static str,
    pub language: &'static str,
    pub download: &'static str,
    pub sources: &'static str,
    pub publisher: &'static str,
    pub read_source: &'static str,
    pub previous: &'static str,
    pub next: &'static str,
    /// "{n} sources"; [`UiText::count`] picks the singular.
    pub sources_many: &'static str,
    pub source_one: &'static str,
    /// paper, standard, report, guidance, law.
    pub kinds: [&'static str; 5],
}

impl UiText {
    pub fn count(&self, n: usize) -> String {
        if n == 1 { self.source_one.to_owned() } else { self.sources_many.replace("{n}", &n.to_string()) }
    }

    pub fn kind(&self, kind: &str) -> &'static str {
        KINDS.iter().position(|k| *k == kind).map(|i| self.kinds[i]).unwrap_or("")
    }
}

pub fn ui_text(code: &str) -> UiText {
    match code {
        "fr" => UiText {
            title: "Base de connaissances", back: "Tous les articles", language: "Langue",
            download: "Télécharger ce guide (PDF)", sources: "Recherches et sources", publisher: "Éditeur",
            read_source: "Lire à la source", previous: "Précédent", next: "Suivant",
            sources_many: "{n} sources", source_one: "1 source",
            kinds: ["Article de recherche", "Norme", "Rapport", "Recommandations", "Législation"],
        },
        "es" => UiText {
            title: "Base de conocimientos", back: "Todos los artículos", language: "Idioma",
            download: "Descargar esta guía (PDF)", sources: "Investigación y fuentes", publisher: "Editor",
            read_source: "Leer en la fuente", previous: "Anterior", next: "Siguiente",
            sources_many: "{n} fuentes", source_one: "1 fuente",
            kinds: ["Artículo de investigación", "Norma", "Informe", "Guía", "Legislación"],
        },
        "it" => UiText {
            title: "Knowledge base", back: "Tutti gli articoli", language: "Lingua",
            download: "Scarica questa guida (PDF)", sources: "Ricerche e fonti", publisher: "Editore",
            read_source: "Leggi alla fonte", previous: "Precedente", next: "Successivo",
            sources_many: "{n} fonti", source_one: "1 fonte",
            kinds: ["Articolo di ricerca", "Standard", "Rapporto", "Linee guida", "Normativa"],
        },
        "de" => UiText {
            title: "Wissensdatenbank", back: "Alle Artikel", language: "Sprache",
            download: "Diese Anleitung herunterladen (PDF)", sources: "Forschung und Quellen", publisher: "Herausgeber",
            read_source: "An der Quelle lesen", previous: "Zurück", next: "Weiter",
            sources_many: "{n} Quellen", source_one: "1 Quelle",
            kinds: ["Fachartikel", "Norm", "Bericht", "Leitfaden", "Rechtsvorschrift"],
        },
        "pt-BR" => UiText {
            title: "Base de conhecimento", back: "Todos os artigos", language: "Idioma",
            download: "Baixar este guia (PDF)", sources: "Pesquisas e fontes", publisher: "Editora",
            read_source: "Ler na fonte", previous: "Anterior", next: "Próximo",
            sources_many: "{n} fontes", source_one: "1 fonte",
            kinds: ["Artigo científico", "Norma", "Relatório", "Orientações", "Legislação"],
        },
        "pt-PT" => UiText {
            title: "Base de conhecimento", back: "Todos os artigos", language: "Idioma",
            download: "Transferir este guia (PDF)", sources: "Investigação e fontes", publisher: "Editor",
            read_source: "Ler na fonte", previous: "Anterior", next: "Seguinte",
            sources_many: "{n} fontes", source_one: "1 fonte",
            kinds: ["Artigo de investigação", "Norma", "Relatório", "Orientações", "Legislação"],
        },
        "tr" => UiText {
            title: "Bilgi bankası", back: "Tüm makaleler", language: "Dil",
            download: "Bu rehberi indir (PDF)", sources: "Araştırma ve kaynaklar", publisher: "Yayıncı",
            read_source: "Kaynağında oku", previous: "Önceki", next: "Sonraki",
            sources_many: "{n} kaynak", source_one: "1 kaynak",
            kinds: ["Araştırma makalesi", "Standart", "Rapor", "Kılavuz", "Mevzuat"],
        },
        _ => UiText {
            title: "Knowledge base", back: "All articles", language: "Language",
            download: "Download this guide (PDF)", sources: "Research and sources", publisher: "Publisher",
            read_source: "Read at source", previous: "Previous", next: "Next",
            sources_many: "{n} sources", source_one: "1 source",
            kinds: ["Research paper", "Standard", "Report", "Guidance", "Law"],
        },
    }
}

/// Each group takes the next hue, as in the SDK's reader: blue, violet,
/// green, amber, rose (the `bar` colours, readable on light and dark).
const HUES: [egui::Color32; 5] = [
    egui::Color32::from_rgb(0x3b, 0x82, 0xf6),
    egui::Color32::from_rgb(0x8b, 0x5c, 0xf6),
    egui::Color32::from_rgb(0x10, 0xb9, 0x81),
    egui::Color32::from_rgb(0xf5, 0x9e, 0x0b),
    egui::Color32::from_rgb(0xf4, 0x3f, 0x5e),
];

fn hue_of(list: &[Article], group: &str) -> egui::Color32 {
    let mut groups: Vec<&str> = Vec::new();
    for a in list {
        if !groups.contains(&a.group.as_str()) {
            groups.push(&a.group);
        }
    }
    HUES[groups.iter().position(|g| *g == group).unwrap_or(0) % HUES.len()]
}

/// The desktop reader: a centred window with the article list (grouped, with a
/// language picker) and one article at a time. Escape steps back out of an
/// article before it closes the window, as in the web readers.
#[derive(Default)]
pub struct KnowledgeReader {
    pub show: bool,
    /// Language code. Empty means English.
    pub language: String,
    current: Option<String>,
    cache: Option<(String, Vec<Article>)>,
}

impl KnowledgeReader {
    /// A reader that remembers `language` (as saved under [`KB_LANGUAGE_KEY`]).
    pub fn with_language(language: Option<String>) -> Self {
        Self { language: language.unwrap_or_default(), ..Self::default() }
    }

    /// Open on the list.
    pub fn open(&mut self) {
        self.show = true;
        self.current = None;
    }

    /// Open straight on one article (by id). An unknown id shows the list.
    pub fn open_article(&mut self, id: &str) {
        self.show = true;
        self.current = Some(id.to_owned());
    }

    fn lang(&self) -> &str {
        if LANGUAGES.iter().any(|(c, _)| *c == self.language) {
            &self.language
        } else {
            "en"
        }
    }

    fn list(&mut self) -> &[Article] {
        let lang = self.lang().to_owned();
        if self.cache.as_ref().map(|(l, _)| l != &lang).unwrap_or(true) {
            self.cache = Some((lang.clone(), articles(&lang)));
        }
        &self.cache.as_ref().expect("filled above").1
    }
}

/// Lay out one line of body text with its `**bold**` runs.
fn rich_line(ui: &egui::Ui, text: &str, size: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for (bold, run) in inline_runs(text) {
        let mut rt = egui::RichText::new(run).size(size);
        if bold {
            rt = rt.strong();
        }
        rt.append_to(&mut job, ui.style(), egui::FontSelection::Default, egui::Align::LEFT);
    }
    job
}

/// Draw the reader window, if it is open.
pub fn show_knowledge_window(ctx: &egui::Context, reader: &mut KnowledgeReader) {
    if !reader.show {
        return;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        if reader.current.is_some() {
            reader.current = None;
        } else {
            reader.show = false;
            return;
        }
    }
    let ui_words = ui_text(reader.lang());
    let mut open = true;
    egui::Window::new(format!("📖  {}", ui_words.title))
        .id(egui::Id::new("knowledge_base_window"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(460.0);
            let max_h = (ctx.screen_rect().height() - 140.0).max(200.0);
            let current = reader.current.clone();
            let article = current.and_then(|id| reader.list().iter().find(|a| a.id == id).cloned());
            match article {
                Some(a) => {
                    if ui.link(format!("‹ {}", ui_words.back)).clicked() {
                        reader.current = None;
                    }
                    ui.add_space(4.0);
                    let list = reader.list().to_vec();
                    let hue = hue_of(&list, &a.group);
                    if !a.group.is_empty() {
                        ui.label(egui::RichText::new(a.group.to_uppercase()).size(10.5).strong().color(hue));
                    }
                    ui.label(egui::RichText::new(&a.title).strong().size(15.0));
                    if !a.summary.is_empty() {
                        ui.add(egui::Label::new(egui::RichText::new(&a.summary).weak()).wrap());
                    }
                    ui.add_space(6.0);
                    let lang = reader.lang().to_owned();
                    let mut go: Option<String> = None;
                    egui::ScrollArea::vertical()
                        .id_salt(("kb_article", a.id.as_str()))
                        .max_height(max_h)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.hyperlink_to(
                                egui::RichText::new(format!("⬇  {}", ui_words.download)).strong().color(hue),
                                guide_url(&lang, &a.id),
                            );
                            ui.add_space(6.0);
                            article_body(ui, &a.body);
                            let sources = sources_for(&a.id);
                            if !sources.is_empty() {
                                ui.add_space(8.0);
                                ui.separator();
                                ui.label(
                                    egui::RichText::new(ui_words.sources.to_uppercase())
                                        .size(10.5)
                                        .strong()
                                        .color(ui.visuals().weak_text_color()),
                                );
                                ui.add_space(2.0);
                                for s in &sources {
                                    source_item(ui, &ui_words, s);
                                    ui.add_space(6.0);
                                }
                            }
                            let i = list.iter().position(|x| x.id == a.id).unwrap_or(0);
                            let prev = i.checked_sub(1).and_then(|p| list.get(p));
                            let next = list.get(i + 1);
                            if prev.is_some() || next.is_some() {
                                ui.add_space(6.0);
                                ui.separator();
                                ui.horizontal(|ui| {
                                    if let Some(p) = prev {
                                        if ui.button(format!("‹ {}", ui_words.previous)).on_hover_text(&p.title).clicked() {
                                            go = Some(p.id.clone());
                                        }
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if let Some(n) = next {
                                            if ui.button(format!("{} ›", ui_words.next)).on_hover_text(&n.title).clicked() {
                                                go = Some(n.id.clone());
                                            }
                                        }
                                    });
                                });
                            }
                        });
                    if go.is_some() {
                        reader.current = go;
                    }
                }
                None => {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(ui_words.language).small().weak());
                        let shown = LANGUAGES
                            .iter()
                            .find(|(c, _)| *c == reader.lang())
                            .map(|(_, n)| *n)
                            .unwrap_or("English");
                        egui::ComboBox::from_id_salt("kb_language")
                            .selected_text(shown)
                            .show_ui(ui, |ui| {
                                for (code, name) in LANGUAGES {
                                    if ui.selectable_label(reader.lang() == *code, *name).clicked() {
                                        reader.language = (*code).to_owned();
                                    }
                                }
                            });
                    });
                    ui.add_space(6.0);
                    let list = reader.list().to_vec();
                    let mut chosen = None;
                    egui::ScrollArea::vertical()
                        .id_salt("kb_list")
                        .max_height(max_h)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            let mut last_group: Option<&str> = None;
                            for a in &list {
                                if last_group != Some(a.group.as_str()) && !a.group.is_empty() {
                                    ui.add_space(6.0);
                                    ui.label(
                                        egui::RichText::new(a.group.to_uppercase())
                                            .size(10.5)
                                            .strong()
                                            .color(ui.visuals().weak_text_color()),
                                    );
                                    last_group = Some(a.group.as_str());
                                }
                                let row = ui.add(
                                    egui::Button::new(
                                        egui::RichText::new(&a.title).strong(),
                                    )
                                    .frame(false),
                                );
                                if !a.summary.is_empty() {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&a.summary).small().weak(),
                                        )
                                        .wrap(),
                                    );
                                }
                                let n = sources_for(&a.id).len();
                                if n > 0 {
                                    ui.label(
                                        egui::RichText::new(ui_words.count(n))
                                            .size(10.5)
                                            .color(hue_of(&list, &a.group)),
                                    );
                                }
                                if row.clicked() {
                                    chosen = Some(a.id.clone());
                                }
                                ui.add_space(4.0);
                            }
                        });
                    if chosen.is_some() {
                        reader.current = chosen;
                    }
                }
            }
        });
    if !open {
        reader.show = false;
    }
}

fn source_item(ui: &mut egui::Ui, words: &UiText, s: &Source) {
    ui.label(egui::RichText::new(words.kind(&s.kind)).size(10.0).strong().weak());
    ui.add(egui::Hyperlink::from_label_and_url(egui::RichText::new(&s.title).strong().size(12.0), &s.href));
    let byline: Vec<&str> = [s.authors.as_str(), s.publisher.as_str(), s.year.as_str()]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect();
    if !byline.is_empty() {
        ui.add(egui::Label::new(egui::RichText::new(byline.join(" · ")).size(11.0).weak()).wrap());
    }
    ui.horizontal_wrapped(|ui| {
        if !s.pdf.is_empty() {
            ui.hyperlink_to(egui::RichText::new("⬇ PDF").size(11.0), library_url(&s.pdf))
                .on_hover_text(&s.licence);
        }
        let label = if s.pdf.is_empty() { words.read_source } else { words.publisher };
        ui.hyperlink_to(egui::RichText::new(format!("{label} ↗")).size(11.0), &s.href);
    });
    if !s.pdf.is_empty() && !s.licence.is_empty() {
        ui.add(egui::Label::new(egui::RichText::new(&s.licence).size(10.0).weak()).wrap());
    }
}

fn article_body(ui: &mut egui::Ui, body: &str) {
    for block in parse_body(body) {
        match block {
            Block::Heading(t) => {
                ui.add_space(6.0);
                let plain: String = inline_runs(&t).into_iter().map(|(_, r)| r).collect();
                ui.label(egui::RichText::new(plain).strong().size(13.5));
            }
            Block::Paragraph(t) => {
                let job = rich_line(ui, &t, 13.0);
                ui.add(egui::Label::new(job).wrap());
            }
            Block::Bullets(items) => list(ui, &items, |_| "•".to_owned()),
            Block::Steps(items) => list(ui, &items, |i| format!("{}.", i + 1)),
        }
        ui.add_space(4.0);
    }
}

fn list(ui: &mut egui::Ui, items: &[String], marker: impl Fn(usize) -> String) {
    for (i, item) in items.iter().enumerate() {
        ui.horizontal_top(|ui| {
            ui.add_sized(
                [16.0, 16.0],
                egui::Label::new(egui::RichText::new(marker(i)).size(13.0)),
            );
            let job = rich_line(ui, item, 13.0);
            ui.add(egui::Label::new(job).wrap());
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(code: &str) -> Vec<String> {
        articles(code).into_iter().map(|a| a.id).collect()
    }

    #[test]
    fn english_has_the_articles() {
        let en = articles("en");
        assert!((5..=8).contains(&en.len()), "{} articles", en.len());
        for a in &en {
            assert!(!a.id.is_empty() && !a.title.is_empty(), "{a:?}");
            assert!(!a.summary.is_empty() && !a.group.is_empty(), "{}", a.id);
            assert!(a.body.len() > 200, "{} has a stub body", a.id);
        }
    }

    #[test]
    fn every_language_has_the_same_articles_in_the_same_order() {
        let en = ids("en");
        for (code, _) in LANGUAGES {
            assert_eq!(ids(code), en, "{code}");
            // And its own text, not English under another name.
            if *code != "en" {
                let t: Vec<String> = articles(code).into_iter().map(|a| a.title).collect();
                let e: Vec<String> = articles("en").into_iter().map(|a| a.title).collect();
                assert_ne!(t, e, "{code} is untranslated");
            }
        }
    }

    #[test]
    fn every_language_has_three_groups_in_one_run_each() {
        for (code, _) in LANGUAGES {
            let mut runs: Vec<String> = Vec::new();
            for a in articles(code) {
                if runs.last() != Some(&a.group) {
                    runs.push(a.group);
                }
            }
            assert_eq!(runs.len(), 3, "{code}: {runs:?}");
        }
    }

    /// Nothing outside the closed mini-markdown: no HTML, links, code, images
    /// or deeper headings, and every `**` is paired.
    #[test]
    fn bodies_stay_inside_the_closed_syntax() {
        for (code, _) in LANGUAGES {
            for a in articles(code) {
                for bad in ["<", "](", "`", "![", "# ", "__"] {
                    for line in a.body.lines() {
                        let l = line.trim_start();
                        if bad == "# " {
                            assert!(
                                !l.starts_with("# ") && !l.starts_with("#### "),
                                "{code}/{}: {line}",
                                a.id
                            );
                        } else {
                            assert!(!line.contains(bad), "{code}/{}: {bad} in {line}", a.id);
                        }
                    }
                }
                assert_eq!(a.body.matches("**").count() % 2, 0, "{code}/{}", a.id);
                for block in parse_body(&a.body) {
                    let texts = match block {
                        Block::Heading(t) | Block::Paragraph(t) => vec![t],
                        Block::Bullets(v) | Block::Steps(v) => v,
                    };
                    for t in texts {
                        for (_, run) in inline_runs(&t) {
                            assert!(!run.contains("**"), "{code}/{}: stray ** in {t}", a.id);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_pin_is_never_said_to_be_synced() {
        // The one claim that must not drift in translation: the article on
        // signing in lists the PIN under what is NOT sent.
        let en = articles("en");
        let a = en.iter().find(|a| a.id == "signing-in").expect("signing-in");
        let not = a.body.split("## What is not").nth(1).expect("a 'What is not' section");
        assert!(not.contains("**The PIN.**"), "{not}");
    }

    #[test]
    fn every_source_belongs_to_an_article_and_links_over_https() {
        let all = parse_sources(SOURCES);
        assert!(all.len() >= 10, "{} sources", all.len());
        let ids = ids("en");
        for s in &all {
            assert!(ids.contains(&s.article), "unknown article {}", s.article);
            assert!(s.href.starts_with("https://"), "{}", s.href);
            if !s.pdf.is_empty() {
                assert!(s.pdf.starts_with("papers/") && s.pdf.ends_with(".pdf"), "{}", s.pdf);
                assert!(!s.licence.is_empty(), "{} has no licence", s.pdf);
            }
        }
    }

    #[test]
    fn parse_sources_matches_the_browser() {
        // The fixture knowledge.test.mjs uses for parseSources.
        let src = "note: x\n---\narticle: a\nkind: paper\ntitle: T: sub\nyear: 2001\nhref: https://e.org/x\n---\narticle: a\nkind: rumour\ntitle: no\nhref: https://e.org\n---\narticle: b\nkind: law\ntitle: L\nhref: javascript:alert(1)\n---\n";
        let s = parse_sources(src);
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].article.as_str(), s[0].title.as_str(), s[0].year.as_str()), ("a", "T: sub", "2001"));
    }

    #[test]
    fn guide_and_library_urls() {
        assert_eq!(guide_url("pt-BR", "the-pin"), "https://opensource.unisim.co.uk/kb/screens/pt-BR/the-pin.pdf");
        assert_eq!(guide_url("xx", "the-pin"), "https://opensource.unisim.co.uk/kb/screens/en/the-pin.pdf");
        assert_eq!(library_url("papers/x.pdf"), "https://opensource.unisim.co.uk/kb/papers/x.pdf");
        for (code, _) in LANGUAGES {
            let w = ui_text(code);
            assert!(w.count(3).contains('3') && !w.download.is_empty() && w.kinds.iter().all(|k| !k.is_empty()), "{code}");
        }
    }

    #[test]
    fn unknown_language_falls_back_to_english() {
        assert_eq!(ids("xx"), ids("en"));
    }

    #[test]
    fn parse_body_blocks() {
        let b = parse_body("Intro line\nwraps here.\n\n## Head\n- one\n- two\n  continued\n\n1. first\n2) second\n\nEnd.");
        assert_eq!(
            b,
            vec![
                Block::Paragraph("Intro line wraps here.".into()),
                Block::Heading("Head".into()),
                Block::Bullets(vec!["one".into(), "two continued".into()]),
                Block::Steps(vec!["first".into(), "second".into()]),
                Block::Paragraph("End.".into()),
            ]
        );
    }

    #[test]
    fn inline_runs_bold_and_literal() {
        assert_eq!(
            inline_runs("a **b** c **d**"),
            vec![(false, "a "), (true, "b"), (false, " c "), (true, "d")]
        );
        assert_eq!(inline_runs("no bold"), vec![(false, "no bold")]);
        assert_eq!(inline_runs("lone ** here"), vec![(false, "lone ** here")]);
        assert_eq!(inline_runs("**x**"), vec![(true, "x")]);
    }

    #[test]
    fn bundle_ignores_the_note_and_tolerates_crlf() {
        let src = "note: not an article\r\n\r\n---\r\nid: a\r\ntitle: T\r\n---\r\nBody\r\n";
        let a = parse_bundle(src);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].id, "a");
        assert_eq!(a[0].body, "Body");
    }
}
