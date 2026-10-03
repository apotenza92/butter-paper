# Decisions

Product and process decisions that still hold. Replace an entry when the owner
changes it; history is in git.

## Product

- Butter Paper is the native Rust/GPUI app. The Electron app is frozen at
  `electron-final` and is a reference only. Startup imports no Electron data
  (`StartupDataPolicy::NativeOnly`); the Electron update feed is left to
  lapse, and Electron users who have not updated download 0.1.x directly.
- Bluebeam Revu is the reference: settle markup format and markup UX
  questions by matching what Revu does ([pdf-format.md](pdf-format.md)).
- Supported: macOS 13+ (Apple silicon and Intel), Windows x64 and arm64,
  Linux x64 and arm64. Butter Paper Beta is macOS only.
- Phone signing goes through the HTTPS signature relay
  (`services/signature-relay`), as Electron did, so the phone needs no route
  to the computer's network. The local-network (qrcp) helper is retired.
  Opening Signature starts a phone QR session straight away.

## Interface

- Controls are stock gpui-component controls; the property toolkit in
  `property_controls.rs` composes them. Canvas, renderers, annotation layers
  and the scroll area are domain UI.
- Document tabs: Outline style with Ghost button fills; width follows the
  title up to 190px; the close button is hidden at rest and appears on hover
  or focus over the end of the title, which keeps its position and truncates.
  Open, New and template controls stay pinned right; an overflow menu appears
  only when tabs overflow.
- New from template and Manage templates split list and preview 50/50.
- Transient panels and dialogs close with an icon-only top-right X inside the
  content; the word Cancel is reserved for a modal decision.
- Sidebars: page thumbnails on the left, properties on the right, both 300px
  by default and resizable; thumbnails hide automatically when the window is
  too narrow for both and return when it widens. The right tool rail snaps to
  whole columns.
- The viewer toolbar has zoom, fit and page-mode controls; CAD View was
  removed from it.

## Release

- `cargo xtask version X.Y.Z`, write the changelog section, commit and push
  `main`, then `cargo xtask release`. Releasing runs no tests: test while
  working.
- Releases are immutable and published once, complete, with checksums and an
  attested Homebrew bundle; never edit a release, ship a new version.
- PDFium is the approved chromium/7881 build, pinned by SHA-256 in
  `xtask/pdfium.json` and hosted on the `pdfium-7881` release.
