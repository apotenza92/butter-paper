#!/usr/bin/env python3
# Save one frame of the current GNOME (Mutter) session's first monitor as a PNG.
# Usage: shot.py OUT.png
# Works in a headless gnome-shell on a private D-Bus session, where the
# Screenshot API is restricted but Mutter's ScreenCast API is not.
import subprocess
import sys

from gi.repository import Gio, GLib

out = sys.argv[1]
bus = Gio.bus_get_sync(Gio.BusType.SESSION)


def call(path, iface, method, args, rtype, dest="org.gnome.Mutter.ScreenCast"):
    return bus.call_sync(dest, path, iface, method, args,
                         GLib.VariantType(rtype) if rtype else None, 0, 5000, None)


state = call("/org/gnome/Mutter/DisplayConfig", "org.gnome.Mutter.DisplayConfig",
             "GetCurrentState", None, "(ua((ssss)a(siiddada{sv})a{sv})a(iiduba(ssss)a{sv})a{sv})",
             dest="org.gnome.Mutter.DisplayConfig").unpack()
connector = state[1][0][0][0]

(session,) = call("/org/gnome/Mutter/ScreenCast", "org.gnome.Mutter.ScreenCast",
                  "CreateSession", GLib.Variant("(a{sv})", ({},)), "(o)").unpack()
(stream,) = call(session, "org.gnome.Mutter.ScreenCast.Session", "RecordMonitor",
                 GLib.Variant("(sa{sv})", (connector, {"cursor-mode": GLib.Variant("u", 0)})),
                 "(o)").unpack()

loop = GLib.MainLoop()
node = {}


def on_stream(_c, _s, _p, _i, _sig, params):
    node["id"] = params.unpack()[0]
    loop.quit()


bus.signal_subscribe("org.gnome.Mutter.ScreenCast", "org.gnome.Mutter.ScreenCast.Stream",
                     "PipeWireStreamAdded", stream, None, Gio.DBusSignalFlags.NONE, on_stream)
call(session, "org.gnome.Mutter.ScreenCast.Session", "Start", None, None)
GLib.timeout_add_seconds(5, loop.quit)
loop.run()

try:
    if "id" not in node:
        sys.exit("screencast stream did not start")
    subprocess.run(["gst-launch-1.0", "-q", "pipewiresrc", f"path={node['id']}", "num-buffers=1",
                    "!", "videoconvert", "!", "pngenc", "!", "filesink", f"location={out}"],
                   check=True, timeout=20)
finally:
    call(session, "org.gnome.Mutter.ScreenCast.Session", "Stop", None, None)
