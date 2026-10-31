# Changelog

All notable Butter Paper changes are recorded here.

## [Unreleased]

## [0.3.8]

### Edit PDF

- Untagged PDFs, such as Word reports saved without accessibility tags,
  edit a wrapped paragraph as one paragraph again. Lines join only where
  one clearly wraps onto the next.
- Ruled tables in untagged PDFs edit as tables: a cell's text rewraps
  inside its cell, and text that outgrows it grows the row, moving the
  rows below, instead of running over them.
- A paragraph moved onto other text by a new line spacing or larger text
  now shows the "Overlaps other text" warning.
- PDFs whose creator protected their content from changes, such as bank
  statements, can be edited after you agree to a warning; the saved file
  keeps its protection settings. Markups never needed it.
- Clicking text that cannot be edited says why in the status bar, as do
  other status messages, such as conversion progress; before, they were
  never shown.
- Pages whose content Adobe software split into pieces mid-instruction
  (common in bank and government forms) can be edited; before, none of
  their text could be.

### Word documents

- File > Convert to Word Document (was Convert and Open in Word) converts
  the PDF itself, on macOS, Windows and Linux, without Microsoft Word:
  paragraphs flow as paragraphs with their fonts, sizes, bold, italic,
  colour, alignment and spacing; ruled and tagged tables become Word tables;
  images are kept in place; and each page keeps its size and margins. Once
  saved beside the PDF, a dialog offers to show it in Finder, File Explorer
  or the file manager, or open it in your app for Word documents.
- The macOS app no longer asks for permission to control other apps.

### Windows

- Butter Paper installed from the zip starts on a PC without the Microsoft
  Visual C++ runtime: it now carries its own.
- Show in File Explorer, for a recent file or a converted Word document,
  opens File Explorer with the file selected.

## [0.3.7]

### Windows

- The Microsoft Store app's icon in the taskbar, Start menu and search no
  longer sits on a blue accent-coloured square under Windows' light theme.

## [0.3.6]

### Clipboard

- Cut, Copy and Paste work between documents and windows: markups copied in
  one PDF paste into any other open PDF, where they were. The page's
  right-click menu has Cut, Copy, Paste and Delete, and right-clicking a
  markup selects it first. The Edit menu now also enables for selected Edit
  PDF content and page text.

### Templates

- New from template and the Template Library have an Orientation toggle:
  turn any paper size portrait or landscape before creating.
- The Template Library picks one paper size at a time from a dropdown, as
  New from template does, and its rows look the same: the selected one
  filled and ticked instead of outlined in blue.

### Snapping

- Rectangles and ellipses snap to other markups while being drawn.
- Lines snap beside other strokes, not only onto them: edges just touching
  on either side, or flush inside a thicker stroke.

### Selection

- Selection handles and the rotation knob always draw above every markup,
  so a markup drawn later no longer hides them.

### Markup

- Every tool that draws a box or a line takes the same two gestures as
  Rectangle: drag it out, or click one end and then the other.
- Cloud and Cloud+ draw rectangles both ways, instead of clicks making a
  polygon. For a cloud of any shape, turn on Cloud outline in the Polygon
  tool's properties: the polygon is drawn as a cloud.
- Length and Dimension can also be dragged out.
- Text boxes are drawn: drag the box, or click two corners, then type. The
  box grows to fit the text.
- Images and signatures fill the box you drag or click out, keeping their
  proportions. Images dropped from the desktop are still placed where you
  drop them.

### Interface

- The start page offers a new document from your last template beside Open,
  and no longer shows view controls with no page to view.
- Menus, buttons and dialog titles use sentence case throughout, such as
  "Delete page" and "Merge documents"; "Apply colour" is spelt the same in
  every panel.
- A selected area or polylength shows its lock, line and fill once, with a
  separate Label section for its text, instead of two overlapping panels.
- Tool properties name colours plainly ("Colour" for highlights and
  polygons), the Dimension colour label no longer wraps, and the new-document
  button's tooltip says what it makes.

## [0.3.5]

### Tabs

- With a New Tab open, the other tabs can still be dragged to reorder them or
  into a new window, and the overflow menu still appears; the New Tab scrolls
  into view when it opens.

### Templates

- A template saved or removed in another window shows up in the Template
  Library list straight away, instead of after it is next reopened.

## [0.3.4]

### Markup

- Drawing could stop working in every document, with the cursor changing but
  nothing drawn, after a tab with unsaved changes was closed from its prompt.
  A prompt or popover that is no longer shown can no longer block the page.

### Snapping

- The snap settings panel uses full-size controls with room between them, and
  no longer cuts off the bottoms of labels such as "Page grid".

### Templates

- Templates name their paper: the built-ins read "A3 · Landscape" rather than
  "420 × 297 mm · Landscape", and custom sizes keep their millimetres.
- Blank Paper and every patterned built-in now also come in A4 portrait.
- New from template has a Paper size dropdown that lists one size at a time and
  keeps your pattern when you switch size, and the Template Library groups its
  templates under a heading for each paper size.
- Templates imported from a PDF show their first page as their preview.
- A template's Remove button fits its row however long the name, appearing as
  a small delete icon on hover or keyboard focus.

### Tabs

- The + beside the tabs, or a double-click on the tab bar's empty space, opens
  a New Tab showing Open and your recent files, instead of going straight to
  the file picker. Opening a file or choosing another tab replaces it; closing
  it returns to the document you were on.

### Look

- Butter Paper describes itself as a PDF editor as well as a markup app: on the
  download page, in the Windows app and default-app settings, on the Linux
  launcher and in Homebrew.

## [0.3.3]

### Dialogs

- Removing a PDF from Merge Documents, or a template from the Template
  Library, with the keyboard no longer moves focus to the document behind the
  dialog, where Cancel, Escape and Tab stopped reaching the dialog. The same
  holds in any dialog whose focused control goes away.

### Look

- A new icon: a folded paper butterfly, butter yellow for the stable app and
  blueprint blue for the beta. It sits optically centred with room around
  it on macOS and Linux, and Windows Start tiles pad it instead of filling
  the tile edge to edge.
- The download page follows the new colours: butter for stable, blueprint
  for beta. Its keyboard focus ring is now clearly visible on every theme.

## [0.3.2]

### Tables

- Text drawn inside a table that isn't part of any of its cells no longer
  makes editing the table warn "Overlaps other text" and frame that text.
- The "Overlaps other text" label sits under the text that is covered,
  instead of under the table.

## [0.3.1]

### Tables

- Clicking text drawn inside a table that isn't part of any of its cells
  edits that text, instead of opening the cell around it and typing into
  another paragraph.

### Look

- The butterfly icon's lower wings are shorter, pulled back along their
  diagonals.

## [0.3.0]

### Pages

- Add, delete, move, duplicate, copy and paste pages from Page Thumbnails:
  right-click a thumbnail for the page menu (which now also holds Rotate),
  drag thumbnails to reorder them, or use + for a blank page after the
  current page or at the end. With the thumbnails focused, Cmd-C, Cmd-X,
  Cmd-V, Cmd-D and Delete work on pages (Ctrl on Windows). Cmd- or
  Shift-click selects several. A page keeps its markups and edits; each change can be undone.
- Merge documents: File ▸ Merge Documents… lists the open documents to
  tick and drag into order, and takes other PDFs added or dropped on it.
  The merge opens as a new tab; documents with unsaved changes are saved
  first.

### Saving

- On Windows, Save As offers PDF as the file type and adds `.pdf` to a
  name typed without it, instead of listing only "All files" and refusing
  the save. A name typed without `.pdf` on any system now saves as a PDF.
- Choosing the open file itself in Save As saves over it, as Save does,
  instead of refusing.

### Look

- Hovering a tab shows the document's whole name, which a narrow tab cuts
  short.
- The Open and New buttons at the right of the tab strip keep the same size
  and spacing however wide the tool sidebar is, instead of growing when it
  has three or more columns, and the divider before them no longer jumps
  when the sidebar reaches three columns.
- The Page Thumbnails header matches the Properties header, and each
  thumbnail page has an edge so it stands out from the panel.
- Rotating a page turns it in one step, instead of briefly showing the old
  page stretched to its new shape.
- Closing a tab with unsaved changes asks in a roomier box, with a
  shorter explanation.
- A document with unsaved changes shows a larger asterisk in a grey
  circle the size of the close button, as far in from the tab's left edge
  as the close button is from its right.
- A squarer butterfly icon, so it fills its slot in the Windows taskbar,
  Start menu and Dock instead of looking short beside other apps: four
  angled wings on one central spine, with a darker top-left wingtip that
  no longer fades into light backgrounds.
- New windows open centred in the part of the screen the Dock and menu
  bar (or the Windows taskbar) leave free.

### Measuring

- Page Scale is redesigned. One list holds the standard metric,
  architectural and engineering scales, searchable, with the scales the
  drawing itself states (`1/4" = 1'-0"`, `1:100 @ A1`) first and offered as
  buttons; a page without a scale starts at the one it states. The dialog
  shows the page's paper size, what the scale means (`1 cm on paper is 1
  m`), the unit and precision to measure in, and which pages to apply it to.
- Calibrate is quicker and more accurate: the dialog steps aside, the line's
  ends snap to the drawing, the line and its length follow the pointer,
  Shift keeps it straight and Esc goes back. Then type its real length,
  including feet and inches (`36'-6"`) or another unit (`3600 mm`), and use
  the standard scale it is close to with one click.
- A scale can be removed (No scale), and the status bar shows the page's
  scale; click it to change it.
- Imported imperial scales show their standard name (`1/4" = 1'-0"`)
  rather than a ratio.
- Saved scale presets are gone: they were lost when the document closed.

### Opening documents

- Unsaved changes left by a crash or forced quit reopen by themselves in a
  tab when Butter Paper starts, without asking. If their PDF has changed
  since, they open as a new PDF in their own tab instead. Only changes
  that cannot be reopened are left in the Recovery panel, which opens to
  show them.
- Opening a PDF from Recently opened, File > Open or the Finder while
  those changes are reopening no longer does nothing: it opens as soon as
  they are back. A recovery that needs a decision no longer stops other
  PDFs opening.

### Markups

- Images keep their proportions as you resize them, from their corners.
  Turn this off for one image with Lock proportions in its properties, or
  for new images in Settings, which has a new Markups page. Locking a
  stretched image puts it back in shape.
- Drag a PNG or JPEG image onto a page and it is added there straight
  away, selected, with no need to pick the Image tool or click to place it.
  Before, dropping an image did nothing.
- Editing a Cloud+ caption happens in place on the page, instead of in a
  stray panel showing the file name and unrelated buttons.
- A Cloud+ caption box fits its text and stays against its leader, so
  text no longer sits far from the leader when the caption is to the left
  of the cloud.
- Callouts work the same way: their text is edited in place, and its box
  fits the text and grows with extra lines instead of clipping them.
  Double-clicking an existing Callout edits its text, which was not
  possible before. A Callout drawn to the left puts its box on the left,
  instead of over its own leader.
- Deleting several markups at once now removes any dimensions among them.
  Before, a deleted dimension stayed on the page, and its unsaved changes
  could not be recovered after a crash.
- Dimension text and rotated text boxes are edited in a plain text panel,
  instead of the panel of unrelated buttons.
- Selecting or hovering a markup shows small handles at the points you can
  drag, instead of blue dashed lines over and around it, so its own colour
  and line weight stay visible. Leaders no longer turn into a dashed blue
  line, and locked markups show grey handles.
- Pen and Highlight strokes can be resized from handles on their box.
- New Arrows start at 1 pt, like Lines and the other line tools, instead of
  0.5 pt.
- Undo works after saving. It did nothing once a document had been saved,
  because the crash-recovery copy refused to go back past the saved state.

### Tables

- Tables are found only where the PDF says it has one (its tags, as Word,
  Google Docs, LibreOffice and Acrobat write them), never guessed from
  ruled lines. Text that ran past a cell border, or a table that grew into
  the text below it, no longer stops a table being a table.
- Right-click a table cell in Edit PDF to insert a row above or below, or
  delete the row. Drag a column border to resize the two columns beside
  it; their text rewraps to fit.
- Edit PDF's paragraphs are the document's own, from its tags: lines the
  PDF says are separate paragraphs, such as an address block, are no
  longer joined into one because they sit close together. A PDF without
  tags says nothing of its paragraphs, so its lines edit one at a time.
- Saved tables stay tagged as tables, with new rows, columns and cell text
  in their right place, so they edit as tables when opened again and
  screen readers read them as tables.

### Editing PDFs

- Edit PDF is now a switch at the left of the toolbar above the page (or
  Shift+T) instead of a tool in the sidebar. While it is on, the Select
  tool works on the PDF's own content wherever no markup or form field is,
  so markups can still be selected and drawn without changing tools.
- Edit PDF is also in the page's right-click menu, ticked while it is on.
- Paragraphs, drawings and images can be moved: drag a drawing or image,
  or a paragraph by the edge just outside its text (Shift-click to add
  more, or drag a lasso around them), or nudge the selection with the arrow
  keys. Inside a paragraph the pointer is a text cursor and a click types
  there. The content itself follows the pointer smoothly as you drag or
  resize it, and stays put when you let go while the page catches up;
  markups on it move with it. Each move is one undo step and is saved into the page.
- Holding an arrow key moves the selection smoothly, like dragging, and
  the whole hold is one undo step. Shift with an arrow moves it 10 points
  at a time. A hover box no longer stays behind where content was before
  a keyboard move.
- Moving content snaps like moving markups: to markups, other content and
  the grids, with the same guides. The boxes round paragraphs, drawings
  and images are snap targets too, along their sides as well as at their
  corners, for content and markups, and a paragraph snaps by its box. Select's lasso selects content and
  markups together, and they move together.
- A dragged paragraph, drawing or image also lines up with the others on
  the page: its sides or centre snap level with theirs from any distance,
  with a green guide between them.
- A line with a word in another colour, such as a bold name in black in a
  grey sentence, is one line again. Before, it came apart into separate
  pieces that moved and edited on their own.
- Content is framed by one thin box only when the pointer reaches it,
  without tinting it, and the same box shows it hovered, selected or
  edited. Clicking away from text being edited only ends the edit.
- A selected drawing or image has resize handles, which look and behave
  like markups' handles; hovered and selected content use the same blues
  as markups. Delete removes selected
  content, and Copy, Cut and Paste make copies of it on the same page.
- Escape ends a text edit and leaves the paragraph selected.
- Pressing Return at the end of text moves the caret to the new line at
  once, instead of leaving it beside the return mark until you type.
- In a table cell, Return at the end grows the row and pushes its borders
  down straight away, not only once the new line has text.
- A drawing inside its own small clipping area, such as an SVG graphic,
  moves as one piece with its clip and any text in it.
- File > Convert and Open in Word has Microsoft Word convert the PDF,
  saves it beside the PDF with the same name as a .docx and opens it in
  Word. When that .docx is already there, it asks whether to replace it
  or keep both (the new one as Name 2.docx and so on).
  Unsaved changes are saved first. The first time, macOS asks to let
  Butter Paper control Word. Needs Word on macOS or Windows.

### Editing tables

- Text typed into a narrow cell wraps onto new lines that show. In
  documents printed from Google Chrome, a cell's text is clipped to its
  first line's height, and the lines below were cut off.
- Edit PDF edits tables like a word processor: text in a cell
  wraps at the cell's width, and its row grows to fit, moving the rows
  and borders below it down. Empty cells take typing too.
- Tab and Shift+Tab move between cells, selecting the next cell's text.
  For bigger changes to a table, convert the PDF to Word.
- Only the table changes as it grows: the text and drawings below it stay
  where they are. Where the table runs into other text, a small label
  under it says so and the text it covers is framed in amber; drag that
  text out of the way by its edge, even where it lies inside the table.
  A table that runs past the bottom of the page is labelled too, and so
  is an edited paragraph that does either.
- Undo reaches back past a save on a page whose tables changed.
- Typing in a table on a page with a lot of drawing updates the page
  about four times faster.
- Deleting text in a table cell shortens its row.
- Pressing Return at the end of a paragraph keeps empty lines, written as
  a space, as word processors write them.
- Typing is faster: the page you type on is redrawn before its
  thumbnail, which follows once you pause.

## [0.2.3]

### Windows

- Editing no longer makes the page flash. Adding, changing or moving any
  markup, and finishing an Edit PDF Content change, used to drop the page
  to a blurry or blank image until it re-rendered; the page now stays sharp.
  Moving between pages no longer re-renders every thumbnail either.
- Pointing at a markup you can move shows the four-headed move arrow;
  Windows showed the plain arrow because it has no hand cursor.

### Viewing

- Zooming no longer drops the page to a blurry image for a moment: the
  page stays drawn at the previous zoom until the new one is ready.

### Updates

- Every copy has Check for Updates in the Butter Paper menu. In the
  Microsoft Store version it opens Butter Paper's Store page.
- Checking for and downloading an update shows in a slim status button at
  the right end of the menu bar, with the download's progress; Settings
  shows its size and percentage too.
- Updates settings list Automatic checks first, and show What's new only
  when there is an update to install.

## [0.2.2]

### Viewing

- In Single Page view, turning the mouse wheel down now goes to the next
  page and turning it up to the previous one; it was reversed.
- In Single Page view, a page larger than the window scrolls with the mouse
  wheel, arrow keys and Page Up/Page Down until its edge is in view, then
  goes on to the next or previous page at the same zoom and position:
  scrolling down shows the top of the next page, scrolling up the bottom
  of the previous one. A page that fits the window turns with each notch
  or key press.
- Arrow keys and Page Up/Page Down no longer push the page out of view.
- Fit Page keeps the page centred when you turn pages, instead of shifting
  it down slightly.

## [0.2.1]

### Large documents

- Clicking a page thumbnail goes to that page at once, showing its thumbnail
  until the page renders, and a page you click is rendered before anything
  else. In a large drawing set this takes under a second instead of up to
  ten.
- Scrolling far through the page thumbnails no longer slows the app down:
  only the thumbnails you stop on are drawn, starting as soon as you stop.
- Page thumbnails are sharper.
- The page you are looking at no longer flashes while thumbnails load on
  Windows and Linux, and no longer shows another page's image behind it
  after scrolling in Continuous view.
- Memory is released as you move through a document, instead of growing
  with every page viewed.
- Large PDFs open faster.

### Viewing

- Select page text with the Select tool to copy it: drag across it,
  double-click a word or triple-click a paragraph. Markups and form fields
  still take a click first.
- In Single Page view, each mouse-wheel notch turns a page, so you can flick
  through a document as fast as the wheel turns.
- No more thin dark lines between parts of the page while zooming.
- The mouse-wheel menus are only as wide as their choices.
- Page thumbnails no longer have a Set Page Scale button, and their rotate
  buttons are named Rotate Left and Rotate Right.

### Windows

- Set as Default PDF App opens Windows Default Apps instead of File
  Explorer.

## [0.2.0]

### Recent files

- Recent Files only lists PDFs that are still there. A PDF you move or rename
  on the same drive stays in the list at its new location on macOS and
  Windows; one you delete or move to the Trash or Recycle Bin leaves it.

### Windows

- Show the Butter Paper icon on the taskbar and title bar of the GitHub
  build, and make the Store icons fill their space on the taskbar, Start and
  in File Explorer.
- In the Store version, Settings and About link to Butter Paper in the
  Microsoft Store instead of the GitHub releases.

## [0.1.10]

### Forms

- Fill in PDF forms with the Select tool: text fields (multi-line, password,
  maximum length and comb), check boxes, radio buttons, and combo and list
  boxes. Fields are outlined while Select is active, Tab and Shift+Tab move
  between them, and each value is one undoable change.
- Save filled values with appearances other PDF apps show, including text
  in other scripts, and draw fields that have no appearance from their
  values.

### Edit PDF Content

- Rename Edit Text to Edit PDF Content, to set it apart from filling forms.

### Windows

- The download page sends Windows visitors to the Microsoft Store, for x64
  and Arm PCs. The GitHub release still has Windows builds, which keep
  updating themselves.

### Fixes

- Draw Chinese, Japanese and Korean text in Text Boxes, form fields and
  edited text at the regular weight; the bundled fallback font was the thin
  one.
- Show Windows paths in Recent Files without the `\\?\` prefix.
- Put the X that closes an opening error in the error's corner.

## [0.1.9]

### Windows

- Install Butter Paper from the Microsoft Store, which keeps it up to date.

### Updates

- Keep the Update ready button's tooltip from covering its menu.

## [0.1.8]

### Edit Text

- Find lines and paragraphs as a reader sees them, including in multi-column
  layouts such as CVs, and mark hard and wrapped line ends with ↵ and ⤵.
- Show every editable paragraph's box, lined up with the rendered text.
- Make Edit Text properties behave like the other tools' properties.
- Wrap growing headings and table cells before the page edge and the text
  beside them.
- Edit text in more PDFs: LaTeX and other subset fonts, Office maths, the
  Symbol font, broken Unicode maps and pages drawn upside down. Right-to-left
  text is read-only.
- Save text edits much faster in large PDFs by appending only what changed.

## [0.1.7]

### Edit Text

- Add the Edit Text tool (`Shift+T`) for editing a PDF's own text a
  paragraph at a time. Text reflows within the paragraph, keeps its
  alignment and never moves other content; untouched lines keep their bytes.
- Use the embedded font, then an installed font of the same name, then a
  bundled family, so a missing font never blocks editing. The properties
  panel names any substitute.
- Change font, size, bold, italic, colour, alignment and line spacing from
  the Edit Text properties.
- Continue bullets and numbering when Enter adds a list item, and flow
  paragraphs from one column into the next.
- Support input methods, word and line selection, clipboard and undo while
  editing; warn before editing a digitally signed PDF.

### Markups and saving

- Resize Ellipses, Text Boxes, Images, Snapshots and Redactions from anywhere
  along their edges, selected or not, as Rectangles already do.
- Place exported Text Box lines where the canvas shows them.
- Keep owner-password protection and its permissions when saving such a PDF.
- Explain quit and close recovery problems in plain language.

### Window and tabs

- Show an update button only when a downloaded update is ready, with Restart
  to update and release notes in its popup.
- Mark unsaved tabs with an asterisk in a small circle and keep filenames in
  place when the close button appears.
- Group the tool rail into General, Edit, Review, Draw and Measure, with a
  cue when more tools are scrolled out of view.

## [0.1.6]

### Navigation and drawing

- Keep two-finger trackpad gestures scrolling, with Ctrl-scroll for zoom;
  preserve the document position during rapid pinch-to-zoom gestures.
- Clarify the Mouse wheel menu with Zoom and Scroll choices and a reminder
  that holding Ctrl performs the opposite action.
- Add a full-width contextual help bar with drawing gestures, navigation
  shortcuts and tooltips that fit narrow windows.
- Keep drawing cursors visible when switching tools without moving the mouse.
- Prevent clicks on controls over a scrolled page from starting canvas edits.
- Show live previews for dragged Clouds, clicked paths, Cloud+'s first
  segment, Arc construction, Length measurements and Snapshot boundaries.
- Explain click and drag behaviour in each drawing tool's help.

## [0.1.5]

### Settings and updates

- Add About Butter Paper and Settings to the application menus, with a
  dedicated Settings window and keyboard shortcut.
- Use a category rail, heading shortcuts and one scrollable Settings view;
  narrow windows switch to compact navigation.
- Show a quiet Update available button beside the template control, including
  when the in-app menu bar is hidden. It opens Updates Settings with release
  notes; downloads start when you choose Download update.
- Show update check results and errors in Settings. After downloading, offer
  Restart to update while preserving the normal unsaved-work decision flow.
- Default automatic checks to startup for beta builds and weekly for stable
  builds, preserving existing saved preferences.

### Popup and tab styling

- Give popovers, menus and selects the standard GPUI medium shadow, while
  keeping the stronger dialog shadow.
- Use circular hover, pressed and focus backgrounds for tab close buttons.

## [0.1.4]

### Selecting and editing markups

- Move and resize rectangles directly from hover, with the same hit areas as
  selected rectangles. Wider resize zones have move zones on both sides;
  the middle of an unfilled rectangle stays clear for selecting other items.
- Show an open hand over movable markups before selecting them, and a closed
  hand while moving. Selection outlines, handles and dashed rotation stems
  stay visible during moves, resizes and rotation.
- Double-click a rotation knob to reset the markup to zero degrees.
- Rotated rectangles save and reopen correctly despite PDF number rounding.
- On Windows, save changes back to the opened PDF through an atomic staged
  replacement, checking that the file is still the one originally opened.

### Tabs, panels and recovery

- Use consistent controls, layouts and top-right close buttons across
  properties, settings, templates and other transient panels.
- Keep dialog text and actions inside the window, including long filenames
  and unsaved-change prompts.
- Replace sprawling recovery banners with a conditional recovery button and
  a contained panel for reviewing, restoring or dismissing recoverable work.
- Show recently opened files in an empty window, with their folder paths,
  full-path tooltips and a separate button to reveal each file in its folder.
- On macOS, Command-W closes the window when no document tabs remain.

### Viewing and installation

- Keep fit, zoom and page navigation consistent across viewing modes, panning
  and window resizing.
- Remove the inset page border that could leave doubled lines along page
  edges.
- Use clear application names in native menus and packages. Windows installs
  share one owned Start menu shortcut, and updates retire the previous
  package without breaking the PDF default-app choice.

## [0.1.3]

### Updating

- Windows: updates finish installing. Until now the update installed the new
  version beside the old one but never switched over, so Butter Paper kept
  opening the old version. If you have more than one Butter Paper entry in the
  Start menu, open the newest; this version and later ones update in place.

## [0.1.2]

### Selecting and editing markups

- The blue dashed outline now sits just outside a selected markup, so the
  markup itself stays visible, and the yellow handle dots are gone.
- Resize a selected rectangle from anywhere along an edge, or from a corner;
  the pointer shows the resize direction and keeps it while you drag.
- Drag inside a selected markup, or along its dashed outline, to move it
  (open hand to grab, closed hand while moving). The rotation knob stays on
  the outline.
- Double-click a text box to edit its text in place on the page, without the
  properties panel.
- New text boxes pad their text evenly, so a single line sits in the middle.

### Viewing

- Pan freely past the edges of the page into blank space, as in the Electron
  app, and zoom with the pointer over blank space without the page jumping.

### Signatures

- Phone signing works again: it goes through the secure relay, as the
  Electron app did, and opening Signature shows the phone QR code straight
  away (Draw here instead, Type and Image are still there).

### Tabs and dialogs

- Hovering a tab no longer shifts its title; the title truncates to make
  room for the close button.
- The template list and preview split the space evenly, in both New from
  template and Manage templates.
- Dialog close buttons respond to hover across the whole button.

## [0.1.1]

### Tools

- Cloud: drag to draw a rectangular cloud, or click its corners and
  double-click to finish.
- Polyline and Polygon finish with a double-click, as in Revu (Enter still
  works).
- Snapshot: drag a box to capture it (two clicks still work).
- Length, Polylength and Area measure straight away on a page without a
  scale, at Revu's default of 1 in = 1 in. Set the page scale to change it.
- Every drawing tool is now covered by a test that draws with it the way
  you would.

### Scrolling and zooming

- Trackpad pinch zooms smoothly about the point between your fingers.
- Ctrl+wheel zoom goes the natural way: scrolling up zooms in, whichever way
  macOS natural scrolling or tools such as LinearMouse set the wheel.
  Reverse Scroll Zoom still flips it.
- Zoom keeps the point under the pointer in place.
- The document's scroll bars stay on the edges of the view instead of
  moving with the page.

## [0.1.0]

Butter Paper is now a single native app built from a clean Rust codebase,
with the old Electron and migration-era code removed. From this release
Butter Paper updates itself on macOS, Windows and Linux, and Butter Paper
Beta (macOS) follows beta releases.

### Updating to 0.1.0

- Homebrew: `brew upgrade --cask --greedy butter-paper` (or `butter-paper@beta`); `--greedy` is needed once, because older versions could not update themselves.
  If Homebrew says the Beta app "is not there", the move from Electron already
  replaced it: run `brew uninstall --cask --force butter-paper@beta`, then
  `brew install --cask --force butter-paper`.
- Butter Paper 0.0.26 cannot update itself, so install 0.1.0 by hand once:
  on macOS replace the app; on Windows run the new `install.ps1`, then the
  old version's `uninstall.ps1`; on Linux run the old version's
  `uninstall-user.sh`, then the new `install-user.sh`. Your settings,
  templates and signatures carry over.
- The Electron app updates itself to 0.1.0. As with the move to 0.0.26, the
  native app starts with fresh settings; your Electron files stay on disk.

### Packages

- Each package carries complete third-party notices for everything it
  contains, including every Rust library.
- Butter Paper Beta has its own icon.

### Markup files

- Markups are saved exactly as Bluebeam Revu saves the same tool: standard PDF
  and Revu's own keys only, no Butter Paper private data. Revu reads every
  Butter Paper markup with the same properties as its own, and Revu's markups
  open in Butter Paper as the matching tool.
- Markups from other apps that you have not edited are saved unchanged, byte
  for byte. Page scales are stored as Revu does, so Revu shows them too.
- Files saved by older Butter Paper versions are no longer read specially:
  their markups open as ordinary PDF markups.
- Measurements show Revu's figures: thousands separators, "sq" area units, and
  page-scale lengths no longer drift by a few parts per million.
- Files Revu has edited and saved incrementally now open correctly.
- Length measurements and dimensions are drawn as Revu draws them: extension
  lines, a dimension line 10 pt out, closed arrowheads that move outside when
  the caption fills the gap, and the caption centred on the line.
- New dimensions start without a label, like Revu's; you can still add one.
- Callouts and Cloud+ text draw a border round their text whenever the line
  width is not the default 1 pt, as in Revu.
- Clouds can be filled. Revu clouds keep their fill when you edit them.
- Callouts whose text was deleted in Revu open as callouts, and dash patterns
  from Revu survive edits that keep the same line style.
- Text boxes, callouts and Cloud+ text use the margin stored with the text
  (3 pt for new text, as in Revu) on screen and in the saved file.
- Area captions are centred on the area, as in Revu.
- Clouds have Revu's round curls with small hooks where they meet, on
  screen and in saved files.
- Cloud+ markups can be filled.
- Length and dimension captions turn to follow sloped lines, as in Revu.
- Revu snapshots can be moved, resized and rotated. Butter Paper keeps
  Revu's own vector drawing in the file.
- Pressing Return to finish a callout, Cloud+ or dimension label no longer
  replaces the selected text with a line break.

### Updates

- Butter Paper now updates itself. It checks GitHub weekly by default (change
  this under Butter Paper > Check Automatically, or use Check for Updates…),
  downloads the new version in the background, verifies it against the
  published checksums (and, on macOS, its Developer ID signature), and installs
  it when you quit or choose Restart. Beta copies also receive newer stable
  releases.
- Butter Paper Beta for macOS is published with each release. It installs
  beside Butter Paper, keeps its own settings and stays on the Beta channel.

### Windows and tabs

- File > New Window opens another window. Drag a tab to reorder it, onto
  another window's tab bar to move it there, or away from the window to open
  it in a new one. Document > Move Document to New Window does the same.
- Each window reopens where you left it, with its own documents.
- Window titles list the open documents, active one first.

### Fixes

- Single-key tool shortcuts keep working after using a menu, a dialog or the
  inspector; clicking the page returns them.
- Clicking to close a menu, popup or dialog no longer starts a selection box or
  draws on the page.
- Drawing and moving markups is much faster on large drawings with many
  markups: snapping no longer rescans every line on each mouse move, and the
  app no longer rebuilds unrelated state while you drag.
- The app icon is larger and uses the macOS 26 icon style.
- Windows and Linux no longer use Ctrl+Q to quit; closing the last window
  quits, as on those platforms.

## [0.0.30]

### Move to the native app

- Moves to the native Butter Paper 0.0.26 app automatically after this update
  restarts: Butter Paper downloads the verified native package for your
  computer, installs it, removes the old Butter Paper apps and opens the new
  one. Your PDFs are not changed.
- macOS 12 stays on this version, because the native app requires macOS 13 or
  later.

## [0.0.29]

### Move to the native app

- Fixes the move to the native Butter Paper 0.0.26 app, which did not start in
  0.0.27. On first launch, Butter Paper asks to update, then downloads the
  verified native package for your computer, installs it, removes the old
  Butter Paper apps and opens the new one. Your PDFs are not changed.
- macOS 12 stays on this version, because the native app requires macOS 13 or
  later.

## [0.0.27]

### Move to the native app

- Moves existing Butter Paper and Butter Paper Beta installs to the native
  Butter Paper 0.0.26 app. On first launch, Butter Paper asks to update, then
  downloads the verified native package for your computer, installs it,
  removes the old Butter Paper apps and opens the new one. Your PDFs are not
  changed.
- macOS 12 stays on this version, because the native app requires macOS 13 or
  later.

## [0.0.26]

### Native desktop application

- Rebuilt Butter Paper as a native GPUI application for macOS, Windows, and
  Linux, with packages for both ARM64 and x64 systems.
- Preserved the core PDF editing workflow, including annotation tools,
  properties, templates, signatures, safe saving, recovery, and Electron data
  migration.

### Rendering and release delivery

- Added isolated PDF rendering workers, production PDFium preparation, and
  deterministic cross-platform package verification.
- Added signed and notarised macOS packages plus integrity-verified unsigned
  Windows and Linux packages for this first native release.
- Optically lowered the butterfly glyph within the application icon for more
  balanced platform presentation.

## [0.0.25]

### Startup performance

- Made blank and PDF launches faster by delaying non-essential updater,
  registration, template, signature, and PDF services until they are needed.
- Reused inspected PDF bytes during document opening to avoid reading the same
  file twice, while keeping PDF preparation available for direct file launches.

### Templates and verification

- Improved long template-name layout and kept the last-template preview within
  constrained windows.
- Added detailed startup timing diagnostics and expanded deterministic and
  packaged checks for blank documents, direct PDF launches, and lazy runtime
  boundaries.

## [0.0.23]

### Rendering and navigation

- Added adaptive rendering that responds to display refresh rate, available
  memory, processor capacity, render latency, and PDF.js backlog pressure.
- Improved page sharpness during normal scrolling while reducing background
  prefetch and high-quality work when the device or document is under load.
- Added direct arrow-key viewport scrolling, Page Up and Page Down scrolling,
  Home and End document navigation, and primary-modifier page navigation.

### Signatures and document controls

- Improved phone signature drawing controls and added context-menu deletion for
  individual recent signatures.
- Kept template and snapping popovers within constrained windows and removed
  obsolete blank-PDF settings compatibility code.

### Release delivery

- Reduced duplicate native release checks and artifact transfers, parallelized
  post-publication verification, and added non-publishing release simulations.
- Added an exact-commit candidate promotion simulation that verifies asset
  names, checksums, attestations, and update-feed sealing without publishing.

## [0.0.22]

### Templates and document workflow

- Added a reusable blank PDF template library with built-in and custom
  templates, PDF import, live previews, last-template reuse, and direct access
  from the document tab bar.
- Added cloud-storage source detection and opening progress for large PDFs.
  Improved safe save and Save As handling around opaque file access grants.
- Improved page thumbnails, blank-page previews, document opening feedback,
  constrained-window layout, and menu and rail controls.

### Markup tools and properties

- Added redaction placement, locked-markup handling, calibration-line feedback,
  dimension-increment snapping, and more consistent selection hit zones.
- Expanded tool properties with bundled standard annotation fonts, live text
  appearance updates, compact colour controls, persistent sortable colour
  presets, and construction-grid feedback.
- Improved annotation import and export fidelity for appearance, geometry,
  rotation, text, line endings, images, snapshots, and page content.

### Verification

- Added deterministic coverage for templates, saving, storage progress,
  calibration, colour presets, PDF round trips, phone-transfer limits, and
  packaged drag-and-drop behavior.
- Allowed explicitly isolated local macOS GUI checks while keeping updater
  replacement tests restricted to disposable GitHub Actions runners.

## [0.0.21]

- Introduced the Butter Paper brand identity with Trace Teal, Markup Coral,
  Studio Violet, Vellum, and Carbon across the repository and download page.
- Replaced the stable and beta application icons with the exact faceted origami
  butterfly geometry and channel colours derived from the master palette.
- Optically sized the transparent icon artwork to fill 80% of the canvas for
  clear native presentation on macOS, Windows, and Linux.

## [0.0.20]

### Signatures and image appearances

- Added a complete visual-signature workflow for drawing a signature, typing a
  signature with the bundled Allura face, importing a PNG or JPEG, or capturing
  a signature with a camera. Signatures remain ordinary image appearances and
  do not add certificate signing or claim cryptographic document approval.
- Added automatic paper removal, ink normalization, edge smoothing, transparent
  cropping, image-dimension limits, and isolated image decoding before a
  signature can enter the document or recent-signature store.
- Added up to five encrypted recent signatures with move-to-front reuse,
  explicit per-item deletion, clear-all handling, corruption checks, bounded
  operations, safe temporary files, and fail-closed behavior when secure system
  storage is unavailable.
- Added the end-to-end encrypted phone-transfer client and a disabled-by-default
  Cloudflare relay package with short-lived capability URLs, authenticated
  encryption, one-time payload acknowledgement, strict size and attempt limits,
  restrictive browser headers, and rate limiting. Public builds keep phone
  transfer unavailable until a reviewed HTTPS relay origin is configured.
- Added narrowly scoped macOS camera entitlement and usage descriptions. The
  runtime permission policy accepts only main-frame video from the application
  renderer and rejects microphone and unrelated permission requests.

### Markup creation and direct manipulation

- Reworked rectangle, ellipse, arc, line, polygon, polyline, pen, highlight,
  image, cloud, Cloud+, and callout placement around consistent click, drag,
  completion, cancellation, and post-placement selection behavior.
- Added stable three-point circular arc creation and direct arc hit testing,
  movement, and resizing. Added Shift-constrained circles and orthogonal segment
  placement without incorrectly constraining arc bulge points.
- Improved polygon and polyline node placement, Enter and Escape completion,
  start-node closure, screen-space cloud closure, multiline compound-markup
  growth, and native Cloud+ leader restoration.
- Added transient move and resize previews so pointer cancellation discards an
  edit and a completed drag creates only one document revision.
- Improved overlapping-markup targeting with topmost hit testing, direct handle
  interaction, double-click property opening, and selection behavior that stays
  consistent between the visible preview and committed edit.
- Added a tool cursor that combines a crosshair with the active markup glyph,
  and kept navigation cursors separate from drawing and direct manipulation.

### Snapping and tool properties

- Added acquired tracking points, horizontal and vertical tracking paths,
  virtual intersections, edge projections, PDF-content intersections, and
  precise-role priority within a viewport-pixel snap tolerance.
- Added equal-width, equal-height, equal-gap, and repeated-spacing snapping for
  new, moved, and resized annotations, with dedicated guide bars and source
  highlighting.
- Separated snapping behavior from guide visibility. Users can keep snapping
  active while hiding all guides or selected guide types, and real resize nodes
  remain visible when snapping is disabled.
- Added per-tool default property storage and reset controls. New annotations
  receive the active tool defaults without changing existing markups.
- Refined tool property panels, snap settings, confirmation controls, keyboard
  focus, and constrained-window scrolling with standard shadcn/Base UI controls.

### Blank PDFs and page controls

- Expanded blank PDF creation with A5 through A0 ordering, portrait and
  landscape controls, custom dimensions, page count, background color, and an
  exact aspect-ratio preview.
- Added grid, dot, lined, isometric, and triangle paper with configurable
  spacing and alphabetized pattern colors. Generated patterns use bounded PDF
  content and reject settings that would create excessive output.
- Improved page-scale presets, calibration interactions, thumbnail actions,
  pattern round trips, page rotations, and native image/snapshot appearance
  reuse.
- Improved thumbnail sizing for landscape and portrait pages and aligned the
  annotation overlay with capped preview geometry.

### Desktop shell and document workflow

- Added a native-height custom title bar with integrated window state,
  macOS full-screen behavior, menu-bar visibility control, and accessible
  stable and beta window identities.
- Added application zoom shortcuts, protected tab and window close shortcuts,
  macOS quit handling, document Home and End navigation, neighboring-page
  navigation, and canvas copy, paste, and select-all commands.
- Replaced blocking dirty-document prompts with anchored save and discard
  confirmations for tab closing and other local destructive actions.
- Improved toolbar, rail, tab, menu, popover, sidebar, and page-scale behavior
  at constrained sizes. Tool shortcuts remain available from non-editing
  controls without stealing keys from text fields, overlays, or system chords.
- Opened dropped PDFs in new tabs through an operating-system-backed path grant
  instead of relying on the removed renderer file-path property.
- Added macOS application registration repair that removes stale stable and beta
  registrations before registering the current installed application.

### Brand, packaging, and verification

- Replaced the quill artwork with the reviewed glass-document icon: a warm
  trace-paper slab, monochrome document content, a revision cloud, and a callout.
  Stable uses ruby-red annotation glass and beta uses violet annotation glass.
- Added canonical 1024 px light and dark artwork, reproducible Blender scene
  sources, adaptive macOS layers, and regenerated Windows ICO, macOS ICNS,
  Linux PNG, README, and download-page assets from one checked pipeline.
- Added third-party license notices for Signature Pad, Allura, QRCode, and its
  transitive QR dependency, and included the notice file in desktop packages.
- Expanded release verification for camera entitlements and usage strings,
  adaptive raster icon sources, dropped-file authorization, app registration,
  signature isolation, encrypted storage, and the disabled production relay
  boundary.
- Added broad deterministic coverage for markup placement, overlap handling,
  snapping, tool defaults, signatures, blank-paper patterns, window controls,
  menus, keyboard navigation, and packaging. The complete release gate now
  passes 995 deterministic application and relay tests.
- Windows and Linux users upgrading directly from 0.0.11 still need one manual
  installation to bootstrap authenticated updates. Later TUF-enabled releases
  continue to update automatically.

## [0.0.19]

- Made CAD View an explicit opt-in while preserving safe snapshot restoration
  when the feature is unavailable.
- Stabilized toolbar tooltip behaviour across pointer and keyboard interaction,
  including constrained popover and menu controls.
- Replaced the stable and beta icon artwork with the approved textured quill
  and faceted ink-pot mark, regenerated for macOS, Windows, Linux, and the
  download page, with dark artwork using the exact light-artwork black-to-white
  contour swap.
- Added deterministic checks for the icon geometry, channel palettes, release
  asset contract, download-page assets, Homebrew publication, and non-signing
  release boundary.

## [0.0.18]

- Redesigned the PDF workspace with clearer toolbar and rail organisation,
  resizable property panels, concise control hints, constrained menus, and
  consistent Base UI Nova controls.
- Expanded annotation workflows with improved snapping and visible snap
  markers, richer property controls, context menus, page rotation, refined
  selection and markup geometry, and stronger Cloud, Cloud+, Callout, and
  measurement behaviour.
- Added a complete blank-PDF workflow with reusable page settings and integrated
  it with document tabs, thumbnails, open, Save As, and normal PDF.js rendering.
- Removed the unused native Rust/PDFium renderer so the desktop app has one
  supported PDF.js rendering path, fewer native build requirements, and no
  dormant backend selector.
- Hardened renderer file access with owner-scoped document and Save As
  capabilities instead of renderer-supplied privileged paths.
- Added development source freshness checks, exact build provenance in the
  development window title, stale-renderer rejection, and repeatable desktop
  performance measurements for empty, one-page, and 100-page workloads.
- Replaced the stable and beta application icons with refreshed,
  channel-specific artwork across macOS, Windows, and Linux, simplifying the
  composition to one centred, faceted pen, and added a public download page
  with deterministic copy and asset checks.
- Expanded deterministic and hosted GUI coverage for the redesigned shell,
  properties, snapping, context menus, blank PDFs, page layouts, accessibility,
  constrained windows, and application provenance.

## [0.0.17]

- Standardized annotation interaction chrome across tools: hovering now shows
  faint blue geometry with pale handles, selected objects use dark blue chrome
  with solid yellow handles, and a hovered handle becomes visibly actionable.
- Added direct manipulation of unselected handles without forcing selection;
  clicking a handle still selects its object, and the first click after placing
  an annotation now deselects it instead of creating an accidental duplicate.
- Added CAD-style multi-object selection with Shift toggling, left-to-right
  contained selection, right-to-left crossing selection, freeform lasso,
  explicit add/remove zones, directional blue/green styling, operation badges,
  and cursor-following hotkey guidance.
- Simplified the annotation rails into stable Review and Draw groups while
  preserving resizing, overflow access, tooltips, accessibility, and the custom
  Butter Paper tool icons.
- Restored saved desktop window size and position safely across launches and
  display changes, while retaining the reviewed default and minimum geometry.
- Reworked cloud scallops as continuous cubic lobes with matching sampled hit
  geometry for a cleaner Bluebeam-style appearance and dependable selection.
- Expanded deterministic and hosted Electron coverage for annotation creation,
  hover and handle behavior, multi-selection geometry, window restoration,
  rail layout, keyboard accessibility, and shell overflow.

## [0.0.16]

- Replaced the experimental Butter Canvas surface with native blank PDFs that
  can be created in standard paper sizes, orientations, and page counts, then
  saved, annotated, and reopened through the normal document workflow.
- Refined the Nova desktop shell with integrated closable and reorderable
  document tabs, configurable annotation rails, streamlined tool controls, and
  safer unsaved-document handling.
- Standardized new desktop windows at 1200×800 with a 900×600 minimum while
  preserving accessible menus, popovers, dialogs, scrolling, and constrained
  layouts.
- Refreshed the renderer against the official shadcn Base UI Nova registry,
  moved typography to Geist Variable, and preserved Butter Paper's custom AEC
  and document-view icon geometry.
- Changed automatic-update defaults for new installations to weekly on the
  stable channel and daily on the beta channel. Existing saved preferences are
  preserved.
- Expanded deterministic and hosted Electron coverage for blank-PDF lifecycle,
  document tabs, shell overflow, rail behavior, update scheduling, and
  repository UI-component hygiene.

## [0.0.15]

- Added immediate, accessible in-app feedback for manual update checks,
  including checking, authenticated download progress, up-to-date,
  failure/retry, and ready-to-install states.
- Kept automatic checks quiet until an update is ready, while ensuring every
  manual check ends with visible success, no-update, or error feedback.
- Allowed the update progress window to close without cancelling its background
  download; the ready-to-install prompt still appears when that download
  completes.
- Preserved isolated stable and beta application identities and feeds. This
  stable release advances both products without converting beta installations
  into the stable app.

## [0.0.14]

- Updated Electron to the supported 43 release line while preserving macOS 12
  compatibility.
- Refreshed the development and packaging toolchain to patched releases.
- Corrected the packaged TUF dependency graph and patched its glob-matching
  dependency so delegated update metadata can be evaluated safely.
- Added package-boundary checks that execute delegated TUF path matching from
  the actual packaged application archive on macOS, Windows, and Linux.
- Updated electron-builder to use an NSIS-compatible ARM64 payload filter,
  removing the duplicated Windows executable and DLL workaround and adding
  package-size regression gates.
- Preserved isolated stable and beta application identities and updater feeds.
  Stable releases advance both feeds; beta prereleases advance only beta.
- Windows and Linux users upgrading directly from 0.0.11 still need one manual
  install to bootstrap authenticated updates. Existing 0.0.13 installations
  update automatically.

## [0.0.13]

- Added authenticated automatic updates on native Windows ARM64/x64 and Linux
  ARM64/x64 AppImages using an embedded, reviewed TUF root. Windows and Linux
  users upgrading from 0.0.11 must install 0.0.13 manually once to bootstrap
  the new updater; later releases can update automatically.
- Added fail-closed update verification for corrupt, expired, incorrectly
  signed, missing, redirected, mismatched, and path-escaping metadata, while
  preserving any newer TUF root already trusted by an installed application.
- Added native Windows install/update/restart/data-preservation/uninstall and
  Linux AppImage update/restart/data-preservation release gates on matching
  ARM64 and x64 hosts. DEB and RPM upgrades remain managed by the system
  package manager.
- Added an offline-root/three-online-key TUF signing ceremony, protected
  metadata refresh workflow, deterministic release projections, and
  independent public-feed verification.
- Windows installers and Linux packages remain unsigned in this release.
  Published SHA-256 checksums, GitHub provenance, and TUF-authenticated
  automatic-update metadata protect distribution without implying platform
  code signing.
- Added native PDF file registration and a user-controlled Set as Default PDF App command on macOS, Windows, and Linux.
- Opened PDFs delivered at startup, through macOS open-file events, or to an existing application instance.
- Made this changelog the source for GitHub release notes and the automatic updater's in-app release notes.
- Reduced packaged-app overhead by retaining only the English Electron locale, excluding source maps, and keeping renderer-only libraries out of production dependencies.
- Kept Electron E2E, packaged-app smoke, and updater GUI verification off local
  macOS desktops; these checks now run only on disposable GitHub Actions hosts.

## [0.0.11]

- Removed the unused Edit, View, and Document menus from the application menu bar.
- Moved Set Page Scale into the PDF viewer toolbar so preset, custom, and calibrated page scaling remains directly accessible.

## [0.0.10]

- Preserved the native adaptive stable and beta icons introduced for 0.0.7.
- Kept strict signed-package verification compatible with macOS asset catalogs that expose light/dark stacks without separate artwork-layer metadata.

## [0.0.9]

- Preserved the native adaptive stable and beta icons introduced for 0.0.7.
- Accepted both native-vector and explicit full-canvas Icon Composer dimensions when verifying signed macOS packages across supported Apple runners.

## [0.0.8]

- Preserved the native adaptive stable and beta icons introduced for 0.0.7.
- Accepted both explicit and implicit native Icon Composer system-background layers when verifying signed macOS packages across supported Apple runners.

## [0.0.7]

- Added native macOS Icon Composer artwork with separate light and dark stable/beta appearances that fill the complete adaptive icon canvas.
- Added distinct generated light stable/beta icon assets for Windows and Linux while preserving each platform's native package formats.
- Added deterministic icon generation and packaged-catalog verification, and moved native macOS packaging to the macOS 26 runners required by Icon Composer.

## [0.0.6]

- Canonicalized temporary macOS executable paths before detecting an updater-started replacement process.

## [0.0.5]

- Accepted the conventional standalone argument separator in the native macOS updater harness across pnpm forwarding behaviors.

## [0.0.4]

- Hardened draft release staging with numeric release IDs and exact remote size and SHA-256 verification before publication.
- Preserved hidden static update-feed files in sealed publication artifacts.
- Updated Homebrew cask rendering, auditing, installation, and cleanup for current Homebrew behavior.

## [0.0.3]

- Passed the selected stable or beta identity through every native package verifier.
- Restored the Windows ARM64 executable and Electron runtime libraries after NSIS archive extraction so the native assisted installer contains the complete packaged application.
- Expanded manual package CI to exercise both release channels at native package boundaries before tagging.

## [0.0.2]

- Corrected native release packaging for macOS OpenSSL 3 signing, canonical Linux architecture filenames, Windows ARM64 installation readiness, and slower hosted beta launches.

## [0.0.1]

- Added the first cross-platform Butter Paper desktop release for native Apple Silicon, Intel Mac, Windows ARM64/x64, and Linux ARM64/x64 packages.
- Added separate stable and beta application identities, data directories, branding, and release channels so both variants can coexist.
- Added signed and notarised macOS packages with configurable automatic update checks, dirty-document protection, channel isolation, and native N-1 verification.
- Added deterministic package, release-asset, checksum, updater, accessibility, and document-persistence coverage while preserving the custom AEC, Fit Width, Fit Page, Continuous, and Butter Canvas icons.
- Adopted the maintained shadcn/ui Base UI and Rhea component conventions for standard desktop controls.
