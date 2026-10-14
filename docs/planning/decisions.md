# Decisions

Product and process decisions that still hold. Replace an entry when the owner
changes it; history is in git.

## Product

- Butter Paper is the native Rust/GPUI app. The Electron app is frozen at
  `electron-final` and is a reference only. Startup imports no Electron data
  (`StartupDataPolicy::NativeOnly`). Electron 0.0.32 (tag `v0.0.32`) is the
  last Electron update: its migration installs native 0.1.3. The Electron
  update feed then lapses when its signed metadata expires.
- Bluebeam Revu is the reference: settle markup format and markup UX
  questions by matching what Revu does ([pdf-format.md](pdf-format.md)).
- Supported: macOS 13+ (Apple silicon and Intel), Windows x64 and arm64,
  Linux x64 and arm64. Butter Paper Beta is macOS only.
- Phone signing goes through the HTTPS signature relay
  (`services/signature-relay`), as Electron did, so the phone needs no route
  to the computer's network. The local-network (qrcp) helper is retired.
  Opening Signature starts a phone QR session straight away.

## Interface

- Use the stock gpui-component theme and standard control appearances as the
  UI baseline. Preserve the current application layout and control locations;
  review existing visual overrides rather than introducing a new theme.
  Popovers, menus and selects share GPUI stock medium shadow elevation;
  dialogs retain their stronger stock shadow.
- All three platforms retain an in-app menu bar with the shared command
  subset. The native macOS menu bar additionally exposes Mac-specific
  application and window commands; the in-app menu need not mirror it.
- Controls are stock gpui-component controls; the property toolkit in
  `property_controls.rs` composes them. Canvas, renderers, annotation layers
  and the scroll area are domain UI.
- Document tabs: stock Outline appearance; width follows the
  title up to 190px; the close button is hidden at rest and appears on hover
  or focus over the end of the title, which keeps its position and truncates.
  Open, New and template controls stay pinned right, over the tool rail's
  last three columns when it has at least three (the strip's divider continues
  the rail's edge; Recovery and Update sit just before them); an overflow menu
  appears only when tabs overflow. Close buttons use the stock small ghost icon
  Button with the theme full radius for circular hover, pressed and focus
  backgrounds. Filenames keep their leading position when close appears;
  only the trailing truncation changes.
- Recovery and save problems live in a conditional Recovery button beside
  Open/New, with a warning icon and item count. Its bounded, scrollable panel
  has a fixed top-right X; closing it never discards data. Recovery actions
  stay beside the affected document, including inactive documents. Ordinary
  unsaved edits use a centred asterisk in a small circle before the tab title,
  plus the close/save decision, not a banner
  or notification. Reviewing a historical warning explicitly clears it.
- New from template and Manage templates split list and preview 50/50.
- Transient panels and dialogs close with an icon-only top-right X inside the
  content; the word Cancel is reserved for a decision prompt beside a commit or
  destructive action.
- Sidebars: page thumbnails on the left, properties on the right, both 300px
  by default and resizable; thumbnails hide automatically when the window is
  too narrow for both and return when it widens. The right tool rail snaps to
  whole columns, two by default, in sections General, Edit, Review, Draw
  and Measure. When it overflows, an ellipsis cue at the hidden edge covers
  the tools faded under it; hovering it shows "Scroll to see other tools"
  beside the ellipsis, and a rail with nothing to scroll never shows it.
- The viewer toolbar has zoom, fit and page-mode controls; CAD View was
  removed from it.
- Page text editing takes the best of Revu, Acrobat and OnlyOffice rather
  than matching Revu alone (Revu's Edit Text is line-only, for minor
  changes). The target:
  - A separate Edit Text tool (`Shift+T`), not part of Select, as in all
    three.
  - Revu's fidelity: edits are drawn in place by PDFium, untouched lines
    keep their exact bytes, and the original embedded font is used.
  - Acrobat's unit: a click edits the recognised paragraph, which reflows
    within its width. Editing never moves other content. A block that
    grows past its space extends downward with a warning outline. Enter
    starts a new line; Escape or a click away finishes.
  - No font is ever required, unlike Acrobat. Characters come from the
    embedded program, then an installed font of the same name, then the
    closest bundled family. Fonts that are not embedded or are missing
    glyphs never block editing. The Properties panel names any substitute.
  - Formatting from Acrobat and OnlyOffice: font (any installed or bundled
    face, embedded as a subset), size, colour, bold, italic, alignment and
    line spacing; lists that continue their bullets and numbering, and
    paragraphs that flow between columns (OnlyOffice).
  - Signed PDFs: warn that saving invalidates the signatures, then allow
    editing (Revu refuses).

- About Butter Paper and Settings are available in both application menus.
  Settings is a dedicated reusable window with edge-to-edge navigation,
  a category rail, a sidebar of heading shortcuts and one
  scrollable content pane per category. The rail and heading sidebar follow
  shadcn's nested-sidebar pattern, using native sidebar surfaces and
  left-aligned, full-width navigation controls. Narrow windows replace the rail and
  sidebar with a category row and a Jump to section menu. Settings owns update
  checks, their visible results, the check frequency and release notes. Checks
  discover releases and download them automatically in the background. A
  button appears beside the template control only when an update is ready,
  separated by a full-height vertical rule and matching the adjacent control
  height and neutral outline style.
  Its first click opens a popup; Restart to update installs and relaunches,
  while View release notes opens Settings. Failed downloads can be retried in
  Settings. Default automatic checks run at startup for
  beta builds and weekly for stable builds, preserving saved user choices.
  Interface size stays in the View menu, not Settings. Restart uses the normal
  unsaved-work quit flow.

## Release

- `cargo xtask version X.Y.Z`, write the changelog section, commit and push
  `main`, then `cargo xtask release`. Releasing runs no tests: test while
  working; CI tests each push to `main` and each pull request separately.
- User-facing app names are Butter Paper (stable) and Butter Paper Beta
  (beta), without a version or architecture suffix on any platform.
- Releases are immutable and published once, complete, with checksums and an
  attested Homebrew bundle; never edit a release, ship a new version.
- PDFium is the approved chromium/7881 build, pinned by SHA-256 in
  `xtask/pdfium.json` and hosted on the `pdfium-7881` release.
- Windows users install from the Microsoft Store; the download page links
  there. Store copies are updated by the Store, so they show no update
  controls and never check GitHub. The Windows zip stays on each release so
  existing zip installs keep updating themselves.

## Blank document window

- The blank workspace shows up to 20 recently opened PDFs, newest first, with
  filenames and folder paths. Each file can be reopened or revealed separately
  in the system file manager; long paths truncate in the middle with the full path on hover.
- History is shared across windows and retained across launches. Only successful
  opens and saves enter it; generated blank/template sources enter after saving.
  Missing files use the normal opening error, rather than disappearing silently.
- On macOS, ⌘W closes the active tab; with no tabs, it closes the window.
