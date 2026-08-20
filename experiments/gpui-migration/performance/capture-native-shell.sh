#!/bin/zsh
set -euo pipefail

if [[ $# -lt 1 ]]; then
  print -u2 "Usage: capture-native-shell.sh <hibbeler.pdf> [capture-prefix]"
  exit 2
fi

BP_PDF_PATH=$1
BP_MIGRATION_DIR=${0:A:h:h}
BP_GALLERY_DIR="$BP_MIGRATION_DIR/gpui-gallery"
BP_BINARY="$BP_GALLERY_DIR/target/Butter Paper GPUI.app/Contents/MacOS/ButterPaperGPUI"
BP_CAPTURE_PREFIX=${2:-"$BP_MIGRATION_DIR/captures/gpui-native-current"}
BP_LOG_PATH="$BP_MIGRATION_DIR/performance/results/gpui-native-capture.log"
BP_CACHE_DIR="$BP_MIGRATION_DIR/performance/results/.native-capture-cache"

if [[ ! -f "$BP_PDF_PATH" ]]; then
  print -u2 "PDF not found: $BP_PDF_PATH"
  exit 2
fi

if [[ ! -x "$BP_BINARY" ]]; then
  print -u2 "GPUI bundle executable not found: $BP_BINARY"
  exit 2
fi

env \
  BP_GPUI_CAPTURE_SHELL=1 \
  BP_GPUI_ZOOM=194 \
  BP_GPUI_CACHE_DIR="$BP_CACHE_DIR" \
  TOOLCHAINS=com.apple.dt.toolchain.Metal.32023.917 \
  DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer \
  "$BP_BINARY" \
  -ApplePersistenceIgnoreState YES \
  "$BP_PDF_PATH" \
  >"$BP_LOG_PATH" 2>&1 &
BP_APP_PID=$!

function cleanup_capture_process() {
  kill "$BP_APP_PID" 2>/dev/null || true
  wait "$BP_APP_PID" 2>/dev/null || true
}
trap cleanup_capture_process EXIT INT TERM

BP_WINDOW_ID=""
for BP_ATTEMPT in {1..30}; do
  BP_WINDOW_ID=$(swift -e '
    import CoreGraphics
    import Foundation
    let pid = Int32(CommandLine.arguments[1])!
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID)
      as? [[String: Any]] ?? []
    for window in windows {
      let ownerPid = window[kCGWindowOwnerPID as String] as? Int32
      let number = window[kCGWindowNumber as String] as? UInt32
      let bounds = window[kCGWindowBounds as String] as? [String: Any]
      let width = bounds?["Width"] as? Int
      let height = bounds?["Height"] as? Int
      if ownerPid == pid, width == 1200, height == 800, let number {
        print(number)
        break
      }
    }
  ' "$BP_APP_PID")
  [[ -n "$BP_WINDOW_ID" ]] && break
  sleep 1
done

if [[ -z "$BP_WINDOW_ID" ]]; then
  print -u2 "No on-screen 1200x800 GPUI window appeared for PID $BP_APP_PID"
  exit 1
fi

# Let the first PDF page and visible thumbnail queue settle before capture.
sleep 5
screencapture -x -o -l "$BP_WINDOW_ID" "$BP_CAPTURE_PREFIX-window-1200x800.png"
sips -z 768 1152 \
  "$BP_CAPTURE_PREFIX-window-1200x800.png" \
  --out "$BP_CAPTURE_PREFIX-1152x768.png" >/dev/null

print "Captured GPUI window $BP_WINDOW_ID for PID $BP_APP_PID"
print "$BP_CAPTURE_PREFIX-window-1200x800.png"
print "$BP_CAPTURE_PREFIX-1152x768.png"
