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
  viewport). One Page Scale dialog chooses it: standard scales, the ones the
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
- Measurements stay their own tools: they are quantities, with captions,
  listed by Revu as measurements. A line drawn to scale is not a Length.
  As in Revu 20 and later, each measurement shows its own unit, independent
  of the page scale's; the page keeps the precision. Geometry is stored
  exactly; units and precision only round what is shown and typed.
- Every feature is free for now. Scale and measurement features (setting a
  scale, measurement tools, typed sizes) may later be a paid tier; that is
  why they hang off the page scale and the Measure section rather than off
  duplicate tools. No licensing code until that is decided.

- A window filling half a 1920-pixel screen shows the whole interface with
  its sidebars collapsed: Edit PDF, every view control, the whole page
  navigator and a signed document's signatures button, for documents of up
  to 9,999 pages. Changes to the view toolbar keep to that. Narrower than it
  needs, the whole toolbar turns compact at once, not control by control:
  the Edit PDF switch shows an icon in place of its label, and the page
  navigator only its page field. The signatures button is always the shield
  alone, coloured by the verdict; its tooltip says the verdict, and a click
  lists the signatures.
- Windows have a least size: document windows 746 × 480, where the view
  toolbar shows Edit PDF, the signatures button and all its view controls,
  with the page field, between the left
  rail and the two-column tool rail (the tool rail folds into its overflow
  menu when short), Settings 600 × 420. Page Thumbnails yields to the
  toolbar: it hides while the window is too narrow for both, and returns
  when it widens. The toolbar scrolls sideways only when something else
  squeezes it (an open properties panel, a widened tool rail or a larger
  interface size). A restored or cascaded window smaller than
  that opens at it. Dialogs stay within the window: a taller one scrolls
  its body between its title and buttons; panels such as Snap settings do
  the same.
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
- Document tabs: stock Outline appearance; width follows the
  title up to 190px; the close button is hidden at rest and appears on hover
  or focus over the end of the title, which keeps its position and truncates.
  Open, New and template controls stay pinned right at one size and spacing,
  the same distance from the window's edge at every rail width, in a lane
  never narrower than a three-column rail: its divider continues the rail's
  edge from three tool columns and stays put below that (Recovery and Update
  sit just before it); an overflow menu
  appears only when tabs overflow. Close buttons use the stock small ghost icon
  Button with the theme full radius for circular hover, pressed and focus
  backgrounds. Filenames keep their leading position when close appears;
  only the trailing truncation changes.
- Recovery and save problems live in a conditional Recovery button beside
  Open/New, with a warning icon and item count. Its bounded, scrollable panel
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
  / 12" (click it to type a page), Next page and Last page. A compact
  toolbar drops the four page buttons; the page field always shows. The view
  controls are centred while the Edit PDF slot's width is free on both
  sides; in a narrower toolbar the empty space opposite Edit PDF gives way
  first and they move right, rather than losing controls.
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
  Settings is a dedicated reusable window with edge-to-edge navigation,
  a category rail, a sidebar of heading shortcuts and one
  scrollable content pane per category. The rail and heading sidebar follow
  shadcn's nested-sidebar pattern, using native sidebar surfaces and
  left-aligned, full-width navigation controls. Narrow windows replace the rail and
  sidebar with a category row and a Jump to section menu. Settings owns update
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
  Interface size stays in the View menu, not Settings. Restart uses the normal
  unsaved-work quit flow.

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
- The + beside the tabs, and a double-click on the tab bar's empty space,
  open a New Tab: the blank workspace (Open and the recent files) in a tab of
  its own after the documents. There is at most one. Choosing another tab or
  opening a document replaces it; closing it (its X or ⌘W) shows the document
  that was active before. The blank workspace's Open button still opens the
  file picker.
- On macOS, ⌘W closes the active tab; with no tabs, it closes the window.
