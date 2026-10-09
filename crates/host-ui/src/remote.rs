//! "Remote access (other networks)": one panel for all three hosts.
//!
//! The host mints a short code, dials the cloud rendezvous as the sender
//! (`extender_web_bridge::dial_room_observed`) and bridges whoever joins to its
//! own listener on loopback. The person at the other end opens the browser
//! client (`/screens/app/`), types the code and the PIN, and the pairing and
//! encryption are the ordinary host handshake, end to end through the relay.
//!
//! ⚠️ **One code, one session.** The rendezvous pairs whoever shares a code, so
//! a code is only offered while its room is live. When the session ends, fails,
//! or the relay expires the empty room, the panel drops the code and offers a
//! fresh one. Until 2026-10-10 each host kept its own copy of this panel with a
//! `remote_active` flag that nothing ever cleared, so after the first session
//! the window kept showing a dead code until the host was restarted.
//!
//! It also replaces the old "Cast to a browser screen" panel, which dialled a
//! code shown by `/screens/receive`, a page that cannot decode video. Showing
//! this screen in a browser, on another network or the same one, is this panel.

use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui;
use extender_web_bridge::{dial_room_observed, RoomEnd, DEFAULT_ROOM_URL};

/// Where the person at the other end goes. The browser client reads `?remote=`
/// and fills the code in.
pub const REMOTE_CLIENT_URL: &str = "https://opensource.unisim.co.uk/screens/app/";

/// Where a remote session stands, shared with the thread that runs it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    /// No live code. `note` is how the last one ended, if there was one.
    Idle { note: Option<String> },
    /// In the room with `code`, nobody else yet.
    Waiting { code: String },
    /// Someone joined on `code`; the session is running.
    Connected { code: String },
}

impl Phase {
    fn code(&self) -> Option<&str> {
        match self {
            Phase::Waiting { code } | Phase::Connected { code } => Some(code),
            Phase::Idle { .. } => None,
        }
    }
}

/// What the dial thread reports when the room ends.
fn end_note(res: &std::io::Result<RoomEnd>) -> String {
    match res {
        Ok(RoomEnd::Ended) => "Remote session ended. Enable remote access again for a new code.".to_owned(),
        Ok(RoomEnd::Unpaired) => {
            "The code expired: nobody joined within 10 minutes. Enable remote access again for a new one."
                .to_owned()
        }
        Err(e) => format!("Remote access stopped: {e}"),
    }
}

/// The panel's state. Hold one in the host's app struct and call [`Self::ui`].
pub struct RemoteAccessPanel {
    phase: Arc<Mutex<Phase>>,
}

impl Default for RemoteAccessPanel {
    fn default() -> Self {
        Self { phase: Arc::new(Mutex::new(Phase::Idle { note: None })) }
    }
}

impl RemoteAccessPanel {
    /// The code on offer right now, if any.
    pub fn active_code(&self) -> Option<String> {
        self.phase.lock().ok().and_then(|p| p.code().map(str::to_owned))
    }

    /// Draw the panel's contents. `host_port` is the port the host's listener
    /// is bound to, or `None` while the host is stopped.
    pub fn ui(&mut self, ui: &mut egui::Ui, host_port: Option<u16>) {
        ui.small(
            "Show this computer's screen in a browser anywhere — another network or this one — \
             and drive it with a mouse and keyboard. Share the code below; they open \
             opensource.unisim.co.uk/screens/app, enter it, then enter this host's PIN.",
        );
        ui.add_space(4.0);
        ui.small("⚠ Relayed through UNI·SIM's cloud (end-to-end encrypted, so the relay can't read it) — slower than a local connection.");
        ui.add_space(4.0);

        let phase = self.phase.lock().map(|p| p.clone()).unwrap_or(Phase::Idle { note: None });
        match &phase {
            Phase::Waiting { code } | Phase::Connected { code } => {
                ui.horizontal(|ui| {
                    ui.label("Your code:");
                    ui.label(egui::RichText::new(code).heading().strong());
                    if ui.small_button("Copy").clicked() {
                        ui.ctx().copy_text(code.clone());
                    }
                    if ui.small_button("Copy link").clicked() {
                        ui.ctx().copy_text(format!("{REMOTE_CLIENT_URL}?remote={code}"));
                    }
                });
                if matches!(phase, Phase::Connected { .. }) {
                    ui.label("Connected — someone is viewing this screen.");
                } else {
                    ui.label("Waiting for the remote to connect… (the code lapses after 10 minutes unused)");
                }
            }
            Phase::Idle { note } => {
                if let Some(note) = note {
                    ui.label(note);
                }
                let label = if note.is_some() { "Enable remote access again" } else { "Enable remote access" };
                if ui.add_enabled(host_port.is_some(), egui::Button::new(label)).clicked() {
                    if let Some(port) = host_port {
                        self.start(ui.ctx().clone(), DEFAULT_ROOM_URL, port);
                    }
                }
                if host_port.is_none() {
                    ui.small("Start the host first, then enable remote access.");
                }
            }
        }
    }

    /// Mint a code and run the room on its own thread. `dial_room_observed`
    /// blocks for the whole session; the thread puts the panel back to `Idle`
    /// when it returns, whatever the reason, so a dead code is never left on
    /// screen.
    fn start(&mut self, ctx: egui::Context, room_url: &str, port: u16) {
        let code = crate::gen_room_code();
        if let Ok(mut p) = self.phase.lock() {
            *p = Phase::Waiting { code: code.clone() };
        }
        let phase = Arc::clone(&self.phase);
        let host_addr = format!("127.0.0.1:{port}");
        let room_url = room_url.to_owned();
        thread::spawn(move || {
            let paired_phase = Arc::clone(&phase);
            let paired_ctx = ctx.clone();
            let paired_code = code.clone();
            let mut on_paired = move || {
                if let Ok(mut p) = paired_phase.lock() {
                    *p = Phase::Connected { code: paired_code.clone() };
                }
                paired_ctx.request_repaint();
            };
            let res = dial_room_observed(&room_url, &code, &host_addr, &mut on_paired);
            if let Ok(mut p) = phase.lock() {
                *p = Phase::Idle { note: Some(end_note(&res)) };
            }
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_panel_offers_no_code() {
        assert_eq!(RemoteAccessPanel::default().active_code(), None);
    }

    #[test]
    fn every_way_a_room_ends_says_how_to_get_a_new_code() {
        for res in [Ok(RoomEnd::Ended), Ok(RoomEnd::Unpaired), Err(std::io::Error::other("room connect failed"))] {
            let note = end_note(&res);
            assert!(!note.is_empty());
            if res.is_ok() {
                assert!(note.contains("again"), "{note}");
            }
        }
    }

    /// The bug this module exists for: when the room ends (here the relay
    /// refuses the connection outright) the panel must drop the code and offer
    /// a new one, rather than keep showing a dead code until a restart.
    #[test]
    fn the_code_is_dropped_when_the_room_ends() {
        // A port nothing listens on: the dial fails at once.
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = free.local_addr().unwrap().port();
        drop(free);

        let mut panel = RemoteAccessPanel::default();
        panel.start(egui::Context::default(), &format!("ws://127.0.0.1:{port}"), 1);
        let first = panel.active_code().expect("a code is offered while the room is live");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while panel.active_code().is_some() {
            assert!(std::time::Instant::now() < deadline, "the dead code {first} stayed on offer");
            thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(matches!(&*panel.phase.lock().unwrap(), Phase::Idle { note: Some(_) }));
    }
}
