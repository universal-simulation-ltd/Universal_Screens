# What Universal Screens does with your screen

Universal Screens puts your computer's screen on another device, so a privacy
page that said "nothing leaves your computer" would be a lie. What this page
promises instead is **where it goes and who can read it on the way**. Every
claim names the file in this repository that makes it true, and you are
welcome to go and read it.

The short version: **your screen goes only to the device you paired with,
encrypted end to end with your PIN.** Out of the box, Screens sends UNI·SIM
nothing about you. The browser client adds one to an anonymous daily open
count, with no identifier, as every suite web app does; the phone apps and the
desktop hosts count nothing. One extra can send something, and it is off until
you switch it on.

---

## On your own network

The computer (the *host*) and your phone, tablet or browser talk to each other
directly over your network:

| What | Where it goes | The code |
|---|---|---|
| The picture of your screen | straight to the paired device | [`crates/host-macos`](crates/host-macos), [`crates/host-windows`](crates/host-windows), [`crates/host-linux`](crates/host-linux) |
| Your keystrokes, mouse, touch and clicker presses | straight back to the host | [`crates/core`](crates/core) |
| Encryption of all of it | end to end, with your PIN as the key (a Noise tunnel) | [`crates/transport/src/lib.rs`](crates/transport/src/lib.rs) |
| The host's announcement (its name and port), so nearby devices can list it | your local network only | [`crates/discovery`](crates/discovery) |
| The name you gave your phone, shown on the host | the host only | [`crates/protocol`](crates/protocol) |

**The PIN is never sent anywhere**, not even scrambled: it is the key both ends
already know. After pairing, each device keeps its own key and the keys of the
devices it paired with, so it can reconnect without the PIN. Those keys stay on
the device.

**Saved computers stay on your device.** The phone apps and the browser client
remember the computers you connected to (address, name, operating system and
your nickname). The Android app also keeps the PIN for each one, on the phone.
See [`apps/android/.../ConnectionStore.kt`](apps/android/app/src/main/java/com/universalsim/extender/ConnectionStore.kt),
[`apps/ios/ScreenExtender/ConnectionStore.swift`](apps/ios/ScreenExtender/ConnectionStore.swift)
and [`apps/web/src/saved.js`](apps/web/src/saved.js).

**They reach another device in one of two ways, both your choice:** syncing
them with your Universal ID (below, off until you switch it on), or a file you
save yourself. The Android app and the browser client have **Save saved
computers to a file** and **Open a file of saved computers**. The file holds,
for each computer, its address on your network, its own name and operating
system, your nickname for it and when you last connected. **Never the PIN**,
nothing about your screen, and nothing goes to UNI·SIM: the file goes wherever
you put it. It still lists addresses on your network, so keep it somewhere
private. See [`apps/web/src/savedFile.js`](apps/web/src/savedFile.js) and
[`apps/android/.../SavedFile.kt`](apps/android/app/src/main/java/com/universalsim/extender/SavedFile.kt).

On a network with no internet connection, Screens works exactly the same.

---

## Across networks: the relay

Two features use UNI·SIM's relay at `opensource.unisim.co.uk`, because a
browser tab cannot accept a connection on its own:

- **Remote access** (a 6-character code from the host's *Remote access* panel).
  The session is the ordinary host connection, still encrypted end to end with
  your PIN, carried through the relay. The relay cannot read it. See
  [`crates/host-ui/src/remote.rs`](crates/host-ui/src/remote.rs).
- **Cast to a browser screen** (the phone app driving a browser tab). The
  trackpad and clicker commands go through the same relay. They are encrypted
  on the way (HTTPS), but not end to end. No picture of your screen is sent.
  See [`apps/android/.../RoomSession.kt`](apps/android/app/src/main/java/com/universalsim/extender/RoomSession.kt)
  and [`apps/ios/ScreenExtender/RoomSession.swift`](apps/ios/ScreenExtender/RoomSession.swift).

The relay pairs the two ends that share a code, passes their messages along,
and keeps nothing once the session ends. Like any server, it sees the two
connections' IP addresses while they are open.

---

## The one thing you can switch on

### Signing in, and syncing your saved computers

Signing in with a Universal ID is optional in the Android app, the browser
client and the desktop hosts. It uses a code emailed to you, and only works
for an account that already exists (made at app.unisim.co.uk). Signing in
sends your email address to our sign-in service. **It uploads nothing else.**
The iPhone app has no sign-in.

In the Android app and the browser client, the sign-in window then offers
**Sync saved computers to my account**, unticked. Only if you tick it does the
app keep your saved computers with your account, so a new device already knows
them. For each one that is:

- its address on your network
- the computer's own name and operating system
- the nickname you gave it, if any
- when you last connected

**Never the PIN**, and nothing about your screen. Untick the box and syncing
stops, and the computers already uploaded are deleted from your account (if
the device is offline at that moment, the delete is retried the next time it
can reach us). They stay on the device. Signing out turns syncing off as well.
See [`apps/web/src/account.js`](apps/web/src/account.js) and
[`apps/android/.../Account.kt`](apps/android/app/src/main/java/com/universalsim/extender/Account.kt).

The desktop hosts sign in for identity only: they have no list of computers to
sync, because they are the computer. See
[`crates/host-ui/src/account.rs`](crates/host-ui/src/account.rs).

### The daily open count (browser client only)

Like every web app in the suite, the browser client adds one to an anonymous
count of how many times Screens was opened that day. The first time the page
is used on a given day (UTC) in your browser, it sends one message holding the
app's name and the word `web`, and nothing else: no ID of any kind, never your
account (even when you are signed in), nothing about your screen, your PIN or
your computers. Our server keeps a total per day, which identifies nobody.

To count each browser only once a day, the page remembers today's date in your
browser (`unisim:opened:screens`); that note is never sent. It waits until you
first press, type, touch or scroll, so a page that merely loads is not counted.
A copy served on your own computer or local network (by a host or a dev
server) never counts, and nor does a browser under automated testing or one
marked as a UNI·SIM staff device. See
[`apps/web/src/openCount.js`](apps/web/src/openCount.js).

**The Android and iPhone apps and the desktop hosts count nothing and send no
usage data at all.** (Until 2026-10-12 the browser client had an opt-in **Show
how many people use Screens** box under Advanced, which sent a random ID
replaced daily; it is gone, along with the code that sent it.)

---

## What there isn't

No third-party analytics, no tracking, no advertising, no crash reporting
service. Nothing is sold or shared.

---

## How to check it yourself

1. Open the browser client with your browser's developer tools on the
   **Network** tab.
2. Connect to a host and use it. On your own network you will see the
   connection to your host and nothing addressed to UNI·SIM.
3. Or turn off your router's internet connection and use Screens anyway. It
   keeps working, because the work was never happening anywhere else.

---

## If you find this page is wrong

That is worth more to us than it costs. Open an issue on
[the repository](https://github.com/universal-simulation-ltd/Universal_Screens/issues).
Contact: inbox@unisim.co.uk.
