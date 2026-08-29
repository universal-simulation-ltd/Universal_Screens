//! Chrome and helpers shared by the desktop hosts' control windows.
//!
//! `host-windows/src/gui.rs` and `host-macos/src/gui.rs` grew as independent
//! copies of the same window — 1,598 and 1,451 lines, and drifting apart in
//! ways nobody chose. This crate holds the parts that were still character-for-
//! character identical, plus three that differed only in comments and rustfmt,
//! so there is one copy to fix instead of two.
//!
//! ⚠️ **What is deliberately NOT here.** `HostApp`, its `impl`, and its
//! `eframe::App` are the genuinely platform-divergent core (0.70 and 0.50
//! similarity) and stay in each host. So does `RecentConn`: macOS carries an
//! extra `name: Option<String>` that Windows does not, which is a real feature
//! difference, and unifying it is a behaviour decision rather than an
//! extraction. `best_lan_ip` differs completely (0.12) and is platform work.
//!
//! ⚠️ **`APP_VERSION` must never move into this crate.** It is
//! `env!("CARGO_PKG_VERSION")`, which resolves against the crate being
//! compiled — hoisting it here would silently make every host's About box
//! report *host-ui's* version instead of its own. It stays defined in each host.
//!
//! ⚠️ The Linux host is **not** a consumer of the *chrome*. `host-linux/src/gui.rs`
//! shares only ~28 lines with these two: it shipped a deliberately lean window
//! rather than a third copy of this one. Wiring that in would mean growing it to
//! match, which is the opposite of the point.
//!
//! It does take **one** thing, from 2026-08-29: [`about_panel`]. The distinction
//! is worth holding onto — layout may differ per platform, but a *claim about
//! the product* (where your screen goes, the licence, the version) must not, and
//! three hand-written About boxes is exactly how it would end up doing so.

use std::net::TcpListener;

use eframe::egui;
use extender_protocol::ClientPlatform;

/// Borrowed Wi-Fi credentials for [`connect_url`].
///
/// Each host has its own `wifi::WifiInfo` — same three fields, but different
/// inherent methods (macOS has `masked_password`, Windows has `qr_payload`) and
/// a different way of obtaining them. Rather than hoist that platform work into
/// this crate, `connect_url` takes the three values it actually reads and each
/// host converts at the call site.
#[derive(Clone, Copy, Debug)]
pub struct WifiQr<'a> {
    pub ssid: &'a str,
    /// `None` for an open network — the QR then carries no `pass=`.
    pub password: Option<&'a str>,
    /// QR auth tag: `"WPA"`, `"WEP"`, or `"nopass"`.
    pub auth: &'a str,
}

pub const BASE_PORT: u16 = 9000;

pub const BRAND: egui::Color32 = egui::Color32::from_rgb(0xe0, 0x55, 0x04);

pub const RECENT_MAX: usize = 8;

pub const OPENSOURCE_ROOT: &str = "https://opensource.unisim.co.uk";

pub const CHANGELOG_URL: &str = "https://changelog.unisim.co.uk";

pub const OPENSOURCE_URL: &str = "https://opensource.unisim.co.uk/screens";

/// The other apps in this one's own catalogue group ("Geeky") on
/// opensource.unisim.co.uk, as (name, path, blurb) so the menu reads like the web
/// suite switcher — name plus its one-line description — rather than bare links.
///
/// ⚠️ These replaced two hardcoded "(soon)" placeholders, one of which advertised
/// "Universal QR (soon)" while Universal QR had been live for months. Every path
/// here was checked for a 200 before it went in: an app menu that offers things
/// you cannot get is the same fault the download page was just cleaned of.
pub const SIBLING_APPS: &[(&str, &str, &str)] = &[
    ("Universal DIY", "diy", "Cut lists for simple butt-joint boxes"),
    (
        "Universal USB Detector",
        "usb",
        "Identify any USB device — version, speed & power",
    ),
    ("Universal Beam", "beam", "Send text straight between your devices"),
];

/// Recent user-visible changes, newest first — the `screens` entries from the
/// suite changelog at changelog.unisim.co.uk, trimmed to a line each.
///
/// ⚠️ A SNAPSHOT, and deliberately so: the host is a desktop binary with no
/// network fetch on the changelog path, so this cannot track the live feed the
/// way the SDK's ChangelogMenu does in the web apps. "See all" links out to the
/// real thing. It previously listed *features* ("Universal navbar with Actions &
/// Profile menus"), which is not what a "what's new" menu is for.
pub const CHANGELOG: &[&str] = &[
    "• Windows installer — per-user, no admin prompt",
    "• Encrypted connections over the LAN (Noise protocol)",
    "• Nearby hosts appear automatically — tap, enter PIN, connect",
    "• Click the connect QR to blow it up across the window",
    "• Cast to a browser screen — no install on the receiver",
];

pub fn pe(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn gen_pin() -> u32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    1000 + (nanos % 9000)
}

/// A short room code for cross-network remote access. Six chars from an
/// ambiguity-free alphabet (no 0/O, 1/I) so it's easy to read out over a call.
/// Seeded from the clock — collisions are harmless (the rendezvous just pairs
/// whoever shares a code), so no RNG dependency is pulled in.
pub fn gen_room_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // 32 chars, no 0/O/1/I
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0u64, |d| d.as_nanos() as u64)
        ^ (std::process::id() as u64).rotate_left(17);
    let mut code = String::with_capacity(6);
    for _ in 0..6 {
        // xorshift step — plenty for a non-secret, human-readable pairing code.
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        code.push(ALPHABET[(seed % 32) as usize] as char);
    }
    code
}

/// Truncate a label to `max` chars with an ellipsis, so a long machine name
/// doesn't overrun an orbit node.
pub fn truncate_label(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

pub fn platform_tag(p: ClientPlatform) -> &'static str {
    match p {
        ClientPlatform::Windows => "windows",
        ClientPlatform::Macos => "macos",
        ClientPlatform::Linux => "linux",
        ClientPlatform::Android => "android",
        ClientPlatform::Ios => "ios",
        ClientPlatform::Unknown => "unknown",
    }
}

pub fn platform_display(tag: &str) -> &str {
    match tag {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        "android" => "Android",
        "ios" => "iOS",
        _ => "Unknown device",
    }
}

pub fn sep_dot(ui: &mut egui::Ui, dark: bool) {
    let c = if dark {
        egui::Color32::from_rgb(0x47, 0x55, 0x69)
    } else {
        egui::Color32::from_rgb(0xcb, 0xd5, 0xe1)
    };
    ui.label(egui::RichText::new("·").color(c).size(16.0));
}

pub fn style_navbar(ui: &mut egui::Ui, dark: bool) {
    let link = if dark {
        egui::Color32::from_rgb(0xcb, 0xd5, 0xe1)
    } else {
        egui::Color32::from_rgb(0x37, 0x41, 0x51)
    };
    let hover = if dark {
        egui::Color32::from_rgb(0xf8, 0xfa, 0xfc)
    } else {
        egui::Color32::from_rgb(0x0f, 0x17, 0x2a)
    };
    let tint = egui::Color32::from_rgba_unmultiplied(
        BRAND.r(),
        BRAND.g(),
        BRAND.b(),
        if dark { 46 } else { 26 },
    );
    let round = egui::Rounding::same(8.0);

    let s = ui.style_mut();
    s.spacing.button_padding = egui::vec2(10.0, 6.0);
    s.spacing.item_spacing.x = 6.0;
    s.visuals.menu_rounding = egui::Rounding::same(10.0);
    if let Some(font) = s.text_styles.get_mut(&egui::TextStyle::Button) {
        font.size = 14.0;
    }

    let w = &mut s.visuals.widgets;
    for v in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        v.bg_stroke = egui::Stroke::NONE;
        v.rounding = round;
        v.expansion = 0.0;
    }
    w.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    w.inactive.bg_fill = egui::Color32::TRANSPARENT;
    w.inactive.fg_stroke.color = link;
    for v in [&mut w.hovered, &mut w.active, &mut w.open] {
        v.weak_bg_fill = tint;
        v.bg_fill = tint;
        v.fg_stroke.color = hover;
    }
}

/// The repository behind every surface of this app.
pub const REPO_URL: &str = "https://github.com/universal-simulation-ltd/Universal_Screens";
/// Where "Report a problem" goes.
pub const ISSUES_URL: &str = "https://github.com/universal-simulation-ltd/Universal_Screens/issues";
/// Where "Contact us" goes.
pub const SUPPORT_URL: &str = "https://unisim.co.uk/#contact";

/// "About this app" — the same answer the web apps give, given here.
///
/// Every other app in the suite got this on 2026-08-29 as the SDK's
/// `<AboutAppDialog>`, reached through an Advanced section of the Actions menu.
/// Universal Screens cannot use that component — the SDK is React and this is
/// egui — so the *content* is ported rather than the code: what the app does,
/// what happens to your screen, that it is open source and where the code is,
/// which build you are running, and who to contact.
///
/// ⚠️ **`version` is a parameter and must stay one.** See the crate note: the
/// hosts' `APP_VERSION` is `env!("CARGO_PKG_VERSION")`, which resolves against
/// the crate being compiled. Reading it in here would make every host report
/// *host-ui's* version instead of its own.
///
/// ⚠️ **The privacy paragraph is deliberately not the suite's local-first
/// line.** "Your screen never leaves this computer" would be a lie about an app
/// whose entire purpose is putting your screen on another device. What is true,
/// and what this says, is that it goes to the device you paired with and to
/// nobody else — see `crates/transport/src/lib.rs` for the Noise tunnel that
/// makes the PIN encryption rather than a gate.
pub fn about_panel(ui: &mut egui::Ui, version: &str) {
    ui.set_max_width(340.0);
    ui.label(egui::RichText::new("Universal Screens").strong().size(15.0));
    ui.small("Use a phone or another computer as a second screen, remote control or clicker");
    ui.add_space(8.0);

    about_heading(ui, "Your screen");
    ui.label(
        "It goes to the device you paired with, and to nobody else. The connection \
         is encrypted end to end with your PIN as the key, so neither your network \
         nor the relay behind a remote code can read what is on it.",
    );
    ui.small("Full detail, including what is NOT locked down, is under the 🔒 button.");
    ui.add_space(8.0);

    about_heading(ui, "Open source");
    ui.label("Free and open source under the MIT licence — every line of it public.");
    ui.horizontal(|ui| {
        ui.hyperlink_to("View the source ↗", REPO_URL);
        ui.hyperlink_to("Report a problem ↗", ISSUES_URL);
    });
    ui.add_space(8.0);

    about_heading(ui, "Version");
    ui.label(egui::RichText::new(format!("v{version}")).strong());
    ui.hyperlink_to("What's new ↗", CHANGELOG_URL);
    ui.add_space(8.0);

    about_heading(ui, "Support");
    ui.label("Questions, or something not working as it should?");
    ui.hyperlink_to("Contact us ↗", SUPPORT_URL);
}

/// The small uppercase section label the About panel is built from — the same
/// rhythm as the web dialog's headings.
fn about_heading(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(10.5)
            .strong()
            .color(ui.visuals().weak_text_color()),
    );
}

pub fn paint_brand_strip(ctx: &egui::Context) {
    let screen = ctx.screen_rect();
    let rect =
        egui::Rect::from_min_max(screen.min, egui::pos2(screen.max.x, screen.min.y + 5.0));
    let t = ctx.input(|i| i.time);
    let pulse = 0.35 + 0.65 * (0.5 + 0.5 * (t * std::f64::consts::TAU / 2.4).sin());
    let alpha = (pulse * 255.0) as u8;
    let orange = egui::Color32::from_rgba_unmultiplied(BRAND.r(), BRAND.g(), BRAND.b(), alpha);
    let clear = egui::Color32::from_rgba_unmultiplied(BRAND.r(), BRAND.g(), BRAND.b(), 0);

    let (y0, y1) = (rect.top(), rect.bottom());
    let (xl, xc, xr) = (rect.left(), rect.center().x, rect.right());
    let v = |x: f32, y: f32, c: egui::Color32| egui::epaint::Vertex {
        pos: egui::pos2(x, y),
        uv: egui::epaint::WHITE_UV,
        color: c,
    };
    let mut mesh = egui::Mesh::default();
    mesh.vertices.extend([
        v(xl, y0, clear),
        v(xl, y1, clear),
        v(xc, y0, orange),
        v(xc, y1, orange),
        v(xr, y0, clear),
        v(xr, y1, clear),
    ]);
    mesh.indices.extend([0, 1, 2, 2, 1, 3, 2, 3, 4, 4, 3, 5]);

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("brand_strip"),
    ));
    painter.add(egui::Shape::mesh(mesh));
    ctx.request_repaint();
}

/// Draw the "Nearby" hosts as an orbit: this machine at the centre with a soft
/// glow and dashed ring, each discovered peer a node circling it (portal-style).
/// The nodes rotate slowly; hovering the area pauses them so a node is easy to
/// click. `centre_label` names the local machine ("This PC" / "This Mac").
/// Returns the peer whose node was clicked this frame, if any.
pub fn nearby_orbit(
    ui: &mut egui::Ui,
    peers: &[extender_discovery::DiscoveredPeer],
    centre_label: &str,
) -> Option<extender_discovery::DiscoveredPeer> {
    let width = ui.available_width();
    let height = 210.0_f32;
    let (rect, _resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let centre = rect.center();
    let radius = (height * 0.34).min(width * 0.3);

    let dark = ui.visuals().dark_mode;
    let ink = if dark { egui::Color32::from_gray(220) } else { egui::Color32::from_gray(40) };
    let muted = if dark { egui::Color32::from_gray(130) } else { egui::Color32::from_gray(140) };
    let card = ui.visuals().extreme_bg_color;

    // Dashed orbit ring.
    let ring_segments = 44;
    for i in 0..ring_segments {
        if i % 2 != 0 {
            continue; // gaps make the dashes
        }
        let a0 = std::f32::consts::TAU * (i as f32) / (ring_segments as f32);
        let a1 = std::f32::consts::TAU * (i as f32 + 1.0) / (ring_segments as f32);
        painter.line_segment(
            [
                centre + radius * egui::vec2(a0.cos(), a0.sin()),
                centre + radius * egui::vec2(a1.cos(), a1.sin()),
            ],
            egui::Stroke::new(1.2, muted.gamma_multiply(0.5)),
        );
    }

    // Pulsing glow behind the centre.
    let t = ui.input(|i| i.time) as f32;
    let pulse = 0.5 + 0.5 * (t * 1.6).sin();
    let glow = egui::Color32::from_rgba_unmultiplied(BRAND.r(), BRAND.g(), BRAND.b(), (26.0 + 20.0 * pulse) as u8);
    painter.circle_filled(centre, 34.0 + 5.0 * pulse, glow);

    // Centre node: this machine.
    painter.circle_filled(centre, 26.0, card);
    painter.circle_stroke(centre, 26.0, egui::Stroke::new(1.5, BRAND));
    painter.text(centre - egui::vec2(0.0, 4.0), egui::Align2::CENTER_CENTER, "🖥", egui::FontId::proportional(20.0), ink);
    painter.text(centre + egui::vec2(0.0, 15.0), egui::Align2::CENTER_CENTER, centre_label, egui::FontId::proportional(9.0), muted);

    // Orbiting peer nodes. A slow global rotation, evenly spread; pause on hover
    // so a moving node stays clickable.
    let hovered_area = ui.rect_contains_pointer(rect);
    let spin = if hovered_area { 0.0 } else { t * 0.35 }; // radians
    let mut clicked = None;
    let node_r = 22.0;

    for (i, peer) in peers.iter().enumerate() {
        let angle = spin + std::f32::consts::TAU * (i as f32) / (peers.len() as f32) - std::f32::consts::FRAC_PI_2;
        let pos = centre + radius * egui::vec2(angle.cos(), angle.sin());
        let node_rect = egui::Rect::from_center_size(pos, egui::vec2(node_r * 2.0, node_r * 2.0));
        let id = ui.id().with(("orbit_peer", i));
        let resp = ui.interact(node_rect, id, egui::Sense::click());
        let hot = resp.hovered();
        if hot {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        painter.circle_filled(pos, node_r, card);
        painter.circle_stroke(pos, node_r, egui::Stroke::new(if hot { 2.0 } else { 1.2 }, if hot { BRAND } else { muted }));
        painter.text(pos - egui::vec2(0.0, 3.0), egui::Align2::CENTER_CENTER, "📡", egui::FontId::proportional(16.0), ink);

        // Label pill under the node — the name, plus the address on hover.
        let name = truncate_label(&peer.name, 16);
        painter.text(
            pos + egui::vec2(0.0, node_r + 9.0),
            egui::Align2::CENTER_CENTER,
            &name,
            egui::FontId::proportional(11.0),
            ink,
        );
        if hot {
            painter.text(
                pos + egui::vec2(0.0, node_r + 22.0),
                egui::Align2::CENTER_CENTER,
                format!("{}:{}  ·  click to connect", peer.addr, peer.port),
                egui::FontId::proportional(9.5),
                muted,
            );
            resp.clone().on_hover_text(format!("Connect to {} ({}:{})", peer.name, peer.addr, peer.port));
        }
        if resp.clicked() {
            clicked = Some(peer.clone());
        }
    }

    // Keep the animation going while nothing else is repainting.
    if !hovered_area {
        ui.ctx().request_repaint();
    }
    clicked
}

pub fn connect_url(host: &str, pin: u32, wifi: Option<WifiQr<'_>>) -> String {
    let (ip, port) = host.rsplit_once(':').unwrap_or((host, "9000"));
    let mut s = format!(
        "https://opensource.unisim.co.uk/screens/connect?host={}&port={}&pin={:04}",
        pe(ip),
        pe(port),
        pin,
    );
    if let Some(wifi) = wifi {
        s.push_str("#ssid=");
        s.push_str(&pe(wifi.ssid));
        s.push_str("&auth=");
        s.push_str(&pe(wifi.auth));
        if let Some(p) = wifi.password {
            s.push_str("&pass=");
            s.push_str(&pe(p));
        }
    }
    s
}

#[derive(Clone, Copy)]
pub enum DeviceKind {
    Windows,
    Mac,
    Android,
    Ios,
    Laptop,
    Other,
}

impl DeviceKind {
    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "windows" => Self::Windows,
            "macos" => Self::Mac,
            "android" => Self::Android,
            "ios" => Self::Ios,
            _ => Self::Other,
        }
    }
}

/// Draw a small monochrome device glyph inline in the current layout. Returns the
/// (clickable) response so callers can make it interactive.
pub fn device_icon(ui: &mut egui::Ui, kind: DeviceKind, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    let p = ui.painter();
    let color = egui::Color32::from_rgb(55, 55, 70);
    let stroke = egui::Stroke::new((size * 0.07).max(1.2), color);
    let at = |fx: f32, fy: f32| rect.min + egui::vec2(fx * size, fy * size);
    let r = |fx: f32, fy: f32, fw: f32, fh: f32| {
        egui::Rect::from_min_size(at(fx, fy), egui::vec2(fw * size, fh * size))
    };

    match kind {
        DeviceKind::Windows => {
            let gap = 0.10;
            let cell = (1.0 - gap) / 2.0;
            for (cx, cy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                p.rect_filled(
                    r(cx * (cell + gap), cy * (cell + gap), cell, cell),
                    1.0,
                    color,
                );
            }
        }
        DeviceKind::Laptop => {
            p.rect_stroke(r(0.15, 0.14, 0.70, 0.50), 1.0, stroke);
            p.rect_filled(r(0.06, 0.66, 0.88, 0.10), 2.0, color); // base bar
        }
        DeviceKind::Mac => {
            p.rect_stroke(r(0.12, 0.10, 0.76, 0.52), 2.0, stroke); // monitor
            p.rect_filled(r(0.45, 0.62, 0.10, 0.12), 0.0, color); // neck
            p.rect_filled(r(0.30, 0.74, 0.40, 0.06), 1.0, color); // foot
        }
        DeviceKind::Android => {
            p.line_segment([at(0.33, 0.12), at(0.40, 0.27)], stroke); // antennae
            p.line_segment([at(0.67, 0.12), at(0.60, 0.27)], stroke);
            p.rect_filled(
                r(0.25, 0.27, 0.50, 0.46),
                egui::Rounding { nw: size * 0.22, ne: size * 0.22, sw: 0.0, se: 0.0 },
                color,
            );
            p.circle_filled(at(0.40, 0.41), size * 0.035, egui::Color32::WHITE); // eyes
            p.circle_filled(at(0.60, 0.41), size * 0.035, egui::Color32::WHITE);
        }
        DeviceKind::Ios => {
            p.rect_stroke(r(0.30, 0.10, 0.40, 0.80), size * 0.12, stroke); // phone
            p.line_segment([at(0.43, 0.82), at(0.57, 0.82)], stroke); // home bar
        }
        DeviceKind::Other => {
            p.rect_stroke(r(0.15, 0.18, 0.70, 0.52), 2.0, stroke); // generic monitor
            p.rect_filled(r(0.38, 0.74, 0.24, 0.06), 1.0, color);
        }
    }
    response
}

/// Bind the first free port at or after `start`, so the host "just works" even
/// when the default port is taken. Returns the bound listener and its port.
pub fn first_free_port(start: u16) -> Option<(TcpListener, u16)> {
    (start..start.saturating_add(50))
        .find_map(|port| TcpListener::bind(("0.0.0.0", port)).ok().map(|l| (l, port)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pe_encodes_reserved_keeps_unreserved() {
        assert_eq!(pe("a b/c?d=e&f"), "a%20b%2Fc%3Fd%3De%26f");
        assert_eq!(pe("Safe-1._~"), "Safe-1._~");
    }

    #[test]
    fn gen_room_code_is_six_unambiguous_chars() {
        const ALPHABET: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        for _ in 0..200 {
            let code = gen_room_code();
            assert_eq!(code.chars().count(), 6, "code {code} not 6 chars");
            for c in code.chars() {
                assert!(ALPHABET.contains(c), "code {code} has ambiguous/invalid char {c}");
            }
        }
    }

    #[test]
    fn truncate_label_keeps_short_ellipsizes_long() {
        assert_eq!(truncate_label("DESKTOP-1", 16), "DESKTOP-1");
        assert_eq!(truncate_label("exactly-sixteen!", 16), "exactly-sixteen!");
        assert_eq!(truncate_label("a-very-long-machine-name", 16), "a-very-long-mac…");
        // Counts chars, not bytes (a multibyte name isn't split mid-codepoint).
        assert_eq!(truncate_label("café-münchen-server", 8), "café-mü…");
    }

    #[test]
    fn connect_url_host_in_query_wifi_in_fragment() {
        let w = WifiQr { ssid: "My Net", password: Some("p@ss"), auth: "WPA" };
        let p = connect_url("10.0.0.5:9100", 1234, Some(w));
        assert!(p.starts_with("https://opensource.unisim.co.uk/screens/connect?"), "{p}");
        // Host + PIN are server-visible (in the query, before any '#').
        let (query, frag) = p.split_once('#').expect("a fragment carrying the Wi-Fi creds");
        assert!(query.contains("host=10.0.0.5"), "{p}");
        assert!(query.contains("port=9100"), "{p}");
        assert!(query.contains("pin=1234"), "{p}");
        // The Wi-Fi password must NOT be in the server-visible query.
        assert!(!query.contains("ssid="), "ssid leaked into the query: {p}");
        assert!(!query.contains("pass="), "Wi-Fi password leaked into the query: {p}");
        // It rides in the fragment instead (kept client-side by browsers).
        assert!(frag.contains("ssid=My%20Net"), "{p}");
        assert!(frag.contains("pass=p%40ss"), "{p}");
        assert!(frag.contains("auth=WPA"), "{p}");
    }

    #[test]
    fn connect_url_open_network_omits_pass() {
        let w = WifiQr { ssid: "Cafe", password: None, auth: "nopass" };
        let p = connect_url("192.168.0.2:9000", 7, Some(w));
        assert!(p.contains("pin=0007"), "{p}");
        assert!(p.contains("#ssid=Cafe"), "{p}");
        assert!(!p.contains("pass="), "{p}");
    }

    #[test]
    fn connect_url_without_wifi_is_query_only() {
        let p = connect_url("192.168.0.2:9000", 42, None);
        assert_eq!(
            p,
            "https://opensource.unisim.co.uk/screens/connect?host=192.168.0.2&port=9000&pin=0042"
        );
        assert!(!p.contains('#'), "no Wi-Fi → no fragment: {p}");
    }

    /// `from_tag` never yields `Laptop` — it is a glyph the Windows host picks
    /// directly for "this PC". Pinned so the variant is not mistaken for a
    /// platform tag and "helpfully" wired into the match.
    #[test]
    fn from_tag_maps_known_platforms_and_never_laptop() {
        assert!(matches!(DeviceKind::from_tag("windows"), DeviceKind::Windows));
        assert!(matches!(DeviceKind::from_tag("macos"), DeviceKind::Mac));
        assert!(matches!(DeviceKind::from_tag("android"), DeviceKind::Android));
        assert!(matches!(DeviceKind::from_tag("ios"), DeviceKind::Ios));
        assert!(matches!(DeviceKind::from_tag("linux"), DeviceKind::Other));
        assert!(matches!(DeviceKind::from_tag(""), DeviceKind::Other));
    }
}
