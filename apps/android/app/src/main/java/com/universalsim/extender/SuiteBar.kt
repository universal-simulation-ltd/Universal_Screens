package com.universalsim.extender

import android.content.Intent
import android.net.Uri
import android.provider.Settings
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * The Universal Apps bar, for the NATIVE Android client.
 *
 * Every other app in the suite is a Capacitor webview of its React site, so it
 * gets this bar from the SDK (`UniversalAppsNavBar.tsx`) for free. Screens is
 * the suite's only hand-written native mobile client, so it draws its own.
 *
 * ⚠️ **Match `UniversalAppsNavBar`, NOT the browser client's `.un-bar`.** The
 * first cut of this file copied `apps/web/index.html`, which is a DIFFERENT and
 * simpler bar — a one-line "Universal Screens", a "Universal Apps ↗" pill and a
 * GitHub button — because that page is the open-source landing page and has no
 * app around it. The bar the phone sits next to on the home screen is the app
 * one: a two-line lockup, the product word in orange, a switcher chevron, and
 * the suite globe at the right-hand end. Checked against Universal PDF on the
 * device, not from memory.
 *
 * Every measurement below is from the SDK source, and the comments name which
 * style it comes from. Colours are `barTheme.ts`'s BAR table.
 *
 * ⚠️ These colours are the SUITE's, NOT `MaterialTheme.colorScheme`. The app's
 * own dark surface is #1A2436 (see Theme.kt); the bar's is #0F172A, because it
 * has to match the bar in Universal PDF, not the app it is bolted to. Do not
 * "fix" these to theme tokens — that is the drift this comment exists to stop.
 *
 * What is deliberately absent: the profile pill ("Bienvenue ▾" + avatar).
 * Screens has no account system, so there is nothing for it to show.
 */

// BAR.light / BAR.dark, verbatim from packages/sdk/src/barTheme.ts.
private val SurfaceLight = Color(0xFFFFFFFF)
private val SurfaceDark = Color(0xFF0F172A)
private val BorderLight = Color(0xFFE2E8F0)
private val BorderDark = Color(0xFF1E293B)
private val MutedLight = Color(0xFF475569)
private val MutedDark = Color(0xFF94A3B8)

/**
 * `claimAccent` — the orange the product word is painted in.
 *
 * ⚠️ NOT one value for both themes. #c2410c is 5.18:1 on white but only 3.5:1
 * on the dark surface; orange-400 is 8.0:1 there. The rule inverts with the
 * background, so darkening the orange to "fix" a dark bar makes it worse. The
 * SDK carries this same warning on the same token.
 */
private val AccentLight = Color(0xFFC2410C)
private val AccentDark = Color(0xFFFB923C)

/** The strip's colour — BRAND.orangeDeep. */
private val StripOrange = Color(0xFFE05504)

/** CSS `ease-in-out`, exactly. Compose's FastOutSlowIn is a different curve. */
private val EaseInOut = CubicBezierEasing(0.42f, 0f, 0.58f, 1f)

/** Where the switcher chevron goes. The SDK's `portalHref` default. */
private const val PORTAL_URL = "https://opensource.unisim.co.uk"

/**
 * The globe at the right-hand end is the CHANGELOG, not a link to the company.
 * Confirmed by tapping it in Universal PDF on the device: it opens the
 * "Nouveautés" panel of suite releases. A native panel would mean shipping a
 * changelog reader, so this opens the same feed's web view.
 */
private const val CHANGELOG_URL = "https://changelog.unisim.co.uk"

@Composable
fun SuiteBar(modifier: Modifier = Modifier) {
    val dark = isSystemInDarkTheme()
    val context = LocalContext.current
    val open: (String) -> Unit = { url ->
        runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) }
    }

    Column(modifier = modifier.fillMaxWidth().background(if (dark) SurfaceDark else SurfaceLight)) {
        PulseStrip()
        Row(
            // headerInnerStyle at the mobile breakpoint: 10x16 padding, 56px min.
            modifier = Modifier
                .fillMaxWidth()
                .defaultMinSize(minHeight = 56.dp)
                .padding(horizontal = 16.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Identity(dark = dark, onClick = { open(PORTAL_URL) })
            Spacer(Modifier.weight(1f))
            GlobeButton(onClick = { open(CHANGELOG_URL) })
        }
        // ⚠️ No gap under the bar: this 1px stroke IS the separator.
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(1.dp)
                .background(if (dark) BorderDark else BorderLight)
        )
    }
}

/**
 * The 4px pulsing rule above the bar — `UniversalBar.tsx`: transparent →
 * #e05504 → transparent, opacity 1 → .35 → 1 over 2400ms.
 *
 * Held static with animations switched off system-wide. Compose has no "reduce
 * motion" flag, so the signal is `ANIMATOR_DURATION_SCALE == 0` — the same one
 * `FailureMark` in MainActivity reads, and what both the developer-options
 * toggle and the accessibility "remove animations" setting write.
 */
@Composable
private fun PulseStrip() {
    val context = LocalContext.current
    val animate = remember {
        Settings.Global.getFloat(
            context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f,
        ) != 0f
    }
    // Half a 2400ms cycle each way, so a full period is 2400ms.
    val transition = rememberInfiniteTransition(label = "suite-strip")
    val pulse by transition.animateFloat(
        initialValue = 1f,
        targetValue = 0.35f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 1200, easing = EaseInOut),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "suite-strip-alpha",
    )

    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(4.dp)
            .alpha(if (animate) pulse else 1f)
            .background(
                Brush.horizontalGradient(
                    0f to Color.Transparent,
                    0.5f to StripOrange,
                    1f to Color.Transparent,
                )
            )
    )
}

/**
 * The mark, the two-line lockup and the switcher chevron.
 *
 * The lockup is `shortProductName`'s doing: "Universal Screens" → "UNIVERSAL"
 * above "Screens", the shared word small and the product word large and in the
 * accent. ⚠️ The eyebrow says UNIVERSAL, not UNI SIM — the owner corrected that
 * on 2026-08-30. UNI·SIM is the company; the product is "Universal Screens".
 *
 * The mark is `app_icon` — the same generated raster the launcher icon and the
 * landing hero use, so there is one piece of artwork here, not a hand-copy of
 * its paths.
 *
 * `clearAndSetSemantics` puts the FULL name back for a screen reader: the
 * lockup is a visual abbreviation, and reading out "UNIVERSAL, Screens" as two
 * strings would be reading the user a piece of layout. The SDK does the same
 * with `aria-label`.
 */
@Composable
private fun Identity(dark: Boolean, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .clip(RoundedCornerShape(8.dp))
            .clickable(onClick = onClick)
            .padding(end = 2.dp)
            .clearAndSetSemantics { contentDescription = "Universal Screens — all the Universal Apps" },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Image(
            painter = painterResource(R.drawable.app_icon),
            contentDescription = null,
            // 24dp — measured off Universal PDF's bar on this device (420dpi).
            modifier = Modifier.size(24.dp).clip(RoundedCornerShape(5.dp)),
        )
        // lockupStyle: a column, lineHeight 1.1, centred.
        Column(verticalArrangement = Arrangement.Center) {
            // suiteEyebrowStyle: 9px / 700 / 0.09em, uppercase, `muted`.
            Text(
                "UNIVERSAL",
                color = if (dark) MutedDark else MutedLight,
                fontSize = 9.sp,
                lineHeight = 10.sp,
                fontWeight = FontWeight.Bold,
                letterSpacing = 0.81.sp,
                maxLines = 1,
            )
            // productNameStyle at claimAccent: 15px / 600 / -0.01em.
            Text(
                "Screens",
                color = if (dark) AccentDark else AccentLight,
                fontSize = 15.sp,
                lineHeight = 16.5.sp,
                fontWeight = FontWeight.SemiBold,
                letterSpacing = (-0.15).sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Icon(
            painter = painterResource(R.drawable.ic_chevron_down),
            contentDescription = null,
            tint = if (dark) MutedDark else MutedLight,
            modifier = Modifier.size(20.dp),
        )
    }
}

/**
 * The suite globe at the right-hand end.
 *
 * ⚠️ This is the REAL artwork (`unisim-icon.png`, the same file the installer
 * seals into its corner), NOT the monochrome glyph the SDK draws INSIDE the
 * product name. Those are two different marks in the SDK for a documented
 * reason — the small one has to be monochrome so it reads as a prefix to a
 * word — and this is the full-colour one, which is what sits at the end of the
 * bar in Universal PDF.
 */
@Composable
private fun GlobeButton(onClick: () -> Unit) {
    Box(
        modifier = Modifier
            .size(38.dp)
            .clip(RoundedCornerShape(10.dp))
            .clickable(onClick = onClick)
            .semantics { contentDescription = "What's new across the suite" },
        contentAlignment = Alignment.Center,
    ) {
        Image(
            painter = painterResource(R.drawable.unisim_globe),
            contentDescription = null,
            modifier = Modifier.size(26.dp),
        )
    }
}
