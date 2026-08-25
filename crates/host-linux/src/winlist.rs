//! Enumerate and raise top-level windows through EWMH, so the clicker can
//! choose which window its keystrokes land in.
//!
//! The Windows twin (`host-windows/src/winlist.rs`) walks `EnumWindows` and
//! calls `SetForegroundWindow`. The X11 equivalent is not an API call but a
//! *convention*: the window manager publishes `_NET_CLIENT_LIST` on the root
//! window and listens for a `_NET_ACTIVE_WINDOW` client message. Both are part
//! of EWMH, which every mainstream WM implements (Mutter, KWin, Xfwm, Openbox,
//! i3, …).
//!
//! ⚠️ **This is the feature Wayland cannot have.** There is no protocol for one
//! client to enumerate another's windows, by design — see `docs/LINUX-HOST.md`
//! §3. So the picker is not "X11 first, Wayland later": on Wayland it degrades
//! permanently to an empty list, and the clicker drives whatever has focus.
//! [`list_windows`] returning empty is therefore a normal answer, not a failure.
//!
//! ⚠️ **A bare X server has no window manager and so no `_NET_CLIENT_LIST`** —
//! which is exactly what a container running Xvfb looks like. [`list_windows`]
//! falls back to walking the root's children and filtering the way a WM would,
//! so the picker still works under a minimal session, and so this module can be
//! tested at all.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, InputFocus, MapState,
    StackMode, Window,
};
use x11rb::rust_connection::RustConnection;
use x11rb::CURRENT_TIME;

/// One raisable window, as the protocol's `Message::WindowList` carries it.
type Listed = (i64, String);

/// Atoms looked up once per connection. X interns strings to ids, and doing that
/// per window would double the round trips for no reason.
struct Atoms {
    client_list: u32,
    client_list_stacking: u32,
    active_window: u32,
    wm_name: u32,
    utf8_string: u32,
    wm_window_type: u32,
    window_type_normal: u32,
    wm_state: u32,
}

impl Atoms {
    fn intern(conn: &RustConnection) -> Option<Self> {
        let get = |name: &str| conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok().map(|r| r.atom);
        Some(Self {
            client_list: get("_NET_CLIENT_LIST")?,
            client_list_stacking: get("_NET_CLIENT_LIST_STACKING")?,
            active_window: get("_NET_ACTIVE_WINDOW")?,
            wm_name: get("_NET_WM_NAME")?,
            utf8_string: get("UTF8_STRING")?,
            wm_window_type: get("_NET_WM_WINDOW_TYPE")?,
            window_type_normal: get("_NET_WM_WINDOW_TYPE_NORMAL")?,
            wm_state: get("WM_STATE")?,
        })
    }
}

/// A connection plus its interned atoms.
struct Ewmh {
    conn: RustConnection,
    root: Window,
    atoms: Atoms,
}

impl Ewmh {
    fn open() -> Option<Self> {
        Self::open_display(None)
    }

    /// Connect to `display` (or `DISPLAY` when `None`).
    ///
    /// ⚠️ The parameter exists so tests can point at a server that cannot exist
    /// **without setting the `DISPLAY` environment variable**. `cargo test` runs
    /// tests as threads in one process, so a test that mutates the environment
    /// silently breaks every other test that reads it — which is exactly how the
    /// live-X-server tests in [`crate::x11_tests`] first "failed".
    fn open_display(display: Option<&str>) -> Option<Self> {
        let (conn, screen_num) = x11rb::connect(display).ok()?;
        let root = conn.setup().roots.get(screen_num)?.root;
        let atoms = Atoms::intern(&conn)?;
        Some(Self { conn, root, atoms })
    }

    /// Read a window-id-list property (`_NET_CLIENT_LIST` and friends).
    fn window_property(&self, window: Window, atom: u32) -> Option<Vec<Window>> {
        let reply = self
            .conn
            .get_property(false, window, atom, AtomEnum::WINDOW, 0, u32::MAX)
            .ok()?
            .reply()
            .ok()?;
        reply.value32().map(Iterator::collect)
    }

    /// A window's title: `_NET_WM_NAME` (UTF-8) first, then legacy `WM_NAME`.
    ///
    /// ⚠️ `WM_NAME` is Latin-1 by specification, not UTF-8, so decoding it as
    /// UTF-8 mangles any accented character. Each byte is a code point.
    fn title(&self, window: Window) -> Option<String> {
        let utf8 = self
            .conn
            .get_property(false, window, self.atoms.wm_name, self.atoms.utf8_string, 0, 1024)
            .ok()
            .and_then(|c| c.reply().ok())
            .filter(|r| !r.value.is_empty())
            .map(|r| String::from_utf8_lossy(&r.value).into_owned());
        if let Some(t) = utf8.filter(|t| !t.trim().is_empty()) {
            return Some(flatten(&t));
        }
        let legacy = self
            .conn
            .get_property(false, window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 1024)
            .ok()?
            .reply()
            .ok()?;
        if legacy.value.is_empty() {
            return None;
        }
        let t: String = legacy.value.iter().map(|&b| char::from(b)).collect();
        Some(flatten(&t)).filter(|t| !t.trim().is_empty())
    }

    /// True if the window is an ordinary application window a person would pick:
    /// mapped, and not a dock/menu/tooltip/splash. Mirrors the Windows twin's
    /// "visible, titled, not a tool window" filter.
    fn is_pickable(&self, window: Window) -> bool {
        let mapped = self
            .conn
            .get_window_attributes(window)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|a| a.map_state == MapState::VIEWABLE);
        if !mapped {
            return false;
        }
        // An explicit type is authoritative; no type at all means "normal".
        match self.window_property(window, self.atoms.wm_window_type) {
            Some(types) if !types.is_empty() => types.contains(&self.atoms.window_type_normal),
            _ => true,
        }
    }

    /// The client windows, preferring the WM's own list.
    ///
    /// Falls back to the root's children when no WM is running. ⚠️ That walk
    /// must skip *override-redirect* windows (menus, tooltips) and, where a WM
    /// does exist, prefer the child that carries `WM_STATE`, because a
    /// reparenting WM puts its own frame window in between and the frame is what
    /// `query_tree` returns.
    fn clients(&self) -> Vec<Window> {
        for atom in [self.atoms.client_list, self.atoms.client_list_stacking] {
            if let Some(list) = self.window_property(self.root, atom) {
                if !list.is_empty() {
                    return list;
                }
            }
        }
        let Some(tree) = self.conn.query_tree(self.root).ok().and_then(|c| c.reply().ok()) else {
            return Vec::new();
        };
        tree.children
            .into_iter()
            .filter(|&w| {
                self.conn
                    .get_window_attributes(w)
                    .ok()
                    .and_then(|c| c.reply().ok())
                    .is_some_and(|a| !a.override_redirect)
            })
            .map(|w| self.client_of(w).unwrap_or(w))
            .collect()
    }

    /// Given a possible WM frame, find the client window inside it — the one
    /// carrying `WM_STATE`. Returns `None` if this window is not a frame.
    fn client_of(&self, frame: Window) -> Option<Window> {
        let has_state = |w: Window| {
            self.conn
                .get_property(false, w, self.atoms.wm_state, AtomEnum::ANY, 0, 1)
                .ok()
                .and_then(|c| c.reply().ok())
                .is_some_and(|r| r.format != 0)
        };
        if has_state(frame) {
            return Some(frame);
        }
        let kids = self.conn.query_tree(frame).ok()?.reply().ok()?.children;
        kids.into_iter().find(|&k| has_state(k))
    }
}

/// Collapse tabs and newlines in a title to spaces, as the Windows twin does.
fn flatten(title: &str) -> String {
    title.replace(['\t', '\n', '\r'], " ").trim().to_owned()
}

/// List pickable top-level windows as `(id, title)`, where `id` is the X window
/// id echoed back to [`focus_window`].
///
/// An empty list is a normal answer: a Wayland session, a machine with no X
/// server, or genuinely no open windows. The caller sends it either way — the
/// client draws "no windows" and moves on, rather than waiting for a reply that
/// never comes.
pub fn list_windows() -> Vec<Listed> {
    let Some(x) = Ewmh::open() else {
        return Vec::new();
    };
    x.clients()
        .into_iter()
        .filter(|&w| x.is_pickable(w))
        .filter_map(|w| x.title(w).map(|t| (i64::from(w), t)))
        .collect()
}

/// Raise the window with id `id` and give it the keyboard focus, so subsequent
/// injected keystrokes land in it.
///
/// ⚠️ Asking the window manager via a `_NET_ACTIVE_WINDOW` client message is the
/// correct route and the only one that respects focus-stealing prevention — a
/// bare `set_input_focus` leaves the window unraised and behind others under
/// most WMs. Both are sent: the message for a real desktop, the direct calls for
/// a bare X server that has no WM to receive it.
pub fn focus_window(id: i64) {
    let Some(x) = Ewmh::open() else {
        return;
    };
    let Ok(window) = u32::try_from(id) else {
        return;
    };

    // source indication 2 = "a pager", the value a WM is required to honour
    // without applying focus-stealing prevention to it.
    let event = ClientMessageEvent::new(32, window, x.atoms.active_window, [2, CURRENT_TIME, 0, 0, 0]);
    let _ = x.conn.send_event(
        false,
        x.root,
        EventMask::SUBSTRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_REDIRECT,
        event,
    );
    let _ = x.conn.configure_window(
        window,
        &x11rb::protocol::xproto::ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
    );
    let _ = x.conn.set_input_focus(InputFocus::PARENT, window, CURRENT_TIME);
    let _ = x.conn.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_flattened_and_trimmed() {
        assert_eq!(flatten("Slides\t— Impress\n"), "Slides — Impress");
        assert_eq!(flatten("  a\r\nb  "), "a  b");
    }

    /// With no X server reachable, opening must fail quietly rather than panic —
    /// this is the Wayland and headless path, where an empty window list is the
    /// permanent, correct answer.
    ///
    /// Display `:90210` has no socket on any machine. It is passed *in* rather
    /// than exported, for the reason on [`Ewmh::open_display`].
    #[test]
    fn an_unreachable_display_is_a_quiet_none_not_a_panic() {
        assert!(Ewmh::open_display(Some(":90210")).is_none());
    }
}
