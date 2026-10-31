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
- Convert to Word Document on macOS and Windows: the default app's name in
  Open (Pages or Word; `LSCopyDefaultApplicationURLForContentType`,
  `AssocQueryStringW`), Show in Finder or File Explorer, and the converted
  report opened in Microsoft Word itself (verified only in LibreOffice).

## Opening

- The macOS-printed fixtures `tests/fixtures/text-edit/quartz*.pdf`
  (Quartz PDFContext, macOS 27) do not open in the app: "PDF worker
  PageError: PdfiumLibraryInternalError(Unknown)", already on 0.3.7. The
  worker's real-PDFium test opens and renders `quartz.pdf`, so a later step
  of opening fails. Find which worker request fails, then check other
  Quartz output (Preview, Print to PDF).

## Interface

- Stock popup menus initially select a heading or disabled item on the first
  Down press, so titled mouse-wheel menus need a second Down to reach Zoom,
  and the page menu's first Down lands on a disabled Cut. Fix initial and
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
- Scale regions: a sheet with views at several scales (a 1/8" plan beside
  1/4" details) needs Revu's viewports, a scale per region. Page scale is
  page-wide, and a PDF's partial viewports are read as page-wide.
- Imperial measurements show decimal feet or inches; drawings write feet
  and inches (`36'-6"`), which Revu stores as a two-unit `/D`.
- A PDF whose `/Annots` names a missing object (a damaged file, some page
  extractions) does not open; a missing object should read as null, as in
  PDFium and Revu.
- Pdf-format follow-ups are listed in [pdf-format.md](pdf-format.md#known-differences-follow-ups).

## Edit PDF

- Paste copied page content on another page or into another PDF. Content
  copies are references to the objects on the page they came from, so this
  needs the worker to copy content objects (with their fonts, images and
  resources) between pages and documents. Markups already paste anywhere.

Paragraph editing works and is checked against real Chrome, LibreOffice
and Quartz output (`tests/page_text_corpus.rs`; see [interactions.md](interactions.md#editing-page-text)
and [pdf-format.md](pdf-format.md#edited-page-text)); the target is in
[decisions.md](decisions.md#interface). Each step below ends
with `cargo test --workspace`, `cargo xtask check`, the real-PDFium
`--ignored` worker tests and, for visible changes, a native review with
inspected captures. Steps are in priority order.

1. **Linux review workflow.** Edit PDF, lists, columns and edge resize
    pass the native review on Windows and on Linux under WSLg. The manual
    `Review` workflow (`.github/workflows/review.yml`) runs Linux under Xvfb,
    which draws only black windows, under WSL and on a real Ubuntu 26.04
    machine alike: GPUI ranks an OpenGL adapter above software Vulkan, and
    with only software Vulkan available the X11 window still presents
    nothing (also under rootful Xwayland). The same build and script pass as
    a Wayland client of a headless GNOME Shell (`gnome-shell --headless
    --virtual-monitor 1600x1100`), captured through Mutter's ScreenCast
    D-Bus API (`scripts/gnome-headless/`; see
    [native-review.md](../agents/native-review.md#linux-gnome-on-wayland)).
    Move the Linux job to that session. Its captures show that after
    saving, the reopened paragraph's box is narrower and lower than the text
    laid out in it: the 16 pt first line crosses its top edge and the widest
    line its left edge.
2. **Table follow-ups.** Ruled tables edit as tables (see
    [interactions.md](interactions.md#tables) and
    [pdf-format.md](pdf-format.md#edited-tables)). Still open:
    - Tables without vertical rules, or without any (borderless, and
      horizontal-rule-only tables), are not found: infer columns from the
      text's alignment, and use a tagged PDF's `/Table` structure.
    - Typing in a growing cell rebuilds the worker's shown document for each
      keystroke (`table_typing_latency`, release, Apple M-series): 0.6 ms
      for a light page, 45 ms for a table over 40,000 strokes, and 125 ms
      for that page in a 204 MB file. About 80 ms of the last is copying and
      comparing the whole file. Writing only the changed objects' update,
      and giving PDFium the file and the update without joining them, would
      remove it. The app's own share is 8 ms (`table_typing_latency_in_the_app`).
3. **Moving content follow-ups.** Paragraphs, drawings and images move,
    resize, delete, copy and snap (see
    [interactions.md](interactions.md#editing-page-content)). Still open:
    - Paste copies only onto the page they were copied from: another page
      would need the original's fonts, images and colour spaces copied
      into its resources.
    - Paths that touch but share no clip are separate drawings; the lasso
      selects them together.
    - Selected content has markups' handles but no rotation knob: drawings
      and images could turn by their matrix; text would need its text
      matrix rewritten.

## Word documents

Convert to Word Document handles headings, paragraphs, ruled and tagged
tables and images. Not handled yet, so their text flows as plain paragraphs
(or is left out):

- Columns: a two-column passage is written one column after the other, and
  a paragraph flowing between columns gets a line break at each line.
- Tables without rules (and untagged ones ruled only between rows): their
  cells come out as separate paragraphs.
- Lists: items keep a text bullet as text, without Word numbering; bullets
  drawn as shapes (Chrome's) are lost.
- Headers and footers: repeated page furniture is body text on each page.
- Cell shading and per-side borders: a converted table is fully bordered
  or not at all.
- Markups (comments, highlights, stamps) and form field values, rotated
  pages and vertical or rotated text.

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
- Linux: qualify production graphics. On GNOME 50 Wayland (Ubuntu 26.04,
  Intel UHD 630 with a GTX 1660 Ti on NVIDIA 595-open), development builds
  draw on the compositor's Intel GPU through Vulkan, and on NVIDIA Vulkan
  (`ZED_DEVICE_ID`) and Mesa OpenGL (Vulkan hidden) too; edge and corner
  resizes repaint and refit on all three. Still open: a packaged release
  build, X11 sessions, other desktops, AMD, fractional scaling and a
  compositor drawing on NVIDIA.
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
