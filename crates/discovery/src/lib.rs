//! LAN discovery: beacon sender + listener so two Universal Screens hosts on the
//! same network can see each other without scanning a QR (PC → PC / PC → Mac).
//!
//! Protocol: UDP multicast on `224.0.0.251:9001`. Each running host broadcasts a
//! small packet every 2s; the listener collects peers and prunes ones not heard
//! from in 6s. The format is `USSCREENS\t{port}\t{name}` — tab-separated so name
//! can safely contain colons (Windows machine names often do not, but just in case).
//!
//! This crate is deliberately platform-agnostic (no UI): every host shares one
//! implementation and one wire format. Instead of poking a specific UI toolkit,
//! the listener calls an `on_change` closure whenever the peer list changes —
//! each host wraps it with a thin adapter that turns that into a repaint
//! (e.g. `egui::Context::request_repaint`). See `docs/M9-lan-discovery.md`.
//!
//! Alongside the custom beacon, a serving host also advertises itself over
//! standard DNS-SD/mDNS (see [`advertise_mdns`]) so the phone apps can browse
//! for hosts with their platform service APIs — Android NSD and iOS Bonjour
//! (`NWBrowser`/`NetServiceBrowser`). iOS in particular cannot join a raw
//! multicast group without a restricted Apple entitlement, but Bonjour browsing
//! needs none, so DNS-SD is the mobile-facing half of discovery.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Multicast group + port the beacon is sent to and the listener joins.
pub const MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
pub const DISCOVERY_PORT: u16 = 9001;
const BEACON_INTERVAL: Duration = Duration::from_secs(2);
const PEER_TTL: Duration = Duration::from_secs(6);

#[derive(Clone)]
pub struct DiscoveredPeer {
    pub name: String,
    /// The source IP address of the peer's beacon.
    pub addr: String,
    pub port: u16,
    pub last_seen: Instant,
}

/// Spawn a background listener thread that collects beacon packets into `peers`.
/// `own_ip` is checked each packet so the host doesn't list itself. `on_change`
/// fires whenever the visible peer set changes (a peer joins or is pruned) so the
/// caller can repaint. The thread runs until `stop` is set (or the process exits).
pub fn start_listener<F>(
    peers: Arc<Mutex<Vec<DiscoveredPeer>>>,
    stop: Arc<AtomicBool>,
    own_ip: Arc<Mutex<Option<String>>>,
    on_change: F,
) where
    F: Fn() + Send + 'static,
{
    std::thread::spawn(move || {
        let sock = match UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)) {
            Ok(s) => s,
            Err(_) => return, // port already taken (e.g. second instance); silent no-op
        };
        let _ = sock.join_multicast_v4(&MULTICAST_ADDR, &Ipv4Addr::UNSPECIFIED);
        let _ = sock.set_read_timeout(Some(Duration::from_millis(500)));

        let mut buf = [0u8; 256];
        while !stop.load(Ordering::Relaxed) {
            // Prune stale peers each loop iteration.
            {
                let mut list = peers.lock().unwrap();
                let before = list.len();
                list.retain(|p| p.last_seen.elapsed() < PEER_TTL);
                if list.len() != before {
                    drop(list);
                    on_change();
                }
            }

            match sock.recv_from(&mut buf) {
                Ok((n, from_addr)) => {
                    let from_ip = match from_addr {
                        SocketAddr::V4(a) => a.ip().to_string(),
                        SocketAddr::V6(_) => continue,
                    };
                    // Don't list ourselves.
                    if own_ip.lock().unwrap().as_deref() == Some(from_ip.as_str()) {
                        continue;
                    }
                    let Some((port, name)) = parse_beacon(&buf[..n]) else { continue };
                    let mut list = peers.lock().unwrap();
                    if let Some(existing) = list.iter_mut().find(|p| p.addr == from_ip) {
                        existing.last_seen = Instant::now();
                        existing.name = name;
                        existing.port = port;
                    } else {
                        list.push(DiscoveredPeer {
                            name,
                            addr: from_ip,
                            port,
                            last_seen: Instant::now(),
                        });
                        drop(list);
                        on_change();
                    }
                }
                Err(_) => {} // read timeout or transient error
            }
        }
    });
}

/// Spawn a beacon sender thread. Sends every 2s until `stop` is set.
pub fn start_beacon(name: String, port: u16, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else { return };
        let _ = sock.set_multicast_ttl_v4(1); // LAN only, don't cross routers
        let target = SocketAddr::from((MULTICAST_ADDR, DISCOVERY_PORT));
        let beacon = format!("USSCREENS\t{port}\t{name}");
        let pkt = beacon.as_bytes();
        while !stop.load(Ordering::Relaxed) {
            let _ = sock.send_to(pkt, target);
            std::thread::sleep(BEACON_INTERVAL);
        }
    });
}

/// The DNS-SD service type the phone apps browse for (Android NSD passes it
/// without the trailing `.local.`; iOS declares it under `NSBonjourServices`).
pub const MDNS_SERVICE_TYPE: &str = "_usscreens._tcp.local.";

/// A running DNS-SD/mDNS advertisement for this host. Keep it alive while the
/// host is serving; call [`MdnsAd::shutdown`] (or drop it) to withdraw the
/// service and stop the responder.
pub struct MdnsAd {
    daemon: mdns_sd::ServiceDaemon,
    fullname: String,
}

impl MdnsAd {
    /// Unregister the service and stop the mDNS responder. Best-effort: a host
    /// that vanishes without this is aged out by browsers via the record TTL.
    pub fn shutdown(self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

/// Advertise this host over DNS-SD (`_usscreens._tcp`) so phones can browse for
/// it. `instance_name` is the human-visible host name; `port` is the TCP serving
/// port. Addresses are auto-detected (and follow interface changes). Runs its
/// own responder thread; returns a handle that withdraws the service on shutdown.
///
/// # Errors
/// Returns an error if the responder can't start (e.g. sockets unavailable) or
/// the service registration is rejected.
pub fn advertise_mdns(instance_name: &str, port: u16) -> Result<MdnsAd, mdns_sd::Error> {
    let daemon = mdns_sd::ServiceDaemon::new()?;
    // DNS-SD instance names allow spaces/UTF-8, but keep the hostname label safe.
    let host_label: String = instance_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    let host_name = format!("{}.local.", if host_label.is_empty() { "usscreens-host" } else { host_label.as_str() });
    let info = mdns_sd::ServiceInfo::new(
        MDNS_SERVICE_TYPE,
        instance_name,
        &host_name,
        (), // no fixed IPs — enable_addr_auto picks up every LAN interface
        port,
        None,
    )?
    .enable_addr_auto();
    let fullname = info.get_fullname().to_owned();
    daemon.register(info)?;
    Ok(MdnsAd { daemon, fullname })
}

fn parse_beacon(data: &[u8]) -> Option<(u16, String)> {
    let s = std::str::from_utf8(data).ok()?;
    let mut parts = s.splitn(3, '\t');
    if parts.next()? != "USSCREENS" {
        return None;
    }
    let port: u16 = parts.next()?.parse().ok()?;
    let name = parts.next()?.to_owned();
    Some((port, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beacon_round_trips() {
        let beacon = format!("USSCREENS\t9000\tMY-PC");
        let (port, name) = parse_beacon(beacon.as_bytes()).unwrap();
        assert_eq!(port, 9000);
        assert_eq!(name, "MY-PC");
    }

    #[test]
    fn beacon_name_may_contain_colons() {
        let beacon = "USSCREENS\t9001\tDESKTOP:1";
        let (port, name) = parse_beacon(beacon.as_bytes()).unwrap();
        assert_eq!(port, 9001);
        assert_eq!(name, "DESKTOP:1");
    }

    #[test]
    fn garbage_returns_none() {
        assert!(parse_beacon(b"garbage").is_none());
        assert!(parse_beacon(b"USSCREENS\tnot_a_port\tname").is_none());
        assert!(parse_beacon(b"").is_none());
    }

    /// End-to-end DNS-SD: advertise, then browse with a second daemon and check
    /// the service resolves with our name + port. Exercises real UDP 5353
    /// multicast on this machine, so a locked-down network stack could make it
    /// flaky — if that ever bites, gate it behind `#[ignore]` rather than
    /// weakening it.
    #[test]
    fn mdns_advertisement_is_browsable() {
        let ad = advertise_mdns("USScreens-Test-Host", 9099).expect("advertise");
        let browser = mdns_sd::ServiceDaemon::new().expect("browser daemon");
        let rx = browser.browse(MDNS_SERVICE_TYPE).expect("browse");

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut found = false;
        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                    if info.get_fullname().starts_with("USScreens-Test-Host.") {
                        assert_eq!(info.get_port(), 9099);
                        found = true;
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
        let _ = browser.shutdown();
        ad.shutdown();
        assert!(found, "advertised service was not resolved within 10s");
    }
}
