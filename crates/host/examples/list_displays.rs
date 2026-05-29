//! Read-only diagnostic: list the active displays and each one's current mode
//! (backing pixels / logical points). Creates nothing, so it's always safe to
//! run — handy for checking virtual-display state.
//!
//! Run: cargo run -p extender-host --example list_displays

use core_graphics::display::CGDisplay;

fn main() {
    let ids = CGDisplay::active_displays().unwrap_or_default();
    println!("{} active display(s): {ids:?}", ids.len());
    for id in ids {
        let display = CGDisplay::new(id);
        match display.display_mode() {
            Some(m) => println!(
                "  display {id}: {}x{} px backing / {}x{} pt logical",
                m.pixel_width(),
                m.pixel_height(),
                m.width(),
                m.height(),
            ),
            None => println!("  display {id}: (no current mode)"),
        }
    }
}
