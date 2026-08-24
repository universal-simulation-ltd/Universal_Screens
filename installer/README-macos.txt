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

The client takes the host's address, so run it from a terminal. It lives inside
the app bundle and gets no icon of its own, because double-clicking it would
have nowhere to connect:

  "/Applications/Universal Screens.app/Contents/Resources/extender-client" 192.168.1.42:9000

Options: --monitor N to pick a display, --res N to drop the resolution if the
stream is choppy, --mirror to view the host's primary display instead of a new
virtual one.


Permissions macOS will ask for
------------------------------

Three, and the app is not much use without the first two:

  Screen Recording   -- to capture what it sends. System Settings > Privacy &
                        Security > Screen Recording. macOS requires a restart
                        of the app after granting this one.
  Accessibility      -- to let a connected phone move the pointer and type.
                        Without it you get picture but no control.
  Local Network      -- so devices on the same Wi-Fi can discover this host.
                        Declined, the app still works if you type the address
                        by hand; discovery just finds nothing.

All three are macOS system prompts. Nothing is sent anywhere: the stream goes
directly to the device you paired with, over your own network.


macOS says it cannot check the app for malicious software
---------------------------------------------------------

It is signed only ad-hoc, and not notarised. A Developer ID and notarisation are
a recurring cost we have chosen not to pass on for a free tool.

Do NOT double-click again -- that dialog has no way forward. Instead:

  Right-click (or Control-click) the app and choose Open, then Open again.

On newer macOS there may be no Open button at all. Go to System Settings >
Privacy & Security, scroll down, and click "Open Anyway". Either way macOS
remembers, so you only do this once.

If you would rather not take our word for it, the whole thing is on GitHub --
read it, or build it yourself in a couple of commands.


Uninstalling
------------

Drag Universal Screens.app from Applications to the Bin. Preferences live in
~/Library/Application Support/UniversalScreens; delete that too for a clean
sweep.


Licence
-------

MIT -- see LICENSE.txt next to this file.
