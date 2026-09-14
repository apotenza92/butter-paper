#!/bin/zsh
set -euo pipefail

script_dir="${0:A:h}"
project_dir="${script_dir:h}"
bundle_path="${project_dir}/target/GPUI Migration.app"
contents_path="${bundle_path}/Contents"

DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode-beta.app/Contents/Developer}" \
  cargo build \
    --manifest-path "${project_dir}/Cargo.toml" \
    --bin gpui-migration \
    --bin butter-paper-pdf-worker

pdfium_library="$(DEVELOPER_DIR="${DEVELOPER_DIR}" node "${project_dir}/scripts/fetch-pdfium-development.mjs")"

if [[ -e "${bundle_path}" ]]; then
  stale_bundle_dir="${project_dir}/target/.stale-bundles"
  mkdir -p "${stale_bundle_dir}"
  stale_bundle_stamp="$(date -u +%Y%m%dT%H%M%SZ)"
  mv "${bundle_path}" "${stale_bundle_dir}/GPUI Migration-${stale_bundle_stamp}.app"
fi
mkdir -p "${contents_path}/MacOS" "${contents_path}/Frameworks"
cp "${project_dir}/bundle/Info.plist" "${contents_path}/Info.plist"
cp "${project_dir}/target/debug/gpui-migration" "${contents_path}/MacOS/gpui-migration"
cp "${project_dir}/target/debug/butter-paper-pdf-worker" "${contents_path}/MacOS/butter-paper-pdf-worker"
cp "${pdfium_library}" "${contents_path}/Frameworks/libpdfium.dylib"
chmod 755 \
  "${contents_path}/MacOS/gpui-migration" \
  "${contents_path}/MacOS/butter-paper-pdf-worker"

plutil -lint "${contents_path}/Info.plist"
codesign --force --deep --sign - "${bundle_path}"
print "${bundle_path}"
