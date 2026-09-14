package com.universalsim.extender

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp

/**
 * Shared brand styling for the Android client, kept at parity with the iOS app's
 * `Theme.swift`. The whole UI is tinted with UNI·SIM brand orange (#E05504); dark
 * mode sits the app on the brand slate the app icon uses (#0F172A). Which of the
 * two is drawn is the user's Appearance choice — light unless they pick otherwise.
 */

/** UNI·SIM brand orange (#E05504) — the global accent, like iOS's AccentColor. */
val BrandOrange = Color(0xFFE05504)

/** The dark slate the app icon sits on — the dark-mode backdrop. */
val BrandSlate = Color(0xFF0F172A)

private val LightColors = lightColorScheme(
    primary = BrandOrange,
    onPrimary = Color.White,
    primaryContainer = Color(0xFFFFE2D1),
    onPrimaryContainer = Color(0xFF4A1B00),
    secondary = Color(0xFF6F5B4E),
    onSecondary = Color.White,
    tertiary = Color(0xFF3B6E4F), // "Drop" (drag-lock) affirmative button
    onTertiary = Color.White,
    background = Color(0xFFF2F2F7), // iOS systemGroupedBackground
    onBackground = Color(0xFF1A1C1E),
    surface = Color.White,
    onSurface = Color(0xFF1A1C1E),
    surfaceVariant = Color(0xFFECE6E1), // trackpad pad / card tints
    onSurfaceVariant = Color(0xFF4E4640),
    outline = Color(0xFF80766E),
    error = Color(0xFFBA1A1A),
    onError = Color.White,
)

private val DarkColors = darkColorScheme(
    primary = Color(0xFFFF8A4D),
    onPrimary = Color(0xFF4A1B00),
    primaryContainer = Color(0xFF7A3A12),
    onPrimaryContainer = Color(0xFFFFE2D1),
    secondary = Color(0xFFD8C2B4),
    onSecondary = Color(0xFF3B2A1C),
    tertiary = Color(0xFFA6D6B8),
    onTertiary = Color(0xFF0C3A24),
    background = BrandSlate,
    onBackground = Color(0xFFE6E1E5),
    surface = Color(0xFF1A2436),
    onSurface = Color(0xFFE6E1E5),
    surfaceVariant = Color(0xFF2A3345),
    onSurfaceVariant = Color(0xFFCFC7BF),
    outline = Color(0xFF998F86),
    error = Color(0xFFFFB4AB),
    onError = Color(0xFF690005),
)

/**
 * The Appearance choice under Advanced — Light, Dark, or Match my device.
 *
 * ⚠️ **LIGHT is the default, not "follow the system".** The suite's standing
 * rule is that every app opens light until the user explicitly picks otherwise;
 * this client used to follow the OS with no choice at all. The iOS client
 * (`Theme.swift`) and the browser client (`index.html`) carry the same three
 * choices with the same labels and the same stored words.
 */
enum class Appearance(val key: String, val label: String) {
    LIGHT("light", "Light"),
    DARK("dark", "Dark"),
    SYSTEM("system", "Match my device");

    companion object {
        fun fromKey(key: String?): Appearance = entries.firstOrNull { it.key == key } ?: LIGHT
    }
}

/** The current choice plus a way to change it, for the Advanced control. */
class AppearanceSetting(val current: Appearance, val set: (Appearance) -> Unit)

val LocalAppearance = staticCompositionLocalOf { AppearanceSetting(Appearance.LIGHT) {} }

/**
 * Whether the app is drawing dark RIGHT NOW — the choice, resolved.
 *
 * ⚠️ Read this, never `isSystemInDarkTheme()`, anywhere a colour depends on the
 * theme (the suite bar does). The OS setting only matters under "Match my
 * device"; reading it directly paints a dark bar over a light app.
 */
val LocalDarkTheme = staticCompositionLocalOf { false }

@Composable
fun UniversalScreensTheme(content: @Composable () -> Unit) {
    val context = LocalContext.current
    // Reloaded from the store on every activity (re)creation, so a rotation or a
    // process death can never lose the choice — it is saved the moment it changes.
    var appearance by remember { mutableStateOf(ConnectionStore.loadAppearance(context)) }
    val systemDark = isSystemInDarkTheme()
    val dark = when (appearance) {
        Appearance.LIGHT -> false
        Appearance.DARK -> true
        Appearance.SYSTEM -> systemDark
    }
    val setting = AppearanceSetting(appearance) { chosen ->
        appearance = chosen
        ConnectionStore.saveAppearance(context, chosen)
    }
    CompositionLocalProvider(LocalAppearance provides setting, LocalDarkTheme provides dark) {
        MaterialTheme(
            colorScheme = if (dark) DarkColors else LightColors,
            content = content,
        )
    }
}

/**
 * Advanced ▸ Appearance: three radio rows. Rows rather than a segmented control
 * because "Match my device" does not fit a third of a phone's width without
 * wrapping or truncating.
 */
@Composable
fun AppearancePicker(modifier: Modifier = Modifier) {
    val setting = LocalAppearance.current
    Column(modifier = modifier.fillMaxWidth().selectableGroup()) {
        Text(
            "Appearance",
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Appearance.entries.forEach { choice ->
            val selected = setting.current == choice
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 48.dp)
                    .selectable(selected = selected, role = Role.RadioButton, onClick = { setting.set(choice) }),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                // onClick = null: the whole row is the target, so TalkBack reads
                // one radio button with its label rather than two controls.
                RadioButton(selected = selected, onClick = null)
                Spacer(Modifier.width(12.dp))
                Text(choice.label, style = MaterialTheme.typography.bodyLarge)
            }
        }
    }
}

// ---- per-mode display metadata (parity with iOS Theme.swift `Mode` extensions) ----

/** Short title shown in pickers, saved-host subtitles and connected headers. */
fun Mode.label(): String = when (this) {
    Mode.CLICKER -> "Clicker"
    Mode.VIEWER -> "Mirror"
    Mode.FULL_CONTROL -> "Remote control"
    Mode.TRACKPAD -> "Trackpad"
    Mode.SECOND_SCREEN -> "Second screen"
}

/** One-line description of what the mode does (mode picker). */
fun Mode.subtitle(): String = when (this) {
    Mode.CLICKER -> "Presentation remote — next/previous, blank, slide previews"
    Mode.VIEWER -> "Watch the host's screen (view only)"
    Mode.FULL_CONTROL -> "See the screen and control it (mouse + keys)"
    Mode.TRACKPAD -> "Use the phone as a touchpad — move, tap, scroll"
    Mode.SECOND_SCREEN -> "Use the phone as an extra display (needs a virtual-display driver on the PC)"
}

/** An emoji glyph representing the mode in the picker chips (the Android stand-in
 *  for iOS's SF Symbols). */
fun Mode.emoji(): String = when (this) {
    Mode.CLICKER -> "📽️"
    Mode.VIEWER -> "👁️"
    Mode.FULL_CONTROL -> "🖱️"
    Mode.TRACKPAD -> "✋"
    Mode.SECOND_SCREEN -> "🖥️"
}
