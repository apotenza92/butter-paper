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
  grab targets. The rotation knob stays, drawn as part of the outline.
- *Native:* a selected Rectangle resizes from anywhere along an edge (within
  4px) or a corner; other box shapes resize from corners and edge midpoints.
  Dragging inside a selected markup, or along the band around its outline,
  moves it.
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
| Box edge or corner | resize in that direction (follows rotation) |
| Vertex or endpoint | crosshair |
| Rotation knob | pointing hand |
| Inside or along the outline of a selected markup | open hand |
| Moving a markup | closed hand |
| Pan tool | open hand; closed while dragging |
| Elsewhere | arrow |

A drag keeps the cursor it started with; on release the cursor reflects what
is under the pointer.

## Viewing

- Scroll pans; Ctrl+wheel and trackpad pinch zoom about the pointer. Scrolling
  up zooms in; Reverse Scroll Zoom flips it.
- *Native:* pages pan past their edges into blank space until 48px of page
  stays in view; zooming over blank space keeps the page where it is.
- Each tab keeps its own page, zoom, fit and scroll.
- Dropping PDFs opens each in a new tab; non-PDF drops are ignored.
