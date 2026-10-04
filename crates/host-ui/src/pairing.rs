//! The host's paired devices: where they are kept, and the window panel that
//! lists them and forgets them.
//!
//! A device that pairs with the code once (SPAKE2 — see
//! `crates/transport/src/handshake.rs`) is remembered by its key and comes back
//! without the code, even after the host restarts with a new PIN. So the PIN no
//! longer decides who can connect *again*; this list does, and "Forget" is how a
//! lost phone stops being able to. One copy here, so the three hosts cannot
//! disagree about it.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use extender_transport::{PairedPeer, PairingStore};

/// The host's pairing file name. A client on the same machine uses
/// `client-pairing.txt` beside it.
pub const HOST_PAIRING_FILE: &str = "host-pairing.txt";

/// `<config dir>/UniversalScreens/host-pairing.txt`.
#[must_use]
pub fn pairing_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("UniversalScreens").join(HOST_PAIRING_FILE))
}

/// The store the accept loop uses. Falls back to one in memory — a fresh
/// identity, nobody remembered — when the file cannot be used, and says why:
/// pairing still works, devices just need the code every time.
#[must_use]
pub fn pairing_store() -> PairingStore {
    let opened = pairing_path()
        .ok_or_else(|| std::io::Error::other("no config folder"))
        .and_then(|p| PairingStore::open(&p));
    match opened {
        Ok(store) => store,
        Err(e) => {
            eprintln!("paired devices unavailable ({e}); every device will need the PIN");
            PairingStore::ephemeral().expect("the OS random number generator is unavailable")
        }
    }
}

/// "Paired devices" — who can reconnect without the PIN, and the button that
/// changes that.
#[derive(Default)]
pub struct PairedDevicesPanel {
    /// The list as last read, and when, so an open menu does not re-read the
    /// file on every frame.
    cached: Option<(Instant, Vec<PairedPeer>)>,
    /// What the last "Forget" did, shown under the button.
    outcome: Option<String>,
}

/// How long a read of the list is reused while the menu stays open.
const REFRESH: Duration = Duration::from_secs(1);

impl PairedDevicesPanel {
    fn peers(&mut self) -> &[PairedPeer] {
        let stale = self.cached.as_ref().is_none_or(|(at, _)| at.elapsed() > REFRESH);
        if stale {
            let peers = pairing_path()
                .and_then(|p| PairingStore::open(&p).ok())
                .map(|s| s.peers().to_vec())
                .unwrap_or_default();
            self.cached = Some((Instant::now(), peers));
        }
        self.cached.as_ref().map_or(&[], |(_, p)| p.as_slice())
    }

    /// Draw the panel (inside a menu).
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.set_max_width(320.0);
        let peers = self.peers().to_vec();
        ui.label(egui::RichText::new(paired_count(peers.len())).strong());
        ui.small(
            "A device that connects once with the PIN is remembered, and reconnects \
             without it — even after the PIN changes.",
        );
        if !peers.is_empty() {
            ui.add_space(4.0);
            for peer in peers.iter().rev().take(8) {
                let label = if peer.label.trim().is_empty() { "a device" } else { peer.label.as_str() };
                ui.small(format!(
                    "• {label} — paired {} (key {})",
                    civil_date(peer.paired_at),
                    extender_transport::pairing::short(&peer.key)
                ));
            }
            if peers.len() > 8 {
                ui.small(format!("…and {} more", peers.len() - 8));
            }
            ui.add_space(4.0);
            if ui.button("Forget all paired devices").clicked() {
                self.outcome = Some(forget_all());
                self.cached = None;
            }
            ui.small(
                "Every device will need the PIN again. Do this if a phone or computer \
                 that paired is lost, or no longer yours.",
            );
        }
        if let Some(outcome) = &self.outcome {
            ui.small(outcome.as_str());
        }
    }
}

/// "No devices paired" / "1 device paired" / "3 devices paired".
#[must_use]
pub fn paired_count(n: usize) -> String {
    match n {
        0 => "No devices paired yet".to_owned(),
        1 => "1 device paired".to_owned(),
        n => format!("{n} devices paired"),
    }
}

fn forget_all() -> String {
    let result = pairing_path()
        .ok_or_else(|| std::io::Error::other("no config folder"))
        .and_then(|p| PairingStore::open(&p))
        .and_then(|mut s| s.forget_all());
    match result {
        Ok(()) => "Forgotten. Each device needs the PIN on its next connection.".to_owned(),
        Err(e) => format!("Could not forget them: {e}"),
    }
}

/// `YYYY-MM-DD` (UTC) for unix seconds — Howard Hinnant's days-to-civil, so
/// the hosts need no date library for one label.
#[must_use]
pub fn civil_date(unix: u64) -> String {
    let days = i64::try_from(unix / 86_400).unwrap_or(0);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_civil() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(951_782_400), "2000-02-29");
        assert_eq!(civil_date(1_791_072_000), "2026-10-04");
    }

    #[test]
    fn counts_read_as_english() {
        assert_eq!(paired_count(0), "No devices paired yet");
        assert_eq!(paired_count(1), "1 device paired");
        assert_eq!(paired_count(4), "4 devices paired");
    }
}
