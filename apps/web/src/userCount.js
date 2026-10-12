// "There are X total users (Y live)" — the same line other apps in the suite
// show, for the browser client.
//
// Universal Screens is not React and has no bundler, so it cannot use
// `@unisim/sdk`'s presence module; this is that module's two calls written out
// against Supabase's REST endpoint:
//
//   * POST /rest/v1/rpc/app_presence_beat {p_product:'screens', p_install_id,
//     p_platform:'web'} on load and every 45 s while the page is visible;
//   * POST /rest/v1/rpc/app_user_counts   {p_product:'screens'} → {total, live}
//     — and `suite_user_counts` when the line has been tapped for the
//     whole-suite figure.
//
// ⚠️ **Off by default, and anonymous.** Nothing is sent until the person ticks
// "Show how many people use Screens" under Advanced. Even then the id is a
// random one that is replaced every day (kept with its date in localStorage),
// so no visit can be linked to another day's, and the beat NEVER carries the
// Universal ID token or user id — it always goes with the public anon key.
// Nothing else is sent: no host, no PIN, no room code, nothing about what is
// on screen. When there is no internet on this network (a normal way to use
// Screens), every call fails quietly and the line simply never appears.
//
// The anon key is a PUBLISHABLE key — it ships in every suite web bundle by
// design, and Row-Level Security is the real boundary: `app_presence` has RLS
// on with no policies at all, so this key can reach it through those two
// functions and in no other way.

const SUPABASE_URL = 'https://rygfxgalojojppxmhddo.supabase.co'
const SUPABASE_ANON =
  'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6InJ5Z2Z4Z2Fsb2pvanBweG1oZGRvIiwicm9sZSI6ImFub24iLCJpYXQiOjE3Nzg3NTY4MjUsImV4cCI6MjA5NDMzMjgyNX0.hLy_vt9vY_rdPKF3nL32yAuMCD604E3CH5VM7D7CaNE'

const PRODUCT = 'screens'
const BEAT_MS = 45000
const SCOPE_KEY = 'unisim:user-count-scope'
/** "1" once the person has ticked the box. */
const ENABLED_KEY = 'universal-screens.show-user-count'
/** `{ day, id }` — today's random id. A new day gets a new id. */
const DAILY_ID_KEY = 'universal-screens.count-id'

function today() {
  return new Date().toISOString().slice(0, 10)
}

/** A random id that lasts one (UTC) day, then is replaced. */
export function dailyId() {
  const day = today()
  try {
    const kept = JSON.parse(localStorage.getItem(DAILY_ID_KEY) ?? 'null')
    if (kept && kept.day === day && typeof kept.id === 'string') return kept.id
    const id = crypto.randomUUID()
    localStorage.setItem(DAILY_ID_KEY, JSON.stringify({ day, id }))
    return id
  } catch {
    // Private mode or blocked storage: a per-load id still counts this visit.
    return crypto.randomUUID()
  }
}

function enabled() {
  try { return localStorage.getItem(ENABLED_KEY) === '1' } catch { return false }
}

async function rpc(fn, body) {
  // Always the anon key — never the Universal ID session — so the server
  // cannot tie a beat to an account.
  const res = await fetch(`${SUPABASE_URL}/rest/v1/rpc/${fn}`, {
    method: 'POST',
    headers: {
      'apikey': SUPABASE_ANON,
      'Authorization': `Bearer ${SUPABASE_ANON}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(body ?? {}),
  })
  if (!res.ok) throw new Error(`${fn}: ${res.status}`)
  const text = await res.text()
  return text ? JSON.parse(text) : null
}

function format(counts, scope) {
  const n = (v) => Number(v).toLocaleString()
  return scope === 'suite'
    ? `There ${counts.total === 1 ? 'is' : 'are'} a total of ${n(counts.total)} user${counts.total === 1 ? '' : 's'} across all UNI·SIM apps (${n(counts.live)} live)`
    : `There ${counts.total === 1 ? 'is' : 'are'} ${n(counts.total)} total user${counts.total === 1 ? '' : 's'} (${n(counts.live)} live)`
}

/**
 * Wire the opt-in box and, only while it is ticked, beat and keep `el` up to
 * date. Tapping the line switches between this app's figure and the whole
 * suite's, remembered under the same localStorage key the React apps use.
 */
export function startUserCount(el, box) {
  if (!el) return
  // The whole suite first; this app's own figure is one tap away. Only an
  // explicit 'app' switches it back.
  let scope = 'suite'
  try { if (localStorage.getItem(SCOPE_KEY) === 'app') scope = 'app' } catch { /* ignore */ }
  let beaten = false
  let timer = null
  let on = enabled()

  const beat = () =>
    rpc('app_presence_beat', { p_product: PRODUCT, p_install_id: dailyId(), p_platform: 'web' })
      .then(() => { beaten = true })
      .catch(() => {})

  const paint = async () => {
    if (!on) return
    try {
      const rows = scope === 'suite'
        ? await rpc('suite_user_counts', {})
        : await rpc('app_user_counts', { p_product: PRODUCT })
      const row = Array.isArray(rows) ? rows[0] : rows
      const total = Number(row?.total)
      let live = Number(row?.live)
      if (!on || !Number.isFinite(total) || !Number.isFinite(live) || total <= 0) return
      // Whoever is reading this is using the app; a count that raced the beat
      // must not answer "(0 live)".
      if (beaten) live = Math.max(live, 1)
      el.textContent = format({ total, live }, scope)
      el.hidden = false
    } catch {
      /* offline, or a LAN with no route out — the line just stays hidden */
    }
  }

  const tick = () => {
    if (!on || document.visibilityState === 'hidden') return
    beat().then(paint)
  }

  const apply = () => {
    if (on) {
      tick()
      timer ??= setInterval(tick, BEAT_MS)
    } else {
      if (timer) clearInterval(timer)
      timer = null
      beaten = false
      el.hidden = true
      el.textContent = ''
    }
  }

  el.addEventListener('click', () => {
    scope = scope === 'app' ? 'suite' : 'app'
    try { localStorage.setItem(SCOPE_KEY, scope) } catch { /* ignore */ }
    void paint()
  })

  if (box) {
    box.checked = on
    box.addEventListener('change', () => {
      on = box.checked
      try {
        if (on) localStorage.setItem(ENABLED_KEY, '1')
        else localStorage.removeItem(ENABLED_KEY)
      } catch { /* ignore */ }
      apply()
    })
  }

  apply()
  document.addEventListener('visibilitychange', () => { if (document.visibilityState !== 'hidden') tick() })
}
