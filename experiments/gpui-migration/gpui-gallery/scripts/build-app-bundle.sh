#!/bin/zsh
set -euo pipefail

script_dir="${0:A:h}"
project_dir="${script_dir:h}"
bundle_path="${project_dir}/target/Butter Paper GPUI.app"
contents_path="${bundle_path}/Contents"

TOOLCHAINS=com.apple.dt.toolchain.Metal.32023.917 \
DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer \
  cargo build --manifest-path "${project_dir}/Cargo.toml"

# This bundle is disposable benchmark output. Start from a clean directory so
# renamed executables or stale resources cannot confuse Launch Services.
rm -rf "${bundle_path}"
mkdir -p "${contents_path}/MacOS" "${contents_path}/Resources"
cp "${project_dir}/bundle/Info.plist" "${contents_path}/Info.plist"
cp "${project_dir}/target/debug/butter-paper-gpui-gallery" "${contents_path}/MacOS/ButterPaperGPUI"
chmod 755 "${contents_path}/MacOS/ButterPaperGPUI"
ditto "${project_dir}/assets" "${contents_path}/Resources/assets"

pdfinfo_command="$(command -v pdfinfo)"
poppler_source="$(cd "${pdfinfo_command:h}/../../native/poppler" 2>/dev/null && pwd || true)"
if [[ -x "${poppler_source}/bin/pdfinfo" && -x "${poppler_source}/bin/pdftoppm" ]]; then
  ditto "${poppler_source}" "${contents_path}/Resources/poppler"
else
  print -u2 "Poppler bundle source was not found. Set BP_POPPLER_BIN_DIR when launching the app."
fi

plutil -lint "${contents_path}/Info.plist"
codesign --force --deep --sign - "${bundle_path}"
print "${bundle_path}"
