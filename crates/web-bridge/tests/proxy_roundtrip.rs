//! End-to-end proof of the M7a transport: a browser-like WebSocket client sends a
//! real `postcard` [`ClientHello`] through the bridge to a fake TCP host, and a
//! real `Message` flows back the other way — exactly the bytes the native client
//! and host exchange, but over WS. If this passes, a browser can speak the
//! existing protocol unchanged.

use std::io::BufReader;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use extender_protocol::{
    self as protocol, CaptureMode, ClientHello, ClientPlatform, Codec, Message,
};
use extender_web_bridge::proxy_connection;
use tungstenite::Message as WsMessage;

#[test]
fn browser_hello_and_host_message_round_trip_through_the_bridge() {
    // 1) Fake TCP host: read the hello the bridge forwards, then push a Message.
    let host_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let host_addr = host_listener.local_addr().unwrap().to_string();

    let expected_hello = ClientHello {
        protocol_version: protocol::PROTOCOL_VERSION,
        width: 2560,
        height: 1440,
        capture_mode: CaptureMode::MirrorPrimary,
        platform: ClientPlatform::Unknown, // a browser isn't one of the native platforms
        pin: 4321,
    };
    let downstream = Message::StreamStart {
        width: 2560,
        height: 1440,
        codec: Codec::H264,
        parameter_sets: vec![vec![0x67, 0x42, 0x00, 0x1f], vec![0x68, 0xce, 0x3c, 0x80]],
    };

    let host_expected = expected_hello;
    let host_downstream = downstream.clone();
    let host = thread::spawn(move || {
        let (mut sock, _) = host_listener.accept().unwrap();
        let mut reader = BufReader::new(sock.try_clone().unwrap());
        // The hello the browser sent must arrive here byte-identical (re-framed).
        let got: ClientHello = protocol::read_framed(&mut reader).unwrap();
        assert_eq!(got, host_expected, "hello mangled in transit");
        // Send a Message downstream for the browser to read.
        protocol::write_framed(&mut sock, &host_downstream).unwrap();
        // Keep the socket open briefly so the bridge can forward before EOF.
        thread::sleep(Duration::from_millis(200));
    });

    // 2) Bridge: accept one WS connection and proxy it to the fake host.
    let ws_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let ws_addr = ws_listener.local_addr().unwrap();
    thread::spawn(move || {
        let (stream, _) = ws_listener.accept().unwrap();
        let _ = proxy_connection(stream, &host_addr);
    });

    // 3) Browser-like WS client: connect, send the hello as a binary message,
    //    then read the downstream Message back.
    let url = format!("ws://{ws_addr}/");
    let (mut ws, _resp) = tungstenite::connect(&url).expect("ws connect");

    let hello_bytes = postcard::to_stdvec(&expected_hello).unwrap();
    ws.send(WsMessage::Binary(hello_bytes)).unwrap();
    ws.flush().unwrap();

    // First non-control frame is the host's Message body.
    let body = loop {
        match ws.read().unwrap() {
            WsMessage::Binary(b) => break b,
            WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
            other => panic!("unexpected ws message: {other:?}"),
        }
    };
    let got: Message = postcard::from_bytes(&body).expect("decode forwarded Message");
    assert_eq!(got, downstream, "downstream Message mangled in transit");

    host.join().unwrap();
    let _ = ws.close(None);
}
