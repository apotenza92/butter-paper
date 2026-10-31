#!/usr/bin/env python3
# Press and release one X keysym in the current GNOME (Mutter) session.
# Usage: key.py [KEYSYM_HEX]   (default ff1b = Escape)
import sys
import time

from gi.repository import Gio, GLib

keysym = int(sys.argv[1], 16) if len(sys.argv) > 1 else 0xFF1B
bus = Gio.bus_get_sync(Gio.BusType.SESSION)
dest = "org.gnome.Mutter.RemoteDesktop"
(session,) = bus.call_sync(dest, "/org/gnome/Mutter/RemoteDesktop", dest, "CreateSession",
                           None, GLib.VariantType("(o)"), 0, 5000, None).unpack()
iface = dest + ".Session"
bus.call_sync(dest, session, iface, "Start", None, None, 0, 5000, None)
for pressed in (True, False):
    bus.call_sync(dest, session, iface, "NotifyKeyboardKeysym",
                  GLib.Variant("(ub)", (keysym, pressed)), None, 0, 5000, None)
    time.sleep(0.05)
bus.call_sync(dest, session, iface, "Stop", None, None, 0, 5000, None)
