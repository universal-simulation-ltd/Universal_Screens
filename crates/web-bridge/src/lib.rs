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

use tungstenite::Message;

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
}
