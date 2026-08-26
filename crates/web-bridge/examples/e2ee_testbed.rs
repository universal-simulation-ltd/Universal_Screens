//! A real bridge in front of a real host, for driving the **browser** side from
//! outside Rust.
//!
//! `apps/web/secure.test.mjs` spawns this, reads the `WS_PORT=` line, and then
//! behaves exactly as the tab does: offer the E2EE subprotocol, run the Noise
//! handshake through the WASM shim, and exchange framed protocol messages. That
//! is the only way to prove the JavaScript, the WASM bindings, the bridge and
//! the host agree — the Rust tests prove every pair of those but never the whole
//! chain with the real browser code in it.
//!
//! The "host" here is the shipped `transport::accept` plus an echo loop, not a
//! desktop host: this testbed must not inject anything into whoever runs it.
//!
//! Run: `cargo run -p extender-web-bridge --example e2ee_testbed -- <pin>`

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

fn main() {
    let pin: u32 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(0);

    // The "host": accept one connection through the transport (encrypted or
    // not, its own choice), then echo every length-prefixed frame back.
    let host = TcpListener::bind("127.0.0.1:0").expect("bind host");
    let host_addr = host.local_addr().expect("host addr").to_string();
    thread::spawn(move || {
        for sock in host.incoming().flatten() {
            thread::spawn(move || {
                let Ok(mut conn) = extender_transport::accept(sock, pin) else { return };
                eprintln!("testbed host: encrypted={}", conn.is_encrypted());
                loop {
                    let mut len = [0u8; 4];
                    if conn.read_exact(&mut len).is_err() {
                        return;
                    }
                    let mut body = vec![0u8; u32::from_le_bytes(len) as usize];
                    if conn.read_exact(&mut body).is_err() {
                        return;
                    }
                    if conn.write_all(&len).is_err() || conn.write_all(&body).is_err() {
                        return;
                    }
                    let _ = conn.flush();
                }
            });
        }
    });

    // The bridge, in front of it.
    let ws = TcpListener::bind("127.0.0.1:0").expect("bind ws");
    let ws_port = ws.local_addr().expect("ws addr").port();
    // The line the test harness waits for. Flushed immediately: a buffered
    // handshake line is a test that hangs for no reason.
    println!("WS_PORT={ws_port}");
    let _ = std::io::stdout().flush();

    for sock in ws.incoming().flatten() {
        let target = host_addr.clone();
        thread::spawn(move || {
            if let Err(e) = extender_web_bridge::proxy_connection(sock, &target) {
                eprintln!("testbed bridge: {e}");
            }
        });
    }
}
