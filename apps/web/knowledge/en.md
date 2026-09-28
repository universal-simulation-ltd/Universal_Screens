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

On a Mac or Windows PC, **Remote access (other networks)** in the host's window gives you a code. Enter it in the browser client under **Remote (across networks)**, with the host's PIN. The connection is relayed through UNI·SIM's server, so expect more lag than on your own network.

---
id: the-pin
group: How it works
title: What does the PIN do?
summary: It lets your devices in, and it is also the key the connection is encrypted with.
---
Every connection needs the host's 4-digit PIN. Scanning the QR code fills it in for you.

The PIN does two jobs. The host checks it before letting a device in, and it is also the key the connection is encrypted with. The article on encryption explains the second part.

## A new PIN each time

Unless you choose your own, the host picks a fresh PIN every time it starts, from the computer's secure random number generator. On a Mac or Windows PC, **Actions ▸ Regenerate PIN** picks a new one straight away.

## Choosing your own

If you want saved devices to reconnect after the computer restarts, you can set a PIN of your own: **Actions ▸ Use my own PIN** on a Mac or Windows PC, or under **⚙** on Linux. It is off by default. 0000 is not allowed, because on the connection it means "no PIN". A PIN you chose keeps working until you change it, for anyone who has learned it.

## Wrong guesses

The first three wrong PINs in a row cost nothing, so a typo does not lock you out. After that the host stops accepting connections for 1 second, then 2, doubling each time up to 5 minutes. The right PIN resets the count, and so does an hour with no wrong PIN. The pause applies to everyone, so while someone keeps guessing, your own devices have to wait too.

## Who has it

Anyone with the PIN, or a photo of the QR code, can connect and control the computer. There is no separate approval for each device. After you have shared your screen with someone, regenerate the PIN.

---
id: encryption
group: How it works
title: Is the connection encrypted?
summary: Yes, end to end, with your PIN as the key. The browser client tells you if it cannot be.
---
Yes. As soon as your device reaches the host, the two run a handshake from the Noise Protocol Framework, a published design for encrypted connections. Everything after it travels inside the encrypted tunnel: the picture of the screen, your keystrokes and text, and the PIN check itself.

## How the PIN fits in

The handshake mixes the PIN in as a shared secret. A device with the wrong PIN cannot complete it, and neither can someone trying to sit in the middle of the connection without the PIN. Each connection also makes fresh one-off keys, so a recording of the traffic stays unreadable even if the PIN becomes known later.

## Through the relay

With a remote code, the connection passes through UNI·SIM's relay server. The browser and the host still encrypt end to end, so the relay only passes on scrambled data it cannot read.

## When it is not encrypted

Builds from before the encryption still connect without it. The host logs a warning when an old client connects in plaintext. The browser client checks what the host can do and says in its session log whether the session is end-to-end encrypted. If the host is an older build, the log says so and asks you to update it.

## For the technically minded

The pattern is Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s, with a key derived from the PIN as the pre-shared key.

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

- **The PIN.** It is the key the connection is encrypted with, so it is never sent. The phone app keeps it on the phone with each saved machine, so it can reconnect. The browser client does not save it at all.
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
