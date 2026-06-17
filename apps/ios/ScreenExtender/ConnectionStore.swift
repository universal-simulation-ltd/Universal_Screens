import Foundation

/// A remembered host shown on the connect screen for quick reconnect. `os` is
/// learned from the host's HostInfo after connecting (empty until then).
struct SavedConnection: Codable, Identifiable {
    var addr: String
    var hostname: String = ""
    var os: String = "" // "windows" | "macos" | "linux" | ""
    var hidden: Bool = false
    var lastConnected: Double = 0

    var id: String { addr }
}

/// Persists saved connections in `UserDefaults` as JSON, one entry per host:port.
enum ConnectionStore {
    private static let key = "savedConnections"

    static func load() -> [SavedConnection] {
        guard let data = UserDefaults.standard.data(forKey: key),
              let list = try? JSONDecoder().decode([SavedConnection].self, from: data)
        else { return [] }
        return list
    }

    /// Insert or update the entry for `addr`, stamping it most-recently-used.
    static func remember(addr: String) {
        var list = load()
        var entry = list.first { $0.addr == addr } ?? SavedConnection(addr: addr)
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
