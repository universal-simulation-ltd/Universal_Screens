Universal Screens
=================

Free and open source, from Universal Simulation Ltd.
  https://opensource.unisim.co.uk/screens
  https://github.com/universal-simulation-ltd/Universal_Screens


What got installed
------------------

  extender-host-windows.exe   The host. Start here.
  extender-client.exe         The desktop client (optional component).


The host  --  "use my phone / another screen with THIS PC"
----------------------------------------------------------

Launch "Universal Screens" from the Start menu. The window shows the address to
connect to and a 4-digit pairing PIN, plus a QR code your phone can scan.

Run it from a terminal instead if you want it headless:

  extender-host-windows.exe 0.0.0.0:9000


The client  --  "use THIS PC as a screen for another computer"
---------------------------------------------------------------

Open Universal Screens and expand "Show another computer here". Click the other
computer under "Nearby" (or type its address), enter the PIN that computer
shows, and click "Be its second screen" -- or "Show its screen" to watch its
main screen instead. It opens in a window of its own; press F there for full
screen. (This needs the desktop client component, ticked in the installer.)

To choose which side of the other computer's screen this one sits on, pick
"left of it", "right of it", "above it" or "below it" before clicking -- or,
in the open window (not in control mode), press an arrow key. The other
computer remembers the side for this PC. (A Mac running Universal Screens
0.4.8 or newer; on the Mac, the Virtual displays list has the same buttons.)

Or run the client from a terminal. It has no Start menu shortcut, because
double-clicking it would have nowhere to connect:

  extender-client.exe 192.168.1.42:9000 --pin 1234

--pin is the 4-digit PIN the host shows. It is only needed the first time: the
pairing is remembered, so later runs connect without it.

Options: --size 2560x1600 for an exact size (or --res N for a preset, if the
stream is choppy), --scale 150 for how big things look (default: the same as
this PC's own display scaling), --side left|right|above|below to place the
screen, --monitor N to pick a display, --allow-sleep to let this screen turn
off (by default it stays on while it shows the other computer), --mirror to
view the host's primary display instead of a new virtual one.

Size, scaling and "Let this screen turn off" are also in the app, under "Show
another computer here", and it remembers them. Scaling needs a Mac running
Universal Screens 0.4.8 or newer; an older one shows the screen at 100%.


Letting a phone reach the host
------------------------------

The host listens on TCP port 9000. Windows Firewall blocks that from other
devices until a rule exists, so the host offers to add one -- that is the single
UAC prompt you will see. Without it, only this PC (loopback) and a USB-tethered
phone can connect.


Windows warned me about this app
--------------------------------

It is not code-signed. A signing certificate is a recurring cost we have chosen
not to pass on for a free tool, so SmartScreen shows "Windows protected your PC"
the first time. Click "More info", then "Run anyway".

If you would rather not take our word for it, the whole thing is on GitHub --
read it, or build it yourself in a couple of commands.


Uninstalling
------------

Settings > Apps > Installed apps > Universal Screens. It removes both binaries
and the saved PIN / preferences.


Licence
-------

MIT -- see LICENSE.txt next to this file.
