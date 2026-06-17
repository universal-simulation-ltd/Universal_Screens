//! Read the PC's current Wi-Fi network so the GUI can offer a "join this network"
//! QR (standard `WIFI:` payload) as step 1 of pairing — the phone has to be on the
//! same network as the host before it can reach it.
//!
//! Everything here shells out to Windows' `netsh wlan`, which exposes the current
//! SSID and (for a saved profile) the cleartext key without admin rights. Parsing
//! is best-effort and English-label based; anything unexpected yields `None`, and
//! the GUI simply falls back to a "connect to the same network" note.

use std::os::windows::process::CommandExt;
use std::process::Command;

/// Don't flash a console window when shelling out (we've called `FreeConsole`).
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The current Wi-Fi network, as much as we could read.
pub struct WifiInfo {
    pub ssid: String,
    /// The cleartext key, if the saved profile has one (open networks: `None`).
    pub password: Option<String>,
    /// QR auth tag: `"WPA"`, `"WEP"`, or `"nopass"`.
    pub auth: String,
}

impl WifiInfo {
    /// A standard Wi-Fi QR payload: `WIFI:T:WPA;S:<ssid>;P:<password>;;`. Phones
    /// recognise this and offer to join the network.
    pub fn qr_payload(&self) -> String {
        match &self.password {
            Some(p) => format!("WIFI:T:{};S:{};P:{};;", self.auth, esc(&self.ssid), esc(p)),
            None => format!("WIFI:T:nopass;S:{};;", esc(&self.ssid)),
        }
    }

    /// The password masked for display (one dot per char, capped), or `None`.
    pub fn masked_password(&self) -> Option<String> {
        self.password.as_ref().map(|p| "•".repeat(p.chars().count().min(12)))
    }
}

/// Read the current Wi-Fi network, or `None` if not on Wi-Fi (e.g. wired) or the
/// info couldn't be parsed.
pub fn current_wifi() -> Option<WifiInfo> {
    let ssid = current_ssid()?;
    let (password, auth) = profile_secrets(&ssid);
    Some(WifiInfo { ssid, password, auth })
}

/// Run `netsh wlan <args>` and capture stdout, suppressing its console window.
fn netsh(args: &[&str]) -> Option<String> {
    let out = Command::new("netsh")
        .creation_flags(CREATE_NO_WINDOW)
        .args(args)
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The SSID of the connected interface, from `netsh wlan show interfaces`.
fn current_ssid() -> Option<String> {
    let out = netsh(&["wlan", "show", "interfaces"])?;
    for line in out.lines() {
        let line = line.trim();
        // Match the "SSID" row but not "BSSID"; the value follows the first colon.
        if let Some(rest) = line.strip_prefix("SSID") {
            if let Some(value) = after_colon(rest) {
                if !value.is_empty() {
                    return Some(value);
                }
            }
        }
    }
    None
}

/// The cleartext key + auth tag for a saved profile, from
/// `netsh wlan show profile name=<ssid> key=clear`.
fn profile_secrets(ssid: &str) -> (Option<String>, String) {
    let mut password = None;
    let mut auth = "WPA".to_owned();
    if let Some(out) = netsh(&["wlan", "show", "profile", &format!("name={ssid}"), "key=clear"]) {
        for line in out.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("Key Content") {
                password = after_colon(rest).filter(|p| !p.is_empty());
            } else if let Some(rest) = line.strip_prefix("Authentication") {
                if let Some(v) = after_colon(rest) {
                    auth = if v.contains("Open") {
                        "nopass".to_owned()
                    } else if v.contains("WEP") {
                        "WEP".to_owned()
                    } else {
                        "WPA".to_owned()
                    };
                }
            }
        }
    }
    (password, auth)
}

/// The trimmed text after the first `:` on a `Label : value` line.
fn after_colon(s: &str) -> Option<String> {
    s.split_once(':').map(|(_, v)| v.trim().to_owned())
}

/// Backslash-escape the characters that are special in a `WIFI:` payload.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | ';' | ',' | ':' | '"') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_escapes_and_formats() {
        let w = WifiInfo {
            ssid: "My;Net".to_owned(),
            password: Some(r"pa:ss\1".to_owned()),
            auth: "WPA".to_owned(),
        };
        assert_eq!(w.qr_payload(), r"WIFI:T:WPA;S:My\;Net;P:pa\:ss\\1;;");
    }

    #[test]
    fn open_network_has_no_password() {
        let w = WifiInfo { ssid: "Cafe".to_owned(), password: None, auth: "nopass".to_owned() };
        assert_eq!(w.qr_payload(), "WIFI:T:nopass;S:Cafe;;");
        assert!(w.masked_password().is_none());
    }

    #[test]
    fn after_colon_takes_value() {
        assert_eq!(after_colon("   : Hello World ").as_deref(), Some("Hello World"));
        assert_eq!(after_colon("no colon"), None);
    }
}
