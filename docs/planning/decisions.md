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

- A page has one scale, stored as Revu stores it (a page-wide `/VP`
  viewport). The view toolbar's Scale turns the current page's scale on and
  off, and its menu drops down the one Page Scale panel that chooses it
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
- Angle snap is a snap setting, on at 90° by default (decided 2026-11-23 at
  the owner's request; it replaces Shift's fixed horizontal-or-vertical rule
  for lines and its 15-degree rule for turning Edit PDF content). Lines and
  path segments being drawn keep their direction at a multiple of the
  increment (90°, 45°, 30°, 15° or custom), and every rotation knob turns to
  a multiple of it. Shift inverts the setting while it is held. A typed
  angle is exact. The Page Scale calibration line follows it as a line
  does. A point snap (an end, midpoint, centre or intersection in range of
  the pointer) wins over the held direction, as AutoCAD's object snaps win
  over Ortho; grids, Nearest and snap guides act only along the held
  direction (decided 2026-11-23 by the owner). Snap dimensions is on at
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
  its sidebars collapsed: Edit PDF, the page's Scale, every view control, the
  page navigator's page field and a signed document's signatures button, for
  documents of up to 9,999 pages. (Its First, Previous, Next and Last buttons
  showed there too until the Scale joined the toolbar, 2026-11-11; the page
  keys and menu still turn pages.) Changes to the view toolbar keep to that.
  Every toolbar control shows at every window size and with every side
  panel open or closed, in one row (decided 2026-11-23); it never
  scrolls. The toolbar has eight forms, and the whole toolbar changes form at once, not control by
  control. Each step gives up the least-used control it can, and the Edit
  and Page Scale labels stay to the last; every function stays reachable by
  pointer and keyboard:
  - Full: the labels Edit and Page Scale beside their switches, the scale
    shown beside the Page Scale switch, and the whole page navigator.
  - Middle: the scale shown and the First, Previous, Next and Last buttons
    go (the status bar shows the scale; the page field and page keys turn
    pages). A 960-pixel window (half a 1080p screen on macOS) with Page
    Thumbnails collapsed shows it for an unsigned document of up to 99
    pages; a longer page count or a signed document's shield takes the
    next form there.
  - Zoom Out and Zoom In move into the zoom menu (their keys still work).
  - Fit Width and Fit Page move into the zoom menu too, as Acrobat and Revu
    keep them.
  - Continuous View, Single Page View and their mouse-wheel menus merge
    into one menu button showing the view in use.
  - Compact: icons in place of the Edit and Page Scale labels.
  - Tightest: small switches. It is 521 points wide for a signed document
    of a four-digit page count, the widest case.
  - Narrowest: the zoom menu shows a magnifier for the zoom (its tooltip
    and menu give the zoom), and the page field the page in view alone (its
    tooltip gives the page count). It shows only between side panels in
    a narrow band of widths: the panels keep it 423 points wide for a
    signed document of a four-digit page count, the widest case.
  Every form lays its groups out alike (decided 2026-11-23): the page
  navigator pinned at the left edge, Edit and Page Scale pinned at the
  right edge, and zoom with the page layout in the toolbar's centre while
  it fits there; a wider side pushes them toward the narrower side, and at
  a form's least width the groups meet at the 8-point group gap.
  Each group of toolbar controls sits in its
  own frame. The signatures button is always the shield
  alone, coloured by the verdict; its tooltip says the verdict, and a click
  lists the signatures.
- Windows have a least size: document windows 918 × 480, where the view
  toolbar shows Edit PDF, Page Scale, the signatures button and all its view
  controls, with the page field, between the left
  rail and the two-column tool rail (the tool rail folds into its overflow
  menu when short), Settings 600 × 420. Side panels give way before the
  viewer toolbar gets narrower than its narrowest form (decided
  2026-11-23): the page column between them keeps that width (26.4375 rem,
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
- Sidebars: page thumbnails on the left, properties on the right, both 300px
  by default and resizable; thumbnails yield to properties only, hiding
  while the page between the two is too narrow (see the window size entry).
  The right tool rail has two columns and cannot be resized, in sections
  General, Edit, Review and Draw. When it overflows, an ellipsis cue at the hidden edge covers
  the tools faded under it; hovering it shows "Scroll to see other tools"
  beside the ellipsis, and a rail with nothing to scroll never shows it.
- Pages are arranged from Page Thumbnails (its right-click menu, drag, the
  header's +, and the clipboard keys when the thumbnails are focused), as in
  Revu and Acrobat. Arranging is undoable until the document is saved;
  saving rearranged pages starts the undo history again, since earlier
  states name pages of the file as it was. Merging open documents is
  "Merge" (the user's word; Revu calls it Combine), in a dialog like Revu's
  Combine: open documents ticked in tab order, other PDFs added or dropped,
  rows dragged into order. It opens the result as a new untitled document,
  leaves the originals unchanged, and saves unsaved changes first.
- The viewer toolbar has zoom, fit and page-mode controls, and the Edit PDF
  switch at its left; CAD View was removed from it. A page navigator sits
  right of the zoom percentage, after Revu's: First page, Previous page, "3
  / 12" (click it to type a page), Next page and Last page. The narrower
  toolbar forms drop the four page buttons; the page field always shows,
  and the page keys and menu still turn pages. The view
  controls are centred in the toolbar while they fit between its sides;
  in a narrower toolbar they meet the wider side and move toward the
  narrower one, rather than losing controls.
- Page text editing takes the best of Revu, Acrobat and OnlyOffice rather
  than matching Revu alone (Revu's Edit Text is line-only, for minor
  changes). The target:
  - Edit PDF is a mode, not a tool (a choice made for Butter Paper; all
    three use a separate tool): a switch at the left of the viewer toolbar
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
  opening a document replaces it; closing it (its X or ⌘W) shows the document
  that was active before. The blank workspace's Open button still opens the
  file picker.
- On macOS, ⌘W closes the active tab; with no tabs, it closes the window.
