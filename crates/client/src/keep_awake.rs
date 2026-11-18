//! Keep this screen on while it shows another computer.
//!
//! A screen that is only being watched gets no local mouse or keyboard input,
//! so the OS decides it is idle and turns it off: James's Windows laptop went
//! black a minute or two into being his Mac's second screen (2026-11-18). The
//! client holds the display awake for as long as it runs; `--allow-sleep`
//! turns that off.
//!
//! Held by [`KeepAwake`], released when it drops (or the process exits).

/// Holds the display awake until dropped.
pub struct KeepAwake {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    child: Option<std::process::Child>,
}

impl KeepAwake {
    /// Start holding the display awake. Never fails: if the platform has no
    /// way to do it, it says so and carries on.
    pub fn hold() -> KeepAwake {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Power::{
                SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
            };
            // Per thread: this is the main (event loop) thread, which lives as
            // long as the window does.
            // SAFETY: a plain flags call with no pointers.
            let previous = unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED)
            };
            if previous == 0 {
                eprintln!("could not keep the screen awake (SetThreadExecutionState failed)");
            } else {
                println!("keeping this screen awake while it shows the other computer");
            }
            KeepAwake {}
        }
        #[cfg(target_os = "macos")]
        {
            // `caffeinate -d` holds a display-sleep assertion; `-w` ends it when
            // this process exits, even if it crashes.
            let child = std::process::Command::new("/usr/bin/caffeinate")
                .args(["-d", "-i", "-w", &std::process::id().to_string()])
                .spawn()
                .map_err(|e| eprintln!("could not keep the screen awake: {e}"))
                .ok();
            if child.is_some() {
                println!("keeping this screen awake while it shows the other computer");
            }
            KeepAwake { child }
        }
        #[cfg(target_os = "linux")]
        {
            // systemd-inhibit holds an idle inhibitor for as long as its command
            // runs; `sleep infinity` is killed on drop.
            let child = std::process::Command::new("systemd-inhibit")
                .args([
                    "--what=idle:sleep",
                    "--who=Universal Screens",
                    "--why=Showing another computer",
                    "sleep",
                    "infinity",
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|_| eprintln!("could not keep the screen awake (no systemd-inhibit)"))
                .ok();
            KeepAwake { child }
        }
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            KeepAwake {}
        }
    }
}

impl Drop for KeepAwake {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS};
            // SAFETY: as above.
            unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
