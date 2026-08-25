//! Read this machine's current Wi-Fi network so the GUI can offer a "join this
//! network" QR (standard `WIFI:` payload) as step 1 of pairing — the phone has to
//! be on the same network as the host before it can reach it.
//!
//! The Linux twin of the Windows host's `netsh wlan` block. Everything here
//! shells out to **`nmcli`**, and that is a narrower answer than `netsh`:
//!
//! ⚠️ **The SSID is easy; the password often isn't.** `nmcli` will print a saved
//! connection's PSK, but reading a *secret* goes through polkit, and on most
//! desktop distros a non-root caller gets an interactive auth prompt — which
//! would hang a GUI shelling out with piped stdio. So the read is run with
//! `--ask` deliberately *off*: it either returns the key immediately or fails,
//! and a failure degrades to the SSID-only QR rather than blocking.
//!
//! ⚠️ **Not every system has NetworkManager.** iwd and systemd-networkd expose
//! nothing equivalent, and a machine on Ethernet has no SSID at all. All of those
//! yield `None`, and the GUI falls back to a plain "connect to the same network"
//! note. Callers must treat Wi-Fi info as a bonus, never a precondition.

use std::process::Command;

/// The current Wi-Fi network, as much as we could read.
pub struct WifiInfo {
    pub ssid: String,
    /// The cleartext key, if the saved profile gave one up without prompting
    /// (open networks, and anything polkit refused: `None`).
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

/// Read the current Wi-Fi network, or `None` if not on Wi-Fi (e.g. wired), if
/// NetworkManager isn't the network stack, or if the info couldn't be parsed.
pub fn current_wifi() -> Option<WifiInfo> {
    let ssid = current_ssid()?;
    let (password, auth) = profile_secrets(&ssid);
    Some(WifiInfo { ssid, password, auth })
}

/// Run `nmcli` with terse, field-selected output and capture stdout. `-t -f`
/// gives colon-separated fields with no headers and no localised labels, so this
/// parses identically under any system locale — unlike the Windows host's
/// English-label scrape of `netsh`.
fn nmcli(args: &[&str]) -> Option<String> {
    let out = Command::new("nmcli").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The SSID of the active Wi-Fi connection, or `None` when there isn't one.
fn current_ssid() -> Option<String> {
    // `ACTIVE:SSID` over the scan list: exactly one row is `yes` when associated.
    let out = nmcli(&["-t", "-f", "ACTIVE,SSID", "dev", "wifi"])?;
    out.lines()
        .find_map(|line| line.strip_prefix("yes:"))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(unescape_field)
}

/// The saved profile's PSK and the QR auth tag for `ssid`.
///
/// Returns `(None, "nopass")` for an open network *and* for the case polkit
/// refused — those are indistinguishable from here, and both mean the same thing
/// to the QR: don't claim to carry a key. The security type is read separately so
/// a protected network we can't read still reports `WPA`.
fn profile_secrets(ssid: &str) -> (Option<String>, String) {
    let key_mgmt = nmcli(&[
        "-t",
        "-f",
        "802-11-wireless-security.key-mgmt",
        "connection",
        "show",
        ssid,
    ])
    .and_then(|s| field_value(&s))
    .unwrap_or_default();

    let auth = match key_mgmt.as_str() {
        "" | "none" => "nopass",
        "wep" | "shared" | "ieee8021x" => "WEP",
        _ => "WPA", // wpa-psk, wpa-eap, sae (WPA3) …
    };

    if auth == "nopass" {
        return (None, auth.to_owned());
    }

    // `-s` asks for secrets. Without `--ask` this returns empty (or fails)
    // rather than prompting when polkit won't authorise it non-interactively.
    let psk = nmcli(&[
        "-s",
        "-t",
        "-f",
        "802-11-wireless-security.psk",
        "connection",
        "show",
        ssid,
    ])
    .and_then(|s| field_value(&s))
    .filter(|p| !p.is_empty());

    (psk, auth.to_owned())
}

/// Pull the value out of one terse `nmcli` `field:value` line.
///
/// ⚠️ Split on the *first* colon only: a PSK is free-form text and may well
/// contain colons of its own.
fn field_value(out: &str) -> Option<String> {
    out.lines()
        .next()?
        .split_once(':')
        .map(|(_, v)| unescape_field(v.trim()))
}

/// Undo `nmcli -t`'s escaping. In terse mode it backslash-escapes the field
/// separator and the backslash itself, so an SSID containing a colon survives the
/// round trip — but only if the caller unescapes it.
fn unescape_field(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Escape the `WIFI:` payload's delimiters, per the de-facto QR spec.
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
    fn qr_payload_carries_the_key_when_we_have_one() {
        let w = WifiInfo {
            ssid: "Office".to_owned(),
            password: Some("hunter2".to_owned()),
            auth: "WPA".to_owned(),
        };
        assert_eq!(w.qr_payload(), "WIFI:T:WPA;S:Office;P:hunter2;;");
    }

    #[test]
    fn qr_payload_for_an_unreadable_key_is_the_open_form() {
        // polkit refused, so we have an SSID and no key — the QR must not claim
        // to carry one, or the phone joins with an empty password and fails.
        let w = WifiInfo { ssid: "Cafe".to_owned(), password: None, auth: "nopass".to_owned() };
        assert_eq!(w.qr_payload(), "WIFI:T:nopass;S:Cafe;;");
    }

    #[test]
    fn payload_delimiters_are_escaped() {
        let w = WifiInfo {
            ssid: "Bob's; Wi-Fi".to_owned(),
            password: Some("a:b;c".to_owned()),
            auth: "WPA".to_owned(),
        };
        assert_eq!(w.qr_payload(), "WIFI:T:WPA;S:Bob's\\; Wi-Fi;P:a\\:b\\;c;;");
    }

    #[test]
    fn terse_output_splits_on_the_first_colon_only() {
        // A PSK with a colon in it: splitting on every colon would truncate it.
        assert_eq!(
            field_value("802-11-wireless-security.psk:pa:ss:word"),
            Some("pa:ss:word".to_owned())
        );
    }

    #[test]
    fn terse_escaping_is_undone() {
        // nmcli -t escapes the separator, so a colon in an SSID arrives as "\:".
        assert_eq!(unescape_field("Guest\\:Net"), "Guest:Net");
        assert_eq!(unescape_field("back\\\\slash"), "back\\slash");
        assert_eq!(unescape_field("plain"), "plain");
    }

    #[test]
    fn masked_password_is_capped() {
        let w = WifiInfo {
            ssid: "X".to_owned(),
            password: Some("a".repeat(40)),
            auth: "WPA".to_owned(),
        };
        assert_eq!(w.masked_password().unwrap().chars().count(), 12);
    }
}
