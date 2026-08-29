import SwiftUI

/// "About this app" — the answer every app in the Universal Suite gives, given
/// here.
///
/// The suite got this on 2026-08-29 as the SDK's `<AboutAppDialog>`, reached
/// through an Advanced section of the Actions menu. Nothing about that component
/// can be reused here — it is React — so what is shared is the **content**: what
/// the app does, what happens to your screen, that it is open source and where
/// the code is, which build you are on, and who to contact. The wording is kept
/// word-for-word with `crates/host-ui/src/lib.rs` (`about_panel`),
/// `apps/web/index.html` and the Android client's `AboutApp.kt`.
///
/// ⚠️ **The privacy section is deliberately NOT the suite's local-first line.**
/// "Your screen never leaves this computer" would be a lie about an app whose
/// entire purpose is putting a screen on another device. What is true, and what
/// this says, is where it goes and who can read it on the way — see
/// `crates/transport/src/lib.rs` for the Noise tunnel that makes the PIN
/// encryption rather than a gate.
struct AboutView: View {
    @Environment(\.dismiss) private var dismiss

    private static let repoURL = "https://github.com/universal-simulation-ltd/Universal_Screens"
    private static let issuesURL = "https://github.com/universal-simulation-ltd/Universal_Screens/issues"
    private static let changelogURL = "https://changelog.unisim.co.uk"
    private static let supportURL = "https://unisim.co.uk/#contact"

    /// ⚠️ Read from the bundle, not written out here. A hand-kept copy is a
    /// version that goes stale the first release nobody remembers to bump it.
    private var version: String? {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 6) {
                    section("What it does")
                    Text("Use this phone as a second screen, remote control or presentation clicker for your computer.")

                    section("Your screen")
                    Text("""
                    It goes to the device you paired with, and to nobody else. The connection is \
                    encrypted end to end with your PIN as the key, so neither your network nor \
                    the relay behind a remote code can read what is on it.
                    """)
                    Text("Nothing is uploaded to UNI·SIM, and there is no account.")
                        .font(.caption)
                        .foregroundStyle(.secondary)

                    section("Open source")
                    Text("Free and open source under the MIT licence — every line of it public, for anyone to read or run themselves.")
                    link("View the source ↗", Self.repoURL)
                    link("Report a problem ↗", Self.issuesURL)

                    if let version {
                        section("Version")
                        Text("v\(version)").fontWeight(.bold)
                        link("What's new ↗", Self.changelogURL)
                    }

                    section("Support")
                    Text("Questions, or something not working as it should?")
                    link("Contact us ↗", Self.supportURL)
                }
                .font(.subheadline)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(20)
            }
            .navigationTitle("Universal Screens")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }

    /// The small uppercase heading the other surfaces' About panels are built from.
    private func section(_ label: String) -> some View {
        Text(label.uppercased())
            .font(.caption2.weight(.bold))
            .kerning(0.8)
            .foregroundStyle(.secondary)
            .padding(.top, 10)
    }

    private func link(_ label: String, _ url: String) -> some View {
        Link(label, destination: URL(string: url)!)
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(Color.brandOrange)
    }
}
