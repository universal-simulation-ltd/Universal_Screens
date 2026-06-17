//! ExtenderScreen core: the platform-agnostic client *session* — the networking
//! half a client needs, with no UI, no GPU, and no video codec.
//!
//! A [`Session`] owns the TCP connection to an `extender-host`: it performs the
//! [`ClientHello`] handshake, reads the downstream [`Message`] stream and surfaces
//! it as [`StreamEvent`]s (geometry + *encoded* frames — decoding is the
//! platform's job, hardware on mobile), and forwards upstream [`Input`] from a
//! channel the caller owns. This is the piece every client shares; the desktop
//! client wraps it with `openh264` + `wgpu`, and a future iOS/Android shell wraps
//! it with the platform decoder and a touch UI (see `docs/M5-mobile-remote-control.md`).

use std::io::{self, BufReader};
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::thread;

pub use extender_protocol::{self as protocol, ClientHello, Codec, Input, Message};

/// An event from the host's downstream stream. Frames are carried *encoded*
/// (AVCC: length-prefixed NAL units) — the caller decodes them, so the codec
/// path stays platform-native (hardware on mobile).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    /// Sent once at stream start: geometry, codec, and the parameter sets
    /// (SPS/PPS for H.264) needed to build a decoder.
    Start {
        width: u32,
        height: u32,
        codec: Codec,
        parameter_sets: Vec<Vec<u8>>,
    },
    /// One encoded frame.
    Frame {
        pts_value: i64,
        pts_timescale: i32,
        keyframe: bool,
        data: Vec<u8>,
    },
    /// A still JPEG snapshot of the host screen (the clicker's slide preview).
    /// `slot` is the slide's offset from the current position: 0 = current,
    /// -1 = previous, +1 = next.
    Snapshot {
        width: u32,
        height: u32,
        slot: i32,
        data: Vec<u8>,
    },
    /// The host's identity (OS tag + machine name), for labelling saved connections.
    HostInfo {
        os: String,
        name: String,
    },
    /// The host's open top-level windows as `(id, title)`, for the focus picker.
    WindowList {
        windows: Vec<(i64, String)>,
    },
}

impl From<Message> for StreamEvent {
    fn from(msg: Message) -> Self {
        match msg {
            Message::StreamStart { width, height, codec, parameter_sets } => {
                StreamEvent::Start { width, height, codec, parameter_sets }
            }
            Message::Frame { pts_value, pts_timescale, keyframe, data } => {
                StreamEvent::Frame { pts_value, pts_timescale, keyframe, data }
            }
            Message::Snapshot { width, height, slot, data } => {
                StreamEvent::Snapshot { width, height, slot, data }
            }
            Message::HostInfo { os, name } => StreamEvent::HostInfo { os, name },
            Message::WindowList { windows } => StreamEvent::WindowList { windows },
        }
    }
}

/// A live client session. Connecting spawns two detached background threads — one
/// reading the downstream stream into the [`events`](Session::events) channel,
/// one draining the caller's input channel onto the socket — so the caller only
/// polls events and pushes input. The reader exits on host disconnect (or when
/// the session is dropped, which shuts the socket down); the writer exits when
/// the caller drops its input `Sender`.
pub struct Session {
    events: Receiver<StreamEvent>,
    /// A spare handle on the socket, used only to force the reader's blocking
    /// read to return when the session is dropped.
    shutdown: TcpStream,
}

impl Session {
    /// Connect to `addr`, send `hello`, and start streaming. `input_rx` is the
    /// receiving end of the caller's input channel; everything sent on its paired
    /// `Sender` is forwarded to the host until the channel closes or the socket
    /// errors. The caller keeps the `Sender` to drive input from its UI.
    ///
    /// # Errors
    /// Returns an error if the connection or the initial handshake write fails.
    pub fn connect(
        addr: &str,
        hello: &ClientHello,
        input_rx: Receiver<Input>,
    ) -> io::Result<Session> {
        let mut stream = TcpStream::connect(addr)?;
        let _ = stream.set_nodelay(true); // disable Nagle — low latency for video + input

        // Handshake first, on the caller's thread, so a failure surfaces as an
        // error from `connect` rather than dying silently in a background thread.
        protocol::write_framed(&mut stream, hello)?;

        // A second handle on the same socket carries input upstream; a third lets
        // `Drop` unblock the reader.
        let input_stream = stream.try_clone()?;
        let shutdown = stream.try_clone()?;
        thread::spawn(move || write_input(input_stream, input_rx));

        let (event_tx, events) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            // Stop on EOF, a decode/socket error, or once the consumer has dropped
            // the events receiver (`send` then fails).
            while let Ok(msg) = protocol::read_framed::<_, Message>(&mut reader) {
                if event_tx.send(StreamEvent::from(msg)).is_err() {
                    break;
                }
            }
        });

        Ok(Session { events, shutdown })
    }

    /// Block until the next stream event, returning `None` once the stream ends
    /// (host disconnected). Suitable for a dedicated consumer thread.
    #[must_use]
    pub fn next_event(&self) -> Option<StreamEvent> {
        self.events.recv().ok()
    }

    /// The raw events receiver, for callers that want `try_recv`, `iter`, or to
    /// select across channels themselves.
    #[must_use]
    pub fn events(&self) -> &Receiver<StreamEvent> {
        &self.events
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Shut the socket so the reader's blocking read returns and it exits
        // promptly (it would otherwise park until the host sent something). The
        // writer exits when the caller drops its input `Sender`. Both threads are
        // detached and own their state — we deliberately don't join, since the
        // writer can be parked on `recv()` and joining it here would deadlock.
        let _ = self.shutdown.shutdown(Shutdown::Both);
    }
}

/// Drain the input channel onto the socket until the channel closes (caller done)
/// or a write fails (host gone).
fn write_input(mut stream: TcpStream, input_rx: Receiver<Input>) {
    while let Ok(input) = input_rx.recv() {
        if protocol::write_framed(&mut stream, &input).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// End-to-end loopback over a real socket: a fake host accepts a connection,
    /// reads the hello, sends StreamStart + two frames, and reads one input back.
    /// Exercises the whole `Session` API the way a real client (or FFI consumer)
    /// would — connect, receive N events, send input.
    #[test]
    fn session_round_trips_stream_and_input() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        // Fake host on its own thread; returns the input it received.
        let host = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut r = BufReader::new(sock.try_clone().unwrap());
            // Read + check the hello.
            let hello: ClientHello = protocol::read_framed(&mut r).unwrap();
            assert_eq!((hello.width, hello.height), (1920, 1080));
            // Send a stream start and two frames.
            protocol::write_framed(
                &mut sock,
                &Message::StreamStart {
                    width: 1920,
                    height: 1080,
                    codec: Codec::H264,
                    parameter_sets: vec![vec![0x67, 0x42], vec![0x68, 0xce]],
                },
            )
            .unwrap();
            for pts in 0..2 {
                protocol::write_framed(
                    &mut sock,
                    &Message::Frame {
                        pts_value: pts,
                        pts_timescale: 60,
                        keyframe: pts == 0,
                        data: vec![pts as u8; 4],
                    },
                )
                .unwrap();
            }
            // Read one input the client sends back.
            let got: Input = protocol::read_framed(&mut r).unwrap();
            got
        });

        let (input_tx, input_rx) = mpsc::channel();
        let hello = ClientHello {
            protocol_version: protocol::PROTOCOL_VERSION,
            width: 1920,
            height: 1080,
            capture_mode: protocol::CaptureMode::default(),
            platform: protocol::ClientPlatform::current(),
            pin: 0,
        };
        let session = Session::connect(&addr.to_string(), &hello, input_rx).unwrap();

        // First event is the stream start.
        match session.next_event().unwrap() {
            StreamEvent::Start { width, height, codec, parameter_sets } => {
                assert_eq!((width, height), (1920, 1080));
                assert_eq!(codec, Codec::H264);
                assert_eq!(parameter_sets.len(), 2);
            }
            other => panic!("expected Start, got {other:?}"),
        }
        // Then two frames.
        for pts in 0..2 {
            match session.next_event().unwrap() {
                StreamEvent::Frame { pts_value, keyframe, data, .. } => {
                    assert_eq!(pts_value, pts);
                    assert_eq!(keyframe, pts == 0);
                    assert_eq!(data, vec![pts as u8; 4]);
                }
                other => panic!("expected Frame, got {other:?}"),
            }
        }

        // Push an input upstream and confirm the fake host received it.
        let click = Input::MouseButton { button: protocol::Button::Left, pressed: true };
        input_tx.send(click.clone()).unwrap();
        assert_eq!(host.join().unwrap(), click);

        // With the host gone, the stream ends — next_event reports None.
        assert_eq!(session.next_event(), None);
        drop(input_tx);
    }
}
