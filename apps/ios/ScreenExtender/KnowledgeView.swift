import SwiftUI

/// The knowledge base — short articles on what Screens is, how a connection
/// works and what leaves your network — and its reader, under Advanced.
///
/// ⚠️ **The articles are not in this app's sources.** They are
/// `apps/web/knowledge/<language>.md`, the one copy every client reads: the
/// browser client fetches it, the desktop hosts embed it
/// (`crates/host-ui/src/knowledge.rs`), the Android app packs the folder as
/// assets, and this app copies the same folder into its bundle as a folder
/// reference (`project.yml`, `../web/knowledge`). So the files sit in the app
/// bundle at `knowledge/en.md`, `knowledge/fr.md` and so on.
///
/// The bundle and body formats, and this parser, are kept in step with
/// `apps/web/src/knowledge.js`, the Android client's `Knowledge.kt` and the Rust
/// port; host-ui's tests check the data for all of them. Bodies are the suite's
/// closed mini-markdown, rendered here as `Text` views — nothing is ever
/// interpreted as markup beyond `**bold**`.
enum Knowledge {
    struct Article: Identifiable, Hashable {
        let id: String
        let group: String
        let title: String
        let summary: String
        let body: String
    }

    enum Block {
        case heading(String)
        case paragraph(String)
        case bullets([String])
        case steps([String])
    }

    /// The suite's eight knowledge-base languages, as (code, own name).
    static let languages: [(code: String, name: String)] = [
        ("en", "English"),
        ("fr", "Français"),
        ("es", "Español"),
        ("it", "Italiano"),
        ("de", "Deutsch"),
        ("pt-BR", "Português (Brasil)"),
        ("pt-PT", "Português (Portugal)"),
        ("tr", "Türkçe"),
    ]

    /// The reader's own words — the SDK's kb.* strings. The app itself is
    /// English-only, so the reader carries its own language picker.
    struct Words {
        let title: String, back: String, close: String, language: String, failed: String
    }

    static func words(_ code: String) -> Words {
        switch code {
        case "fr": Words(title: "Base de connaissances", back: "Tous les articles", close: "Fermer", language: "Langue", failed: "Impossible de charger les articles.")
        case "es": Words(title: "Base de conocimientos", back: "Todos los artículos", close: "Cerrar", language: "Idioma", failed: "No se han podido cargar los artículos.")
        case "it": Words(title: "Knowledge base", back: "Tutti gli articoli", close: "Chiudi", language: "Lingua", failed: "Impossibile caricare gli articoli.")
        case "de": Words(title: "Wissensdatenbank", back: "Alle Artikel", close: "Schließen", language: "Sprache", failed: "Die Artikel konnten nicht geladen werden.")
        case "pt-BR", "pt-PT": Words(title: "Base de conhecimento", back: "Todos os artigos", close: "Fechar", language: "Idioma", failed: "Não foi possível carregar os artigos.")
        case "tr": Words(title: "Bilgi bankası", back: "Tüm makaleler", close: "Kapat", language: "Dil", failed: "Makaleler yüklenemedi.")
        default: Words(title: "Knowledge base", back: "All articles", close: "Close", language: "Language", failed: "The articles couldn't be loaded.")
        }
    }

    /// The UserDefaults key for the reader's language. Empty means "not chosen
    /// yet", which follows the device.
    static let languageKey = "kb_language"

    /// The saved choice, else the device's language, else English.
    static func language(saved: String) -> String {
        if languages.contains(where: { $0.code == saved }) { return saved }
        for raw in Locale.preferredLanguages {
            let tag = raw.lowercased()
            if tag.hasPrefix("pt") { return tag.hasPrefix("pt-br") ? "pt-BR" : "pt-PT" }
            let base = String(tag.split(separator: "-").first ?? "")
            if let hit = languages.first(where: { $0.code == base }) { return hit.code }
        }
        return "en"
    }

    /// Articles for a language, read from the app bundle. Empty on failure.
    static func articles(_ code: String) -> [Article] {
        let lang = languages.contains(where: { $0.code == code }) ? code : "en"
        guard let url = Bundle.main.url(forResource: lang, withExtension: "md", subdirectory: "knowledge"),
              let src = try? String(contentsOf: url, encoding: .utf8) else { return [] }
        return parseBundle(src)
    }

    /// Split a bundle into articles. See `apps/web/src/knowledge.js` for the format.
    static func parseBundle(_ src: String) -> [Article] {
        let lines = src.replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
            .components(separatedBy: "\n")
        var i = 0
        while i < lines.count && lines[i].trimmingCharacters(in: .whitespaces) != "---" { i += 1 } // the translators' note
        i += 1
        var out: [Article] = []
        while i < lines.count {
            var fields: [String: String] = [:]
            var closed = false
            while i < lines.count {
                let line = lines[i]; i += 1
                if line.trimmingCharacters(in: .whitespaces) == "---" { closed = true; break }
                if let at = line.firstIndex(of: ":"), at != line.startIndex {
                    let key = line[..<at].trimmingCharacters(in: .whitespaces)
                    fields[key] = line[line.index(after: at)...].trimmingCharacters(in: .whitespaces)
                }
            }
            if fields.isEmpty || !closed { break }
            var body: [String] = []
            while i < lines.count {
                let line = lines[i]; i += 1
                if line.trimmingCharacters(in: .whitespaces) == "---" { break }
                body.append(line)
            }
            out.append(Article(
                id: fields["id"] ?? "",
                group: fields["group"] ?? "",
                title: fields["title"] ?? "",
                summary: fields["summary"] ?? "",
                body: body.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
            ))
        }
        return out
    }

    /// Split a body into blocks. A port of the SDK's parseArticleBody.
    static func parseBody(_ body: String) -> [Block] {
        var blocks: [Block] = []
        var para: [String] = []
        func flush() {
            if !para.isEmpty { blocks.append(.paragraph(para.joined(separator: " "))) }
            para.removeAll()
        }
        for raw in body.replacingOccurrences(of: "\r\n", with: "\n").components(separatedBy: "\n") {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.isEmpty { flush(); continue }
            if let m = line.wholeMatch(of: /#{2,3}\s+(.*)/) {
                flush()
                blocks.append(.heading(String(m.1)))
                continue
            }
            let ul = line.wholeMatch(of: /[-•*]\s+(.*)/).map { String($0.1) }
            let ol = ul == nil ? line.wholeMatch(of: /\d+[.)]\s+(.*)/).map { String($0.1) } : nil
            if let text = ul ?? ol {
                flush()
                switch (blocks.last, ul != nil) {
                case (.bullets(var items), true):
                    items.append(text); blocks[blocks.count - 1] = .bullets(items)
                case (.steps(var items), false):
                    items.append(text); blocks[blocks.count - 1] = .steps(items)
                case (_, true): blocks.append(.bullets([text]))
                case (_, false): blocks.append(.steps([text]))
                }
                continue
            }
            // A wrapped, indented line directly under a list item continues it.
            if para.isEmpty, raw.first?.isWhitespace == true {
                switch blocks.last {
                case .bullets(var items) where !items.isEmpty:
                    items[items.count - 1] += " " + line; blocks[blocks.count - 1] = .bullets(items); continue
                case .steps(var items) where !items.isEmpty:
                    items[items.count - 1] += " " + line; blocks[blocks.count - 1] = .steps(items); continue
                default: break
                }
            }
            para.append(line)
        }
        flush()
        return blocks
    }

    /// `**bold**` runs as bold spans; everything else stays literal.
    static func styled(_ text: String) -> AttributedString {
        var out = AttributedString()
        var rest = Substring(text)
        while let m = rest.firstMatch(of: /\*\*([^*]+)\*\*/) {
            out += AttributedString(String(rest[..<m.range.lowerBound]))
            var bold = AttributedString(String(m.1))
            bold.inlinePresentationIntent = .stronglyEmphasized
            out += bold
            rest = rest[m.range.upperBound...]
        }
        out += AttributedString(String(rest))
        return out
    }
}

/// Advanced ▸ Knowledge base. The same behaviour as the suite's other readers:
/// a list grouped by heading, then one article (pushed, so the back button and
/// the edge swipe step out of an article before Close shuts the sheet). Styled
/// as the About sheet beside it, so it follows the app's Appearance.
struct KnowledgeView: View {
    @Environment(\.dismiss) private var dismiss
    @AppStorage(Knowledge.languageKey) private var savedLanguage = ""
    @State private var path: [Knowledge.Article] = []

    private var language: String { Knowledge.language(saved: savedLanguage) }

    var body: some View {
        let words = Knowledge.words(language)
        let articles = Knowledge.articles(language)
        NavigationStack(path: $path) {
            ScrollView {
                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Label(words.language, systemImage: "globe")
                            .font(.subheadline)
                        Spacer()
                        Picker(words.language, selection: Binding(
                            get: { language },
                            set: { savedLanguage = $0 }
                        )) {
                            ForEach(Knowledge.languages, id: \.code) { lang in
                                Text(lang.name).tag(lang.code)
                            }
                        }
                        .pickerStyle(.menu)
                        .tint(Color.brandOrange)
                    }

                    if articles.isEmpty {
                        Text(words.failed)
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .padding(.top, 10)
                    }
                    ForEach(Array(articles.enumerated()), id: \.element.id) { index, article in
                        if !article.group.isEmpty && (index == 0 || articles[index - 1].group != article.group) {
                            section(article.group)
                        }
                        NavigationLink(value: article) { row(article) }
                            .buttonStyle(.plain)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(20)
            }
            // The connect screen's own backdrop, so the rows read as its cards
            // in light and dark alike.
            .background(Color(.systemGroupedBackground))
            .navigationTitle(words.title)
            .navigationBarTitleDisplayMode(.inline)
            .navigationDestination(for: Knowledge.Article.self) { article in
                ArticleView(article: article, close: words.close) { dismiss() }
            }
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(words.close) { dismiss() }
                }
            }
        }
        // A language change swaps the article set; an open article in the old
        // language would no longer match any row, so step back to the list.
        .onChange(of: savedLanguage) { _, _ in path.removeAll() }
        #if DEBUG
        .onAppear {
            let id = UserDefaults.standard.string(forKey: Self.debugOpenKey)
            if let article = articles.first(where: { $0.id == id }) { path = [article] }
        }
        #endif
    }

    /// Debug builds only: the launch argument (`-openKnowledgeBase list`, or
    /// `-openKnowledgeBase <article id>`) that opens the reader at start — see
    /// ConnectView. `-kb_language fr` picks the language the same way, through
    /// UserDefaults' argument domain.
    static let debugOpenKey = "openKnowledgeBase"

    /// The small uppercase heading About's sections use.
    private func section(_ label: String) -> some View {
        Text(label.uppercased())
            .font(.caption2.weight(.bold))
            .kerning(0.8)
            .foregroundStyle(.secondary)
            .padding(.top, 14)
    }

    private func row(_ article: Knowledge.Article) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(article.title)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.primary)
                if !article.summary.isEmpty {
                    Text(article.summary)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .multilineTextAlignment(.leading)
            Spacer(minLength: 0)
            Image(systemName: "chevron.right")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.tertiary)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .contentShape(Rectangle())
    }
}

/// One article: its body's blocks, top to bottom.
private struct ArticleView: View {
    let article: Knowledge.Article
    let close: String
    let onClose: () -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 10) {
                Text(article.title)
                    .font(.title3.weight(.bold))
                ForEach(Array(Knowledge.parseBody(article.body).enumerated()), id: \.offset) { _, block in
                    switch block {
                    case .heading(let text):
                        Text(Knowledge.styled(text))
                            .font(.headline)
                            .padding(.top, 6)
                    case .paragraph(let text):
                        Text(Knowledge.styled(text))
                    case .bullets(let items):
                        ForEach(Array(items.enumerated()), id: \.offset) { _, item in listItem("•", item) }
                    case .steps(let items):
                        ForEach(Array(items.enumerated()), id: \.offset) { i, item in listItem("\(i + 1).", item) }
                    }
                }
            }
            .font(.subheadline)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(20)
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                Button(close, action: onClose)
            }
        }
    }

    private func listItem(_ marker: String, _ text: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Text(marker)
                .frame(width: 20, alignment: .leading)
            Text(Knowledge.styled(text))
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
