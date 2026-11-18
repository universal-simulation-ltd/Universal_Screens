//! Where a virtual display sits: left, right, above or below this Mac's main
//! display, chosen from either end, without System Settings → Displays.
//!
//! macOS puts a new display wherever it likes (James's Windows laptop, sitting
//! to the RIGHT of his Mac, came up on the LEFT, 2026-11-18), so the host moves
//! it with `CGConfigureDisplayOrigin` and remembers the side per connecting
//! device. The choice comes from the host window's Virtual displays panel or
//! from the client (`Input::PlaceDisplay`), and is applied again every time
//! that device's display is created.

use std::collections::BTreeMap;
use std::path::PathBuf;

use core_graphics::display::{CGConfigureOption, CGDisplay};
use extender_protocol::DisplaySide;

/// A display's global rectangle in points: origin x/y, width, height.
type Rect = (f64, f64, f64, f64);

/// The origin that puts a `display`-sized rectangle on `side` of `main`, touching
/// it edge to edge and centred along that edge. Pure, so it is testable.
pub(crate) fn origin_for(main: Rect, display: (f64, f64), side: DisplaySide) -> (i32, i32) {
    let (mx, my, mw, mh) = main;
    let (w, h) = display;
    let centre_x = mx + (mw - w) / 2.0;
    let centre_y = my + (mh - h) / 2.0;
    let (x, y) = match side {
        DisplaySide::Left => (mx - w, centre_y),
        DisplaySide::Right => (mx + mw, centre_y),
        DisplaySide::Above => (centre_x, my - h),
        DisplaySide::Below => (centre_x, my + mh),
    };
    (x.round() as i32, y.round() as i32)
}

/// Move display `id` to `side` of the main display, for this login session.
/// macOS settles any overlap with other screens itself.
pub(crate) fn place(id: u32, side: DisplaySide) -> Result<(), String> {
    let main = CGDisplay::main();
    if main.id == id {
        return Err("that display is the main one".into());
    }
    let m = main.bounds();
    let target = CGDisplay::new(id);
    let t = target.bounds();
    let (x, y) = origin_for(
        (m.origin.x, m.origin.y, m.size.width, m.size.height),
        (t.size.width, t.size.height),
        side,
    );
    let config = main.begin_configuration().map_err(|e| format!("begin: {e}"))?;
    if let Err(e) = target.configure_display_origin(&config, x, y) {
        let _ = main.cancel_configuration(&config);
        return Err(format!("origin: {e}"));
    }
    main.complete_configuration(&config, CGConfigureOption::ConfigureForSession)
        .map_err(|e| format!("complete: {e}"))?;
    println!("placed display {id} {} of the main display", side.as_str());
    Ok(())
}

/// The side saved for `device`, if one was chosen before.
pub(crate) fn saved(device: &str) -> Option<DisplaySide> {
    load().get(device).copied()
}

/// Remember `side` for `device` (the raw connecting-device name).
pub(crate) fn remember(device: &str, side: DisplaySide) {
    if device.trim().is_empty() {
        return;
    }
    let mut all = load();
    all.insert(device.to_owned(), side);
    if let Some(path) = store_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, format_store(&all));
    }
}

fn load() -> BTreeMap<String, DisplaySide> {
    store_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|text| parse_store(&text))
        .unwrap_or_default()
}

/// `~/Library/Application Support/UniversalScreens/display-sides.txt`, beside
/// the pairing files.
fn store_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/UniversalScreens/display-sides.txt"))
}

/// One `side<TAB>device` line per device. Device names can hold spaces but
/// not tabs or newlines (they come from a single-line field).
fn format_store(all: &BTreeMap<String, DisplaySide>) -> String {
    all.iter().map(|(device, side)| format!("{}\t{device}\n", side.as_str())).collect()
}

fn parse_store(text: &str) -> BTreeMap<String, DisplaySide> {
    text.lines()
        .filter_map(|line| {
            let (side, device) = line.split_once('\t')?;
            Some((device.to_owned(), DisplaySide::parse(side)?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: Rect = (0.0, 0.0, 1512.0, 982.0);

    #[test]
    fn each_side_touches_the_main_display_and_is_centred() {
        let d = (1440.0, 900.0);
        assert_eq!(origin_for(MAIN, d, DisplaySide::Right), (1512, 41));
        assert_eq!(origin_for(MAIN, d, DisplaySide::Left), (-1440, 41));
        assert_eq!(origin_for(MAIN, d, DisplaySide::Above), (36, -900));
        assert_eq!(origin_for(MAIN, d, DisplaySide::Below), (36, 982));
    }

    #[test]
    fn saved_sides_survive_a_round_trip() {
        let mut all = BTreeMap::new();
        all.insert("SPECTRE".to_owned(), DisplaySide::Right);
        all.insert("James's iPhone".to_owned(), DisplaySide::Below);
        assert_eq!(parse_store(&format_store(&all)), all);
        assert!(parse_store("sideways\tX\nnonsense\n").is_empty());
    }
}
