#!/bin/sh
set -eu
fail() { printf '%s\n' "$*" >&2; exit 1; }
[ "$(id -u)" -ne 0 ] || fail 'Run this script as your user, not root.'
[ -n "${HOME:-}" ] || fail 'HOME must be set.'
case "$HOME" in /*) ;; *) fail 'HOME must be an absolute path.' ;; esac
case "$HOME" in *'/../'*|*/..|../*|..) fail 'Unsafe HOME.' ;; esac
data=${XDG_DATA_HOME:-"$HOME/.local/share"}
case "$data" in /*) ;; *) fail 'XDG_DATA_HOME must be an absolute path.' ;; esac
case "$data" in *'/../'*|*/..|../*|..) fail 'Unsafe XDG_DATA_HOME.' ;; esac
bin="$HOME/.local/bin"
target="$data/butter-paper/@VERSION@"
launcher="$bin/butter-paper"
desktop="$data/applications/butter-paper.desktop"
icon="$data/icons/hicolor/1024x1024/apps/butter-paper.png"
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P) || fail 'Cannot locate package directory.'
[ -f "$root/MANIFEST.json" ] && [ ! -L "$root/MANIFEST.json" ] || fail 'Package identity manifest is missing or unsafe.'
[ -d "$target" ] && [ ! -L "$target" ] || fail 'Owned versioned installation was not found.'
actual_count=0
for entry in "$target"/* "$target"/.[!.]* "$target"/..?*
do
  [ -e "$entry" ] || [ -L "$entry" ] || continue
  case "$(basename -- "$entry")" in @PAYLOAD_CASES@) ;; *) fail 'Versioned install directory contains unrelated files; preserved it.' ;; esac
  [ -f "$entry" ] && [ ! -L "$entry" ] || fail 'Installed payload contains an unsafe entry.'
  actual_count=$((actual_count + 1))
done
[ "$actual_count" -eq @PAYLOAD_COUNT@ ] || fail 'Installed payload inventory does not match this package.'
for name in @PAYLOAD_WORDS@
do
  [ -f "$root/$name" ] && [ ! -L "$root/$name" ] || fail 'Package payload is missing or unsafe.'
  cmp -s "$root/$name" "$target/$name" || fail 'Installed payload differs from the package; preserving it.'
done
launcher_tmp= desktop_tmp= tmpdir=
tmpdir=$(printenv TMPDIR || true)
[ -n "$tmpdir" ] || tmpdir=/tmp
trap 'rm -f -- "$launcher_tmp" "$desktop_tmp"' EXIT
trap 'exit 1' HUP INT TERM
launcher_tmp=$(mktemp "$tmpdir/butter-paper-launcher-XXXXXX")
make_launcher() {
  quoted=$(printf '%s' "$target/butter-paper" | sed "s/'/'\\''/g")
  printf '#!/bin/sh\nexec '\''%s'\'' "$@"\n' "$quoted" > "$1"
  chmod 0755 "$1"
}
make_launcher "$launcher_tmp"
cmp -s "$launcher_tmp" "$launcher" || fail 'Launcher identity does not match this installation.'
desktop_tmp=$(mktemp "$tmpdir/butter-paper-desktop-XXXXXX")
make_desktop() {
  BP_EXEC="$launcher" awk 'function esc(s, o,i,c) { for (i=1;i<=length(s);i++) { c=substr(s,i,1); if (c=="\\" || c=="\"") o=o "\\" c; else if (c=="%") o=o "%%"; else o=o c } return o } /^Exec=/ { printf "Exec=\"%s\" %%F\n", esc(ENVIRON["BP_EXEC"]); next } { print }' "$root/butter-paper.desktop" > "$1"
}
make_desktop "$desktop_tmp"
cmp -s "$desktop_tmp" "$desktop" || fail 'Desktop entry identity does not match this installation.'
cmp -s "$root/butter-paper.png" "$icon" || fail 'Icon identity does not match this installation.'
rm -f -- "$launcher_tmp" "$desktop_tmp"
trap - EXIT HUP INT TERM
for name in @PAYLOAD_WORDS@
do rm -f -- "$target/$name"; done
rmdir -- "$target" || fail 'Versioned install directory contains unrelated files; preserved it.'
rm -- "$launcher" "$desktop" "$icon"
if command -v update-desktop-database >/dev/null 2>&1 && [ -d "$data/applications" ]; then update-desktop-database "$data/applications"; fi
if command -v update-mime-database >/dev/null 2>&1 && [ -d "$data/mime" ]; then update-mime-database "$data/mime"; fi
printf '%s\n' 'Butter Paper desktop integration removed for this user.'
