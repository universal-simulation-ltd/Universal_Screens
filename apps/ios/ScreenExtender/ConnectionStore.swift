import Foundation

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

    var id: String { addr }
}

/// Persists saved connections in `UserDefaults` as JSON, one entry per host:port.
enum ConnectionStore {
    private static let key = "savedConnections"
    private static let sensitivityKey = "trackpadSensitivity"

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

    // MARK: - Private

    private static func save(_ list: [SavedConnection]) {
        if let data = try? JSONEncoder().encode(list) {
            UserDefaults.standard.set(data, forKey: key)
        }
    }
}

/// SF Symbol name standing in for the host OS, for saved-connection rows.
func deviceSymbol(_ os: String) -> String {
    switch os.lowercased() {
    case "windows": return "pc"
    case "macos", "mac": return "desktopcomputer"
    case "linux": return "terminal"
    default: return "display"
    }
}
