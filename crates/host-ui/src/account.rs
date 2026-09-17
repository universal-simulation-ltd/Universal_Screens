//! A Universal ID on the desktop hosts — sign in, stay signed in, sign out
//! (James, 2026-09-17: "go ahead with screens account").
//!
//! ⚠️ **Identity only here, and that is not an omission.** The phone and
//! browser clients sync their saved machines to the account
//! (`screens_saved_hosts`, migration 0178), because those are the clients that
//! keep a list of machines to connect TO. A host IS the machine — its
//! `RecentConn` list is who connected to it, which belongs to this computer and
//! not to a person — so there is nothing here to follow an account around. What
//! signing in does do is make the live user count treat your laptop, your phone
//! and your browser as one person rather than three.
//!
//! The flow is the suite's email one-time code, **login only**: a Universal ID
//! is created on the hub (app.unisim.co.uk), never here, so a typo cannot mint
//! a stray account. `apps/web/src/account.js` is the same thing in JavaScript;
//! keep the two in step.
//!
//! ⚠️ Everything is best-effort and off the UI thread. A host on a LAN with no
//! route out is normal, and it must keep working exactly as it does today.

use std::io::Read;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;

const SUPABASE_URL: &str = "https://rygfxgalojojppxmhddo.supabase.co";
const SUPABASE_ANON: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6InJ5Z2Z4Z2Fsb2pvanBweG1oZGRvIiwicm9sZSI6ImFub24iLCJpYXQiOjE3Nzg3NTY4MjUsImV4cCI6MjA5NDMzMjgyNX0.hLy_vt9vY_rdPKF3nL32yAuMCD604E3CH5VM7D7CaNE";

/// `<config dir>/UniversalScreens/session.json`, beside the install id.
fn session_path() -> Option<std::path::PathBuf> {
    let dir = dirs::config_dir()?.join("UniversalScreens");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("session.json"))
}

/// Read one string field out of a flat JSON object. The three shapes this file
/// parses are GoTrue's and its own, and two of them have four fields between
/// them — not worth a JSON dependency in the hosts (the same call `user_count`
/// makes).
fn field(body: &str, name: &str) -> Option<String> {
    let key = format!("\"{name}\"");
    let at = body.find(&key)? + key.len();
    let rest = body[at..].trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => out.push(chars.next()?),
            _ => out.push(c),
        }
    }
    None
}

/// The nested `"user": { … "email": … }` — GoTrue puts an `email` on the user,
/// and `field` would otherwise find whichever came first.
fn user_email(body: &str) -> Option<String> {
    let at = body.find("\"user\"")?;
    field(&body[at..], "email")
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[derive(Clone, Default)]
struct Session {
    access: String,
    refresh: String,
    email: String,
}

fn load_session() -> Option<Session> {
    let body = std::fs::read_to_string(session_path()?).ok()?;
    Some(Session {
        access: field(&body, "access_token")?,
        refresh: field(&body, "refresh_token")?,
        email: field(&body, "email").unwrap_or_default(),
    })
}

fn save_session(s: Option<&Session>) {
    let Some(path) = session_path() else { return };
    match s {
        None => {
            let _ = std::fs::remove_file(path);
        }
        Some(s) => {
            let body = format!(
                "{{\"access_token\":\"{}\",\"refresh_token\":\"{}\",\"email\":\"{}\"}}",
                escape(&s.access), escape(&s.refresh), escape(&s.email),
            );
            if std::fs::write(&path, body).is_ok() {
                // ⚠️ The refresh token is a long-lived credential for a UNI·SIM
                // account, sitting in a file. On unix that file is the owner's
                // alone; Windows inherits the user profile's ACL, which is the
                // same promise in a different shape. A keychain would be
                // better and is a per-OS job — written down rather than
                // pretended about.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
                }
            }
        }
    }
}

/// POST to GoTrue. `Ok(body)` on success, `Err(message)` with GoTrue's own
/// wording on failure — the user typed something, so they get an answer.
fn auth(path: &str, body: String) -> Result<String, String> {
    let response = ureq::post(&format!("{SUPABASE_URL}/auth/v1/{path}"))
        .set("apikey", SUPABASE_ANON)
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(20))
        .send_string(&body);
    match response {
        Ok(res) => {
            let mut text = String::new();
            res.into_reader().take(64 * 1024).read_to_string(&mut text).map_err(|e| e.to_string())?;
            Ok(text)
        }
        Err(ureq::Error::Status(_, res)) => {
            let mut text = String::new();
            let _ = res.into_reader().take(16 * 1024).read_to_string(&mut text);
            Err(field(&text, "error_description")
                .or_else(|| field(&text, "msg"))
                .or_else(|| field(&text, "message"))
                .unwrap_or_else(|| "Sign-in failed.".into()))
        }
        Err(_) => Err("No connection to unisim.co.uk.".into()),
    }
}

/// What the worker thread hands back to the UI thread.
enum Msg {
    CodeSent,
    SignedIn(Session),
    Failed(String),
}

/// The sign-in state the host draws: who is signed in, which stage the dialog
/// is at, and whatever the last attempt said.
pub struct UserAccount {
    session: Arc<Mutex<Option<Session>>>,
    /// Shared with `UserCount` so a signed-in beat goes as the account.
    token: Arc<Mutex<Option<String>>>,
    pub show: bool,
    email: String,
    code: String,
    stage_code: bool,
    busy: bool,
    message: Option<String>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl Default for UserAccount {
    fn default() -> Self {
        Self::new()
    }
}

impl UserAccount {
    pub fn new() -> Self {
        let restored = load_session();
        let token = Arc::new(Mutex::new(restored.as_ref().map(|s| s.access.clone())));
        let session = Arc::new(Mutex::new(restored));
        let (tx, rx) = channel();
        // A stored access token is minutes old at best — refresh once on start
        // so the first beat of this run goes as the account rather than as a
        // guest for the first 45 seconds.
        {
            let session = session.clone();
            let token = token.clone();
            std::thread::Builder::new()
                .name("unisim-account-refresh".into())
                .spawn(move || {
                    let current = session.lock().unwrap().clone();
                    let Some(current) = current else { return };
                    match auth(
                        "token?grant_type=refresh_token",
                        format!("{{\"refresh_token\":\"{}\"}}", escape(&current.refresh)),
                    ) {
                        Ok(body) => {
                            if let (Some(access), Some(refresh)) =
                                (field(&body, "access_token"), field(&body, "refresh_token"))
                            {
                                let next = Session {
                                    access: access.clone(),
                                    refresh,
                                    email: user_email(&body).unwrap_or(current.email),
                                };
                                save_session(Some(&next));
                                *token.lock().unwrap() = Some(access);
                                *session.lock().unwrap() = Some(next);
                            }
                        }
                        Err(_) => {
                            // A REFUSED refresh means the session is gone for
                            // good. A failed one (no network) must not sign
                            // anybody out — auth() cannot tell us which this
                            // was, so the benefit of the doubt goes to the
                            // user and the token is simply left as it is.
                        }
                    }
                })
                .ok();
        }
        Self {
            session,
            token,
            show: false,
            email: String::new(),
            code: String::new(),
            stage_code: false,
            busy: false,
            message: None,
            tx,
            rx,
        }
    }

    /// The token handle to hand `UserCount::start_with`.
    pub fn token(&self) -> Arc<Mutex<Option<String>>> {
        self.token.clone()
    }

    /// The signed-in address, for the menu row.
    pub fn email(&self) -> Option<String> {
        self.session.lock().ok()?.as_ref().map(|s| s.email.clone())
    }

    fn drain(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            self.busy = false;
            match msg {
                Msg::CodeSent => {
                    self.stage_code = true;
                    self.message = Some(format!("We emailed a 6-digit code to {}.", self.email.trim()));
                }
                Msg::SignedIn(s) => {
                    *self.token.lock().unwrap() = Some(s.access.clone());
                    *self.session.lock().unwrap() = Some(s);
                    self.stage_code = false;
                    self.code.clear();
                    self.message = None;
                    self.show = false;
                }
                Msg::Failed(m) => self.message = Some(m),
            }
        }
    }

    fn send_code(&mut self) {
        let email = self.email.trim().to_string();
        if !email.contains('@') {
            self.message = Some("Enter your email address.".into());
            return;
        }
        self.busy = true;
        self.message = None;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let msg = match auth("otp", format!("{{\"email\":\"{}\",\"create_user\":false}}", escape(&email))) {
                Ok(_) => Msg::CodeSent,
                // With create_user false, GoTrue answers an unknown address
                // with "Signups not allowed for otp". Say what that means here.
                Err(e) if e.to_lowercase().contains("not allowed") => Msg::Failed(
                    "No Universal ID for that email. Create one free at app.unisim.co.uk, then sign in here.".into(),
                ),
                Err(e) => Msg::Failed(e),
            };
            let _ = tx.send(msg);
        });
    }

    fn verify(&mut self) {
        let email = self.email.trim().to_string();
        let code = self.code.trim().to_string();
        self.busy = true;
        self.message = None;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let msg = match auth(
                "verify",
                format!(
                    "{{\"email\":\"{}\",\"token\":\"{}\",\"type\":\"email\"}}",
                    escape(&email), escape(&code),
                ),
            ) {
                Ok(body) => match (field(&body, "access_token"), field(&body, "refresh_token")) {
                    (Some(access), Some(refresh)) => {
                        let s = Session { access, refresh, email: user_email(&body).unwrap_or(email) };
                        save_session(Some(&s));
                        Msg::SignedIn(s)
                    }
                    _ => Msg::Failed("That code was not accepted.".into()),
                },
                Err(e) => Msg::Failed(e),
            };
            let _ = tx.send(msg);
        });
    }

    pub fn sign_out(&mut self) {
        let token = self.session.lock().unwrap().as_ref().map(|s| s.access.clone());
        *self.session.lock().unwrap() = None;
        *self.token.lock().unwrap() = None;
        save_session(None);
        self.stage_code = false;
        self.code.clear();
        self.message = None;
        if let Some(token) = token {
            std::thread::spawn(move || {
                let _ = ureq::post(&format!("{SUPABASE_URL}/auth/v1/logout"))
                    .set("apikey", SUPABASE_ANON)
                    .set("Authorization", &format!("Bearer {token}"))
                    .timeout(Duration::from_secs(10))
                    .call();
            });
        }
    }
}

/// The profile-menu row: "Sign in" or the account, opening the window below.
pub fn account_menu_row(ui: &mut egui::Ui, account: &mut UserAccount) {
    let label = match account.email() {
        Some(email) => format!("👤  {email}"),
        None => "👤  Sign in".to_string(),
    };
    if ui.button(label).clicked() {
        account.show = true;
        ui.close_menu();
    }
}

/// The sign-in window. Call once per frame from the host's `update`.
pub fn show_account_window(ctx: &egui::Context, account: &mut UserAccount) {
    account.drain();
    if !account.show {
        return;
    }
    let signed_in = account.email();
    let mut open = true;
    egui::Window::new(if signed_in.is_some() { "Your Universal ID" } else { "Sign in" })
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_max_width(320.0);
            match &signed_in {
                Some(email) => {
                    ui.label(format!("Signed in as {email}."));
                    ui.label(
                        egui::RichText::new(
                            "Your devices count as one person in the user count. Nothing about \
                             sharing this screen needs an account.",
                        )
                        .small()
                        .weak(),
                    );
                    ui.add_space(8.0);
                    if ui.button("Sign out").clicked() {
                        account.sign_out();
                        account.show = false;
                    }
                }
                None => {
                    ui.label(
                        egui::RichText::new(
                            "Sign in with your Universal ID — the same account as every other \
                             UNI·SIM app. Sharing this screen never needs one.",
                        )
                        .small()
                        .weak(),
                    );
                    ui.add_space(8.0);
                    if account.stage_code {
                        ui.label("Your code");
                        ui.add(egui::TextEdit::singleline(&mut account.code).hint_text("123456"));
                    } else {
                        ui.label("Email address");
                        ui.add(
                            egui::TextEdit::singleline(&mut account.email)
                                .hint_text("you@example.com"),
                        );
                    }
                    if let Some(msg) = &account.message {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(msg).small().color(egui::Color32::from_rgb(0xb9, 0x1c, 0x1c)));
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let label = if account.stage_code { "Sign in" } else { "Email me a code" };
                        if ui.add_enabled(!account.busy, egui::Button::new(label)).clicked() {
                            if account.stage_code {
                                account.verify();
                            } else {
                                account.send_code();
                            }
                        }
                        if account.stage_code && ui.button("Use a different email").clicked() {
                            account.stage_code = false;
                            account.code.clear();
                            account.message = None;
                        }
                    });
                }
            }
        });
    if !open {
        account.show = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_fields_gotrue_returns() {
        let body = r#"{"access_token":"a.b.c","token_type":"bearer","expires_in":3600,
          "refresh_token":"r-1","user":{"id":"u-1","email":"someone@unisim.co.uk"}}"#;
        assert_eq!(field(body, "access_token").as_deref(), Some("a.b.c"));
        assert_eq!(field(body, "refresh_token").as_deref(), Some("r-1"));
        // ⚠️ The nested user's email, not any earlier "email" in the body.
        assert_eq!(user_email(body).as_deref(), Some("someone@unisim.co.uk"));
        assert_eq!(field(body, "nothing_here"), None);
    }

    #[test]
    fn an_error_body_is_not_mistaken_for_a_session() {
        let body = r#"{"code":400,"error_code":"otp_disabled","msg":"Signups not allowed for otp"}"#;
        assert_eq!(field(body, "access_token"), None);
        assert_eq!(field(body, "msg").as_deref(), Some("Signups not allowed for otp"));
    }

    #[test]
    fn quotes_and_backslashes_survive_the_round_trip() {
        // An address cannot contain these, but a message or a token could, and
        // a broken escape here would write a file that never parses again.
        assert_eq!(escape(r#"a"b\c"#), r#"a\"b\\c"#);
        let body = format!("{{\"access_token\":\"{}\"}}", escape(r#"a"b"#));
        assert_eq!(field(&body, "access_token").as_deref(), Some(r#"a"b"#));
    }
}
