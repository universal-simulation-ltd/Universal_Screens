//! M2b injection probe: validate synthetic mouse injection via CoreGraphics in
//! isolation — no networking. Traces the cursor around a small square a few
//! times so you can SEE injection working, and tells you what to do if it
//! doesn't move (grant Accessibility permission).
//!
//! Mouse *move* only on purpose: a move is harmless and self-evident, and it
//! exercises the same `CGEvent` + `post()` + Accessibility path that clicks,
//! scroll, and keystrokes use. Those are validated in M2c/M2d, where they're
//! aimed at an intended target rather than wherever the pointer happens to be.
//!
//! Run: cargo run -p extender-host --example inject_probe
//! Requires Accessibility permission (System Settings > Privacy & Security >
//! Accessibility) for the app that runs it (e.g. your terminal).

use std::thread::sleep;
use std::time::Duration;

use core_graphics::event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;

/// Post an absolute mouse-move to screen coordinates `(x, y)`.
fn move_cursor(x: f64, y: f64) {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        eprintln!("could not create a CGEventSource");
        return;
    };
    match CGEvent::new_mouse_event(
        source,
        CGEventType::MouseMoved,
        CGPoint::new(x, y),
        CGMouseButton::Left,
    ) {
        Ok(event) => event.post(CGEventTapLocation::HID),
        Err(()) => eprintln!("could not create a mouse-move event"),
    }
}

fn main() {
    println!("inject_probe: tracing the cursor around a square 3 times.");
    println!("watch your pointer — if it does NOT move, grant Accessibility to the app");
    println!("running this (System Settings > Privacy & Security > Accessibility), then rerun.");

    // A small square in a safe top-left region of the screen (absolute coords).
    let corners = [(200.0, 200.0), (500.0, 200.0), (500.0, 500.0), (200.0, 500.0)];
    for _ in 0..3 {
        for &(x, y) in &corners {
            move_cursor(x, y);
            sleep(Duration::from_millis(150));
        }
    }
    move_cursor(200.0, 200.0);

    println!("done — if the pointer traced a square, CGEvent injection + Accessibility work.");
}
