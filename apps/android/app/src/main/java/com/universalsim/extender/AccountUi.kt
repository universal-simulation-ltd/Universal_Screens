package com.universalsim.extender

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

/**
 * The Universal ID row under Advanced, and the dialog behind it (James,
 * 2026-09-17). Signed out it offers "Sign in"; signed in it names the account
 * and offers sign-out.
 *
 * The copy says plainly that connecting never needs an account. Screens is a
 * LAN tool first — it is used on networks with no route out as a matter of
 * course — and a sign-in that looked compulsory would be a reason not to use
 * it. What signing in buys is the saved machines following the account, and
 * the count treating your devices as one person. See Account.kt.
 */
@Composable
fun AccountRow(onSynced: () -> Unit = {}) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var email by remember { mutableStateOf(Account.email(context)) }
    var open by remember { mutableStateOf(false) }

    TextButton(onClick = { open = true }) {
        Text(if (email != null) "👤  ${email}" else "👤  Sign in")
    }

    // Opening the app (and signing in) merges this phone's machines with the
    // account's. Signed out this does nothing at all.
    LaunchedEffect(email) {
        // `true` means the merged list differs from what was on screen, so the
        // caller repaints — without this a machine pulled from the account only
        // appeared after something else happened to reload the list.
        if (email != null && Account.syncSaved(context)) onSynced()
    }

    if (open) {
        AccountDialog(
            email = email,
            onDismiss = { open = false },
            onSignedIn = { email = Account.email(context) },
            onSignOut = {
                scope.launch {
                    Account.signOut(context)
                    email = null
                }
            },
        )
    }
}

/** Two stages: the address, then the code it was emailed. */
@Composable
private fun AccountDialog(
    email: String?,
    onDismiss: () -> Unit,
    onSignedIn: () -> Unit,
    onSignOut: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var address by remember { mutableStateOf("") }
    var code by remember { mutableStateOf("") }
    var stage by remember { mutableStateOf("email") }
    var message by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (email != null) "Your Universal ID" else "Sign in") },
        text = {
            Column {
                Text(
                    if (email != null) {
                        "Signed in as $email. Your saved machines are kept with this account."
                    } else {
                        "Your saved machines follow your Universal ID. Connecting never needs an " +
                            "account — this is only so a new phone or laptop already knows your machines."
                    },
                    style = MaterialTheme.typography.bodySmall,
                )
                message?.let {
                    Spacer(Modifier.height(8.dp))
                    Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                }
                if (email == null) {
                    Spacer(Modifier.height(12.dp))
                    if (stage == "email") {
                        OutlinedTextField(
                            value = address,
                            onValueChange = { address = it },
                            label = { Text("Email address") },
                            singleLine = true,
                        )
                    } else {
                        OutlinedTextField(
                            value = code,
                            onValueChange = { code = it.filter { c -> c.isDigit() }.take(6) },
                            label = { Text("Your code") },
                            singleLine = true,
                        )
                    }
                }
            }
        },
        confirmButton = {
            if (email != null) {
                TextButton(onClick = { onSignOut(); onDismiss() }) { Text("Sign out") }
            } else {
                TextButton(
                    enabled = !busy,
                    onClick = {
                        busy = true
                        message = null
                        scope.launch {
                            try {
                                if (stage == "email") {
                                    Account.sendCode(address)
                                    stage = "code"
                                    message = "We emailed a 6-digit code to ${address.trim()}."
                                } else {
                                    Account.verifyCode(context, address, code)
                                    onSignedIn()
                                    onDismiss()
                                }
                            } catch (e: Exception) {
                                message = e.message ?: "Sign-in failed."
                            } finally {
                                busy = false
                            }
                        }
                    },
                ) {
                    Text(if (stage == "email") "Email me a code" else "Sign in")
                }
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Close") } },
    )
}
