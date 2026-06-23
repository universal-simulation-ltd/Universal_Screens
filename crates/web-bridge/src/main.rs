//! M7a transport spike CLI: front a running `extender-host` with a WebSocket so a
//! browser tab can connect. See `docs/M7-browser-client.md`.
//!
//! Run:  cargo run -p extender-web-bridge [-- WS_ADDR] [HOST_ADDR]
//!   WS_ADDR    where browsers connect      (default 0.0.0.0:9002)
//!   HOST_ADDR  the extender-host to proxy  (default 127.0.0.1:9000)
//!
//! Start the host first (`extender-host` / `extender-host-windows`), then this,
//! then open the spike page (`apps/web/spike.html`) at the WS address.

use extender_web_bridge::{serve, DEFAULT_HOST_ADDR, DEFAULT_WS_ADDR};

fn main() -> std::io::Result<()> {
    let ws_addr = std::env::args().nth(1).unwrap_or_else(|| DEFAULT_WS_ADDR.to_string());
    let host_addr = std::env::args().nth(2).unwrap_or_else(|| DEFAULT_HOST_ADDR.to_string());
    serve(&ws_addr, &host_addr)
}
