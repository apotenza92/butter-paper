# Canvas interactions

How the canvas behaves. Electron (`electron-final`) was the original
reference; entries marked *native* are deliberate departures from it.

## Tools

- Shortcuts: Select `V`, Pan `Space` (hold), Text `T`, Rectangle `R`, Ellipse
  `E`, Arc `Shift+C`, Line `L`, Arrow `A`, Dimension `Shift+L`, Length
  `Shift+Alt+L`, Polylength `Shift+Alt+Q`, Area `Shift+Alt+A`, Polyline
  `Shift+N`, Polygon `Shift+P`, Pen `P`, Highlight `H`, Cloud `C`, Cloud+
  `K`, Callout `Q`, Image `I`, Snapshot `G`. Tool shortcuts ignore Cmd/Ctrl
  and editable or menu targets.
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
| Cloud | Drag a rectangle, or click corners | Double-click or Enter |
| Arc | Three clicks: start, end, bulge | Third click |
| Text box | Click | Type in place; click away or Escape commits, empty text discards |
| Pen, Highlight | Drag | Release |
| Image | Click places the chosen image | |
| Snapshot | Drag a box, or click then click | Release or second click |

Shift constrains while drawing (lines and points horizontal or vertical from
the last point, ellipse to a circle) and applies mid-drag. Length, Polylength
and Area measure at 1 in = 1 in on a page without a scale.

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
- Other box shapes resize from corners and edge midpoints. Dragging along the
  band around a selected markup's outline, from outside it to 20px inside,
  moves it.
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

## Cursors

| Where | Cursor |
| --- | --- |
| Drawing tools, selection box, calibration | crosshair |
| Box edge or corner | Butter Paper's resize arrow at the handle's angle, including the box's rotation |
| Vertex or endpoint | crosshair |
| Rotation knob | Butter Paper's rotate arrow |
| Movable stroke, fill or content; either Rectangle move band, selected or not | open hand |
| Moving a markup | closed hand |
| Pan tool | open hand; closed while dragging |
| Elsewhere | arrow |

Resize and rotate arrows are drawn by Butter Paper (`canvas_cursors.rs`) and
shown through the GPUI fork's cursor images on macOS and Windows; Linux shows
the nearest system cursor. All other cursors are the system's, so they follow
the user's pointer size and colour settings.

A drag keeps the cursor it started with; on release the cursor reflects what
is under the pointer.

## Viewing

- Scroll pans; Ctrl+wheel and trackpad pinch zoom about the pointer. Scrolling
  up zooms in; Reverse Scroll Zoom flips it.
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
