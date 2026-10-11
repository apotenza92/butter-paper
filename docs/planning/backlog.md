# Backlog

Open work, roughly in priority order. Remove an item when it is done.

## Confirm on screen

Remaining runtime and platform checks. Source and native harness coverage
alone do not establish these behaviours.

- Installed update download/restart on macOS, Windows and Linux: preserve
  open documents, view positions and unsaved work, and check relaunch focus.
- Phone signing with a physical phone (the relay is verified with synthetic
  transfers only), and camera signature capture: permission, capture,
  cancellation and camera release.

## Interface

- Stock checkboxes, switches, toggles, buttons and inputs do not expose a
  disabled state to accessibility (the snapping "Show grid" switch and spacing
  fields report enabled while greyed out). Deferred: the fix belongs in
  gpui-kit's base controls, not in Butter Paper.
- The template library offers both a top-right X and a Done button that do
  the same thing; decide whether to drop Done (the X is the convention for
  transient panels).
- Quit and close recovery alerts show raw internal errors (for example
  `Operation(Io { operation: InspectRoot, kind: NotFound })`); map them to
  plain language.

## Editing

- Ellipse, Text Box, Image, Snapshot and Redact resize only from corners and
  edge midpoints; give them the Rectangle's resize-from-any-edge zone.
- Text box appearances export the first baseline one font size below the
  inset, about 1.8 pt lower than the canvas at 12 pt; align the export with
  the canvas line box.
- Elliptical Arcs import and render but are not editable as ellipses.
- Properties the reference had but native lacks: separate fill opacity,
  hatch fill, text vertical alignment and auto-size, image borders, editing
  imported appearances.
- Pdf-format follow-ups are listed in [pdf-format.md](pdf-format.md#known-differences-follow-ups).

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
  no maintained protocol yet.

## Product requests

- PDF text search with result navigation, later scanned-image search (#36).
- Scriptable CLI sharing the app's document, annotation and export
  capabilities, possibly with an MCP interface (#37).
- In-app agent chat with cloud and local models (#38).
- Interactive PDF forms and popup notes: currently preserved and rendered
  statically, not editable.
