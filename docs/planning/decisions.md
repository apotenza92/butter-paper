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
- Digital signatures are checked natively (`pdf_signatures`, RustCrypto),
  never by a bundled signer or validator (the Electron work used a pyHanko
  sidecar). A signature is valid when its signed bytes and signature match,
  later revisions only added signatures, notes, markups or form entries,
  and its certificate chains to a trusted root and is in date: at a
  trusted timestamp's time, else now (a signer's clock proves nothing), as
  Acrobat and pyHanko judge. Every issuer in a chain must be a CA allowed
  to sign certificates, within its path length, and the signer's
  certificate must allow signing documents (else "trusted, but not for
  signing documents"). A timestamp dates a signature only when its
  authority is for timestamping, chains to a trusted root and was in date
  then. Later changes are found by comparing every object of the file
  with the signed revision (not by where an update wrote it), with pages
  and the page tree recognised by structure: anything the pages draw, the
  page tree (inherited boxes and resources included), the catalog beyond
  validation data, the form and bookkeeping, or an earlier signature's own
  objects changing makes the signature invalid. Trusted roots are the system's and those of
  Adobe's two published trust lists (AATL and the EU Trusted List), which
  hold the document-signing roots system stores omit. Each list is a PDF
  signed through Adobe Root CA G2, which is pinned; it is fetched from
  `trustlist.adobe.com` when a signed PDF first opens in a run, refreshed
  weekly and used only when its signature checks out. Its policy
  restrictions are not applied yet. Revocation comes from what the PDF
  holds (its document security store, Adobe's revocation archive, the
  signature's CRLs), else from the certificates' OCSP responders and CRLs,
  asked in the background once the PDF's own answer is shown, as Acrobat
  does by default. A "not revoked" answer counts only while current for
  the time checked (issued then or later, or before its next update), an
  answer saying revoked wins over others, and an OCSP responder other
  than the issuer must be certified for OCSP signing; OCSP and CRLs are read as raw DER because `x509-ocsp`
  exists only for the previous RustCrypto line. A certificate revoked
  before the signature's trusted time (now without a timestamp) makes it
  invalid; one revoked after a timestamped signature does not. Checks run
  in the app, so every stream they decompress is bounded (64 MB) against
  decompression bombs. RSA keys
  use the `rsa` crate, whose
  timing advisory (RUSTSEC-2023-0071) concerns attackers who can time the
  private-key operations: signature checks use only public keys, and
  signing happens on the user's own computer.
- Digital signing (File ▸ Sign with Certificate…) follows Acrobat's and
  Revu's defaults: a box drawn where the signature goes, the certificate,
  then Save As for the signed copy, and a timestamp only when a timestamp
  server is set (Settings ▸ Signatures; none at first, so nothing is sent
  anywhere unless the user sets one). It appends a PAdES signature
  (ETSI.CAdES.detached, SHA-256, RSA or ECDSA) as an incremental update,
  so earlier signatures stay valid. With a timestamp (B-T; only the
  signature's hash is sent), the chain's OCSP answers (or CRLs) are kept in
  a document security store appended after it (B-LT), so it can be checked
  after the certificates expire. A drawn box shows Acrobat's standard text
  (the name large, then "Digitally signed by", name and date), only the
  details, or nothing (an invisible signature, listed but not drawn); a
  selected handwritten signature can be signed in place of instead, and
  only the signed copy leaves that markup out (matched by its `/NM`), so
  the PDF it came from keeps it. A PDF with only an owner password is
  signed under its own encryption: the update's objects are encrypted with
  the file's key, except the signature's `/Contents`, as the standard
  says; PDFs that need a password to open are refused. Imported
  signatures keep their ink colour, as both apps keep an imported image's.
  The CMS is written by hand in DER: `cms`'s builder does not build with
  the current RustCrypto crates. Every signed file is read back with the
  signature checker before it is written, through a temporary file;
  signing over the PDF itself closes it first so no process holds the
  file.
- Certificates the system holds sign without leaving it: macOS Keychain
  identities through `SecKeyCreateSignature` (smart cards it exposes
  included), and the Windows personal store through CNG
  (`NCryptSignHash`; smart card drivers put their certificates there).
  Only certificates whose key and extended key usages allow signing
  documents are offered, so developers' code-signing certificates are not.
  Linux has no such store; PKCS#11 smart cards there are not supported yet.
- Signatures can be imported from PNG, JPEG or PDF files (a Bluebeam stamp,
  or a page signed in Acrobat and saved): Acrobat's own signature store is
  private and cannot be read.

## Interface

- App Appearance (decided 2026-11-26 by the owner): a setting for the app's
  own theme, apart from Page Appearance. View ▸ App Appearance and Settings ▸
  Appearance ▸ App choose Light, Dark or Match System (default). Light and
  Dark force the theme in every window and ignore the system. Match System
  follows the system as it changes. The setting is global and kept across
  launches. It has no shortcut and no toolbar button. On macOS, Light and
  Dark also set the application's native appearance, so window frames,
  menus and native panels match; Match System clears it.
- Dark pages (decided 2026-11-25 by the owner, changed 2026-11-26): a view
  setting draws pages dark and never changes the document. View ▸ Page
  Appearance and Settings ▸ Appearance ▸ Pages choose Original (default,
  the PDF as it is), Force Dark (always dark) or Match App (dark while the
  app theme is dark, original while it is light). Match App follows the
  effective App Appearance, so with App Appearance on Match System one
  system switch changes the app and the pages. Profiles from 0.6.0 stored
  `light`, `dark` and `match_system`: they load as Original, Force Dark and
  Match App. The setting is global and kept across launches. There is no
  toolbar button and no shortcut. The worker maps ink and paper
  after it renders (paper unblend, then a lift of the ink in OKLab that
  keeps hue). Pictures stay in their real colours: images that are not
  stencil masks, shadings and Stamp annotations. A full-page scan stays real
  and the status bar says so. Markup colours map only where they paint.
  Stored colours, the Properties panel, saves, exports, copies and previews
  keep the real colours. Paper-relative colours follow the dark pages
  setting, never the app theme, and annotation colours stay document data
  (AGENTS.md). Dark pages have a one-pixel grey ring outside the page edge.
- Two Pages view (decided 2026-11-27 by the owner): a third page view, a
  split button after Continuous and Single Page with its own ▾ menu:
  "Cover page alone" (a check item) and its mouse-wheel setting. Pages
  show side by side in pairs (1–2, 3–4; with the cover alone, 1, then 2–3,
  4–5) and scroll continuously as rows. It works with Fit Width and Fit
  Page (each fits the row), zoom, each page's own rotation, mixed page
  sizes (a row's pages meet at their tops; the row is centred), the page
  navigator, thumbnails, search, every markup tool on either page and dark
  pages. Print and export do not change. The mode and the cover setting
  are kept per document in the session, as other view state; Single Page's
  per-page memory does not apply to it. Rows keep two column slots and
  are centred as two-column rows, as a book: the cover (with Cover page
  alone) sits in the right column and an odd last page in the left, and
  Fit Width and Fit Page fit the full two-column row. Next and Previous
  page (buttons, menu and keys) turn a spread, to the first page of the
  next or previous row, which becomes current; First and Last go to the
  first and last rows; the page field still goes to any page and shows its
  row. A double-click on its button fits the page (the whole row).
- Split View (decided 2026-11-27 by the owner): an icon toggle after the
  page views shows the document in two panes side by side, one document
  session and two views. Each pane has its own scroll, zoom, page, fit and
  page view; markups, edits, history, unsaved state and saving are shared.
  One pane is active (a ring in the focus colour) and the toolbar, menus,
  thumbnails, find and search act on it. A draggable divider, also moved
  by the keyboard, separates the panes; each pane keeps at least 240 px.
  A press or wheel in the other pane makes it active and does nothing else
  (decided 2026-11-27 by the owner). Off, the active
  pane's view stays. Split View is kept per document in the session; it
  reopens with both panes on the active pane's view.
- A file's memory after its tab closes (decided 2026-11-27 by the owner)
  keeps the page view, Cover page alone and Split View with the pages
  remembered, so the file opens again as it was left, not only through a
  restored session. The memory is written in the session manifest's form:
  a 0.6.3 memory (Continuous, no flags) still loads. A 0.6.3 build cannot
  read a memory that holds Two Pages, a cover or Split View, and starts
  with none: the accepted downgrade risk of `pageViews`.
- Find in document (decided 2026-11-25 by the owner): Cmd+F (Ctrl+F on
  Windows and Linux) opens a find bar that floats at the top right of the
  canvas, 8 px from the top and right edges, so nothing on the page moves.
  It searches the page text and the markups of the active document for
  literal text, with Match case and Match whole word, and shows "3 of 17"
  with Previous, Next and a close X. Matches are yellow on the page (the
  warning look, apart from the blue text selection), the current one
  stronger. The yellows are part of the dark pages sets. Two lines join for
  a phrase when they are of one paragraph, or adjacent, overlapping in
  width, with no table rule between them. With the focus on the page,
  Escape first clears a selection, a drawing or a tool, and closes the bar
  only when nothing else is left; with the focus in the bar it closes it.
- Search panel (decided 2026-11-25 by the owner): the left rail has a
  Search button, and the left sidebar shows Page Thumbnails or Search, one
  at a time, at one shared width and with the same yield rule.
  Cmd+Shift+F (Ctrl+Shift+F) opens it with the focus in the query; Edit ▸
  Find and Find in Documents are in the native and in-window menus. Its
  scope is This document (first, and the default; decided 2026-11-26 by
  the owner) or All open documents (every tab in every window; document
  ids are unique across windows). The find bar and the panel use the
  app's medium controls, as the toolbar and properties panels do (decided
  2026-11-26 by the owner). A query uses a subset of
  a common notes app's search language (words, phrases, OR, exclusion,
  groups, regular expressions, case, and the filters text:, markup:,
  author:, subject:, page: and file:), and all terms match in one unit: a
  page's text, or a markup with its replies. Results group by document and
  page with counts, with Match case, Explain search, Collapse results,
  More context and a Sort menu, and at most 10,000 results. Matches in one
  paragraph that share a snippet are one result row with each match marked.
- Visual search (decided 2026-11-25 by the owner) is the panel's second
  mode, beside Text, with the same scope. The picture to find is a box drawn
  with the Snapshot tool (no markup, no copy) or a picture pasted from the
  clipboard (screen pixels at the zoom in view). Matching uses page content
  only, never markups, drawn in real colours even with dark pages on, at
  the lowest worker priority, with zero-mean normalised cross-correlation
  on grey pictures and no learned model. Turns, sizes from 85 % to 118 %
  (50 % to 200 % for a paste) and mirrored copies are on by default; a
  symmetric picture is searched once for each different orientation. The
  similarity slider (70 % to 99 %, 80 % at first; owner decision) filters without a new
  search, the source box is listed as "Source", and hits are outlined in
  the dark pages colours. Colour is ignored, and marking every hit (as a
  markup) is a later item.
- A page has one scale, stored as Revu stores it (a page-wide `/VP`
  viewport). The view toolbar's Scale turns the current page's scale on and
  off, and its caret drops down the one Page Scale panel that chooses it
  (decided 2026-11-11, replacing a Page Scale button among the measuring
  tools, which read as one of them): standard scales, chosen by group
  (Metric, Architectural, Engineering) and then by scale, the ones the
  page states (suggested, never applied without Apply), a custom ratio, or
  calibration from a line of known length; then the measuring unit,
  precision and pages. Saved scale presets were dropped: they lived only in
  the open document and were lost on closing it, and applying to several
  pages covers reuse.
- Drawing to scale is part of the ordinary drawing tools (Rectangle,
  Ellipse, Line, Arrow, Polyline, Polygon, Arc), not a second set of tools
  as Revu's Sketch to Scale is. On a page with a scale, a shape being drawn
  shows its size beside the pointer with the page's scale
  (`3.214 m · 32° | 1:100`), and typing fixes it, AutoCAD-style: Tab moves
  between a rectangle's width and height or a line's length and angle, a
  typed size may carry its unit (`3.2m`, `36'6"`), Enter places. The markup
  saved is the same ordinary markup, as Sketch to Scale's is. A page without
  a scale shows no size and takes no typed size. Properties show and edit a
  shape's size in the page's real units.
- Angle snap is a snap setting, on at 15° by default (decided 2026-11-23 at
  the owner's request, 90° then; 15° from 2026-11-25; it replaces Shift's fixed horizontal-or-vertical rule
  for lines and its 15-degree rule for turning Edit PDF content). Lines and
  path segments being drawn keep their direction at a multiple of the
  increment (90°, 45°, 30°, 15° or custom), and every rotation knob turns to
  a multiple of it. Shift inverts the setting while it is held. A typed
  angle is exact. The Page Scale calibration line follows it as a line
  does. A point snap (an end, midpoint, centre or intersection in range of
  the pointer) wins over the held direction, as AutoCAD's object snaps win
  over Ortho; grids, Nearest and snap guides act only along the held
  direction (decided 2026-11-23 by the owner). Dragging a placed line end or
  path point follows the same rules: the dragged point keeps its direction,
  from its neighbour, at a multiple of the increment (decided 2026-11-25 by
  the owner; the neighbour is the other end of a line, or the previous point
  in path order). Snap dimensions is on at
  1 mm by default (same date and request). Like the other snap settings, both apply to every open document
  and last while the app is open.
- Measuring is a switch on the drawing tools, not separate tools (decided
  2026-11-11, replacing the Measure section of Length, Polylength and
  Area): Dimension, Polyline and Polygon each have Measure, in the tool's
  properties and a selected markup's. On, they draw Revu's measurements (a
  Length, Polylength or Area), still quantities with captions that Revu
  lists as measurements and saved as Revu writes them; off, the plain
  markup. A Dimension measures by default wherever its page has a scale,
  and takes typed text where it has none; Polyline and Polygon start with
  it off. Adding a scale never turns plain markups into measurements. A
  line drawn to scale is not a Length.
  As in Revu 20 and later, each measurement shows its own unit, independent
  of the page scale's; the page keeps the precision. Geometry is stored
  exactly; units and precision only round what is shown and typed.
- Every feature is free for now. Scale and measurement features (setting a
  scale, measuring, typed sizes) may later be a paid tier; that is why they
  hang off the page scale and the Measure switches rather than off duplicate
  tools. No licensing code until that is decided.

- A window filling half a 1920-pixel screen shows the whole interface with
  its sidebars collapsed: Undo and Redo, the page navigator's page field,
  every view control, Edit PDF, the page's Scale and a signed document's
  signatures button, for documents of up to 9,999 pages. (Its First,
  Previous, Next and Last buttons showed there too until the Scale joined
  the toolbar, 2026-11-11; the page keys and menu still turn pages.)
  Changes to the view toolbar keep to that. Every toolbar control shows at
  every window size and with every side panel open or closed, in one row
  (decided 2026-11-23); it never scrolls. The toolbar has seven forms, and
  the whole toolbar changes form at once, not control by control. Each
  step gives up the least-used control it can, and the scale shown beside
  the Scale's ruler stays until the view controls are in menus; every
  function stays reachable by pointer and keyboard:
  - Full: the whole page navigator, and the page's scale beside the
    Scale's ruler.
  - Middle: the First, Previous, Next and Last buttons go (the page field
    and page keys turn pages).
  - Zoom Out and Zoom In move into the zoom menu (their keys still work).
  - Continuous View, Single Page View, Two Pages View, their mouse-wheel
    menus and the Split View toggle merge into one View menu button
    showing the view in use: the three views as checked items, Cover page
    alone and Split View as check items, and each view's mouse wheel in a
    submenu (decided 2026-11-27 by the owner). This step comes before the
    fit buttons go, as those are used more than a change of page view
    (decided 2026-11-27 by the owner). A 960-pixel window (half a 1080p screen on macOS) with Page Thumbnails
    collapsed shows this form for an unsigned document of up to 99 pages.
  - Fit Width and Fit Page move into the zoom menu too, as Acrobat and Revu
    keep them.
  - Compact: the scale shown beside the ruler goes (the Scale's tooltip and
    the status bar name it).
  - Narrowest: the zoom menu shows a magnifier for the zoom (its tooltip
    and menu give the zoom), the page field the page in view alone (its
    tooltip gives the page count), the gaps are halved, and Redo is hidden:
    Shift+click on Undo redoes, which Undo's tooltip says in this form
    only (decided 2026-11-27 by the owner). It shows only
    between side panels in a narrow band of widths: the panels keep it 383
    points wide for a signed document of a four-digit page count, the
    widest case.
  Every form lays its groups out alike (decided 2026-11-25): Undo and Redo
  pinned at the left edge, Edit PDF and the Scale pinned at the right edge
  (then the signatures button), and the page navigator, zoom and page
  layout in the toolbar's centre while they fit there; a wider side pushes
  them toward the narrower side, and at a form's least width the groups
  meet at the 8-point group gap. Each group of toolbar controls sits in its
  own frame. The signatures button is always the shield alone, coloured by
  the verdict; its tooltip says the verdict, and a click lists the
  signatures.
- The toolbar's Undo and Redo send the Edit menu's Undo and Redo, and are
  enabled as its items are (for a focused text field, or the document's
  history). Their tooltips show the keys the platform's way (⌘Z and ⇧⌘Z
  on macOS, Ctrl+Z and Ctrl+Y on Windows and Linux).
- Windows have a least size: document windows 918 × 480, where the view
  toolbar shows Edit PDF, Page Scale, the signatures button and all its view
  controls, with the page field, between the left
  rail and the two-column tool rail (the tool rail folds into its overflow
  menu when short), Settings 600 × 420. Side panels give way before the
  viewer toolbar gets narrower than its narrowest form (decided
  2026-11-23): the page column between them keeps that width (23.9375 rem
  since Redo leaves the narrowest form, decided 2026-11-27 by the owner;
  never less than 15 rem). So a 1200-point window holds Page Thumbnails
  and the properties panel side by side at their usual widths. Alone, Page Thumbnails or the properties panel opens at any
  window width, narrower than its preferred width when it must (down to
  16 rem and 15 rem), and back at that width when the window widens.
  Together, Page Thumbnails yield to the properties panel: they hide while
  both do not fit at their preferred widths, the Page Thumbnails button
  says to widen the window or close properties, and they return when the
  window widens. A restored or cascaded window smaller than
  that opens at it. Dialogs stay within the window: a taller one scrolls
  its body between its title and buttons; panels such as Snap settings do
  the same. Snap settings opens beside the tool rail, over the page, in two
  columns (what to snap to; angles, dimension increments and the
  construction grid), all in view at the default window size and at half a
  1080p screen. In a window too narrow for both columns, it shows them as
  one scrolling column, in the same order.
- Every dialog dims the window behind it and closes by Escape, its Cancel
  or X, and a press on the dimmed backdrop outside it. Alert prompts (a
  decision with a commit or destructive action) and a dialog busy with
  work it must not drop (Convert to Word while it saves or converts)
  ignore the backdrop press. `tests/support/dialog_contract.rs` checks
  each dialog, and a source-policy test fails for a dialog without it.
- Tool guidance and warnings that come and go with a tool (Redact's "marks
  don't remove content") use the status bar, not a banner above the page:
  nothing above the page appears or moves when a tool is chosen.

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
- Document tabs: the stock Segmented tab bar. The bar is the whole tab row,
  edge to edge and top to bottom, with square corners and the viewer
  toolbar's 48px height, on the row's own background (decided 2026-11-17,
  replacing a rounded track inset in the strip and a New PDF lane over the
  tool rail).
  The tabs are the stock large size with 14px titles, their raised area
  32px high to match the row's medium buttons (decided 2026-11-18,
  replacing the 36px row and small buttons). The row has one spacing, the
  7.5px above and below the tabs: at both ends, between tabs, on each side
  of a vertical separator between the tabs and the buttons, and between
  buttons. Each tab is drawn by the stock outline button that draws the
  row's buttons and the toolbar's Fit Width and Fit Page toggles (decided
  2026-11-23, replacing a white raised selected tab with a shadow on grey
  tabs): a tab that is not selected looks like those buttons at rest,
  hovered and pressed, in both themes, and the selected tab looks like a
  selected toggle: no shadow and no other border. A selected outline
  button fills with its border's colour (decided 2026-11-23, in gpui-kit),
  one step past its press, so it reads apart from hover in both themes. Keyboard focus draws the stock
  focus ring's colours inside the tab; a click on a tab focuses it for the
  arrow keys but shows no ring, as a click on a stock button shows none.
  Width follows the title up to 190px; the close button is hidden at rest
  and appears on hover or keyboard focus over the end of the title, which
  keeps its position and truncates. A menu of every open document (a chevron) sits on
  the track just after the last tab, with the tabs it lists; then the
  separator, New Tab (+) and the New from template split control, in that
  order (decided 2026-11-23, replacing the menu after New from template).
  They are medium outline buttons with their own background (white in
  light, dark in dark), and keyboard focus moves through them in that
  order. When the tabs don't fit, they scroll and these stay at the
  track's right end. An edge with tabs scrolled out of sight fades: 16px of
  gradient to the row's background, then the row's 8px spacing in solid
  background at the edge, so the visible tabs end one spacing before the
  documents menu and the strip's left edge, as they do when every tab fits.
  An edge with nothing hidden has no fade; the fade takes no pointer input
  (decided 2026-11-23, replacing tabs clipped hard against the menu). With no document open, the track holds New Tab and
  New from template, and no documents menu. The split control's button
  makes a document from the default template; its tooltip says what a
  click makes, the template's name ("New from template: Blank Paper") and
  below it the template list row's line ("A3 · Landscape · No page
  grid"). Its second half, an open book, opens "Select default template":
  a choice there of template, paper size or orientation is the default at
  once, and the panel makes no document (decided 2026-11-23, replacing a
  "New from template" panel with Create). Recovery and Update sit at the strip's end. Close buttons use
  the stock extra-small ghost icon Button, centred in a square lane at the
  end of the tab's raised area with equal clearance on three sides.
  Filenames keep their leading position when close appears; only the
  trailing truncation changes.
- Buttons in a framed toolbar group (the page navigator, zoom) reach the
  frame: the end buttons' hover and press fill it to its rounded edge, as
  a stock button group's do.
- The toolbar's on/off controls are icon toggles, as Fit Width is (decided
  2026-11-25, replacing labelled switches): Edit PDF is an icon button,
  selected while on; the Scale is one split button, as the page-view
  controls are, whose ruler half turns the current page's scale on and off
  and whose caret drops down the Page Scale panel. Each control's
  accessible name says whether it is on, and the Scale's names the scale.
- Recovery and save problems live in a conditional Recovery button beside
  New PDF, with a warning icon and item count. Its bounded, scrollable panel
  has a fixed top-right X; closing it never discards data. Unsaved changes
  found at startup reopen automatically in tabs (as a new PDF when their
  source has changed); only those that cannot be reopened stay in the panel,
  which then opens. Opens wait while that runs, rather than being dropped. Recovery actions
  stay beside the affected document, including inactive documents. Ordinary
  unsaved edits use a centred asterisk in a small circle before the tab title,
  plus the close/save decision, not a banner
  or notification. Reviewing a historical warning explicitly clears it.
- Save replaces a PDF only with a complete, verified copy written beside it
  and renamed into place, so a crash never leaves it half-written. When the
  system refuses that copy (macOS protects Downloads, Documents and the
  Desktop until the app is allowed them; Windows' Controlled folder access;
  a read-only folder), Save leaves the PDF untouched and its Recovery item
  says why, with Retry, Save to Downloads (unless Downloads refused it),
  Save As… and, on macOS and Windows, the exact switch that allows the
  folder with a button to its settings page: on macOS the item under Butter
  Paper in Files & Folders (Desktop, Documents or Downloads Folder,
  Removable or Network Volumes), otherwise Full Disk Access (which needs a
  reopen); on Windows, Controlled folder access's allowed apps. Save never rewrites the PDF in place instead. The Mac app
  states why it saves in those folders, so macOS can ask the first time.
- A PDF whose changes can't go back to it is told apart when it opens:
  another app's copy (Mail's, Outlook's or Messages' attachment, a
  temporary file, a file opened from inside a zip: on macOS anything in
  `~/Library` but iCloud Drive and `~/Library/CloudStorage`, and the temp
  folders; on Windows `%LOCALAPPDATA%`, Temp included; on Linux `/tmp`,
  `/var/tmp` and `~/.cache`), a read-only file or a read-only disk. Reading
  it needs nothing. Its first change, and only then (once any text edit
  under way ends), opens a prompt naming what it is, with Save to Downloads
  (the system's Downloads folder: XDG's on Linux, the known folder on
  Windows; the same name, numbered when taken), Choose where to save…, a
  Save panel starting in Downloads under its own name, and Keep editing, which keeps the changes. It asks
  once per document. This is the one prompt for unsaved changes, as saving
  them back is not possible. Save and closing with changes go to the Save
  panel too. Another app's copy
  never enters Open Recent, which would reopen a stale copy rather than the
  person's file; once saved, the saved PDF is the document and its entry.
- Select default template and Manage templates split list and preview 50/50.
- Transient panels and dialogs close with an icon-only top-right X inside the
  content; the word Cancel is reserved for a decision prompt beside a commit or
  destructive action.
- Sidebars: page thumbnails or search on the left, properties on the
  right, both 300px by default and resizable; the left sidebar yields to
  properties only, hiding
  while the page between the two is too narrow (see the window size entry).
  The right tool rail has two columns and cannot be resized, in sections
  General, Edit, Review and Draw. General holds Select and Hand, then
  Rotate Left and Rotate Right, which turn the current page at once (as
  the Document menu's commands do) and are never shown selected;
  Shift+click turns every page (decided 2026-11-25; the tooltip's second
  line says so). A keyboard press on a button carries no modifiers, so the
  keyboard turns every page through the Document menu. When it overflows, an ellipsis cue at the hidden edge covers
  the tools faded under it; hovering it shows "Scroll to see other tools"
  beside the ellipsis, and a rail with nothing to scroll never shows it.
- Pages are arranged from Page Thumbnails (its right-click menu, drag, the
  header's +, and the clipboard keys when the thumbnails are focused), as in
  Revu and Acrobat. The header's Rotate All Pages Left and Right, beside
  the +, Shift+click on the rail's Rotate buttons, and the Document menu's
  items of the same names turn every page, whatever is selected, as one
  undoable change. With Fit Width or Fit Page in force, a rotation that
  turns the page in view, and its Undo or Redo, fits the turned page
  again; a free zoom stays (decided 2026-11-25). Arranging is undoable until the document is saved;
  saving rearranged pages starts the undo history again, since earlier
  states name pages of the file as it was. Merging open documents is
  "Merge" (the user's word; Revu calls it Combine), in a dialog like Revu's
  Combine: open documents ticked in tab order, other PDFs added or dropped,
  rows dragged into order. It opens the result as a new untitled document,
  leaves the originals unchanged, and saves unsaved changes first.
- Single Page view remembers each page's zoom and scroll, as Revu does
  (decided 2026-11-25). A page not seen before opens as Revu opens it at a
  zoomed edge: from its top (its bottom when the wheel goes back), at the
  same zoom and horizontal position, or fitted when Fit Width or Fit Page
  is in force. The memory is part of the document's restart view, at most
  100 pages a document; a rotation of a page forgets its view. It also stays
  for each file after its tab closes, for the last 50 files (the least
  recently used goes first), and returns when the file opens again (decided
  2026-11-25 by the owner). It is kept in the app's own data, never in the
  PDF, under the file's canonical path; a file that moved within its volume
  is found again by file identity (bookmark, file ID) where the system has
  one. A file reopens on the page that was shown, in Single Page view, with
  that page's view; one left in Continuous view opens as usual (owner
  decision, 2026-11-25). Quitting writes the memory for every open tab.
  Accepted risk (owner decision, 2026-11-25): a build older than this memory
  reads a session manifest with `pageViews` as not valid, so after a
  downgrade it does not restore the last session. The PDFs do not change.
- Settings ▸ Mouse wheel has Zoom speed (Fine ×1.10, Standard ×1.20,
  Coarse ×1.35 a notch) and Page turning (Deliberate, Balanced,
  Responsive) (decided 2026-11-25). Reverse scroll zoom and Zoom speed
  are together under the heading Zoom (owner decision). The app
  names no other product there. Responsive, the default, is after Revu 21.2
  with Single Page set to Scroll, as measured: one notch (delta 120) turns
  one page at Fit Page and any smaller delta also turns one; zoomed in, a
  notch scrolls 54 points and one notch more at the edge turns the page;
  notches during a turn are dropped (a 0 ms burst turns about one page,
  gaps of 50 to 75 ms three or four, 100 ms or more every notch). Butter
  Paper drops notches less than 40 ms after a turn, turns from the page
  last asked for, and never queues a turn, so a fast spin no longer goes on
  turning after the wheel stops. Unlike Revu, a part notch counts only when
  the parts make a whole notch. Balanced and Deliberate turn one page a spin
  (notches with no gap over 180 ms, or 450 ms) and stop at each page edge,
  for readers who find Revu's flicking too fast. Zoom speed applies to every
  discrete-wheel zoom, which moves smoothly unless reduce motion is set. A
  trackpad pinch or Ctrl+swipe does not follow it: it follows the fingers,
  so a speed would break the direct feel.
- The viewer toolbar has Undo and Redo at its left; the page navigator,
  zoom, fit and page-mode controls in its centre; and Edit PDF and the
  page's Scale at its right. CAD View was removed from it. The page
  navigator is after Revu's: First page, Previous page, "3 / 12" (click it
  to type a page), Next page and Last page. The narrower toolbar forms
  drop the four page buttons; the page field always shows, and the page
  keys and menu still turn pages. The centre controls are centred in the
  toolbar while they fit between its sides; in a narrower toolbar they meet
  the wider side and move toward the narrower one, rather than losing
  controls.
- Page text editing takes the best of Revu, Acrobat and OnlyOffice rather
  than matching Revu alone (Revu's Edit Text is line-only, for minor
  changes). The target:
  - Edit PDF is a mode, not a tool (a choice made for Butter Paper; all
    three use a separate tool): a toggle at the right of the viewer toolbar
    (`Shift+T`) under which Select edits and moves the page's own text,
    drawings and images, as Acrobat's Edit Text & Images does, wherever no
    markup or form field takes the press. Markups and fields stay on top,
    so reviewing and editing need no tool change.
  - Revu's fidelity: edits are drawn in place by PDFium, untouched lines
    keep their exact bytes, and the original embedded font is used.
  - Acrobat's unit: a double-click edits the recognised paragraph (a click
    selects it to move), which reflows within its width. A block that
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
  - PDFs whose permissions forbid changing content (owner restrictions,
    which open without a password and which Acrobat honours): ask the user
    to agree before editing their content, then allow it, keeping the
    file's permissions on save. The restriction is advisory (any app can
    print or decrypt such a file), so refusing outright only pushed users to
    print a copy; the question keeps editing a statement's figures
    deliberate.
  - Structure comes from the PDF's own data first: paragraphs are the
    structure tree's paragraph elements. Lines it puts in no paragraph (all
    of an untagged PDF's, such as Word reports saved without tags) join
    only on firm layout evidence, as a word processor wraps: same font,
    size and left edge at an even pitch, nothing ruled between them, and
    the line above full (the next line's first word would not have fitted
    in its column or ruled cell). A wrong join is worse than a missed one,
    so lists and centred or right-aligned text still edit a line at a time.
    Editing every line of a wrapped cell paragraph alone drew edits over
    their neighbours.
  - Tables edit as tables (beyond Acrobat and Revu, which edit a cell's
    text as loose text): a table is what the PDF's structure tree declares
    (a Tagged PDF's `Table`, as Word, Google Docs, LibreOffice and Acrobat
    write), and on a page without one, a grid its rules close on every side
    with an inner rule and text in it (a Word report saved without tags),
    with no question asked. Editing such a report's cells as loose text ran
    a growing cell over the next row; a closed ruled grid is firm enough
    evidence, and its rows growing is what the user expects.
    Cells wrap and rows grow to fit their text, Tab moves between cells,
    rows are inserted and deleted from the page menu, and a column border
    drags to resize its two columns (their text rewraps). The structure
    tree is kept true to the edited rows and columns, so a table stays a
    table after saving; a table that continues on another page, or has
    merged cells, keeps its rows (and the continuing one its columns).
    Only the table changes: content outside it never moves with it, and
    nothing moves between pages (as in Acrobat and Revu). A table or
    paragraph that grows into other text, or past the page's bottom edge,
    shows a warning label saying so, not an outline, and the user moves
    what is in the way.
    Pushing content below, footer and margin detection, and pagination
    across pages were tried and removed: they rested on layout guesses
    (what is a footer, how much room a page has) that held on the sample
    reports and failed on others.
- Heavier rework of a PDF goes to a word processor: Convert to Word
  Document converts it with Butter Paper's own converter, on every
  platform, saves the .docx beside the PDF (in Downloads for another
  app's copy or a PDF on a read-only disk; asking before replacing an
  existing one) and offers to show or open it. Unsaved changes are saved
  first. It depends on no installed app: driving Microsoft Word (AppleScript,
  COM) failed silently for users and needed Word, which Linux lacks. Other
  converters were rejected: LibreOffice's import and ONLYOFFICE's
  `DocxRenderer` (AGPL; its table builder is a stub) make floating text
  boxes, and pdf2docx (AGPL) breaks row heights and clips text. The
  converter reads pages as Edit PDF does, so paragraphs, tables and runs
  come out as flowing paragraphs and real Word tables, never text boxes.
  It writes the OOXML and its zip itself over `flate2` and `crc32fast`,
  already in the dependency tree, rather than `docx-rs` (MIT), which would
  add `zip`, `xml-rs` and their dependencies for little of the work.
- Form fields are filled with the Select tool, as in Acrobat and Revu; Edit
  PDF edits only the page's own content. Fillable fields are
  outlined in the selection blue whenever Select is active (a choice made
  for Butter Paper), filled faintly on hover and strongly when focused,
  matching Edit PDF's paragraph outlines.
  Markups sit above fields: a press on a markup selects it.

- About Butter Paper and Settings are available in both application menus.
  Settings is a dedicated reusable window built on gpui-component's stock
  Settings (decided 2026-11-18, replacing a custom category rail, heading
  sidebar and Jump to section menu): its sidebar searches and lists the
  pages (Updates, Markups, Signatures) and each page's groups, and each
  section is a titled stock group. Settings owns update
  checks, their visible results and the check frequency. Its Updates
  category shows Automatic checks first, then Version (with the download's
  size and percentage while one runs), then What's new only while there is
  an update to install. Checks discover releases and download them
  automatically in the background. Every copy has Check for Updates under
  About in the Butter Paper menu: a Microsoft Store copy opens its Store
  page, an installed release checks and opens Updates in Settings, and other
  copies open the releases page.
  While there is update work (checking, downloading, an update to download
  or one ready), a slim status button is pinned to the right end of the
  in-app menu bar. On macOS with that menu bar turned off, it sits beside
  the template control instead, separated by a full-height vertical rule and
  matching the adjacent control height and neutral outline style.
  Checking and downloading open Updates in Settings. An available or ready
  update opens a popup: Download update or Restart to update, and View
  release notes. Failed downloads can be retried there or in Settings.
  Default automatic checks run at startup for
  beta builds and weekly for stable builds, preserving saved user choices.
  Restart uses the normal unsaved-work quit flow.
- Interface size: users cannot change it for now (decided 2026-11-23). At
  larger sizes the layout was not correct: button outlines collapsed,
  rows collided, icons stayed small and padding squeezed. The View menu
  has no interface size items, and Cmd+Shift+=, Cmd+Shift+− and
  Cmd+Shift+0 (Ctrl on Windows and Linux) do nothing. Page zoom keeps
  Cmd+=, Cmd+− and Cmd+0. A profile saved at a larger size opens at the
  default size, and the next save stores the default. To make the app
  larger, use the operating system's display scaling (a Retina or scaled
  macOS display, Windows 125% or 150%), which scales the whole window
  evenly. The backlog has the item to bring the control back.
- The view never flashes (decided 2026-11-17, after flashes were fixed one
  command at a time). Two rules in the viewer apply to every change, so a
  new command gets them without its own fix:
  - A frame never paints a document without its layout. When a change
    drops the layout, the workspace lays the pages out again in `render`,
    before the frame paints (`replan_before_paint`). A document shown for
    the first time takes the window's last measured viewport. Only the
    first document in a new window has none: its first frame shows the
    canvas without the page, never a page that then changes size.
  - Pages keep their tiles on screen until their new tiles arrive. When the
    pages are rendered again (`invalidate_raster`), the caller says where
    each shown page went: the same place, a new place, turned, or changed.
    Each page that still shows the same content keeps its tiles, moved and
    turned with it. Only a page whose content changed loses them.
  A change to the content does not move the view. Only a navigation moves
  it: rotating the page in view leaves the view where it is.
- Spacing is an 8pt grid (decided 2026-11-18; `layout_metrics.rs`).
  Controls are 32px. Every edge and every gap between controls is 8px;
  groups of controls are 16px apart. A bar's stated size includes its 1px
  line, drawn inside it without taking layout space: the tab row, the viewer
  toolbar and the panel headers are 48px high (8 + 32 + 8), the left rail is
  48px wide and the two-column tool rail 88px (8 + 32 + 8 + 32 + 8). The
  status bar is 36px (8 + its 20px controls + 8). Panels have 8px margins.
  Stock dialogs and menus keep gpui-component's own spacing.
- Motion is stock (decided 2026-11-18). Every control is the stock
  gpui-component control with its default motion. Where the app has its own
  element with a stock counterpart, the stock one replaces it. Elements with
  no stock counterpart use the stock motion settings:
  - The thumbnails and properties panels show and hide with the stock
    Sidebar's offcanvas motion (200 ms width, ease-in-out).
  - Zoom, Fit Width, Fit Page and rotate animate the view over about
    200 ms. During the motion the page draws the tiles it has. The no-flash
    rules above still apply to every frame. Going to a page still jumps.
  - Reordered thumbnails and document tabs slide to their new places. The
    selection outline and handles fade in, snap guides fade in and out, and
    the Snapshot tool's copied area flashes. Drawing, dragging, resizing and
    nudging are never animated: they follow the pointer or key at once.
  - The app follows the operating system's reduce-motion setting (macOS
    Reduce motion, Windows Animation effects, GNOME Reduce animation and
    the portal's `reduced-motion`, KDE's animation speed). With it on, every
    change takes its end state at once.
- Errors use the stock Alert, choice rows the stock ButtonGroup (vertical in a
  narrow dialog), shortcut hints in tooltips the stock Kbd, and the Recovery
  count the stock Badge. Clickable rows in lists (recent documents, Select
  default template, the merge list, startup recovery) stay Buttons: the stock
  ListItem has no keyboard focus, accessibility role or tooltip. Select
  default template's rows take the stock list's hover and selected colours
  and the stock focus ring, as the Template Library's ListItem rows do.
- A press on the page acts only when the page has the user's attention
  (decided 2026-11-25 by the owner, extending the 2026-11-23 click-out
  decision). Text typed anywhere in the window (the page field, a property
  or default, a form field) makes the press a click-out that only finishes
  the typing, for every tool. On macOS the press that activates an
  inactive window only activates it, as AppKit document views do; on
  Windows and Linux it acts, as those platforms do. A click away from a
  property field keeps the markup selected; the next press deselects it.
  Focus on a button, tab or thumbnail holds no typed text and stops
  nothing. Changed 2026-11-27 by the owner: a press on the selected
  markup's handles or body moves or resizes it at once, also while a field
  has the focus: a properties or default field commits as it does when it
  is left, and text edited in place on a markup (a Text Box, Callout,
  Cloud+ or Dimension caption) commits as one undo step for the text, then
  the gesture starts in the same press. A press on empty page or on another
  markup keeps the rule above: it only finishes the typing.
- In Continuous view a click on a page makes it the current page (decided
  2026-11-25 by the owner), with no scroll, even when the click only
  dismisses. A scroll makes the page level with the view centre current
  again.

## Release

- `cargo xtask version X.Y.Z`, write the changelog section, commit and push
  `main`, then `cargo xtask release`. Releasing runs no tests: test while
  working; CI tests each push to `main` and each pull request separately.
- Versions count up a digit at a time: after X.Y.9 comes X.(Y+1).0, not
  X.Y.10.
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
- macOS ships only from GitHub releases and Homebrew, signed with Developer ID
  and notarised; there is no Mac App Store build. The Mac App Store's sandbox
  would need security-scoped bookmarks for recents and recovery, moving storage
  into the app container, sandboxed helpers and no built-in updater.

## Blank document window

- The blank workspace shows up to 20 recently opened PDFs, newest first, with
  filenames and folder paths. Each file can be reopened or revealed separately
  in the system file manager (its button named for it: Show in Finder, File
  Explorer, or the Linux default folder app); long paths truncate in the
  middle with the full path on hover. A tab's right-click menu reveals its
  PDF the same way, unavailable before a new PDF is first saved and for
  another app's copy.
- History is shared across windows and retained across launches. Only successful
  opens and saves enter it; generated blank/template sources enter after saving.
  The list only offers files that exist: it is checked at launch, when a window
  activates and when an entry is clicked. On macOS and Windows a PDF moved or
  renamed within its volume keeps its place at the new path, found by file
  identity (bookmark, file ID) rather than by searching; one deleted, moved
  to the Trash or Recycle Bin, or otherwise not found leaves the list.
- The + after the last tab, and a double-click on the track's empty space,
  open a New Tab: the blank workspace (Open and the recent files) in a tab of
  its own after the documents. There is at most one. Choosing another tab or
  opening a document replaces it; an opened document's tab takes its place at
  once, selected while it loads, and if the open fails or the loading tab
  closes, the New Tab returns. Closing it (its X or ⌘W) shows the document
  that was active before. The blank workspace's Open button still opens the
  file picker.
- PDFs opened from outside the app open in a new tab of the most recently
  active window by default, or in a window of their own (Settings ▸
  Documents ▸ Open documents in; decided 2026-11-25). Dropped PDFs always
  open as tabs in the window they are dropped on. On Windows and Linux one
  app runs per profile for this: a launch with PDFs hands them to the
  running app through a channel private to the user and exits. A launch
  without PDFs still starts its own app. The channel is a Unix socket in the
  user's runtime or temporary folder, used only when that folder is the
  user's alone (owned by the user, mode 0700), each end checking that the
  other runs as the same user; or, on Windows, a named pipe named with the
  user's SID, open only to the user and SYSTEM, refusing remote clients, to
  which a launch sends nothing unless the pipe's server runs as the same
  user (a pipe another user made first gets nothing).
- On macOS, ⌘W closes the active tab; with no tabs, it closes the window.
