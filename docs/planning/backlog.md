# Backlog

Open work, roughly in priority order. Remove an item when it is done.

## Confirm on screen

Remaining runtime and platform checks. Source and native harness coverage
alone do not establish these behaviours.

- Physical trackpad pinch/scroll delivery on Windows and Linux. The X11
  GPUI backend reports both trackpads and mouse wheels as line deltas, so
  separating their preferences there needs device-source support in the fork.
- Installed update download/restart on macOS, Windows and Linux: preserve
  open documents, view positions and unsaved work, and check relaunch focus.
- Phone signing with a physical phone (the relay is verified with synthetic
  transfers only), and camera signature capture: permission, capture,
  cancellation and camera release.

## Interface

- Stock popup menus initially select a heading on the first Down press, so
  titled mouse-wheel menus need a second Down to reach Zoom. Fix initial and
  wraparound selection to skip non-actionable rows in gpui-kit.
- Stock checkboxes, switches, toggles, buttons and inputs do not expose a
  disabled state to accessibility (the snapping "Show grid" switch and spacing
  fields report enabled while greyed out). Deferred: the fix belongs in
  gpui-kit's base controls, not in Butter Paper.
- The template library offers both a top-right X and a Done button that do
  the same thing; decide whether to drop Done (the X is the convention for
  transient panels).

## Editing

- Elliptical Arcs import and render but are not editable as ellipses.
- Properties the reference had but native lacks: separate fill opacity,
  hatch fill, text vertical alignment and auto-size, image borders, editing
  imported appearances.
- Pdf-format follow-ups are listed in [pdf-format.md](pdf-format.md#known-differences-follow-ups).
- Undo after Save does nothing in the review build (Rectangles too): the
  Undo action reports no error. Check whether history should survive a save.

## Edit PDF Content

Paragraph editing works and is checked against real Chrome, LibreOffice
and Quartz output (`tests/page_text_corpus.rs`; see [interactions.md](interactions.md#editing-page-text)
and [pdf-format.md](pdf-format.md#edited-page-text)); the target is in
[decisions.md](decisions.md#interface). Each step below ends
with `cargo test --workspace`, `cargo xtask check`, the real-PDFium
`--ignored` worker tests and, for visible changes, a native review with
inspected captures. Steps are in priority order.

1. **Linux review workflow.** Edit PDF Content, lists, columns and edge resize
    pass the native review on Windows and on Linux under WSLg. The manual
    `Review` workflow (`.github/workflows/review.yml`) runs Linux under Xvfb
    with software Vulkan, which drew only black windows under WSL (with GL
    too). Run the workflow once pushed and check its Linux captures.

## Platforms and release

- Remove the unused local phone helper (`local_phone_signature`,
  `xtask/src/phone.rs`, `crates/butter-paper/phone-helper/`, its packaging and
  notices) once relay phone signing is confirmed on a phone.
- `release.yml`: bump `actions/create-github-app-token`, which still targets
  Node.js 20.
- Windows: qualify the native save and file pickers by hand.
- Windows: the real-PDFium (`--ignored`) tests are not maintained there; many
  build fixture paths without canonicalising them.
- macOS native window tabs have no visible tab bar (GPUI leaves drawing it to
  the app, and Butter Paper does not), so Window ▸ Merge All Windows hides
  every window but one. New Window and torn-off tabs open as separate windows
  to avoid this; either draw a window tab bar or drop the tabbing identifier.
- Linux: qualify production graphics (OpenGL, window resize).
- macOS 12 is unsupported (minimum 13.0); supporting it needs an older Go
  toolchain for the phone helper and a real macOS 12 run.
- Performance qualification (startup, memory, rendering latency, leaks) has
  no maintained protocol yet beyond the large-document benchmarks
  (`cargo xtask render-bench`, `cargo xtask thumb-bench`; see
  [native-review.md](../agents/native-review.md)).
- Large documents: a document's one PDF worker draws one page at a time, so
  a heavy page's first draw (up to about a second) still holds up the next
  request. A second worker for thumbnails would remove that, but each worker
  holds about five times the file in memory (1.5 GB for a 290 MB file: the
  bytes twice and two parsed copies), which should shrink first. Opening also
  loads every page in PDFium to check its boxes against the parsed page
  dictionary (0.45 s for 66 heavy pages), and thumbnails are drawn again on
  every open rather than cached.
- Snapshot cuts from the whole-page image of the page last navigated to; on
  another page (Continuous view after scrolling) it now reports the image
  unavailable instead of cutting the wrong page, and should render one.

## Product requests

- PDF text search with result navigation, later scanned-image search (#36).
- Scriptable CLI sharing the app's document, annotation and export
  capabilities, possibly with an MCP interface (#37).
- In-app agent chat with cloud and local models (#38).
- Popup notes: currently preserved and rendered statically, not editable.
- Form filling follow-ups: form JavaScript (calculations, formatting,
  validation), creating fields, signing signature fields, XFA forms, FDF and
  XFDF import and export, and flattening.
