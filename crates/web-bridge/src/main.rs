//! WebSocket front-end for a running `extender-host`. Two modes:
//!
//! LAN listener (M7a) — browsers on the LAN connect to this bridge:
//!   cargo run -p extender-web-bridge [-- WS_ADDR] [HOST_ADDR]
//!     WS_ADDR    where browsers connect      (default 0.0.0.0:9002)
//!     HOST_ADDR  the extender-host to proxy  (default 127.0.0.1:9000)
//!
//! Dial-the-room (M8d, "cast to a browser") — the host dials the cloud
//! rendezvous so a browser tab anywhere can view/drive it, no inbound port:
//!   cargo run -p extender-web-bridge -- --room CODE [--url BASE] [--host HOST_ADDR]
//!     CODE       the receiver tab's pairing code (from …/screens/receive)
//!     BASE       rendezvous base URL          (default wss://opensource.unisim.co.uk)
//!     HOST_ADDR  the extender-host to bridge  (default 127.0.0.1:9000)
//!
//! Start the host first (`extender-host` / `extender-host-windows`), then this.
//! See `docs/M7-browser-client.md` and `docs/M8-browser-receiver.md`.

use extender_web_bridge::{dial_room, serve, DEFAULT_HOST_ADDR, DEFAULT_ROOM_URL, DEFAULT_WS_ADDR};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if let Some(code) = flag(&args, "--room") {
        let url = flag(&args, "--url").unwrap_or_else(|| DEFAULT_ROOM_URL.to_string());
        let host = flag(&args, "--host").unwrap_or_else(|| DEFAULT_HOST_ADDR.to_string());
        return dial_room(&url, &code, &host);
    }

    // LAN listener mode: positional [WS_ADDR] [HOST_ADDR].
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let ws_addr = positional.first().map_or_else(|| DEFAULT_WS_ADDR.to_string(), |s| s.to_string());
    let host_addr = positional.get(1).map_or_else(|| DEFAULT_HOST_ADDR.to_string(), |s| s.to_string());
    serve(&ws_addr, &host_addr)
}

/// Value following `name` in `--name VALUE` (or `--name=VALUE`), if present.
fn flag(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().cloned();
        }
        if let Some(v) = a.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}
