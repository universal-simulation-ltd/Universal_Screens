Universal Screens knowledge base, English. The one copy every client reads: the browser client fetches this file, the desktop hosts embed it with include_str!, and the Android app packs it as an asset. Format: each article opens with a block of "key: value" lines between two "---" lines (id, group, title, summary), and its body follows until the next "---". Bodies use only blank-line paragraphs, "## " headings, "- " bullets, "1. " steps and **bold**. Everything above the first "---" is ignored.

---
id: what-is-screens
group: The basics
title: What is Universal Screens?
summary: A phone or a browser as a clicker, trackpad, mirror, remote control or extra display for your computer.
---
Universal Screens lets another device use your computer's screen, mouse and keyboard. It comes in two halves.

- **The host** runs on the computer you want to reach: a Mac, a Windows PC or a Linux machine. It shows a QR code and a 4-digit PIN, and it only accepts connections while its window is open.
- **A client** is what you connect from: the phone app, or a web browser on another device.

## What you can do with it

- **Clicker:** a presentation remote with next, previous and blank buttons, and previews of your slides.
- **Trackpad:** use the phone as a touchpad to move the pointer, tap and scroll.
- **Mirror:** watch the computer's screen, without controlling it.
- **Remote control:** see the screen and control it with the mouse and keyboard.
- **Second screen:** use the phone as an extra display. On Windows this needs a virtual-display driver installed on the PC.

The phone app offers all five. The browser client offers Remote control, Mirror and Clicker.

---
id: connecting
group: The basics
title: How do I connect to my computer?
summary: Scan the QR code, pick a nearby computer, or type its address and PIN.
---
Start the host on the computer first. Your device and the computer need to be on the same network, unless you use a remote code (see below).

## Scan the QR code

In the phone app, tap **Scan to connect** and point the camera at the QR code in the host's window. The code carries the computer's address and PIN, so there is nothing to type. It can also carry the computer's Wi-Fi network: on Android 10 and later, the app then asks to join that network, and Android shows its own one-tap prompt before it does.

## Nearby computers

While the connect screen is open, the phone app lists the hosts it can find on your network. Tap one and type its PIN, which the host shows in its window.

## Typing it in

In the phone app, **Advanced** has boxes for the host's address (such as 192.168.1.20:9000) and its PIN. The browser client asks for them on its main page.

## Saved machines

Each machine you connect to is saved, so next time it is one tap. In the phone app you can rename, hide or delete a saved machine, and **Remember next time?** skips the choice of mode.

## From another network

**Remote access (other networks)** in the host's window (Mac, Windows or Linux) gives you a code. Enter it in the browser client under **Remote (across networks)**, with the host's PIN. The connection is relayed through UNI·SIM's server, so expect more lag than on your own network.

---
id: the-pin
group: How it works
title: What does the PIN do?
summary: It pairs a device with the host the first time. After that, the device is remembered.
---
A device needs the host's 4-digit PIN the first time it connects. Scanning the QR code fills it in for you.

The PIN proves your device is allowed in, without ever being sent. Once a device has paired, it and the host remember each other, and it reconnects without the PIN, even after the PIN changes. The article on encryption explains how.

## A new PIN each time

Unless you choose your own, the host picks a fresh PIN every time it starts, from the computer's secure random number generator. On a Mac or Windows PC, **Actions ▸ Regenerate PIN** picks a new one straight away.

## Choosing your own

Paired devices reconnect without a PIN anyway. Your own PIN helps if you want to pair new devices without reading a fresh one off the screen, and for apps from version 0.3 and earlier, which still need the current PIN every time. Set it with **Actions ▸ Use my own PIN** on a Mac or Windows PC, or under **⚙** on Linux. It is off by default. 0000 is not allowed, because on the connection it means "no PIN". A PIN you chose keeps working until you change it, for anyone who has learned it.

## Paired devices

**Actions ▸ Paired devices** on a Mac or Windows PC, or under **⚙** on Linux, lists the devices that have paired with the computer. **Forget all paired devices** makes every one of them enter the PIN again. Do it if a phone or computer that paired is lost, or no longer yours.

## Wrong guesses

The first three wrong PINs in a row cost nothing, so a typo does not lock you out. After that the host stops accepting connections for 1 second, then 2, doubling each time up to 5 minutes. The right PIN resets the count, and so does an hour with no wrong PIN. The pause applies to everyone, so while someone keeps guessing, your own devices have to wait too.

## Who has it

Anyone with the PIN, or a photo of the QR code, can pair a device and control the computer, and that device stays paired until you forget it. There is no separate approval for each device. After you have shared your screen with someone, regenerate the PIN, and forget any paired devices you do not recognise.

---
id: encryption
group: How it works
title: Is the connection encrypted?
summary: Yes, end to end. Your PIN is never sent, and a recording of the connection cannot be used to work it out.
---
Yes. As soon as your device reaches the host, the two run a handshake from the Noise Protocol Framework, a published design for encrypted connections. Everything after it travels inside the encrypted tunnel: the picture of the screen, and your keystrokes and text.

## How the PIN fits in

The first time a device connects, it pairs with the host using the PIN. Pairing uses SPAKE2, a password-authenticated key exchange: each side mixes the PIN into a fresh exchange of keys, so the two can prove they know the same PIN without sending it, or anything worked out from the PIN alone. Someone who records the connection cannot use the recording to guess the PIN. Someone guessing live gets one try per connection, and the host's pause after wrong PINs limits those tries.

While pairing, the host and the device swap long-term keys and remember each other. Later connections use those keys instead of the PIN, so the device reconnects without it, and someone without one of those keys cannot sit in the middle of the connection. Each connection also makes fresh one-off keys, so a recording stays unreadable even if a key or the PIN becomes known later.

## Through the relay

With a remote code, the connection passes through UNI·SIM's relay server. The browser and the host still encrypt end to end, so the relay only passes on scrambled data it cannot read.

## Older versions

Version 0.3 and earlier paired differently, with a key made from the PIN alone. Someone who recorded one of those connections could try all 10,000 PINs against the recording and find yours. Current hosts still let those older apps in, so nothing stops working, and log a warning when one connects. Update them.

A current phone or desktop app never falls back to the older way: if the host is version 0.3 or earlier, it asks you to update the host. The browser client uses the older way only when the host says that is all it can do, says so in its session log, and never does it for a host that has paired with it the current way before. Builds from before the encryption connect without it, and the log says that too.

## For the technically minded

Pairing runs SPAKE2 (Ed25519 group) over the PIN, then Noise_XXpsk0_25519_ChaChaPoly_BLAKE2s with a key derived from the SPAKE2 result as the pre-shared key. Reconnecting runs Noise_XX_25519_ChaChaPoly_BLAKE2s with the remembered keys. Version 0.3 and earlier used Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s with a key derived from the PIN as the pre-shared key.

---
id: what-leaves-your-network
group: Privacy and security
title: What leaves your network?
summary: Your screen goes only to the device you paired with. This is everything else, and where it goes.
---
Your screen is never uploaded to UNI·SIM. On your own network it goes straight from the host to the device you paired with, and nowhere else.

## What stays on your network

- The picture of the screen, your keystrokes, and your mouse and touch input.
- The host's announcement to nearby devices, which carries its name and port so that phones on the same network can list it.
- The host's list of recent connections, which you can clear from its Actions menu.

## What goes to UNI·SIM, and when

- **The user count.** Screens sends a random install ID, so that it can show how many people use it. Signed in, it also says which account, so your devices count as one person. Nothing about your screen, your PIN or your machines is part of it.
- **Your Universal ID, if you sign in.** The article on signing in lists exactly what is kept.
- **Remote access.** A session with a remote code travels through UNI·SIM's relay. It is end-to-end encrypted, so the relay cannot read it.
- **Cast to a browser screen.** When the phone app drives a browser tab this way, its trackpad and clicker commands go through the same relay. They are protected on the way, but they are not end-to-end encrypted. No picture of your screen is sent.

On a network with no internet connection, none of this is sent, and Screens works just the same.

## Scanning with the phone's camera

The host's QR code is a web address. If you scan it with the phone's own camera and the app is not installed, the browser opens that page, and the host's local address and PIN are part of the address the website receives. A Wi-Fi password, if the code carries one, sits in the part of the address that browsers never send.

---
id: signing-in
group: Privacy and security
title: What does signing in do?
summary: It is optional. Signed in, your saved machines follow you, but never your PINs.
---
Nothing about connecting needs an account. Signing in with your Universal ID is only so that a new phone or browser already knows your machines.

## What is synced

The phone app and the browser client keep your saved machines with your account. For each one, that is:

- its address on your network
- the machine's own name and operating system
- the name you gave it, if any
- when you last connected

Deleting a saved machine while signed in removes it from your account too.

## What is not

- **The PIN.** It is never sent, not even scrambled. The phone app keeps it on the phone with each saved machine. The browser client does not save it at all. What a device does keep after pairing is its own key and the keys of the computers it has paired with, so it can reconnect without the PIN. They stay on the device.
- In the phone app, the mode you chose and whether you hid a machine stay on the phone.
- Anything about your screen.

## On the computer

The host signs in for identity only. It has no list of machines to sync, because it is the machine: its recent connections belong to that computer, not to you. Signing in there lets the user count treat your computer, phone and browser as one person.

## Where accounts are made

Screens can only sign in to a Universal ID that already exists. You create one, free, at app.unisim.co.uk, so a mistyped email address can never make a stray account.

---
id: email-code-sign-in
group: Privacy and security
title: Why is there no "Sign in with Google"?
summary: Screens signs in with a code sent to your email, the one method that works in every client.
---
To sign in, you type your email address, you are sent a 6-digit code, and you type the code back. There is no password.

## Why not Google

The browser client is opened from a machine on your own network, over a plain http:// address. It has to be: browsers block a secure https:// page from opening the kind of local connection the client uses to reach the host. Google only lets its sign-in run on secure web addresses registered with it in advance, and an address on your home or office network can never be one of them. So Screens uses the emailed code everywhere: in the phone app, in the browser and on the computer.

## After you sign in

You stay signed in on that device until you sign out. If the sign-in can no longer be renewed, for example because the account was deleted, the app signs you out on that device. Connecting to your machines never depends on it.
