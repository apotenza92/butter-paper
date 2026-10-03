# Backlog

Open work, roughly in priority order. Remove an item when it is done.

## Confirm on screen

These pass the native harness but have not been checked by eye or by hand in
the running app.

- Selection outline without handles, edge resizing and the cursor set
  ([interactions.md](interactions.md#selection-and-editing)).
- Text box editing in place on the page; text centred in click-placed boxes.
- Free panning past page edges and zooming over blank space.
- Tab title staying put when the close button appears.
- Phone signing with a physical phone (the relay is verified with synthetic
  transfers only), and camera signature capture: permission, capture,
  cancellation and camera release.

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
- Windows: ordinary Save falls back to Save As; qualify native save and file
  pickers.
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
