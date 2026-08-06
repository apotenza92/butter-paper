# Bluebeam compatibility reference

These findings were observed from a Bluebeam Revu sample PDF and manual UI inspection in May 2026. Raw captures and extraction outputs were intentionally removed from the working tree after the findings were consolidated here; they remain recoverable from Git history.

## Important caveats

- These are defaults observed from the current Bluebeam Revu sample PDF, not a full product spec.
- Some visual details are encoded in generated appearance streams (`AP/N`) rather than top-level annotation dictionaries.
- Selection/hover/control-point behaviour still needs screenshot-based inspection in Bluebeam; saved PDF annotations do not encode Bluebeam's UI chrome.
- Stamp and image insertion were intentionally not included by the user. The sample does include Snapshot, which Bluebeam stores as a stamp-like annotation.

## Cross-tool defaults observed

- Most markup drawing tools default to **red** stroke/text: PDF colour array `[1, 0, 0]`, CSS equivalent `#FF0000`.
- Most simple shape/line appearance streams use **1 pt** stroke width.
- Arrow uses **0.5 pt** stroke width in this sample.
- Highlight uses **yellow** `[1, 1, 0]`, blend mode `/Multiply`, and **12 pt** ink border width.
- Text-bearing markups use **Helvetica 12 pt**, red text, left alignment unless the tool is a dimension caption.
- Most shape tools are stroke-only by default: no fill colour (`IC`) except where arrowheads/dimension heads need fill or when a specific text/snapshot/note appearance requires it.

## Tool-by-tool findings

| Tool | PDF representation | Default content properties observed | Notes for Butter Paper |
| --- | --- | --- | --- |
| Text Box | `/FreeText`, `Subj: Text Box` | `DA: 1 0 0 rg /Helv 12 Tf`; `DS: Helvetica 12pt`, `color:#FF0000`, `margin:3pt`, `line-height:13.8pt`; border style width `0`; colour array empty | Text box is effectively red Helvetica text with no visible border by default. |
| Highlight | `/Ink`, `Subj: Highlight` | `C: [1,1,0]`; `BM: /Multiply`; `BS.W: 12` | Store as highlighter ink with multiply blend and wide yellow stroke. |
| Pen | `/Ink`, `Subj: Pen` | `C: [1,0,0]`; `BS.W: 1`; AP stream uses `1 0 0 RG 1 w` | Red 1 pt freehand ink. |
| Cloud+ cloud body | `/Polygon`, `Subj: Cloud+`, `IT: /PolygonCloud`, `ITEx: /PolyText` | `C: [1,0,0]`; `BE: { S: /C, I: 2 }`; AP stream uses `1 w` | Cloud+ is not one simple annotation. It is grouped cloud polygon plus text/callout annotation. Cloud border effect intensity is `2`. |
| Cloud+ text/leader | `/FreeText`, `Subj: Cloud+`, `IT: /FreeTextCallout`, `ITEx: /PolyText` | Red Helvetica 12 pt text; border width `0`; `CL` leader points; AP draws red 1 pt leader; `GroupNesting` links group | Model Cloud+ as a composite/group: cloud polygon + freetext callout/leader. |
| Cloud | `/Polygon`, `Subj: Cloud`, `IT: /PolygonCloud` | `C: [1,0,0]`; `BE: { S: /C, I: 2 }`; AP uses red 1 pt stroke | Plain cloud is polygon with cloud border effect, no fill. |
| Callout | `/FreeText`, `Subj: Callout`, `IT: /FreeTextCallout` | Red Helvetica 12 pt text; border width `0`; `LE: /OpenArrow`; `CL` leader points; AP draws red 1 pt leader/open arrow | Default callout text itself has no visible box border; leader and open arrow are red. |
| Line | `/Line`, `Subj: Line` | `C: [1,0,0]`; `BS.W: 1`; AP draws red 1 pt line | Straight red 1 pt line, no line endings. |
| Arrow | `/Line`, `Subj: Arrow`, `IT: /LineArrow` | `C: [1,0,0]`; `IC: [1,0,0]`; `BS.W: 0.5`; `LE: [/None, /ClosedArrow]` | Arrowhead is closed and filled red; shaft is 0.5 pt in this sample. |
| Arc | `/Circle`, `Subj: Arc`, `IT: /CircleArc` | `C: [1,0,0]`; `Angle1: 90`; `Angle2: 180`; AP draws red 1 pt arc | Arc is represented as a circle annotation with arc intent and angle metadata. |
| Polyline | `/PolyLine`, `Subj: PolyLine` | `C: [1,0,0]`; `IC: [1,0,0]`; AP draws red 1 pt path through `Vertices` | No line endings in this sample. |
| Dimension | `/Line`, `Subj: Dimension`, `IT: /LineDimension` | `C: [1,0,0]`; `IC: [1,0,0]`; `BS.W: 1`; `LE: [/ClosedArrow, /ClosedArrow]`; `LL: 10`; `LLE: 2`; `Cap: true`; centered Helvetica 12 pt red caption style | Dimension uses line annotation with both closed arrowheads plus leader-line extension metadata. |
| Length | `/Line`, `Subj: Length Measurement`, `IT: /LineDimension` | Has `Measure`, `MeasurementTypes: 130`, `Cap: true`, centered red Helvetica 12 pt `DS`, `Contents` label, and closed arrowheads | Preserve the PDF measurement dictionary and closed arrowheads. |
| Polylength | `/PolyLine`, `Subj: Polylength Measurement`, `IT: /PolyLineDimension` | Has `Measure`, `MeasurementTypes: 130`, `Cap: true`, `AlignOnSegment: true`, centered red Helvetica 12 pt `DS`, and `Contents` label | Bluebeam may repeat the first vertex at the end for a closed-looking sampled path. |
| Area | `/Polygon`, `Subj: Area Measurement`, `IT: /PolygonDimension` | Has `Measure`, `MeasurementTypes: 129`, `Cap: true`, `AlignOnSegment: true`, centered red Helvetica 12 pt `DS`, and `Contents` label | Bluebeam’s sample label uses `sq m`; Butter Paper currently formats generated area labels as `m^2`. |
| Rectangle | `/Square`, `Subj: Rectangle` | `C: [1,0,0]`; AP stream draws red 1 pt rectangle; no `IC` fill | Default rectangle is red outline, no fill. |
| Ellipse | `/Circle`, `Subj: Ellipse` | `C: [1,0,0]`; AP stream draws red 1 pt ellipse; no `IC` fill | Default ellipse is red outline, no fill. |
| Polygon | `/Polygon`, `Subj: Polygon` | `C: [1,0,0]`; AP stream draws red 1 pt closed polygon; no `IC` fill | Default polygon is red outline, no fill. |
| Snapshot | `/Stamp`, `Subj: Snapshot`, `IT: /StampSnapshot` | Rectangular stamp with appearance stream containing copied page content; `Rotation: 0` | Bluebeam Snapshot is stamp-like with an embedded appearance, not a vector annotation. |
| Note | `/Text`, `Subj: Note`, `Name: /Note` | `C: [1,1,0]`; popup annotation linked by `Popup`; flags `F: 28` | Note icon is yellow and has a popup child annotation. |
| Popup | `/Popup` | Linked to Note via `Parent` | Implementation detail for Note, not a standalone user tool. |

## Butter Paper compatibility defaults

Initial content-style defaults should be separated from interaction chrome. Suggested constants based on the extraction:

```ts
const BLUEBEAM_RED = '#ff0000';
const BLUEBEAM_YELLOW = '#ffff00';

const bluebeamDefaultText = {
  fontFamily: 'Helvetica',
  fontSizePt: 12,
  textColor: BLUEBEAM_RED,
  textAlign: 'left',
  marginPt: 3,
  lineHeightPt: 13.8,
  borderWidthPt: 0,
};

const bluebeamDefaultShape = {
  strokeColor: BLUEBEAM_RED,
  strokeWidthPt: 1,
  fillColor: null,
};

const bluebeamDefaultHighlight = {
  strokeColor: BLUEBEAM_YELLOW,
  strokeWidthPt: 12,
  blendMode: 'multiply',
};

const bluebeamDefaultArrow = {
  strokeColor: BLUEBEAM_RED,
  fillColor: BLUEBEAM_RED,
  strokeWidthPt: 0.5,
  startLineEnding: 'none',
  endLineEnding: 'closedArrow',
};
```

## Bluebeam UI chrome inspection

### Text Box selected state

Observed selected-state chrome:

- A light/bright blue dashed rectangular selection border surrounds the text box bounds.
- The annotation content remains unchanged: red text stays red; the selection does not recolour the text or add a content fill.
- Corner resize handles are small circular handles at top-left, top-right, bottom-left, and bottom-right.
- Edge resize handles are small square handles at center-left, top-center, center-right, and bottom-center.
- The top-center square edge handle sits on the selected bounds; a rotation control is a separate blue circular handle above it, connected vertically to the top-center area.
- Resizing the text box changes the text box bounds but does not scale the text glyphs themselves.

Butter Paper primitive decision from Text Box, generalized to all selectable tools where practical:

- Use a dashed blue selection bounds rectangle as the common selected-state primitive.
- Do not mutate annotation content colour/fill for selection.
- Use the selected bounds themselves as resize affordances:
  - hovering near corners changes to diagonal resize cursors (`nwse-resize` / `nesw-resize`);
  - hovering near horizontal edges changes to vertical resize cursor (`ns-resize`);
  - hovering near vertical edges changes to horizontal resize cursor (`ew-resize`);
  - this can replace tiny always-visible edge/corner icons in Butter Paper’s simplified UX.
- Preserve an explicit top rotation dot/handle above the selected bounds.
- For text boxes, resizing changes the containing box only; font size remains unchanged unless the user edits text properties separately.

### Hybrid/composite item selected state — Cloud+

Observed Bluebeam selected-state chrome:

- Cloud+ shows an outer blue dashed group bounds rectangle around the entire composite.
- The outer group bounds includes corner resize handles and a top rotation handle.
- Nested parts also expose their own manipulation affordances:
  - the text box has text-box resize handles;
  - the cloud body has its own local dashed bounds and resize handles;
  - the leader/connector has line/knee/end handles.
- This confirms the PDF extraction: Cloud+ is a composite/group, not a single primitive annotation.

Butter Paper hybrid/composite primitive decision:

- Show a global group bounds rectangle for hybrid/composite tools. It communicates the whole-item extent and provides a natural place for a global rotation handle.
- Visually distinguish group chrome from child-part chrome:
  - child editable parts use the standard selected blue dashed bounds;
  - global group bounds should use a secondary/lighter/different dashed style token so it reads as “group container”, not another editable child;
  - global group chrome should include the top rotation dot/handle for rotating the whole composite.
- Initial implementation should **not** support global group resize. Group resizing is semantically ambiguous for composites because it could mean scaling child geometry/text, reflowing child positions, or resizing only the group envelope. Prefer child-specific resize handles first.
- Future option: global group resize could act as a layout/reflow operation that moves child components relative to the new group envelope without scaling text glyphs or stroke widths, but this should be designed later with explicit rules.
- On selecting a composite item, also show selected bounds/chrome for the meaningful child parts:
  - cloud body gets its own dashed bounds, resize zones, and optional rotation dot;
  - text box gets its own dashed bounds, resize zones, and optional rotation dot;
  - connector/leader gets endpoint/knee handles.
- The child parts remain linked by the connector line/leader.
- Bluebeam behaviour observed by user: for Cloud+, dragging the cloud body moves the whole composite; the leader and text box can move independently.
- Adopt that as the preferred Butter Paper interaction model for Cloud+:
  - dragging the cloud body moves the whole Cloud+ group;
  - dragging/resizing the cloud handles edits the cloud body shape;
  - dragging the text box body moves only the text box while preserving/updating the leader connection;
  - dragging text box handles resizes the text box only;
  - dragging leader endpoint/knee handles adjusts the leader only;
  - no extra subselection gesture is required for the common independent text/leader movement case.

General multi-component tool primitive:

- Composite annotations must declare editable parts/components, e.g. `cloudBody`, `textBox`, `leader`.
- Composite annotations must declare global group chrome capabilities, e.g. `showGroupBounds`, `canRotateGroup`, and `canResizeGroup`. For now, default composites should use `showGroupBounds: true`, `canRotateGroup: true`, and `canResizeGroup: false`.
- Each component must declare body-drag behaviour:
  - `moveGroup`: dragging this component body translates the whole composite;
  - `moveSelf`: dragging this component body moves only that component and lets connectors/constraints update;
  - `adjustOnly`: body drag is not a move target, but handles/control points can adjust it.
- Each component may declare handles/control points with independent behaviours, e.g. `resizeSelf`, `rotateSelf`, `moveEndpoint`, `moveKnee`, `reshapeVertex`.
- Every composite annotation must have at least one clear `moveGroup` body target so users can move the whole item without needing a hidden modifier or command.
- If `showGroupBounds` is enabled, the group bounds should use a visually distinct group chrome token from child selected bounds.
- The group-move target should usually be the semantic anchor/main shape of the composite: cloud body for Cloud+, line body for some measured composite tools, or text box for a text-primary callout if that feels more natural.

### Callout selected state

Observed Bluebeam selected-state chrome:

- Callout has a large blue dashed selected bounds rectangle covering the text box plus leader/arrow.
- Text box exposes resize handles around the text area, plus top rotation handle for the overall selected bounds.
- Leader/arrow exposes control handles at the arrow tip, leader knee/elbow, and text-box connection point.
- User observation: in Bluebeam, dragging most of the callout moves the text box around, and the arrow tip is moved independently.

Butter Paper callout primitive decision:

- Treat callout as a composite annotation with at least `textBox` and `leader`/`arrowTip` components.
- Prefer a clearer interaction than Bluebeam:
  - dragging the text box body moves only the text box while preserving/updating the leader connection;
  - dragging the arrow tip moves only the arrow tip;
  - dragging leader/knee handles adjusts only that part of the leader;
  - dragging the leader line body should move the entire callout group, giving the composite an obvious `moveGroup` target;
  - text box handles resize the text box only;
  - rotation dot can rotate the callout/group when rotation support exists.
- This satisfies the composite primitive requirement that every hybrid/group annotation has at least one obvious whole-item move target.

### Vertex-shape selected state — Cloud and Polygon

Observed Bluebeam selected-state chrome:

- Irregular Cloud and Polygon both show global blue dashed bounds around the whole shape.
- The global bounds has corner resize handles and a top rotation handle.
- The shape itself exposes editable vertex/control points inside the global bounds.
- For Cloud, the underlying polygon vertices/control segments define the cloud border path; moving nodes reshapes the cloud.
- For Polygon, each polygon vertex is directly adjustable.
- Global bounds resize stretches/scales the whole vertex shape in the resize direction.

Butter Paper primitive decision for vertex-shape tools:

- Treat Cloud, Polygon, Polyline, Arc-like editable curves, and similar vertex/path tools as **single vertex-shape annotations**, not composites/hybrids.
- They may use both:
  - global selected bounds for move/resize/rotate of the whole shape;
  - internal vertex/control handles for local reshaping.
- For vertex-shape annotations, global resize is allowed and means geometric scale/stretch of the vertex coordinates relative to the opposite edge/corner anchor. Text is not involved, so scaling ambiguity is low.
- Vertex handles use `reshapeVertex` / curve-control behaviours and should not imply group/component movement.

Cloud-specific path primitive:
- Bluebeam shows a separate blue dashed path with yellow vertex nodes inside the red cloud border.
- The dashed path is not the rendered cloud stroke itself. It represents the underlying polygon/safe-zone path that the cloud border wraps around.
- For clouds, the annotation model should preserve both concepts:
  - `controlPath` / safe-zone polygon: editable vertices shown as dashed blue guide path with nodes;
  - `renderedCloudPath`: generated scalloped/cloud stroke offset around or wrapped around the control path.
- This should be modeled as a generic smart/generated line-type primitive, not as a one-off `cloudBorder` primitive.
- Regular Line, Arrow, Polyline, Dimension, etc. should use normal stroke/line-style primitives: colour, width, dash pattern, caps, joins, and line endings.
- Cloud rendering is one registered line-type renderer. Editing manipulates the control path; rendering regenerates the visible cloud/scallop path.
- Keep Bluebeam/PDF compatibility metadata for cloud intensity: Bluebeam writes PDF border effect `/BE << /S /C /I 2 >>`. In Butter Paper, expose clearer line-type options such as `offset`, `scallopRadius`, and `scallopSpacing`, while preserving `pdfBorderEffectIntensity` or an equivalent compatibility field for import/export fidelity.
- The tool-building API/SDK should start in TypeScript because tool UX, pointer interaction, selection chrome, properties, and custom tool registration are renderer/UI concerns. Keep PDF rendering, annotation read/write/flattening, and geometry acceleration behind package APIs so native implementations are introduced only for a demonstrated need.
