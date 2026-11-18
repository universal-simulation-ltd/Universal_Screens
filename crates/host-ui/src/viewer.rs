//! "Show another computer here": the host window's way to WATCH another host,
//! rather than be watched. It runs the desktop client (`extender-client`) that
//! ships beside the host — inside the bundle's Resources on macOS, next to the
//! host exe on Windows — in a window of its own, in one of two modes: mirror
//! ("Show its screen") or extend ("Be its second screen": the other computer
//! makes a virtual display the size of this one's, and this window shows it).
//!
//! The second-screen button arrived 2026-11-18, when James wanted his Windows
//! laptop as a second screen for his Mac from the app rather than PowerShell;
//! the Windows host got this panel at the same time (it had been Mac-only).
//!
//! Until 2026-11-04 the only way in was a terminal command, and clicking a
//! computer under "Nearby" opened the phone pairing page in a browser, which
//! cannot show a screen on a desktop at all.
//!
//! The PIN is only needed the first time: the client remembers the pairing
//! (core's pairing store), so later connections leave it blank.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui;

/// What the client is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Watch (and drive) the other computer's main screen.
    Mirror,
    /// Be an extra screen for the other computer: it makes a virtual display.
    SecondScreen,
}

impl Mode {
    /// The client's flags for this mode. Extend is the client's default.
    fn args(self) -> &'static [&'static str] {
        match self {
            Mode::Mirror => &["--mirror"],
            Mode::SecondScreen => &[],
        }
    }
}

/// Where a viewer stands, shared with the thread that waits on the client.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    /// Nothing open. `note` is how the last one ended, if it said anything.
    Idle { note: Option<String> },
    /// The client is running (connecting, or showing the screen).
    Open { addr: String, mode: Mode },
}

/// The panel's state. Hold one in the host's app struct, call [`Self::ui`]
/// where the panel goes, and [`Self::choose`] when a nearby computer is clicked.
pub struct ViewerPanel {
    address: String,
    pin: String,
    /// Which side of the other computer's main screen this one should sit on
    /// as its second screen. `None` = leave it where that computer last put it.
    side: Option<extender_protocol::DisplaySide>,
    /// Set by [`Self::choose`] so the next frame opens the collapsed panel.
    reveal: bool,
    phase: Arc<Mutex<Phase>>,
}

impl Default for ViewerPanel {
    fn default() -> Self {
        Self {
            address: String::new(),
            pin: String::new(),
            side: None,
            reveal: false,
            phase: Arc::new(Mutex::new(Phase::Idle { note: None })),
        }
    }
}

impl ViewerPanel {
    /// Fill in a computer found on the network and open the panel on it.
    pub fn choose(&mut self, addr: &str, port: u16) {
        self.address = format!("{addr}:{port}");
        self.pin.clear();
        self.reveal = true;
    }

    /// Draw the panel: a collapsing header titled "Show another computer here".
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let open = std::mem::take(&mut self.reveal).then_some(true);
        egui::CollapsingHeader::new("Show another computer here")
            .open(open)
            .default_open(false)
            .show(ui, |ui| self.contents(ui));
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        ui.small(
            "See another computer's screen in a window on this one, or make this one its \
             second screen, and drive it with this mouse and keyboard. Start Universal \
             Screens on that computer first.",
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Address:");
            ui.add(egui::TextEdit::singleline(&mut self.address).hint_text("192.168.1.42:9000").desired_width(150.0));
        });
        ui.horizontal(|ui| {
            ui.label("PIN:");
            ui.add(
                egui::TextEdit::singleline(&mut self.pin)
                    .hint_text("1234")
                    .char_limit(4)
                    .password(true)
                    .desired_width(60.0),
            );
            ui.small("only the first time");
        });

        let phase = self.phase.lock().map(|p| p.clone()).unwrap_or(Phase::Idle { note: None });
        match &phase {
            Phase::Open { addr, mode } => {
                let what = match mode {
                    Mode::Mirror => format!("Showing {addr} in its own window."),
                    Mode::SecondScreen => format!(
                        "This is now a second screen for {addr}. Move windows onto it there; \
                         press F in that window for full screen, or an arrow key to move it to \
                         that side of {addr}'s screen."
                    ),
                };
                ui.label(format!("{what} Close that window to stop."));
            }
            Phase::Idle { note } => {
                let ready = !self.address.trim().is_empty();
                // Only meaningful for "Be its second screen"; asked for only when
                // chosen, because a host older than 0.4.8 can't take it.
                ui.horizontal_wrapped(|ui| {
                    ui.label("As its second screen, this sits:");
                    for (side, text) in [
                        (None, "where it was"),
                        (Some(extender_protocol::DisplaySide::Left), "left of it"),
                        (Some(extender_protocol::DisplaySide::Right), "right of it"),
                        (Some(extender_protocol::DisplaySide::Above), "above it"),
                        (Some(extender_protocol::DisplaySide::Below), "below it"),
                    ] {
                        if ui.selectable_label(self.side == side, text).clicked() {
                            self.side = side;
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(ready, egui::Button::new("Show its screen")).clicked() {
                        self.launch(Mode::Mirror);
                    }
                    if ui
                        .add_enabled(ready, egui::Button::new("Be its second screen"))
                        .on_hover_text("The other computer gets an extra display the size of this screen")
                        .clicked()
                    {
                        self.launch(Mode::SecondScreen);
                    }
                });
                if let Some(note) = note {
                    ui.colored_label(ui.visuals().warn_fg_color, note);
                }
            }
        }
    }

    /// Run the client on the typed address, and wait for it on a thread that
    /// puts the panel back to `Idle` — with the client's own explanation if it
    /// failed — when it exits.
    fn launch(&mut self, mode: Mode) {
        let Some(client) = client_path() else {
            self.set(Phase::Idle {
                note: Some("The viewer isn't installed with this copy of Universal Screens.".to_owned()),
            });
            return;
        };
        let addr = normalise_addr(&self.address);
        let mut cmd = Command::new(client);
        cmd.arg(&addr).args(mode.args());
        if let (Mode::SecondScreen, Some(side)) = (mode, self.side) {
            cmd.arg("--side").arg(side.as_str());
        }
        let pin = self.pin.trim();
        if !pin.is_empty() {
            cmd.arg("--pin").arg(pin);
        }
        // stdout goes nowhere rather than a pipe nobody drains: the client
        // prints freely, and a closed pipe would panic it mid-start.
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                self.set(Phase::Idle { note: Some(format!("Couldn't open the viewer: {e}")) });
                return;
            }
        };
        self.pin.clear();
        self.set(Phase::Open { addr: addr.clone(), mode });

        let phase = Arc::clone(&self.phase);
        thread::spawn(move || {
            let mut problem: Vec<String> = Vec::new();
            if let Some(stderr) = child.stderr.take() {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if let Some(msg) = line.strip_prefix("client error: ") {
                        problem = vec![sentence(msg)];
                    } else if !problem.is_empty() {
                        // The hint the client prints after an error.
                        problem.push(sentence(&line.replace("run again with --pin NNNN", "enter it above")));
                    }
                }
            }
            let _ = child.wait();
            let note = (!problem.is_empty()).then(|| problem.join(" "));
            if let Ok(mut p) = phase.lock() {
                *p = Phase::Idle { note };
            }
        });
    }

    fn set(&self, phase: Phase) {
        if let Ok(mut p) = self.phase.lock() {
            *p = phase;
        }
    }
}

/// The desktop client shipped with this host: `../Resources/` inside a macOS
/// bundle, otherwise beside the host's own executable (the Windows install,
/// and `target/<profile>/` in development).
fn client_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = if cfg!(windows) { "extender-client.exe" } else { "extender-client" };
    [dir.join("../Resources").join(name), dir.join(name)].into_iter().find(|p| p.is_file())
}

/// Default the port: a bare address means the host's usual one.
fn normalise_addr(typed: &str) -> String {
    let typed = typed.trim();
    if typed.contains(':') {
        typed.to_owned()
    } else {
        format!("{typed}:{}", crate::BASE_PORT)
    }
}

/// Capitalise and full-stop one of the client's messages for the window.
fn sentence(msg: &str) -> String {
    let msg = msg.trim().trim_end_matches('.');
    let mut chars = msg.chars();
    match chars.next() {
        Some(first) => format!("{}{}.", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_address_gets_the_usual_port() {
        assert_eq!(normalise_addr(" 192.168.1.42 "), format!("192.168.1.42:{}", crate::BASE_PORT));
        assert_eq!(normalise_addr("192.168.1.42:9003"), "192.168.1.42:9003");
    }

    #[test]
    fn second_screen_is_the_clients_default_mode() {
        assert_eq!(Mode::Mirror.args(), ["--mirror"]);
        assert!(Mode::SecondScreen.args().is_empty());
    }

    #[test]
    fn client_messages_read_as_sentences() {
        assert_eq!(
            sentence("not paired: this device must pair again with the code"),
            "Not paired: this device must pair again with the code."
        );
        assert_eq!(sentence(""), "");
    }
}
