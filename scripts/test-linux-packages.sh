#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo 'Usage: test-linux-packages.sh <release-directory> <stable|beta> <x64|arm64>' >&2
  exit 2
fi

release_directory="$(realpath "$1")"
channel="$2"
arch="$3"
if [[ "$channel" != stable && "$channel" != beta ]]; then
  echo "Unsupported channel: $channel" >&2
  exit 2
fi
if [[ "$arch" != x64 && "$arch" != arm64 ]]; then
  echo "Unsupported architecture: $arch" >&2
  exit 2
fi

prefix='Butter-Paper'
executable_name='butter-paper'
if [[ "$channel" == beta ]]; then
  prefix='Butter-Paper-Beta'
  executable_name='butter-paper-beta'
fi

expected_uname='x86_64'
expected_file='x86-64'
expected_deb='amd64'
expected_rpm='x86_64'
if [[ "$arch" == arm64 ]]; then
  expected_uname='aarch64'
  expected_file='ARM aarch64'
  expected_deb='arm64'
  expected_rpm='aarch64'
fi
if [[ "$(uname -m)" != "$expected_uname" ]]; then
  echo "Package verification requires native $expected_uname, received $(uname -m)" >&2
  exit 1
fi

appimage="$release_directory/$prefix-Linux-$arch.AppImage"
deb="$release_directory/$prefix-Linux-$arch.deb"
rpm="$release_directory/$prefix-Linux-$arch.rpm"
for artifact in "$appimage" "$deb" "$rpm"; do
  [[ -f "$artifact" ]] || { echo "Missing exact Linux package: $artifact" >&2; exit 1; }
done

work_root="$(mktemp -d "${RUNNER_TEMP:-/tmp}/butter-paper-linux-packages.XXXXXX")"
installed_deb_package=''
cleanup() {
  if [[ -n "$installed_deb_package" ]]; then
    sudo apt-get purge -y "$installed_deb_package" >/dev/null 2>&1 || true
  fi
  rm -rf -- "$work_root"
}
trap cleanup EXIT

assert_native_executable() {
  local executable="$1"
  [[ -x "$executable" ]] || { echo "Executable is missing or not executable: $executable" >&2; exit 1; }
  file "$executable" | grep -F "$expected_file" >/dev/null
}

assert_no_update_config() {
  local executable
  executable="$(realpath "$1")"
  local update_config
  update_config="$(dirname "$executable")/resources/app-update.yml"
  [[ ! -e "$update_config" ]] || {
    echo "Unsigned Linux package unexpectedly contains updater configuration: $update_config" >&2
    exit 1
  }
}

smoke_executable() {
  local executable="$1"
  local label="$2"
  assert_native_executable "$(realpath "$executable")"
  assert_no_update_config "$executable"
  BP_ELECTRON_EXECUTABLE_PATH="$executable" \
    BP_TEST_USER_DATA_DIR="$work_root/user-data-$label" \
    xvfb-run -a pnpm test:package:desktop
}

# AppImage: exercise the published filesystem boundary and launch its AppRun entrypoint.
chmod 755 "$appimage"
mkdir "$work_root/appimage"
(
  cd "$work_root/appimage"
  "$appimage" --appimage-extract >/dev/null
)
app_run="$work_root/appimage/squashfs-root/AppRun"
[[ -x "$app_run" ]] || { echo 'AppImage extraction did not produce AppRun' >&2; exit 1; }
appimage_executable="$(find "$work_root/appimage/squashfs-root" -type f -name "$executable_name" -perm -111 -print -quit)"
[[ -n "$appimage_executable" ]] || { echo 'AppImage does not contain the application executable' >&2; exit 1; }
assert_native_executable "$appimage_executable"
assert_no_update_config "$appimage_executable"
BP_ELECTRON_EXECUTABLE_PATH="$app_run" \
  BP_TEST_USER_DATA_DIR="$work_root/user-data-appimage" \
  xvfb-run -a pnpm test:package:desktop

# DEB: prove declared architecture, install through apt, launch the installed app, then purge it.
[[ "$(dpkg-deb --field "$deb" Architecture)" == "$expected_deb" ]] || {
  echo 'DEB declares the wrong architecture' >&2
  exit 1
}
installed_deb_package="$(dpkg-deb --field "$deb" Package)"
[[ -n "$installed_deb_package" ]] || { echo 'DEB package name is empty' >&2; exit 1; }
sudo apt-get install -y "$deb"
deb_executable="/usr/bin/$installed_deb_package"
smoke_executable "$deb_executable" deb
sudo apt-get purge -y "$installed_deb_package"
if dpkg-query -W -f='${db:Status-Abbrev}' "$installed_deb_package" 2>/dev/null | grep -q '^ii'; then
  echo "DEB package remains installed: $installed_deb_package" >&2
  exit 1
fi
[[ ! -e "$deb_executable" ]] || { echo "DEB executable remains after purge: $deb_executable" >&2; exit 1; }
installed_deb_package=''

# RPM: validate metadata, extract the exact public package, and launch from its packaged layout.
[[ "$(rpm -qp --qf '%{ARCH}' "$rpm")" == "$expected_rpm" ]] || {
  echo 'RPM declares the wrong architecture' >&2
  exit 1
}
mkdir "$work_root/rpm"
rpm2cpio "$rpm" | (cd "$work_root/rpm" && cpio -idm --quiet)
mapfile -t rpm_executables < <(find "$work_root/rpm" -type f -name "$executable_name" -perm -111 -print)
[[ ${#rpm_executables[@]} -eq 1 ]] || {
  echo "RPM must contain exactly one $executable_name executable; found ${#rpm_executables[@]}" >&2
  exit 1
}
smoke_executable "${rpm_executables[0]}" rpm

echo "Linux $channel/$arch AppImage, DEB, and RPM package-boundary verification passed."
