//! End-to-end proof of the Nearby plumbing for the web client: `serve()`'s
//! `GET /peers` endpoint lists a host advertised over DNS-SD (real mDNS on this
//! machine), the `?host=` retarget proxies to that discovered host, and an
//! address the bridge has NOT discovered is refused. See docs/M9-lan-discovery.md.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use extender_web_bridge::{read_frame_body, serve};
use tungstenite::Message as WsMessage;

/// One plain-HTTP GET against the bridge; returns the response body.
fn http_get(addr: &str, path: &str) -> String {
    let mut sock = TcpStream::connect(addr).unwrap();
    write!(sock, "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n").unwrap();
    let mut response = String::new();
    sock.read_to_string(&mut response).unwrap();
    let (headers, body) = response.split_once("\r\n\r\n").expect("http response has headers");
    assert!(headers.starts_with("HTTP/1.1 200"), "unexpected response: {headers}");
    assert!(
        headers.contains("Access-Control-Allow-Origin: *"),
        "missing CORS header (the page may be served from another origin)"
    );
    body.to_owned()
}

#[test]
fn peers_endpoint_lists_mdns_hosts_and_gates_the_host_retarget() {
    // 1) A fake "discovered host": a TCP listener that reads one framed body and
    //    echoes it back framed — enough to prove the retargeted proxy reached it.
    let host_listener = TcpListener::bind("0.0.0.0:0").unwrap();
    let host_port = host_listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for sock in host_listener.incoming().flatten() {
            let mut sock = sock;
            if let Ok(body) = read_frame_body(&mut sock) {
                let _ = extender_web_bridge::write_frame_body(&mut sock, &body);
            }
            thread::sleep(Duration::from_millis(100));
        }
    });

    // 2) Advertise it over DNS-SD, exactly as a serving GUI host does.
    let ad = extender_discovery::advertise_mdns("Bridge-Peers-Test", host_port).expect("advertise");

    // 3) The bridge under test, on an ephemeral port. `serve` runs forever, so
    //    park it on a thread; the default host target is a dead address to prove
    //    the retarget (not the default) is what connects.
    let bridge_port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let bridge_addr = format!("127.0.0.1:{bridge_port}");
    {
        let bridge_addr = bridge_addr.clone();
        thread::spawn(move || {
            let _ = serve(&bridge_addr, "127.0.0.1:1"); // default target: nothing there
        });
    }

    // 4) /peers eventually lists the advertised host (mDNS resolve is fast on
    //    loopback-capable machines, but give it a lenient window).
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut peer_target = None;
    while Instant::now() < deadline {
        let body = http_get(&bridge_addr, "/peers");
        if let Some(idx) = body.find("\"Bridge-Peers-Test\"") {
            // Pull the addr out of the matching entry (fields are ordered).
            let rest = &body[idx..];
            let addr = rest.split("\"addr\":\"").nth(1).unwrap().split('"').next().unwrap();
            let port: u16 = rest.split("\"port\":").nth(1).unwrap().split(['}', ',']).next().unwrap().parse().unwrap();
            assert_eq!(port, host_port);
            peer_target = Some(format!("{addr}:{port}"));
            break;
        }
        thread::sleep(Duration::from_millis(300));
    }
    let peer_target = peer_target.expect("/peers never listed the advertised host");

    // 5) ?host= with an UNDISCOVERED address is refused: the WS opens (handshake
    //    completes) but closes without proxying.
    let (mut ws, _) = tungstenite::connect(format!("ws://{bridge_addr}/?host=9.9.9.9:9")).expect("ws connect");
    let refused = loop {
        match ws.read() {
            Ok(WsMessage::Close(_)) | Err(_) => break true,
            Ok(WsMessage::Binary(_)) => break false,
            Ok(_) => continue,
        }
    };
    assert!(refused, "bridge proxied to an address it never discovered");

    // 6) ?host= with the DISCOVERED address proxies: our frame comes back echoed
    //    through bridge -> fake host -> bridge.
    let (mut ws, _) = tungstenite::connect(format!("ws://{bridge_addr}/?host={peer_target}")).expect("ws connect");
    let payload = vec![0xAB, 0xCD, 0x01, 0x02];
    ws.send(WsMessage::Binary(payload.clone())).unwrap();
    ws.flush().unwrap();
    let echoed = loop {
        match ws.read().expect("echo read") {
            WsMessage::Binary(b) => break b,
            WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
            other => panic!("unexpected ws message: {other:?}"),
        }
    };
    assert_eq!(echoed, payload, "echo mangled through the retargeted proxy");
    let _ = ws.close(None);

    ad.shutdown();
}
