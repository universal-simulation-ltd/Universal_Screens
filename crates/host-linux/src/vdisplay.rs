//! A **second screen** on X11: extra desktop area the phone becomes, rather
//! than a second copy of the monitor.
//!
//! This is the Linux answer to the Windows host's virtual-display driver and
//! macOS's `CGVirtualDisplay`, and it was the last X11 mode left unbuilt (the
//! `—` row of `docs/LINUX-HOST.md` §7). The mechanism is RandR's, not a
//! driver's: **grow the root framebuffer to the right, then declare the new area
//! a monitor** with `RRSetMonitor`. Toolkits and window managers enumerate
//! monitors through exactly that call, so a window maximises into the new area,
//! a panel can be placed on it, and the pointer moves across — it is a real
//! second display, not a rectangle we happen to photograph.
//!
//! ⚠️ **Why not a `VIRTUAL` output, which §7 named.** Enabling a spare RandR
//! output is the more obvious reading of "the X11 equivalent", and it is what
//! `xf86-video-intel` (`VIRTUAL1`/`VIRTUAL2`) and the dummy driver (`DUMMY0`…)
//! offer — but **`modesetting`, the default driver on essentially every current
//! desktop, exposes no spare outputs at all**, so that route would work on a
//! minority of machines and silently fall back on the rest. Framebuffer +
//! monitor needs nothing from the driver beyond a resizable screen, which every
//! RandR 1.2 server has. The two are not exclusive: if this ever needs a real
//! CRTC behind it (for a compositor that refuses to draw outside a CRTC), the
//! output route can be added as a preferred path with this as the fallback.
//!
//! ## What is deliberately temporary
//!
//! The framebuffer grows for the length of one session and shrinks again when
//! [`VirtualScreen`] drops. That is the *whole* lifecycle: a host that left a
//! 3000-pixel-wide desktop behind after a phone disconnected would be a bug
//! reported as "Screens broke my display settings".
//!
//! ⚠️ Restoring is conditional on nothing else having changed it in the
//! meantime — see [`VirtualScreen::drop`]. Putting the size back unconditionally
//! would undo a resolution change the *user* made mid-session.
//!
//! ⚠️ **Monitor lifetime differs between servers, so both cleanups are done.**
//! Measured, not assumed: on Xvfb an `RRSetMonitor` monitor disappears when the
//! creating client disconnects, while a real Xorg keeps it. So this deletes the
//! monitor explicitly *and* holds its own connection for the session — the
//! explicit delete is what a crash-free exit uses, and dropping the connection
//! is what limits the damage of a crash on the servers that honour it.

use x11rb::connection::Connection;
use x11rb::protocol::randr::{ConnectionExt as _, MonitorInfo};
use x11rb::protocol::xproto::{Atom, ConnectionExt as _, Window};
use x11rb::rust_connection::RustConnection;

use crate::capture::Region;

/// The RandR monitor name this host creates. Fixed rather than derived from the
/// client's device name: the host serves one client at a time (`accept` is a
/// sequential loop), and a fixed name is what makes a leftover monitor from a
/// previous crash findable and deletable.
const MONITOR_NAME: &str = "Universal-Screens";

/// Smallest second screen worth making. Below this the client has sent a
/// nonsense size and mirroring is the better answer.
const MIN_SIDE: u32 = 240;

/// Largest, matching the other hosts' `MAX_DIMENSION` hello check.
const MAX_SIDE: u32 = 16384;

/// Fallback pixel density when the server reports no physical size, as Xvfb and
/// several headless servers do (0 mm). 96 dpi is X's own default assumption.
const FALLBACK_DPI: u32 = 96;

/// A live second screen: extra framebuffer plus the RandR monitor that makes it
/// a display. Dropping it puts the desktop back the way it was.
pub(crate) struct VirtualScreen {
    /// ⚠️ Held for the whole session on purpose — see the module note on
    /// monitor lifetime. Dropping this connection is the backstop cleanup.
    conn: RustConnection,
    root: Window,
    name: Atom,
    region: Region,
    /// The framebuffer as it was before this grew it, for [`Self::drop`].
    prev: ScreenSize,
    /// The size this grew it to. Drop restores only if this is still current.
    grown: (u16, u16),
}

/// A framebuffer size in both units RandR wants: pixels and millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScreenSize {
    width: u16,
    height: u16,
    mm_width: u32,
    mm_height: u32,
}

impl VirtualScreen {
    /// Extend the desktop by a `want_w` × `want_h` area to the right of
    /// everything already there, and declare it a monitor.
    ///
    /// `label` names the device in the log line only; the monitor's own name is
    /// fixed (see [`MONITOR_NAME`]).
    ///
    /// # Errors
    /// A human-readable reason, on any of: no X server, a server older than
    /// RandR 1.5 (where monitors do not exist), an implausible client size, a
    /// framebuffer that cannot grow that far, or the server refusing either
    /// call. **Every one of them is a reason to mirror instead**, which is what
    /// [`crate::stream`] does with it — none is a reason to fail the session.
    pub(crate) fn create(want_w: u32, want_h: u32, label: &str) -> Result<Self, String> {
        if !(MIN_SIDE..=MAX_SIDE).contains(&want_w) || !(MIN_SIDE..=MAX_SIDE).contains(&want_h) {
            return Err(format!(
                "the client asked for a {want_w}x{want_h} second screen, which is not a plausible display"
            ));
        }

        let (conn, screen_num) =
            x11rb::connect(None).map_err(|e| format!("cannot connect to an X server: {e}"))?;
        let screen = conn.setup().roots[screen_num].clone();
        let root = screen.root;

        // ⚠️ The version handshake is not a formality: RandR requests before it
        // are undefined, and `RRSetMonitor` arrived in 1.5. A 1.4 server (or one
        // with the extension absent) must be told apart from a working one here
        // rather than by a BadRequest three calls later.
        let ver = conn
            .randr_query_version(1, 5)
            .map_err(|e| format!("this X server has no RandR extension: {e}"))?
            .reply()
            .map_err(|e| format!("this X server has no RandR extension: {e}"))?;
        if (ver.major_version, ver.minor_version) < (1, 5) {
            return Err(format!(
                "this X server speaks RandR {}.{}; monitors need 1.5",
                ver.major_version, ver.minor_version
            ));
        }

        let prev = current_size(&conn, root, &screen)?;
        let range = conn
            .randr_get_screen_size_range(root)
            .map_err(|e| format!("cannot read the screen size range: {e}"))?
            .reply()
            .map_err(|e| format!("cannot read the screen size range: {e}"))?;

        // H.264 wants even dimensions and so does the encoder downstream; doing
        // it here means the region and the stream agree about the size.
        let (want_w, want_h) = (want_w & !1, want_h & !1);
        let (new_w, new_h) = grown_size(prev.width, prev.height, want_w, want_h).ok_or_else(
            || format!("a {want_w}x{want_h} second screen would overflow the X framebuffer"),
        )?;
        if new_w > range.max_width || new_h > range.max_height {
            return Err(format!(
                "this X server's framebuffer cannot grow to {new_w}x{new_h} (its maximum is \
                 {}x{}) — a headless server such as Xvfb is fixed at its start-up size",
                range.max_width, range.max_height
            ));
        }

        let region = Region {
            x: i16::try_from(prev.width).map_err(|_| "the desktop is too wide to extend")?,
            y: 0,
            width: u16::try_from(want_w).map_err(|_| "the client asked for too wide a screen")?,
            height: u16::try_from(want_h).map_err(|_| "the client asked for too tall a screen")?,
        };

        // A monitor left behind by a previous crash owns the name we want, and
        // `RRSetMonitor` would replace it anyway — but deleting first keeps the
        // failure path simple, and costs one round trip once per session.
        let name = conn
            .intern_atom(false, MONITOR_NAME.as_bytes())
            .map_err(|e| format!("cannot name the monitor: {e}"))?
            .reply()
            .map_err(|e| format!("cannot name the monitor: {e}"))?
            .atom;
        let _ = conn.randr_delete_monitor(root, name).map(|c| c.check());

        let grown = ScreenSize {
            width: new_w,
            height: new_h,
            mm_width: scale_mm(prev.mm_width, prev.width, new_w),
            mm_height: scale_mm(prev.mm_height, prev.height, new_h),
        };
        set_size(&conn, root, grown).map_err(|e| {
            format!("the X server refused to grow the framebuffer to {new_w}x{new_h}: {e}")
        })?;

        let info = MonitorInfo {
            name,
            primary: false,
            // "automatic" means the server made this monitor from its own
            // outputs. Ours is deliberate, and saying otherwise would invite a
            // desktop settings panel to re-derive it away.
            automatic: false,
            x: region.x,
            y: region.y,
            width: region.width,
            height: region.height,
            width_in_millimeters: scale_mm(prev.mm_width, prev.width, region.width),
            height_in_millimeters: scale_mm(prev.mm_height, prev.height, region.height),
            // No outputs: the area has no CRTC scanning it out, which is exactly
            // what makes it a *virtual* screen rather than a second physical one.
            outputs: Vec::new(),
        };
        let set = conn
            .randr_set_monitor(root, info)
            .map_err(|e| e.to_string())
            .and_then(|c| c.check().map_err(|e| e.to_string()));
        // ⚠️ Roll the framebuffer back before returning the error. Without this
        // a failed second screen leaves the user's desktop permanently wider —
        // a worse outcome than the mirror they are about to get instead.
        if let Err(e) = set {
            let _ = set_size(&conn, root, prev);
            return Err(format!("the X server refused to create a monitor: {e}"));
        }

        // Prove it landed rather than trusting a void request. `get_active:
        // false` matters: ours has no outputs, so an active-only listing would
        // not contain it and this check would fail a working second screen.
        let listed = conn
            .randr_get_monitors(root, false)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|m| m.monitors.iter().any(|m| m.name == name));
        if !listed {
            let _ = conn.randr_delete_monitor(root, name).map(|c| c.check());
            let _ = set_size(&conn, root, prev);
            return Err("the X server accepted the monitor but does not list it".to_owned());
        }

        println!(
            "second screen: {}x{} at +{}+{} for {label:?} (desktop grown {}x{} -> {new_w}x{new_h})",
            region.width, region.height, region.x, region.y, prev.width, prev.height
        );
        Ok(Self { conn, root, name, region, prev, grown: (new_w, new_h) })
    }

    /// The area of the root window this screen occupies — what [`crate::stream`]
    /// captures instead of the whole desktop.
    pub(crate) fn region(&self) -> Region {
        self.region
    }
}

impl Drop for VirtualScreen {
    fn drop(&mut self) {
        let _ = self.conn.randr_delete_monitor(self.root, self.name).map(|c| c.check());

        // ⚠️ Only shrink back if the framebuffer is still the size this made it.
        // A user who changed resolution mid-session would otherwise have that
        // change silently undone when their phone disconnected — the sort of
        // "it fixed itself later" bug nobody ever traces back to here.
        let unchanged = self
            .conn
            .get_geometry(self.root)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|g| (g.width, g.height) == self.grown);
        if unchanged {
            let _ = set_size(&self.conn, self.root, self.prev);
        } else {
            eprintln!(
                "second screen: the desktop was resized during the session, so its size is being \
                 left alone rather than reverted"
            );
        }
        let _ = self.conn.flush();
    }
}

/// Apply a framebuffer size, waiting for the server's answer.
///
/// ⚠️ `.check()` is the point. `RRSetScreenSize` is a void request, so without
/// it a refusal (past the driver's maximum, or a size the server will not take)
/// arrives later as an unattributed protocol error, and the caller carries on
/// believing in a screen that was never created.
fn set_size(conn: &RustConnection, root: Window, size: ScreenSize) -> Result<(), String> {
    conn.randr_set_screen_size(root, size.width, size.height, size.mm_width, size.mm_height)
        .map_err(|e| e.to_string())
        .and_then(|c| c.check().map_err(|e| e.to_string()))
}

/// The framebuffer as it is right now.
///
/// The live geometry comes from `GetGeometry` rather than the connection setup:
/// the setup is a snapshot taken when the connection opened, so on a
/// long-running host it can describe a resolution the user has since changed.
/// Millimetres have no per-request equivalent, so they come from the setup and
/// are corrected proportionally if the pixel size has moved since.
fn current_size(
    conn: &RustConnection,
    root: Window,
    screen: &x11rb::protocol::xproto::Screen,
) -> Result<ScreenSize, String> {
    let geom = conn
        .get_geometry(root)
        .map_err(|e| format!("cannot read the desktop size: {e}"))?
        .reply()
        .map_err(|e| format!("cannot read the desktop size: {e}"))?;
    if geom.width == 0 || geom.height == 0 {
        return Err("the desktop has no size".to_owned());
    }
    Ok(ScreenSize {
        width: geom.width,
        height: geom.height,
        mm_width: scale_mm(u32::from(screen.width_in_millimeters), screen.width_in_pixels, geom.width),
        mm_height: scale_mm(
            u32::from(screen.height_in_millimeters),
            screen.height_in_pixels,
            geom.height,
        ),
    })
}

/// The framebuffer needed to hold `want_w` × `want_h` to the right of a
/// `cur_w` × `cur_h` desktop, or `None` if that does not fit in X's 16-bit
/// screen dimensions.
fn grown_size(cur_w: u16, cur_h: u16, want_w: u32, want_h: u32) -> Option<(u16, u16)> {
    let new_w = u32::from(cur_w).checked_add(want_w)?;
    let new_h = u32::from(cur_h).max(want_h);
    Some((u16::try_from(new_w).ok()?, u16::try_from(new_h).ok()?))
}

/// Rescale a physical size so the new framebuffer keeps the old pixel density.
///
/// ⚠️ Falls back to 96 dpi when the server reports no physical size at all —
/// Xvfb and several headless servers report 0 mm, and passing 0 through would
/// make every toolkit's DPI calculation divide by zero or report an absurd
/// density, which is how "the fonts went enormous when I connected my phone"
/// happens.
fn scale_mm(cur_mm: u32, cur_px: u16, new_px: u16) -> u32 {
    let new_px = u32::from(new_px);
    if cur_mm == 0 || cur_px == 0 {
        // 1 inch = 25.4 mm, kept in integers: px * 254 / (dpi * 10).
        return (new_px * 254) / (FALLBACK_DPI * 10);
    }
    ((u64::from(cur_mm) * u64::from(new_px)) / u64::from(cur_px)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_new_area_goes_to_the_right_and_the_desktop_keeps_its_height() {
        assert_eq!(grown_size(1920, 1080, 1179, 720), Some((3099, 1080)));
    }

    #[test]
    fn a_taller_client_makes_the_framebuffer_taller() {
        // A phone held in portrait is the normal case, not an edge one.
        assert_eq!(grown_size(1920, 1080, 1179, 2556), Some((3099, 2556)));
    }

    #[test]
    fn a_desktop_that_cannot_fit_the_extra_area_is_refused_not_wrapped() {
        // X screen dimensions are 16-bit. 60000 + 8000 does not fit, and the
        // wrong answer here would be a silently truncated 2464-pixel desktop.
        assert_eq!(grown_size(60000, 1080, 8000, 1000), None);
    }

    #[test]
    fn millimetres_keep_the_desktops_own_density() {
        // 1920 px across 508 mm is 96 dpi; 3099 px at the same density is 820 mm.
        assert_eq!(scale_mm(508, 1920, 3099), 819);
    }

    #[test]
    fn a_server_reporting_no_physical_size_gets_96_dpi_rather_than_zero() {
        assert_eq!(scale_mm(0, 1920, 1920), 508);
        assert_eq!(scale_mm(508, 0, 1920), 508);
    }
}
