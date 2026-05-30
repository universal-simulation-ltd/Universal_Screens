//! M3 feasibility spike: prove we can create a private `CGVirtualDisplay` and
//! have macOS register it as a real display from our Rust + Objective-C-shim
//! stack — the riskiest unknown of the whole project, checked in isolation.
//!
//! Creates one 1920x1080 virtual display via the shim, confirms the active
//! display count grows and the new `displayID` appears (`CGGetActiveDisplayList`
//! via `core-graphics`), prints details, and holds it alive ~15s so you can also
//! confirm it in System Settings > Displays. The display vanishes on exit.
//!
//! Run: cargo run -p extender-host --example vdisplay_probe

use std::thread::sleep;
use std::time::Duration;

use core_graphics::display::CGDisplay;

extern "C" {
    /// Create a virtual display; returns its `CGDirectDisplayID` (0 on failure).
    fn extender_vdisplay_create(width: u32, height: u32) -> u32;
}

fn main() {
    let before = CGDisplay::active_displays().unwrap_or_default();
    println!("active displays before: {before:?}");

    let id = unsafe { extender_vdisplay_create(1920, 1080) };
    if id == 0 {
        eprintln!(
            "FAILED to create a virtual display — CGVirtualDisplay initWithDescriptor/applySettings \
             rejected our request (the private interface may differ on this macOS version)."
        );
        return;
    }
    println!("created virtual display, displayID = {id}");

    // Registration is asynchronous on the descriptor's queue; poll briefly.
    let mut after = before.clone();
    for _ in 0..50 {
        after = CGDisplay::active_displays().unwrap_or_default();
        if after.contains(&id) {
            break;
        }
        sleep(Duration::from_millis(100));
    }

    println!("active displays after:  {after:?}");
    if after.contains(&id) {
        let display = CGDisplay::new(id);
        let mode = display.display_mode();
        let (pw, ph) = mode.as_ref().map_or((0, 0), |m| (m.pixel_width(), m.pixel_height()));
        let (lw, lh) = mode.as_ref().map_or((0, 0), |m| (m.width(), m.height()));
        println!(
            "SUCCESS: virtual display {id} — {pw}x{ph} px backing / {lw}x{lh} pt logical (scale {:.1}), count {} -> {}.",
            pw as f64 / lw.max(1) as f64,
            before.len(),
            after.len()
        );
        println!("Look in System Settings > Displays — holding it alive for 15s...");
        sleep(Duration::from_secs(15));
        println!("done (the virtual display is removed as this process exits)");
    } else {
        eprintln!("display {id} was created but did not appear in the active display list");
    }
}
