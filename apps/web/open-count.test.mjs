// The browser client's anonymous daily open count (src/openCount.js): what
// counts, what never does, and that exactly one request with no identifier is
// sent. No network, no browser — a fake window and a fake fetch.
//
// Run: node apps/web/open-count.test.mjs

import { OPENED_KEY, OWN_DEVICE_KEY, countDailyOpen, isDevHost, isOwnTraffic, startOpenCount, utcDay } from "./src/openCount.js";

let failures = 0;
const A = (cond, msg) => {
  console.log((cond ? "PASS" : "FAIL") + ": " + msg);
  if (!cond) failures++;
};

function fakeStorage() {
  const m = new Map();
  return {
    getItem: (k) => (m.has(k) ? m.get(k) : null),
    setItem: (k, v) => { m.set(k, String(v)); },
    removeItem: (k) => { m.delete(k); },
    get size() { return m.size; },
    keys: () => [...m.keys()],
  };
}

function fakeWindow({ host = "opensource.unisim.co.uk", webdriver = false, storage = fakeStorage() } = {}) {
  const listeners = new Map();
  const doc = { visibilityState: "visible", addEventListener() {}, removeEventListener() {} };
  const win = {
    navigator: { webdriver },
    location: { hostname: host },
    document: doc,
    addEventListener: (t, f) => { listeners.set(t, f); },
    removeEventListener: (t) => { listeners.delete(t); },
    fire: (t) => { const f = listeners.get(t); if (f) f(); },
    listeners,
  };
  Object.defineProperty(win, "localStorage", {
    get() { if (storage === "throw") throw new Error("blocked"); return storage; },
  });
  return win;
}

function fakeFetch(ok = true) {
  const calls = [];
  const fn = async (url, init) => { calls.push({ url, init }); return { ok }; };
  fn.calls = calls;
  return fn;
}

// Dev hosts: the SDK's rules.
for (const h of ["localhost", "app.localhost", "screens.local", "127.0.0.1", "10.0.0.5", "192.168.1.20", "172.16.0.1", "172.31.9.9", "[::1]", "0.0.0.0"]) {
  A(isDevHost(h), `${h} is a dev host`);
}
for (const h of ["opensource.unisim.co.uk", "172.32.0.1", "8.8.8.8", "example.com"]) {
  A(!isDevHost(h), `${h} is not a dev host`);
}

// Own traffic.
A(isOwnTraffic(fakeWindow({ webdriver: true })), "automation is our own traffic");
{
  const s = fakeStorage(); s.setItem(OWN_DEVICE_KEY, "1");
  A(isOwnTraffic(fakeWindow({ storage: s })), "a staff browser is our own traffic");
}
A(isOwnTraffic(fakeWindow({ host: "192.168.1.20" })), "a LAN copy is our own traffic");
A(!isOwnTraffic(fakeWindow()), "the live site is not");

// One count a day, with nothing but product + platform.
{
  const win = fakeWindow();
  const f = fakeFetch();
  A(await countDailyOpen(win, f) === true, "first open of the day is counted");
  A(f.calls.length === 1, "exactly one request");
  const { url, init } = f.calls[0];
  A(url.endsWith("/rest/v1/rpc/app_open_count"), "it calls app_open_count");
  A(JSON.stringify(JSON.parse(init.body)) === JSON.stringify({ p_product: "screens", p_platform: "web" }), "body is product + 'web', nothing else");
  A(init.headers.Authorization === "Bearer " + init.headers.apikey, "always the anon key, never a session");
  A(win.localStorage.getItem(OPENED_KEY) === utcDay(), "today's date is remembered");
  A(win.localStorage.keys().length === 1, "and nothing else is stored");
  A(await countDailyOpen(win, f) === false && f.calls.length === 1, "a second open the same day is not");
}
{
  const s = fakeStorage(); s.setItem(OPENED_KEY, "2000-01-01");
  const f = fakeFetch();
  A(await countDailyOpen(fakeWindow({ storage: s }), f) === true, "a new day counts again");
}
{
  const win = fakeWindow();
  const f = fakeFetch(false);
  A(await countDailyOpen(win, f) === false, "a failed call reports false");
  A(win.localStorage.getItem(OPENED_KEY) === null, "and is not remembered, so it is retried");
}
{
  const f = fakeFetch();
  A(await countDailyOpen(fakeWindow({ storage: "throw" }), f) === false && f.calls.length === 0, "no storage: not counted at all");
  A(await countDailyOpen(fakeWindow({ webdriver: true }), f) === false && f.calls.length === 0, "automation: nothing sent");
  A(await countDailyOpen(fakeWindow({ host: "localhost" }), f) === false && f.calls.length === 0, "localhost: nothing sent");
  const throwing = async () => { throw new Error("offline"); };
  A(await countDailyOpen(fakeWindow(), throwing) === false, "offline: never throws");
}

// The interaction gate.
{
  const win = fakeWindow();
  const realFetch = globalThis.fetch;
  const f = fakeFetch();
  globalThis.fetch = f;
  try {
    startOpenCount(win);
    await new Promise((r) => setTimeout(r, 10));
    A(f.calls.length === 0, "a load alone sends nothing");
    A(["pointerdown", "keydown", "touchstart", "wheel", "scroll"].every((t) => win.listeners.has(t)), "waits on press, key, touch, wheel and scroll");
    win.fire("scroll");
    await new Promise((r) => setTimeout(r, 10));
    A(f.calls.length === 1, "the first interaction counts");
    A(win.listeners.size === 0, "and the listeners are gone");
  } finally {
    globalThis.fetch = realFetch;
  }
  const own = fakeWindow({ webdriver: true });
  startOpenCount(own);
  A(own.listeners.size === 0, "our own traffic never even listens");
}

console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
