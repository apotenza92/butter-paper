#!/bin/zsh
set -euo pipefail

script_dir="${0:A:h}"
project_dir="${script_dir:h}"
bundle_path="${project_dir}/target/GPUI Migration.app"
contents_path="${bundle_path}/Contents"

python3 "${project_dir}/scripts/run-native-bounded.py" cargo build \
    --locked \
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
cp "${project_dir:h}/.build-targets/gpui-migration/debug/gpui-migration" "${contents_path}/MacOS/gpui-migration"
cp "${project_dir:h}/.build-targets/gpui-migration/debug/butter-paper-pdf-worker" "${contents_path}/MacOS/butter-paper-pdf-worker"
cp "${pdfium_library}" "${contents_path}/Frameworks/libpdfium.dylib"
chmod 755 \
  "${contents_path}/MacOS/gpui-migration" \
  "${contents_path}/MacOS/butter-paper-pdf-worker"

python3 "${project_dir}/scripts/run-native-bounded.py" xcrun swiftc -swift-version 5 -O \
  "${project_dir}/native/SignatureCamera.swift" -o "${contents_path}/MacOS/butter-paper-signature-camera"

phone_project="${project_dir:h:h}/phone-signature-prototype"
python3 "${phone_project}/prepare.py"
cp "${phone_project}/dist/signature-prototype" "${contents_path}/MacOS/butter-paper-signature-phone"
chmod 755 "${contents_path}/MacOS/butter-paper-signature-phone"
mkdir -p "${contents_path}/Resources/Licenses"
cp "${phone_project}/dist/QRCP_LICENSE" "${contents_path}/Resources/Licenses/qrcp.txt"
cp "${phone_project}/dist/package/LICENSE" "${contents_path}/Resources/Licenses/signature-pad.txt"

plutil -lint "${contents_path}/Info.plist"
codesign --force --deep --sign - "${bundle_path}"
print "${bundle_path}"
