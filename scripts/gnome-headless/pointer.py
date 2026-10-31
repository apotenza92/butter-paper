#!/usr/bin/env python3
# Move or drag the pointer in the current GNOME (Mutter) session, in monitor
# pixels, with relative motion from the bottom-left corner (absolute motion
# needs a negotiated ScreenCast stream; top-left is the hot corner).
# Usage: pointer.py move X Y | drag X1 Y1 X2 Y2
import sys
import time

from gi.repository import Gio, GLib

bus = Gio.bus_get_sync(Gio.BusType.SESSION)
RD = "org.gnome.Mutter.RemoteDesktop"


def call(path, iface, method, args=None, rtype=None, dest=RD):
    return bus.call_sync(dest, path, iface, method, args,
                         GLib.VariantType(rtype) if rtype else None, 0, 5000, None)


state = call("/org/gnome/Mutter/DisplayConfig", "org.gnome.Mutter.DisplayConfig",
             "GetCurrentState", None,
             "(ua((ssss)a(siiddada{sv})a{sv})a(iiduba(ssss)a{sv})a{sv})",
             dest="org.gnome.Mutter.DisplayConfig").unpack()
height = next(m for m in state[1][0][1] if m[6].get("is-current"))[2]
(rd,) = call("/org/gnome/Mutter/RemoteDesktop", RD, "CreateSession", None, "(o)").unpack()
session = RD + ".Session"
call(rd, session, "Start")
position = [0.0, float(height - 1)]


def relative(dx, dy):
    call(rd, session, "NotifyPointerMotionRelative", GLib.Variant("(dd)", (dx, dy)))
    time.sleep(0.02)


def to(x, y, steps=1):
    for i in range(1, steps + 1):
        tx = position[0] + (x - position[0]) / (steps - i + 1)
        ty = position[1] + (y - position[1]) / (steps - i + 1)
        relative(tx - position[0], ty - position[1])
        position[:] = [tx, ty]


def button(pressed):
    call(rd, session, "NotifyPointerButton", GLib.Variant("(ib)", (0x110, pressed)))
    time.sleep(0.15)


try:
    # Down first, then left: a fresh session starts the pointer at (0, 0),
    # where pushing left would press into the hot corner.
    relative(0, 100000)
    relative(-100000, 0)
    args = [float(value) for value in sys.argv[2:]]
    to(*args[:2], steps=10)
    if sys.argv[1] == "drag":
        time.sleep(0.3)
        button(True)
        to(*args[2:4], steps=25)
        time.sleep(0.3)
        button(False)
finally:
    call(rd, session, "Stop")
