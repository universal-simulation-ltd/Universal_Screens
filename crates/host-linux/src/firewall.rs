//! Best-effort firewall helper — the Linux twin of the Windows host's
//! `netsh advfirewall` block, and deliberately a much smaller promise.
//!
//! On Windows there is one firewall, one command, and one UAC prompt, so that
//! host *offers to add the rule*. Linux has at least three front-ends over
//! netfilter (ufw, firewalld, raw nft/iptables) and no equivalent of the
//! "elevate this one command" gesture that doesn't involve either a terminal or
//! a polkit policy file we'd have to ship and register.
//!
//! ⚠️ **So this module never changes anything.** It detects which front-end is
//! active, works out whether the port is likely reachable, and hands the user the
//! exact command to paste. Attempting the change would mean a `pkexec` prompt
//! from a GUI that may not have a polkit agent running — a hang, not a dialog.
//!
//! ⚠️ **Most desktop Linux needs nothing at all.** Ubuntu ships ufw *inactive*,
//! Debian and Arch ship no active filtering, so the common case is
//! [`FirewallState::Inactive`] — the GUI must not nag there. Fedora is the
//! opposite: firewalld is on by default and *will* silently drop the phone's
//! connection while the host sits there logging nothing.

use std::process::Command;

/// What, if anything, is filtering inbound connections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirewallState {
    /// Nothing is running that would block us — the common desktop case.
    Inactive,
    /// ufw is active and the port is already allowed.
    UfwAllowed,
    /// ufw is active and the port is not allowed.
    UfwBlocked,
    /// firewalld is running and the port is already open.
    FirewalldAllowed,
    /// firewalld is running and the port is not open.
    FirewalldBlocked,
    /// Something is there but we couldn't interrogate it (no permission, or an
    /// unrecognised front-end). Say so rather than guessing "fine".
    Unknown,
}

impl FirewallState {
    /// Whether the phone can be expected to reach us on the LAN.
    pub fn is_ok(&self) -> bool {
        matches!(
            self,
            FirewallState::Inactive
                | FirewallState::UfwAllowed
                | FirewallState::FirewalldAllowed
        )
    }

    /// A one-line status for the GUI.
    pub fn summary(&self) -> &'static str {
        match self {
            FirewallState::Inactive => "No firewall blocking inbound connections",
            FirewallState::UfwAllowed => "ufw is active and this port is allowed",
            FirewallState::UfwBlocked => "ufw is active and will block this port",
            FirewallState::FirewalldAllowed => "firewalld is running and this port is open",
            FirewallState::FirewalldBlocked => "firewalld is running and will block this port",
            FirewallState::Unknown => "Couldn't check the firewall — if the phone can't connect, this is the first thing to look at",
        }
    }

    /// The command that would fix it, or `None` when nothing needs doing.
    /// Returned as text for the user to run — see the module note on why this
    /// module doesn't run it.
    pub fn fix_command(&self, port: u16) -> Option<String> {
        match self {
            FirewallState::UfwBlocked => Some(format!("sudo ufw allow {port}/tcp")),
            FirewallState::FirewalldBlocked => Some(format!(
                "sudo firewall-cmd --add-port={port}/tcp --permanent && sudo firewall-cmd --reload"
            )),
            _ => None,
        }
    }
}

/// Work out whether inbound TCP on `port` is likely to reach us.
///
/// Order matters: ufw is checked first because a machine running ufw is
/// *configuring* netfilter through it, and firewalld would not also be active.
pub fn state(port: u16) -> FirewallState {
    if let Some(state) = ufw_state(port) {
        return state;
    }
    if let Some(state) = firewalld_state(port) {
        return state;
    }
    // Neither front-end is installed or running. Raw nftables rules are possible
    // but a desktop with no ufw/firewalld and hand-written filtering is rare
    // enough — and unknowable enough without root — to call inactive.
    FirewallState::Inactive
}

/// ufw's state, or `None` when ufw isn't installed / isn't active.
///
/// ⚠️ `ufw status` needs root: as a normal user it fails outright rather than
/// printing an unprivileged view. That failure is *not* "no firewall" — it is
/// [`FirewallState::Unknown`], and conflating the two is how a user ends up
/// staring at a host that says everything is fine while the phone times out.
fn ufw_state(port: u16) -> Option<FirewallState> {
    // `ufw status` is root-only, but the config file that decides whether ufw is
    // enabled at all is world-readable, so check that first and avoid claiming
    // Unknown on the overwhelmingly common "ufw installed but off" case.
    let enabled = std::fs::read_to_string("/etc/ufw/ufw.conf")
        .ok()?
        .lines()
        .any(|l| l.trim() == "ENABLED=yes");
    if !enabled {
        return Some(FirewallState::Inactive);
    }
    let out = Command::new("ufw").arg("status").output().ok()?;
    if !out.status.success() {
        return Some(FirewallState::Unknown);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if port_mentioned(&text, port) {
        Some(FirewallState::UfwAllowed)
    } else {
        Some(FirewallState::UfwBlocked)
    }
}

/// firewalld's state, or `None` when it isn't running.
fn firewalld_state(port: u16) -> Option<FirewallState> {
    let running = Command::new("firewall-cmd").arg("--state").output().ok()?;
    if !running.status.success() {
        return None; // installed but not running
    }
    let out = Command::new("firewall-cmd")
        .args(["--query-port", &format!("{port}/tcp")])
        .output()
        .ok()?;
    // `--query-port` uses the exit code, not stdout: 0 = open, 1 = not open.
    Some(if out.status.success() {
        FirewallState::FirewalldAllowed
    } else {
        FirewallState::FirewalldBlocked
    })
}

/// Whether a `ufw status` table mentions our port as an allowed destination.
/// Matching the bare number would hit the "From" column and rule numbers, so the
/// port is matched with its `/tcp` (or `/udp`-free bare ALLOW) form.
fn port_mentioned(status: &str, port: u16) -> bool {
    let needle = format!("{port}/tcp");
    let bare = port.to_string();
    status.lines().any(|line| {
        if !line.contains("ALLOW") {
            return false;
        }
        let dest = line.split_whitespace().next().unwrap_or("");
        dest == needle || dest == bare
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_and_allowed_states_need_no_command() {
        assert!(FirewallState::Inactive.is_ok());
        assert!(FirewallState::Inactive.fix_command(9000).is_none());
        assert!(FirewallState::UfwAllowed.is_ok());
        assert!(FirewallState::FirewalldAllowed.is_ok());
    }

    #[test]
    fn blocked_states_hand_back_a_runnable_command() {
        assert_eq!(
            FirewallState::UfwBlocked.fix_command(9000).unwrap(),
            "sudo ufw allow 9000/tcp"
        );
        assert!(FirewallState::FirewalldBlocked
            .fix_command(9001)
            .unwrap()
            .contains("--add-port=9001/tcp"));
        assert!(!FirewallState::UfwBlocked.is_ok());
        assert!(!FirewallState::FirewalldBlocked.is_ok());
    }

    #[test]
    fn unknown_is_not_treated_as_fine() {
        // The distinction the module note exists for.
        assert!(!FirewallState::Unknown.is_ok());
        assert!(FirewallState::Unknown.fix_command(9000).is_none());
    }

    #[test]
    fn ufw_status_matches_the_destination_column_only() {
        let status = "\
Status: active

To                         Action      From
--                         ------      ----
9000/tcp                   ALLOW       Anywhere
22                         ALLOW       192.168.1.9000
";
        assert!(port_mentioned(status, 9000));
        // The 9000 in the From column must not count as an open port 9000 rule
        // for port 22's row — and 22 itself is allowed.
        assert!(port_mentioned(status, 22));
        assert!(!port_mentioned(status, 8080));
    }

    #[test]
    fn a_denied_rule_is_not_an_allow() {
        let status = "9000/tcp                   DENY        Anywhere\n";
        assert!(!port_mentioned(status, 9000));
    }
}
