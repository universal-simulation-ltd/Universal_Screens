// What the browser client's Universal ID does with the saved-machine list.
//
// The sign-in itself is GoTrue's and is not re-tested here; what IS worth
// pinning down is the merge, because it is the part that can lose somebody's
// list: a bad rule silently drops a machine, or resurrects one they deleted, or
// overwrites the nickname they just typed with an older one.
//
// Everything is driven through a stub `fetch`, so this runs offline and touches
// no account. Run: node apps/web/account.test.mjs
//
// ⚠️ The last test is the one to keep: **no PIN may ever appear in a request
// body.** The PIN is the Noise pre-shared key, `screens_saved_hosts` has no
// column for it (migration 0178), and the day someone adds `pin` to a saved
// entry this test is what stops it being uploaded.

import assert from 'node:assert/strict'

const SESSION = {
  access_token: 'access-token',
  refresh_token: 'refresh-token',
  expires_at: Date.now() + 3_600_000,
  email: 'someone@unisim.co.uk',
  user_id: 'user-1',
}

/** A localStorage that lives in a Map, plus a recording fetch. */
function harness({ signedIn = true, remote = [], sync = true } = {}) {
  const store = new Map()
  if (signedIn) store.set('universal-screens.session', JSON.stringify(SESSION))
  if (sync) store.set('universal-screens.sync-saved', '1')
  globalThis.localStorage = {
    getItem: (k) => (store.has(k) ? store.get(k) : null),
    setItem: (k, v) => store.set(k, String(v)),
    removeItem: (k) => store.delete(k),
  }
  const calls = []
  globalThis.fetch = async (url, init = {}) => {
    calls.push({ url: String(url), method: init.method ?? 'GET', body: init.body ? JSON.parse(init.body) : null, headers: init.headers ?? {} })
    if (String(url).includes('/rest/v1/screens_saved_hosts') && (init.method ?? 'GET') === 'GET') {
      return { ok: true, text: async () => JSON.stringify(remote) }
    }
    return { ok: true, text: async () => '[]' }
  }
  return { calls, store }
}

const iso = (ms) => new Date(ms).toISOString()
const results = []
const check = (name, fn) => {
  try {
    fn()
    results.push(true)
    console.log(`  ok   ${name}`)
  } catch (err) {
    results.push(false)
    console.log(` FAIL  ${name} — ${err.message}`)
  }
}

// A fresh import per case: the module memoises the session it booted with.
async function load(opts) {
  const h = harness(opts)
  const mod = await import(`./src/account.js?case=${results.length}-${Math.random()}`)
  return { ...h, account: mod }
}

console.log('\nsigned out')
{
  const { account, calls } = await load({ signedIn: false })
  const merged = await account.syncSaved([{ addr: '10.0.0.5:9002', lastConnected: 5 }])
  check('syncSaved answers null, so the caller keeps its local list', () => assert.equal(merged, null))
  check('…and nothing was sent', () => assert.equal(calls.length, 0))
  check('account() is null', () => assert.equal(account.account(), null))
}

console.log('\nsigned in — the merge')
{
  const local = [
    { addr: 'a:9002', hostname: 'Mac', os: 'macos', customName: 'Office Mac', lastConnected: 2000 },
    { addr: 'only-here:9002', hostname: '', os: '', customName: '', lastConnected: 1000 },
  ]
  const remote = [
    // Older than this device's copy: the local nickname must win.
    { addr: 'a:9002', hostname: 'Mac', os: 'macos', custom_name: 'Old name', last_connected: iso(1000) },
    // Newer than nothing here at all: must arrive.
    { addr: 'only-there:9002', hostname: 'Studio PC', os: 'windows', custom_name: '', last_connected: iso(3000) },
  ]
  const { account, calls } = await load({ remote })
  const merged = await account.syncSaved(local)
  const byAddr = Object.fromEntries(merged.map((m) => [m.addr, m]))

  check('every machine from both sides survives', () => assert.deepEqual(Object.keys(byAddr).sort(), ['a:9002', 'only-here:9002', 'only-there:9002']))
  check('the newer side wins per machine', () => assert.equal(byAddr['a:9002'].customName, 'Office Mac'))
  check("the account's own machine arrives with its labels", () => {
    assert.equal(byAddr['only-there:9002'].hostname, 'Studio PC')
    assert.equal(byAddr['only-there:9002'].os, 'windows')
  })
  check('most recently connected first', () => assert.deepEqual(merged.map((m) => m.addr), ['only-there:9002', 'a:9002', 'only-here:9002']))

  const push = calls.find((c) => c.method === 'POST' && c.url.includes('screens_saved_hosts'))
  check('only what the account was missing is written back', () => {
    assert.ok(push, 'no write happened')
    assert.deepEqual(push.body.map((r) => r.addr).sort(), ['a:9002', 'only-here:9002'])
  })
  check('…as an upsert on (user_id, addr), not a blind insert', () => {
    assert.match(push.url, /on_conflict=user_id,addr/)
    assert.equal(push.headers.Prefer, 'resolution=merge-duplicates')
  })
  check('…carrying the session token, not the anon key', () => assert.equal(push.headers.Authorization, 'Bearer access-token'))
}

console.log('\nsigned in — nothing to do')
{
  const remote = [{ addr: 'a:9002', hostname: '', os: '', custom_name: '', last_connected: iso(5000) }]
  const { account, calls } = await load({ remote })
  await account.syncSaved([{ addr: 'a:9002', lastConnected: 1000 }])
  check('an up-to-date account is not written to', () => assert.equal(calls.filter((c) => c.method === 'POST').length, 0))
}

console.log('\nthe PIN never leaves the device')
{
  const { account, calls } = await load({ remote: [] })
  // A saved entry that has (wrongly) picked up a pin along the way.
  await account.syncSaved([{ addr: 'a:9002', lastConnected: 1, pin: 4321 }])
  await account.remember({ addr: 'b:9002', lastConnected: 2, pin: 4321, customName: 'x' })
  const bodies = JSON.stringify(calls.filter((c) => c.method === 'POST').map((c) => c.body))
  check('no request body mentions a pin', () => assert.equal(/pin|4321/i.test(bodies), false, bodies))
}

console.log('\nforget')
{
  const { account, calls } = await load({ remote: [] })
  await account.forget('a b:9002')
  const del = calls.find((c) => c.method === 'DELETE')
  check('deletes that one machine, by address, escaped', () => {
    assert.ok(del, 'no delete sent')
    assert.match(del.url, /addr=eq\.a%20b%3A9002/)
  })
}

console.log('\nsync is off by default')
{
  const { account, calls } = await load({ remote: [], sync: false })
  check('syncEnabled() starts false', () => assert.equal(account.syncEnabled(), false))
  const merged = await account.syncSaved([{ addr: 'a:9002', lastConnected: 1 }])
  await account.remember({ addr: 'b:9002', lastConnected: 2 })
  await account.forget('a:9002')
  check('signed in but not opted in: syncSaved answers null', () => assert.equal(merged, null))
  check('…and nothing at all was sent', () => assert.equal(calls.length, 0))
}

console.log('\nturning sync off')
{
  const { account, calls, store } = await load({ remote: [] })
  const { deleted } = await account.setSyncEnabled(false)
  const del = calls.find((c) => c.method === 'DELETE')
  check('deletes every row for this account', () => {
    assert.ok(del, 'no delete sent')
    assert.match(del.url, /screens_saved_hosts\?user_id=eq\.user-1$/)
    assert.equal(del.headers.Authorization, 'Bearer access-token')
  })
  check('reports the delete as done', () => assert.equal(deleted, true))
  check('the choice is forgotten (off is the default)', () => assert.equal(store.has('universal-screens.sync-saved'), false))
  calls.length = 0
  await account.remember({ addr: 'b:9002', lastConnected: 2 })
  check('nothing is uploaded afterwards', () => assert.equal(calls.length, 0))
}

console.log('\nturning sync off while offline')
{
  const { account, store } = await load({ remote: [] })
  globalThis.fetch = async () => { throw new Error('offline') }
  const { deleted } = await account.setSyncEnabled(false)
  check('says the delete is still to do', () => assert.equal(deleted, false))
  check('…and keeps it pending for this account', () => assert.equal(store.get('universal-screens.sync-purge-pending'), 'user-1'))
  const calls = []
  globalThis.fetch = async (url, init = {}) => { calls.push({ url: String(url), method: init.method ?? 'GET' }); return { ok: true, text: async () => '[]' } }
  const merged = await account.syncSaved([{ addr: 'a:9002', lastConnected: 1 }])
  check('the next sync attempt finishes the delete and uploads nothing', () => {
    assert.equal(merged, null)
    assert.deepEqual(calls.map((c) => c.method), ['DELETE'])
    assert.equal(store.has('universal-screens.sync-purge-pending'), false)
  })
}

console.log('\nsigning out resets the choice')
{
  const { account, store } = await load({ remote: [] })
  await account.signOut()
  check('syncEnabled() is false again', () => assert.equal(account.syncEnabled(), false))
  check('…and not stored', () => assert.equal(store.has('universal-screens.sync-saved'), false))
}

const failed = results.filter((r) => !r).length
console.log(`\n${results.length - failed}/${results.length} passed`)
process.exit(failed === 0 ? 0 : 1)
