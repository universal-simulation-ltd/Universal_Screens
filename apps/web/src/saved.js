// Saved connections in localStorage — the browser parallel of the mobile apps'
// ConnectionStore (most-recent first, labelled by the host's name/OS once its
// HostInfo arrives). Used by the connect screen's "Saved connections" list.
const KEY = "universal-screens.saved";

/// All saved connections, most-recently-connected first.
export function load() {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "[]").sort((a, b) => b.lastConnected - a.lastConnected);
  } catch {
    return [];
  }
}

function write(list) {
  localStorage.setItem(KEY, JSON.stringify(list));
}

/// Record a connect to `addr` (creates or refreshes its entry).
export function touch(addr, now) {
  const list = load().filter((h) => h.addr !== addr);
  const prev = load().find((h) => h.addr === addr);
  list.push({ addr, hostname: prev?.hostname ?? "", os: prev?.os ?? "", lastConnected: now });
  write(list);
}

/// Attach the host's identity (from a HostInfo message) to a saved entry.
export function label(addr, hostname, os) {
  const list = load();
  const e = list.find((h) => h.addr === addr);
  if (e) { e.hostname = hostname; e.os = os; write(list); }
}

export function remove(addr) {
  write(load().filter((h) => h.addr !== addr));
}
