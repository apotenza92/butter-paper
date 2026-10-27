# Markup file format (Bluebeam parity)

Decision (2026-10-02, user): the native app stores markups using only standard
PDF and the conventions Bluebeam Revu itself writes. No private `BP*` keys, no
`bp:` name prefix, no Electron-era compatibility readers. When a format question
comes up, match what Revu writes. Untouched markups from other applications are
saved byte-exact; only created or edited markups are rewritten.

A PDF with only an owner password (it opens without one) is saved encrypted
again under its own key and `/Encrypt` settings, so its passwords and
permission restrictions survive. Files that need a password to open are not
editable.

Settings with no standard or Bluebeam field are not saved per markup: pen
smooth-curves, image aspect lock, measurement show-caption.

## Ground truth

Specimens were drawn in licensed Revu 21 (Windows 11 VM) with the GUI and
varied with `ScriptEngine.exe` `MarkupSet`, then saved by Revu. Reference files
live in `crates/butter-paper/tests/fixtures/bluebeam/`.
`tests/bluebeam_format.rs` writes the same markups natively and checks every
family's dictionary keys against these files.

## Common keys (every markup Revu writes)

| Key | Revu convention |
|---|---|
| `/NM` | 16 random uppercase letters; the markup id. New markups use the same form. |
| `/Subj` | Tool name (`Rectangle`, `Ellipse`, `Line`, `Arrow`, `PolyLine`, `Polygon`, `Cloud`, `Cloud+`, `Callout`, `Text Box`, `Pen`, `Highlight`, `Arc`, `Dimension`, `Length Measurement`, `Polylength Measurement`, `Area Measurement`, `Image`, `Snapshot`). |
| `/T` | Author (operating-system user name). |
| `/CreationDate`, `/M` | PDF dates; `/M` updated on edit. |
| `/P` | Page reference. |
| `/F` | `4` (print); `128` added when locked. |
| `/C` | Stroke colour. |
| `/CA` | Opacity, written only when below 1. Never `/ca`. |
| `/IC` | Fill colour (also arrow-head fill on lines). |
| `/FillOpacity` | Fill opacity (Bluebeam key), written only when below 1, except Area which always writes it. |
| `/Rotation` | Degrees, only on rotatable shapes; `/Rect` is then the rotated bounding box. |
| `/BS` | `<</Type /Border /W w /S /S>>`; dashed `/S /D /D [..]`. Square, Circle and Polygon/PolyLine omit it at the 1 pt solid default; Line, Ink and measurements always write it. |
| `/RD` | Square and Circle: half the stroke width on each side; `/Rect` is the drawn rectangle inflated by it. |
| `/Contents` | Only when the markup carries text. |

## Per family

| Tool | Subtype / IT | Extra keys |
|---|---|---|
| Rectangle | `Square` | `RD` |
| Ellipse | `Circle` | `RD` |
| Arc | `Circle` / `CircleArc` | `Angle1`, `Angle2`, `RD` |
| Line | `Line` | `L`, `BS`, `PitchRun 12`, `SlopeType 0` |
| Arrow | `Line` / `LineArrow` | `L`, `LE [/None /ClosedArrow]`, `IC`, `BS`, `PitchRun`, `SlopeType` |
| Polyline | `PolyLine` | `Vertices`, `IC` (= stroke) |
| Polygon | `Polygon` | `Vertices` |
| Cloud | `Polygon` / `PolygonCloud` | `Vertices`, `BE <</S /C /I i>>` |
| Pen | `Ink` | `InkList`, `BS` |
| Highlight | `Ink` | `InkList`, `BS`, `BM /Multiply` |
| Text Box | `FreeText` | `Contents`, `DA`, `DS`, `RC`, `BS <</W 0>>`, `C []` |
| Callout | `FreeText` / `FreeTextCallout` | `CL`, `LE /OpenArrow`, `RD`, `DA`, `DS`, `RC`, `BS <</W 0>>`, `C []` |
| Cloud+ | `Polygon` / `PolygonCloud` + `FreeText` / `FreeTextCallout`, both `ITEx /PolyText` | text has `GroupNesting [(Cloud+) /textNM /cloudNM]`; cloud has `RT /Group`, `IRT` → text |
| Dimension | `Line` / `LineDimension` | `LE [/ClosedArrow /ClosedArrow]`, `LL`, `LLE`, `Cap true`, `DS`, `IC` |
| Length | `Line` / `LineDimension` | as Dimension plus `Measure`, `DepthUnit`, `MeasurementTypes 130`, `Label ()`, `Contents`, `RC`, `SlopeType 1` |
| Polylength | `PolyLine` / `PolyLineDimension` | `Measure`, `DepthUnit`, `MeasurementTypes 130`, `AlignOnSegment`, `Cap`, `RiseDrop 0`, `Label`, `Contents`, `RC`, `DS`, `IC` |
| Area | `Polygon` / `PolygonDimension` | `Measure`, `DepthUnit`, `MeasurementTypes 129`, `FillOpacity`, `AlignOnSegment`, `Cap`, `Label`, `Contents`, `RC`, `DS`, `PitchRun`, `SlopeType 1` |
| Image | `Square` / `SquareImage` | `Image` stream, `BS <</W 0>>`, `RD [0 0 0 0]` |
| Snapshot | `Stamp` / `StampSnapshot` | `Rotation` (always) |
| Redact | `Redact` | ISO 32000 only (`QuadPoints`, `IC`, `OverlayText`); Revu Standard has no redaction tool to compare. |

Text style: `DA (r g b rg /Helv 12 Tf)` (`/HelvBld` when bold), `DS (font: [bold ]Family Npt; text-align:left|center|right; margin:3pt; line-height:Lpt; color:#RRGGBB)`. `RC` is Revu's XHTML body (`xfa:APIVersion="BluebeamPDFRevu:2018"`) with one `<p>` per line; a bold or aligned paragraph carries `style="font-weight:bold; text-align:center"`. No `/Q`.

Page scale: page `/VP [<</Type /Viewport /BBox [...] /Measure <</Type /Measure /Subtype /RL /R (1 cm = 1 m) ...>> /NM (...)>>]` — no private page-scale key.

## Status (2026-10-02)

- [x] Revu ground truth captured for every tool and the main property variants
  (`tests/fixtures/bluebeam/`).
- [x] Native writers emit only the keys above; `tests/bluebeam_format.rs`
  compares every family's key set with the Revu specimens and rejects any
  `BP*` key anywhere in the output.
- [x] Importers read only standard/Bluebeam keys; Electron and private-key
  readers removed. Indirect `/BS`, `/Measure` etc. (as Revu writes them) are
  resolved. Page scales come from `/VP`.
- [x] Untouched imported markups and page viewports are saved byte-exact; a
  markup the model cannot represent is kept untouched instead of failing open.
- [x] Revu (21, ScriptEngine `MarkupGetExList`) reads all 20 native markups
  with the same property set and values as its own; it renders them and shows
  the 1:100 scale. Revu `MarkupSet` edits to native markups, saved
  incrementally, reopen typed with Revu's changes (`revu-edited-native.pdf`).
- [x] lopdf upgraded to 0.45.0: 0.44's PNG "Average" predictor bug misread
  Revu's incremental cross-reference streams.
- [x] Fill opacity is independent of opacity (Revu `ca` = `FillOpacity`).
- [x] Measurements use Revu captions (`2,697.37 mm`, `5.46 sq m`), Revu's
  base-unit `/Measure`, and keep the exact page-scale factor (previously
  rounded to 6 decimal places of a per-point factor, ~6 ppm error).

- [x] Clouds can be filled (`IC`, `FillOpacity`), as in Revu; the canvas, the
  Cloud tool defaults and the Cloud properties panel all expose the fill.
- [x] Callouts without text import and save as callouts.
- [x] Revu dash arrays (`dashed1..6`) are kept on edit when the width and
  style are unchanged.
- [x] Callout and Cloud+ text draw a box border when `/BS /W` > 0 (Butter
  Paper writes `W 0` for the default 1 pt leader).
- [x] Dimension: no label by default, no `Contents`/`RC` when unlabelled,
  `LLE 2`.
- [x] Length and Dimension appearances follow Revu's drawing (2026-10-02
  fixture streams): extension lines from the points to `LL + LLE`; tips `w`
  inside the extension lines; closed arrowheads `7.8w` long and `9w` wide,
  filled and stroked (`b`); when the caption and both arrowheads do not fit,
  the arrowheads sit outside on `15.6w` tails; the caption is centred on the
  dimension line. Length uses `LL 10` on screen as well as in the file.
- [x] Text box, callout and Cloud+ appearances use the style's margin (`DS
  margin`) instead of a fixed inset.
- [x] Text box appearances place each line as the canvas does: the text's
  ascent and descent are centred in a 1.15 em line box, the first one margin
  below the top, and a line holding larger rich text is as tall as its
  largest size. The metrics are the written font's: the embedded bundled
  font's, else Helvetica's standard ones.
- [x] Area captions are centred on the area, as Revu centres them.
- [x] A callout or Cloud+ without text writes no text object in its appearance.

- [x] Clouds use Revu's (Adobe's) cloudy border, measured from Revu 21
  streams: curls are circles centred on the outline at equal perimeter
  spacing from the first vertex (nominal 7.05 pt × intensity, rounded to a
  whole number of curls), radius 0.6 × spacing; each curl is an outward arc
  between its neighbours' meeting points that runs 21.2° past the next one and
  hooks back (handles 0.1034 r). `/Rect` is the outline grown by 1.789 r.
- [x] Cloud+ clouds can be filled (Cloud+ tool default); a filled Cloud+
  appearance path (`B`/`b`) imports.
- [x] Length and Dimension captions turn to follow the line, kept upright
  (angle within (-90°, 90°]), on the canvas and in the appearance.
- [x] Revu vector Snapshots are editable Snapshots. Import records the
  original Form; the PDF worker rasterises it (render mode `vector_snapshots`,
  one layer page per Snapshot, transparent ground) for the canvas. An edit
  writes a new appearance with Butter Paper's rotated-box placement that draws
  Revu's original Form fitted to the new box; untouched Snapshots stay
  byte-exact.

## Edited page text

Edit PDF changes the page content stream, not an annotation
(`pdf_text_edit.rs`). An edit names a line by its origin and original text
and always applies to the unedited file.

- Only the edited text operators change; every other byte of every content
  stream is kept, and an edit that leaves a line as it was changes nothing. The line's first operator draws the whole edited line,
  keeping unchanged glyphs' codes and positions (kerning and word gaps);
  the line's other operators become text-matrix moves of their original
  width, so following content stays where it was.
- New characters use the line's font when its program has the glyph
  (ToUnicode, Differences, or a code learned from `/ActualText`; in a
  TrueType subset without either, as Word writes, a code whose glyph the
  program draws and the font gives a width, looked up by code when the font
  is symbolic and by character otherwise). A Type 1 or CFF subset lists its
  whole encoding (pdfTeX's Differences) but keeps only the glyphs it drew,
  so a code counts only when the program has a glyph of its name; a code
  mapped to `.notdef` never does. A subset with a ToUnicode map types only
  the codes the map lists, as viewers read no other. Others use
  a bundled Type0 font (Identity-H with ToUnicode) added to the page's
  `/Resources /Font` as `BPEdit<n>`. Glyphs readable only through
  `/ActualText` are redrawn inside their own `/ActualText` span.
- Fonts using Adobe's Unicode CMaps (`UniGB`, `UniCNS`, `UniJIS`, `UniKS`;
  UCS2 or UTF16, horizontal) read their codes as text and their widths by
  CID through tables packed from Adobe's CMap resources (`assets/cmaps`).
  Characters the page already draws in such a font are typed with it;
  others use a bundled font.
- When a font's name and descriptor do not say serif or sans, the outline
  of a plain stem letter ("l", "I", "i", "H", "T") in its embedded program
  decides the bundled family.
- Text in a Form XObject is edited in the form's stream. The edited form,
  and every form that draws it, becomes a copy private to the page (another
  page using the form keeps the original), and fallback fonts join the
  copy's own resources. Font keys name the form path (`Fm0/F1`), so a form's
  fonts never clash with the page's. A form drawn more than once on a page
  is read-only there.
- Installed fonts (TrueType outlines; the platform's font folders, scanned
  once) supply characters the bundled fonts lack, and an installed font
  named like a non-standard page font supplies its missing characters
  first. They are embedded as tagged subsets the same way. A font the viewer
  supplies (not embedded) and without `/Widths` takes its widths from an
  installed font of its name or the closest bundled family.
- Bundled fonts (for edited text and Text Box appearances) are embedded once
  per document as a compressed TrueType subset that keeps the bundled glyph
  ids, so `CIDToGIDMap` addresses the bundled font; the name carries the
  required subset tag (`ABCDEF+Arimo-Regular`). Later edits re-subset the
  same font. A font counts as bundled only if its program is exactly that
  subset (or, in older saves, the whole bundled program), so another
  producer's subset of the same family is never reused.
- A rewrapped paragraph is stored as line edits. A line's fit moves it
  (centred or right-aligned text) or stretches it across its spaces to the
  paragraph's right edge (justified text). Lines the paragraph grew by are
  drawn by the last line's operator, each on its own baseline a line pitch
  below, inside the same text object; the text and line matrices are then
  restored, so following operators draw where they did. An added line's
  operator starts a hundredth of a point along from the paragraph's (its
  glyphs do not move) when another line already has that origin, so line
  anchors stay unique. After a save, added lines are lines of the file;
  undoing past them empties them.
- Text properties are run styles. A colour is written as `r g b rg` before
  its glyphs, and the line's own fill operators (whatever its colour space)
  are written again after them. Runs of one line may differ in device
  colour (`g`, `rg`, `k`, or `sc`/`scn` in a device space), so such a line
  reads back as one line with coloured runs; a line in another colour space
  only joins text of its own colour. A chosen font (a bundled family, or an
  installed face by PostScript name, with the bundled family of its kind
  for characters it lacks) is embedded as a tagged subset like a fallback
  font; the run keeps its page font's text state. A size is the run's `Tf`
  size on the line's baseline.
- A chosen alignment refits every line of the paragraph. A chosen line
  spacing, or text larger than the paragraph's own, keeps the first line and
  draws the paragraph's other text as added lines (the original lines are
  emptied), each placed by its own height and moved along to its column, so
  they continue at the top of the next column. The settings are kept in the
  first line's edit (`paragraph`) so the paragraph reopens with them; they
  do not change how it is drawn.
- A list line that changes between an item and an item's wrapped line is
  drawn as an added line in its place (the base line is emptied), so it
  does not take the base line's marker spacing. A line's fit `tab` starts an
  item's text after its marker at the list's indent. A line or added line
  ending in a hard break (an item's end) records `hard_break`, so the
  paragraph reopens with its items apart.
- Spacing follows the original: each kept glyph keeps the gap before it,
  and the gaps on both sides of a space (which justification widens) go
  with the space when it is deleted. A space the original drew as a gap is
  drawn as a space glyph where a gap would not read back as one (at either
  end of the line, or beside other whitespace).
- A span's `/ActualText` takes precedence over the font's ToUnicode, which
  some producers (LibreOffice) fill from a ligature's first use.
- ToUnicode maps lopdf rejects (a bare word for the CMapName, AutoCAD's
  bracketed values) are read directly. A glyph mapped to one character
  outside the Basic Multilingual Plane repeated (Office's maths letters,
  "𝑥𝑥" for 𝑥) reads as that character once, and a glyph mapped to a tab
  reads as a space when the font maps nothing else to one.
- The standard Symbol font, not embedded and without an `/Encoding`, reads
  by its built-in encoding and Adobe's widths; its serif and sans ©, ® and
  ™ read as those characters.
- With a negative font size (y-down pages, with `-100 Tz`), added lines are
  placed away from the glyphs' tops, down the page.
- The worker previews edits as an incremental update: the file's bytes
  followed by only the objects the edits change, in the file's own
  cross-reference form. New objects are numbered from the file's `/Size`,
  never reusing its cross-reference stream's number. It writes the whole file when the source has no
  cross-reference to continue or is still encrypted. Saving writes the
  whole file as before.
- Saving checks every edited line in the reopened file. After a save the
  undo history is rebased onto the saved text.

### Edited tables

A table edit (`pdf_tables.rs`, `PageTableEdit`) is kept with the page's
text edits and applied in the same pass to the unedited file. It records
the base table's row and column boundaries and its rows and columns as
edited: each a base band at a size, or a new band like a base one (a row
dragged to a height also keeps that height as `set_size`), plus new cell
text. Content outside the table never moves with it. The app writes base
bands at the size their text needs or a column border was dragged to, and
new rows (a line of their model's text tall, kept as their `set_size` so
fitting rows to text starts from it) and deleted ones; moved bands are
still read and drawn as described here.

Tables are found only from the structure tree (`pdf_structure.rs`): the
page's `/StructParents` entry in `/ParentTree` names the element of each
marked-content identifier, whose nearest `TD`/`TH`, `TR` (directly or in
`THead`/`TBody`/`TFoot`) and `Table` ancestors place it, with `/RowSpan`
and `/ColSpan` from `/Table` attributes and `/RoleMap` applied. Saving an
edited table keeps the tree true to it:

- New cell text is drawn in `/P <</MCID n>> BDC … EMC`, `n` the page's next
  free identifier; an `MCR` for it is added to its cell's `/K` and the
  page's `/ParentTree` array maps `n` to the cell.
- A new row is a new `TR` of new `TD`s (`TH` where its model row is all
  header cells), inserted among the page's rows; a new column adds a cell
  to each row. Deleted rows and columns leave the tree, and the
  identifiers of their content map to `null`.
- Rows and columns of a table that continues on another page, or has
  merged cells, are not changed: the save refuses rather than leave the
  tree untrue.

- Only the page's own content streams change. A path construction operator
  in the table is rewritten with its points moved. A subpath that runs
  from one band boundary to another (a border, a cell's fill) stretches:
  each point keeps its offset from its band's start, or stays at its
  band's end when within 1.5 points of it. Any other subpath (a border
  below a band, a drawing's stroke, even one over a border) moves whole:
  with its band's end when it lies at the end, otherwise with the start of
  the band at its middle. A deleted band's content collapses to zero size.
  Clips move but are never copied.
- A new band's borders and fills are copies of its model band's subpaths,
  written after the original's painting operator with the same operator,
  so they draw in its colour, width and clip.
- A text line in the table is redrawn by its first operator from its text
  matrix moved by its band's offset (`Tm`), then the line's own matrices
  are restored, as for added lines; a deleted band's lines are emptied.
  Images and forms the page draws in the table are wrapped in
  `q 1 0 0 1 tx ty cm … Q`.
- New cell text is drawn after the page's content in its own text object,
  the content wrapped in `q`/`Q` first so it starts from the default state:
  each line placed by `Tm` at its cell's top left plus its offsets, sizes in
  page units, in the table's page font or a bundled one.
- Saving checks each edited line where the table moved it, and that new
  cell text reads back.
- A save that rewrites a page around its tables keeps the page as the file
  had it (`pdf_page_base.rs`): a one-page PDF of its content streams and
  resources, named by its SHA-256 and kept at most 16 MB. Undo and redo
  states before the save apply their edits of that page to it: it replaces
  the saved page's `/Contents` and `/Resources` first (its annotations,
  boxes and rotation stay). Kept pages are part of recovery; the PDF worker
  is sent each once. A page too large to keep is shown as saved by every
  state, as before.

### Moved page content

A move (`pdf_content_moves.rs`, `ContentMove`) is kept with the page's
text and table edits and applied in the same pass to the unedited file.
It names a piece of content by its kind (`text`, `drawing` or `image`) and
where the unedited page draws it (`rect`: a text line's core box, a painted
path's bounds or the bounds of the small clip it is drawn in, an image's
or form's placed bounds), and records how it is placed: scaled by `scale`
(drawings and images only) from `rect`'s bottom-left corner, then moved by
`offset`. `removed` deletes it. With `copy` (a number), it is instead a copy
of that content, placed the same way, drawn as well as the original.

- Only the page's own content streams change. A moved text line is redrawn
  by its first operator at its moved place, as a table moves its lines; a
  moved or scaled path's construction operators are rewritten with every
  point placed; a moved or scaled image or form is wrapped in `q … cm … Q`.
  A removed line is emptied, a removed path's painting operator becomes
  `n`, and a removed image's `Do` goes.
- A copy is drawn again right after its original, in its graphics state
  (colour, line width, clip): a path's construction operators and painting
  operator, or an image's `Do`, are repeated inside `q <matrix> cm … Q`. A
  copied drawing in a small clip repeats the whole `q`…`Q` block around its
  clip after that block. A copied text line is drawn as new text, as new
  cell text is, in its fonts and fill.
- What a clip covering under half of the page box clips is one drawing,
  named by the clip's bounds: the clip, its paths and its images move
  together, and the app moves the text lines it clips as text moves.
- A moved piece moves by its move alone: table edits do not move it.
- Markups are annotations, not page content: the app moves those whose
  centre is on moved content itself, in the same undo step.
- A page with moves is kept as the file had it when a save rewrites it, as
  for tables, so undo and redo reach back past the save.

## Arranged pages

Pages added, deleted, moved or copied in Page Thumbnails
(`pdf_page_structure.rs`) are saved by rebuilding the page tree; every
other change of the save then names pages in their new order.

- The page tree becomes one `/Pages` node listing the pages in order. Each
  page takes the attributes it inherited (`/Resources`, `/MediaBox`,
  `/CropBox`, `/Rotate`).
- A new blank page is a `/Page` with a `/MediaBox` and empty resources.
- A copied page has its own content streams and top-level resource
  dictionaries (fonts and images are shared), so editing one copy's text
  leaves the other's alone. It leaves out the page's markups the app edits,
  which the save writes again as copies under new `/NM` names, and its form
  widgets; other annotations (links, popups) are copied with it. It has no
  `/StructParents`.
- A deleted page is removed with the form fields whose widgets are all on
  it; outline entries and links to it become null, and the save drops the
  page's objects from the file.
- Renders apply the same arrangement (`DocumentEdits::pages`) before the
  other edits.

## Merged documents

`pdf_merge.rs` merges saved PDFs into a new one: the first file's catalog
(outline, metadata) stays, and each later file's pages, with their
annotations, join its page tree in order. Form fields join the first
file's `/AcroForm`; a top-level field whose name is taken is renamed
`Name (2)`, so fields keep their own values, and the form's `/DR` gains the
fonts it lacks. A file with a markup `/NM` the files before it use has its
markups renamed `merged-N:name`. Protected (encrypted) files are not
merged.

## Filled form fields

Butter Paper fills the AcroForm fields a PDF already has (`pdf_forms.rs`).
It does not create fields, run form JavaScript, sign signature fields or
fill XFA forms.

- A value is written as the field's `/V` on the node that names it: text as
  a PDF text string (UTF-16 with a byte-order mark when it has non-ASCII
  characters or line breaks); a check box or radio button as its on-state
  name, or `/Off`, with each widget's `/AS` set to match; a choice as a
  string or, for several choices, an array, with a list box's `/I` indices.
- Each widget of a filled text or choice field gets a new normal appearance
  (`/AP /N`): the `/MK` background and border colours, the `/BS` (or
  `/Border`) width and style, and text in the `/DA` colour and size. A zero
  size fits the text to the field (a multi-line field shrinks from 12 pt to
  fit its lines). Comb fields centre a character in each cell, multi-line
  fields wrap at words, a combo box shows the chosen option's display text,
  and a list box lists its options from the top one shown, highlighting the
  chosen ones. `/MK /R` rotation becomes the appearance's `/Matrix`.
- The `/DA` font picks the family: Times names draw in Tinos, Courier names
  in Roboto Mono, and others in the standard Helvetica (WinAnsi). Text
  Helvetica cannot encode is set in Arimo, its metric twin, and characters
  the family lacks fall back to Noto Sans SC and Noto Emoji, embedded as
  subsets.
- Check boxes and radio buttons keep the appearance states they have; one
  without them gets ZapfDingbats on and off states (`/MK /CA` or a check
  mark; a dot for radio buttons).
- A form with both AcroForm fields and XFA loses its `/XFA` when filled, so
  viewers that prefer XFA cannot show stale values. `/NeedAppearances` is
  left as it is.
- For display only, the worker draws fields with no appearance (all text
  and choice fields when the form sets `/NeedAppearances`) from their
  values, so they never show the missing-appearance marker. Saving changes
  only the fields that were filled.
- Saving checks each filled value in the reopened file. After a save the
  undo history is rebased onto the saved values, as for page text.
- Revu 21 reads the values Butter Paper writes (its `FormExport` lists
  them, Unicode and line breaks included) and keeps them when it re-saves
  the file.

## Known differences (follow-ups)

- Polylength: Revu also labels each segment along the line; Butter Paper
  shows the total only.
- Length `LL` is fixed at 10 pt; a Revu Length with another offset is redrawn
  at 10 pt when edited.
