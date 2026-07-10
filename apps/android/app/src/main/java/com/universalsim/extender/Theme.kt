package com.universalsim.extender

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

/**
 * Shared brand styling for the Android client, kept at parity with the iOS app's
 * `Theme.swift`. The whole UI is tinted with UNI·SIM brand orange (#E05504); dark
 * mode sits the app on the brand slate the app icon uses (#0F172A).
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

@Composable
fun UniversalScreensTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = if (isSystemInDarkTheme()) DarkColors else LightColors,
        content = content,
    )
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
