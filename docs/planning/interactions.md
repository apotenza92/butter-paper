# Canvas interactions

How the canvas behaves. Electron (`electron-final`) was the original
reference; entries marked *native* are deliberate departures from it.

## Tools

- Shortcuts: Select `V`, Pan `Space` (hold), Text `T`, Rectangle `R`, Ellipse
  `E`, Arc `Shift+C`, Line `L`, Arrow `A`, Dimension `Shift+L`, Length
  `Shift+Alt+L`, Polylength `Shift+Alt+Q`, Area `Shift+Alt+A`, Polyline
  `Shift+N`, Polygon `Shift+P`, Pen `P`, Highlight `H`, Cloud `C`, Cloud+
  `K`, Callout `Q`, Image `I`, Snapshot `G`. `Shift+T`, or Edit PDF in the
  page's right-click menu (ticked while on), turns Edit PDF on and off
  ([Editing page content](#editing-page-content)). Tool shortcuts
  ignore Cmd/Ctrl and editable or menu targets, including the page text
  editor.
- Holding Space pans and restores the previous tool on release; two Space
  presses within 300ms toggle Pan.
- Holding the middle button is Pan while held: the Pan tool shows active and
  the cursor is a closed hand. The current tool and a markup being drawn are
  kept. A middle double-click fits the view as Revu does: Fit Width in
  Continuous, Fit Page in Single Page.
- A rail click selects a tool; clicking the selected tool again (or
  double-clicking) toggles its properties.
- After placing a markup the tool returns to Select with the new markup
  selected. Text boxes are not selected after commit.

## Placing markups

| Tool | Gesture | Finish |
| --- | --- | --- |
| Rectangle, Ellipse, Line, Arrow, Redact | Drag, or click then click | Release past 3px, or the second click; Escape cancels |
| Length, Dimension | Two clicks | Second click; Dimension then edits its text |
| Polyline, Polygon | Click per point | Double-click or Enter; click the start marker (within 10px) to close |
| Polylength, Area | Click per point | Double-click or Enter; Escape discards |
| Cloud | Drag a rectangle, or click corners for a polygon | Release the rectangle; double-click, Enter or click the start marker for the polygon |
| Cloud+ | Drag a rectangle, or click corners for a polygon | Release the rectangle; double-click, Enter or click the start marker for the polygon; then type its caption |
| Arc | Three clicks: start, end, bulge | Third click |
| Text box | Click | Type in place; click away or Escape commits, empty text discards |
| Pen, Highlight | Drag | Release |
| Image | Click places the chosen image | |
| Snapshot | Drag a box, or click then click | Release or second click |

Shift constrains while drawing (lines and points horizontal or vertical from
the last point, ellipse to a circle) and applies mid-drag. Length, Polylength
and Area measure at 1 in = 1 in on a page without a scale.

Creation previews follow every pointer move during drags and between clicks,
without committing history. Cloud drags preview the rectangular scalloped
outline that will be committed; polygon drafts preview their next point.
Cloud+ shows the first segment before its polygon and caption can be formed.
Arc shows its start-to-end chord before the second click, then its live bulge.
The bottom contextual help explains each tool's click and drag gestures; its
tooltip expands the instructions when the window truncates them.

## Selection and editing

- *Native:* selection never draws a line over or around a markup, so its
  own colour, thickness and style stay visible. A selected or hovered markup
  shows small square handles at the points that can be dragged: line and
  leader ends and knees, vertices, arc controls, box corners and edges (an
  aspect-locked Image's corners only), turned with the markup. Handles are
  white with a blue border (the same for the focused markup), light blue
  while hovered or a marquee candidate, filled under the pointer, and inert
  grey on a locked markup. The rotation knob stays, on a thin solid stem. Handles and
  knobs remain visible while moving, rotating or resizing, including when
  snapping is enabled; creation previews show no handles.
- *Native:* the band that moves a markup is a box turned to it: along a
  line for Lines, Arrows, Lengths and Dimensions, at the rotation for boxes,
  else upright. A Length or Dimension's box holds everything it draws: the
  measured points, extension lines, arrows and caption.
- *Native:* a Length or Dimension end is dragged from anywhere along its
  extension line, out to the arrow; the measured point follows the pointer
  from where it was grabbed, without jumping to it.
- *Native:* Rectangle outlines use three equal 20px bands from outside to
  inside: Move / Resize / Move. Resize straddles the line by 10px each way;
  the adjacent move bands reach from 10px to 30px outside and inside it.
  Hovered and selected Rectangles use the same hit geometry at every rotation
  and zoom. Small Rectangles shrink the inward resize and move bands together
  to keep the central half of the shorter side empty. Corners follow the same
  band boundaries, with diagonal resize feedback in the resize band.
- Ellipses, Text Boxes, Images, Snapshots and Redactions use the Rectangle's
  resize band, selected or not: anywhere along an edge resizes from that
  edge, and near a corner from the corner. An Ellipse's band follows its
  curve, takes the handle at the nearest eighth of a turn, and the grabbed
  point follows the pointer; Move bands lie either side of it, as for a
  Rectangle. Aspect-locked Images resize only from their corners. New
  Images are aspect-locked unless Settings > Markups > Images turns "Keep
  proportions when resizing" off; Lock proportions in an Image's
  properties changes one image, and locking a stretched image returns it
  to its pixels' shape, keeping its width and centre (one undo step). An
  unselected markup's edge takes the press only where the cursor offers to
  resize it. Dragging along the band around a selected markup's outline,
  from outside it to 20px inside, moves it.
- Pen and Highlight strokes resize from eight handles on their box, half
  the line width outside the stroke: the stroke scales into the new box and
  keeps its line width. A stroke flat along one axis has no handles across
  it.
- A markup is hit on its strokes, its fill and its text or image. The empty
  inside of an unfilled shape is not part of it: a press there reaches what
  is underneath, selected or not.
- Double-clicking a rotation knob only turns the markup back to 0°; it never
  opens properties, including when the rotation is already 0°.
- Click selects; Shift-click toggles. Away from page text, a drag draws a
  freehand selection lasso and a click starts a selection box that a
  second click finishes (Escape cancels it). Either one begun rightwards
  selects what is fully inside (blue), leftwards what it touches (green,
  dashed). Shift adds, Alt removes. With something selected, a press on
  empty page only ends the selection, as in Revu.
- Double-clicking a text box, or the caption of a Callout or Cloud+, edits
  it in place (*native*: no properties panel). A Callout or Cloud+ caption
  box fits its text and stays against its leader; a new Callout's box sits
  beyond the drag end, away from the arrow tip. Dimension text and rotated
  text boxes, which cannot be edited where they sit, edit in a sidebar
  text panel holding only the text. Double-clicking other markups opens
  properties.
- Select All works on the current page. Locked markups cannot be moved,
  resized or deleted.

## Selecting page text

With Edit PDF off, the Select tool also selects the page's own text to
copy it, as Acrobat's does; nothing about the PDF changes.

- Markups and fillable fields take a press first. Elsewhere a press in
  text places the start of a selection and dragging extends it, in reading
  order, to the nearest place in the page's text, including across lines
  and paragraphs. A press away from text clears it and draws a selection
  box as before.
- Double-click selects a word, triple-click its paragraph (Edit PDF's
  paragraphs); Shift-click extends the selection.
- Copy puts the selected text on the clipboard, a line break between lines.
  Selecting text clears a markup selection, and selecting a markup clears
  the text selection; Escape and changing tool clear it too.
- The text is what Edit PDF reads, as the page shows it after edits;
  a selection stays on the page it began on.

## Editing page content

Edit PDF edits and moves the PDF's own content (its text, drawings and
images), as Acrobat's Edit Text & Images does. It is a mode, not a tool: a
switch at the left of the viewer toolbar (or `Shift+T`) turns it on for the
document, off when a document opens. While it is on, the Select tool works
on the page's content wherever no markup or fillable field takes a press;
the other tools draw markups as usual, and turning it on arms Select.

For more than Edit PDF does, File > Convert and Open in Word saves any
unsaved changes, has Microsoft Word convert the PDF into `Name.docx` beside
it and opens it in Word. When `Name.docx` exists, a dialog asks first:
Replace (Word closes its open copy first, unless that has unsaved
changes), Keep Both (the new one as `Name 2.docx` and on) or Cancel. The
status bar says while it converts, and what went wrong if it could not:
no Word, no permission to control it, or unsaved changes in the copy open
in Word.

- Markups and fillable fields come first, for hover and press alike. Then
  a press in text edits it, as in a word processor: a click places the
  caret, a drag selects text and a double-click selects the word. A
  paragraph moves from the band just outside it (10 pixels), as a markup
  moves from its edge: a press there, or on a drawing, image or copy,
  selects it (Shift adds to the selection, keeping selected markups) and
  dragging moves the selection; the page shows it moving as it is
  dragged, with the unlocked markups whose centre is on it. Select's
  selection lasso or box selects page content by the same rule as
  markups, and selects both together; dragging either moves both as one
  change.
- With an edit or a selection in hand, a press on empty page ends it and
  does no more, as for markups; the next click or drag there starts a
  selection box or lasso, which selects content too.
- Content is framed, never tinted, and nothing is outlined until the
  pointer reaches it: one thin box, a few pixels off the content at any
  zoom, in markups' hover blue while hovered and their selection blue while
  selected or edited. A
  paragraph's box holds its lines' full height, the same hovered,
  selected or edited. While dragged or resized, the content itself follows
  the pointer every frame, inside its box: its picture as the page showed
  it, with text's and drawings' paper clear so what it passes over shows,
  while the page is shown without it; once let go, the picture stays until
  the page shows the content there. A drag snaps as
  a markup move does (its corners, edge midpoints and centre to markups,
  other page content and the grids, with the same guides); content never
  snaps to itself. The boxes Edit PDF frames paragraphs, drawings and
  images with are targets for content and markups alike: their corners,
  side midpoints and centre, and anywhere along a side whether or not
  Nearest is on. A paragraph snaps by its box. Short of a point to snap
  to, a dragged selection's box lines up with the other boxes on its page
  (its sides or centre level with theirs, on each axis, within the snap
  sensitivity) with Alignment guides on, showing a guide between them.
  The arrow keys nudge the selection a point, Shift+arrow ten; a held key
  moves its picture as a drag does, and the move ends (one change) when
  the key is let go. Each change
  is one undo step; undo and redo end the selection, and Escape ends it.
- A single selected drawing or image shows eight resize handles, drawn as
  markups' handles are (white squares in the selection blue, filled under
  the pointer): dragging
  one scales it from the opposite handle (Shift keeps a corner's
  proportions), snapping the handle. Paragraphs take their size from their
  text, set in the properties.
- Delete or Backspace deletes the selected content and the markups
  selected with it. Copy, Cut and Paste work on it too: Paste adds copies
  10 points right and down (further each time), selected, on the page the
  content was copied from; elsewhere it says so. A copy is drawn in its
  original's colour and line width, and moves, resizes and deletes on its
  own.
- Enter edits a selected paragraph from its end; Escape ends the edit,
  leaving the paragraph selected.
- What a small clip (under half the page) clips, such as an SVG or a chart,
  is one drawing: it moves with its clip and the text inside it, and the
  band round that text moves the drawing (a press in the text edits it).
  Otherwise
  each painted path and each image is its own. A table's rules and fills,
  and a drawing covering most of the page (a background), are not picked:
  tables change through their grips.

### Editing page text

Edit PDF edits text a paragraph at a time, as Acrobat does.

- Lines group into paragraphs: same font, size and line spacing, sharing a
  left, right or centre edge (or both edges, justified), in one column.
  A list (items beginning with the same bullet, or numbered `1.` / `1)`, at
  one edge, with their wrapped lines at the items' text) is one paragraph.
  A paragraph that runs to the foot of a column without ending a sentence
  and continues at the top of the next column of the same width is one
  paragraph over both. In Edit PDF, every editable paragraph on
  the visible pages is outlined faintly (not text outside the page's visible
  area); hovering outlines one strongly. A page's text is read once the view
  settles on it and dropped when the view moves far away, so scrolling a long
  document never queues pages it passes.
- A line is everything drawn on one baseline, however the producer split it
  (Word draws each run in its own clip and graphics state, and its bullets a
  tab before their text; a letter bullet such as Word's "o" counts when its
  font differs from the item's). A line that ends where the next line's
  first word would still fit in the text column ended at a hard break
  (Shift+Enter, one entry per line); other lines wrap.
- In the paragraph being edited, a click places the caret, dragging
  selects across its lines, a double-click selects a word, a triple-click
  the line and a quadruple-click the paragraph. Shift-click extends the
  selection.
- Input methods (Chinese, Japanese, dead keys) compose in place with an
  underline, and the committed text replaces it.
- Typing, Backspace/Delete (with Option/Ctrl for words), arrows, Up/Down
  between lines, Home/End (the line), Cut/Copy/Paste (line breaks kept) and
  Select All work in the paragraph. While editing, a return mark (↵) follows
  each line that ends at a hard break, and a fainter wrap mark (⤵) each line
  whose text continues on the next.
  Enter starts a new line; in a list it
  starts the next item with the same bullet or the next number, and in an
  empty item it removes the item's marker, ending the list there. Numbers
  count on from the first item whenever items are added or removed. Undo and Redo
  step through the editing session while it is open.
- The page shows the edited text as it will be saved: PDFium re-renders the
  page after each change while the previous tiles stay on screen. Text
  rewraps at the paragraph's width (left-aligned text out to the text
  column's edge, short of other text on the same baseline) from the changed
  line on, keeping its
  alignment (justified lines stay justified); lines before the change, and
  lines the text wraps back onto, keep their bytes. A single line (a
  heading, a label) grows instead of wrapping, as in Revu, until it would
  pass the page's edge (keeping the margin it has on its other side, up to
  an em) or come within an em of other text on its baseline; there it
  wraps. Text kept more than an em from text beside it still reads as its
  own line once saved. A list item's
  wrapped lines start at the items' text, and its text after the marker
  starts there too. A paragraph over two columns wraps through the first
  column's lines into the next column's.
- Other text never moves. A paragraph that grows adds lines below its last
  at its line spacing; when they overlap other text the frame turns to the
  warning colour. Text made larger than the paragraph's own, or a chosen
  line spacing, places every line afresh: the first keeps its baseline, each
  next is the line spacing times its largest text below, and at the foot of
  a column the next line starts at the top of the next column.
- Typed text takes the style of the character before the caret, or of the
  first replaced character when typing over a selection. Characters the
  line's font cannot draw use, in order: an installed font of the same name,
  a matching bundled font (Arimo, Tinos or Roboto Mono), Noto Sans SC, then
  any installed font that has the character (in the line's weight and
  slant first). Only a character no available font draws is refused, with a
  status message.
- The Edit PDF properties show while page content is selected or edited
  and the properties are open (the properties button); they never open by
  themselves. Without a paragraph being edited they say to double-click
  text. While one is, they show its font
  (the document font, the bundled families or any installed family), size,
  bold, italic and colour, applied to the selection or, with none, the whole
  paragraph; and its alignment and line spacing (a multiple of the font
  size), applied to the paragraph. A note names any font drawing characters
  the chosen one lacks. Each change is a step of the session's undo; buttons,
  the font list and Enter in a field return typing to the text. The X closes
  them.
- Escape finishes editing and leaves the paragraph selected; a click
  elsewhere, turning Edit PDF off, changing tool or document, saving and
  closing all finish editing too. One finished paragraph edit is one undo
  step.
- Text inside Form XObjects (stamped, imported or flattened content) edits
  like page text.
- Text that cannot be edited (Type3 fonts, vertical text, CMaps other than
  Identity-H and Adobe's horizontal Unicode CMaps, invisible OCR text,
  glyphs without Unicode, right-to-left text, a form drawn more than once on
  the page, or a file whose permissions forbid changing content) is shown
  with a not-allowed
  cursor; a click explains why.
- In a digitally signed PDF the first press asks "Edit content in a signed
  PDF?" (saving invalidates the signatures); Cancel leaves the text alone
  and asks again next time, Edit PDF allows editing for that document.

### Tables

A ruled grid the page draws (stroked lines or thin filled rectangles, as
Word, LibreOffice and Chrome draw borders) whose text sits in its cells is a
table. A missing border merges cells; a line crossing a border, or text
beside text in one cell, means the grid is not a table of that text, and its
text edits as ordinary paragraphs. So does a grid with no text in any
cell: it is a drawing's. A rule that crosses only one other rule, and does
not reach well past it on both sides, is not a border: a drawing's strokes
touching a table do not change its grid.

- Each cell's lines group into paragraphs of their own, and a cell's text
  wraps at the cell's width (a single line wraps too, rather than growing).
  A row grows to hold its tallest cell's text, keeping the gap each cell
  keeps below its text; the rows, borders and fills below move down with
  it, and a cell's fill and vertical borders stretch. Typing grows the row
  as it goes, a line break included before its line has text; the
  finished edit and its table are one undo step. Removing lines shortens
  a row as adding lines lengthens it, keeping any room it had to spare.
- Content outside a table never moves with it: in Edit PDF, a table that
  grows into other text, or past the page's bottom edge, shows a label
  under its left edge ("Overlaps other text" or "Runs off the bottom of
  the page"; just inside the page's bottom when there is no room below),
  and each line of text it covers is framed in the warning colour, where
  it is drawn; that text is moved by hand, and the frames and label go
  once nothing is covered. An edited paragraph that does so shows the
  same while it is edited. The label takes no presses. The band round a
  covered paragraph still drags it where it lies inside the table (the
  move cursor shows there); its text inside stays the cell's. An empty added line is drawn as a space, as word
  processors write an empty paragraph.
- A press in a cell away from its text edits the cell's nearest paragraph.
  A cell with no text takes typing in its model cell's font, size and place; its text is kept
  with the table and drawn as new text. Inside a table, its cells take a
  press rather than text outside the table that it now covers.
- Rows and columns are never added, removed, moved or resized by hand,
  as in Acrobat and Revu: only text changes a table, and a row's height
  follows its text. Tab and Shift-Tab finish the cell and edit the next
  or previous one, its text selected; in the last cell (or the first,
  with Shift) they do nothing, and the cell stays edited.
- Backspace in an empty paragraph continues the paragraph above it in its
  cell (outside tables, the one above it) from its end.
- The Edit PDF properties show the edited cell's text and paragraph
  settings, as for any paragraph.
- Each page's part of a table is its own table: nothing joins a table's
  parts across a page break, and nothing moves between pages. A table
  whose text or drawing a Form XObject draws, or that has text Edit PDF
  cannot edit, keeps its rows as they are; its cells' text still edits.
- Undo and redo reach back past a save: a page the save rewrote around its
  tables shows, in earlier states, as the file had it before the save.

## Filling forms

A PDF's form fields are filled with the Select tool, as in Acrobat and Revu.

- With Select active, every fillable field on the visible pages is outlined
  in the selection blue, as Edit PDF outlines paragraphs; hovering
  a field fills it faintly, and the focused field is filled and outlined
  strongly. Read-only fields are not outlined.
- Markups sit above fields: a press on a markup or its handles selects it,
  and hover shows a field only where no markup is. Elsewhere a press on a
  field focuses and fills it, and clears any markup selection.
- A text field opens an editor in place, in the field's text size (a fitted
  size when it has none), on the paper. Multi-line fields wrap and take
  Enter; password fields are masked; a maximum length stops typing at it.
  Enter finishes a single-line field.
- Check boxes and radio buttons toggle on a click, or Space when focused. A
  radio group that cannot be turned off stays on its choice.
- A combo or list box opens its options below the field on a click, or
  Space or Enter when focused, with the current choices ticked; a pick sets
  the field. A list box allowing several choices toggles the picked option.
  An editable combo box also offers "Type a Value…".
- Tab and Shift+Tab commit the field and move through the fillable fields
  in the page's tab order (rows, columns or annotation order), page by page,
  scrolling the next into view. Escape, a click elsewhere, changing tool or
  document and saving commit and finish.
- Each committed value is one undo step and marks the document unsaved.
  The page shows the field as it will be saved: PDFium re-renders it with
  its new appearance. Characters no available font can draw are refused
  with a status message.
- A click on a signature field or a form button says it is not supported;
  on a read-only field, that it is read-only. A form described only in XFA
  has no fields to fill; a click on its page says so.
- In a digitally signed PDF the first fill asks "Fill in a signed PDF?";
  Fill In allows filling and editing content for that document, as the
  Edit PDF prompt does.

## Page scale

Page Scale (the rail's first Measure button, the page's context menu, the
status bar's scale, or a selected measurement's Change button) sets the
scale a page is drawn at. Length, Polylength and Area measure through it; a
page without one measures paper size (1 in = 1 in).

- The dialog names the page, its paper (`Page 2 of 16 · ARCH D, 36 × 24 in`)
  and its current scale.
- Scale is a searchable list of standard scales: metric (1:1 to 1:5000),
  architectural (12" = 1'-0" to 1/32" = 1'-0") and engineering (1" = 10' to
  1" = 200'), imperial first on Letter, ARCH and ANSI sheets. Then Custom
  ratio (`1 in on paper = 4 ft`), No scale, and once calibrated, Calibrated
  from your line. A line under it says what the scale means (`1 cm on paper
  is 1 m`).
- The scales the page's own text states come first, and as buttons:
  `1/4" = 1'-0"`, `1" = 20'`, and metric `1:100` only beside `SCALE` or a
  sheet size (`1:100 @ A1`), since a bare `1:8` is usually a gradient. One
  stated for another sheet is converted: `1:100 @ A1` on an A3 page is 1:200.
  A page without a scale starts at the one it states most; nothing applies
  until Apply.
- Calibrate hides the dialog to click both ends of a length you know. Ends
  snap as a Length's do, Shift keeps the line straight, and the line follows
  the pointer with its paper length (its length at the page's scale, when it
  has one). The first click may be on any page: the scale is then for that
  page. Esc or Back returns to the dialog as it was. The dialog asks for the
  line's real length, focused (prefilled with what it measures now, when the
  page has a scale); a length may be typed as drawings write it (`36'-6"`) or
  with its own unit (`3600 mm`). Within 3% of a standard scale, it offers
  that scale.
- Measure in sets the unit and precision measurements show; changing the
  unit keeps the scale, converting typed lengths.
- Apply to, in documents of more than one page: this page, all pages, or
  pages like `1-3, 5` (typed straight away).
- Apply is one undoable change, and remeasures the measurements on those
  pages. No scale removes the scale; measurements already made keep their
  values. An unchanged dialog changes nothing, keeping even what it cannot
  show (a different vertical scale, an unlisted precision).
- The status bar shows the current page's scale once it has one, or while a
  measuring tool is armed; clicking it opens Page Scale.

## Cursors

| Where | Cursor |
| --- | --- |
| Drawing tools, selection lasso or box, calibration | crosshair |
| Box edge or corner | Butter Paper's resize arrow at the handle's angle, including the box's rotation |
| Vertex or endpoint | crosshair |
| Rotation knob | Butter Paper's rotate arrow |
| Movable stroke, fill or content; either Rectangle move band, selected or not | open hand (Windows: move arrow) |
| In Edit PDF: text that can be edited, and while selecting it | I-beam |
| In Edit PDF: the band round a paragraph, a drawing or an image, and while dragging it | open hand (Windows: move arrow) |
| Page text Edit PDF cannot change | not allowed |
| A fillable text field with Select | I-beam |
| Page text with Select, where no markup or field is, and while selecting it | I-beam |
| A fillable check box, radio button, combo or list box with Select | pointing hand |
| Moving a markup | closed hand (Windows: move arrow) |
| Pan tool | open hand; closed while dragging |
| Elsewhere | arrow |

Keyboard shortcuts and typing keep the cursor visible; switching tools updates
the cursor even when the pointer is stationary.

Resize and rotate arrows are drawn by Butter Paper (`canvas_cursors.rs`) and
shown through the GPUI fork's cursor images on macOS and Windows; Linux shows
the nearest system cursor. Windows has no hand cursors, so moving uses Butter
Paper's four-headed move arrow there, as Windows applications do; the Pan tool
still shows the arrow on Windows. All other cursors are the system's, so they follow
the user's pointer size and colour settings.

A drag keeps the cursor it started with; on release the cursor reflects what
is under the pointer.

## Pages

Page Thumbnails arranges a document's pages. Each change is one undo step,
and a page takes its markups, edited text and tables, scale and rotation
with it.

- Click a thumbnail to select its page and show it; Cmd-click (Ctrl on
  Windows and Linux) adds or removes a page, and Shift-click selects the
  pages from the last one clicked. Page actions act on the selected pages,
  or the current page when none are.
- Right-click a thumbnail for Insert Blank Page Before or After, Duplicate,
  Cut, Copy, Paste After, Rotate Left or Right, and Delete; thumbnails have
  no buttons of their own. A right-click on an unselected page selects it
  first. Rotate turns the selected pages one after another. Delete is
  unavailable when every page is selected: a document keeps one page.
- Drag thumbnails to move them: a line shows where they will land, before
  the page under the pointer in its top half and after it in its bottom
  half, and the list scrolls near its top and bottom edges. Dragging a
  selected page moves every selected page.
- The + in the Page Thumbnails header adds a blank page after the current
  page or at the end of the document. A new page is the size of the page
  beside it.
- With the thumbnails focused (after clicking one), Cmd-C, Cmd-X, Cmd-V,
  Cmd-D and Delete copy, cut, paste after the selected page, duplicate and
  delete pages; a press on the page gives the keys back to markups. Cut
  pages move when pasted, once; copied pages paste as copies until the pages
  are rearranged. Pages paste only into the document they came from.
- A copy has copies of its page's markups under new names. Form fields stay
  on the original page only.
- After a change, the page shown is the one acted on: a moved, added,
  pasted or duplicated page, or the page after a deletion.

## Merging documents

- File ▸ Merge Documents… (or Merge Documents… in a tab's right-click
  menu) opens a dialog listing the open documents in tab order, each ticked,
  with its page count. Untick a document to leave it out; drag rows by their
  grip to set the order. A document that has never been saved, or is still
  opening, is listed but cannot be ticked.
- Add PDFs… adds files that are not open, as does dropping PDFs on the
  dialog; each joins the end of the list, ticked,
  and can be removed with the delete button shown on hover. A file that cannot be read, or is password
  protected, is listed but cannot be ticked.
- Merge needs two ticked documents. It reads each as saved: when a ticked
  open document has unsaved changes the button reads Save and Merge, and
  saves them first. The merge opens as a new untitled tab, saved with Save
  As; the originals do not change.

## Viewing

- Trackpads and other precise scroll devices always scroll with two fingers,
  including Single Page and CAD View. Pinch and Ctrl+trackpad scroll zoom about
  the pointer. Mouse-wheel dropdowns apply only to discrete mouse wheels;
  Ctrl+wheel reverses their Scroll/Zoom choice. CAD View always zooms with a
  discrete mouse wheel. Scrolling up zooms in; Reverse Scroll Zoom flips it.
- In Single Page, the mouse wheel (set to Scroll), the arrow keys and Page
  Up/Page Down scroll a page larger than the view until its edge is in
  view, then turn the page: down to the next page, shown from its top edge,
  up to the previous one, shown from its bottom edge, at the same zoom and
  horizontal position. A page that fits turns with each notch or key, and
  stays centred. Left and Right do the same across. Turning the wheel
  quickly flicks through the pages: each shows at once from its thumbnail,
  if drawn, and the view ends on the last page asked for, rendering only
  that one rather than a render per page. Keys never scroll a page out of
  view.
- Going to a page (a thumbnail, the page buttons, the wheel) moves there at
  once: the page shows its thumbnail drawn up to size, if the sidebar has
  drawn it, until its own render replaces it. What is on screen renders
  first: the current page's tiles, then its whole-page image, then
  thumbnails.
- The thumbnails sidebar draws the thumbnails of the rows it shows, then two
  rows above and below, one at a time, and never pages further off. While
  the list scrolls it draws none; it starts on the rows where the scroll
  stops, so a fling or a jump of the scroll bar never waits behind rows it
  passed. Thumbnails far from view are unloaded once they hold 96 MB.
  A page an edit changes is drawn again at once; its thumbnail once edits
  pause for 400 ms, so typing never waits behind it.
- A full-width bottom status bar explains the active tool and navigation
  shortcuts while a document is open. Empty windows omit it. Narrow windows
  show shorter guidance; full help remains in tooltips.
- *Native:* a pan (Pan tool, Space or middle button) goes past the pages into
  blank space until 48px of a page stays in view; zooming over blank space
  keeps the page where it is. Scrolling stops at the pages' margins, as a
  document scrolls; after a pan it may scroll back towards the pages but no
  further out.
- Zoom In, Zoom Out, Actual Size and the zoom menu zoom about the view centre.
- Switching Continuous and Single Page keeps the current page, and the point
  at the view centre when it is on that page.
- Fit Width centres the current page across the view and Fit Page centres the
  whole page; a window resize refits and reframes. A fit is resolved when it
  is chosen or the window resizes, not refitted to each page scrolled past
  (as in Revu, where a wider page runs past the view's sides).
- The current page is the one level with the view centre.
- Each tab keeps its own page, zoom, fit and scroll.
- Dropping PDFs opens each in a new tab. Dropping PNG or JPEG images on a
  document places each at once as an Image, centred where it is dropped on
  the page under the pointer (or the nearest shown page, kept on it), at
  its natural size up to the Image tool's limit; several step 24 points
  apart. The last is selected, the tool is Select, and each image is one
  undo step. With no document open, images are ignored, as are other
  files.
