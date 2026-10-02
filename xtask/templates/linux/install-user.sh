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
[ -x "$root/butter-paper" ] || fail 'Package executable is missing.'
[ -f "$root/MANIFEST.json" ] && [ -f "$root/butter-paper.desktop" ] && [ -f "$root/butter-paper.png" ] || fail 'Package integration files are missing.'
[ ! -e "$target" ] && [ ! -L "$target" ] && [ ! -e "$launcher" ] && [ ! -L "$launcher" ] && [ ! -e "$desktop" ] && [ ! -L "$desktop" ] && [ ! -e "$icon" ] && [ ! -L "$icon" ] || fail 'Butter Paper files already exist; remove the owned installation first.'
mkdir -p "$data/butter-paper" "$data/applications" "$data/icons/hicolor/1024x1024/apps" "$bin"
stage= launcher_tmp= desktop_tmp= icon_tmp= owner_token=
owned_target=0 owned_launcher=0 owned_desktop=0 owned_icon=0
rollback() {
  result=$?
  trap - EXIT HUP INT TERM
  set +e
  changed=0
  if [ -n "${stage:-}" ] && [ -d "$stage" ]; then rm -rf -- "$stage"; fi
  if [ "${owned_icon:-0}" -eq 1 ] && [ -f "${icon_tmp:-}" ] && [ "$icon_tmp" -ef "$icon" ] && cmp -s "$root/butter-paper.png" "$icon"; then rm -f -- "$icon"; changed=1; fi
  if [ "${owned_desktop:-0}" -eq 1 ] && [ -f "${desktop_tmp:-}" ] && [ "$desktop_tmp" -ef "$desktop" ] && cmp -s "$desktop_tmp" "$desktop"; then rm -f -- "$desktop"; changed=1; fi
  if [ "${owned_launcher:-0}" -eq 1 ] && [ -f "${launcher_tmp:-}" ] && [ "$launcher_tmp" -ef "$launcher" ] && cmp -s "$launcher_tmp" "$launcher"; then rm -f -- "$launcher"; changed=1; fi
  if [ "${owned_target:-0}" -eq 1 ] || { [ -n "${owner_token:-}" ] && [ -f "$target/.bp-owner" ] && [ "$(cat -- "$target/.bp-owner")" = "$owner_token" ]; }; then
    for name in @PAYLOAD_WORDS@
    do if cmp -s "$root/$name" "$target/$name"; then rm -f -- "$target/$name"; changed=1; fi; done
    rm -f -- "$target/.bp-owner"
    rmdir -- "$target" 2>/dev/null || :
  fi
  if [ "$changed" -eq 1 ]; then
    if command -v update-desktop-database >/dev/null 2>&1 && [ -d "$data/applications" ]; then update-desktop-database "$data/applications" || :; fi
    if command -v update-mime-database >/dev/null 2>&1 && [ -d "$data/mime" ]; then update-mime-database "$data/mime" || :; fi
  fi
  rm -f -- "${launcher_tmp:-}" "${desktop_tmp:-}" "${icon_tmp:-}"
  exit "$result"
}
trap rollback EXIT
trap 'exit 1' HUP INT TERM
stage=$(mktemp -d "$data/butter-paper/.install-XXXXXX")
for name in @PAYLOAD_WORDS@
do
  if [ -f "$root/$name" ] && [ ! -L "$root/$name" ]; then cp -p -- "$root/$name" "$stage/$name"; else fail 'Package payload is missing or unsafe.'; fi
done
owner_token=${stage##*/}
printf '%s' "$owner_token" > "$stage/.bp-owner"
# Atomic mv -T -- "$stage" "$target" with -n prevents replacing a raced-in path.
mv -T -n -- "$stage" "$target"
[ ! -e "$stage" ] || fail 'Versioned install path appeared during installation.'
owned_target=1
rm -f -- "$target/.bp-owner"
launcher_tmp=$(mktemp "$bin/.butter-paper-launcher-XXXXXX")
make_launcher() {
  quoted=$(printf '%s' "$target/butter-paper" | sed "s/'/'\\''/g")
  printf '#!/bin/sh\nexec '\''%s'\'' "$@"\n' "$quoted" > "$1"
  chmod 0755 "$1"
}
make_launcher "$launcher_tmp"
ln -- "$launcher_tmp" "$launcher"
owned_launcher=1
desktop_tmp=$(mktemp "$data/applications/.butter-paper-desktop-XXXXXX")
make_desktop() {
  BP_EXEC="$launcher" awk 'function esc(s, o,i,c) { for (i=1;i<=length(s);i++) { c=substr(s,i,1); if (c=="\\" || c=="\"") o=o "\\" c; else if (c=="%") o=o "%%"; else o=o c } return o } /^Exec=/ { printf "Exec=\"%s\" %%F\n", esc(ENVIRON["BP_EXEC"]); next } { print }' "$root/butter-paper.desktop" > "$1"
}
make_desktop "$desktop_tmp"
chmod 0644 "$desktop_tmp"
ln -- "$desktop_tmp" "$desktop"
owned_desktop=1
icon_tmp=$(mktemp "$data/icons/hicolor/1024x1024/apps/.butter-paper-icon-XXXXXX")
cp -p -- "$root/butter-paper.png" "$icon_tmp"
ln -- "$icon_tmp" "$icon"
owned_icon=1
if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database "$data/applications"; fi
if command -v update-mime-database >/dev/null 2>&1 && [ -d "$data/mime" ]; then update-mime-database "$data/mime"; fi
rm -f -- "$launcher_tmp" "$desktop_tmp" "$icon_tmp"
trap - EXIT HUP INT TERM
printf '%s\n' 'Butter Paper installed for this user. Select it as the default PDF application in your desktop settings if desired.'
