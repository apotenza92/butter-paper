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
maximum_glibc='2.35'
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

assert_elf_contract() {
  local root="$1"
  local label="$2"
  local elf_count=0
  local dependency_failures=''
  local glibc_failures=''
  local packaged_library_path
  packaged_library_path="$(
    find "$root" -type f \( -name '*.so' -o -name '*.so.*' \) -printf '%h\n' |
      sort -u |
      paste -sd: -
  )"
  while IFS= read -r -d '' candidate; do
    if ! file "$candidate" | grep -F 'ELF ' >/dev/null; then
      continue
    fi
    elf_count=$((elf_count + 1))
    local required
    required="$(
      readelf --version-info "$candidate" 2>/dev/null |
        grep -oE 'GLIBC_[0-9]+\.[0-9]+' |
        sed 's/^GLIBC_//' |
        sort -Vu |
        tail -n 1 || true
    )"
    if [[ -n "$required" ]] &&
      [[ "$(printf '%s\n%s\n' "$maximum_glibc" "$required" | sort -V | tail -n 1)" != "$maximum_glibc" ]]; then
      glibc_failures+="$candidate requires GLIBC_$required"$'\n'
    fi
    if readelf -d "$candidate" 2>/dev/null | grep -F '(NEEDED)' >/dev/null; then
      local closure
      closure="$(
        LD_LIBRARY_PATH="${packaged_library_path}${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
          ldd "$candidate" 2>&1 || true
      )"
      if grep -F 'not found' <<<"$closure" >/dev/null; then
        # electron-builder's AppImage runtime includes optional desktop-integration
        # shims under usr/lib. They are not loaded by Butter Paper itself; validate
        # the application executable and native modules without treating unused
        # GTK2/GConf compatibility shims as application dependency failures.
        if [[ "$label" != AppImage || "$candidate" != "$root/usr/lib/"* ]]; then
          dependency_failures+="$candidate"$'\n'"$closure"$'\n'
        fi
      fi
    fi
  done < <(find "$root" -type f -print0)
  [[ $elf_count -gt 0 ]] || {
    echo "$label contains no inspectable ELF binaries" >&2
    exit 1
  }
  [[ -z "$glibc_failures" ]] || {
    echo "$label exceeds the supported GLIBC_$maximum_glibc contract:" >&2
    printf '%s' "$glibc_failures" >&2
    exit 1
  }
  [[ -z "$dependency_failures" ]] || {
    echo "$label has unresolved ELF dependencies:" >&2
    printf '%s' "$dependency_failures" >&2
    exit 1
  }
}

assert_desktop_integration() {
  local root="$1"
  local label="$2"
  local desktop
  desktop="$(find "$root" -type f -name "$executable_name.desktop" -print -quit)"
  [[ -n "$desktop" ]] || {
    echo "$label is missing $executable_name.desktop" >&2
    exit 1
  }
  desktop-file-validate "$desktop"
  grep -E '^Exec=.*'"$executable_name" "$desktop" >/dev/null
  grep -E '^MimeType=.*application/pdf' "$desktop" >/dev/null
  local icon
  icon="$(sed -n 's/^Icon=//p' "$desktop" | head -n 1)"
  [[ -n "$icon" ]] || {
    echo "$label desktop entry has no icon" >&2
    exit 1
  }
  find "$root" -type f -path "*/share/icons/*/apps/$icon.*" -print -quit |
    grep -q . || {
      echo "$label desktop icon is missing: $icon" >&2
      exit 1
    }
}

assert_update_contract() {
  local executable
  executable="$(realpath "$1")"
  local update_config
  update_config="$(dirname "$executable")/resources/app-update.yml"
  [[ -f "$update_config" ]] || {
    echo "Linux package is missing TUF-gated updater configuration: $update_config" >&2
    exit 1
  }
  local expected_feed="https://raw.githubusercontent.com/apotenza92/butter-paper/updates/$channel/linux/$arch"
  grep -F 'provider: generic' "$update_config" >/dev/null
  grep -F "url: $expected_feed" "$update_config" >/dev/null
  local trust_root
  trust_root="$(dirname "$executable")/resources/update-trust/root.json"
  [[ -f "$trust_root" ]] || {
    echo "Linux package is missing the reviewed TUF root: $trust_root" >&2
    exit 1
  }
  ! grep -F 'PRIVATE KEY' "$trust_root" >/dev/null || {
    echo "Linux package contains private TUF key material: $trust_root" >&2
    exit 1
  }
}

smoke_executable() {
  local executable="$1"
  local label="$2"
  assert_native_executable "$(realpath "$executable")"
  assert_update_contract "$executable"
  BP_ELECTRON_EXECUTABLE_PATH="$executable" \
    BP_RELEASE_CHANNEL="$channel" \
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
assert_update_contract "$appimage_executable"
assert_elf_contract "$work_root/appimage/squashfs-root" AppImage
assert_desktop_integration "$work_root/appimage/squashfs-root" AppImage
BP_ELECTRON_EXECUTABLE_PATH="$app_run" \
  BP_RELEASE_CHANNEL="$channel" \
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
deb_install_root="$(dirname "$(realpath "$deb_executable")")"
assert_elf_contract "$deb_install_root" DEB
assert_desktop_integration /usr DEB
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
assert_elf_contract "$work_root/rpm" RPM
assert_desktop_integration "$work_root/rpm" RPM
smoke_executable "${rpm_executables[0]}" rpm

echo "Linux $channel/$arch AppImage, DEB, and RPM package-boundary verification passed."
