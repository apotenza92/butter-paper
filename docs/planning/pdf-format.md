# Markup file format (Bluebeam parity)

Decision (2026-10-02, user): the native app stores markups using only standard
PDF and the conventions Bluebeam Revu itself writes. No private `BP*` keys, no
`bp:` name prefix, no Electron-era compatibility readers. When a format question
comes up, match what Revu writes. Untouched markups from other applications are
saved byte-exact; only created or edited markups are rewritten.

Settings with no standard or Bluebeam field are not saved per markup: pen
smooth-curves, image aspect lock, measurement show-caption.

## Ground truth

Specimens were drawn in licensed Revu 21 (Windows 11 VM) with the GUI and
varied with `ScriptEngine.exe` `MarkupSet`, then saved by Revu. Reference files
live in `experiments/gpui-migration/gpui-migration/tests/fixtures/bluebeam/`.
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
- [x] Area captions are centred on the area, as Revu centres them.
- [x] A callout or Cloud+ without text writes no text object in its appearance.

## Known differences (follow-ups)

- Snapshot: Revu snapshots are vector Form copies of the page; native snapshots
  are raster. Revu snapshots are kept untouched (not editable). Making them
  movable needs the PDFium worker to rasterise the Form for the canvas and the
  save path to wrap the original Form in a transformed appearance.
- Cloud shape: Revu draws Adobe-style cloudy borders (fixture `revu-shapes.pdf`,
  `BE /I 2`): circles of about 8.45 pt radius centred on the polygon edges,
  about 14.1 pt apart, each drawn as an outward arc with a small hook where
  neighbours meet. Native clouds use shallower scallops on the canvas and in
  the regenerated appearance. Untouched Revu clouds and Cloud+ appearance
  paths keep Revu's drawing.
- Cloud+: Revu allows a cloud fill on Cloud+; the native Cloud+ cloud is
  unfilled, so an imported Cloud+ fill is dropped when it is edited.
- Measurement captions are horizontal; Revu turns the caption to follow a
  sloped dimension line. Length `LL` is fixed at 10 pt; a Revu Length with
  another offset is redrawn at 10 pt when edited.
- The TypeScript `packages/pdf` reader used by the CLI still understands the
  Electron private keys; it does not write PDFs.
