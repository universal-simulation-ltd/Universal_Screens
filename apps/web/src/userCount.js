// "There are X total users (Y live)" — the same line every other app in the
// suite shows (James, 2026-09-17), for the browser client.
//
// Universal Screens is not React and has no bundler, so it cannot use
// `@unisim/sdk`'s presence module; this is that module's two calls written out
// against Supabase's REST endpoint. The behaviour is deliberately identical, so
// read `packages/sdk/src/presence.ts` in `universal-platform` before changing
// anything here:
//
//   * POST /rest/v1/rpc/app_presence_beat {p_product:'screens', p_install_id}
//     on load and every 45 s while the page is visible (migration 0175);
//   * POST /rest/v1/rpc/app_user_counts   {p_product:'screens'} → {total, live}
//     — and `suite_user_counts` (0177) when the line has been tapped for the
//     whole-suite figure.
//
// The install id is a random UUID in localStorage. Nothing else is sent: no
// host, no PIN, no room code, nothing about what is on screen. This page is
// served over plain http:// from the host or `serve.mjs`, and a request from
// an http page to an https endpoint is allowed (it is the other way round that
// browsers block), so the LAN case works; when it does not (no internet on
// this network, which is a normal way to use Screens), every call fails
// quietly and the line simply never appears.
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
const INSTALL_ID_KEY = 'unisim:install-id'   // shared spelling with the SDK
const SCOPE_KEY = 'unisim:user-count-scope'

function installId() {
  try {
    const existing = localStorage.getItem(INSTALL_ID_KEY)
    if (existing) return existing
    const id = crypto.randomUUID()
    localStorage.setItem(INSTALL_ID_KEY, id)
    return id
  } catch {
    // Private mode or blocked storage: a per-load id still counts this session.
    return crypto.randomUUID()
  }
}

async function rpc(fn, body) {
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
 * Beat, and keep `el` up to date. Tapping it switches between this app's figure
 * and the whole suite's, remembered under the same localStorage key the React
 * apps use, so a browser that has both keeps one answer.
 */
export function startUserCount(el) {
  if (!el) return
  const id = installId()
  let scope = 'app'
  try { if (localStorage.getItem(SCOPE_KEY) === 'suite') scope = 'suite' } catch { /* ignore */ }
  let beaten = false

  const beat = () =>
    rpc('app_presence_beat', { p_product: PRODUCT, p_install_id: id })
      .then(() => { beaten = true })
      .catch(() => {})

  const paint = async () => {
    try {
      const rows = scope === 'suite'
        ? await rpc('suite_user_counts', {})
        : await rpc('app_user_counts', { p_product: PRODUCT })
      const row = Array.isArray(rows) ? rows[0] : rows
      const total = Number(row?.total)
      let live = Number(row?.live)
      if (!Number.isFinite(total) || !Number.isFinite(live) || total <= 0) return
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
    if (document.visibilityState === 'hidden') return
    beat().then(paint)
  }

  el.addEventListener('click', () => {
    scope = scope === 'app' ? 'suite' : 'app'
    try { localStorage.setItem(SCOPE_KEY, scope) } catch { /* ignore */ }
    void paint()
  })

  tick()
  setInterval(tick, BEAT_MS)
  document.addEventListener('visibilitychange', () => { if (document.visibilityState !== 'hidden') tick() })
}
