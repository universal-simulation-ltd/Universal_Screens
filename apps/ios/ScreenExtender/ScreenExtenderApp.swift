import SwiftUI
import UIKit

/// App entry point. Mirrors the Android app's structure: a connect screen, then
/// the clicker. Viewer / full-control (video) modes are stubbed for now.
@main
struct ScreenExtenderApp: App {
    /// Light unless the user picks otherwise under Advanced ▸ Appearance — see
    /// `Appearance` in Theme.swift for why the default is not "follow the system".
    @AppStorage(Appearance.storageKey) private var appearance: Appearance = .light

    var body: some Scene {
        WindowGroup {
            ContentView()
                .preferredColorScheme(appearance.colorScheme)
                // ⚠️ Belt and braces for "Match my device". Going from an explicit
                // scheme back to `.preferredColorScheme(nil)` has not always
                // released the window in SwiftUI — it could stay on the last
                // explicit scheme until relaunch. Setting the window's override
                // directly (which is what the modifier does underneath) makes
                // `.unspecified` stick, and sheets inherit it from the window.
                .onChange(of: appearance, initial: true) { _, chosen in
                    applyInterfaceStyle(chosen.interfaceStyle)
                }
        }
    }

    private func applyInterfaceStyle(_ style: UIUserInterfaceStyle) {
        for scene in UIApplication.shared.connectedScenes {
            guard let windowScene = scene as? UIWindowScene else { continue }
            for window in windowScene.windows { window.overrideUserInterfaceStyle = style }
        }
    }
}
