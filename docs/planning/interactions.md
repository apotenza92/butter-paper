# Canvas interactions

How the canvas behaves. Electron (`electron-final`) was the original
reference; entries marked *native* are deliberate departures from it.

## Tools

- Shortcuts: Select `V`, Pan `Space` (hold), Text `T`, Rectangle `R`, Ellipse
  `E`, Arc `Shift+C`, Line `L`, Arrow `A`, Dimension `Shift+L`, Polyline
  `Shift+N`, Polygon `Shift+P` (with Measure off; with it on: Dimension `Shift+Alt+L`, Polyline `Shift+Alt+Q`, Polygon
  `Shift+Alt+A`), Pen `P`, Highlight `H`, Cloud `C`, Cloud+
  `K`, Callout `Q`, Image `I`, Snapshot `G`; Page Scale `Shift+Alt+S` opens
  the Scale panel for the current page (Revu has no default key for it).
  `Shift+T`, or Edit PDF in the page's right-click menu (ticked while on),
  turns Edit PDF on and off
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
| Rectangle, Ellipse, Line, Arrow, Redact, Cloud | Drag, or click then click | Release past 3px, or the second click; Escape cancels |
| Length, Dimension | Drag, or click then click | Release past 3px, or the second click; Dimension then edits its text on the page |
| Polyline, Polygon | Click per point | Double-click or Enter; a Polygon closes on a click at its start (within 10px). A Polygon with Cloud outline on is created as a Cloud |
| Polylength, Area | Click per point | Double-click or Enter; an Area closes on a click at its start (within 10px). Escape discards |
| Cloud+ | Drag, or click then click | Release past 3px, or the second click; then type its caption |
| Arc | Three clicks: start, end, bulge | Third click |
| Text box | Drag its box, or click then click | Type in place; the box grows to fit its text. Click away or Escape commits, empty text discards |
| Pen, Highlight | Drag | Release |
| Image (and signatures) | Drag its box, or click then click | Release or second click; it fills the box at its own proportions |
| Snapshot | Drag a box, or click then click | Release or second click copies the box; nothing is placed |

Every tool that draws a box or a line between two points takes either
gesture: a drag, or a click on each end. A click never places a markup on its
own. Clouds of any shape are drawn with the Polygon tool, by turning on Cloud
outline in its properties (Revu's cloud line style).

Drawn and moved markups snap to other markups' corners, midpoints and (with
Nearest on) edges, rectangles and ellipses included. A path being drawn
(Polyline, Polygon, Polylength, Area) also snaps to its own placed points,
the middles of its sides and (with Nearest on) its sides, so a Polyline
closes on its start. Its last point is no target, and the side ending there
is one only at its middle. A stroked line also
snaps beside another markup's stroke, by both widths: its outer edge touching
the other's on either side, or sitting flush inside the wider stroke, with
the corners and edge midpoints of those positions. Those count a few pixels
further away than the exact edge, so the exact edge wins until the two are
visibly apart on screen.

Angle snap (Snap settings, Angles; on at 15° by default) keeps the direction
of a line or path segment being drawn from its last point at a multiple of
the increment: 90°, 45°, 30°, 15° or a custom angle (0.1° to 90°). This
applies to Line, Arrow, Dimension, Length, a Callout's leader, Arc's chord,
each segment of Polyline, Polygon, Polylength and Area, and the Page Scale
calibration line. Shift inverts
the setting while it is held: with it on, Shift frees the direction; with
it off, Shift snaps it to the increment. Shift also keeps an ellipse round
and snaps an Arc's bulge to a quarter, half or three-quarter circle; these
do not follow the setting. Shift applies mid-drag, at the next pointer
move. A point snap wins over a held direction, as an object snap wins over
Ortho in CAD: with the pointer within the snap range of an end, midpoint,
centre or intersection (of markups or page content, as Snap settings
enables them, or one of the path's own placed points), the point ends
exactly on that target, off the increment if it must. So a path closes on
its start whatever the angle. With no point target in range, the direction
holds, and only targets on the held ray count: a point target on it, a side
or edge (Nearest) where it crosses the ray, and the page grid and the
construction grid only where they lie on or cross the ray. Grids, Nearest
and snap guides never turn the held direction. A point snap also comes
before the Snap dimensions increment. The Page Scale calibration line
follows the same rules. A typed angle is exact and is not snapped.

Dragging a placed end or point follows the same rules, with its direction
held from a neighbour that stays where it is. The neighbour of a Line,
Arrow, Dimension, Length or Arc end is the other end. For a point of a
Polyline, Polygon, Polylength or Area, or of a Callout's leader, it is the
previous point in path order. The first point of an open path has no previous
point, so it takes the next point; the first point of a closed path (Polygon,
Area) takes the last point. The Arc's bulge handle and Cloud+ are not held.
Shift, the setting, a typed angle, point snaps and the guide behave as for a
markup being drawn. The guide shows once the drag is past the 3px threshold.

While a direction is held at the increment, a thin dashed guide runs along
it from the anchor (the last point, or the centre of a turn) through and
past the pointer. It has the colour and fade of the other snap guides, and
shows at once with reduce motion on. A small label beside the anchor gives
the held angle in degrees anticlockwise from the right (`45°`); for a turn
about a centre it gives the new rotation. It shows for Line, Arrow,
Dimension, Length, a Callout's leader, Arc's chord, each segment of
Polyline, Polygon, Polylength and Area, the Page Scale calibration line, and
every rotation knob turn (markups and Edit PDF content), and for a dragged end
or point as above. It hides while
Shift frees the direction, while angle snap is off and Shift is not held,
while a typed angle sets the direction, and while a point snap wins (its
indicator shows instead).

Snap dimensions (Snap settings, Dimension increments; on at 1 mm by
default) rounds the length of a plain Dimension being drawn to a whole
multiple of the increment, in millimetres on the paper, whether or not the
page has a scale. It does not apply to a measured Length, lines or paths. It
keeps the direction, so a held angle stays held. A point snap comes before
it, and a typed length is exact.

On a page without
a scale, the first click of a Polyline or Polygon with Measure on, or of a
Dimension chosen measuring (`Shift+Alt+L`, or its switch turned on), opens
Page Scale for that page instead, saying why; once applied, the next click
measures. Cancel measures at 1 in = 1 in, and that page does not ask again
while the document is open.

## Measure

Dimension, Polyline and Polygon have a Measure switch, at the top of the
tool's properties and in a selected one's. On, they draw Revu's
measurements: a Dimension a Length, a Polyline a Polylength (its total
length), a Polygon an Area. The rail button and the properties title keep
the tool's own name. Every drawing tool a page scale enhances (Rectangle,
Ellipse, Line, Polyline, Polygon, Dimension) has a tooltip with the
tool's name and shortcut on the first line. The muted second line says what
the tool measures on a page with a scale, after a bold `Page Scale on:`
(the toolbar's Page Scale toggle): for example `Page Scale on: Area
measurement` for Polygon. The tooltip does not name the measuring shortcuts (`Shift+Alt+L`,
`Shift+Alt+Q`, `Shift+Alt+A`), which stay for Revu users.

- Dimension's starts on: from the rail, each press draws a Length where its
  page has a scale and a Dimension with typed text where it has none.
  `Shift+Alt+L` (as in Revu) or turning its switch on measures everywhere,
  asking for a scale first where there is none; `Shift+L` draws typed text.
  Polyline's and Polygon's start off. The switches are the app's, kept while it is open;
  a rail button chooses its tool as its switch was last set, and a
  shortcut sets it.
- Turning Measure on in a tool's properties for a page without a scale drops
  down the Scale panel to choose one, the toolbar's Page Scale showing on
  until it closes.
- A selected markup's switch turns it into the other kind, as one undoable
  change for every such markup selected; locked ones are left. Each keeps
  its id, place in the stacking order, points and appearance. Measured, it
  measures at its page's scale (paper size without one, and the Scale panel
  opens); a Length turned back into a Dimension keeps its line and its
  caption as text. Saved, it is written as the other kind under its own
  `/NM`.
- Adding or changing a page's scale never turns plain markups into
  measurements; it remeasures the measurements already there.

On a page with a scale, once a markup's first point is placed a box beside
the pointer shows its size in the page's real unit and the page's scale:
Length and Angle for Line, Dimension (measured or not) and each segment of
Polyline, Polygon, Polylength and Area; Width and Height for Rectangle and
Ellipse (across and up the screen, on a turned page too). Typing a number
fixes the size in the highlighted field, AutoCAD-style; what is not typed
still follows the pointer, a box on the side of its first corner the pointer
is on. A bare number is in the page's unit (`3.2`), or names its own
(`3200mm`, `36'-6"`); an angle is in degrees anticlockwise from the right.
Tab and Shift+Tab move between the two fields, Enter places the point as a
click there would (Enter with nothing typed still finishes a path),
Backspace edits, and Escape clears what was typed before it cancels. Once a
number is typed, letters type units rather than switch tools. A typed size
overrides snapping. A page without a scale shows no box and takes no typed
size. An Arrow points at something rather than measuring it: it shows no
box and takes no typed size on any page, while angle snap and object snap
still apply to it.

Creation previews follow every pointer move during drags and between clicks,
without committing history. Cloud drags preview the rectangular scalloped
outline that will be committed; polygon drafts preview their next point
(as a cloud when Polygon's Cloud outline is on). A Text Box being drawn shows
its box as a dashed outline, and an Image fills the box being drawn.
Arc shows its start-to-end chord before the second click, then its live bulge.
The bottom contextual help explains each tool's click and drag gestures; its
tooltip expands the instructions when the window truncates them.

## Selection and editing

- *Native:* selection never draws a line over or around a markup, so its
  own colour, thickness and style stay visible. A selected or hovered markup
  shows small square handles at the points that can be dragged: line and
  leader ends and knees, vertices, arc controls, box corners and edges (an
  aspect-locked Image's or Snapshot's corners only), turned with the markup. Handles are
  white with a blue border (the same for the focused markup), light blue
  while hovered or a marquee candidate, filled under the pointer, and inert
  grey on a locked markup. The rotation knob stays, on a thin solid stem. Handles and
  knobs remain visible while moving, rotating or resizing, including when
  snapping is enabled; creation previews show no handles. Handles and knobs
  draw above every markup on the page, so a markup drawn later never hides
  the selection's. Each handle is centred on the line it moves: a box's
  line is centred on its box edge (as the PDF draws it), and a Callout's or
  Cloud+'s caption border runs half its width inside the box, so its
  handles sit there and dragging one moves the box edge with no jump.
  Upright box lines and handles are drawn an odd number of device pixels
  wide on the same pixel centre, so the line runs through each handle's
  middle at any zoom.
- Cut, Copy and Paste work on selected markups from the keyboard, the Edit
  menu and the page's right-click menu (which first selects an unselected
  markup under the pointer). The clipboard is shared by every window, so
  markups paste into any open document. They land centred on the pointer
  when it is over a page (the page menu's Paste: where it was opened);
  otherwise, in another document they land where they were, in the same
  one each paste steps 12 points right and down, and a cut pastes back
  where it was. Pasted markups that would fall off the page (copied from a
  larger page) move onto it together, as little as they must.
- As in Revu, a Snapshot box is copied, not placed, and the tool returns to
  Select. The copy is the page's content in the box with the markups on it
  (all but redactions) drawn in, as vector where the page's content fits in
  16 MB and as a picture otherwise; it pastes as one Snapshot markup, as a
  copied markup does. Other apps get a 300 dpi PNG of it on white (and a
  bitmap on Windows) and, when it is vector, a PDF beside it on every
  platform, which apps that read PDF paste as vector. Copying a selected
  Snapshot gives other apps the same.
- Paste takes a PDF copied since Butter Paper last copied, by another app or
  another Butter Paper (on another machine, through a shared clipboard), as
  a vector Snapshot of its first page, centred on the pointer (or on the
  page). A picture copied since (a screenshot, a browser's Copy Image)
  pastes as an Image placed as a dropped one is, centred on the pointer (or
  on the page): PNG or JPEG as it is, and a TIFF (as macOS apps copy), a
  bitmap (as Windows copies), GIF or WebP converted to PNG. Otherwise it
  pastes Butter Paper's own copy.
- *Native:* the band that moves a markup is a box turned to it: along a
  line for Lines, Arrows, Lengths and Dimensions, at the rotation for boxes,
  else upright. A Length or Dimension's box holds everything it draws: the
  measured points, extension lines, arrows and caption.
- *Native:* a Length or Dimension end is dragged from anywhere along its
  extension line, out to the arrow; the measured point follows the pointer
  from where it was grabbed, without jumping to it.
- A Length's, Dimension's, Polylength's or Area's caption is dragged on its
  own, selected or not (Revu: Shift+drag); dragging the rest of the markup
  moves it all, caption included. A moved Length or Dimension caption is
  drawn upright, as Revu draws it, over an unbroken line, with a leader to
  the line's middle; a moved Polylength or Area caption keeps a thin leader
  to its usual place (*native*: Revu draws none). A selected measurement's
  Properties offer Reset caption position while its caption is moved.
  One undoable change.
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
  Rectangle. Aspect-locked Images and Snapshots resize only from their
  corners, keeping their shape. New Images are aspect-locked unless
  Settings > Markups > Images turns "Keep proportions when resizing" off;
  Lock proportions in an Image's properties changes one image, and locking
  a stretched image returns it to its pixels' shape, keeping its width and
  centre (one undo step). Snapshots are aspect-locked when taken, pasted or
  opened; Lock proportions in a Snapshot's properties frees one to stretch
  from its edges, and locking it again keeps the shape it has (one undo
  step). Aspect lock has no PDF field, so a freed Snapshot is locked again
  when its file reopens. An
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
- Dragging a markup's rotation knob (Rectangle, Ellipse, Text Box, Image,
  Snapshot) turns it to whole multiples of the angle snap increment while
  angle snap is on; Shift frees the turn. With angle snap off, the turn is
  free and Shift snaps it to the increment.
- Double-clicking a rotation knob only turns the markup back to 0°; it never
  opens properties, including when the rotation is already 0°.
- A click-out only dismisses (decided 2026-11-23, extended 2026-11-25). A
  press that dismisses something does nothing else: no selection box or
  lasso, no new markup, no tool action and no text caret, for its drag and
  release too. A press acts on the page only when nothing is open over it,
  no text is being typed anywhere in the window, the window was already
  active (macOS), and, with Select, nothing is selected that the press
  dismisses. Otherwise it dismisses an open popup, menu, panel or dialog;
  finishes text being typed (a Text Box, a markup's caption, Edit PDF text,
  a form field, or a field outside the page such as the page number or a
  property, each as a click elsewhere finishes it); clears highlighted page
  text; or clears a markup or Edit PDF selection. This applies to every
  tool, Pan included. A click away from a property field keeps the markup
  selected; the next press deselects it. Focus on a button, tab or
  thumbnail holds no typed text: the next press on the page acts. A double
  or triple click that begins with such a press counts from the click after
  it. While a click would only dismiss, the pointer over empty page is the
  arrow, not the crosshair, nothing under it offers another action, and the
  status bar says "Click away to deselect" (or "Click away to finish
  typing" while a field is typed in). A press that changes a selection
  (another markup, other page text, Shift or Alt with a press, Ctrl or Cmd
  for a box) is not a click-out. A press while drawing a path places its
  next point and dismisses nothing, unless text is being typed outside the
  page: then it only finishes the typing, and the next press places the
  point. The middle button, the wheel and Space pan stay navigation: they
  finish nothing.
- Click selects; Shift-click toggles. Away from page text, a drag draws a
  freehand selection lasso and a click starts a selection box that a
  second click finishes (Escape cancels it). Either one begun rightwards
  selects what is fully inside (blue), leftwards what it touches (green,
  dashed). Shift adds, Alt removes. With something selected, a press away
  from markups only ends the selection, as in Revu: on empty page, page
  text, a form field or a link it starts no box, lasso, text selection or
  fill, and its drag and release do nothing; the next press does. A press
  on another markup selects it instead, and Shift-click toggles. Holding Ctrl (Cmd on
  macOS) makes a press draw a selection box wherever it lands, over a
  markup, handle, text, drawing or image as well as empty page, and in
  Edit PDF it selects page content too: dragged it is the box, clicked it
  starts one a second click finishes, by the same rightwards and leftwards
  rule; Shift or Alt with it adds or removes. The pointer is a crosshair
  and nothing under it is highlighted while it is held.
- The cursor is what a press does. Over a selected markup's text (a Text
  Box, the caption of a Callout, Cloud+ or Dimension) it is an I-beam, and
  a press there edits that text on the page with the caret where it landed;
  dragging selects text. Handles, resize bands and the rest of the markup
  keep their resize, rotate and move cursors and drags, so an unselected
  markup is selected by the first click and its text edited by the next.
  A selected Dimension's caption is moved from its edge, up to 6px outside
  its text. Its offset handle sits centred just below the caption's text,
  in the text's own turned frame, 12px clear of it at any zoom (just above
  instead where below would leave the page), or at the line's middle when
  the caption is empty or moved off, so no handle covers the text. A turned Text Box and a Dimension caption are edited in an
  upright box over where the text sits.
- A Callout or Cloud+ caption box fits its text and stays against its
  leader; a new Callout's box sits beyond the drag end, away from the arrow
  tip.
- Escape or a click away finishes the text and keeps it, for every markup
  with text; an empty new Text Box is discarded.
- Double-clicking a markup opens its properties. While text is being edited,
  a double-click in it selects a word, as in any text field.
- Every markup with text (Text Box, Callout, Cloud+, Dimension) shows it in
  a Text field in its properties, kept in step with the page. The field
  commits when it is left or on Cmd/Ctrl+Enter, as one undoable change. A
  Callout's and Cloud+'s properties also set its line, font, opacity and
  (Cloud+) fill and cloud intensity; a new style refits the caption box.
- Select All works on the current page. Locked markups cannot be moved,
  resized or deleted.

## Selecting page text

With Edit PDF off, the Select tool also selects the page's own text to
copy it, as Acrobat's does; nothing about the PDF changes.

- Markups, fillable fields and links take a press first. Elsewhere a press in
  text places the start of a selection and dragging extends it, in reading
  order, to the nearest place in the page's text, including across lines
  and paragraphs. With text highlighted, a press away from it (on empty
  page, a markup, a form field or a link) only clears it: it starts no
  selection box or lasso, selects nothing, and its drag and release do
  nothing. The next press acts as usual. A press in other page text starts
  a new selection; Shift-click extends this one.
- Double-click selects a word, triple-click its paragraph (Edit PDF's
  paragraphs); Shift-click extends the selection.
- Copy puts the selected text on the clipboard, a line break between lines.
  Selecting text clears a markup selection, and selecting a markup clears
  the text selection; Escape and changing tool clear it too.
- The text is what Edit PDF reads, as the page shows it after edits;
  a selection stays on the page it began on.

## Searching

The find bar finds text in the open document. The search panel searches
every open document, in every window, with a query language.

- Cmd+F (Ctrl+F on Windows and Linux) opens the find bar of the active
  document and puts the focus in its field with the text selected. The same
  keys work with the focus in any other text field of the window. If page
  text is selected, its first line (256 characters at most) becomes the
  query. Each tab has its own bar, query, options and current match.
- The bar floats at the top right of the canvas, 8 px from the top and the
  right edge, over the page. Nothing on the page moves. From left to right:
  the field (search icon, placeholder "Find in document"), the count, Match
  case, Match whole word, Previous match, Next match and a top-right X
  ("Close find bar"). The bar is at most as wide as the canvas less 16 px:
  the count shortens first, then the field.
- The query is literal text. Runs of white space match any run of white
  space, and a line break between two lines. The search covers page text,
  as Copy reads it with edits included, and the text of markups (Text Box,
  Callout, Cloud+, measurement labels and captions, and comments of other
  apps). Match case and Match whole word (Alt+C and Alt+W, Cmd+Option+C and
  Cmd+Option+W on macOS) change the search.
- Two lines of page text join for a phrase when they are of one paragraph,
  or when they are adjacent (the gap between them is at most 0.6 of the
  taller line), share some width, and no table rule lies between them.
  Other lines do not join: a phrase never runs across a table's cells.
- The search starts when the query rests for 150 ms. It starts at the page in
  view, goes to the last page, then goes on from the first. A new query,
  option or edit ends the old search. After an edit the search runs again
  400 ms after the edits pause. At most 10,000 matches are kept.
- The count reads "3 of 17" when the search is done, "17+" while it reads
  on, "Searching…" before the first match, and "No results" when nothing
  matches. An empty field shows no count. Matches count in page order. On
  each page, text matches come first, then markups.
- The first match is the first one on the page in view or after it. Enter
  and Next go to the next match, Shift+Enter and Previous to the one
  before, and both go round at the ends. F3 and Shift+F3 (Cmd+G and
  Shift+Cmd+G on macOS) do the same when the focus is in the document.
- The current match scrolls into view with a 24 px margin and keeps the
  zoom. In Single Page view the view turns to its page first. A match in a
  markup is outlined (no fill). Finding a markup does not select it, so
  typing never edits it.
- Every match on the pages in view is yellow, not the blue of the text
  selection. The current match is stronger and has an outline. The yellows
  follow the dark pages setting, not the app theme, and the text under
  them stays readable (4.5:1) on both papers.
- Escape with the focus in the bar, and the X, close the bar, clear every
  highlight and give the focus back to the document. With the focus on the
  page, Escape first clears what else it clears there (a text or markup
  selection, a drawing in progress, a placement, a tool other than Select),
  and closes the bar only when nothing else is left. The view stays where
  it is. The field keeps its text, so Cmd+F again searches it.
- A press on the bar never starts a selection, a drawing or an edit on the
  page under it. Letters typed in the field never switch tools.
- Edit ▸ Find (Cmd+F, Ctrl+F) opens the find bar; Edit ▸ Find in Documents
  (Cmd+Shift+F, Ctrl+Shift+F) opens the search panel.

### Search panel

- The left rail has a Search button under Page Thumbnails (tooltip with
  the shortcut). The left sidebar shows Page Thumbnails or Search, one at a
  time, at one width, and both follow the same yield rule. A click on the
  open panel's button closes the sidebar; a click on the other button
  switches the panel. In the empty window the button is disabled.
- Cmd+Shift+F (Ctrl+Shift+F on Windows and Linux) opens the panel, also with
  the focus in another text field, puts the focus in the query field and
  selects its text. Selected page text becomes the query. While the sidebar
  yields to properties, the panel is open but hidden and the focus stays
  where it was; the panel shows when the window widens or properties close.
- From the top: the header "Search", the Text and Visual switch, the query
  field (search icon, clear button), the toggles Match case, Explain
  search, Collapse results, More context and the Sort menu, then the scope:
  All open documents (every tab in every window) or This document (the tab
  in view). The two modes share the scope.
- The query language: words (all must match, in any order), "a phrase",
  `OR`, `-` to exclude, `( )` to group, `/regex/`, `match-case:` and
  `ignore-case:`, and the filters `text:` (or `content:`), `markup:`,
  `author:`, `subject:`, `page:3`, `page:3-10`, `page:12-` and `file:`. All
  terms match in one unit: one page's text, or one markup with its replies.
  A query never fails to read: an unclosed quote or bracket closes at the
  end, an unknown filter is literal text, and an invalid regular expression
  matches nothing and says so in the explanation.
- The search starts 250 ms after the query rests, or at once on Enter, and
  runs again on a new option or scope. Results arrive page by page. Over
  the list: "42 results in 3 documents", "Searching…" or "No results".
  Explain search shows the query in plain words under the toggles.
- Results group by document (this window's tabs first, then the other
  windows'), then page, each with its count. A name that documents in two
  windows share gets "· window N"; long names shorten in the middle, and the
  tooltip gives the full path. Matches in one paragraph that fit in one
  snippet (at most 40 characters apart, 80 from the first to the last) are
  one row, with each match bold and yellow; the find bar still counts each
  match. A row shows one line; More context shows up to three lines. A markup shows
  "Subject · Author", its text and its replies ("↳ Author: text").
- The chevron on a document or page row (or a click on the row) collapses
  it. Collapse results shows only document and page rows. The Sort menu
  orders documents: Document order, Name A to Z, Name Z to A, Most results
  first, Markups newest first and Markups oldest first.
- A click on a result, or Enter or Space on it, brings its window and tab
  to the front, goes to the match and marks the document's matches on the
  page as the find bar does, the opened one stronger. A markup in the model
  is selected, as a click on it does; another app's comment is outlined
  only. The find bar does not open, and the panel keeps the focus.
- Down in the field moves the focus to the results. Up and Down move
  between rows, Left collapses a group (on a match, it goes to its page),
  and Right expands it. Escape in the field clears the query, then gives
  the focus back to the page; in the list it gives the focus back to the
  page. The panel stays open. Escape on the page clears the marks of an
  opened result once nothing else is left to clear.
- The list builds only the rows in view. A search keeps 10,000 results at
  most, and then the list ends with "First 10,000 results. Narrow the
  search to see more." Under the list, "3 pages have no searchable text."
  counts scans and pages whose text is drawn as lines, with the link "Try
  Visual search", which switches the panel to Visual.
- An edit to a searched document, in this window or another, or a tab
  opened or closed in a window in scope, runs the search again 400 ms after
  the edits pause.

### Visual search

- The Visual mode of the search panel finds more of a symbol by its look.
  From the top, under the Text and Visual switch and the scope: the picture
  card (the picture, where it came from and its size in points, Draw box
  and Paste picture, and a top-right X that clears it), Similarity (a
  slider from 70 % to 99 %, 80 % at first), the options "Turned 90°, 180°,
  270°", "Sizes 85 % to 118 %" (50 % to 200 % for a pasted picture) and
  "Include mirrored", all on at first, then the progress line, the summary
  ("23 matches on 9 pages in 2 documents") and the results.
- Draw box arms the Snapshot tool for the page in view and gives it the
  keys. The prompt over the page and the status bar say "Drag a box around
  what to find". The box leaves no markup and copies nothing; the tool goes
  back to Select. Escape, the prompt's X or another tool stops it. The
  picture is the box's content trimmed to its ink, cut from the page drawn
  at the scale that makes it about 32 px on its short side.
- Paste picture, or Cmd+V (Ctrl+V) with the focus in the panel, takes a
  picture from the clipboard, such as a screenshot. Its pixels count as
  screen pixels of the page in view at its zoom. With no picture, the panel
  says "Copy a picture first, such as a screenshot." A picture that is too
  small or has no ink says "Draw a larger box." or "That box has nothing in
  it to find."
- The search starts at once and runs again on a new picture, option or
  scope, or when a searched document's pages change. It draws each page
  with its content only (no markups), in its real colours whatever the
  dark pages setting, after everything the viewer asks for, and matches
  the pages on background threads. It searches the picture's own page
  first. "Searching page 3 of 20…" with a progress bar and Stop shows while
  it runs; Stop keeps what was found ("Stopped after 3 of 20 pages"). A
  document that closes leaves the search and its hits go.
- Results group by document, then page, with counts; each hit shows a small
  picture of the page there and its score, three to a row. With dark pages
  on, these pictures and the picture card show the page's dark colours
  (the search still matches the real colours). The box the
  picture came from is a hit marked "Source". The tooltip says how a hit
  lies ("91 % similar, turned 90°, mirrored, 106 % size"). The slider only
  hides or shows hits already found: no new search.
- A click on a hit, or Enter or Space on it, brings its window and tab to
  the front, scrolls it into view and outlines the document's shown hits,
  the opened one stronger, in the colours of the dark pages setting. Up and
  Down move through the documents, pages and hits; Left goes from a hit to
  its page and collapses a group; Right expands it.

## Editing page content

Edit PDF edits and moves the PDF's own content (its text, drawings and
images), as Acrobat's Edit Text & Images does. It is a mode, not a tool: a
toggle at the right of the viewer toolbar (or `Shift+T`) turns it on for the
document, off when a document opens. While it is on, the Select tool works
on the page's content wherever no markup or fillable field takes a press;
the other tools draw markups as usual, and turning it on arms Select.

For more than Edit PDF does, File > Convert to Word Document… opens a
dialog that converts the PDF into `Name.docx` beside it (in Downloads for
another app's copy, such as a mail attachment, or a PDF on a read-only
disk), on macOS, Windows and Linux alike, with no other app needed. The dialog names the Word
document and offers Convert (Save and Convert when there are unsaved
changes, which are saved first, by Save As for a document that needs a
file or a new place) beside Cancel. When `Name.docx` exists, it says so and offers Replace
or Keep Both (the new one as `Name 2.docx` and on) instead. The dialog then
stays up while it works: a progress bar in the Word document's row counts
the pages converted. Closing it (top-right X or Escape) stops the
conversion and saves nothing; a save already under way finishes, but the
conversion after it does not happen. Once it is saved, the row says how many pages
it has and the dialog offers Show in Finder (File Explorer, or the Linux
file manager's name) and Open in the system's app for Word documents, named
when the system says which (plain Open otherwise); Open is the default
button. If it could not convert (an unreadable or password-protected PDF,
or a `Name.docx` another app holds open), or the save before it failed,
the dialog says why and the unsaved changes stay. A PDF saved from a Word
document as `Name.docx.pdf` converts to `Name.docx`. When the system will
not let it save in the PDF's folder (macOS protects Downloads, Documents
and the Desktop; Windows' Controlled folder access; a read-only folder),
the dialog says so, naming the folder (“this PDF's folder” for one only
an app knows), and offers Save to Downloads and Choose where to save…, a
Save panel starting in Downloads; without a Downloads folder, Choose where
to save… starts in the PDF's folder. On macOS and Windows the message names
the switch that allows the folder (as for Save), and without Downloads a
button opens its settings page. Nothing is left in the folder. A signed copy saved where a Save panel
allows only that file is written in place the same way.

The converter writes what Edit PDF reads (see
[decisions.md](decisions.md#interface)): paragraphs in reading order, top to
bottom, with their font family, size, bold, italic, underline and colour,
alignment, line spacing (at least the page's pitch) and the space above
each from the gap above it on the page; tables as Word tables on their
column grid with merged cells kept, bordered when the page rules them;
images inline where they are drawn; and a section per page with its size
and margins. Wrapped text runs on to the text area's edge, as the fonts in
the word processor decide. A page flowing text cannot hold keeps its
layout instead: a sheet larger than A3 (a drawing set, a poster), a page
that draws more than rules and boxes (a plan, a map), or one with text at
an angle or over an image. Its drawings are an SVG picture and its images
pictures of their own (a JPEG as it is; turned, mirrored, cropped and faded
as the PDF draws them), stacked in the PDF's paint order; each line of text
is a borderless text box at its place, quarter turns written vertically,
with words the PDF sets apart kept apart at tab stops and each stretch
spaced to the PDF's width in the metric-compatible font (Arial, Times New
Roman or Courier New stand in for fonts readers are unlikely to have).
Text a later fill covers stays, hidden under the drawing as on the page.
Every page converts to one page of the same size.

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
- Content is framed, never tinted: one thin box, a few pixels off the
  content at any zoom, in markups' hover blue, and their selection blue
  while hovered, selected or edited. Paragraphs are framed whenever Edit PDF is on;
  drawings and images once the pointer reaches them. A paragraph's box
  holds its lines' full height and their whole text, spaces and tabs
  included, the same framed, selected or edited, and is where a click edits
  it. Where two boxes would overlap or nearly touch, their facing edges
  meet on one line halfway between them; boxes whose text itself overlaps,
  or one within another, keep their edges. While dragged or resized, the content itself follows
  the pointer every frame, inside its box, while the page is shown without
  it; once let go, the picture stays until the page shows the content
  there. A move soon shows the content drawn by itself, as the page will
  draw it, in its own dashes and transparency, on a clear ground, with
  nothing drawn over it; until that arrives (and for a resize or turn) it
  is cut from the page as shown, with text's and drawings' paper clear so
  what it passes over shows. A selection of more than 48 pieces waits for
  the drawn picture, its frame moving meanwhile. A drag snaps as
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
- A single selected drawing, image, form or label of lettering (with no
  text in a clip) also shows a markup's rotation knob above its frame:
  dragging it turns the content about its centre, following the pointer, to
  whole multiples of the angle snap increment while angle snap is on (Shift
  frees it; with angle snap off, Shift snaps it). Its frame and handles stay
  upright round what it covers; resizing turned content scales it in its
  own axes, so it never skews: a quarter-turned piece stretches along the
  page's axes, and one at another angle keeps its proportions. A turned
  path is written with its points turned (its line width kept), a turned
  image or form by its matrix. A pattern a moved path is painted with (a
  hatch, a shading), being fixed to the page, is copied and moved, resized
  or turned with it. Text is never turned. Markups on turned
  content move with it as their centres turn; they do not turn.
- Delete or Backspace deletes the selected content and the markups
  selected with it. Copy, Cut and Paste work on it too. On the page it was
  copied from, Paste adds copies 10 points right and down (further each
  time), selected; a copy is drawn in its original's colour and line
  width, and moves, resizes and deletes on its own. On any other page, of
  this PDF or any other open one, in any window, Paste writes the content
  into that page centred on the pointer (or on the page, each paste a step
  from the last), turning Edit PDF on there, and selects it: its lines,
  fills, images and text are page content there, each its own to select,
  move, resize and delete, in their own colours, dashes, transparency and
  fonts, as one undo step. On a page turned differently it is turned to
  look as it did. A PDF or picture copied by another app since takes
  Paste instead. Text with unsaved edits pastes elsewhere only once
  saved.
- Enter edits a selected paragraph from its end; Escape ends the edit,
  leaving the paragraph selected.
- What a small clip (under half the page, clipping at most 250 paths and
  images) clips, such as an SVG or a chart, is one drawing: it moves with
  its clip and the text inside it, and the band round that text moves the
  drawing (a press in the text edits it). Otherwise each painted path and
  each image or form is its own, as in a CAD sheet, whose viewports clip
  more: a path moved there stays clipped to its viewport. A stroked path of
  several pieces (CAD programs batch lines into one path) is a drawing per
  piece, each from its `m` or `re` to the next; a filled one stays whole,
  as its pieces fill together (a letter's counter, a hatch). A table's rules
  and fills, and a drawing covering most of the page (a background), are
  not picked: tables change through their rows' menu and column borders.
- A path is picked where it is drawn, as in a CAD program: on its stroke
  (within a few pixels beyond half its line width) or inside its fill, not
  anywhere in its box, so a press inside a room picks nothing rather than
  its walls. The nearest picked wins, as in a CAD program, so a border
  beats lettering drawn just inside it; of those about as near (within a
  pixel), the smallest. Copies of a path drawn exactly over it (CAD sheets
  often draw a line twice) are picked, moved and deleted with it. Images
  and forms are picked
  anywhere in their box. A drawing sheet's border and title block are not
  a table: a ruled grid one of whose cells covers a quarter of the page
  and holds a drawing or image is drawing.
- Lettering drawn as paths rather than text (AutoCAD's SHX fonts, text
  Bluebeam or Quartz outlined) is picked a label at a time: small paths
  of the same stroke within half a letter of each other are a word, and
  words a word's space apart along a line, across or up the page (a
  dimension's figures written up a sheet's side), are one label, framed,
  moved, resized and turned whole. A label has at least three pieces. A
  row of one repeated shape (a dotted line, a pattern) is not
  lettering, nor a group whose pieces are mostly flat or dots (a dashed
  line's dashes: letters have bowls, diagonals and curves, so at least a
  quarter of a label's pieces are a quarter of its letter size across both
  ways), nor a group spreading wider than 20 letters both ways (hatching);
  a line that would be is taken a word at a time. Alt-click (Option-click
  on macOS) picks the one stroke, letter or piece under the pointer
  instead, out of a selected label too, and passes through the selection's
  handles to the piece under them; Alt on empty page still starts a lasso
  that deselects.
- While content is dragged, turned or resized, a path follows the pointer
  drawn as the page draws it, crisp at any angle: its outline and fill
  rule (a ring keeps its hole), its line width, dashes, ends and corners
  (scaled as a resize scales them), and its fill and stroke in the PDF's
  own colours and opacity, a colour profile's read as its grey, RGB or
  CMYK. A spot colour or a blend, which has no plain colour, is drawn in
  the colour the page shows it in; a pattern or shading, and a paragraph
  or image, follow as their picture. Nothing it frames or crosses comes
  with it.
- A block (a Form XObject: a CAD block, a flattened markup, a placed
  drawing) is picked whole, as an image is. Explode block, in the page's
  right-click menu and the Edit menu while one block is selected (right-
  clicking one selects it), writes it into the page where it is shown (as
  moved, resized or turned), so its lines, fills, text and the blocks it
  draws are each their own, as AutoCAD's Explode breaks a block into its
  parts; one undo step. Only the copy exploded changes: the block's other
  placements stay blocks, and copies pasted of it go with it. Its parts
  are written unclipped unless it drew beyond its bounding box. A block
  painted as a transparency group (a highlighter's markup) blends as one,
  so it stays whole: it is not offered for Explode and is not opened as a
  sheet. The help line says what to do with a selected block.
- A page drawn by one form covering most of it (a sheet placed into
  another PDF, or flattened into one form) is opened by itself: its
  drawings are picked one by one, as if the page drew them, with no
  command. The page is written opened once it is edited.
- Handles fit the frame: the middle of a side shows only on a side at
  least 40 pixels long, and a frame smaller than 24 pixels both ways has
  none (a press on it moves what it frames); the rotation knob stays.

### Editing page text

Edit PDF edits text a paragraph at a time, as Acrobat does.

- Lines group into paragraphs as the PDF's structure tree groups them:
  the lines of one paragraph element (`P`, a heading, a caption, a list's
  items, a cell's text) are one paragraph, in reading order even across
  columns.
- Lines in no paragraph element (every line of an untagged PDF) join only
  where one wraps onto the next: upright, the same font and size, the same
  left edge, an even line pitch of 0.9–1.75 em, no rule drawn between
  them, and the line above full: the next line's first word, after a
  space, would not have fitted before the column's edge. That edge is a
  table cell's text edge, a box the page rules on all four sides (inset as
  far from its right rule as the text starts from its left), or where at
  least two lines starting at that edge end, within an em. Otherwise a line
  is a paragraph of its own (it grows as typed rather than wrapping onto
  the next line, unless ruled round, when it wraps within its box), as are
  list items and centred or right-aligned lines.
- Within a paragraph, its layout reads it: its alignment (a shared left,
  right or centre edge, or both edges, justified), its line spacing, a
  list's markers (the same bullet, or `1.` / `1)`, with wrapped lines at
  the items' text), and where it runs from the foot of one column to the
  top of the next.
- In Edit PDF, every editable paragraph on the visible pages is outlined
  in hover blue (not text outside the page's visible area), so what a click
  edits shows without hovering; the hovered and edited ones turn selection blue. A page's text is read once the view
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
  at its line spacing; when any of its lines, moved by a chosen line
  spacing or larger text too, covers other text it did not already cover,
  a warning label says so. Text made larger than the paragraph's own, or a chosen
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
  glyphs without Unicode, right-to-left text, or a form drawn more than once
  on the page) is shown with a not-allowed cursor; a click explains why.
- In a file whose permissions forbid changing its content (an owner
  restriction, which opens without a password, as banks set on
  statements), the first press on its content, or selecting it, asks "Edit
  a protected PDF?": Cancel leaves it alone and asks again next time, Edit
  anyway allows editing for that document. Markups and form fields never
  ask. The saved file keeps its encryption and permissions.
- In a digitally signed PDF the first press asks "Edit content in a signed
  PDF?" (saving invalidates the signatures); Cancel leaves the text alone
  and asks again next time, Edit PDF allows editing for that document.

### Tables

A table is what the document's structure tree declares (Tagged PDF): its
cells hold the text drawn in their marked content, so text running past a
border is still its cell's. Where no declared table covers it, a grid the
page's rules close on every side, with at least one inner rule and some
text, is a table too (an untagged report's): a cell spans the grid units no
rule divides, and holds the lines whose middle is inside it. A row or
column reaches as far as its cells' text; its
edges sit on the borders the page draws between rows and columns (else
midway), so a border stays its row's as the row grows. Rows at a page's
top or bottom whose text is drawn on another page are that page's.

- Each cell's lines group into paragraphs of their own, and a cell's text
  wraps at the cell's width (a single line wraps too, rather than growing).
  A row grows to hold its tallest cell's text, keeping the gap each cell
  keeps below its text; the rows, borders and fills below move down with
  it, and a cell's fill and vertical borders stretch. Typing grows the row
  as it goes, a line break included before its line has text; the
  finished edit and its table are one undo step. Removing lines shortens
  a row as adding lines lengthens it, keeping any room it had to spare.
- Content outside a table never moves with it: in Edit PDF, a table that
  grows into other text, or past the page's bottom edge, shows a label:
  "Overlaps other text" under the left edge of the text it covers, or
  "Runs off the bottom of the page" under its own left edge (just inside
  the page's bottom when there is no room below). Each line of text it
  covers is framed in the warning colour, where it is drawn; text the page
  already drew inside the table is not covered, whether or not a cell's
  tags hold it; that text is moved by hand, and the frames and label go
  once nothing is covered. An edited paragraph that does so shows the
  same while it is edited. The label takes no presses. The band round a
  covered paragraph still drags it where it lies inside the table (the
  move cursor shows there); its text inside stays the cell's. An empty added line is drawn as a space, as word
  processors write an empty paragraph.
- A press in a cell away from its text edits the cell's nearest paragraph.
  A cell with no text takes typing in its model cell's font, size and place; its text is kept
  with the table and drawn as new text. Inside a table, its cells take a
  press rather than text outside the table that it now covers. Text the
  page draws inside a table that no cell's tags hold is hit as itself.
- Right-clicking a cell in Edit PDF puts Insert Row Above, Insert Row
  Below and Delete Row at the top of the page menu. A new row is empty,
  drawn like the clicked row (its borders and fills) and a line of its
  text tall; Delete Row takes the row's text with it, and a table keeps
  its last row. Each is one undo step.
- A table's inner column border shows the column-resize cursor; dragging
  it widens one column as it narrows the other, keeping the table's width,
  and stops where a column's widest word with its cell's gaps no longer
  fits. On release the two columns' text rewraps at their new widths and
  rows grow to hold it, as one undo step. Markups and form fields over a
  border take the press first.
- A table that continues on another page, or has merged cells, keeps its
  rows (the menu items are disabled, with the reason); a continuing one
  keeps its columns too. Tab and Shift-Tab finish the cell and edit the
  next or previous one, its text selected; in the last cell (or the
  first, with Shift) they do nothing, and the cell stays edited.
- Backspace in an empty paragraph continues the paragraph above it in its
  cell (outside tables, the one above it) from its end.
- The Edit PDF properties show the edited cell's text and paragraph
  settings, as for any paragraph.
- Each page's part of a table is its own table: nothing joins a table's
  parts across a page break, and nothing moves between pages. A table
  whose text a Form XObject draws, that has text Edit PDF cannot edit, or
  a line running across cells, keeps its rows and columns as they are;
  its cells' text still edits.
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
  size when it has none) at the page's zoom, filling the field, on the paper. Multi-line fields wrap and take
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
  document and saving commit and finish. A click elsewhere on the page commits
  and finishes, and does nothing else; a click on another field moves to
  it.
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

## Following links

With Edit PDF off, the Select tool follows a PDF's link annotations, as
Acrobat and Preview do.

- Markups and form fields sit above links. Elsewhere a link shows the
  pointing hand, and a click on it (released within a few pixels of the
  press) follows it; a press that drags off does nothing and does not
  start a text selection. With markups or page text selected, a press on
  a link only clears them; the next click follows it.
- Web (`http`, `https`) and `mailto` addresses open with the system.
  Other addresses (files, programs, JavaScript) are not opened; a status
  message says so.
- A link to a page of the document shows that page, or a copy of it after
  rearranging; a link to a deleted page says so. A page copy shows its file
  page's links; a new blank page has none.
- Links are not drawn or outlined beyond what the page itself draws, and
  are not followed in Edit PDF mode or with other tools.

## Page scale

The view toolbar's Page Scale, beside Edit PDF, is the current page's
scale: one split button, as the page-view controls are. Its half with the
ruler turns the page's scale on and off and is selected while on; its caret
drops down the Page Scale panel. While the page has a scale, the scale shows
beside the ruler (`1:100`, cut short with an ellipsis when long; the tooltip
names it in full). Its accessible name says whether it is on, and the
scale. Measurements measure through it; a page without one measures paper
size (1 in = 1 in).

- The current page is the one the page navigator shows. The ruler and the
  caret act on it alone; the ruler's tooltip names it (`Page Scale on: page
  3 at 1:100. Click to remove page 3's scale.`).
- Off removes the page's scale, as one undoable change, and the ruler
  remembers it: on brings it back without asking, while the document is
  open. Measurements already made keep their values. The status bar says
  what changed (`Removed the scale from page 3. Turn Page Scale on to bring
  back 1:100.`).
- On, for a page never scaled, drops down the Page Scale panel, the ruler
  showing on until it closes; closed without Apply, it is off again.
- The caret, the page's context menu (Set page scale…, and Remove page scale
  when it has one), the status bar's scale, a selected measurement's Change
  button and a Measure switch drop down the same panel, under the toolbar's
  Scale; the caret shows pressed while it is open. The caret closes it
  again; so do its X, Cancel, Escape (after a list open in it) and a press
  outside it, which only closes it.
- The ruler and the caret always show, as icons. The scale beside the ruler
  goes in the two narrowest toolbar forms, after the zoom, fit and
  page-view buttons have stepped into menus; the tooltip and the status bar
  still name it.
- More than one page in view, the current page has a 2-point outline just
  outside its edge, in the theme's text colour (35% in light, 60% in dark),
  not the paper's. Over the toolbar's Scale, or with its panel open, the
  outline darkens, and with the panel open it marks every page Apply would
  change (`1-8` as it is typed).

- The panel names the page, its paper (`Page 2 of 16 · ARCH D, 36 × 24 in`)
  and its current scale.
- Scale is two lists. The first chooses the group: Metric, Architectural,
  Engineering, Custom, No scale, and once calibrated, Calibrated. A standard
  group's scales are a searchable list under it: metric (1:1 to 1:5000),
  architectural (12" = 1'-0" to 1/32" = 1'-0") or engineering (1" = 10' to
  1" = 200'). A page with a scale opens on that scale's group; one without
  opens on Metric, or Architectural on Letter, ARCH and ANSI sheets. A group
  chosen without a scale of it chooses nothing until one is chosen (a scale
  the page states in that group, if any). Custom shows the ratio in place of
  the list (`1 mm on paper = 0.1 m`), in millimetres on paper (inches when
  measuring in feet or inches); a custom scale keeps the paper unit it is
  stored with. A line under it says what the scale means (`1 cm on paper is
  1 m`, or in the custom ratio's own paper unit).
- The scales the page's own text states come first, and as buttons:
  `1/4" = 1'-0"`, `1" = 20'`, and metric `1:100` only beside `SCALE` or a
  sheet size (`1:100 @ A1`), since a bare `1:8` is usually a gradient. One
  stated for another sheet is converted: `1:100 @ A1` on an A3 page is 1:200.
  A page without a scale starts at the one it states most; nothing applies
  until Apply.
- Calibrate hides the panel to click both ends of a length you know. Ends
  snap as a Length's do, the line's direction follows angle snap as a Line's
  does (Shift inverts the setting while it is held, and the hint on the page
  says what Shift does now), and the line follows
  the pointer with its paper length (its length at the page's scale, when it
  has one). The first click may be on any page: the scale is then for that
  page. A banner over the page is the one place that says what to click
  next; a click for the second end on another page changes it to say where
  that end goes ("Click the other end on page 1, where the line starts."),
  and raises no toast. The status bar names the mode and its key alone
  (`Calibrate · Esc to go back to Page Scale`). Esc or Back returns to the
  panel as it was. The panel asks for the
  line's real length, focused (prefilled with what it measures now, when the
  page has a scale); a length may be typed as drawings write it (`36'-6"`) or
  with its own unit (`3600 mm`). Within 3% of a standard scale, it offers
  that scale.
- Show measurements in sets the unit and precision measurements show:
  metres to 0.01 m on metric plans (1:50 and coarser, so areas read in
  square metres), millimetres to 0.1 mm on metric details and on a page that
  suggests no scale, decimal feet on imperial drawings (Revu starts with
  none chosen); changing the
  unit keeps the scale, converting typed lengths. Applying a scale moves
  measurements shown in the page's old unit to the new one; one shown in a
  unit of its own keeps it, as in Revu 20 and later.
- A selected measurement's Unit (mm, cm, m, km, in, ft, yd, mi) changes the
  unit that measurement shows, as one undoable change; what it measures and
  the page's scale stay the same. A unit Butter Paper cannot convert, from
  another app, is shown and kept.
- Apply to, in documents of more than one page: this page, all pages, or
  pages like `1-3, 5` (typed straight away).
- Apply is one undoable change, and remeasures the measurements on those
  pages. No scale removes the scale, as the switch does; measurements
  already made keep their values. An unchanged panel changes nothing, keeping even what it cannot
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
| Page text with Select, where no markup, field or link is, and while selecting it | I-beam |
| A fillable check box, radio button, combo or list box with Select | pointing hand |
| A link with Select, where no markup or field is | pointing hand |
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
  pages from the last one clicked. With the thumbnails focused, Cmd+A
  (Ctrl+A) selects every page, leaving the page in view, and Escape clears
  the selection (with none, Escape acts as it does on the page). Select
  All elsewhere selects the page's markups, as before. Page actions act on
  the selected pages, or the current page when none are.
- The header's Rotate All Pages Left and Right, beside its +, turn every
  page of the document whatever is selected, as one undoable change; so do
  Shift+click on the rail's Rotate Left and Rotate Right (a plain click
  turns the current page) and the Document menu's items of the same names.
  The page in view and every thumbnail turn at once, and the view stays on
  the page it shows, in Continuous view at a free zoom too; so does its
  Undo or Redo.
- Right-click a thumbnail for Insert Blank Page Before or After, Duplicate,
  Cut, Copy, Paste After, Rotate Left or Right, and Delete; thumbnails have
  no buttons of their own. A right-click on an unselected page selects it
  first. Rotate turns the selected pages one after another. A page turns in
  the first frame, from its own tiles turned, and the other pages keep
  theirs; rotating the page in view does not move the view at a free zoom.
  With Fit Width or Fit Page in force, a rotation that turns the page in
  view (one page, selected pages, every page, or Undo or Redo of a
  rotation) fits the turned page again; a turned thumbnail's page is shown
  and fitted. Delete is
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
  on the original page only, and are outlined, filled and reached with Tab
  wherever that page is moved; a deleted page's fields cannot be filled.
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

## Tabs and windows

- Each document window holds its documents as tabs in its own tab strip.
  macOS native window tabs are off: Butter Paper draws no window tab bar,
  so AppKit adds no tab commands to the Window menu.
- Merge All Windows (Window menu on macOS, View menu on Windows and Linux)
  moves the documents of every other window to the end of the active
  window's tabs, window by window in the order the windows opened, each
  window's tabs in order. Each document keeps its unsaved edits, view and
  undo history, and the active window keeps the tab it shows. A window left
  without tabs closes without a prompt. A document that cannot move yet
  (opening, saving, or waiting for a prompt) stays in its window, which
  stays open. The command is unavailable with one window.
- A PDF opened from outside the app (Finder or File Explorer, the Dock or
  taskbar, the app icon, the command line) goes where Settings ▸ Documents
  ▸ Open documents in says. New tab, the default: a new tab in the most
  recently active document window, which comes to the front; with no
  window, one opens. New window: a window of its own for each outside open
  (several PDFs opened at once share it). Under either choice a window
  with no documents takes the open, so a cold launch from a PDF opens one
  window, not two.
- On Windows and Linux the file manager starts a new process for each
  file. That launch hands its PDFs to the running Butter Paper of the same
  profile and exits; with none running, it becomes the app. A launch with
  no PDFs starts an app of its own, as before. macOS hands outside opens to
  the running app itself.
- A PDF dropped anywhere on a window (its tab strip, a page, the New Tab
  page, a window with no documents) opens as a new tab in that window,
  several in the order they were dropped, whatever the setting. A drop on
  Merge Documents adds the PDFs to its list instead, and an image dropped
  on a page is placed on it. File ▸ Open… and the New Tab page's Open open
  in their own window; New Window and dragging a tab out are unchanged.

## Viewing

- Trackpads and other precise scroll devices always scroll with two fingers,
  including Single Page and CAD View. In Single Page, a swipe that finds the
  page's edge already in view (always, for a page that fits) turns the page
  once it has travelled 40 points: one page a swipe, however long it or its
  momentum goes on, the next swipe turning the next. A swipe that scrolled the
  page to its edge stops there. Pinch and Ctrl+trackpad scroll zoom about
  the pointer. Mouse-wheel dropdowns apply only to discrete mouse wheels;
  Ctrl+wheel reverses their Scroll/Zoom choice. CAD View always zooms with a
  discrete mouse wheel. Scrolling up zooms in; Reverse Scroll Zoom flips it.
- In Single Page, the mouse wheel (set to Scroll), the arrow keys and Page
  Up/Page Down scroll a page larger than the view until its edge is in
  view, then turn the page: down to the next page, up to the previous one.
  A page that fits turns with each notch or key, and stays centred. Left
  and Right do the same across. Once the page shown is
  sharp, the pages either side of it are drawn ahead (and the page just
  left is kept), so a turn to one shows it at once as sharp as its own
  render, never its thumbnail scaled up. Turning the wheel quickly flicks
  through the pages: each shows at once from its thumbnail, if drawn, and
  the view ends on the last page asked for, rendering only that one rather
  than a render per page. A page with no picture yet (no thumbnail, not
  drawn ahead) does not show white: the page before it stays, if the same
  shape, until the whole of the new one can replace it. Pages drawn ahead
  are drawn at the size the page shown has on screen (up to 2,400 pixels
  wide), so a turn does not sharpen a moment later. Keys never scroll a
  page out of view.
- Single Page view remembers each page's zoom and scroll, however the page
  is left (wheel, keys, page navigator, thumbnails, a switch to Continuous
  view). A page returned to shows as it was left, panned past its edges
  too (with part of it in view). A page not seen before shows from its
  top edge going down and its bottom edge going back up, at the same zoom
  and horizontal position; with Fit Width or Fit Page in force it opens
  fitted to its own size. Choosing Fit Width or Fit Page forgets the page's
  view. Rotating a page, by any command or by its Undo or Redo, forgets
  that page's view; moved pages keep theirs. Continuous view keeps one zoom
  and scroll for the document, and switching views keeps the memory. The
  memory is saved with the session, for the last 100 pages seen. Closing a
  file's tab keeps its memory (the page left too), for the last 50 files in
  the app's data; the file's next tab opens on the page left, in Single Page
  view, with that page's view, unless a restored session view brings its
  own. A file left in Continuous view opens as usual, with its pages
  remembered. Quitting keeps the memory of every open tab the same way.
- Settings ▸ Mouse wheel ▸ Page turning sets how the discrete mouse wheel
  turns pages in Single Page view. A notch is three lines on Linux, one on
  macOS, and the system's lines per notch on Windows (three by default), so
  one notch zooms one step and turns one page on each platform; smaller movements (a high-resolution or
  free-spinning wheel) turn nothing until they add up to a notch, and part
  of a notch is forgotten after 300 ms. A spin is a run of notches with no
  gap longer than the choice's spin gap. No choice queues a turn: once the
  wheel stops, the pages stop.
  - Responsive (default): "Every click turns a page." Each notch turns a
    page, except notches less than 40 ms after a turn. Zoomed in, the
    wheel scrolls the page and one notch past its edge turns it.
  - Balanced: "One page for each spin of the wheel. The wheel stops at each
    page edge." The first notch of a spin (180 ms gap) turns the page; the
    rest of the spin is ignored. Zoomed in, a spin that scrolls the page to
    its edge stops there, and a new spin turns it.
  - Deliberate: "Pages turn only after a clear pause. The wheel stops at
    each page edge." As Balanced, with a 450 ms gap between spins.
- Settings ▸ Mouse wheel ▸ Zoom speed sets how far one notch zooms: Fine
  ×1.10, Standard (default) ×1.20, Coarse ×1.35, part of a notch by part
  of the ratio, two notches an event at most. It applies to every
  discrete-wheel zoom (Ctrl+wheel, and the plain wheel where it zooms: CAD
  View, or a wheel set to Zoom), about the pointer. The zoom moves over
  200 ms, a notch during the motion going on from where the motion heads;
  with reduce motion set it is there at once. Trackpad pinch and Ctrl+swipe
  follow the fingers and ignore Zoom speed. The page groups Reverse scroll
  zoom and Zoom speed under the heading Zoom. Reverse scroll zoom is also in
  the View menu.
- Dark pages draw the pages dark. The View menu has Page Appearance with
  Light (default), Dark and Match System, and Settings ▸ View has the same
  three choices. The setting is for all documents and windows, and the app
  keeps it across launches. Match System follows the system appearance while
  the app runs. There is no toolbar button.
  - Ctrl+Shift+D (Cmd+Shift+D on macOS) sets Light or Dark, the opposite of
    the pages on screen. From Match System it also sets the opposite of the
    current look. It does not act while a text field has focus.
  - The setting changes the view only. Saves, exports, copies, previews and
    recovery files keep the real colours of the document.
  - Ink and paper go dark and keep their hue. Dark colours become lighter
    colours of the same hue. Light colours such as yellow stay. Pictures
    (photos, logos, scans placed as images, shadings and stamps) keep their
    real colours.
  - A page that is half pictures or more (a full-page scan) stays in real
    colours. The status bar then starts with "Pictures keep their real
    colours in Dark pages". The text does not block anything.
  - Markups draw through the same colour map. Their stored colours, colour
    pickers and the Properties panel show the real colours. A highlight
    draws as a dark highlight colour over the page, so text stays readable.
  - The canvas chrome follows the paper: selection, handles, guides, the
    marquee, the Edit PDF caret and the form field editor use lighter
    colours on dark paper.
  - A dark page has a one-pixel grey ring outside its edge, so it stands
    apart from the viewport. The ring never insets the page image.
  - Thumbnails follow the setting.
- The page navigator in the centre of the view toolbar shows the page in
  view of the total ("3 / 12") between First and Previous page, and Next
  and Last page, which are disabled at either end. Clicking the page number
  makes it a field with the number selected: a page typed and Enter goes
  there (a number past the end goes to the last page; anything else stays
  put), Escape or clicking elsewhere keeps the page shown, and focus
  returns to where it was. A click on the page only closes the field: it
  selects and draws nothing. The field is as wide as the page count's digits
  (two at least) with its padding, its number centred, and its focus ring
  inside the navigator's frame, so a number typed or selected is never
  cut; opening it moves nothing beside it. Letters typed there never
  switch tools. Previous and Next count from the page last asked for, so
  quick presses keep going while pages render.
- Undo and Redo, at the toolbar's left, do what the Edit menu's Undo and
  Redo do, and are enabled as they are.
- A press on anything drawn over the page (a menu of the in-window menu
  bar, a popup, a notification, a dialog, the find bar) acts only there or
  dismisses it; it never also selects, draws or edits on the page beneath.
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
  show shorter guidance; full help remains in tooltips. A status message
  (why text cannot be edited, a save or conversion's progress or result)
  replaces the tool's guidance, in the stronger text colour, for six
  seconds, and shows as a toast. Toasts sit at the bottom right of the
  canvas, 16px inside its edges: above the status bar, and left of the right
  rail and any panel open beside the canvas, never over them. Toasts are for
  results and errors that have no visible place of their own; guidance a
  mode's banner already gives (Calibrate) never shows as a toast too.
- *Native:* a pan (Pan tool, Space or middle button) goes past the pages into
  blank space until 48px of a page stays in view; zooming over blank space
  keeps the page where it is. Scrolling stops at the pages' edges, as a
  document scrolls; after a pan it may scroll back towards the pages but no
  further out.
- Zoom In, Zoom Out, Actual Size and the zoom menu zoom about the view centre.
  Zoom In and Zoom Out (buttons and shortcuts) step to the next whole 10%:
  136% goes to 140% or 130%.
  When these or the fit buttons change the zoom, each page shows
  its old render scaled to the new size until its new render is complete; a
  zoomed page skips the low-resolution preview pass, so it never turns
  blurry in between.
- Switching Continuous and Single Page keeps the current page, and the point
  at the view centre when it is on that page.
- The viewport meets the toolbar and rails with no frame, and the pages meet
  its edges: only the space between pages (24pt at 100%) is blank. The
  scroll bars float over the pages.
- Fit Width centres the current page across the view and Fit Page centres the
  whole page, each filling the view to a tenth of a percent; a window resize
  refits and reframes, staying on the pages. A fit is resolved when it
  is chosen or the window resizes, not refitted to each page scrolled past
  (as in Revu, where a wider page runs past the view's sides).
- The current page is the one level with the view centre. In Continuous
  view a click on a page makes it the current page (the page navigator,
  its thumbnail, Rotate and the Scale follow it) and moves nothing, even
  when the click only dismisses; the next scroll makes the page level with
  the view centre current again. Single Page view shows only the current
  page.
- Each tab keeps its own page, zoom, fit and scroll.
- The Signature panel's Image tab takes a signature from wherever it is:
  From the page (the panel steps aside and the Snapshot tool draws a box
  round a signature on any open page; the status bar says so until it is
  drawn or Esc or another tool cancels; the box leaves no markup and the
  panel comes back with it as a preview), Paste (a copied picture, such as
  a screenshot), Choose file (PNG, JPEG, or a PDF whose first page is the
  signature), and the camera or phone where available, two to a row. Each
  is cleaned up alike: the paper made transparent and cropped to the ink.
- A digitally signed PDF shows a signatures button at the view toolbar's
  trailing edge, after Edit PDF and the Scale: the shield alone, so the toolbar keeps
  its room, with the verdict in its tooltip and accessible name: grey
  "Checking signatures…" while its signatures are
  checked in the background, then "Signed" (green shield), "Signer not
  verified" (amber: a signer's certificate is not trusted, out of date or
  cannot be checked) or "Signature invalid" (red: signed bytes altered or
  pages changed after signing). The worst signature decides. It is checked
  again after each save, which rewrites the file and so breaks its
  signatures. The button opens the Signatures dialog (top-right X or
  Escape closes it): one entry per signature with the signer, whether the
  PDF changed since, the certificate's issuer and trust, when it was signed
  (by its timestamp, else "by the signer's clock"), whether its
  certificate was revoked (asked of its issuer after the first result
  shows; a revocation before it signed makes it "Signature invalid"), its
  reason and location, and Show on page for a visible signature. Notes are muted,
  doubts about the signer in the body colour, broken signatures in red.
- File ▸ Sign with Certificate…, as in Acrobat and Revu: with a placed
  signature (an image) selected, the signature goes in its place (the
  signed copy draws it in the signature's box with "Digitally signed by",
  the name and date beside it, and leaves that markup out; the open PDF
  keeps it); otherwise a prompt over the top of the page (and the status
  bar) asks for a box where it goes. The box is drawn as the Snapshot tool
  draws, leaving no markup, while the rail shows the Signature tool in use;
  the prompt's X, Esc or another tool cancels, and a press on the prompt
  draws nothing. Signature ▸ From the page asks the same way. The dialog lists the system's signing
  certificates (looked up in the background; expired ones shown but not
  choosable), the first valid one chosen, then "A certificate file (.p12
  or .pfx)", which asks for the file (the last one chosen is offered
  again) and its password. A system certificate needs no password here;
  the system may ask for a PIN or permission as it signs. Then an optional
  reason; for a drawn box, Appearance: Name and details (Acrobat's
  standard text, chosen), Details only, or Invisible (no mark on the page);
  and "Add a timestamp from <server>" (on) when Settings ▸
  Signatures names a timestamp server, else a note that there is none.
  Sign stays disabled until a certificate is chosen; a wrong password or
  unusable file is said in the dialog, which stays open. Unsaved changes
  are saved first; then Save As suggests "<name> (signed).pdf" beside it.
  The signed copy opens in a new tab; choosing the PDF itself signs it in
  place (the tab closes while the signature is appended and reopens where
  it was). The status bar says "Signed by …"; cancelling Save As says "Not
  signed."; a failure leaves the PDF unchanged and says why.
- Settings ▸ Signatures ▸ Timestamp server: empty (none) at first; an
  `http` or `https` address is kept when the field is left or Enter is
  pressed, and anything else is called out below it.
- Dropping PDFs opens each in a new tab in that window (see Tabs and
  windows). Dropping PNG or JPEG images on a
  document places each at once as an Image, centred where it is dropped on
  the page under the pointer (or the nearest shown page, kept on it), at
  its natural size up to the Image tool's limit; several step 24 points
  apart. The last is selected, the tool is Select, and each image is one
  undo step. With no document open, images are ignored, as are other
  files.
