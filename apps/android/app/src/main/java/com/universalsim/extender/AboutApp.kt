package com.universalsim.extender

import android.content.pm.PackageManager
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * "About this app" — the answer every app in the Universal Suite gives, given
 * here.
 *
 * The suite got this on 2026-08-29 as the SDK's `<AboutAppDialog>`, reached
 * through an Advanced section of the Actions menu. Nothing about that component
 * can be reused here — it is React — so what is shared is the **content**: what
 * the app does, what happens to your screen, that it is open source and where
 * the code is, which build you are on, and who to contact. The wording is kept
 * word-for-word with `crates/host-ui/src/lib.rs` (`about_panel`) and
 * `apps/web/index.html`, and with `apps/ios/ScreenExtender/AboutView.swift`.
 *
 * ⚠️ **The privacy section is deliberately NOT the suite's local-first line.**
 * "Your screen never leaves this computer" would be a lie about an app whose
 * entire purpose is putting a screen on another device. What is true, and what
 * this says, is where it goes and who can read it on the way — see
 * `crates/transport/src/lib.rs` for the Noise tunnel that makes the PIN
 * encryption rather than a gate.
 */
@Composable
fun AboutAppDialog(onDismiss: () -> Unit) {
    val context = LocalContext.current
    val uriHandler = LocalUriHandler.current

    // ⚠️ Read from the installed package, not a constant in this file. A
    // hand-kept copy is a version that goes stale the first time nobody
    // remembers to bump it — and `buildConfig` is off for this module, so
    // BuildConfig.VERSION_NAME is not available either.
    val version = remember {
        try {
            context.packageManager.getPackageInfo(context.packageName, 0).versionName
        } catch (_: PackageManager.NameNotFoundException) {
            null
        }
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = { TextButton(onClick = onDismiss) { Text("Close") } },
        title = { Text("Universal Screens") },
        text = {
            Column(
                modifier = Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Section("What it does")
                Text("Use this phone as a second screen, remote control or presentation clicker for your computer.")

                Section("Your screen")
                Text(
                    "It goes to the device you paired with, and to nobody else. The connection is " +
                        "encrypted end to end with your PIN as the key, so neither your network nor " +
                        "the relay behind a remote code can read what is on it.",
                )
                Text(
                    "Nothing is uploaded to UNI·SIM, and there is no account.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )

                Section("Open source")
                Text("Free and open source under the MIT licence — every line of it public, for anyone to read or run themselves.")
                Link("View the source ↗", REPO_URL, uriHandler::openUri)
                Link("Report a problem ↗", ISSUES_URL, uriHandler::openUri)

                if (version != null) {
                    Section("Version")
                    Text("v$version", fontWeight = FontWeight.Bold)
                    Link("What's new ↗", CHANGELOG_URL, uriHandler::openUri)
                }

                Section("Support")
                Text("Questions, or something not working as it should?")
                Link("Contact us ↗", SUPPORT_URL, uriHandler::openUri)
            }
        },
    )
}

private const val REPO_URL = "https://github.com/universal-simulation-ltd/Universal_Screens"
private const val ISSUES_URL = "https://github.com/universal-simulation-ltd/Universal_Screens/issues"
private const val CHANGELOG_URL = "https://changelog.unisim.co.uk"
private const val SUPPORT_URL = "https://unisim.co.uk/#contact"

/** The small uppercase heading the other surfaces' About panels are built from. */
@Composable
private fun Section(label: String) {
    Text(
        label.uppercase(),
        fontSize = 11.sp,
        fontWeight = FontWeight.Bold,
        letterSpacing = 0.08.sp,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.fillMaxWidth(),
    )
}

@Composable
private fun Link(label: String, url: String, open: (String) -> Unit) {
    TextButton(onClick = { open(url) }) { Text(label) }
}
