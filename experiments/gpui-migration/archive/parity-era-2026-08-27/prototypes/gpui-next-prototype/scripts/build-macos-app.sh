#!/usr/bin/env bash
set -euo pipefail

prototype_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bundle_root="${prototype_root}/target/Butter Paper GPUI Next.app"
contents_root="${bundle_root}/Contents"
binary_name="butter-paper-gpui-next-prototype"

cargo build --locked --manifest-path "${prototype_root}/Cargo.toml"

mkdir -p "${contents_root}/MacOS" "${contents_root}/Resources"
cp "${prototype_root}/target/debug/${binary_name}" "${contents_root}/MacOS/ButterPaperGPUINext"

cat > "${contents_root}/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>ButterPaperGPUINext</string>
  <key>CFBundleIdentifier</key><string>dev.butterpaper.gpui-next-prototype</string>
  <key>CFBundleName</key><string>Butter Paper GPUI Next</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.0.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

echo "${bundle_root}"
