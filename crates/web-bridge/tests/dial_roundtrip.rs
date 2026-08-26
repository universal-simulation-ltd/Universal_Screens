//! M8d: prove `dial_room` bridges a paired rendezvous room to a local host in
//! both directions. We stand up a fake room (a plain `ws://` server that sends
//! `{"type":"paired"}` then relays) and a fake host (a TCP server speaking the
//! length-prefixed framing), run `dial_room` between them, and assert a host
//! frame reaches the room and a room frame reaches the host.
//!
//! ⚠️ **The browser speaks first, and this test now says so.** It used to have
//! the fake host push its frame before the room sent anything — an order no real
//! session takes, because a host answers a `ClientHello` and never opens the
//! conversation. That went unnoticed until `dial_room` had to read the browser's
//! first message to choose a relay mode (plaintext re-framing vs encrypted
//! passthrough), at which point the artificial order deadlocked. Nothing is lost
//! by a host that writes early — TCP buffers it — but the test should exercise
//! the real sequence.

use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use extender_web_bridge::{dial_room, read_frame_body, write_frame_body};
use tungstenite::Message;

const DOWN: &[u8] = b"host->browser frame (e.g. a video Message)";
const UP: &[u8] = b"browser->host frame (e.g. an Input)";

#[test]
fn dial_room_bridges_a_paired_room_to_the_host_both_ways() {
    // --- fake host: accept once, send DOWN, expect UP ---
    let host_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let host_addr = host_listener.local_addr().unwrap().to_string();
    let (host_got_tx, host_got_rx) = mpsc::channel::<Vec<u8>>();
    let host_thread = thread::spawn(move || {
        let (sock, _) = host_listener.accept().unwrap();
        let mut writer = sock.try_clone().unwrap();
        sock.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut reader = std::io::BufReader::new(sock);
        // Read the browser's frame first (a real host waits for the hello), then
        // answer — the order a live session actually uses.
        let up = read_frame_body(&mut reader).unwrap(); // room -> (bridge) -> host
        host_got_tx.send(up).unwrap();
        write_frame_body(&mut writer, DOWN).unwrap(); // host -> (bridge) -> room
    });

    // --- fake room: accept the bridge's ws, say "paired", relay one each way ---
    let room_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let room_port = room_listener.local_addr().unwrap().port();
    let (room_got_tx, room_got_rx) = mpsc::channel::<Vec<u8>>();
    let room_thread = thread::spawn(move || {
        let (sock, _) = room_listener.accept().unwrap();
        let mut ws = tungstenite::accept(sock).unwrap();
        ws.send(Message::Text(r#"{"type":"paired","peerRole":"receiver"}"#.into())).unwrap();
        // The browser's frame goes up first; it is also what tells the bridge
        // which relay mode this connection is (this one is plaintext).
        ws.send(Message::Binary(UP.to_vec())).unwrap();
        // Then the host's DOWN frame comes back, relayed by the bridge.
        loop {
            match ws.read().unwrap() {
                Message::Binary(b) => {
                    room_got_tx.send(b).unwrap();
                    break;
                }
                Message::Close(_) => return,
                _ => {}
            }
        }
        // Keep the socket open briefly, then close.
        thread::sleep(Duration::from_millis(300));
        let _ = ws.close(None);
        let _ = ws.flush();
    });

    // --- run the bridge: dial the fake room, bridge to the fake host ---
    let bridge = thread::spawn(move || {
        let _ = dial_room(&format!("ws://127.0.0.1:{room_port}"), "TEST", &host_addr);
    });

    let room_got = room_got_rx.recv_timeout(Duration::from_secs(5)).expect("room never got a frame");
    assert_eq!(room_got, DOWN, "host->room frame was not relayed verbatim");
    let host_got = host_got_rx.recv_timeout(Duration::from_secs(5)).expect("host never got a frame");
    assert_eq!(host_got, UP, "room->host frame was not relayed verbatim");

    let _ = host_thread.join();
    let _ = room_thread.join();
    let _ = bridge.join();
}
