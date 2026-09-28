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
}

pub fn ui_text(code: &str) -> UiText {
    let (title, back, language) = match code {
        "fr" => ("Base de connaissances", "Tous les articles", "Langue"),
        "es" => ("Base de conocimientos", "Todos los artículos", "Idioma"),
        "it" => ("Knowledge base", "Tutti gli articoli", "Lingua"),
        "de" => ("Wissensdatenbank", "Alle Artikel", "Sprache"),
        "pt-BR" => ("Base de conhecimento", "Todos os artigos", "Idioma"),
        "pt-PT" => ("Base de conhecimento", "Todos os artigos", "Idioma"),
        "tr" => ("Bilgi bankası", "Tüm makaleler", "Dil"),
        _ => ("Knowledge base", "All articles", "Language"),
    };
    UiText { title, back, language }
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
            ui.set_width(380.0);
            let max_h = (ctx.screen_rect().height() - 140.0).max(200.0);
            let current = reader.current.clone();
            let article = current.and_then(|id| reader.list().iter().find(|a| a.id == id).cloned());
            match article {
                Some(a) => {
                    if ui.link(format!("‹ {}", ui_words.back)).clicked() {
                        reader.current = None;
                    }
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(&a.title).strong().size(15.0));
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .id_salt(("kb_article", a.id.as_str()))
                        .max_height(max_h)
                        .auto_shrink([false, true])
                        .show(ui, |ui| article_body(ui, &a.body));
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
