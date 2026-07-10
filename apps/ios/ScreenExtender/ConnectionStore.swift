import Foundation
import UIKit

/// A remembered host shown on the connect screen for quick reconnect. `os` and
/// `hostname` are learned from the host's HostInfo after connecting. `mode` is
/// the last-chosen mode (empty = re-ask next time).
struct SavedConnection: Codable, Identifiable {
    var addr: String
    var hostname: String = ""
    var os: String = ""     // "windows" | "macos" | "linux" | ""
    var mode: String = ""   // Mode.rawValue, empty = show picker next time
    var pin: Int = 0
    var hidden: Bool = false
    var lastConnected: Double = 0
    /// User-given friendly name for this saved host. Shown as the main label with
    /// the hostname/address in brackets when set. Defaults to "" (use hostname).
    var customName: String = ""

    var id: String { addr }
}

/// Persists saved connections in `UserDefaults` as JSON, one entry per host:port.
enum ConnectionStore {
    private static let key = "savedConnections"
    private static let sensitivityKey = "trackpadSensitivity"
    private static let deviceNameKey = "deviceDisplayName"

    static func load() -> [SavedConnection] {
        guard let data = UserDefaults.standard.data(forKey: key),
              let list = try? JSONDecoder().decode([SavedConnection].self, from: data)
        else { return [] }
        return list
    }

    /// Insert or update the entry for `addr`, stamping it most-recently-used.
    /// Pass `mode: ""` to clear the remembered mode so the picker shows next time.
    static func remember(addr: String, mode: String = "", pin: Int = 0) {
        var list = load()
        var entry = list.first { $0.addr == addr } ?? SavedConnection(addr: addr)
        entry.mode = mode
        entry.pin = pin
        entry.hidden = false
        entry.lastConnected = Date().timeIntervalSince1970
        list.removeAll { $0.addr == addr }
        list.append(entry)
        save(list)
    }

    /// Record the host identity (OS + machine name) learned from HostInfo.
    static func setIdentity(addr: String, os: String, hostname: String) {
        var list = load()
        guard let i = list.firstIndex(where: { $0.addr == addr }) else { return }
        list[i].os = os
        list[i].hostname = hostname
        save(list)
    }

    static func setHidden(addr: String, _ hidden: Bool) {
        var list = load()
        guard let i = list.firstIndex(where: { $0.addr == addr }) else { return }
        list[i].hidden = hidden
        save(list)
    }

    /// Set (or clear, with "") the user's friendly name for a saved host.
    static func setCustomName(addr: String, _ name: String) {
        var list = load()
        guard let i = list.firstIndex(where: { $0.addr == addr }) else { return }
        list[i].customName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        save(list)
    }

    static func delete(addr: String) {
        save(load().filter { $0.addr != addr })
    }

    // MARK: - Trackpad sensitivity

    static func loadSensitivity() -> Float {
        let v = UserDefaults.standard.float(forKey: sensitivityKey)
        return v > 0 ? v : 1.0
    }

    static func saveSensitivity(_ value: Float) {
        UserDefaults.standard.set(value, forKey: sensitivityKey)
    }

    // MARK: - This device's name

    /// The user-set nickname for this device, or "" if never set. Empty means
    /// "use the system device name" — see `effectiveDeviceName()`.
    static func loadDeviceName() -> String {
        UserDefaults.standard.string(forKey: deviceNameKey) ?? ""
    }

    static func saveDeviceName(_ value: String) {
        UserDefaults.standard.set(value.trimmingCharacters(in: .whitespacesAndNewlines),
                                  forKey: deviceNameKey)
    }

    /// The name to advertise to a host: the user's nickname if set, else the
    /// system device name (e.g. "iPhone"; iOS 16+ hides the personalised name).
    static func effectiveDeviceName() -> String {
        let nickname = loadDeviceName()
        return nickname.isEmpty ? UIDevice.current.name : nickname
    }

    // MARK: - Private

    private static func save(_ list: [SavedConnection]) {
        if let data = try? JSONEncoder().encode(list) {
            UserDefaults.standard.set(data, forKey: key)
        }
    }
}

/// Emoji standing in for the host OS, for saved-connection rows. Hosts are always
/// desktop machines, so this is the OS identity (form factor is implicitly a PC).
func deviceEmoji(_ os: String) -> String {
    switch os.lowercased() {
    case "macos", "mac": return "🍎"
    case "windows": return "🪟"
    case "linux": return "🐧"
    default: return "🖥️"
    }
}

/// A friendly stand-in name for a saved host when neither a custom name nor a
/// machine name is known yet — derived from the host OS (e.g. "Windows device").
/// Used for the top line of a saved-host row so it never falls back to the IP.
func deviceFallback(_ os: String) -> String {
    switch os.lowercased() {
    case "windows":      return "Windows device"
    case "macos", "mac": return "Apple device"
    case "linux":        return "Linux device"
    case "ios":          return "iOS device"
    case "android":      return "Android device"
    default:             return "Saved host"
    }
}
