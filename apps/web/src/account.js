// A Universal ID in the browser client — sign in, stay signed in, and let the
// saved-machine list follow the account (James, 2026-09-17: "go ahead with
// screens account", choosing "Identity + saved hosts follow you").
//
// Screens has no `@unisim/sdk` on any client and no bundler on this one, so
// this is GoTrue's REST API written out directly. The flow is the suite's
// email one-time code, LOGIN ONLY — a Universal ID is created on the hub
// (app.unisim.co.uk), never here, so a typo cannot mint a stray account.
//
//   POST /auth/v1/otp     { email, create_user: false }      → code emailed
//   POST /auth/v1/verify  { email, token, type: 'email' }    → session
//   POST /auth/v1/token?grant_type=refresh_token             → fresh session
//   POST /auth/v1/logout                                     → ends it server-side
//
// ⚠️ **Signing in is entirely optional and changes nothing about how a
// connection works.** Screens is a LAN tool that must keep working with no
// route out: everything here fails quietly, and a signed-out client behaves
// exactly as it always has.
//
// ⚠️ **The PIN is never involved.** It is the Noise pre-shared key, it has
// never been written to disk by any client, and `screens_saved_hosts`
// (migration 0178) has no column for it. Sync carries the address, the
// machine's name and OS, and your nickname for it — nothing else.
//
// The session is kept in localStorage under this app's own key, deliberately
// NOT the suite's shared one: this page is served over plain http:// from a
// host on a LAN, which is a different origin from every other suite app and
// cannot share their cookie anyway.

const SUPABASE_URL = 'https://rygfxgalojojppxmhddo.supabase.co'
const SUPABASE_ANON =
  'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6InJ5Z2Z4Z2Fsb2pvanBweG1oZGRvIiwicm9sZSI6ImFub24iLCJpYXQiOjE3Nzg3NTY4MjUsImV4cCI6MjA5NDMzMjgyNX0.hLy_vt9vY_rdPKF3nL32yAuMCD604E3CH5VM7D7CaNE'

const SESSION_KEY = 'universal-screens.session'

/** Read the stored session, or null. */
function stored() {
  try {
    const raw = JSON.parse(localStorage.getItem(SESSION_KEY) ?? 'null')
    return raw && raw.refresh_token ? raw : null
  } catch {
    return null
  }
}

function store(session) {
  try {
    if (session) localStorage.setItem(SESSION_KEY, JSON.stringify(session))
    else localStorage.removeItem(SESSION_KEY)
  } catch {
    /* private mode — the session then lasts as long as the page does */
  }
  memory = session
  for (const fn of listeners) fn(session)
}

let memory = stored()
const listeners = new Set()

/** Call `fn` whenever the signed-in account changes. Returns an unsubscribe. */
export function onAccountChange(fn) {
  listeners.add(fn)
  return () => listeners.delete(fn)
}

async function auth(path, body, query = '') {
  const res = await fetch(`${SUPABASE_URL}/auth/v1/${path}${query}`, {
    method: 'POST',
    headers: { apikey: SUPABASE_ANON, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  const text = await res.text()
  const data = text ? JSON.parse(text) : {}
  if (!res.ok) throw new Error(data.error_description || data.msg || data.message || `HTTP ${res.status}`)
  return data
}

function shape(data) {
  if (!data?.access_token) return null
  return {
    access_token: data.access_token,
    refresh_token: data.refresh_token,
    // A minute of slack, so a request is never sent with a token about to expire.
    expires_at: Date.now() + Math.max(0, (data.expires_in ?? 3600) - 60) * 1000,
    email: data.user?.email ?? memory?.email ?? '',
    user_id: data.user?.id ?? memory?.user_id ?? '',
  }
}

/** Email a 6-digit code. Rejects with a readable message. */
export async function sendCode(email) {
  try {
    await auth('otp', { email: email.trim(), create_user: false })
  } catch (err) {
    // GoTrue answers an unknown address with "Signups not allowed for otp"
    // when create_user is false. Say what that means for this product.
    if (/signups? not allowed/i.test(String(err.message))) {
      throw new Error('No Universal ID for that email. Create one free at app.unisim.co.uk, then sign in here.')
    }
    throw err
  }
}

/** Exchange the emailed code for a session. */
export async function verifyCode(email, token) {
  const session = shape(await auth('verify', { email: email.trim(), token: token.trim(), type: 'email' }))
  if (!session) throw new Error('That code was not accepted.')
  store(session)
  return session
}

/** The signed-in account, refreshing the token when it is due. Null signed out. */
export async function session() {
  const current = memory
  if (!current) return null
  if (current.expires_at > Date.now()) return current
  try {
    const next = shape(await auth('token', { refresh_token: current.refresh_token }, '?grant_type=refresh_token'))
    if (!next) throw new Error('no session')
    store(next)
    return next
  } catch {
    // A refused refresh means the session is gone for good (signed out
    // elsewhere, account deleted, token revoked) — don't keep a dead one.
    store(null)
    return null
  }
}

/** Who is signed in, without touching the network. */
export function account() {
  return memory ? { email: memory.email, id: memory.user_id } : null
}

export async function signOut() {
  const current = memory
  store(null)
  if (!current) return
  try {
    await fetch(`${SUPABASE_URL}/auth/v1/logout`, {
      method: 'POST',
      headers: { apikey: SUPABASE_ANON, Authorization: `Bearer ${current.access_token}` },
    })
  } catch {
    /* the local session is already gone, which is what the user asked for */
  }
}

/** An authorised REST call, or null when signed out / the call fails. */
async function rest(path, { method = 'GET', body, prefer } = {}) {
  const s = await session()
  if (!s) return null
  try {
    const res = await fetch(`${SUPABASE_URL}/rest/v1/${path}`, {
      method,
      headers: {
        apikey: SUPABASE_ANON,
        Authorization: `Bearer ${s.access_token}`,
        'Content-Type': 'application/json',
        ...(prefer ? { Prefer: prefer } : {}),
      },
      ...(body ? { body: JSON.stringify(body) } : {}),
    })
    if (!res.ok) return null
    const text = await res.text()
    return text ? JSON.parse(text) : []
  } catch {
    return null
  }
}

/**
 * Merge this device's saved machines with the account's, newest-connected
 * winning per address, and write back whatever the other side was missing.
 *
 * Returns the merged list (most recent first), or null when signed out or
 * offline — the caller then just keeps showing what it had.
 *
 * ⚠️ There are no tombstones: deleting a saved machine while signed OUT only
 * affects that device, and the row comes back on the next sync. Deleting while
 * signed in calls `forget()` and is gone everywhere. The connect screen says
 * so rather than pretending otherwise.
 */
export async function syncSaved(local) {
  const remote = await rest('screens_saved_hosts?select=addr,hostname,os,custom_name,last_connected')
  if (remote === null) return null

  const merged = new Map()
  for (const r of remote) {
    merged.set(r.addr, {
      addr: r.addr,
      hostname: r.hostname ?? '',
      os: r.os ?? '',
      customName: r.custom_name ?? '',
      lastConnected: Date.parse(r.last_connected) || 0,
    })
  }
  const push = []
  for (const l of local) {
    const r = merged.get(l.addr)
    if (!r || l.lastConnected > r.lastConnected) {
      merged.set(l.addr, l)
      push.push(l)
    }
  }
  // Rows this device has never seen are already in `merged` and need no write.
  if (push.length) {
    const s = await session()
    await rest('screens_saved_hosts?on_conflict=user_id,addr', {
      method: 'POST',
      prefer: 'resolution=merge-duplicates',
      body: push.map((h) => ({
        user_id: s.user_id,
        addr: h.addr,
        hostname: h.hostname ?? '',
        os: h.os ?? '',
        custom_name: h.customName ?? '',
        last_connected: new Date(h.lastConnected || Date.now()).toISOString(),
      })),
    })
  }
  return [...merged.values()].sort((a, b) => b.lastConnected - a.lastConnected)
}

/** Save (or refresh) one machine against the account. No-op signed out. */
export async function remember(host) {
  const s = await session()
  if (!s) return
  await rest('screens_saved_hosts?on_conflict=user_id,addr', {
    method: 'POST',
    prefer: 'resolution=merge-duplicates',
    body: [{
      user_id: s.user_id,
      addr: host.addr,
      hostname: host.hostname ?? '',
      os: host.os ?? '',
      custom_name: host.customName ?? '',
      last_connected: new Date(host.lastConnected || Date.now()).toISOString(),
    }],
  })
}

/** Drop one machine from the account. No-op signed out. */
export async function forget(addr) {
  if (!memory) return
  await rest(`screens_saved_hosts?addr=eq.${encodeURIComponent(addr)}`, { method: 'DELETE' })
}

/** The access token for the presence beat, so a signed-in visit counts as the
 *  account rather than as this browser. Null signed out. */
export async function accessToken() {
  const s = await session()
  return s?.access_token ?? null
}
