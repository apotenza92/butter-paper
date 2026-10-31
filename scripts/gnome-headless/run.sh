#!/bin/sh
# Run a command inside a private, headless GNOME session with a virtual
# monitor, independent of the real screen, lid and logged-in desktop. Inside
# it, capture the screen with shot.py, press keys with key.py and move or drag
# the pointer with pointer.py (all in this folder).
# Usage: run.sh [--size WxH] [--x11] COMMAND [ARGS...]
#   --x11  run COMMAND as an X11 client through Xwayland (GNOME draws its
#          title bar), instead of as a native Wayland client.
# GNOME_HEADLESS_LOG names gnome-shell's log (default: a file in /tmp).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
size=1600x1100
x11=0
while [ $# -gt 0 ]; do
  case $1 in
    --size) size=$2; shift 2 ;;
    --x11) x11=1; shift ;;
    *) break ;;
  esac
done
[ $# -gt 0 ] || { echo "usage: run.sh [--size WxH] [--x11] COMMAND [ARGS...]" >&2; exit 2; }

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
display="headless-$$"
log="${GNOME_HEADLESS_LOG:-/tmp/gnome-headless-$$.log}"

exec dbus-run-session -- sh -c '
  here=$1 display=$2 size=$3 log=$4 x11=$5; shift 5
  before=$(ls /tmp/.X11-unix 2>/dev/null | sort)
  if [ "$x11" = 1 ]; then x11_flag=; else x11_flag=--no-x11; fi
  gnome-shell --headless $x11_flag --wayland-display "$display" --virtual-monitor "$size" >"$log" 2>&1 &
  shell=$!
  trap "kill $shell 2>/dev/null; wait $shell 2>/dev/null" EXIT
  i=0
  until [ -S "$XDG_RUNTIME_DIR/$display" ] && gdbus introspect --session \
      --dest org.gnome.Mutter.ScreenCast --object-path /org/gnome/Mutter/ScreenCast >/dev/null 2>&1; do
    i=$((i + 1)); [ $i -lt 60 ] || { echo "headless GNOME did not start; see $log" >&2; exit 1; }
    sleep 0.5
  done
  sleep 2
  "$here/key.py" ff1b  # leave the overview GNOME opens at login
  if [ "$x11" = 1 ]; then
    new=$(ls /tmp/.X11-unix | sort | grep -vxF "$before" | head -1)
    [ -n "$new" ] || { echo "Xwayland did not start; see $log" >&2; exit 1; }
    export DISPLAY=":${new#X}"
    unset WAYLAND_DISPLAY
    "$@"
  else
    unset DISPLAY
    WAYLAND_DISPLAY=$display "$@"
  fi
' sh "$here" "$display" "$size" "$log" "$x11" "$@"
