#!/bin/zsh
set -euo pipefail

script_dir="${0:A:h}"
project_dir="${script_dir:h}"
bundle_path="${project_dir}/target/GPUI Migration.app"
contents_path="${bundle_path}/Contents"
target_dir="${BP_NATIVE_TARGET_DIR:-${project_dir:h}/.build-targets/gpui-migration}"
python_bin="${BP_PYTHON_BIN:-python3}"
minimum_macos_version="$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "${project_dir}/bundle/Info.plist")"
native_arch="$(uname -m)"

# Cargo build scripts invoke `cc` directly, so DEVELOPER_DIR alone does not
# prevent the standalone Command Line Tools SDK from leaking into the link.
# Resolve the compiler and SDK through the same selected Xcode toolchain used
# by xcrun, while preserving explicit caller overrides.
export SDKROOT="${SDKROOT:-$(xcrun --sdk macosx --show-sdk-path)}"
export CC="${CC:-$(xcrun --sdk macosx --find clang)}"
export CXX="${CXX:-$(xcrun --sdk macosx --find clang++)}"

case "${native_arch}" in
  arm64|x86_64) ;;
  *)
    print -u2 "Unsupported macOS build architecture: ${native_arch}"
    exit 1
    ;;
esac

node "${project_dir}/scripts/verify-cargo-graph.mjs"

"${python_bin}" "${project_dir}/scripts/run-native-bounded.py" cargo build \
    --locked \
    --features development-pdfium-override \
    --target-dir "${target_dir}" \
    --manifest-path "${project_dir}/Cargo.toml" \
    --bin gpui-migration \
    --bin butter-paper-pdf-worker

pdfium_library="$(node "${project_dir}/scripts/fetch-pdfium-development.mjs")"

if [[ -e "${bundle_path}" ]]; then
  stale_bundle_dir="${project_dir}/target/.stale-bundles"
  mkdir -p "${stale_bundle_dir}"
  stale_bundle_stamp="$(date -u +%Y%m%dT%H%M%SZ)"
  mv "${bundle_path}" "${stale_bundle_dir}/GPUI Migration-${stale_bundle_stamp}.app"
fi
mkdir -p "${contents_path}/MacOS" "${contents_path}/Frameworks"
cp "${project_dir}/bundle/Info.plist" "${contents_path}/Info.plist"
cp "${target_dir}/debug/gpui-migration" "${contents_path}/MacOS/gpui-migration"
cp "${target_dir}/debug/butter-paper-pdf-worker" "${contents_path}/MacOS/butter-paper-pdf-worker"
cp "${pdfium_library}" "${contents_path}/Frameworks/libpdfium.dylib"
chmod 755 \
  "${contents_path}/MacOS/gpui-migration" \
  "${contents_path}/MacOS/butter-paper-pdf-worker"

"${python_bin}" "${project_dir}/scripts/run-native-bounded.py" env \
  CARGO_TARGET_DIR="${target_dir}" xcrun swiftc -swift-version 5 -O \
  -target "${native_arch}-apple-macos${minimum_macos_version}" \
  "${project_dir}/native/SignatureCamera.swift" -o "${contents_path}/MacOS/butter-paper-signature-camera"

phone_project="${project_dir:h:h}/phone-signature-prototype"
"${python_bin}" "${phone_project}/prepare.py"
cp "${phone_project}/dist/signature-prototype" "${contents_path}/MacOS/butter-paper-signature-phone"
chmod 755 "${contents_path}/MacOS/butter-paper-signature-phone"
mkdir -p "${contents_path}/Resources/Licenses"
cp "${phone_project}/dist/QRCP_LICENSE" "${contents_path}/Resources/Licenses/qrcp.txt"
cp "${phone_project}/dist/package/LICENSE" "${contents_path}/Resources/Licenses/signature-pad.txt"
cp "${project_dir}/assets/fonts/Allura-OFL.txt" "${contents_path}/Resources/Licenses/allura-font.txt"
cp "${project_dir}/assets/fonts/Arimo-OFL.txt" "${contents_path}/Resources/Licenses/arimo-font.txt"
cp "${project_dir}/assets/fonts/RobotoMono-OFL.txt" "${contents_path}/Resources/Licenses/roboto-mono-font.txt"
cp "${project_dir}/assets/fonts/Tinos-OFL.txt" "${contents_path}/Resources/Licenses/tinos-font.txt"
cp "${project_dir}/assets/fonts/Expo-Google-Fonts-MIT.txt" \
  "${contents_path}/Resources/Licenses/expo-google-fonts.txt"
cp "${project_dir}/.prepared/gpui-component-c27f5d5c/crates/story-web/fonts/OFL.txt" \
  "${contents_path}/Resources/Licenses/noto-fonts.txt"

plutil -lint "${contents_path}/Info.plist"
codesign --force --deep --sign - "${bundle_path}"
print "${bundle_path}"
