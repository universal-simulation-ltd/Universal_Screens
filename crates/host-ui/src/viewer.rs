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
use std::process::{Child, Command, Stdio};
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

/// The panel's choices that are worth keeping between launches. The host saves
/// them with its other settings ([`ViewerPanel::prefs`] / [`ViewerPanel::set_prefs`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ViewerPrefs {
    /// The last address shown, so a reopened app is one click from reconnecting.
    pub address: String,
    pub side: Option<extender_protocol::DisplaySide>,
    /// The second screen's pixel size; `None` = this screen's full size.
    pub size: Option<(u32, u32)>,
    /// The second screen's scaling in percent; `None` = this screen's own.
    pub scale: Option<u16>,
    /// Let this screen turn off while it shows the other computer. Off (the
    /// screen is kept awake) unless ticked: James's laptop kept going black.
    pub allow_sleep: bool,
    /// Open the viewer full screen (`--fullscreen`). Off unless ticked; the
    /// viewer shows "Press Esc to exit full screen" when it opens.
    pub fullscreen: bool,
}

/// The panel's state. Hold one in the host's app struct, call [`Self::ui`]
/// where the panel goes, and [`Self::choose`] when a nearby computer is clicked.
pub struct ViewerPanel {
    address: String,
    pin: String,
    /// Which side of the other computer's main screen this one should sit on
    /// as its second screen. `None` = leave it where that computer last put it.
    side: Option<extender_protocol::DisplaySide>,
    size: Option<(u32, u32)>,
    /// What's typed in the custom size box, while "Custom" is chosen.
    size_draft: String,
    custom_size: bool,
    scale: Option<u16>,
    allow_sleep: bool,
    fullscreen: bool,
    /// Set by [`Self::choose`] so the next frame opens the collapsed panel.
    reveal: bool,
    phase: Arc<Mutex<Phase>>,
    /// The running viewer, so **Disconnect** can end it from this window.
    child: Arc<Mutex<Option<Child>>>,
}

impl Default for ViewerPanel {
    fn default() -> Self {
        Self {
            address: String::new(),
            pin: String::new(),
            side: None,
            size: None,
            size_draft: String::new(),
            custom_size: false,
            scale: None,
            allow_sleep: false,
            fullscreen: false,
            reveal: false,
            phase: Arc::new(Mutex::new(Phase::Idle { note: None })),
            child: Arc::new(Mutex::new(None)),
        }
    }
}

impl ViewerPanel {
    /// What to save between launches.
    pub fn prefs(&self) -> ViewerPrefs {
        ViewerPrefs {
            address: self.address.clone(),
            side: self.side,
            size: self.size,
            scale: self.scale,
            allow_sleep: self.allow_sleep,
            fullscreen: self.fullscreen,
        }
    }

    /// Restore what [`Self::prefs`] saved.
    pub fn set_prefs(&mut self, prefs: ViewerPrefs) {
        self.address = prefs.address;
        self.side = prefs.side;
        self.size = prefs.size;
        self.scale = prefs.scale;
        self.allow_sleep = prefs.allow_sleep;
        self.fullscreen = prefs.fullscreen;
        if let Some((w, h)) = self.size {
            self.size_draft = format!("{w}×{h}");
        }
    }

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
                         in that window, F switches full screen on and off (Esc leaves it), and \
                         an arrow key moves it to that side of {addr}'s screen."
                    ),
                };
                ui.label(format!("{what} Close that window, or disconnect here, to stop."));
                if ui
                    .button("Disconnect")
                    .on_hover_text("Close the viewer and end the connection to the other computer")
                    .clicked()
                {
                    self.disconnect();
                }
            }
            Phase::Idle { note } => {
                let ready = !self.address.trim().is_empty();
                self.second_screen_options(ui);
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

    /// Size and scaling for "Be its second screen", whether the viewer opens
    /// full screen, and whether this screen may sleep while it shows the other
    /// computer.
    fn second_screen_options(&mut self, ui: &mut egui::Ui) {
        // This screen, as egui sees it: size in points and pixels per point.
        let (native, own_scale) = ui.ctx().input(|i| {
            let vp = i.viewport();
            let ppp = vp.native_pixels_per_point.unwrap_or(i.pixels_per_point);
            let px = vp.monitor_size.map(|s| ((s.x * ppp).round() as u32 & !1, (s.y * ppp).round() as u32 & !1));
            (px, ((ppp * 4.0).round() * 25.0) as u16)
        });
        let own_scale = own_scale.clamp(100, 300);

        ui.horizontal_wrapped(|ui| {
            ui.label("Size:");
            let full = match native {
                Some((w, h)) => format!("full size of this screen ({w}×{h})"),
                None => "full size of this screen".to_owned(),
            };
            let current = if self.custom_size {
                "custom".to_owned()
            } else {
                match self.size {
                    None => full.clone(),
                    Some((w, h)) => format!("{w}×{h}"),
                }
            };
            egui::ComboBox::from_id_salt("viewer-size")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.size.is_none() && !self.custom_size, full).clicked() {
                        self.size = None;
                        self.custom_size = false;
                    }
                    if let Some(native) = native {
                        // The client's own presets, with their real pixel sizes.
                        for pct in [75u32, 67, 50] {
                            let w = (native.0 * pct / 100) & !1;
                            let h = (native.1 * pct / 100) & !1;
                            let label = format!("{w}×{h} ({pct}%)");
                            if ui.selectable_label(self.size == Some((w, h)) && !self.custom_size, label).clicked() {
                                self.size = Some((w, h));
                                self.custom_size = false;
                            }
                        }
                    }
                    if ui.selectable_label(self.custom_size, "custom…").clicked() {
                        self.custom_size = true;
                    }
                });
            if self.custom_size {
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.size_draft)
                        .hint_text("2560×1600")
                        .desired_width(90.0),
                );
                if edit.changed() {
                    self.size = parse_size(&self.size_draft);
                }
                if self.size.is_none() && !self.size_draft.trim().is_empty() {
                    ui.small("width × height, e.g. 2560×1600");
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Scaling:");
            let same = format!("same as this screen ({own_scale}%)");
            let current = self.scale.map_or(same.clone(), |s| format!("{s}%"));
            egui::ComboBox::from_id_salt("viewer-scale")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.scale.is_none(), same).clicked() {
                        self.scale = None;
                    }
                    for pct in [100u16, 125, 150, 175, 200, 225, 250, 300] {
                        if ui.selectable_label(self.scale == Some(pct), format!("{pct}%")).clicked() {
                            self.scale = Some(pct);
                        }
                    }
                })
                .response
                .on_hover_text("How big things look on the second screen, like Windows' display scaling");
        });
        ui.checkbox(&mut self.fullscreen, "Open full screen (press Esc to leave it)");
        ui.checkbox(&mut self.allow_sleep, "Let this screen turn off while it shows the other computer");
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
        if mode == Mode::SecondScreen {
            if let Some(side) = self.side {
                cmd.arg("--side").arg(side.as_str());
            }
            if let Some((w, h)) = self.size {
                cmd.arg("--size").arg(format!("{w}x{h}"));
            }
            if let Some(scale) = self.scale {
                cmd.arg("--scale").arg(scale.to_string());
            }
        }
        if self.allow_sleep {
            cmd.arg("--allow-sleep");
        }
        if self.fullscreen {
            cmd.arg("--fullscreen");
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

        let stderr = child.stderr.take();
        if let Ok(mut slot) = self.child.lock() {
            *slot = Some(child);
        }
        let held = Arc::clone(&self.child);
        let phase = Arc::clone(&self.phase);
        thread::spawn(move || {
            let mut problem: Vec<String> = Vec::new();
            // Runs until the client exits (or is killed by Disconnect): its
            // stderr closes with it.
            if let Some(stderr) = stderr {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if let Some(msg) = line.strip_prefix("client error: ") {
                        problem = vec![sentence(msg)];
                    } else if !problem.is_empty() {
                        // The hint the client prints after an error.
                        problem.push(sentence(&line.replace("run again with --pin NNNN", "enter it above")));
                    }
                }
            }
            if let Some(mut child) = held.lock().ok().and_then(|mut slot| slot.take()) {
                let _ = child.wait();
            }
            let note = (!problem.is_empty()).then(|| problem.join(" "));
            if let Ok(mut p) = phase.lock() {
                *p = Phase::Idle { note };
            }
        });
    }

    /// End the running viewer, which closes its connection. The waiting thread
    /// then puts the panel back to `Idle` as for any other exit.
    fn disconnect(&self) {
        if let Ok(mut slot) = self.child.lock() {
            if let Some(child) = slot.as_mut() {
                let _ = child.kill();
            }
        }
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

/// `2560×1600` (or `2560x1600`) → (2560, 1600), even-rounded; the client's rule.
fn parse_size(text: &str) -> Option<(u32, u32)> {
    let text = text.trim().replace(['×', 'X'], "x");
    let (w, h) = text.split_once('x')?;
    let (w, h): (u32, u32) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    (w >= 320 && h >= 240).then_some((w & !1, h & !1))
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
    fn prefs_survive_a_round_trip_through_the_panel() {
        let prefs = ViewerPrefs {
            address: "192.168.4.20:9000".to_owned(),
            side: Some(extender_protocol::DisplaySide::Right),
            size: Some((2560, 1600)),
            scale: Some(150),
            allow_sleep: true,
            fullscreen: true,
        };
        let mut panel = ViewerPanel::default();
        panel.set_prefs(prefs.clone());
        assert_eq!(panel.prefs(), prefs);
        assert_eq!(parse_size("2560×1600"), Some((2560, 1600)));
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
