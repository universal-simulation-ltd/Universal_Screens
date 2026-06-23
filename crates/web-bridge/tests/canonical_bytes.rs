//! Prints canonical `postcard` bytes for a few messages, so the spike page's
//! hand-rolled JS decoder can be cross-checked against the real wire format.
//! Run: `cargo test -p extender-web-bridge --test canonical_bytes -- --nocapture`

use extender_protocol::{ClientHello, CaptureMode, ClientPlatform, Codec, Input, Message};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn print_canonical_postcard_bytes() {
    let hello = ClientHello {
        protocol_version: 10,
        width: 1920,
        height: 1080,
        capture_mode: CaptureMode::MirrorPrimary,
        platform: ClientPlatform::Unknown,
        pin: 4321,
    };
    let start = Message::StreamStart {
        width: 1920,
        height: 1080,
        codec: Codec::H264,
        parameter_sets: vec![vec![0x67, 0x42, 0xc0, 0x1f], vec![0x68, 0xce, 0x3c, 0x80]],
    };
    let frame = Message::Frame {
        pts_value: 12_345,
        pts_timescale: 60,
        keyframe: true,
        data: vec![0xde, 0xad, 0xbe, 0xef],
    };
    let mousemove = Input::MouseMove { x: 0.5, y: 0.5 };

    println!("HELLO {}", hex(&postcard::to_stdvec(&hello).unwrap()));
    println!("STREAMSTART {}", hex(&postcard::to_stdvec(&start).unwrap()));
    println!("FRAME {}", hex(&postcard::to_stdvec(&frame).unwrap()));
    println!("MOUSEMOVE {}", hex(&postcard::to_stdvec(&mousemove).unwrap()));
}
