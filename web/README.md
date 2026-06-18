# Web pieces for the host QRs (Step 1 "Get the app" + Step 2 "Connect")

The host shows two https QRs, both on the suite domain, so a phone **camera** is
never a dead end and an **installed app** deep-links straight in:

- **Step 1** points at `https://opensource.unisim.co.uk/screens` — installed → opens
  the app; not installed → the marketing page (with download links).
- **Step 2** (the connection code) points at
  `https://opensource.unisim.co.uk/screens/connect?host=<ip>&port=<port>&pin=<pin>`
  with any Wi-Fi credentials in the URL **fragment** (`#ssid=…&auth=…&pass=…`, kept
  client-side by browsers so the password never reaches the server). Installed →
  opens the app, which pairs from the query/fragment; not installed → the
  `/screens/connect` page (`opensource-portal/public/screens/connect.html`) says
  "scan this inside the app" and offers the download buttons. The host still keeps
  the legacy `unisimscreens://connect?…` custom scheme working as a fallback (the
  app parses both — see `MainActivity.parseConnectPayload` / `ContentView`).

To make the **camera-opens-the-app** half work (Android App Links / iOS Universal
Links) the domain must carry the association files below.

## The one required file

- `.well-known/assetlinks.json` → deploy at the **domain root**:
  **`https://opensource.unisim.co.uk/.well-known/assetlinks.json`**
  (served as `application/json`, HTTP 200, **no redirects**). This is what makes
  Android open the app instead of the browser. No dedicated landing page is needed
  — when the app isn't installed, the browser just loads your existing `/screens`
  page (with its download links).

## Before it works

1. **Add the production signing fingerprint** to `assetlinks.json`. The one there
   now is the **debug** cert (`CE:14:…`), which only verifies debug builds. For the
   published app, add the **Play App Signing** SHA-256 (Play Console → *Test and
   release → App integrity → App signing key certificate*). App Links accept
   multiple fingerprints — keep both:
   ```json
   "sha256_cert_fingerprints": ["<debug …>", "<play release …>"]
   ```
2. **iOS Universal Links** need an `apple-app-site-association` (AASA) file at the
   domain root once the iOS app exists. A **template** is provided here at
   `.well-known/apple-app-site-association` — it claims `/screens` and
   `/screens/connect`, but its `appIDs` is a placeholder (`TEAMID.…`). Before
   deploying it to `opensource-portal/public/.well-known/` (served `application/json`,
   HTTP 200, no redirects, **no file extension**): replace `TEAMID` with the Apple
   Developer **Team ID**, add the `applinks:opensource.unisim.co.uk`
   **Associated Domains** entitlement to the iOS app, and strip the `comment` keys
   (Apple ignores them, but keep the file lean). It is **not** deployed yet — iOS is
   still a Windows-side scaffold (no Xcode build); `ContentView.onOpenURL` already
   parses the link so the app side is ready.

The Android app's intent filters already target these URLs
(`apps/android/.../AndroidManifest.xml`: `autoVerify="true"` on the
`https opensource.unisim.co.uk /screens` **prefix** — which covers both `/screens`
and `/screens/connect` — plus the `unisimscreens://` scheme). `assetlinks.json` uses
`handle_all_urls`, so it already authorises `/screens/connect` with no change.

## Optional: smart-banner snippet for the existing /screens page

App Links already auto-open the app when installed, so this is just polish — an
explicit "Open / download" affordance on the marketing page. Paste into `/screens`
and fill in the store URLs:

```html
<script>
  var PLAY = "https://play.google.com/store/apps/details?id=com.universalsim.extender";
  var APPSTORE = "https://apps.apple.com/app/universal-screens/idXXXXXXXXX";
  var ua = navigator.userAgent || "";
  if (/Android/i.test(ua)) location.href = PLAY;          // or render a button
  else if (/iPhone|iPad|iPod/i.test(ua)) location.href = APPSTORE;
  // desktop: leave the page as-is (host download + QR instructions)
</script>
```

## Verify (Android)

```bash
adb shell pm verify-app-links --re-verify com.universalsim.extender
adb shell pm get-app-links com.universalsim.extender   # look for "verified"
```
