Universal Screens
=================

Free and open source, from Universal Simulation Ltd.
  https://opensource.unisim.co.uk/screens
  https://github.com/universal-simulation-ltd/Universal_Screens


Installing
----------

Drag "Universal Screens.app" onto the Applications shortcut in this window.
That is the whole install -- nothing is copied outside the app itself.

Requires macOS 12.3 or newer. The app uses ScreenCaptureKit to capture the
screen, which does not exist before 12.3, so older versions are refused at
launch rather than failing halfway through.


The host  --  "use my phone / another screen with THIS Mac"
------------------------------------------------------------

Open Universal Screens from Applications. The window shows the address to
connect to and a 4-digit pairing PIN, plus a QR code your phone can scan.

Run it headless from a terminal instead if you prefer:

  "/Applications/Universal Screens.app/Contents/MacOS/extender-host-macos" 0.0.0.0:9000


The client  --  "use THIS Mac as a screen for another computer"
----------------------------------------------------------------

Open Universal Screens and expand "Show another computer here". Click the other
computer under "Nearby" (or type its address), enter the PIN that computer
shows, and click "Show its screen" -- or "Be its second screen" to extend
that computer onto this Mac. It opens in a window of its own; tick "Open full
screen" to have it fill this screen. Press F in that window to switch full
screen on or off, and Esc to leave it: a reminder says so at the top whenever
it goes full screen. "Disconnect" in the panel closes it.

While the host runs, "Connected now" lists every device using this Mac, each
with a "Disconnect" button. When a device that was this Mac's second screen
disconnects (however it ends), its virtual display is removed, so no windows
or pointer are left on a screen nobody can see.

Or run the client from a terminal. It lives inside the app bundle and gets no
icon of its own, because double-clicking it would have nowhere to connect:

  "/Applications/Universal Screens.app/Contents/Resources/extender-client" 192.168.1.42:9000 --pin 1234

--pin is the 4-digit PIN the host shows (under "More details"). It is only
needed the first time: the pairing is remembered, so later runs connect
without it.

Options: --monitor N to pick a display, --res N to drop the resolution if the
stream is choppy, --fullscreen to open full screen, --mirror to view the host's
primary display instead of a new virtual one (this is how you watch a PC's
screen on this Mac).


Permissions macOS will ask for
------------------------------

Three, and the app is not much use without the first two:

  Screen Recording   -- to capture what it sends. System Settings > Privacy &
                        Security > Screen Recording. macOS requires a restart
                        of the app after granting this one.
                        If a device connects but gets no picture while the
                        switch shows on (it happens once, when updating from
                        0.4.7 or older): remove Universal Screens from that
                        list with the minus button, add it back, and restart
                        the app. Do the same under Accessibility if the
                        picture comes but the mouse and keyboard do nothing.
  Accessibility      -- to let a connected phone move the pointer and type.
                        Without it you get picture but no control.
  Local Network      -- so devices on the same Wi-Fi can discover this host.
                        Declined, the app still works if you type the address
                        by hand; discovery just finds nothing.

All three are macOS system prompts. Nothing is sent anywhere: the stream goes
directly to the device you paired with, over your own network.


Signed and notarised
--------------------

Universal Screens is signed with Universal Simulation Ltd's Developer ID and
notarised by Apple, so it opens like any other app -- no right-click or "Open
Anyway" needed. If you would rather not take our word for it, the whole thing
is on GitHub -- read it, or build it yourself in a couple of commands.

Updating from 0.4.7 or older? Those were signed differently, so macOS treats
this version as a new app once: if a connecting device gets a black window, or
a picture you cannot control, follow the Screen Recording note above. Later
updates keep their permissions.


Uninstalling
------------

Drag Universal Screens.app from Applications to the Bin. Preferences live in
~/Library/Application Support/UniversalScreens; delete that too for a clean
sweep.


Licence
-------

MIT -- see LICENSE.txt next to this file.
