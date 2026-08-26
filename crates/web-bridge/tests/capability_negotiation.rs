//! How a browser tab finds out whether the thing it just connected to can carry
//! an **encrypted** session — before it sends a byte it can't take back.
//!
//! ⚠️ This exists because the bridge ships **inside the host binary**. The web
//! client updates the instant it deploys; the host on someone's desk may be a
//! year old. A tab that simply started encrypting would have its handshake
//! re-framed into a garbage `ClientHello`, and the session would die with
//! nothing to say about why.
//!
//! There are two answers because there are two paths, and the difference is not
//! stylistic:
//!
//! - **LAN** — the tab talks to the bridge directly, so a WebSocket subprotocol
//!   settles it in the handshake. Deterministic: no timeout, no reconnect.
//! - **Room** — the tab's WebSocket terminates at *Cloudflare*, not at the host,
//!   so no header it sends can reach the host. The host announces instead, with
//!   a text signal the room relays verbatim.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use extender_web_bridge::{dial_room, E2EE_SUBPROTOCOL};
use tungstenite::client::IntoClientRequest;
use tungstenite::Message;

const TIMEOUT: Duration = Duration::from_secs(10);

/// Open a bridge on an ephemeral port and return its address.
fn spawn_bridge_serving(host_addr: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    thread::spawn(move || {
        let (sock, _) = listener.accept().unwrap();
        let _ = extender_web_bridge::proxy_connection(sock, &host_addr);
    });
    addr
}

/// A TCP listener that accepts and then does nothing — enough for a handshake
/// test, which never gets as far as talking to a host.
fn spawn_idle_host() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    thread::spawn(move || {
        while let Ok((sock, _)) = listener.accept() {
            std::mem::forget(sock); // hold it open; nothing is exchanged
        }
    });
    addr
}

#[test]
fn a_bridge_that_relays_verbatim_echoes_the_e2ee_subprotocol() {
    let bridge = spawn_bridge_serving(spawn_idle_host());

    let mut req = format!("ws://{bridge}/").into_client_request().unwrap();
    req.headers_mut()
        .insert("Sec-WebSocket-Protocol", E2EE_SUBPROTOCOL.parse().unwrap());
    let (_ws, resp) = tungstenite::connect(req).unwrap();

    let echoed = resp
        .headers()
        .get("Sec-WebSocket-Protocol")
        .and_then(|v| v.to_str().ok());
    assert_eq!(
        echoed,
        Some(E2EE_SUBPROTOCOL),
        "the tab reads this to decide whether to encrypt"
    );
}

/// A tab that offers several subprotocols must still get a clean answer — the
/// header is a comma-separated preference list, not a single value.
///
/// ⚠️ Driven as a **raw handshake** rather than through `tungstenite::connect`,
/// because tungstenite's *client* compares the echoed token against the request
/// header as one whole string and so rejects a correct answer to a comma-joined
/// offer. A browser splits the list properly, which is what this asserts. The
/// limitation is in the test client, not in the bridge.
#[test]
fn the_subprotocol_is_matched_as_a_token_not_as_the_whole_header() {
    let bridge = spawn_bridge_serving(spawn_idle_host());
    let response = raw_handshake(&bridge, Some(&format!("something-else, {E2EE_SUBPROTOCOL}")));
    assert!(
        response.to_ascii_lowercase().contains(&format!(
            "sec-websocket-protocol: {}",
            E2EE_SUBPROTOCOL.to_ascii_lowercase()
        )),
        "the offered token must be picked out of the list; got:\n{response}"
    );
}

/// Perform the HTTP half of a WebSocket upgrade by hand and return the raw
/// response head, so a test can read headers a WebSocket client would validate
/// away.
fn raw_handshake(addr: &str, subprotocols: Option<&str>) -> String {
    let mut sock = std::net::TcpStream::connect(addr).unwrap();
    sock.set_read_timeout(Some(TIMEOUT)).unwrap();
    let mut req = String::new();
    req.push_str("GET / HTTP/1.1\r\n");
    req.push_str(&format!("Host: {addr}\r\n"));
    req.push_str("Connection: Upgrade\r\n");
    req.push_str("Upgrade: websocket\r\n");
    req.push_str("Sec-WebSocket-Version: 13\r\n");
    req.push_str("Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n");
    if let Some(list) = subprotocols {
        req.push_str(&format!("Sec-WebSocket-Protocol: {list}\r\n"));
    }
    req.push_str("\r\n");
    sock.write_all(req.as_bytes()).unwrap();

    // Read until the end of the response head; the body/frames are irrelevant.
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if sock.read(&mut byte).unwrap_or(0) == 0 {
            break;
        }
        head.push(byte[0]);
    }
    String::from_utf8_lossy(&head).into_owned()
}

/// ⚠️ The negative case is the one that matters for compatibility: a tab that
/// does **not** ask must not be told it can encrypt, or every old client would
/// start seeing a header it doesn't understand.
#[test]
fn a_tab_that_does_not_ask_is_not_offered_encryption() {
    let bridge = spawn_bridge_serving(spawn_idle_host());
    let (_ws, resp) = tungstenite::connect(format!("ws://{bridge}/")).unwrap();
    assert!(
        resp.headers().get("Sec-WebSocket-Protocol").is_none(),
        "an unasked-for subprotocol in the response is a protocol error for the browser"
    );
}

/// The room path: the host announces, because the tab's own handshake never
/// reaches it. An older host sends nothing here and the tab stays plaintext.
#[test]
fn a_host_announces_its_capability_into_the_room_before_anything_else() {
    // A fake room that pairs, then records what the host says.
    let room = TcpListener::bind("127.0.0.1:0").unwrap();
    let room_port = room.local_addr().unwrap().port();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    thread::spawn(move || {
        let (sock, _) = room.accept().unwrap();
        let mut ws = tungstenite::accept(sock).unwrap();
        ws.send(Message::Text(r#"{"type":"paired","peerRole":"receiver"}"#.into())).unwrap();
        // The very next thing from the host must be its capability signal.
        loop {
            match ws.read().unwrap() {
                Message::Text(t) => {
                    tx.send(t.to_string()).unwrap();
                    return;
                }
                Message::Binary(_) => {
                    tx.send("<binary before caps>".to_owned()).unwrap();
                    return;
                }
                Message::Close(_) => return,
                _ => {}
            }
        }
    });

    // A host socket that just accepts, so `dial_room` gets past its connect.
    let host_addr = spawn_idle_host();
    thread::spawn(move || {
        let _ = dial_room(&format!("ws://127.0.0.1:{room_port}"), "TEST", &host_addr);
    });

    let announced = rx.recv_timeout(TIMEOUT).expect("the host announced nothing");
    assert!(
        announced.contains("\"type\":\"caps\"") && announced.contains("\"e2ee\":true"),
        "expected a caps signal, got: {announced}"
    );
}

/// And the announcement must be *ignorable*: it is JSON with an unknown `type`,
/// which every existing peer — browser and host alike — already skips.
#[test]
fn the_caps_signal_is_shaped_like_a_signal_an_old_peer_ignores() {
    let caps = extender_web_bridge::CAPS_SIGNAL;
    assert!(caps.starts_with('{') && caps.ends_with('}'), "must be a JSON object: {caps}");
    assert!(caps.contains("\"type\""), "old peers switch on `type` and default to ignoring");
    // Not a type any existing peer acts on.
    for known in ["waiting", "paired", "peer-left"] {
        assert!(!caps.contains(known), "must not collide with the signal `{known}`");
    }
}

/// Keep the two mechanisms honest about their own shape: the subprotocol token
/// is versioned, so a future incompatible tunnel can be a different token rather
/// than a silent behaviour change.
#[test]
fn the_subprotocol_token_carries_a_version() {
    assert!(
        E2EE_SUBPROTOCOL.contains(".v"),
        "an unversioned token leaves no way to change the tunnel later: {E2EE_SUBPROTOCOL}"
    );
    assert!(E2EE_SUBPROTOCOL.is_ascii(), "a header value must be ASCII");
}
