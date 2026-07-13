//! Thin egui adapter over the shared, platform-agnostic `extender-discovery`
//! crate. The UDP multicast beacon/listener + wire format live there and are
//! shared with the Windows host; here we only bridge the listener's `on_change`
//! callback to `egui::Context::request_repaint` so the peer list stays live in
//! the GUI. Callers use `crate::discovery::{DiscoveredPeer, start_listener,
//! start_beacon}` exactly as before.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use eframe::egui;

pub use extender_discovery::{advertise_mdns, start_beacon, DiscoveredPeer, MdnsAd};

/// Start the shared LAN listener, repainting the egui frame whenever the peer
/// set changes. Same signature as before the extraction into `extender-discovery`.
pub fn start_listener(
    peers: Arc<Mutex<Vec<DiscoveredPeer>>>,
    stop: Arc<AtomicBool>,
    ctx: egui::Context,
    own_ip: Arc<Mutex<Option<String>>>,
) {
    extender_discovery::start_listener(peers, stop, own_ip, move || ctx.request_repaint());
}
