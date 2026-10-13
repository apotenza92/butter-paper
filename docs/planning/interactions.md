# Canvas interactions

How the canvas behaves. Electron (`electron-final`) was the original
reference; entries marked *native* are deliberate departures from it.

## Tools

- Shortcuts: Select `V`, Pan `Space` (hold), Text `T`, Rectangle `R`, Ellipse
  `E`, Arc `Shift+C`, Line `L`, Arrow `A`, Dimension `Shift+L`, Length
  `Shift+Alt+L`, Polylength `Shift+Alt+Q`, Area `Shift+Alt+A`, Polyline
  `Shift+N`, Polygon `Shift+P`, Pen `P`, Highlight `H`, Cloud `C`, Cloud+
  `K`, Callout `Q`, Image `I`, Snapshot `G`, Edit Text `Shift+T`. Tool
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

- *Native:* a selected markup gets a blue dashed outline 6px outside it, over
  a white halo, so the markup itself stays visible. No resize or vertex
  handles are painted; the markup's own corners, edges and vertices are the
  grab targets. The rotation knob stays, with a dashed connector matching the outline.
  Selection outlines and rotation knobs remain visible while moving, rotating
  or resizing, including when snapping is enabled.
- *Native:* the outline is a box turned to the markup: along a line for
  Lines, Arrows, Lengths and Dimensions, at the rotation for boxes, else
  upright. A Length or Dimension's box holds everything it draws: the
  measured points, extension lines, arrows and caption. The band that moves
  a markup follows the same box.
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
- A markup is hit on its strokes, its fill and its text or image. The empty
  inside of an unfilled shape is not part of it: a press there reaches what
  is underneath, selected or not.
- Double-clicking a rotation knob only turns the markup back to 0°; it never
  opens properties, including when the rotation is already 0°.
- Click selects; Shift-click toggles. Dragging on empty canvas draws a
  selection box: left to right selects what is fully inside (blue), right to
  left what it touches (green, dashed). Shift adds, Alt removes.
- Double-clicking a text box edits it in place (*native*: no properties
  panel). Double-clicking other markups opens properties.
- Select All works on the current page. Locked markups cannot be moved,
  resized or deleted.

## Editing page text

Edit Text (the rail's Edit section) edits the PDF's own text a paragraph at a
time, as Acrobat does; it is a separate tool, not part of Select.

- Lines group into paragraphs: same font, size and line spacing, sharing a
  left, right or centre edge (or both edges, justified), in one column.
  A list (items beginning with the same bullet, or numbered `1.` / `1)`, at
  one edge, with their wrapped lines at the items' text) is one paragraph.
  A paragraph that runs to the foot of a column without ending a sentence
  and continues at the top of the next column of the same width is one
  paragraph over both. Hovering outlines the paragraph.
- A click places the caret, dragging selects across its lines, a
  double-click selects a word, a triple-click the line and a quadruple-click
  the paragraph. Shift-click extends the selection.
- Input methods (Chinese, Japanese, dead keys) compose in place with an
  underline, and the committed text replaces it.
- Typing, Backspace/Delete (with Option/Ctrl for words), arrows, Up/Down
  between lines, Home/End (the line), Cut/Copy/Paste (line breaks kept) and
  Select All work in the paragraph. Enter starts a new line; in a list it
  starts the next item with the same bullet or the next number, and in an
  empty item it removes the item's marker, ending the list there. Numbers
  count on from the first item whenever items are added or removed. Undo and Redo
  step through the editing session while it is open.
- The page shows the edited text as it will be saved: PDFium re-renders the
  page after each change while the previous tiles stay on screen. Text
  rewraps at the paragraph's width from the changed line on, keeping its
  alignment (justified lines stay justified); lines before the change, and
  lines the text wraps back onto, keep their bytes. A single line (a
  heading, a label) grows instead of wrapping, as in Revu. A list item's
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
- While a paragraph is edited, the Edit Text properties show its font
  (the document font, the bundled families or any installed family), size,
  bold, italic and colour, applied to the selection or, with none, the whole
  paragraph; and its alignment and line spacing (a multiple of the font
  size), applied to the paragraph. A note names any font drawing characters
  the chosen one lacks. Each change is a step of the session's undo; buttons,
  the font list and Enter in a field return typing to the text. The X hides
  the properties until another paragraph is edited.
- Escape, a click elsewhere, changing tool or document, saving and closing
  all finish editing. One finished paragraph edit is one undo step.
- Text inside Form XObjects (stamped, imported or flattened content) edits
  like page text.
- Text that cannot be edited (Type3 fonts, vertical text, CMaps other than
  Identity-H and Adobe's horizontal Unicode CMaps, invisible OCR text,
  glyphs without Unicode, a form drawn more than once on the page, or a file
  whose permissions forbid changing content) is shown with a not-allowed
  cursor; a click explains why.
- In a digitally signed PDF the first press asks "Edit text in a signed
  PDF?" (saving invalidates the signatures); Cancel leaves the text alone
  and asks again next time, Edit Text allows editing for that document.

## Cursors

| Where | Cursor |
| --- | --- |
| Drawing tools, selection box, calibration | crosshair |
| Box edge or corner | Butter Paper's resize arrow at the handle's angle, including the box's rotation |
| Vertex or endpoint | crosshair |
| Rotation knob | Butter Paper's rotate arrow |
| Movable stroke, fill or content; either Rectangle move band, selected or not | open hand |
| Editable page text with Edit Text, and while selecting it | I-beam |
| Page text Edit Text cannot change | not allowed |
| Moving a markup | closed hand |
| Pan tool | open hand; closed while dragging |
| Elsewhere | arrow |

Keyboard shortcuts and typing keep the cursor visible; switching tools updates
the cursor even when the pointer is stationary.

Resize and rotate arrows are drawn by Butter Paper (`canvas_cursors.rs`) and
shown through the GPUI fork's cursor images on macOS and Windows; Linux shows
the nearest system cursor. All other cursors are the system's, so they follow
the user's pointer size and colour settings.

A drag keeps the cursor it started with; on release the cursor reflects what
is under the pointer.

## Viewing

- Trackpads and other precise scroll devices always scroll with two fingers,
  including Single Page and CAD View. Pinch and Ctrl+trackpad scroll zoom about
  the pointer. Mouse-wheel dropdowns apply only to discrete mouse wheels;
  Ctrl+wheel reverses their Scroll/Zoom choice. CAD View always zooms with a
  discrete mouse wheel. Scrolling up zooms in; Reverse Scroll Zoom flips it.
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
