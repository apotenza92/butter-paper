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

## Review and handoff

Inspect the actual final screenshots in a separate contradiction-seeking pass,
including neighbouring controls and state transitions. Source checks and build
success do not certify rendered geometry, pointer input, keyboard access or
visual acceptance. Distinguish synthetic fixtures from real update, device and
package behaviour, and report unverified checks.

Keep reusable review lessons in [ux-review.md](../planning/ux-review.md) and open
defects in [backlog.md](../planning/backlog.md). Do not create chronological
worklogs or duplicate evidence trails in tracked Markdown.
