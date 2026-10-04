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
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

pub use extender_protocol::{self as protocol, ClientHello, Codec, Input, Message};
pub use extender_transport::Failure;
use extender_transport::{self as transport, Conn, PairingStore};

/// The client's pairing file: its long-term key and the hosts it has paired
/// with. Separate from a host's (`host-pairing.txt`), so one machine can be both.
pub const PAIRING_FILE: &str = "client-pairing.txt";

/// Where the pairing file lives. See [`set_pairing_location`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingLocation {
    /// The platform default: the per-user config folder on desktops, the app's
    /// `Library/Application Support` on iOS, and **nowhere** on Android (which
    /// must say where).
    Default,
    /// This folder.
    Dir(PathBuf),
    /// Keep nothing: pair with the code on every connection. For tests.
    Nowhere,
}

static PAIRING_LOCATION: Mutex<Option<PairingLocation>> = Mutex::new(None);

/// Tell the client where to keep its pairing keys. Call once, before the
/// first connect. Android must (its files folder is only known to the app);
/// everything else has a working default.
pub fn set_pairing_location(location: PairingLocation) {
    if let Ok(mut slot) = PAIRING_LOCATION.lock() {
        *slot = Some(location);
    }
}

/// The pairing file this client uses, or `None` to keep nothing.
fn pairing_path() -> Option<PathBuf> {
    let location = PAIRING_LOCATION.lock().ok().and_then(|l| l.clone()).unwrap_or(PairingLocation::Default);
    match location {
        PairingLocation::Dir(dir) => Some(dir.join(PAIRING_FILE)),
        PairingLocation::Nowhere => None,
        PairingLocation::Default => default_pairing_dir().map(|dir| dir.join(PAIRING_FILE)),
    }
}

#[cfg(target_os = "ios")]
fn default_pairing_dir() -> Option<PathBuf> {
    // Inside the iOS sandbox HOME is the app's own container.
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support/UniversalScreens"))
}

#[cfg(target_os = "android")]
fn default_pairing_dir() -> Option<PathBuf> {
    None
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn default_pairing_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("UniversalScreens"))
}

/// The client's pairing store: from its file when it has one, otherwise in
/// memory for this connection only (the code is then needed every time — as
/// before pairing was remembered, and no less safe).
fn open_pairing_store() -> io::Result<PairingStore> {
    match pairing_path().map(|path| PairingStore::open(&path)) {
        Some(Ok(store)) => Ok(store),
        Some(Err(e)) => {
            eprintln!("pairing keys unavailable ({e}); pairing with the code this time");
            PairingStore::ephemeral()
        }
        None => PairingStore::ephemeral(),
    }
}

/// How long to wait for the TCP connection to the host to be established before
/// giving up. Without a cap the OS default applies (tens of seconds, sometimes
/// longer), so an unreachable host would leave a client parked on "Connecting…"
/// with no error for an uncomfortably long time.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to allow the encrypted handshake (Noise + the [`ClientHello`]) to
/// complete once the socket is open. Bounds the case where a peer accepts the TCP
/// connection but never speaks the protocol (wrong port, or a firewall that drops
/// packets after the accept), which would otherwise block on the handshake read
/// indefinitely. Cleared once the session is live so the frame reader can block
/// normally.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

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
    shutdown: Conn,
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
        let mut store = open_pairing_store()?;
        Session::connect_inner(addr, hello, input_rx, CONNECT_TIMEOUT, HANDSHAKE_TIMEOUT, &mut store)
    }

    /// [`connect`](Session::connect) with the two deadlines and the pairing store
    /// injectable, so tests can drive it with short timeouts and no files. See
    /// the public method for the full contract.
    fn connect_inner(
        addr: &str,
        hello: &ClientHello,
        input_rx: Receiver<Input>,
        connect_timeout: Duration,
        handshake_timeout: Duration,
        store: &mut PairingStore,
    ) -> io::Result<Session> {
        // Encrypt the transport *before* any framing, and settle who may talk:
        // a remembered host by its key, otherwise pairing over the code. The
        // `ClientHello` and everything after it then travel inside the tunnel,
        // so the LAN sees only ciphertext. A wrong code fails here.
        let mut conn = open_tunnel(addr, hello.pin, store, connect_timeout, handshake_timeout)?;

        // Handshake first, on the caller's thread, so a failure surfaces as an
        // error from `connect` rather than dying silently in a background thread.
        protocol::write_framed(&mut conn, hello)?;

        // Session is live: drop the handshake deadlines so the downstream reader
        // parks on the socket waiting for the next frame (which may be seconds out)
        // rather than tripping the timeout during a quiet stretch.
        conn.set_read_timeout(None)?;
        conn.set_write_timeout(None)?;

        // A second handle on the same socket carries input upstream; a third lets
        // `Drop` unblock the reader.
        let input_stream = conn.try_clone()?;
        let shutdown = conn.try_clone()?;
        thread::spawn(move || write_input(input_stream, input_rx));

        let (event_tx, events) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(conn);
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

/// Open the encrypted tunnel to `addr`.
///
/// 1. If this client has paired with any host, try **reconnecting** with the
///    remembered keys — no code. That is what lets a saved machine reconnect
///    after it restarted with a new PIN.
/// 2. If the host does not know this device (or it is a host this device has
///    not paired with), **pair** with `pin` on a fresh connection, and remember
///    the host's key for next time.
///
/// ⚠️ **No v1 fall-back.** A v0.3 host speaks only the v1 handshake, whose
/// first message lets a recording be brute-forced for the PIN. Falling back to
/// it when v2 is refused would hand exactly that to anyone able to make v2
/// fail — so against a v0.3 host this fails with [`Failure::OlderHost`] and the
/// user updates the host. (Hosts still *accept* v1, so v0.3 clients keep
/// working; the browser falls back only when the host's own bridge says it is
/// old. See `docs/M10-transport-encryption.md`.)
fn open_tunnel(
    addr: &str,
    pin: u32,
    store: &mut PairingStore,
    connect_timeout: Duration,
    handshake_timeout: Duration,
) -> io::Result<Conn> {
    if !store.peers().is_empty() {
        let stream = dial(addr, connect_timeout, handshake_timeout)?;
        match transport::connect_known(stream, store.identity(), store.peer_keys()) {
            Ok((conn, _)) => return Ok(conn),
            Err(e) if matches!(transport::failure(&e), Some(Failure::NotPaired | Failure::UnknownHost)) => {}
            Err(e) => return Err(e),
        }
    }
    let stream = dial(addr, connect_timeout, handshake_timeout)?;
    let (conn, host_key) = transport::connect_pair(stream, pin, store.identity())?;
    // Pairing with no code (a host that lets anyone in) proves nothing about
    // who answered, so it earns no trust.
    if pin != 0 {
        if let Err(e) = store.remember(host_key, addr) {
            eprintln!("could not save the pairing with {addr}: {e}");
        }
    }
    Ok(conn)
}

/// A TCP connection to `addr`, with Nagle off and the handshake deadlines set.
fn dial(addr: &str, connect_timeout: Duration, handshake_timeout: Duration) -> io::Result<TcpStream> {
    let stream = tcp_connect_within(addr, connect_timeout)?;
    let _ = stream.set_nodelay(true); // disable Nagle — low latency for video + input
    // Bound the handshake so a peer that accepts the TCP connection but never
    // speaks the protocol can't wedge the connect forever — it surfaces as an
    // error instead of a silent hang. Cleared once the session is live so the
    // frame reader can park waiting on the socket.
    stream.set_read_timeout(Some(handshake_timeout))?;
    stream.set_write_timeout(Some(handshake_timeout))?;
    Ok(stream)
}

/// Connect to `addr` with a bounded wait, trying each resolved socket address in
/// turn. Mirrors [`TcpStream::connect`] (which also tries every resolved address)
/// but caps each attempt with [`TcpStream::connect_timeout`], so an unreachable
/// host fails within `timeout` instead of after the (much longer) OS default.
fn tcp_connect_within(addr: &str, timeout: Duration) -> io::Result<TcpStream> {
    let mut last_err: Option<io::Error> = None;
    for socket_addr in addr.to_socket_addrs()? {
        match TcpStream::connect_timeout(&socket_addr, timeout) {
            Ok(stream) => return Ok(stream),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, format!("could not resolve host {addr:?}"))
    }))
}

/// Drain the input channel onto the socket until the channel closes (caller done)
/// or a write fails (host gone).
fn write_input(mut stream: Conn, input_rx: Receiver<Input>) {
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
    use std::time::Instant;

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
            let (sock, _) = listener.accept().unwrap();
            // Mirror the real host: run the Noise responder handshake (PIN 0, to
            // match the client's hello below) before any framing.
            let conn = extender_transport::accept(sock, 0).unwrap();
            let mut sock = conn.try_clone().unwrap();
            let mut r = BufReader::new(conn);
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
            device_name: String::new(),
        };
        // Never touch the developer's real pairing file from a test.
        set_pairing_location(PairingLocation::Nowhere);
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

    /// A peer that accepts the TCP connection but never runs the handshake must not
    /// wedge `connect` forever: the handshake timeout turns it into a prompt error
    /// (this is what makes a "Cancel" affordance unnecessary in the worst case, and
    /// lets an abandoned connect attempt terminate on its own).
    #[test]
    fn connect_times_out_on_a_silent_peer() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        // Accept the connection, then sit silent for a while holding it open.
        let host = thread::spawn(move || {
            let (_sock, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_secs(2));
        });

        let (_input_tx, input_rx) = mpsc::channel();
        let hello = ClientHello {
            protocol_version: protocol::PROTOCOL_VERSION,
            width: 1920,
            height: 1080,
            capture_mode: protocol::CaptureMode::default(),
            platform: protocol::ClientPlatform::current(),
            pin: 0,
            device_name: String::new(),
        };

        let start = Instant::now();
        let result = Session::connect_inner(
            &addr,
            &hello,
            input_rx,
            Duration::from_secs(5),
            Duration::from_millis(300),
            &mut PairingStore::ephemeral().unwrap(),
        );
        assert!(result.is_err(), "a silent peer must fail the handshake, not hang");
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "connect should fail promptly via the handshake timeout, took {:?}",
            start.elapsed(),
        );
        host.join().unwrap();
    }

    // ---- pairing: remembered hosts, and no v1 fall-back ---------------------

    fn hello(pin: u32) -> ClientHello {
        ClientHello {
            protocol_version: protocol::PROTOCOL_VERSION,
            width: 800,
            height: 600,
            capture_mode: protocol::CaptureMode::ControlOnly,
            platform: protocol::ClientPlatform::current(),
            pin,
            device_name: String::new(),
        }
    }

    /// The fake host's store afterwards, and how each connection authenticated
    /// (`None` for a refused one).
    type HostRecord = (PairingStore, Vec<Option<transport::PeerAuth>>);

    /// A host that serves `connections` connections with `accept_with` over
    /// one store, at the PINs given, reading each hello and replying with a
    /// HostInfo. Returns the store and how each connection was authenticated.
    fn paired_host(
        pins: Vec<u32>,
        store: PairingStore,
    ) -> (String, thread::JoinHandle<HostRecord>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let host = thread::spawn(move || {
            let mut store = store;
            let mut auths = Vec::new();
            let mut pins = pins.into_iter();
            while let Some(pin) = pins.next() {
                let (sock, _) = listener.accept().unwrap();
                sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                match transport::accept_with(sock, pin, &mut store, "test") {
                    Ok(mut conn) => {
                        auths.push(Some(conn.auth()));
                        let _hello: ClientHello = protocol::read_framed(&mut conn).unwrap();
                        protocol::write_framed(
                            &mut conn,
                            &Message::HostInfo { os: "test".into(), name: "host".into() },
                        )
                        .unwrap();
                    }
                    // A refused reconnect: the client pairs on a second
                    // connection, which this same PIN serves.
                    Err(_) => {
                        auths.push(None);
                        pins = std::iter::once(pin).chain(pins).collect::<Vec<_>>().into_iter();
                    }
                }
            }
            (store, auths)
        });
        (addr, host)
    }

    fn connect_with(addr: &str, pin: u32, store: &mut PairingStore) -> io::Result<Session> {
        let (_tx, rx) = mpsc::channel();
        Session::connect_inner(addr, &hello(pin), rx, Duration::from_secs(5), Duration::from_secs(5), store)
    }

    #[test]
    fn a_paired_client_reconnects_with_no_code_after_the_host_changes_its_pin() {
        let mut client = PairingStore::ephemeral().unwrap();
        let (addr, host) = paired_host(vec![4321, 8765], PairingStore::ephemeral().unwrap());

        let first = connect_with(&addr, 4321, &mut client).unwrap();
        assert!(matches!(first.next_event(), Some(StreamEvent::HostInfo { .. })));
        assert_eq!(client.peers().len(), 1, "the client remembers the host it paired");

        // New host PIN, and the client offers none: the keys are enough.
        let second = connect_with(&addr, 0, &mut client).unwrap();
        assert!(matches!(second.next_event(), Some(StreamEvent::HostInfo { .. })));

        let (_, auths) = host.join().unwrap();
        assert!(matches!(auths[0], Some(transport::PeerAuth::Paired { .. })));
        assert!(matches!(auths[1], Some(transport::PeerAuth::Known { .. })));
    }

    #[test]
    fn a_host_that_forgot_this_device_gets_the_code_on_a_second_connection() {
        let mut client = PairingStore::ephemeral().unwrap();
        let host_store = PairingStore::ephemeral().unwrap();
        // The client trusts this host, but the host has no memory of it.
        client.remember(host_store.identity().public(), "earlier").unwrap();
        let (addr, host) = paired_host(vec![4321], host_store);

        let session = connect_with(&addr, 4321, &mut client).unwrap();
        assert!(matches!(session.next_event(), Some(StreamEvent::HostInfo { .. })));
        let (store, auths) = host.join().unwrap();
        assert_eq!(auths.len(), 2, "one refused reconnect, then the pairing");
        assert!(auths[0].is_none());
        assert!(matches!(auths[1], Some(transport::PeerAuth::Paired { .. })));
        assert_eq!(store.peers().len(), 1);
    }

    /// The downgrade that must not happen: against a v0.3 host the client
    /// reports an older host, and never sends the v1 handshake whose recording
    /// would give the PIN away.
    #[test]
    fn a_v03_host_is_reported_as_older_and_is_never_sent_the_v1_handshake() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let old_host = thread::spawn(move || {
            listener.set_nonblocking(false).unwrap();
            let mut openings = Vec::new();
            // v0.3: read the 5-byte preamble and a u16 length; over 4 KiB, close.
            let (mut sock, _) = listener.accept().unwrap();
            let mut head = [0u8; 7];
            std::io::Read::read_exact(&mut sock, &mut head).unwrap();
            openings.push(head);
            drop(sock);
            // Any second connection would be a fall-back attempt.
            listener.set_nonblocking(true).unwrap();
            thread::sleep(Duration::from_millis(500));
            if let Ok((mut sock, _)) = listener.accept() {
                let mut head = [0u8; 7];
                let _ = std::io::Read::read_exact(&mut sock, &mut head);
                openings.push(head);
            }
            openings
        });
        let err = connect_with(&addr, 4321, &mut PairingStore::ephemeral().unwrap())
            .err()
            .expect("a v0.3 host cannot pair");
        assert_eq!(transport::failure(&err), Some(Failure::OlderHost), "{err}");
        let openings = old_host.join().unwrap();
        assert_eq!(openings.len(), 1, "no second (v1) attempt");
        assert_eq!(&openings[0][5..], &[0xFF, 0xFF], "the one attempt was v2");
    }
}
