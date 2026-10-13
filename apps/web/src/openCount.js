// The suite's anonymous daily open count, for the browser client only.
//
// Every web app in the suite adds one to "how many times was this app opened
// today" — @unisim/sdk's `countDailyOpen` / `startOpenCount`
// (universal-platform packages/sdk/src/presence.ts). Screens is not React and
// has no bundler, so it cannot import the SDK; this is that module's web path
// written out against Supabase's REST endpoint, and it follows the same rules:
//
//   * ONE call, `app_open_count {p_product:'screens', p_platform:'web'}`, the
//     first time the page is used on a given UTC day in this browser. Nothing
//     else is sent: no id, no account (always the anon key, even when signed
//     in), no host, PIN, room code or anything about what is on screen. The
//     server keeps a per-day total and nothing that identifies anyone.
//   * "Already counted today" is `localStorage['unisim:opened:screens']` = the
//     date. It stays in this browser and is never sent. A browser that cannot
//     remember (storage blocked) is not counted at all, rather than once per
//     load.
//   * It waits for the first real interaction (press, key, touch, wheel,
//     scroll): a page load on its own is not an open.
//   * Our own traffic is left out: a browser under automation
//     (`navigator.webdriver`), a browser marked as a staff device
//     (`localStorage['unisim:own-device'] === '1'`, set by a platform
//     administrator's sign-in or by opensource.unisim.co.uk/staff-device), and
//     any dev or LAN host — which also means the copy a host or `serve.mjs`
//     serves on this machine or the local network is never counted. In
//     practice only https://opensource.unisim.co.uk/screens/app/ counts.
//   * Never throws. Offline, or on a LAN with no route out, it simply fails
//     quietly and tries again on a later visit.
//
// ⚠️ Browser client only. The Android and iPhone apps and the desktop hosts do
// not count anything (suite rule: installed phone/desktop builds of the
// Universal Apps send no usage figures).
//
// The anon key is a PUBLISHABLE key: it ships in every suite web bundle by
// design, and Row-Level Security is the real boundary.

const SUPABASE_URL = "https://rygfxgalojojppxmhddo.supabase.co";
const SUPABASE_ANON =
  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6InJ5Z2Z4Z2Fsb2pvanBweG1oZGRvIiwicm9sZSI6ImFub24iLCJpYXQiOjE3Nzg3NTY4MjUsImV4cCI6MjA5NDMzMjgyNX0.hLy_vt9vY_rdPKF3nL32yAuMCD604E3CH5VM7D7CaNE";

const PRODUCT = "screens";
/** Same prefix as the SDK's OPENED_KEY_PREFIX. Holds the date only. */
export const OPENED_KEY = "unisim:opened:" + PRODUCT;
/** Same key as the SDK's OWN_DEVICE_KEY. */
export const OWN_DEVICE_KEY = "unisim:own-device";
/** Same list as the SDK's INTERACTION_EVENTS. */
export const INTERACTION_EVENTS = ["pointerdown", "keydown", "touchstart", "wheel", "scroll"];

/** Today's date in UTC, `YYYY-MM-DD`. */
export function utcDay(now = new Date()) {
  return now.toISOString().slice(0, 10);
}

/** A dev server or a LAN address — the SDK's isDevHost, rule for rule. */
export function isDevHost(host) {
  const h = String(host || "").toLowerCase().replace(/^\[|\]$/g, "");
  if (h === "localhost" || h.endsWith(".localhost") || h.endsWith(".local")) return true;
  if (h === "::1" || h === "0.0.0.0") return true;
  const ip = /^(\d+)\.(\d+)\.\d+\.\d+$/.exec(h);
  if (!ip) return false;
  const a = Number(ip[1]), b = Number(ip[2]);
  return a === 127 || a === 10 || (a === 192 && b === 168) || (a === 172 && b >= 16 && b <= 31);
}

function storage(win) {
  try { return win.localStorage ?? null; } catch { return null; }
}

/** True when this page is us (automation, a staff browser, a dev/LAN host). */
export function isOwnTraffic(win = globalThis) {
  try {
    if (win.navigator?.webdriver === true) return true;
    const ls = storage(win);
    if (ls && ls.getItem(OWN_DEVICE_KEY) === "1") return true;
    return isDevHost(win.location?.hostname ?? "");
  } catch {
    return true;
  }
}

let counting = false;

/**
 * Add today's open, once per UTC day in this browser. Resolves whether a count
 * was sent. Never throws.
 */
export async function countDailyOpen(win = globalThis, fetchFn = globalThis.fetch) {
  try {
    if (typeof fetchFn !== "function" || isOwnTraffic(win)) return false;
    const ls = storage(win);
    if (!ls) return false;
    const today = utcDay();
    if (ls.getItem(OPENED_KEY) === today) return false;
    if (counting) return false;
    counting = true;
    try {
      const res = await fetchFn(`${SUPABASE_URL}/rest/v1/rpc/app_open_count`, {
        method: "POST",
        headers: {
          "apikey": SUPABASE_ANON,
          "Authorization": `Bearer ${SUPABASE_ANON}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ p_product: PRODUCT, p_platform: "web" }),
        // Nothing of this page's address rides along with the count.
        referrerPolicy: "no-referrer",
        credentials: "omit",
      });
      if (!res || !res.ok) return false;
      try { ls.setItem(OPENED_KEY, today); } catch { /* ignore */ }
      return true;
    } finally {
      counting = false;
    }
  } catch {
    return false;
  }
}

/**
 * Count today's open at the first interaction, and again if the page is still
 * open (or brought back) after midnight UTC. Returns a stop function.
 */
export function startOpenCount(win = globalThis) {
  try {
    if (!win.addEventListener || isOwnTraffic(win)) return () => {};
  } catch {
    return () => {};
  }
  const run = () => { void countDailyOpen(win); };
  const onVisible = () => { if (win.document?.visibilityState !== "hidden") run(); };
  let started = false;
  const opts = { capture: true, passive: true };
  const disarm = () => {
    for (const type of INTERACTION_EVENTS) win.removeEventListener(type, begin, true);
  };
  function begin() {
    if (started) return;
    started = true;
    disarm();
    run();
    win.document?.addEventListener("visibilitychange", onVisible);
  }
  for (const type of INTERACTION_EVENTS) win.addEventListener(type, begin, opts);
  return () => {
    disarm();
    win.document?.removeEventListener("visibilitychange", onVisible);
  };
}
