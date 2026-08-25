//! Regression test for the rustls swap (was `native-tls`): prove a `wss://`
//! dial gets far enough to build a TLS client.
//!
//! The failure this guards is a **panic, not an error**. tungstenite pulls
//! rustls with `default-features = false`, so it compiles in no crypto
//! provider of its own; if this crate stops supplying one, the
//! `ClientConfig::builder()` inside tungstenite's TLS path aborts the process
//! with "no process-level CryptoProvider available". Nothing catches it, and
//! nothing fails until the first real `wss://` dial — which is the cloud
//! rendezvous, on a user's machine.
//!
//! So: stand up a plain TCP listener and dial it as `wss://`. The connection
//! reaches the TLS wrap (the panic site), sends a ClientHello, and the peer —
//! which speaks no TLS — hangs up. Reaching an `Err` is the pass condition; a
//! *panic* is the regression. No network, no certificates, no real server.

use std::io::Read;
use std::net::TcpListener;
use std::thread;

use extender_web_bridge::dial_room;

#[test]
fn dialing_a_wss_room_builds_a_tls_client_instead_of_panicking() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    // Accept, read whatever arrives (the ClientHello), then drop — no TLS.
    let peer = thread::spawn(move || {
        if let Ok((mut sock, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf);
        }
    });

    // `localhost` (not `127.0.0.1`) because rustls needs a DNS name for SNI.
    // The host address is never dialed: `dial_room` fails at the room first.
    let err = dial_room(&format!("wss://localhost:{port}"), "TESTCODE", "127.0.0.1:1")
        .expect_err("a peer that speaks no TLS cannot complete the handshake");

    // Assert it failed at the *handshake*, not before reaching the TLS layer.
    // "unexpected end of file" is rustls reading EOF after its ClientHello — so
    // a client was built and did speak. Without this a regression that failed
    // earlier (refused connection, unresolvable name, a cert store that won't
    // load) would still produce an `Err` and pass. Same string on all three
    // hosts now that none of them uses a platform TLS stack.
    let msg = err.to_string();
    assert!(msg.contains("room connect failed"), "unexpected failure stage: {msg}");
    assert!(msg.contains("unexpected end of file"), "never reached the handshake: {msg}");

    peer.join().unwrap();
}
