# Native development and GUI review

Read this before launching a development/review build or reviewing native UI.
Use the gpui-component and gpui-migration-ux-review skills alongside
[the UX checklist](../planning/ux-review.md). Read
[interactions.md](../planning/interactions.md) when canvas behaviour is involved,
including when skill discovery is unavailable.

## Isolation and running versions

Live GUI review uses `BP_NATIVE_DEVELOPMENT=1`, an isolated disposable
`BP_GPUI_DATA_DIR`, and a uniquely named review bundle. Never modify an installed
application or real profile. Keep bundles, captures and logs under ignored
`target/` directories; do not put machine-specific paths in tracked files.

Before opening an updated build, quit other Butter Paper versions and stop any
watcher that could respawn them. Preserve unsaved work through saving or verified
recovery; never discard it just to clean up. Verify that only the intended
testing version is running before handoff. Follow the launch, environment and
capture checks in the UX checklist; a shell environment assignment alone does
not prove that LaunchServices delivered it.

## Commands

- `cargo xtask pdfium` fetches the approved library and prints its path.
- Run with `BP_NATIVE_DEVELOPMENT=1 BP_GPUI_DATA_DIR=<disposable-root> BP_PDFIUM_LIBRARY=<approved-library> cargo run`.
- `cargo xtask dev [PDF...]` fetches PDFium, rebuilds and restarts when sources
  change, using `target/dev-profile`. Use `--release` for an optimised build.
  This development command does not replace the uniquely named review bundle
  required for live GUI review.
- Real-PDFium tests: `BP_PDFIUM_LIBRARY=<approved-library> cargo test <test-filter> -- --ignored`.
- `cargo xtask render-bench PDF` times how the worker opens and draws one
  large local PDF (thumbnails, page images, tiles, and a page asked for
  behind a backlog of thumbnails) and writes `target/render-bench`.
- `cargo xtask thumb-bench [--runs N] [--mode single|continuous] [--sweep PAGES] [--label NAME] PDF`
  benchmarks the thumbnails sidebar in a background review bundle: it
  flings the list, clicks the page it stops on and snaps the scroll bar to
  the end, timing until the rows shown have thumbnails and the clicked page
  shows and is sharp, while measuring main-thread delays, canvas flashes and
  memory. It writes each measure's median to
  `target/thumb-bench/LABEL.json`; compare labels before and after a change,
  on a quiet machine. macOS only.
- `cargo xtask pdf-corpus [--pages N] [--edits N] [--jobs N] [--limit N] DIR...`
  checks Edit PDF Content against a local folder of PDFs: its text against PDFium's
  characters, its outlines against rendered ink, no-op and typed edit
  round-trips, and timings. It reads the PDFs only, skips online-only cloud
  files and duplicates, and writes `report.md`, crops and an edited copy of
  each failed edit (`failures/`) under `target/pdf-corpus`. Reproduce a
  finding with a synthetic fixture before fixing it; never add the user's
  documents to the repository.

## Linux (GNOME on Wayland)

Xvfb draws only black windows (see [backlog.md](../planning/backlog.md)). On a
GNOME desktop, `scripts/gnome-headless/run.sh` instead runs a command as a
Wayland client of a private headless GNOME Shell with a virtual monitor,
independent of the logged-in desktop, the real screen and a closed lid; the
session ends with the command. Inside it, `shot.py OUT.png` captures the
monitor through Mutter's ScreenCast API, `key.py [KEYSYM_HEX]` presses a key
and `pointer.py move X Y` or `pointer.py drag X1 Y1 X2 Y2` moves the pointer
in monitor pixels, for operating-system input the review driver cannot send,
such as resizing a window from its edge (the hit zone reaches 4px outside
the border). They need `gnome-shell`, `python3-gi`, `gstreamer1.0-tools`,
`gstreamer1.0-pipewire`, `gstreamer1.0-plugins-base`,
`gstreamer1.0-plugins-good` and the user's PipeWire. The Edit PDF review:

```sh
cargo build --locked -p butter-paper --bins --features review-driver
review=$PWD/target/review; mkdir -p "$review"
cp crates/butter-paper/tests/fixtures/text-edit/libreoffice.pdf "$review/fixture.pdf"
BP_NATIVE_DEVELOPMENT=1 BP_GPUI_DATA_DIR="$review/profile" BP_PDFIUM_LIBRARY="$(cargo xtask pdfium)" \
BP_REVIEW_SCRIPT="$PWD/crates/butter-paper/tests/review/edit-pdf-content.txt" \
BP_REVIEW_OUT="$review/captures" BP_REVIEW_CAPTURE_CMD="$PWD/scripts/gnome-headless/shot.py {path}" \
timeout 300 scripts/gnome-headless/run.sh target/debug/butter-paper "$review/fixture.pdf"
```

`tests/review/word-table.txt`, run the same way against a copy of
`tests/fixtures/text-edit/word-table.pdf`, reviews an untagged report's cell
editing and Convert to Word Document.

To run `cargo xtask dev` on the logged-in desktop from an SSH shell, set
`WAYLAND_DISPLAY=wayland-0` (and `XDG_RUNTIME_DIR=/run/user/$(id -u)` if the
shell lacks it). GPUI uses the GPU the compositor draws on, through Vulkan;
`ZED_DEVICE_ID=0x<PCI device id>` picks another GPU, and pointing
`VK_DRIVER_FILES` at a missing file hides Vulkan to check OpenGL.

## Windows builds from Linux

`cargo xwin clippy|build --target x86_64-pc-windows-msvc` (cargo-xwin, with
`llvm-lib`, `lld-link` and `llvm-rc` on the path) checks and builds the
Windows app without Windows; `cargo xtask pdfium --target
x86_64-pc-windows-msvc` fetches its PDFium. A debug build run on another
Windows machine needs the VC++ runtime there and reads GPUI's shaders
(`gpui_windows/src/*.hlsl`) from the build machine's path, so copy them to
the same path on the Windows machine's `C:` drive; release builds embed
them but need the Windows SDK's `fxc.exe` to build. Start it on the
logged-in desktop (a scheduled task with `/it`), not from an SSH session.

## Review and handoff

Inspect the actual final screenshots in a separate contradiction-seeking pass,
including neighbouring controls and state transitions. Source checks and build
success do not certify rendered geometry, pointer input, keyboard access or
visual acceptance. Distinguish synthetic fixtures from real update, device and
package behaviour, and report unverified checks.

Keep reusable review lessons in [ux-review.md](../planning/ux-review.md) and open
defects in [backlog.md](../planning/backlog.md). Do not create chronological
worklogs or duplicate evidence trails in tracked Markdown.
