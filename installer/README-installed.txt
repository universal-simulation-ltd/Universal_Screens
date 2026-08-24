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

The client takes the host's address, so run it from a terminal (there is no
Start menu shortcut, because double-clicking it would have nowhere to connect):

  extender-client.exe 192.168.1.42:9000

Options: --monitor N to pick a display, --res N to drop the resolution if the
stream is choppy.


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
