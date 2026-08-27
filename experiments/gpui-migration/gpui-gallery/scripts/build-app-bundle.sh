#!/bin/zsh
set -euo pipefail

script_dir="${0:A:h}"
project_dir="${script_dir:h}"
bundle_path="${project_dir}/target/Butter Paper GPUI.app"
contents_path="${bundle_path}/Contents"

TOOLCHAINS=com.apple.dt.toolchain.Metal.32023.917 \
DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer \
  cargo build \
    --manifest-path "${project_dir}/Cargo.toml" \
    --bin butter-paper-gpui-gallery \
    --bin butter-paper-pdf-worker \
    --features gallery,pdfium-worker

# This fetches a checksum-pinned prototype supplier artifact into ignored
# target output. It is not the production PDFium supply chain.
pdfium_library="$(node "${project_dir}/scripts/fetch-pdfium-development.mjs")"

# This bundle is disposable benchmark output. Start from a clean directory so
# renamed executables or stale resources cannot confuse Launch Services.
rm -rf "${bundle_path}"
mkdir -p "${contents_path}/MacOS" "${contents_path}/Resources"
cp "${project_dir}/bundle/Info.plist" "${contents_path}/Info.plist"
cp "${project_dir}/target/debug/butter-paper-gpui-gallery" "${contents_path}/MacOS/ButterPaperGPUI"
cp "${project_dir}/target/debug/butter-paper-pdf-worker" "${contents_path}/MacOS/butter-paper-pdf-worker"
chmod 755 \
  "${contents_path}/MacOS/ButterPaperGPUI" \
  "${contents_path}/MacOS/butter-paper-pdf-worker"
ditto "${project_dir}/assets" "${contents_path}/Resources/assets"
mkdir -p "${contents_path}/Resources/pdfium"
cp "${pdfium_library}" "${contents_path}/Resources/pdfium/libpdfium.dylib"

plutil -lint "${contents_path}/Info.plist"
codesign --force --deep --sign - "${bundle_path}"
print "${bundle_path}"
