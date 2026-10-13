package com.universalsim.extender

import android.content.Context
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * The knowledge base — short articles on what Screens is, how a connection
 * works and what leaves your network — and its reader, under Advanced.
 *
 * ⚠️ **The articles are not in this app's sources.** They are
 * `apps/web/knowledge/<language>.md`, the one copy every client reads: the
 * browser client fetches it, the desktop hosts embed it
 * (`crates/host-ui/src/knowledge.rs`), and this app packs the same folder as
 * assets (`app/build.gradle.kts`, `assets.srcDir`). So the files sit at the
 * root of the APK's assets as `en.md`, `fr.md` and so on.
 *
 * The bundle and body formats, and this parser, are kept in step with
 * `apps/web/src/knowledge.js` and the Rust port; host-ui's tests check the data
 * for all three. Bodies are the suite's closed mini-markdown, rendered here as
 * Text composables — nothing is ever interpreted as HTML.
 */
object Knowledge {
    data class Article(val id: String, val group: String, val title: String, val summary: String, val body: String)

    /** A paper, standard or guidance an article draws on (`sources.md`). */
    data class Source(
        val article: String, val kind: String, val title: String, val authors: String,
        val publisher: String, val year: String, val href: String, val pdf: String, val licence: String,
    )

    /** Where the suite's knowledge-base PDFs are served (the opensource portal). */
    const val LIBRARY_URL = "https://opensource.unisim.co.uk/kb"

    /** The printed guide for an article in a language (English for anything unknown). */
    fun guideUrl(code: String, id: String): String {
        val lang = if (LANGUAGES.any { it.first == code }) code else "en"
        return "$LIBRARY_URL/screens/$lang/$id.pdf"
    }

    /** A source's hosted copy (`papers/…`) as a URL. */
    fun libraryUrl(path: String): String =
        if (path.startsWith("https://") || path.startsWith("http://")) path else "$LIBRARY_URL/${path.trimStart('/')}"

    private val KINDS = listOf("paper", "standard", "report", "guidance", "law")

    /**
     * Split `sources.md` into sources, in file order. The same rules as
     * `parseSources` in apps/web/src/knowledge.js and `parse_sources` in the
     * Rust port: one source per block of `key: value` lines closed by `---`;
     * a block without an article, title, known kind or http(s) link is dropped.
     */
    fun parseSources(src: String): List<Source> {
        val lines = src.replace("\r\n", "\n").replace('\r', '\n').split('\n')
        var i = 0
        while (i < lines.size && lines[i].trim() != "---") i++
        i++
        val out = mutableListOf<Source>()
        var cur = HashMap<String, String>()
        fun flush() {
            val href = cur["href"].orEmpty()
            if (!cur["article"].isNullOrEmpty() && !cur["title"].isNullOrEmpty() && cur["kind"] in KINDS &&
                (href.startsWith("https://") || href.startsWith("http://"))
            ) {
                out += Source(
                    cur["article"]!!, cur["kind"]!!, cur["title"]!!, cur["authors"].orEmpty(), cur["publisher"].orEmpty(),
                    cur["year"].orEmpty(), href, cur["pdf"].orEmpty(), cur["licence"].orEmpty(),
                )
            }
            cur = HashMap()
        }
        while (i < lines.size) {
            val line = lines[i++]
            if (line.trim() == "---") { flush(); continue }
            val at = line.indexOf(':')
            if (at > 0) cur[line.substring(0, at).trim()] = line.substring(at + 1).trim()
        }
        flush()
        return out
    }

    /** Every source, read from the APK's assets. Empty on failure. */
    fun sources(context: Context): List<Source> = runCatching {
        context.assets.open("sources.md").bufferedReader(Charsets.UTF_8).use { parseSources(it.readText()) }
    }.getOrDefault(emptyList())

    sealed interface Block {
        data class Heading(val text: String) : Block
        data class Paragraph(val text: String) : Block
        data class Bullets(val items: MutableList<String>) : Block
        data class Steps(val items: MutableList<String>) : Block
    }

    /** The suite's eight knowledge-base languages, as (code, own name). */
    val LANGUAGES = listOf(
        "en" to "English",
        "fr" to "Français",
        "es" to "Español",
        "it" to "Italiano",
        "de" to "Deutsch",
        "pt-BR" to "Português (Brasil)",
        "pt-PT" to "Português (Portugal)",
        "tr" to "Türkçe",
    )

    /** The reader's own words — the SDK's kb.* strings. */
    data class Words(
        val title: String, val back: String, val close: String, val language: String, val failed: String,
        val download: String, val sources: String, val publisher: String, val readSource: String,
        val previous: String, val next: String, val sourceOne: String, val sourcesMany: String,
        /** paper, standard, report, guidance, law. */
        val kinds: List<String>,
    ) {
        fun count(n: Int) = if (n == 1) sourceOne else sourcesMany.replace("{n}", n.toString())
        fun kind(k: String) = KINDS.indexOf(k).let { if (it < 0) "" else kinds[it] }
    }

    fun words(code: String): Words = when (code) {
        "fr" -> Words("Base de connaissances", "Tous les articles", "Fermer", "Langue", "Impossible de charger les articles.",
            "Télécharger ce guide (PDF)", "Recherches et sources", "Éditeur", "Lire à la source", "Précédent", "Suivant", "1 source", "{n} sources",
            listOf("Article de recherche", "Norme", "Rapport", "Recommandations", "Législation"))
        "es" -> Words("Base de conocimientos", "Todos los artículos", "Cerrar", "Idioma", "No se han podido cargar los artículos.",
            "Descargar esta guía (PDF)", "Investigación y fuentes", "Editor", "Leer en la fuente", "Anterior", "Siguiente", "1 fuente", "{n} fuentes",
            listOf("Artículo de investigación", "Norma", "Informe", "Guía", "Legislación"))
        "it" -> Words("Knowledge base", "Tutti gli articoli", "Chiudi", "Lingua", "Impossibile caricare gli articoli.",
            "Scarica questa guida (PDF)", "Ricerche e fonti", "Editore", "Leggi alla fonte", "Precedente", "Successivo", "1 fonte", "{n} fonti",
            listOf("Articolo di ricerca", "Standard", "Rapporto", "Linee guida", "Normativa"))
        "de" -> Words("Wissensdatenbank", "Alle Artikel", "Schließen", "Sprache", "Die Artikel konnten nicht geladen werden.",
            "Diese Anleitung herunterladen (PDF)", "Forschung und Quellen", "Herausgeber", "An der Quelle lesen", "Zurück", "Weiter", "1 Quelle", "{n} Quellen",
            listOf("Fachartikel", "Norm", "Bericht", "Leitfaden", "Rechtsvorschrift"))
        "pt-BR" -> Words("Base de conhecimento", "Todos os artigos", "Fechar", "Idioma", "Não foi possível carregar os artigos.",
            "Baixar este guia (PDF)", "Pesquisas e fontes", "Editora", "Ler na fonte", "Anterior", "Próximo", "1 fonte", "{n} fontes",
            listOf("Artigo científico", "Norma", "Relatório", "Orientações", "Legislação"))
        "pt-PT" -> Words("Base de conhecimento", "Todos os artigos", "Fechar", "Idioma", "Não foi possível carregar os artigos.",
            "Transferir este guia (PDF)", "Investigação e fontes", "Editor", "Ler na fonte", "Anterior", "Seguinte", "1 fonte", "{n} fontes",
            listOf("Artigo de investigação", "Norma", "Relatório", "Orientações", "Legislação"))
        "tr" -> Words("Bilgi bankası", "Tüm makaleler", "Kapat", "Dil", "Makaleler yüklenemedi.",
            "Bu rehberi indir (PDF)", "Araştırma ve kaynaklar", "Yayıncı", "Kaynağında oku", "Önceki", "Sonraki", "1 kaynak", "{n} kaynak",
            listOf("Araştırma makalesi", "Standart", "Rapor", "Kılavuz", "Mevzuat"))
        else -> Words("Knowledge base", "All articles", "Close", "Language", "The articles couldn't be loaded.",
            "Download this guide (PDF)", "Research and sources", "Publisher", "Read at source", "Previous", "Next", "1 source", "{n} sources",
            listOf("Research paper", "Standard", "Report", "Guidance", "Law"))
    }

    private const val PREFS = "knowledge"
    private const val KEY_LANGUAGE = "language"

    /** The saved choice, else the device's language, else English. */
    fun language(context: Context): String {
        val saved = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY_LANGUAGE, null)
        if (saved != null && LANGUAGES.any { it.first == saved }) return saved
        val locales = context.resources.configuration.locales
        for (i in 0 until locales.size()) {
            val l = locales[i]
            if (l.language == "pt") return if (l.country == "BR") "pt-BR" else "pt-PT"
            LANGUAGES.firstOrNull { it.first == l.language }?.let { return it.first }
        }
        return "en"
    }

    fun saveLanguage(context: Context, code: String) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString(KEY_LANGUAGE, code).apply()
    }

    /** Articles for a language, read from the APK's assets. Empty on failure. */
    fun articles(context: Context, code: String): List<Article> {
        val lang = if (LANGUAGES.any { it.first == code }) code else "en"
        return runCatching {
            context.assets.open("$lang.md").bufferedReader(Charsets.UTF_8).use { parseBundle(it.readText()) }
        }.getOrDefault(emptyList())
    }

    /** Split a bundle into articles. See `apps/web/src/knowledge.js` for the format. */
    fun parseBundle(src: String): List<Article> {
        val lines = src.replace("\r\n", "\n").replace('\r', '\n').split('\n')
        var i = 0
        while (i < lines.size && lines[i].trim() != "---") i++ // the translators' note
        i++
        val out = mutableListOf<Article>()
        while (i < lines.size) {
            val fields = HashMap<String, String>()
            var closed = false
            while (i < lines.size) {
                val line = lines[i++]
                if (line.trim() == "---") { closed = true; break }
                val at = line.indexOf(':')
                if (at > 0) fields[line.substring(0, at).trim()] = line.substring(at + 1).trim()
            }
            if (fields.isEmpty() || !closed) break
            val body = StringBuilder()
            while (i < lines.size) {
                val line = lines[i++]
                if (line.trim() == "---") break
                body.append(line).append('\n')
            }
            out += Article(
                id = fields["id"].orEmpty(),
                group = fields["group"].orEmpty(),
                title = fields["title"].orEmpty(),
                summary = fields["summary"].orEmpty(),
                body = body.toString().trim(),
            )
        }
        return out
    }

    private val HEADING = Regex("^#{2,3}\\s+(.*)$")
    private val BULLET = Regex("^[-•*]\\s+(.*)$")
    private val STEP = Regex("^\\d+[.)]\\s+(.*)$")

    /** Split a body into blocks. A port of the SDK's parseArticleBody. */
    fun parseBody(body: String): List<Block> {
        val blocks = mutableListOf<Block>()
        val para = mutableListOf<String>()
        fun flush() {
            if (para.isNotEmpty()) blocks += Block.Paragraph(para.joinToString(" "))
            para.clear()
        }
        for (raw in body.replace("\r\n", "\n").split('\n')) {
            val line = raw.trim()
            if (line.isEmpty()) { flush(); continue }
            val h = HEADING.find(line)
            if (h != null) {
                flush()
                blocks += Block.Heading(h.groupValues[1])
                continue
            }
            val ul = BULLET.find(line)
            val ol = STEP.find(line)
            if (ul != null || ol != null) {
                flush()
                val text = (ul ?: ol)!!.groupValues[1]
                val last = blocks.lastOrNull()
                when {
                    ul != null && last is Block.Bullets -> last.items += text
                    ol != null && last is Block.Steps -> last.items += text
                    ul != null -> blocks += Block.Bullets(mutableListOf(text))
                    else -> blocks += Block.Steps(mutableListOf(text))
                }
                continue
            }
            // A wrapped, indented line directly under a list item continues it.
            val last = blocks.lastOrNull()
            if (para.isEmpty() && raw.firstOrNull()?.isWhitespace() == true) {
                val items = (last as? Block.Bullets)?.items ?: (last as? Block.Steps)?.items
                if (items != null && items.isNotEmpty()) {
                    items[items.lastIndex] = items.last() + " " + line
                    continue
                }
            }
            para += line
        }
        flush()
        return blocks
    }

    private val BOLD = Regex("\\*\\*[^*]+\\*\\*")

    /** `**bold**` runs as bold spans; everything else stays literal. */
    fun styled(text: String): AnnotatedString = buildAnnotatedString {
        var at = 0
        for (m in BOLD.findAll(text)) {
            append(text.substring(at, m.range.first))
            withStyle(SpanStyle(fontWeight = FontWeight.SemiBold)) { append(m.value.substring(2, m.value.length - 2)) }
            at = m.range.last + 1
        }
        append(text.substring(at))
    }
}

/**
 * Advanced ▸ Knowledge base. The same behaviour as the suite's other readers:
 * a list grouped by heading, then one article; the system Back gesture (and
 * the back button) steps out of an article before it closes the dialog.
 * Styled as the About dialog beside it, so it follows the app's Appearance.
 */
@Composable
fun KnowledgeBaseDialog(onDismiss: () -> Unit) {
    val context = LocalContext.current
    var language by remember { mutableStateOf(Knowledge.language(context)) }
    var articles by remember { mutableStateOf<List<Knowledge.Article>?>(null) }
    var current by remember { mutableStateOf<String?>(null) }
    var pickerOpen by remember { mutableStateOf(false) }
    val sources = remember { Knowledge.sources(context) }
    val uri = LocalUriHandler.current
    val words = Knowledge.words(language)
    val scroll = rememberScrollState()
    val maxHeight = (LocalConfiguration.current.screenHeightDp * 0.6f).dp

    LaunchedEffect(language) { articles = Knowledge.articles(context, language) }
    LaunchedEffect(current) { scroll.scrollTo(0) }

    val article = current?.let { id -> articles?.firstOrNull { it.id == id } }

    AlertDialog(
        // Back (or a tap outside) steps out of an article first, like Escape
        // in the web readers.
        onDismissRequest = { if (current != null) current = null else onDismiss() },
        confirmButton = { TextButton(onClick = onDismiss) { Text(words.close) } },
        title = {
            Column {
                if (article != null) {
                    Text(
                        "‹  ${words.back}",
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.primary,
                        modifier = Modifier
                            .clip(RoundedCornerShape(6.dp))
                            .clickable { current = null }
                            .padding(vertical = 4.dp),
                    )
                }
                Text(article?.title ?: words.title)
            }
        },
        text = {
            Column(
                modifier = Modifier.heightIn(max = maxHeight).verticalScroll(scroll),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                if (article != null) {
                    if (article.summary.isNotEmpty()) {
                        Text(article.summary, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    OutlinedButton(onClick = { uri.openUri(Knowledge.guideUrl(language, article.id)) }) {
                        Text("⬇  ${words.download}")
                    }
                    ArticleBody(article.body)
                    val mine = sources.filter { it.article == article.id }
                    if (mine.isNotEmpty()) {
                        Text(
                            words.sources.uppercase(),
                            fontSize = 11.sp,
                            fontWeight = FontWeight.Bold,
                            letterSpacing = 0.08.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.padding(top = 10.dp),
                        )
                        mine.forEach { SourceItem(words, it) { url -> uri.openUri(url) } }
                    }
                    val list = articles.orEmpty()
                    val i = list.indexOfFirst { it.id == article.id }
                    val prev = list.getOrNull(i - 1)
                    val next = list.getOrNull(i + 1)
                    Row(modifier = Modifier.fillMaxWidth().padding(top = 8.dp), horizontalArrangement = Arrangement.SpaceBetween) {
                        if (prev != null) TextButton(onClick = { current = prev.id }) { Text("‹ ${words.previous}") } else Box {}
                        if (next != null) TextButton(onClick = { current = next.id }) { Text("${words.next} ›") }
                    }
                } else {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            words.language,
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Box(modifier = Modifier.padding(start = 8.dp)) {
                            OutlinedButton(onClick = { pickerOpen = true }) {
                                Text(Knowledge.LANGUAGES.first { it.first == language }.second)
                            }
                            DropdownMenu(expanded = pickerOpen, onDismissRequest = { pickerOpen = false }) {
                                Knowledge.LANGUAGES.forEach { (code, name) ->
                                    DropdownMenuItem(text = { Text(name) }, onClick = {
                                        pickerOpen = false
                                        language = code
                                        Knowledge.saveLanguage(context, code)
                                    })
                                }
                            }
                        }
                    }
                    val list = articles
                    if (list != null && list.isEmpty()) {
                        Text(words.failed, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    var lastGroup: String? = null
                    list?.forEach { a ->
                        if (a.group.isNotEmpty() && a.group != lastGroup) {
                            lastGroup = a.group
                            Text(
                                a.group.uppercase(),
                                fontSize = 11.sp,
                                fontWeight = FontWeight.Bold,
                                letterSpacing = 0.08.sp,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                modifier = Modifier.padding(top = 6.dp),
                            )
                        }
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(8.dp))
                                .clickable { current = a.id }
                                .padding(vertical = 6.dp),
                        ) {
                            Column(modifier = Modifier.weight(1f)) {
                                Text(a.title, style = MaterialTheme.typography.titleSmall)
                                if (a.summary.isNotEmpty()) {
                                    Text(
                                        a.summary,
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    )
                                }
                                val n = sources.count { it.article == a.id }
                                if (n > 0) {
                                    Text(
                                        words.count(n),
                                        style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.primary,
                                    )
                                }
                            }
                            Text(
                                "›",
                                style = MaterialTheme.typography.titleLarge,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                modifier = Modifier.padding(start = 8.dp),
                            )
                        }
                    }
                }
            }
        },
    )
}

@Composable
private fun SourceItem(words: Knowledge.Words, s: Knowledge.Source, open: (String) -> Unit) {
    Column(modifier = Modifier.padding(top = 4.dp)) {
        Text(words.kind(s.kind), fontSize = 10.sp, fontWeight = FontWeight.Bold, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(
            s.title,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier.clip(RoundedCornerShape(4.dp)).clickable { open(s.href) },
        )
        val byline = listOf(s.authors, s.publisher, s.year).filter { it.isNotEmpty() }.joinToString(" · ")
        if (byline.isNotEmpty()) {
            Text(byline, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Row {
            if (s.pdf.isNotEmpty()) {
                TextButton(onClick = { open(Knowledge.libraryUrl(s.pdf)) }) { Text("⬇ PDF") }
            }
            TextButton(onClick = { open(s.href) }) { Text("${if (s.pdf.isEmpty()) words.readSource else words.publisher} ↗") }
        }
        if (s.pdf.isNotEmpty() && s.licence.isNotEmpty()) {
            Text(s.licence, fontSize = 10.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@Composable
private fun ArticleBody(body: String) {
    for (block in Knowledge.parseBody(body)) {
        when (block) {
            is Knowledge.Block.Heading ->
                Text(
                    Knowledge.styled(block.text),
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.padding(top = 4.dp),
                )
            is Knowledge.Block.Paragraph -> Text(Knowledge.styled(block.text), style = MaterialTheme.typography.bodyMedium)
            is Knowledge.Block.Bullets -> block.items.forEach { ListItem("•", it) }
            is Knowledge.Block.Steps -> block.items.forEachIndexed { i, it -> ListItem("${i + 1}.", it) }
        }
    }
}

@Composable
private fun ListItem(marker: String, text: String) {
    Row {
        Text(marker, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.width(20.dp))
        Text(Knowledge.styled(text), style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
    }
}
