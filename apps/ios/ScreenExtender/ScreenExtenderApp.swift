import SwiftUI

/// App entry point. Mirrors the Android app's structure: a connect screen, then
/// the clicker. Viewer / full-control (video) modes are stubbed for now.
@main
struct ScreenExtenderApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}
