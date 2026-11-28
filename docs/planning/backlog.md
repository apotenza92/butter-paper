# Backlog

Open work, roughly in priority order. Remove an item when it is done.

## Confirm on screen

Remaining runtime and platform checks. Source and native harness coverage
alone do not establish these behaviours.

- Physical trackpad pinch/scroll delivery on Windows and Linux. The X11
  GPUI backend reports both trackpads and mouse wheels as line deltas, so
  separating their preferences there needs device-source support in the fork.
- App Appearance and Page Appearance on Windows and macOS: both View
  submenus, Settings ▸ Appearance, Match App, and on macOS the native
  window frames and menus under a forced App Appearance. Dark pages'
  menu and highlights are verified on both; the page edge ring is not.
- Settings window behind the main window on macOS: after a change from
  the View menu, it showed the old Page Appearance until it came to the
  front. Check whether an inactive Settings window repaints.
- Decide on a 1 pt box border for Cloud+ and Callout. At line width 1 the
  text box has no border, on screen and in the file, and at any other
  width it has one. This follows Revu: `/BS /W 0` is the 1 pt leader with
  no box border ([pdf-format.md](pdf-format.md)). A 1 pt border needs a
  separate border setting, or imported borderless callouts gain a border.
- A red rectangle briefly showed a thin black outline while the Split
  View divider was dragged on Windows. Not reproduced.
- Installed update download/restart on macOS, Windows and Linux: preserve
  open documents, view positions and unsaved work, and check relaunch focus.
- Phone signing with a physical phone (the relay is verified with synthetic
  transfers only), and camera signature capture: permission, capture,
  cancellation and camera release.

## Interface

- Split View draws these only in the active pane: markup selection,
  hover and handles, the lasso, tool previews and snap guides, a Text Box
  or form field while it is typed, page text selection, Find highlights
  and the current-page outline.
- The stock dialog X closes on a click and Escape, but not on Enter or
  Space when it has the keyboard.
- The stock colour control: a black swatch has almost no edge on the
  dark theme, and a disabled control does not dim its hex text (the
  Edit PDF formatting bar and Properties).
- Stock motion and controls (see decisions.md), still open:
  - Confirm reduce motion, the view and panel motion and the 8pt spacing on
    Windows and on a Linux desktop (GNOME and KDE).
  - The CAD view button and its settings chevron are two loose buttons:
    the chevron is a Popover trigger, and the stock ButtonGroup takes only
    plain Buttons.
- Stock popup menus initially select a heading or disabled item on the first
  Down press, so titled mouse-wheel menus need a second Down to reach Zoom,
  and the page menu's first Down lands on a disabled Cut. Fix initial and
  wraparound selection to skip non-actionable rows in gpui-kit. The narrow
  toolbar's page-view menu has the same titled mouse-wheel sections.
- Stock checkboxes, switches, toggles, buttons and inputs do not expose a
  disabled state to accessibility (the snapping "Show grid" switch and spacing
  fields report enabled while greyed out). Deferred: the fix belongs in
  gpui-kit's base controls, not in Butter Paper.
- The stock Switch draws no focus ring. The Snap settings switches show the
  stock ring through an app-side wrapper (`focus_ring_switch`); the switches in the properties
  panels, tool defaults and Settings still show none. Give them the wrapper
  once their panels leave room for the ring, or give the stock Switch the ring
  in gpui-kit.
- The template library offers both a top-right X and a Done button that do
  the same thing; decide whether to drop Done (the X is the convention for
  transient panels).
- Bring back the interface size control (removed for now, see decisions.md,
  "Interface size"). The rem-based sizing and the level plumbing remain
  (`application_ui_zoom_font_size`, `layout_metrics::scaled`); restore the
  View menu items, their shortcuts and the stored level. Fix first the
  faults that the owner saw at 1.44x and 1.73x (a 915 × 660 window, a
  three-page document):
  - Button outlines collapse and stray vertical border lines show: fixed
    pixel bars (48 px) clip controls that grow in rem.
  - Rows collide: tab, status bar and rail rows keep fixed heights while
    their text grows.
  - The Fit Width, Fit Page and page view icons stay at 16 px beside larger
    controls.
  - Padding squeezes: fixed pixel padding and thresholds do not keep the
    8pt-grid proportions; the layout must step to a compact form first.
  - The rail "•••" overflow and the status bar hints overlap at small
    window sizes.
  Verify every bar, panel, menu and dialog at each size, with tests at the
  larger sizes.

## Editing

- Dark pages: a vector Snapshot markup still draws in the real colours of
  its copied content. It needs its own design.
- Escape did not clear a markup selection after a switch of the page
  appearance in review. Check whether older versions do the same.
- The Rectangle X property field accepts values that move the shape off the
  page (for example 3205 pt). Decide the limit and apply it.
- Elliptical Arcs import and render but are not editable as ellipses.
- Properties the reference had but native lacks: separate fill opacity,
  hatch fill, text vertical alignment and auto-size, image borders, editing
  imported appearances.
- Drawing to scale ([decisions](decisions.md#interface)), still to do: Set
  as Default for a measurement's unit (as Revu), and a unit for typed sizes
  other than the page's; typed sizes for Arc and Cloud; real-unit size in
  shape Properties; feet-and-inches display. Revu has not yet opened a
  measurement whose unit was changed here. Revu's own Length tool also
  takes a typed angle and segment length while drawing (seen in Revu 21);
  compare before extending typed sizes.
- Revu measurement tools not yet here, by usefulness: Perimeter, Area
  Cutout, Count, Diameter, Radius, Angle, Volume. Most fit the Measure
  switch ([interactions](interactions.md#measure)) as a choice of what a
  shape measures (Polygon: area or perimeter; Ellipse: diameter or radius;
  a three-point Polyline: angle); Count needs a tool of its own.
- Measure follow-ups: the Measure switches and a page's removed scale last
  only while the app or document is open; Enter does not apply the Scale
  panel; keyboard focus on the toolbar's Page Scale does not darken the page
  outline as hovering does; a Dimension turned into a Length takes Length's
  fixed 10-point leader. Revu has not yet opened a markup whose Measure was
  switched here.
- Scale regions: a sheet with views at several scales (a 1/8" plan beside
  1/4" details) needs Revu's viewports, a scale per region. Page scale is
  page-wide, and a PDF's partial viewports are read as page-wide.
- Imperial measurements show decimal feet or inches; drawings write feet
  and inches (`36'-6"`), which Revu stores as a two-unit `/D`.
- Pdf-format follow-ups are listed in [pdf-format.md](pdf-format.md#known-differences-follow-ups).

## Edit PDF

Paragraph editing works and is checked against real Chrome, LibreOffice
and Quartz output (`tests/page_text_corpus.rs`; see [interactions.md](interactions.md#editing-page-text)
and [pdf-format.md](pdf-format.md#edited-page-text)); the target is in
[decisions.md](decisions.md#interface). Each step below ends
with `cargo test --workspace`, `cargo xtask check`, the real-PDFium
`--ignored` worker tests and, for visible changes, a native review with
inspected captures. Steps are in priority order.

0. **List follow-ups.** Lists edit as in a word processor (see
   [interactions.md](interactions.md#editing-page-text) and the confidence
   rule in [decisions.md](decisions.md#interface)). Still open, each left
   as plain paragraphs or unchanged today:
   - Renumber a list that continues in the next column or on the next
     page (each part renumbers on its own now).
   - Lists the detector misses: reversed numbering (4, 3, 2, 1), bullets
     drawn as shapes rather than text (Google Docs, some browsers), items
     whose wrapped lines return to the marker's edge, and tagged lists
     whose structure elements also hold other lines (footnote references
     and notes in GOV.UK Word exports).
   - One numbered line made with Numbered list is not found again after
     a save: a single ordinal needs a wrapped line to show its hanging
     indent.
   - Enter in an empty nested item ends the list; Word moves it up a
     level first.
   - A list that runs through two columns keeps its lines' places after
     Enter or a join, so its item spacing can become uneven there. In one
     column, the lines below an edit take the list's own pitches.
   - Undo in an open paragraph removes one typed letter at a time (each
     key is its own step in `page_text_editor.rs`). Word groups a run of
     typing into one step. Decide whether to group typing here.
   - "Cover page alone" in the page-view menu sets the option only; it
     does not switch to Two Pages view. Decide whether it must.

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
    - Pasting elsewhere does not take text with unsaved edits (the clip is
      read from the page before them): it asks for a save first.
    - Paths that touch but share no clip are separate drawings; the lasso
      selects them together.
    - Text does not turn: turning it would need its text matrix
      rewritten. Groups of several pieces, and drawings whose clip holds
      text, have no rotation knob.
    - Explode writes one placement of a block; AutoCAD's "explode all
      copies" (every placement at once) and editing a block's definition
      for all its placements are not offered. Blocks inside a small clip
      (a viewport) are part of that clip's drawing and are not exploded.
    - Lettering drawn as paths is found by how its pieces lie (see
      [interactions.md](interactions.md#editing-page-content)); AutoCAD's
      hidden searchable text, where a sheet has it, would name it exactly.

## Signatures

Do these in order; each step ships on its own.

1. **Start the signature file picker in Bluebeam's stamps folder** on
   Windows when it exists (PDF signatures are imported already). GPUI's
   `PathPromptOptions` has no starting folder, so this needs a fork change.
2. **Signature checks, remaining:** the trust lists' policy restrictions,
   judging later changes by a certification's (DocMDP) permissions (today
   any permitted change is shown alike; signing a PDF certified to allow no
   changes is refused), the revocation of timestamp authorities'
   certificates, and a setting to stop online revocation checks. Checked
   today: integrity, later changes, chains (CAs, path lengths, document
   purposes) to system and Adobe roots, dates, trusted timestamps and the
   signer chain's revocation.
3. **Signing follow-ups:** certifying signatures (Acrobat's Certify, with
   DocMDP permissions), PDFs that need a password to open (refused today),
   and appearance details beyond Acrobat's defaults (reason, location or a
   logo in the standard text).
4. **System certificates, remaining:** PKCS#11 smart cards on Linux, and
   on-screen checks of a real smart card's PIN prompt on macOS and Windows
   (the Keychain and the personal store are verified with test identities).

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
- Windows: qualify the native save and file pickers by hand.
- Windows: the real-PDFium (`--ignored`) tests are not maintained there; many
  build fixture paths without canonicalising them.
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
- Signature ▸ From the page cuts from the whole-page image of the page last
  navigated to; on another page (Continuous view after scrolling) it reports
  the page not shown yet, and should render the box as Snapshot does.
- Snapshots put a PDF on the Windows clipboard as "Portable Document
  Format", which Butter Paper reads but few Windows apps do; vector pastes
  into Office or Revu would need EMF as well. Edit ▸ Paste is enabled by
  Butter Paper's own copies only: a PDF copied elsewhere pastes from the
  keyboard, but does not enable the menu item. Under X11 a very large
  Snapshot PDF may not paste, as GPUI's X11 clipboard sends each format in
  one piece (no INCR transfers). A Snapshot also copies the page's whole
  content however small its box; dropping what lies outside it would shrink
  heavy drawings' copies.

## Product requests

- Scriptable CLI sharing the app's document, annotation and export
  capabilities, possibly with an MCP interface (#37).
- In-app agent chat with cloud and local models (#38).
- SharePoint markup sessions: when two people open the same SharePoint or
  OneDrive PDF, they share markups live. Find the file's Microsoft Graph
  `driveItem` from a sync-folder path or a link, and use it as the session
  key. Send markup changes keyed by `/NM` through a relay room (a Durable
  Object, as the signature relay uses), and check access through Graph.
  Saves stay one at a time with an ETag `If-Match` upload, so the file stays
  a normal PDF. Edit PDF changes stay single-user, with a lock. Risks: an
  Entra app registration and tenant admin consent, markup privacy on the
  relay (encrypt end to end), and no OneDrive client on Linux.
- Popup notes: currently preserved and rendered statically, not editable.
- Form filling follow-ups: form JavaScript (calculations, formatting,
  validation), creating fields, signing signature fields, XFA forms, FDF and
  XFDF import and export, and flattening.
