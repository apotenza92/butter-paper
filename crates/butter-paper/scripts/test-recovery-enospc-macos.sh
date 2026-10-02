#!/bin/sh
set -eu

project_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
filesystem=${BP_RECOVERY_ENOSPC_FILESYSTEM:-apfs}
case "$filesystem" in
  apfs) expected_filesystem=apfs ;;
  hfs) expected_filesystem=hfs ;;
  *)
    echo "BP_RECOVERY_ENOSPC_FILESYSTEM must be 'apfs' or 'hfs'" >&2
    exit 2
    ;;
esac

scratch=$(mktemp -d "${TMPDIR%/}/bp-recovery-enospc.XXXXXX")
scratch=$(CDPATH='' cd -- "$scratch" && pwd -P)
chmod 700 "$scratch"
image="$scratch/recovery-enospc.dmg"
mountpoint="$scratch/volume"
attach_plist="$scratch/attach.plist"
volume_info_plist="$scratch/volume-info.plist"
device_info="$scratch/device-info.txt"
mount_info="$scratch/mount-info.txt"
device=
nonce=$(/usr/bin/uuidgen | /usr/bin/tr '[:upper:]' '[:lower:]')
sentinel="$mountpoint/.bp-recovery-enospc-fixture-$nonce"

is_whole_disk_device() {
  case "$1" in
    /dev/disk*)
      disk_number=${1#/dev/disk}
      case "$disk_number" in
        ''|*[!0-9]*) return 1 ;;
        *) return 0 ;;
      esac
      ;;
    *) return 1 ;;
  esac
}

device_is_attached() {
  [ -n "$device" ] || return 1
  if ! /usr/bin/hdiutil info >"$device_info" 2>/dev/null; then
    return 2
  fi
  /usr/bin/awk -v expected="$device" \
    '$1 == expected { found = 1 } END { exit !found }' "$device_info"
}

mountpoint_is_mounted() {
  if ! /sbin/mount >"$mount_info" 2>/dev/null; then
    return 2
  fi
  /usr/bin/awk -v expected="$mountpoint" '
      index($0, " on " expected " (") != 0 { found = 1 }
      END { exit !found }
    ' "$mount_info"
}

device_is_absent() {
  if device_is_attached; then
    attachment_state=0
  else
    attachment_state=$?
  fi
  [ "$attachment_state" -eq 1 ]
}

mountpoint_is_unmounted() {
  if mountpoint_is_mounted; then
    mount_state=0
  else
    mount_state=$?
  fi
  [ "$mount_state" -eq 1 ]
}

recover_device_from_attach_plist() {
  if [ -z "$device" ] && [ -s "$attach_plist" ]; then
    device=$(/usr/bin/python3 - "$attach_plist" <<'PY'
import plistlib
import re
import sys

with open(sys.argv[1], "rb") as stream:
    plist = plistlib.load(stream)

whole_disks = [
    entity.get("dev-entry", "")
    for entity in plist.get("system-entities", [])
    if entity.get("content-hint") == "GUID_partition_scheme"
    and re.fullmatch(r"/dev/disk[0-9]+", entity.get("dev-entry", ""))
]
if len(whole_disks) == 1:
    print(whole_disks[0])
PY
    )
  fi
}

detach_device() {
  recover_device_from_attach_plist
  if [ -z "$device" ]; then
    mountpoint_is_unmounted
    return
  fi
  if ! /usr/bin/hdiutil detach "$device" >/dev/null 2>&1; then
    /bin/sleep 1
    if ! /usr/bin/hdiutil detach "$device" >/dev/null 2>&1; then
      /usr/bin/hdiutil detach -force "$device" >/dev/null 2>&1 || true
    fi
  fi
  if ! device_is_absent || ! mountpoint_is_unmounted; then
    return 1
  fi
  device=
}

cleanup() {
  status=$?
  trap - EXIT INT TERM HUP
  if ! detach_device; then
    echo "Disposable ENOSPC image cleanup did not complete." >&2
    echo "The image and scratch directory were preserved for safe recovery." >&2
    if [ -n "$device" ]; then
      printf 'Detach the exact device first: /usr/bin/hdiutil detach -force %s\n' \
        "$device" >&2
    else
      printf 'Inspect the exact image first: /usr/bin/hdiutil info -plist\n' >&2
    fi
    printf 'After it is detached, remove only: /bin/rm -rf -- %s\n' \
      "$scratch" >&2
    exit 1
  fi

  cleanup_files_failed=0
  /bin/rm -f -- "$attach_plist" "$volume_info_plist" "$device_info" \
    "$mount_info" "$image" || cleanup_files_failed=1
  if [ -e "$mountpoint" ]; then
    /bin/rmdir "$mountpoint" 2>/dev/null || cleanup_files_failed=1
  fi
  if [ -e "$scratch" ]; then
    /bin/rmdir "$scratch" 2>/dev/null || cleanup_files_failed=1
  fi
  if [ "$cleanup_files_failed" -ne 0 ] || [ -e "$image" ] ||
    [ -e "$mountpoint" ] || [ -e "$scratch" ]; then
    echo "The image detached, but its exact disposable files were not removed." >&2
    printf 'Inspect and remove only: /bin/rm -rf -- %s\n' "$scratch" >&2
    exit 1
  fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

mkdir -m 700 "$mountpoint"
case "$filesystem" in
  apfs)
    /usr/bin/hdiutil create -size 64m -type UDIF -layout GPTSPUD -fs APFS \
      -volname "BPENOSPC-$nonce" -nospotlight -ov "$image" >/dev/null
    ;;
  hfs)
    /usr/bin/hdiutil create -size 64m -type UDIF -layout NONE -ov "$image" >/dev/null
    /usr/bin/hdiutil attach -nomount -plist "$image" >"$attach_plist"
    recover_device_from_attach_plist
    is_whole_disk_device "$device" || {
      echo "could not resolve the exact disposable image device" >&2
      exit 1
    }
    raw="/dev/r${device#/dev/}"
    /sbin/newfs_hfs -J -U "$(id -u)" -G "$(id -g)" -M 700 \
      -v "BPENOSPC-$nonce" "$raw" >/dev/null
    if ! detach_device; then
      echo "could not detach the exact disposable image after formatting" >&2
      exit 1
    fi
    /bin/rm -f -- "$attach_plist"
    ;;
esac

/usr/bin/hdiutil attach -owners on -nobrowse -noautoopen \
  -mountpoint "$mountpoint" -plist "$image" >"$attach_plist"
recover_device_from_attach_plist
is_whole_disk_device "$device" || {
  echo "could not resolve the mounted disposable image device" >&2
  exit 1
}
device_is_attached || {
  echo "the captured disposable image device is not attached" >&2
  exit 1
}
mountpoint_is_mounted || {
  echo "the disposable image is not mounted at the exact isolated mount point" >&2
  exit 1
}

/usr/sbin/diskutil info -plist "$mountpoint" >"$volume_info_plist"
reported_filesystem=$(/usr/bin/plutil -extract FilesystemType raw -o - \
  "$volume_info_plist")
reported_mountpoint=$(/usr/bin/plutil -extract MountPoint raw -o - \
  "$volume_info_plist")
capacity_bytes=$(/usr/bin/plutil -extract TotalSize raw -o - \
  "$volume_info_plist")
case "$capacity_bytes" in
  ''|*[!0-9]*) echo "the disposable volume reported an invalid capacity" >&2; exit 1 ;;
esac
[ "$reported_filesystem" = "$expected_filesystem" ] || {
  echo "expected $expected_filesystem but mounted $reported_filesystem" >&2
  exit 1
}
[ "$reported_mountpoint" = "$mountpoint" ] || {
  echo "the disposable volume mounted at an unexpected path: $reported_mountpoint" >&2
  exit 1
}
[ "$capacity_bytes" -ge $((48 * 1024 * 1024)) ] &&
  [ "$capacity_bytes" -le $((80 * 1024 * 1024)) ] || {
    echo "refusing to fill a volume with unexpected capacity: $capacity_bytes bytes" >&2
    exit 1
  }

chmod 700 "$mountpoint"
(
  umask 077
  printf '%s\n' "$nonce" >"$sentinel"
)
[ "$(/bin/cat "$sentinel")" = "$nonce" ] || {
  echo "the disposable fixture nonce sentinel could not be verified" >&2
  exit 1
}
[ ! -L "$sentinel" ] && [ -f "$sentinel" ] || {
  echo "the disposable fixture nonce sentinel is not a regular file" >&2
  exit 1
}
sentinel_uid=$(/usr/bin/stat -f '%u' "$sentinel")
sentinel_links=$(/usr/bin/stat -f '%l' "$sentinel")
sentinel_mode=$(/usr/bin/stat -f '%Lp' "$sentinel")
[ "$sentinel_uid" = "$(id -u)" ] && [ "$sentinel_links" = 1 ] &&
  [ "$sentinel_mode" = 600 ] || {
    echo "the disposable fixture nonce sentinel has unsafe metadata" >&2
    exit 1
  }

echo "Disposable recovery ENOSPC fixture: filesystem=$reported_filesystem device=$device image=$image"
echo "Fixture nonce sentinel: $sentinel"

developer_dir=${DEVELOPER_DIR:-/Applications/Xcode-beta.app/Contents/Developer}
sdkroot=$(DEVELOPER_DIR="$developer_dir" /usr/bin/xcrun --sdk macosx --show-sdk-path)
clang=$(DEVELOPER_DIR="$developer_dir" /usr/bin/xcrun --sdk macosx --find clang)
clangxx=$(DEVELOPER_DIR="$developer_dir" /usr/bin/xcrun --sdk macosx --find clang++)

BP_RECOVERY_ENOSPC_MOUNT_ROOT="$mountpoint" \
BP_RECOVERY_ENOSPC_EXPECTED_FILESYSTEM="$reported_filesystem" \
BP_RECOVERY_ENOSPC_FIXTURE_NONCE="$nonce" \
BP_RECOVERY_ENOSPC_FIXTURE_SENTINEL="$sentinel" \
DEVELOPER_DIR="$developer_dir" \
SDKROOT="$sdkroot" \
CC="$clang" \
CXX="$clangxx" \
RUST_MIN_STACK=16777216 \
node "$project_dir/scripts/test-macos-window-harness.mjs" \
  --test document_workspace \
  real_enospc_initial_recovery_stream_blocks_edits_and_visible_retry_recovers \
  -- --ignored --exact --nocapture --test-threads=1
