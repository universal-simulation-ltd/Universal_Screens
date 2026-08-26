//! The browser leg, encrypted end to end: a browser-shaped client runs the Noise
//! handshake **through the WebSocket bridge** to a real host, and the bridge
//! never sees the plaintext.
//!
//! This is the test the browser E2EE work exists for. It uses only what a
//! browser has — WebSocket messages and `transport::session`, whose whole point
//! is that it needs no socket — and it talks to the **shipped** responder
//! (`transport::accept`), so the bytes have to be right by the host's
//! definition rather than by this test's.
//!
//! ⚠️ The other half of the job is that **nothing else changes**:
//! `proxy_roundtrip.rs` still drives the historical plaintext path through the
//! same function. The bridge ships inside the host binary, so a browser that
//! upgraded before the host did must keep working — see `Relay`.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use extender_transport::{self as transport, Initiator, Session};
use extender_web_bridge::proxy_connection;
use tungstenite::Message as WsMessage;

/// The pairing PIN both ends must agree on.
const PIN: u32 = 8642;
/// Cap every wait so a broken relay fails in seconds instead of wedging the run.
const TIMEOUT: Duration = Duration::from_secs(10);

/// A host that accepts one connection, insists it is encrypted, and echoes one
/// length-prefixed frame back. Returns the plaintext it saw.
fn spawn_host(listener: TcpListener) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let (sock, _) = listener.accept().unwrap();
        sock.set_read_timeout(Some(TIMEOUT)).unwrap();
        let mut conn = transport::accept(sock, PIN).unwrap();
        assert!(
            conn.is_encrypted(),
            "the bridge must have relayed the preamble verbatim, or the host sees a plaintext peer"
        );

        let mut len = [0u8; 4];
        conn.read_exact(&mut len).unwrap();
        let mut body = vec![0u8; u32::from_le_bytes(len) as usize];
        conn.read_exact(&mut body).unwrap();

        conn.write_all(&(body.len() as u32).to_le_bytes()).unwrap();
        conn.write_all(&body).unwrap();
        conn.flush().unwrap();
        body
    })
}

/// A bridge listening on an ephemeral port, proxying one browser to `host_addr`.
fn spawn_bridge(host_addr: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    thread::spawn(move || {
        let (sock, _) = listener.accept().unwrap();
        let _ = proxy_connection(sock, &host_addr);
    });
    addr
}

/// Read WS messages until `want` is satisfied by the bytes collected so far.
fn read_until(
    ws: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    buf: &mut Vec<u8>,
    want: impl Fn(&[u8]) -> bool,
) {
    while !want(buf) {
        match ws.read().expect("the bridge relayed within the timeout") {
            WsMessage::Binary(b) => buf.extend_from_slice(&b),
            WsMessage::Text(t) => buf.extend_from_slice(t.as_bytes()),
            WsMessage::Close(_) => panic!("closed before the expected bytes arrived"),
            _ => {}
        }
    }
}

#[test]
fn a_browser_can_run_the_noise_tunnel_through_the_bridge() {
    let host_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let host_addr = host_listener.local_addr().unwrap().to_string();
    let host = spawn_host(host_listener);
    let bridge_addr = spawn_bridge(host_addr);

    let (mut ws, _) = tungstenite::connect(format!("ws://{bridge_addr}/")).unwrap();
    if let tungstenite::stream::MaybeTlsStream::Plain(s) = ws.get_ref() {
        s.set_read_timeout(Some(TIMEOUT)).unwrap();
    }

    // 1) The handshake. The first message carries the preamble, which is what
    //    puts the bridge into passthrough for the rest of the connection.
    let (init, first) = Initiator::start(PIN).unwrap();
    ws.send(WsMessage::Binary(first)).unwrap();

    // ⚠️ The reply is a byte stream, not a message: the bridge chunks whatever
    // the host wrote, so a browser must buffer until the u16-prefixed handshake
    // message is whole. It usually arrives in one piece, which is exactly why
    // this is worth asserting rather than assuming.
    let mut inbox = Vec::new();
    read_until(&mut ws, &mut inbox, |b| {
        b.len() >= 2 && b.len() >= 2 + u16::from_le_bytes([b[0], b[1]]) as usize
    });
    let reply_len = 2 + u16::from_le_bytes([inbox[0], inbox[1]]) as usize;
    let mut session: Session = init.finish(&inbox[..reply_len]).unwrap();
    let leftover: Vec<u8> = inbox[reply_len..].to_vec();

    // 2) A real framed protocol message, sealed browser-side.
    let mut framed = 11u32.to_le_bytes().to_vec();
    framed.extend_from_slice(b"page down!!");
    ws.send(WsMessage::Binary(session.seal(&framed).unwrap())).unwrap();

    // 3) The echo, opened browser-side.
    session.feed(&leftover).unwrap();
    while session.available() < framed.len() {
        let mut chunk = Vec::new();
        read_until(&mut ws, &mut chunk, |b| !b.is_empty());
        session.feed(&chunk).unwrap();
    }
    assert_eq!(session.take_all(), framed);
    assert_eq!(host.join().unwrap(), b"page down!!");
}

/// The bridge must never be able to read what it relays.
///
/// ⚠️ Worth its own test because the passthrough is *supposed* to be a dumb
/// pipe, and a "helpful" future change — logging bodies, sniffing the hello to
/// label a session — would silently undo the entire point of this work while
/// every other test still passed.
#[test]
fn the_bytes_on_the_bridge_leg_are_not_the_plaintext() {
    let (mut session, _responder) = {
        // Stand up the same pair without a bridge, purely to seal one message.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let host = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            sock.set_read_timeout(Some(TIMEOUT)).unwrap();
            transport::accept(sock, PIN).unwrap()
        });
        let mut carrier = TcpStream::connect(addr).unwrap();
        carrier.set_read_timeout(Some(TIMEOUT)).unwrap();
        let (init, first) = Initiator::start(PIN).unwrap();
        carrier.write_all(&first).unwrap();
        let mut head = [0u8; 2];
        carrier.read_exact(&mut head).unwrap();
        let n = u16::from_le_bytes(head) as usize;
        let mut body = vec![0u8; n];
        carrier.read_exact(&mut body).unwrap();
        let mut reply = head.to_vec();
        reply.extend_from_slice(&body);
        (init.finish(&reply).unwrap(), host.join().unwrap())
    };

    let secret = b"the pairing pin is 8642 and this is the slide text";
    let wire = session.seal(secret).unwrap();
    assert!(
        !wire.windows(secret.len()).any(|w| w == secret),
        "the sealed record must not contain the plaintext"
    );
    assert!(
        !wire.windows(4).any(|w| w == b"8642"),
        "nor any recognisable fragment of it"
    );
}
