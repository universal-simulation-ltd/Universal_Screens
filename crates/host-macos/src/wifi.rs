//! Read the Mac's current Wi-Fi network so the GUI can embed the SSID and
//! password in the combined connect QR (https URL fragment).
//!
//! SSID: via the `airport` CLI (private, present on macOS 10.7–14).
//! Password: via `security find-generic-password` against the System keychain.
//! macOS shows a one-time "Allow / Always Allow" dialog; after "Always Allow"
//! subsequent reads are silent — same end-result as the Windows netsh path.

use std::process::Command;

/// The current Wi-Fi network.
pub struct WifiInfo {
    pub ssid: String,
    /// Cleartext key read from the System keychain (triggers a one-time dialog).
    pub password: Option<String>,
    /// QR auth tag: `"WPA"`, `"WEP"`, or `"nopass"`.
    pub auth: String,
}

impl WifiInfo {
    pub fn masked_password(&self) -> Option<String> {
        self.password.as_ref().map(|p| "•".repeat(p.chars().count().min(12)))
    }
}

pub fn current_wifi() -> Option<WifiInfo> {
    let ssid = current_ssid()?;
    let password = keychain_wifi_password(&ssid);
    Some(WifiInfo { ssid, password, auth: "WPA".to_owned() })
}

/// Read the Wi-Fi password for `ssid` from the System keychain.
/// Triggers a macOS "Allow access" dialog on first call; silent after "Always Allow".
fn keychain_wifi_password(ssid: &str) -> Option<String> {
    let out = Command::new("security")
        .args(["find-generic-password", "-a", ssid, "-w"])
        .output()
        .ok()?;
    if out.status.success() {
        let pw = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if !pw.is_empty() { Some(pw) } else { None }
    } else {
        None
    }
}

/// SSID of the connected Wi-Fi interface, via the `airport` utility.
fn current_ssid() -> Option<String> {
    let airport = "/System/Library/PrivateFrameworks/Apple80211.framework\
                   /Versions/Current/Resources/airport";
    let out = Command::new(airport).arg("-I").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let line = line.trim();
        // airport -I outputs lines like "  SSID: MyNetwork"
        if let Some(rest) = line.strip_prefix("SSID: ") {
            let ssid = rest.trim().to_owned();
            if !ssid.is_empty() {
                return Some(ssid);
            }
        }
    }
    None
}

