//! Best-effort Windows Firewall helper. Reading rules needs no admin; adding one
//! does, so we *detect* whether inbound TCP on the host's port is already allowed
//! and, only when the user asks, add a rule via an elevated `netsh` (one UAC
//! prompt). Without the rule, phones on the same Wi-Fi can't reach the host even
//! though it's listening (loopback/USB still works).

use std::os::windows::process::CommandExt;
use std::process::Command;

/// Don't flash a console window when shelling out (we've called `FreeConsole`).
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The inbound-rule name we manage, scoped to the port (ASCII, so we can match it
/// in `netsh` output regardless of the system locale).
fn rule_name(port: u16) -> String {
    format!("Universal Screens (TCP {port})")
}

/// Whether an inbound allow rule for `port` already exists.
pub fn rule_present(port: u16) -> bool {
    let name = rule_name(port);
    let out = Command::new("netsh")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["advfirewall", "firewall", "show", "rule", &format!("name={name}")])
        .output();
    match out {
        // `netsh` echoes the rule name when it exists, and prints a "No rules
        // match…" line (localised) when it doesn't — so match on our ASCII name.
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains(&name),
        Err(_) => false,
    }
}

/// Add an inbound allow rule for `port` via an elevated `netsh` (prompts UAC).
/// Fire-and-forget — we don't block on the elevation result. The rule covers the
/// currently-active network profile(s) — crucially including **Public**, since
/// many shared/guest Wi-Fis are categorised Public and a private/domain-only rule
/// wouldn't apply there.
pub fn request_allow(port: u16) {
    let name = rule_name(port);
    let profile = active_profiles().unwrap_or_else(|| "domain,private,public".to_owned());
    // Elevate via PowerShell's Start-Process -Verb RunAs. Each ArgumentList element
    // is a single-quoted PowerShell string, so the rule name's spaces/parens reach
    // netsh as one argument.
    let ps = format!(
        "Start-Process netsh -Verb RunAs -WindowStyle Hidden -ArgumentList \
         @('advfirewall','firewall','add','rule','name={name}','dir=in','action=allow',\
         'protocol=TCP','localport={port}','profile={profile}')"
    );
    let _ = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-Command", &ps])
        .spawn();
}

/// The netsh profile list for the currently-connected network(s), e.g.
/// `"public"` or `"private,public"`. `None` if it can't be determined.
fn active_profiles() -> Option<String> {
    let out = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-Command", "(Get-NetConnectionProfile).NetworkCategory"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut profiles = Vec::new();
    for line in text.lines() {
        let p = match line.trim() {
            "Public" => "public",
            "Private" => "private",
            "DomainAuthenticated" | "Domain" => "domain",
            _ => continue,
        };
        if !profiles.contains(&p) {
            profiles.push(p);
        }
    }
    if profiles.is_empty() {
        None
    } else {
        Some(profiles.join(","))
    }
}
