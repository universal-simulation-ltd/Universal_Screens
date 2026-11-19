//! "Connected now": who is using this computer through Universal Screens at
//! this moment, each with a **Disconnect** button.
//!
//! James, 2026-11-19: the window should show which connections are active and
//! let you end each one. Until then the only sign was a one-line status, and
//! the only way to end a session from this side was to stop the whole host.
//!
//! The accept loop calls [`Sessions::begin`] once a client's hello is in and
//! holds the [`SessionGuard`] for as long as it serves that client; dropping
//! the guard takes the row away, so the list cannot outlive the session even if
//! serving fails part-way. **Disconnect** shuts the socket down in both
//! directions: the session's blocking read returns at once, the next write
//! fails, and the host's own end-of-session path runs exactly as if the other
//! device had hung up (on a Mac that includes removing the virtual display).
//!
//! It is a list because the window should not have to change if a host ever
//! serves more than one client at a time. Today each host serves one.

use std::net::Shutdown;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use extender_protocol::{CaptureMode, ClientPlatform};
use extender_transport::Conn;

struct Active {
    id: u64,
    /// "SPECTRE", "Bradley's iPhone", or the platform when the device sent no name.
    who: String,
    /// The peer's address without the port.
    addr: String,
    mode: CaptureMode,
    since: Instant,
    /// A second handle on the session's socket, kept only to shut it down.
    /// `None` if the socket couldn't be cloned: the row still shows, without
    /// the button.
    closer: Option<Conn>,
    /// Disconnect was pressed; the row stays until the session has ended.
    ending: bool,
}

/// The live sessions, shared between the accept loop (which adds and removes
/// them) and the window (which lists them). Cheap to clone.
#[derive(Clone, Default)]
pub struct Sessions {
    inner: Arc<Mutex<Vec<Active>>>,
    next_id: Arc<AtomicU64>,
}

/// Held by the accept loop while it serves one client; dropping it removes
/// that client's row.
#[must_use = "the session's row disappears as soon as the guard is dropped"]
pub struct SessionGuard {
    sessions: Sessions,
    id: u64,
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        if let Ok(mut list) = self.sessions.inner.lock() {
            list.retain(|a| a.id != self.id);
        }
    }
}

impl Sessions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a session that is starting. `device_name` is what the client
    /// called itself in its hello (may be empty); `peer` is `ip:port`.
    pub fn begin(
        &self,
        conn: &Conn,
        device_name: &str,
        platform: ClientPlatform,
        peer: &str,
        mode: CaptureMode,
    ) -> SessionGuard {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let who = if device_name.trim().is_empty() {
            crate::platform_display(crate::platform_tag(platform)).to_owned()
        } else {
            device_name.trim().to_owned()
        };
        let addr = peer.rsplit_once(':').map_or(peer, |(ip, _)| ip).to_owned();
        let active = Active {
            id,
            who,
            addr,
            mode,
            since: Instant::now(),
            closer: conn.try_clone().ok(),
            ending: false,
        };
        if let Ok(mut list) = self.inner.lock() {
            list.push(active);
        }
        SessionGuard { sessions: self.clone(), id }
    }

    /// How many sessions are live.
    pub fn count(&self) -> usize {
        self.inner.lock().map_or(0, |l| l.len())
    }

    /// End the session `id` from this side. Returns whether there was one to end.
    pub fn disconnect(&self, id: u64) -> bool {
        let Ok(mut list) = self.inner.lock() else { return false };
        let Some(active) = list.iter_mut().find(|a| a.id == id) else { return false };
        active.ending = true;
        if let Some(conn) = &active.closer {
            let _ = conn.shutdown(Shutdown::Both);
        }
        true
    }

    /// Draw the "Connected now" list. Shows a quiet line when nothing is
    /// connected, so the place to look is always there.
    pub fn ui(&self, ui: &mut egui::Ui) {
        let rows: Vec<(u64, String, String, bool, bool)> = match self.inner.lock() {
            Ok(list) => list
                .iter()
                .map(|a| {
                    let what = format!(
                        "{} · {} · {}",
                        a.addr,
                        mode_label(a.mode),
                        connected_for(a.since.elapsed())
                    );
                    (a.id, a.who.clone(), what, a.closer.is_some(), a.ending)
                })
                .collect(),
            Err(_) => Vec::new(),
        };

        ui.label(egui::RichText::new(format!("Connected now ({})", rows.len())).strong());
        if rows.is_empty() {
            ui.small("Nothing is connected to this computer.");
            return;
        }
        let mut end = None;
        for (id, who, what, can_end, ending) in &rows {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(who).strong());
                ui.label(what);
                if *ending {
                    ui.small("disconnecting…");
                } else if *can_end
                    && ui
                        .button("Disconnect")
                        .on_hover_text("End this connection now. The other device can connect again.")
                        .clicked()
                {
                    end = Some(*id);
                }
            });
        }
        if let Some(id) = end {
            self.disconnect(id);
        }
        // The "for N min" counts up; nothing else would repaint an idle window.
        ui.ctx().request_repaint_after(Duration::from_secs(30));
    }
}

/// What the session is doing, in the window's words.
fn mode_label(mode: CaptureMode) -> &'static str {
    match mode {
        CaptureMode::VirtualDisplay => "second screen",
        CaptureMode::MirrorPrimary => "showing this screen",
        CaptureMode::ControlOnly => "remote control",
    }
}

/// "just now", "for 1 min", "for 2 h 5 min".
fn connected_for(elapsed: Duration) -> String {
    let mins = elapsed.as_secs() / 60;
    match mins {
        0 => "just now".to_owned(),
        m if m < 60 => format!("for {m} min"),
        m if m % 60 == 0 => format!("for {} h", m / 60),
        m => format!("for {} h {} min", m / 60, m % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};

    /// A connected pair: (the host's end as a `Conn`, the client's end).
    fn pair() -> (Conn, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        (Conn::Plain(server), client)
    }

    #[test]
    fn a_session_is_listed_until_its_guard_drops() {
        let sessions = Sessions::new();
        let (conn, _client) = pair();
        let guard = sessions.begin(&conn, "SPECTRE", ClientPlatform::Windows, "192.168.4.36:51234", CaptureMode::VirtualDisplay);
        assert_eq!(sessions.count(), 1);
        {
            let list = sessions.inner.lock().unwrap();
            assert_eq!(list[0].who, "SPECTRE");
            assert_eq!(list[0].addr, "192.168.4.36");
        }
        drop(guard);
        assert_eq!(sessions.count(), 0);
    }

    #[test]
    fn a_nameless_device_is_called_by_its_platform() {
        let sessions = Sessions::new();
        let (conn, _client) = pair();
        let _guard = sessions.begin(&conn, "  ", ClientPlatform::Ios, "10.0.0.2:1", CaptureMode::ControlOnly);
        assert_eq!(sessions.inner.lock().unwrap()[0].who, "iOS");
    }

    #[test]
    fn disconnect_ends_the_sessions_blocking_read() {
        use std::io::Read;
        let sessions = Sessions::new();
        let (conn, client) = pair();
        // The other device, as the real clients behave: when the host's side
        // closes, its read sees the end and it hangs up too. (⚠️ On Windows a
        // shutdown alone does not wake a recv blocked on another thread; it
        // returns once the peer closes, which every Screens client does. A peer
        // that never reacts held this test for 120 s.)
        let device = std::thread::spawn(move || {
            let mut client = client;
            let mut buf = [0u8; 1];
            let _ = client.read(&mut buf);
        });
        let _guard = sessions.begin(&conn, "Mac", ClientPlatform::Macos, "127.0.0.1:2", CaptureMode::MirrorPrimary);
        // The session's own read, as an accept loop would be blocked in it.
        let mut reader = conn.try_clone().unwrap();
        let blocked = std::thread::spawn(move || {
            let mut buf = [0u8; 1];
            reader.read(&mut buf)
        });
        std::thread::sleep(Duration::from_millis(100));
        assert!(sessions.disconnect(0));
        // Shut down: the read returns (EOF or an error) instead of waiting forever.
        let started = Instant::now();
        let result = blocked.join().unwrap();
        device.join().unwrap();
        assert!(matches!(result, Ok(0) | Err(_)), "{result:?}");
        assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
        assert!(sessions.inner.lock().unwrap()[0].ending);
        assert!(!sessions.disconnect(99));
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(connected_for(Duration::from_secs(20)), "just now");
        assert_eq!(connected_for(Duration::from_secs(60 * 7 + 5)), "for 7 min");
        assert_eq!(connected_for(Duration::from_secs(3600)), "for 1 h");
        assert_eq!(connected_for(Duration::from_secs(3600 * 2 + 300)), "for 2 h 5 min");
    }
}
