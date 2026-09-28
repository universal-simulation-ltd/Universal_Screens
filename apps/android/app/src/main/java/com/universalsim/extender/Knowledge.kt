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
    data class Words(val title: String, val back: String, val close: String, val language: String, val failed: String)

    fun words(code: String): Words = when (code) {
        "fr" -> Words("Base de connaissances", "Tous les articles", "Fermer", "Langue", "Impossible de charger les articles.")
        "es" -> Words("Base de conocimientos", "Todos los artículos", "Cerrar", "Idioma", "No se han podido cargar los artículos.")
        "it" -> Words("Knowledge base", "Tutti gli articoli", "Chiudi", "Lingua", "Impossibile caricare gli articoli.")
        "de" -> Words("Wissensdatenbank", "Alle Artikel", "Schließen", "Sprache", "Die Artikel konnten nicht geladen werden.")
        "pt-BR" -> Words("Base de conhecimento", "Todos os artigos", "Fechar", "Idioma", "Não foi possível carregar os artigos.")
        "pt-PT" -> Words("Base de conhecimento", "Todos os artigos", "Fechar", "Idioma", "Não foi possível carregar os artigos.")
        "tr" -> Words("Bilgi bankası", "Tüm makaleler", "Kapat", "Dil", "Makaleler yüklenemedi.")
        else -> Words("Knowledge base", "All articles", "Close", "Language", "The articles couldn't be loaded.")
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
                    ArticleBody(article.body)
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
