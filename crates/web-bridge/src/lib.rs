//! M7a transport spike: a WebSocket front-end for the native TCP host protocol.
//!
//! A browser tab can't open a raw TCP socket, but the whole `extender` stack
//! speaks plaintext TCP on `:9000` with length-prefixed `postcard` frames
//! (`crates/protocol`). This bridge accepts a **WebSocket** connection from a
//! browser and proxies it to a running `extender-host`, translating between the
//! two framings:
//!
//! - **Upstream (browser → host):** each WS *binary* message is exactly one
//!   `postcard` body (a [`ClientHello`] or an `Input`). The bridge prepends the
//!   4-byte little-endian length prefix the TCP host expects and writes it on.
//! - **Downstream (host → browser):** the bridge reads each length-prefixed frame
//!   from the host (a `Message`) and ships the body as one WS binary message.
//!
//! So the WS payloads are the same `postcard` bytes the native client sends —
//! the browser deals only in whole messages (WS already delimits them), and the
//! host's `serve()` / capture / encode / inject code is reused untouched.
//!
//! This is a standalone proxy for the spike (zero changes to the host); M7f folds
//! the listener into the host process itself. See `docs/M7-browser-client.md`.

use std::io::{self, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::Duration;

use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// Default base URL of the cloud rendezvous (the opensource-portal Worker).
pub const DEFAULT_ROOM_URL: &str = "wss://opensource.unisim.co.uk";

/// How long the per-connection loop parks when neither side has data ready, so a
/// nonblocking WS read that returns `WouldBlock` doesn't spin the CPU. Bounds the
/// added latency per direction; fine for a LAN spike, revisited for production.
const IDLE_POLL: Duration = Duration::from_millis(4);

/// Default WebSocket bind address the bridge listens on for browsers.
pub const DEFAULT_WS_ADDR: &str = "0.0.0.0:9002";
/// Default TCP address of the `extender-host` the bridge proxies to.
pub const DEFAULT_HOST_ADDR: &str = "127.0.0.1:9000";

/// Read one length-prefixed frame body (4-byte LE length + body) from a TCP
/// stream — the wire framing of `protocol::read_framed`, but without decoding
/// the `postcard` body (the bridge only forwards bytes). Must stay in step with
/// `extender_protocol::{read_framed, write_framed}`.
///
/// # Errors
/// Returns an error if the stream ends or the declared length can't be read.
pub fn read_frame_body<R: Read>(r: &mut R) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    Ok(body)
}

/// Write one length-prefixed frame body (4-byte LE length + body) to a TCP
/// stream — the inverse of [`read_frame_body`], matching `protocol::write_framed`.
///
/// # Errors
/// Returns an error if `body` is larger than `u32::MAX` or the write fails.
pub fn write_frame_body<W: Write>(w: &mut W, body: &[u8]) -> io::Result<()> {
    let len = u32::try_from(body.len()).map_err(|_| io::Error::other("message too large"))?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(body)?;
    Ok(())
}

/// Accept WebSocket connections forever and proxy each to a fresh TCP connection
/// to `host_addr`. One client at a time mirrors the host's own sequential accept
/// loop; that's all the spike needs.
///
/// # Errors
/// Returns an error only if binding the listener fails. Per-connection errors are
/// logged and the loop continues.
pub fn serve(ws_addr: &str, host_addr: &str) -> io::Result<()> {
    let listener = TcpListener::bind(ws_addr)?;
    println!("extender-web-bridge: WebSocket on ws://{ws_addr}  ->  host {host_addr}");
    println!("waiting for a browser to connect...");
    for incoming in listener.incoming() {
        let stream = match incoming {
            Ok(s) => s,
            Err(e) => {
                eprintln!("accept failed: {e}");
                continue;
            }
        };
        let peer = stream.peer_addr().map_or_else(|_| "?".into(), |a| a.to_string());
        println!("browser connected: {peer}");
        match proxy_connection(stream, host_addr) {
            Ok(()) => println!("browser {peer} disconnected"),
            Err(e) => eprintln!("session with {peer} ended: {e}"),
        }
        println!("waiting for a browser to connect...");
    }
    Ok(())
}

/// Proxy a single browser: complete the WS handshake on `ws_stream`, open a TCP
/// connection to `host_addr`, then pump messages both ways until either side
/// closes.
///
/// The WS handshake runs blocking (so it completes), then the socket is switched
/// to nonblocking for the data phase: a downstream reader thread drains the host
/// into a channel, and this thread interleaves "flush pending downstream to the
/// browser" with a nonblocking "read one upstream WS message → host".
///
/// # Errors
/// Returns an error if the handshake, the host connection, or a forward fails.
pub fn proxy_connection(ws_stream: TcpStream, host_addr: &str) -> io::Result<()> {
    let mut ws = tungstenite::accept(ws_stream)
        .map_err(|e| io::Error::other(format!("websocket handshake failed: {e}")))?;
    // Data phase is nonblocking so a quiet upstream doesn't stall downstream
    // delivery; `WouldBlock` from `ws.read()` is the documented resume signal.
    ws.get_ref().set_nonblocking(true)?;

    let host = TcpStream::connect(host_addr)?;
    host.set_nodelay(true)?;
    let mut host_writer = host.try_clone()?;

    // Downstream (host → browser) reader thread: parse each framed body off the
    // host and hand it to the main loop, which owns the WS for writing.
    let (down_tx, down_rx) = mpsc::channel::<Vec<u8>>();
    let mut host_reader = BufReader::new(host);
    thread::spawn(move || {
        while let Ok(body) = read_frame_body(&mut host_reader) {
            if down_tx.send(body).is_err() {
                break; // main loop gone
            }
        }
        // Channel drops here on host EOF/error; the main loop sees Disconnected.
    });

    loop {
        // 1) Forward everything pending from the host to the browser.
        loop {
            match down_rx.try_recv() {
                Ok(body) => ws
                    .write(Message::Binary(body))
                    .map_err(|e| io::Error::other(format!("ws write: {e}")))?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // Host closed: flush what we have, close the WS, done.
                    let _ = ws.flush();
                    let _ = ws.close(None);
                    return Ok(());
                }
            }
        }
        match ws.flush() {
            Ok(()) => {}
            Err(tungstenite::Error::Io(e)) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(io::Error::other(format!("ws flush: {e}"))),
        }

        // 2) Forward one upstream WS message to the host (nonblocking).
        match ws.read() {
            Ok(Message::Binary(body)) => write_frame_body(&mut host_writer, &body)?,
            // A browser may send the hello as text in early experiments; treat its
            // bytes the same. Ping/Pong are handled internally by tungstenite.
            Ok(Message::Text(t)) => write_frame_body(&mut host_writer, t.as_bytes())?,
            Ok(Message::Close(_)) => {
                let _ = host_writer.shutdown(std::net::Shutdown::Both);
                return Ok(());
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(IDLE_POLL);
            }
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                let _ = host_writer.shutdown(std::net::Shutdown::Both);
                return Ok(());
            }
            Err(e) => return Err(io::Error::other(format!("ws read: {e}"))),
        }
    }
}

/// Extract the `"type"` value from a rendezvous JSON signal frame
/// (`{"type":"paired",…}`), without pulling in a JSON dependency. Returns the
/// value of the first `"type"` key, or `None` if absent/malformed.
fn signal_type(text: &str) -> Option<&str> {
    let after_key = &text[text.find("\"type\"")? + 6..];
    let after_colon = &after_key[after_key.find(':')? + 1..];
    let open = after_colon.find('"')? + 1;
    let close = after_colon[open..].find('"')? + open;
    Some(&after_colon[open..close])
}

/// Set the underlying TCP socket of a (possibly TLS-wrapped) client WebSocket to
/// (non)blocking. The data phase wants nonblocking so a quiet upstream doesn't
/// stall downstream delivery (same reasoning as [`proxy_connection`]).
fn set_room_nonblocking(ws: &WebSocket<MaybeTlsStream<TcpStream>>, nonblocking: bool) -> io::Result<()> {
    match ws.get_ref() {
        MaybeTlsStream::Plain(s) => s.set_nonblocking(nonblocking),
        MaybeTlsStream::NativeTls(s) => s.get_ref().set_nonblocking(nonblocking),
        // MaybeTlsStream is #[non_exhaustive]; other variants aren't produced here.
        _ => Ok(()),
    }
}

/// Dial the cloud rendezvous as the *sender* (the host side of "cast to a
/// browser", M8d) and bridge it to a local `extender-host`. This is
/// [`proxy_connection`] turned inside out: instead of *listening* for a browser
/// on the LAN, the host *dials out* to `base_url`'s `/screens/room?code=…` and,
/// once a receiver tab pairs, pumps the same `postcard` frames to/from the local
/// host on `host_addr`. So a browser tab anywhere — across networks, behind NAT —
/// can view/drive this machine with no inbound port.
///
/// Phase 1 blocks reading the room's JSON signals until `paired`; phase 2 is the
/// nonblocking pump (host → room binary downstream, room → host binary upstream),
/// ignoring further signal frames except `peer-left`.
///
/// # Errors
/// Returns an error if the room connection, the host connection, or a forward fails.
pub fn dial_room(base_url: &str, code: &str, host_addr: &str) -> io::Result<()> {
    let url = format!("{}/screens/room?code={}&role=sender", base_url.trim_end_matches('/'), code);
    let (mut ws, _resp) =
        tungstenite::connect(&url).map_err(|e| io::Error::other(format!("room connect failed: {e}")))?;
    println!("extender-web-bridge: dialed room {code} at {base_url}; waiting to pair...");

    // Phase 1: wait (blocking) for a receiver tab to join. Binary frames only
    // flow once we bridge to the host below.
    loop {
        match ws.read() {
            Ok(Message::Text(t)) => match signal_type(&t) {
                Some("paired") => break,
                Some("peer-left") | None => {} // keep waiting
                Some(_) => {}                  // "waiting" etc.
            },
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => {}
            Err(e) => return Err(io::Error::other(format!("room read: {e}"))),
        }
    }
    println!("extender-web-bridge: paired; bridging to host {host_addr}");

    // Phase 2: bridge the paired room to a fresh loopback connection to the host's
    // serve() — which stays completely untouched (it just sees a localhost peer).
    set_room_nonblocking(&ws, true)?;
    let host = TcpStream::connect(host_addr)?;
    host.set_nodelay(true)?;
    let mut host_writer = host.try_clone()?;

    let (down_tx, down_rx) = mpsc::channel::<Vec<u8>>();
    let mut host_reader = BufReader::new(host);
    thread::spawn(move || {
        while let Ok(body) = read_frame_body(&mut host_reader) {
            if down_tx.send(body).is_err() {
                break;
            }
        }
    });

    loop {
        // 1) Host → room: ship each framed `Message` (video etc.) as one WS binary.
        loop {
            match down_rx.try_recv() {
                Ok(body) => ws
                    .write(Message::Binary(body))
                    .map_err(|e| io::Error::other(format!("room write: {e}")))?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    let _ = ws.flush();
                    let _ = ws.close(None);
                    return Ok(());
                }
            }
        }
        match ws.flush() {
            Ok(()) => {}
            Err(tungstenite::Error::Io(e)) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(io::Error::other(format!("room flush: {e}"))),
        }

        // 2) Room → host: a binary frame is the browser's `Input`; a text frame is
        // a rendezvous signal (only `peer-left` matters here).
        match ws.read() {
            Ok(Message::Binary(body)) => write_frame_body(&mut host_writer, &body)?,
            Ok(Message::Text(t)) => {
                if signal_type(&t) == Some("peer-left") {
                    let _ = host_writer.shutdown(std::net::Shutdown::Both);
                    return Ok(());
                }
            }
            Ok(Message::Close(_)) => {
                let _ = host_writer.shutdown(std::net::Shutdown::Both);
                return Ok(());
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(IDLE_POLL);
            }
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                let _ = host_writer.shutdown(std::net::Shutdown::Both);
                return Ok(());
            }
            Err(e) => return Err(io::Error::other(format!("room read: {e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frame_body_round_trips_through_the_wire_framing() {
        let body = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x10];
        let mut buf = Vec::new();
        write_frame_body(&mut buf, &body).unwrap();
        // 4-byte LE length prefix, then the body.
        assert_eq!(&buf[..4], &(body.len() as u32).to_le_bytes());
        let got = read_frame_body(&mut Cursor::new(buf)).unwrap();
        assert_eq!(got, body);
    }

    #[test]
    fn empty_body_round_trips() {
        let mut buf = Vec::new();
        write_frame_body(&mut buf, &[]).unwrap();
        assert_eq!(buf, vec![0, 0, 0, 0]);
        assert_eq!(read_frame_body(&mut Cursor::new(buf)).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn truncated_frame_is_an_error() {
        // Length says 5 bytes but only 2 follow.
        let buf = vec![5, 0, 0, 0, 0xAA, 0xBB];
        assert!(read_frame_body(&mut Cursor::new(buf)).is_err());
    }

    #[test]
    fn signal_type_reads_the_rendezvous_signal() {
        assert_eq!(signal_type(r#"{"type":"paired","peerRole":"receiver"}"#), Some("paired"));
        assert_eq!(signal_type(r#"{"type":"waiting"}"#), Some("waiting"));
        assert_eq!(signal_type(r#"{ "type" : "peer-left" }"#), Some("peer-left"));
        // A relayed control/postcard payload has no "type" → None (not a signal).
        assert_eq!(signal_type(r#"{"t":"move","dx":1}"#), None);
        assert_eq!(signal_type("not json"), None);
    }
}
