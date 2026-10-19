# Canvas interactions

How the canvas behaves. Electron (`electron-final`) was the original
reference; entries marked *native* are deliberate departures from it.

## Tools

- Shortcuts: Select `V`, Pan `Space` (hold), Text `T`, Rectangle `R`, Ellipse
  `E`, Arc `Shift+C`, Line `L`, Arrow `A`, Dimension `Shift+L`, Length
  `Shift+Alt+L`, Polylength `Shift+Alt+Q`, Area `Shift+Alt+A`, Polyline
  `Shift+N`, Polygon `Shift+P`, Pen `P`, Highlight `H`, Cloud `C`, Cloud+
  `K`, Callout `Q`, Image `I`, Snapshot `G`, Edit PDF Content `Shift+T`. Tool
  shortcuts ignore Cmd/Ctrl and editable or menu targets, including the Edit
  Text editor.
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
  Rectangle. Aspect-locked Images resize only from their corners. An
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
- Click selects; Shift-click toggles. Dragging away from page text draws a
  selection box: left to right selects what is fully inside (blue), right to
  left what it touches (green, dashed). Shift adds, Alt removes.
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

The Select tool also selects the page's own text to copy it, as Acrobat's
does; nothing about the PDF changes.

- Markups and fillable fields take a press first. Elsewhere a press in
  text places the start of a selection and dragging extends it, in reading
  order, to the nearest place in the page's text, including across lines
  and paragraphs. A press away from text clears it and draws a selection
  box as before.
- Double-click selects a word, triple-click its paragraph (Edit PDF
  Content's paragraphs); Shift-click extends the selection.
- Copy puts the selected text on the clipboard, a line break between lines.
  Selecting text clears a markup selection, and selecting a markup clears
  the text selection; Escape and changing tool clear it too.
- The text is what Edit PDF Content reads, as the page shows it after edits;
  a selection stays on the page it began on.

## Editing page text

Edit PDF Content (the rail's Edit section) edits the PDF's own text a paragraph at a
time, as Acrobat does; it is a separate tool, not part of Select.

- Lines group into paragraphs: same font, size and line spacing, sharing a
  left, right or centre edge (or both edges, justified), in one column.
  A list (items beginning with the same bullet, or numbered `1.` / `1)`, at
  one edge, with their wrapped lines at the items' text) is one paragraph.
  A paragraph that runs to the foot of a column without ending a sentence
  and continues at the top of the next column of the same width is one
  paragraph over both. With Edit PDF Content active, every editable paragraph on
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
- A click places the caret, dragging selects across its lines, a
  double-click selects a word, a triple-click the line and a quadruple-click
  the paragraph. Shift-click extends the selection.
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
- The Edit PDF Content properties open as other tools' do (a double-click on the
  tool, or the properties button), never by editing text; without a
  paragraph being edited they say to click text. While one is, they show its font
  (the document font, the bundled families or any installed family), size,
  bold, italic and colour, applied to the selection or, with none, the whole
  paragraph; and its alignment and line spacing (a multiple of the font
  size), applied to the paragraph. A note names any font drawing characters
  the chosen one lacks. Each change is a step of the session's undo; buttons,
  the font list and Enter in a field return typing to the text. The X closes
  them.
- Escape, a click elsewhere, changing tool or document, saving and closing
  all finish editing. One finished paragraph edit is one undo step.
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
  and asks again next time, Edit PDF Content allows editing for that document.

## Filling forms

A PDF's form fields are filled with the Select tool, as in Acrobat and Revu.

- With Select active, every fillable field on the visible pages is outlined
  in the selection blue, as Edit PDF Content outlines paragraphs; hovering
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
  Edit PDF Content prompt does.

## Cursors

| Where | Cursor |
| --- | --- |
| Drawing tools, selection box, calibration | crosshair |
| Box edge or corner | Butter Paper's resize arrow at the handle's angle, including the box's rotation |
| Vertex or endpoint | crosshair |
| Rotation knob | Butter Paper's rotate arrow |
| Movable stroke, fill or content; either Rectangle move band, selected or not | open hand (Windows: move arrow) |
| Editable page text with Edit PDF Content, and while selecting it | I-beam |
| Page text Edit PDF Content cannot change | not allowed |
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
- Dropping PDFs opens each in a new tab; non-PDF drops are ignored.
