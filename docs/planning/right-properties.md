# Expanded right properties sidebar

Status: Highlight tool-defaults slice implemented; user visually approved the final reference/before/after screenshots. Recorded automated checks pass; live mid-drag verification remains limited as detailed below. Other tool and selected-annotation states require their own comparison before acceptance.

## Approved region

The user approved `test-results/parallel-pilot/proposed-right-properties-region.png` in the main checkout. Bounds x=779,y=108,w=289,h=660; padded context x=763,y=92,w=321,h=676. Includes the header, property controls, separators, scrolling and canvas boundary. Excludes the adjacent toolbar rail. The sidebar starts directly below tabs, beside the canvas toolbar, not beneath it.

## First implementation slice

Replace the interim document-actions presentation when Highlight defaults are being edited with a stock-component properties panel: Highlight header, top-right X, Appearance disclosure, colour, stroke width and opacity controls, and a Reset disclosure with confirmation. Mutations belong to the workspace's existing per-document Highlight defaults, never a fabricated selected annotation. Switching or closing documents must not apply stale events to another document. Saving disables mutation. Keep other document actions reachable through their existing menu paths and retain other interim tool paths until reviewed.

Use GPUI Component controls and default focus/animation. Keep full-height scrolling, semantic borders, rem-based panel and control sizing, and canvas-relative view-button centring. Do not customise control geometry to mimic Electron.

## Success gate

- Reference/target crop and surrounding region reviewed at rest and with colour/reset overlays.
- Single click selects Highlight; double click toggles its expanded panel; X closes it without changing defaults.
- Valid colour, width and opacity edits affect subsequent highlights, not existing annotations; reset requires confirmation and cancellation preserves values.
- Document identity, disabled state and invalid input are covered by deterministic tests, plus current native and repository checks.
- Final live Mac screenshots show reference, target before and target after. Other property families remain explicitly unaccepted until reviewed.

## Implementation and verification

- Uses stock Accordion (borderless option), Field, ColorPicker, Slider, Input, Button, AlertDialog and Resizable. Default focus and animations remain unchanged. Header is centred with symmetric space around the stock X; section and canvas boundaries are explicit.
- Width is 1–48 pt; opacity is 0–100% with 5% slider steps. Typed valid values commit on Enter/blur; invalid values restore the canonical value. Slider Change events update the workspace defaults and number continuously, including before pointer release. Defaults do not alter existing annotations or history.
- The user requested a stepper-free trial matching Electron: stock Input replaces NumberInput without customising either component. Inputs use non-shrinking 80-logical-pixel slots with pt/% suffixes. The row uses gap_4 rather than gap_2: the stock thumb projects beyond its track, and the previous gap left no painted clearance at maximum. Preserve room for the hover ring as well as the resting thumb.
- The stock colour picker's optional featured row repeats colour-derived accessibility IDs already in its main palette and crashed the Mac debug app when AX was active. The supported `featured_colors(Vec::new())` palette-only configuration removes that duplication without forking or restyling the component. Live palette selection was verified after the correction. Keep this explicit compatibility configuration until the pinned library supports unique featured IDs.
- Expanded/collapsed layouts use separate stock resizable keys so hidden-slot measurements cannot corrupt the first drag. A native pointer regression verifies 240/420-pixel bounds, restoration to 300, readable input slots and rail pinning. Resize completion notifies the workspace so its viewer controls can refresh.
- Viewport planning also synchronises the retained toolbar after resolving fit zoom; live resizing now shows 110% → 120% → 110% with the canvas instead of retaining the larger percentage after restoration.
- Current focused checks: 111 library, 20 native application, 5 viewer state, and 1 real Highlight control/history/scene test. Includes pointer-down and intermediate-drag number synchronisation before release, 16-pixel track/input layout gap, invalid width recovery, Cancel/Reset clicks, stale-document rejection and saving guards. Not a complete native-suite run.
- The corrected Mac bundle builds; pnpm check passes (1,088 main tests plus 23 relay tests, hygiene and typechecks), and git diff --check passes.
- Computer Use verified typed width/opacity, invalid width recovery, blur commits, track-click/input agreement and Reset restoring yellow/12/100. Final default-width screenshot checks visible thumb/input clearance. Continuous drag updates are proved by the native window regression; Computer Use drag delivery was inconsistent, so no live mid-drag claim is made. Minimum/maximum/restored sizes pass native geometry checks; the earlier narrow live capture predates the final spacing correction. Existing Electron reference remains unchanged; no PDF fixture was saved.
- Main-checkout evidence: `test-results/parallel-pilot/right-properties-input-comparison.png` (existing Electron reference / live stepper baseline / final stock inputs). Full captures are `right-properties-input-before.jpg` and `right-properties-input-after.jpg`. Earlier panel/palette evidence remains in `right-properties-comparison.png` and `right-properties-final-palette.jpg`.

## Remaining boundary

The user accepted the visual result of this Highlight slice. This does not close the recorded live mid-drag verification limitation. Other tool defaults, selected-annotation property families, exhaustive constrained-window combinations and overall application integration are not signed off by these checks. Keep each subsequent family comparison and acceptance explicit.

## Universal properties toolkit migration

The user approved moving from individual property-family slices to one coordinated migration, with reusable application-level property UI composed from stock GPUI Component controls. The existing expanded-sidebar region remains the spatial boundary; the rail, canvas tools and unrelated global UI are not being redesigned. One consolidated reference review and final acceptance gate replace repeated per-family approvals. A family is not complete merely because it uses shared controls.

### Toolkit requirements

- Build repeated property patterns from real Electron fields: labelled colour, numeric/slider with units, choice, toggle and text controls, plus sections, reset and sidebar structure. Preserve stock internals, component sizes, default focus and motion. Do not add a gallery, dependency fork, generic schema engine or speculative fields.
- Share layout and control synchronisation, not document ownership. Tool defaults, selected-object edits and mixed selection are different targets. The workspace/domain layer owns validation, applicability, locking, undo/redo, save/reopen and rejection of stale document/selection events.
- Keep retained editing state stable within a target and discard or reconcile drafts explicitly when the target changes. External synchronisation must not emit user edits or overwrite an active draft on every render.
- Preserve live slider/number agreement and painted thumb/hover clearance from the accepted Highlight slice. For selected-object edits, establish preview/commit and undo grouping from the actual reference rather than recording each drag frame as an independent undo command by accident.
- Add reusable pieces only when a real family needs them; retain family-specific controls. Treat unsupported reference behaviour as an explicit implementation gap, never as a successful hidden/disabled substitute.

### Unified acceptance

Inventory every existing reference family and applicable fields before implementation. Capture a consolidated Electron reference sheet with padded sidebar boundaries and distinguish tool defaults, selection, multiple-selection/locked state and overlays. The toolkit must not invent aggregate mixed-value editing if the reference presents only a primary selected item. Do not present a planning mock-up as a migrated after image.

For each applicable family verify real control edits, validation, ranges/units, reset/cancel, correct defaults versus selected-object scope, history, save/reopen and disabled/stale-target guards. Shared tests do not substitute for per-family wiring tests. Compare the final reference/before/after layouts, minimum/default/maximum panel sizes, text metrics, labels, separators, edge spacing and canvas/rail adjacency. Keep automated, live, visual and user-acceptance results separate.

### Current checkpoint

- Lead session metadata confirms gpt-6-astra; selected low effort is unchanged. Astra-orchestrate is active. Native workers use fresh context (`fork_context: false`, the available API equivalent of no forked turns).
- Electron read-only inventory: Plato, agent `01a09ab5-19be-7d80-be35-4998b70d374b`, runtime-confirmed gpt-5.6-luna/max. Completed and closed; findings consolidated below, critical updater/model/selection/history claims inspected by the lead. No writes or UI interaction by the worker.
- GPUI read-only inventory: Turing, agent `01a09ab5-1a45-7471-8aa9-39d1ca9b3f55`, runtime-confirmed gpt-5.6-luna/max. Stopped and closed when the missing-capability scope decision became a prerequisite. No final inventory delivered; do not claim native-family coverage from this worker. Resume or narrow that read-only inventory after the user decision. No active workers remain.
- Implementation workers (fresh context, runtime-confirmed Sol/high): Feynman `01a09acc-750d-7f81-8bdb-d2ea23f92ba2` completed shared controls, selected inspectors and primary-safe event wiring; Euler `01a09acd-90d7-7500-bbcd-856f66d5302c` completed adapter/default model and preview parity; Hubble `01a09ace-e87f-7b50-8e1b-de2180c2d106` completed defaults-panel presentation and ported ten inspector tests. Pasteur supplied the alpha correction described below. All workers are closed; lead owns final integration, plan, reference evidence and acceptance.
- Next: user review of the implemented column/slider correction and its comparison boards below. General visual approval did not waive structural requirements. Native capability, remaining selected-family visual states and save/reopen gaps remain open. Correction baseline: main-checkout test-results/parallel-pilot/properties-toolkit/candidate-final-selected-rectangle.jpg and candidate-final-selected-layout.jpg. Earlier before evidence retains the old two-pane defect. No external issues, separate app tasks or recurring automations are authorised or needed.

### Consolidated reference evidence

Fresh Mac Computer Use captures use the Electron main checkout at main@9e947ae4 dirty, isolated disposable application data, dark appearance, a 1152×768 window and the repository-generated tests/fixtures/generated/multi-page.pdf. No PDF was saved. Dark captures document structure and controls, not colour parity against the earlier light Highlight evidence. The previous temporary Rail-review.pdf has expired.

Main-checkout evidence lives under test-results/parallel-pilot/properties-toolkit/. Full captures retain the window; contact sheets use the established padded x=764,y=92,w=320,h=676 crop, including canvas and rail context.

| Reference set | Captured defaults or state |
| --- | --- |
| reference-sheet-1.png | Text Box, Arrow, Rectangle, Pen, Cloud, Cloud+, Callout, Dimension |
| reference-sheet-2.png | Highlight, Ellipse, Line, Polyline, Arc, Polygon, Length, Polylength |
| reference-sheet-3.png | Area, Redact, Snapshot, Select with no selection |
| reference-selected-rectangle-top.jpg / reference-selected-rectangle-bottom.jpg | Existing rectangle: lock, appearance, line style, fill colour/opacity, hatch, position, dimensions and rotation; captures show both scroll positions |
| reference-multiple-shapes-primary-rectangle.jpg | Rectangle and ellipse selected together; sidebar still presents the primary Rectangle and its values, not a mixed-value summary |

The selected-object captures use unsaved test shapes in the fixture. Signature/image flows, other selected families, locked controls, overlay variants and full gesture/undo semantics still require reference evidence; these sheets are not a complete universal acceptance package. Current discovery checks: pnpm check and git diff --check pass; no new application behaviour is claimed.

### Electron source inventory and scope decision

Source paths below refer to the Electron main checkout, not the native migration worktree. Canonical owners are apps/desktop/src/renderer/src/components/ToolPropertiesPanel.tsx, components/domain-ui/PropertyControls.tsx, pdf-tools/toolRegistry.ts, pdf-tools/toolPropertyDefaults.ts and the built-in tool definitions. The main checkout's planning copy is stale; do not adopt its queued status over this plan.

| Defaults family | Shared fields and specific controls |
| --- | --- |
| Rectangle, ellipse, polygon | Stroke colour; width 0.25–24 pt in 0.25 steps; transparent/colour fill; opacity |
| Line, polyline, arc, arrow | Stroke colour, width, opacity; Arrow defaults to 0.5 pt rather than 1 pt |
| Pen | Stroke colour, width, opacity, smooth-curves toggle |
| Highlight | Accepted colour, width 1–48 pt, opacity composition; fixed internal multiply blend |
| Cloud | Stroke colour, width, opacity; cloud intensity 0–4 in 0.25 steps |
| Cloud+ | Text/stroke colour, font size, opacity, cloud intensity |
| Callout | Text/stroke colour, font size, opacity |
| Dimension, length, polylength, area | Text/stroke colour, stroke width, font size, opacity |
| Text Box | Text colour, font size, font family, opacity |
| Image, snapshot | Opacity; media acquisition remains a separate workflow |
| Select, pan, redact, imported annotation | No configurable defaults; retain meaningful empty/unavailable presentation |

Tool-default opacity is stored as 0–1 and displayed as 0–100%, with 5% slider steps. Font-size schema defaults are 12 pt with 6–72 bounds; selected text has a different editing range, so do not impose a global numeric range. Tool resets affect the active tool's in-memory defaults only, not annotations or document history.

Selected properties have distinct Details, Appearance, Text/measurement and Layout groups. Working generic edits include lock, supported stroke/text colour, overall opacity, stroke width, fill colour, supported typography fields, bounds and rotation where the model provides it. Family applicability and render/persistence support must be proved individually, especially images/imported annotations.

Lead-verified source findings:

- RightSidebar.tsx:13–33 resolves selectedMarkupIds[0]; the handler changes that focused object, not all selected objects. There is no aggregate mixed-value editor. Preserve primary-selection semantics unless separately approved.
- ToolPropertiesPanel.tsx:402–448 handles lock, stroke/text colour, fill colour, overall opacity, line width, a subset of typography, supported rotation and geometry; unsupported keys return the unchanged markup.
- packages/core/src/document.ts:99–131 has stroke colour/width, fill colour, text colour/font/size/line-height/alignment/inset, overall opacity and blend mode. It has no line-style, hatch or separate fill-opacity fields. Those visible Electron controls are not working migration features.
- The worker also found unwired emphasis/script, vertical alignment, automatic sizing and selected measurement configuration. These require individual implementation-scope decisions, not silent claims of parity.
- state/viewerStore.ts:264–296 records a history entry for each changing updateDocument call. The properties panel does not use the controls' separate onCommit callback to group slider edits. Live feedback and undo grouping are separate contracts; do not assume Electron already groups gestures.

Approved decision: migrate working properties and omit/backlog inert Electron controls. Preserve already-working native capabilities; do not remove them merely because the Electron control is inert. No new persistence schema or bulk-edit semantics is implied. Implementation is in progress, with lead ownership of workspace routing, defaults UI, integration and screenshots; Sol workers own selected-inspector/shared-control consistency and adapter/default-model wiring separately. No acceptance is claimed until family wiring and visual/interaction checks pass.

### Toolkit checkpoint coverage and gap inventory

The shared toolkit composes stock panel/header, numeric Input without steppers, slider/input with live number feedback, colour picker and sections. Existing selected editors use it, and defaults have a single applicability/range model owned by the session adapter. Stock fill-picker alpha maps to the already-working native fill-opacity capability rather than being ignored. All three implementation workers were runtime-confirmed Sol/high.

The final source/overlay audit found selected-object picker alpha still being discarded by six legacy editor families (Ink already handled alpha). This is an in-scope defect, not a native model gap. Pasteur `01a09aff-2aca-7a83-b435-6eb067db4179` (runtime-confirmed Sol/high) supplied the inspector callbacks and atomic workspace patches, then was stopped at the code checkpoint; the lead reviewed and completed integration. All workers are closed. Colour and alpha now commit together without two stale-revision events or two undo entries. Typed combined patches preserve unrelated imported appearance fields. A shared precision-safe opacity helper prevents f64-to-f32 picker synchronisation from creating a precision-only undo edit, including Ink. Existing RGBA tests had explicitly expected discarded alpha; their expectations now verify the working control contract rather than being weakened. The new rectangle/ellipse semantic-picker regression passes colour/fill alpha, primary-only mutation, atomic undo/redo and canonical-value no-op checks. Final captures must use the rebuilt correction, not the earlier candidate bundle. Hubble returned ownership after porting all ten existing inspector entry paths; the lead owns the final coordinated test rerun.

At this toolkit checkpoint, native model gaps were distinct from inert Electron controls: Length had no appearance model; Image had no opacity model; and Polylength/Area had no text-style model. Those working Electron fields could not be claimed migrated by hiding them. The follow-on parity slice recorded below deliberately closes this inventory through the model, rendering, controls and persistence paths. Primary-selection editing continues to use identity-checked additive adapter APIs, without temporary narrowing or bulk changes.

Toolkit-checkpoint checks: pnpm check passed (1088 main tests and 23 relay tests), including the final rerun after the alpha correction. The full native library run then passed all 129 tests, including creation/preview parity, precision-safe opacity conversion and Dimension line/text alpha coherence. Four universal workspace tests passed for defaults scope/validation/history, live slider values/one-sidebar geometry across ten tool families, primary-only selected edits, and rectangle/ellipse picker alpha with atomic undo. The inspector filter passed 11 tests; two real-PDF save/reopen tests remained ignored at this checkpoint. The accepted Highlight real-control/history test passed. Native application (20) and viewer toolbar state (5) tests passed with the canonical sidebar expectation. The newer focused and all-target results are recorded below.

Prior alpha-correction executable SHA-256: 49deb7048b169c6bdb817dcb99149ad68b6d481182e4695041a5a90d8fedce46. This identifies the archived toolkit baseline, not the newer column/slider build recorded below. Visual evidence must verify executable identity at launch.

The latest Mac app builds. Area/Polylength now compose their appearance and measurement controls under one panel and scroll owner; the measurement regression passes caption edits, scale-dialog opening, undo/redo, primary-only multi-selection and locked/saving guards, but its selected-state visual review remains open. Interim Computer Use checks verified colour selection, typed width, invalid-value recovery, creation using defaults, primary selected-rectangle routing, minimum-width resizing, and Cancel/Reset without changing an existing rectangle. No PDF fixture was saved. candidate-interim-rectangle.jpg remains interim evidence, not a final after capture.

### Unlocked Mac visual review

Computer Use captured the verified executable above in a fresh isolated candidate, 1152×768, light system appearance, multi-page.pdf. The Electron reference was rebuilt from the unchanged main@9e947ae4 dirty checkout with isolated test data and light appearance. Existing before captures are dark; this mismatch is explicitly captioned, not colour-parity evidence. Both apps contain only unsaved disposable test rectangles; no fixture was saved.

Main-checkout evidence under test-results/parallel-pilot/properties-toolkit/:

- final-defaults-comparison.png: Electron reference / archived GPUI actions-stack before / latest GPUI Rectangle defaults.
- final-selected-comparison.png: Electron selected Rectangle / archived two-pane GPUI before / latest single-sidebar selected Rectangle. Wider contextual crops preserve the whole old extra pane.
- final-family-sheet-1.png, final-family-sheet-2.png, final-family-sheet-3.png: 19 current tool-default/empty states. These are visual inventory, not proof of every family's functional or selected-state parity.
- candidate-final-selected-alpha-50.jpg and candidate-final-selected-layout.jpg: selected opacity after a live HSLA alpha click, and the lower layout fields.
- candidate-final-minimum-width.jpg and candidate-final-maximum-width.jpg: real minimum/maximum resize captures. Fit zoom followed 110% → 120% → 92% → 110% on restoration. A few initial drags missed the narrow divider; only successful captures count.

Live verified: standard colour popup opens with native accessibility active; HSLA alpha click changes selected opacity to 50%; Undo restores 100%; selected Rectangle uses one sidebar; numeric input/slider spacing remains clear at minimum width; maximum width and restoration preserve rail adjacency. The exact comparison boards and family sheets were reopened and inspected independently of the implementation description.

The user initially said “looks good enough”, then explicitly identified the missing two-column composition and sliders. That clarification supersedes the earlier interpretation that single-column density was accepted polish. Paired groups and working reference sliders are required with stock controls; do not add sliders to font-size fields where the reference uses a numeric control. The initial review also showed larger labels and six-decimal geometry; the compact geometry correction below preserves stored values and history. Native capability gaps, unreviewed selected states and unrun save/reopen checks remain open.

### Column and slider correction

Implemented with standard GPUI grid/Field composition and stock Slider/Input controls; no library fork or custom primitive. Defaults use a two-column grid with full-width slider rows. Rectangle/Ellipse pair line style with fill, X with Y, and width with height. Selected Text Box and Dimension pair text colour with font size. Added selected line-width sliders to Rectangle/Ellipse, Line/Arrow, Ink, vertex paths, engineering visuals and Dimension; Rectangle/Ellipse also regain rotation sliders. Other geometry/model gaps listed above are not newly implemented. Existing native numeric validity ranges are retained rather than silently changing domain support to match every Electron endpoint.

Shared slider subscriptions update the number on Change and use the existing guarded document commit on Release. The live minimum-width review exposed clipped six-decimal geometry text; Rectangle/Ellipse geometry now displays up to two decimals while unchanged display-string commits preserve the full stored f64 and create no undo entry. Repeated labels were removed from compact disabled colour controls.

Final verification:

- Native library: 129 passed. Universal workspace: 5 passed, including actual Rectangle/Ellipse column bounds at 240/300/420, live intermediate slider values, primary-only commits, one undo per release, undo restoration and unchanged compact-value blur. Existing inspector regressions: 11 passed, two real-PDF save/reopen tests remain ignored. pnpm check: hygiene, typechecks, 1088 main tests and 23 relay tests passed. git diff --check passed. No full native-suite or save/reopen acceptance claim.
- Active Mac bundle executable SHA-256: c866447b33465b18024d88a73004295db4541b91caba5ca3041afa86c689a91c. Launched with isolated disposable data. Final source is not the earlier temporary re-signed candidates; those showed stale captures until native redraw and were closed without saving their test rectangles. The active bundle updates normally through Computer Use. Root cause of the temporary-candidate redraw issue is unproven.
- Final live checks: selected width 1 → 10.25 changes the number and painted stroke, Undo → 1; rotation 0 → 182 changes the number and shape, Undo → 0. Real minimum/maximum/restored sidebar captures keep paired values readable and the rail attached. Fit zoom follows 110/120/92/110. A few divider drags missed or reached the opposite endpoint; only observed endpoint captures count. Continuous intermediate drag agreement is proved by native tests, not inferred from endpoint screenshots.
- Main-checkout evidence under test-results/parallel-pilot/properties-toolkit/: columns-appearance-comparison.png and columns-layout-comparison.png show Electron/reference, prior GPUI/before and verified GPUI/after, all light, 1152×768, with padded region crops. Reference and target use unsaved rectangles in the same fixture, with different actual geometry due to canvas fit; this is control/layout evidence, not identical annotation-coordinate evidence. columns-verified-minimum.jpg, columns-verified-maximum.jpg and columns-verified-text-defaults.jpg provide supporting views. The exact comparison boards were reopened and inspected after the final code change. No fixture was saved.

The current correction is ready for user review, not blanket acceptance of every selected family or remaining native capability. The active app is left on the selected Rectangle at restored default sidebar width.

### Phase 2 Rectangle qualification

Rectangle now passes the bounded real-PDF inspector journey through actual workspace creation, supported appearance/layout/lock edits, Save As, qpdf/pdfinfo checks, independent PDFium pixel distinction, clean close and fresh-workspace reopen with stable identity, appearance and geometry. The passing receipt is `.prepared/evidence/button-probe-20260914T110707Z-130045.*`; the real shell journey is recorded at `.prepared/evidence/button-probe-20260914T110717Z-130378.*`. The ordinary rendered-inspector regression now also clicks Fill off and on and proves that both changes travel through document history rather than testing only direct event application.

The live Linux-orb reader separately created and selected a Rectangle with actual pointer input, enabled white Fill in the rendered inspector, saved an owned public-fixture copy, closed, and freshly reopened it. The clean reopened tab, red Rectangle, selected handles, and enabled Fill toggle were observed together; the saved PDF contains `/IC`, and qpdf reported no syntax or stream errors. White fill intentionally blends into the white page, so toggle state and the persisted `/IC` entry—not a colour contrast claim—establish that property. This qualifies Rectangle on the development Linux path only; broader family audit and Mac production qualification remain open. Ellipse is next.

### Phase 2 remaining annotation-tool slices

The remaining family traversal is complete on the Linux development path. Final guarded modes cover shared Rectangle/Ellipse, Line/Arrow, Pen/Highlight, Text Box, Polyline/Polygon, Length/Polylength/Area, Cloud, Callout, Cloud+, Dimension, Arc, semantic snapping, Redact, Snapshot, Image and Signature. The journeys verify the controls and properties each native family actually owns, real pointer or keyboard input, single-selection identity, history, lock suppression where supported, save, typed and fresh-workspace reopen, qpdf/pdfinfo integrity, PDFium pixels where meaningful, and cleanup. Passing `.prepared/evidence/button-probe-20260914T13*.summary.json` receipts in this checkout bind the final sources, pinned PDFium inputs and exact executables; the complete command-level result is reported at handoff.

Snapshot's formerly incomplete path now passes capture, move/resize, 30-degree rotation, double-click reset, Undo, 45% opacity, lock/no-op edits, Save As, fresh reopen and deletion of its Form/Image/SMask graph. The persistence comparison permits the same `0.0001` PDF-number rounding already used for other pointer geometry while rejecting changes outside it. Length calibration fields use a narrower `0.000001` tolerance; equivalent page-scale normalization is checked by effective scale and caption at the fresh application boundary while the independent typed persistence check still verifies the stored calibration.

The declared appearance gaps are now closed deliberately rather than hidden. Length owns line and text appearance with the shared Dimension controls (without the inapplicable offset), Polylength and Area own caption text style alongside their path and calibration controls, and Image/Signature own opacity through the shared visual inspector. Defaults flow into creation and previews; identity/revision-checked edits are atomic and undoable; canvas rendering consumes the stored values; and PDF dictionaries/appearance streams round-trip them with backward-compatible defaults. The guarded real-PDF receipts are `.prepared/evidence/button-probe-20260914T210753Z-370238.summary.json` (Polylength/Area), `.prepared/evidence/button-probe-20260914T211426Z-391476.summary.json` (Length), `.prepared/evidence/button-probe-20260914T211703Z-399771.summary.json` (Image) and `.prepared/evidence/button-probe-20260914T211640Z-399323.summary.json` (Signature). The Length journey caught and fixed dashed/dotted border-style loss on typed reopen. The guarded all-target receipt `.prepared/evidence/button-probe-20260914T212019Z-408689.summary.json` records 131 passing library tests, including the new atomic parity edit test, and the unchanged standalone-tab result of 18 passes plus six obsolete template-geometry failures. The Linux OpenGL fallback remains development-only, and the completed matrix still needs Mac production qualification.
