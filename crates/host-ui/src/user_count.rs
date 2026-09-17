//! "There are X total users (Y live)" — the line every app in the suite shows
//! (James, 2026-09-17), for the desktop hosts.
//!
//! Screens has no `@unisim/sdk` and no Supabase client, so this is the SDK's
//! `presence.ts` written out against the REST endpoint. Read that file before
//! changing anything here; the behaviour is deliberately identical:
//!
//!   * `app_presence_beat(product, install_id)` on start and every 45 s
//!     (migration 0175);
//!   * `app_user_counts('screens')` → total + live, or `suite_user_counts()`
//!     (0177) once the line has been clicked for the whole-suite figure.
//!
//! The install id is a random UUID in a file beside the host's own state.
//! Nothing else leaves the machine: not the PIN, the room code, the machine
//! name, the peers, or anything about what is on screen. A host on a network
//! with no route out is a normal way to use Screens, so every call is
//! best-effort and silent — when they fail the line simply never appears.
//!
//! ⚠️ This is the ONLY thing in the desktop hosts that talks to a UNI·SIM
//! server. Keep it that way: anything else added here is a claim the About box
//! ("your screen never leaves your network") would have to stop making.
//!
//! The anon key is a PUBLISHABLE key — it ships in every suite web bundle by
//! design, and Row-Level Security is the boundary: `app_presence` has RLS on
//! with no policies, so this key reaches it through those two functions and in
//! no other way.

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;

const SUPABASE_URL: &str = "https://rygfxgalojojppxmhddo.supabase.co";
const SUPABASE_ANON: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6InJ5Z2Z4Z2Fsb2pvanBweG1oZGRvIiwicm9sZSI6ImFub24iLCJpYXQiOjE3Nzg3NTY4MjUsImV4cCI6MjA5NDMzMjgyNX0.hLy_vt9vY_rdPKF3nL32yAuMCD604E3CH5VM7D7CaNE";

const PRODUCT: &str = "screens";
const BEAT: Duration = Duration::from_secs(45);

/// Where the install id lives: `<config dir>/UniversalScreens/install-id`.
fn install_id_path() -> Option<std::path::PathBuf> {
    let dir = dirs::config_dir()?.join("UniversalScreens");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("install-id"))
}

/// This install's id, made once and kept. A machine that cannot write the file
/// gets a per-run id, and counts as a new install next time — the same
/// concession the browser makes for a blocked localStorage.
fn install_id() -> String {
    if let Some(path) = install_id_path() {
        if let Ok(existing) = std::fs::read_to_string(&path) {
            let trimmed = existing.trim().to_string();
            if trimmed.len() == 36 {
                return trimmed;
            }
        }
        let id = random_uuid();
        let _ = std::fs::write(&path, &id);
        return id;
    }
    random_uuid()
}

/// A v4 UUID from the OS RNG — `getrandom` is already in the tree for the PIN,
/// which is the encryption key, so this adds no dependency.
fn random_uuid() -> String {
    let mut b = [0u8; 16];
    if getrandom::getrandom(&mut b).is_err() {
        return "00000000-0000-4000-8000-000000000000".into();
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// POST one RPC and hand back the body. `None` on any failure, and nothing is
/// logged: a host with no internet would otherwise print this every 45 seconds.
fn rpc(function: &str, body: String, token: Option<&str>) -> Option<String> {
    // Signed in, the beat goes as the ACCOUNT: the server reads auth.uid() and
    // counts one person across their devices instead of one per install.
    let response = ureq::post(&format!("{SUPABASE_URL}/rest/v1/rpc/{function}"))
        .set("apikey", SUPABASE_ANON)
        .set("Authorization", &format!("Bearer {}", token.unwrap_or(SUPABASE_ANON)))
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(10))
        .send_string(&body)
        .ok()?;
    let mut text = String::new();
    response.into_reader().take(4096).read_to_string(&mut text).ok()?;
    Some(text)
}

/// `[{"total":12,"live":3}]` → (12, 3). Hand-parsed: two integers out of one
/// known shape is not worth a JSON dependency in the hosts.
fn parse_counts(body: &str) -> Option<(u64, u64)> {
    let field = |name: &str| -> Option<u64> {
        let at = body.find(&format!("\"{name}\""))? + name.len() + 3;
        let rest = &body[at..];
        let digits: String = rest.chars().skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok()
    };
    Some((field("total")?, field("live")?))
}

fn format_counts(total: u64, live: u64, suite: bool) -> String {
    let group = |n: u64| {
        let s = n.to_string();
        let mut out = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        out
    };
    let (is, plural) = if total == 1 { ("is", "") } else { ("are", "s") };
    if suite {
        format!(
            "There {is} a total of {} user{plural} across all UNI·SIM apps ({} live)",
            group(total), group(live),
        )
    } else {
        format!("There {is} {} total user{plural} ({} live)", group(total), group(live))
    }
}

/// The figure, kept up to date by a background thread.
///
/// Cheap to clone (one `Arc`), and `line()` never blocks the UI thread — the
/// egui frame simply reads whatever the last successful poll left behind.
#[derive(Clone)]
pub struct UserCount {
    line: Arc<Mutex<Option<String>>>,
    /// The signed-in access token, shared with `UserAccount`. None = a guest.
    token: Arc<Mutex<Option<String>>>,
    suite: Arc<AtomicBool>,
    /// Bumped by `toggle` so the thread re-reads at once rather than in 45 s.
    refresh: Arc<AtomicBool>,
}

impl Default for UserCount {
    fn default() -> Self {
        Self::start()
    }
}

impl UserCount {
    /// Begin beating and reading, as a guest.
    pub fn start() -> Self {
        Self::start_with(Arc::new(Mutex::new(None)))
    }

    /// Begin beating and reading, following `token` — hand it
    /// `UserAccount::token()` so signing in is reflected on the next beat.
    pub fn start_with(token: Arc<Mutex<Option<String>>>) -> Self {
        let this = Self {
            token,
            line: Arc::new(Mutex::new(None)),
            suite: Arc::new(AtomicBool::new(false)),
            refresh: Arc::new(AtomicBool::new(false)),
        };
        let worker = this.clone();
        std::thread::Builder::new()
            .name("unisim-user-count".into())
            .spawn(move || {
                let id = install_id();
                loop {
                    let token = worker.token.lock().unwrap().clone();
                    let token = token.as_deref();
                    let beaten = rpc(
                        "app_presence_beat",
                        format!("{{\"p_product\":\"{PRODUCT}\",\"p_install_id\":\"{id}\"}}"),
                        token,
                    )
                    .is_some();
                    let suite = worker.suite.load(Ordering::Relaxed);
                    let body = if suite {
                        rpc("suite_user_counts", "{}".into(), token)
                    } else {
                        rpc("app_user_counts", format!("{{\"p_product\":\"{PRODUCT}\"}}"), token)
                    };
                    if let Some((total, live)) = body.as_deref().and_then(parse_counts) {
                        if total > 0 {
                            // Whoever is looking at this window is using the
                            // app: a count that raced the beat must not say
                            // "(0 live)".
                            let live = if beaten { live.max(1) } else { live };
                            *worker.line.lock().unwrap() = Some(format_counts(total, live, suite));
                        }
                    }
                    // Wake early when the figure is switched, so the click
                    // feels like a switch rather than a wait.
                    for _ in 0..90 {
                        if worker.refresh.swap(false, Ordering::Relaxed) {
                            break;
                        }
                        std::thread::sleep(BEAT / 90);
                    }
                }
            })
            .ok();
        this
    }

    /// The line to draw, once there is a real number. `None` until then.
    pub fn line(&self) -> Option<String> {
        self.line.lock().ok().and_then(|l| l.clone())
    }

    /// Switch between this app's figure and the whole suite's.
    pub fn toggle(&self) {
        self.suite.fetch_xor(true, Ordering::Relaxed);
        *self.line.lock().unwrap() = None;
        self.refresh.store(true, Ordering::Relaxed);
    }
}

/// Draw the line as the last row of the hosts' profile menu — clickable,
/// because clicking it switches to the suite-wide figure, and nothing at all
/// while there is no number to show.
pub fn show_user_count(ui: &mut egui::Ui, counts: &UserCount, dark: bool) {
    let Some(text) = counts.line() else { return };
    let muted = if dark {
        egui::Color32::from_rgb(0x94, 0xa3, 0xb8)
    } else {
        egui::Color32::from_rgb(0x64, 0x74, 0x8b)
    };
    ui.separator();
    let label = egui::Label::new(egui::RichText::new(text).size(10.0).color(muted))
        .sense(egui::Sense::click());
    if ui
        .add(label)
        .on_hover_text("Click for every UNI·SIM app")
        .clicked()
    {
        counts.toggle();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_rpc_shape() {
        // Exactly what PostgREST returns for app_user_counts / suite_user_counts.
        assert_eq!(parse_counts(r#"[{"total":12,"live":3}]"#), Some((12, 3)));
        assert_eq!(parse_counts(r#"[{"total":0,"live":0}]"#), Some((0, 0)));
        // An error body, or an empty result set, must not read as a number.
        assert_eq!(parse_counts("[]"), None);
        assert_eq!(parse_counts(r#"{"message":"permission denied"}"#), None);
    }

    #[test]
    fn wording_matches_the_other_apps() {
        assert_eq!(format_counts(1, 1, false), "There is 1 total user (1 live)");
        assert_eq!(format_counts(1234, 5, false), "There are 1,234 total users (5 live)");
        assert_eq!(
            format_counts(9876, 12, true),
            "There are a total of 9,876 users across all UNI·SIM apps (12 live)",
        );
    }

    #[test]
    fn install_ids_are_v4_uuids() {
        let id = random_uuid();
        assert_eq!(id.len(), 36);
        assert_eq!(id.as_bytes()[14], b'4');
        assert!(matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
        assert_ne!(id, random_uuid());
    }
}
