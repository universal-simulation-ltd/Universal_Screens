import SwiftUI

/// The Universal Apps bar, for the NATIVE iOS client.
///
/// Every other app in the suite is a Capacitor webview of its React site, so it
/// gets this bar from the SDK (`UniversalAppsNavBar.tsx`) for free. Screens is
/// the suite's only hand-written native mobile client, so it draws its own —
/// here in SwiftUI and in `SuiteBar.kt` on Android, which this mirrors.
///
/// ⚠️ **Match `UniversalAppsNavBar`, NOT the browser client's `.un-bar`.** The
/// first cut of the Android one copied `apps/web/index.html`, which is a
/// DIFFERENT and simpler bar — a one-line "Universal Screens", a "Universal
/// Apps ↗" pill and a GitHub button — because that page is the open-source
/// landing page with no app around it. The bar that belongs here is the app
/// one: a two-line lockup, the product word in the accent, a switcher chevron,
/// and the suite globe at the right-hand end.
///
/// ⚠️ These colours are the SUITE's, NOT the app's own palette in `Theme.swift`.
/// They are `packages/sdk/src/barTheme.ts`'s BAR table, so this bar matches the
/// one in Universal PDF rather than the app it is bolted to.
///
/// What is deliberately absent: the profile pill ("Bienvenue ▾" + avatar).
/// Screens has no account system, so there is nothing for it to show.
struct SuiteBar: View {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var dim = false

    /// Where the switcher chevron goes — the SDK's `portalHref` default.
    private static let portalURL = URL(string: "https://opensource.unisim.co.uk")!

    /// The globe at the right-hand end is the CHANGELOG, not a link to the
    /// company. Confirmed by tapping it in Universal PDF on a device: it opens
    /// the "Nouveautés" panel of suite releases. A native panel would mean
    /// shipping a changelog reader, so this opens the same feed's web view.
    private static let changelogURL = URL(string: "https://changelog.unisim.co.uk")!

    private var dark: Bool { scheme == .dark }

    // BAR.light / BAR.dark, verbatim.
    private var surface: Color { dark ? Color(hex: 0x0F172A) : .white }
    private var border: Color { dark ? Color(hex: 0x1E293B) : Color(hex: 0xE2E8F0) }
    private var muted: Color { dark ? Color(hex: 0x94A3B8) : Color(hex: 0x475569) }

    /// `claimAccent`.
    ///
    /// ⚠️ NOT one value for both themes. #c2410c is 5.18:1 on white but only
    /// 3.5:1 on the dark surface; orange-400 is 8.0:1 there. The rule inverts
    /// with the background, so darkening the orange to "fix" a dark bar makes
    /// it worse. The SDK carries this same warning on the same token.
    private var accent: Color { dark ? Color(hex: 0xFB923C) : Color(hex: 0xC2410C) }

    var body: some View {
        VStack(spacing: 0) {
            strip
            HStack(spacing: 0) {
                identity
                Spacer(minLength: 8)
                globe
            }
            // headerInnerStyle at the mobile breakpoint: 10x16 padding, 56 min.
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .frame(minHeight: 56)
            // ⚠️ No gap under the bar: this 1pt rule IS the separator.
            Rectangle().fill(border).frame(height: 1)
        }
        .background(surface)
    }

    /// The 4pt pulsing rule — `UniversalBar.tsx`: transparent → #e05504 →
    /// transparent, opacity 1 → .35 → 1 over 2400ms. Held static under Reduce
    /// Motion, which is the same call the Android side makes off
    /// `ANIMATOR_DURATION_SCALE`.
    private var strip: some View {
        LinearGradient(
            gradient: Gradient(colors: [.clear, Color(hex: 0xE05504), .clear]),
            startPoint: .leading,
            endPoint: .trailing
        )
        .frame(height: 4)
        .opacity(dim ? 0.35 : 1)
        .onAppear {
            guard !reduceMotion else { return }
            withAnimation(.easeInOut(duration: 1.2).repeatForever(autoreverses: true)) {
                dim = true
            }
        }
    }

    /// The mark, the two-line lockup and the switcher chevron.
    ///
    /// The lockup is `shortProductName`'s doing: "Universal Screens" →
    /// "UNIVERSAL" above "Screens", the shared word small and the product word
    /// large and in the accent. ⚠️ The eyebrow says UNIVERSAL, not UNI SIM —
    /// the owner corrected that on 2026-08-30. UNI·SIM is the company; the
    /// product is "Universal Screens".
    ///
    /// The full name goes back into the accessibility tree: the lockup is a
    /// visual abbreviation, and reading out "UNIVERSAL, Screens" as two strings
    /// would be reading the user a piece of layout.
    private var identity: some View {
        Button {
            UIApplication.shared.open(Self.portalURL)
        } label: {
            HStack(spacing: 8) {
                Image("AppLogo")
                    .resizable()
                    .frame(width: 24, height: 24)
                    .clipShape(RoundedRectangle(cornerRadius: 5, style: .continuous))
                VStack(alignment: .leading, spacing: 0) {
                    // suiteEyebrowStyle: 9 / 700 / 0.09em, uppercase, `muted`.
                    Text("UNIVERSAL")
                        .font(.system(size: 9, weight: .bold))
                        .tracking(0.81)
                        .foregroundColor(muted)
                    // productNameStyle at claimAccent: 15 / 600 / -0.01em.
                    Text("Screens")
                        .font(.system(size: 15, weight: .semibold))
                        .tracking(-0.15)
                        .foregroundColor(accent)
                        .lineLimit(1)
                }
                Image(systemName: "chevron.down")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundColor(muted)
            }
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Universal Screens — all the Universal Apps")
    }

    /// The suite globe at the right-hand end.
    ///
    /// ⚠️ The REAL artwork (`unisim-icon.png`, the same file the installer seals
    /// into its corner), NOT the monochrome glyph the SDK draws INSIDE the
    /// product name. Those are two different marks in the SDK for a documented
    /// reason — the small one has to be monochrome so it reads as a prefix to a
    /// word — and this is the full-colour one that ends the bar.
    private var globe: some View {
        Button {
            UIApplication.shared.open(Self.changelogURL)
        } label: {
            Image("SuiteGlobe")
                .resizable()
                .frame(width: 26, height: 26)
                .frame(width: 38, height: 38)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("What's new across the suite")
    }
}

extension Color {
    /// 0xRRGGBB, so the BAR table above can be pasted straight from the SDK.
    init(hex: UInt32) {
        self.init(
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255
        )
    }
}
