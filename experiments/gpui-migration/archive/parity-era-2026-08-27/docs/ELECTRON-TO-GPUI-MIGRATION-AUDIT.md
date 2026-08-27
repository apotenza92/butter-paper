# Electron-to-GPUI migration audit

> Governance reset, 2026-08-24: Longbridge GPUI Component revision
> `c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4` is now the default component
> system. The raw-GPUI component recommendations below describe the existing
> experiment and are superseded where they conflict with
> [`MIGRATION-STACK-AUDIT.md`](MIGRATION-STACK-AUDIT.md) and
> [`COMPONENT-PARITY-LEDGER.md`](COMPONENT-PARITY-LEDGER.md). The feature
> inventory remains the Electron contract source.

This document maps the maintained Electron application at commit
`2dc72b86f3e02a618412dd1fed5a91e0676b283d` to the isolated direct-GPUI
experiment. It describes product behavior, module seams, dependencies,
verification, and a dependency-ordered migration sequence. It is not a worklog
or a claim that the listed behavior already exists in GPUI.

## Executive conclusion

The direct-GPUI experiment now contains a comparison-candidate editor slice,
not only a viewer and owned control layer. It is still not a complete Butter
Paper migration or a distributable application.

The experiment currently proves:

- a native GPUI process and window on macOS and NVIDIA/X11 Linux;
- owned button, button-group, split-button, separator, menu, icon, and light
  theme controls;
- opening PDFs through a native file dialog or command-line path;
- page geometry and rasterization through an isolated, long-lived PDFium worker;
- bounded BGRA mapped surfaces converted directly to GPUI `RenderImage` values,
  with no Poppler, PNG, or base64 application hot path;
- multiple selectable and closable tabs;
- thumbnail and page-surface queues;
- continuous and single-page viewing;
- page selection, zoom, Fit Width, Fit Page, and wheel-mode preferences;
- three hard-coded one-page blank PDF sizes;
- correct mixed-page single/continuous layout, bounded high-zoom tile plans,
  stale-result rejection, cancellation, and declared cache limits;
- visible Rectangle, streamed Highlight, Text Box, Length, and Image workflows
  backed by one typed document and command model;
- shared selection, hit testing, lock/delete, history, dirty state, document
  scenes, and thumbnail scenes for the representative annotation families;
- native `/Square`, `/Ink`, `/FreeText`, measured `/Line`, and image-XObject PDF
  mappings with appearance streams, semantic reimport, and two save/reopen
  cycles in the focused compatibility fixture;
- exact preservation probes for an untouched unknown annotation plus original
  page content, boxes, and metadata; and
- versioned comparison manifests, semantic diagnostic scenarios, fixture
  oracles, and statistical decision policy.

The implemented representative families cover the main input, vector, text,
measurement, bitmap, thumbnail, PDF-write, CPU, memory, and GPU workload
classes needed to prepare an investment comparison. The final v5 boundary adds
a four-document session, a native property edit with exact undo, the maintained
inclusive 8 CSS-pixel per-axis snap transform, and dynamic fidelity throughout
the native scroll trajectory. It does not reproduce the complete maintained
document model, all annotation tools, multi-selection, templates, signatures,
full save/source replacement policy, accessibility, updater, packaging, or
platform integration.

The comparison is not live-qualified or decision-complete. The v5 workload,
runner adapters, paired orchestrator, hard-report validator, and paired analyzer
exist, but a real Linux GPU GUI run must still prove the native X11/XTest target,
geometry, timestamps, milestones, output, and complete paired evidence. GPUI
The maintained GPUI Component workspace now precomposes committed Highlight
bodies with deterministic CPU Multiply on annotation-free raster bases. Its
CropBox/rotation mapping and fresh native visual oracle remain incomplete.
Text persistence uses basic Helvetica rather than complete font
shaping, fallback, embedding, and cross-viewer parity. The inherited PDF source
descriptor works on Unix; the equivalent Windows handle transfer is not
implemented.

The migration must therefore preserve product behavior across four distinct
areas:

1. Pure document and annotation behavior.
2. PDF inspection, rasterization, geometry, and safe publication.
3. Native interaction and visual presentation.
4. Desktop operating-system, packaging, update, and security behavior.

Replacing React controls without those areas would produce a native viewer,
not Butter Paper.

## Audit basis

The review inspected these current sources:

- `packages/core/src`: the document, markup, appearance, selection, scale,
  transform, font, and signature value model.
- `packages/pdf/src`: blank-PDF generation, PDF.js rasterization, native PDF
  annotation import/export, geometry extraction, caching, and canvas adapters.
- `apps/desktop/src/renderer/src/app.tsx`: workspace orchestration, tabs,
  saving, menu commands, editing commands, templates, and signatures.
- `apps/desktop/src/renderer/src/pdf-tools`: the tool registry and all tool
  geometry, rendering, selection, interaction, properties, and PDF mappings.
- `apps/desktop/src/renderer/src/components`: the maintained Nova shell,
  viewport, annotation layer, templates, properties, signatures, scale, rails,
  tabs, menus, dialogs, and responsive behavior.
- `apps/desktop/src/renderer/src/services`: document sessions, PDF adapters,
  render scheduling, caches, and performance instrumentation.
- `apps/desktop/src/renderer/src/state`: document history, dirty state, viewer
  state, snapping preferences, and tool defaults.
- `apps/desktop/src/main`, `src/preload`, and `src/shared`: privileged file and
  PDF operations, capability handles, window lifecycle, application menus,
  signatures, templates, updates, and the 57-channel Electron bridge.
- `apps/desktop/electron-builder.config.cjs`: stable/beta product identities,
  signing, file associations, platform packages, and update metadata.
- deterministic tests and the Playwright Electron workflows.
- `experiments/gpui-migration/gpui-gallery/src`: the current native spike.

The file and test counts recorded in the first audit are historical because the
experiment has expanded substantially. Current evidence comes from the checked
source and focused annotation-model, annotation-adapter, viewer, PDF-worker,
PDF-persistence, fixture, workload, runner, and decision-policy tests. Do not
use the old counts as a current gate result.

## Status vocabulary

- **Implemented**: the GPUI implementation performs the observable product
  behavior and has relevant deterministic evidence.
- **Partial**: some behavior exists, but the maintained product contract is not
  complete.
- **Visual only**: a control or state is drawn but does not perform the product
  operation.
- **Missing**: no GPUI implementation exists.
- **Decision gate**: implementation should not start until a foundation choice
  is accepted.
- **Blocked evidence**: implementation may exist, but required evidence is not
  available in this checkout.

The status tables describe full maintained-product parity. A **Partial** row can
contain a complete representative comparison slice while still lacking the
other variants required for product migration.

## Comparison candidate versus full migration

The technical investment comparison needs representative work from every
expensive class, not every product variation. Its inherited v4 editor set is
Rectangle, Highlight/Freehand, Text Box, Length Measurement, Image, and one
untouched unknown annotation. Its viewer set covers app-cold open, mixed-page
single/continuous layout, virtualization, thumbnails, navigation, fit, pan,
bounded zoom through 1600%, cancellation, stale rejection, cache pressure,
close, and reopen. Its persistence set requires native dictionaries and
appearances, independent validation, original-document preservation, and two
save/reopen cycles.

The final v5 additions are a one-process four-document session, a native
Rectangle property commit and exact one-step undo, a native snap-enabled
transform, and fidelity observations throughout continuous scrolling. The snap
contract retains the maintained inclusive 8 CSS-pixel threshold on each axis as
an L-infinity test and derives its point-space threshold from the observed scale
at run time. Dynamic fidelity retains the exact 120 Hz native trajectory, an
independent 60 Hz observer, and three registered crops.

This boundary deliberately defers work that does not add a distinct performance
inference:

- Templates are a separate library, import, preview, migration, and storage
  subsystem.
- Remaining annotation variants still require product-parity work, but the five
  families already cover the distinct vector, streamed freehand, text,
  measurement, bitmap, history, and persistence cost classes.
- Signatures add sanitization, secure storage, privacy, image-processing, and
  phone-transfer risks. Image annotation evidence does not prove signatures.
- Packaging, signing, and updater work qualifies distributable and installed
  candidates, release trust, and N-1 replacement rather than an unpackaged
  development runtime.
- Full accessibility requires complete keyboard, focus, semantic-tree,
  screen-reader, input-method, and constrained-window evidence.
- macOS and Windows have different render, font, input, accessibility, package,
  and platform-integration behavior. A Linux GPU run cannot qualify either
  operating system.
- The 180 MB USGS fixture is a non-inferential robustness stress case. Its
  large-sheet workload is useful for diagnosis but is not the representative
  fixture population.
- The private Hibbeler corpus is optional supplementary evidence and remains
  blocked/not transferred. It cannot gate or influence the paid Linux result.

All deferred work remains mandatory for a full product migration. A favorable
v5 result supports an investment decision only; it does not establish feature,
platform, accessibility, packaging, updater, signing, or release parity.

Both implementations must consume the same locked manifest. Direct semantic
commands remain diagnostic only. No timing becomes decision-eligible until the
implemented native-input lane proves target selection, page geometry, event
schedule, milestones, and final state on the actual Linux GPU GUI and emits a
complete hard-valid report bundle.

## Maintained application map

```text
Operating system and package integration
  └─ Electron main process
       ├─ window, menu, open-file, close, theme and updater lifecycle
       ├─ capability-scoped source and save paths
       ├─ PDF load, geometry and safe publication
       ├─ template and recent-signature stores
       └─ phone-signature transfer
            ↓ typed preload bridge
React workspace
  ├─ tabs and per-document LocalPdfSession instances
  ├─ viewer state, document history and editing commands
  ├─ render coordinator, caches and adaptive performance
  ├─ viewport, page views, thumbnails and annotation layers
  ├─ annotation tool registry
  └─ Nova shell, dialogs, menus and property controls
            ↓
Shared product modules
  ├─ @butter-paper/core: document and annotation semantics
  └─ @butter-paper/pdf: PDF inspection, rendering and writing
```

The Electron split is partly an implementation constraint. A native application
does not need a preload bridge, but it still needs the same privilege separation:
untrusted or user-controlled paths must not bypass validated open and save
operations, and UI code must not implement publication safety itself.

## Exact feature-parity map

### 1. Application lifecycle and host integration

| Maintained behavior            | Current implementation                                                                                   | GPUI status | Migration requirement                                                                           |
| ------------------------------ | -------------------------------------------------------------------------------------------------------- | ----------- | ----------------------------------------------------------------------------------------------- |
| Stable and beta identities     | Distinct product names, package names, application IDs, icons, data directories, feeds, and artifacts    | Missing     | Preserve distinct identities in every native package and runtime path.                          |
| Window creation and recovery   | Hidden-until-ready window, failure page, renderer-crash handling, background color, constrained geometry | Partial     | GPUI opens a window, but recovery and product window policy are missing.                        |
| Window-state persistence       | Validated and clamped size/position with atomic persistence                                              | Missing     | Add native per-product state with multi-display validation.                                     |
| macOS full screen              | Native state notifications and layout changes                                                            | Missing     | Implement and test native full-screen transitions without duplicating title chrome.             |
| Native/open-file launches      | macOS `open-file`, Windows/Linux argv, second-instance routing, focused-window reveal                    | Partial     | Initial command-line paths work; OS events, second-instance behavior, and focus routing do not. |
| PDF file associations          | macOS, Windows, and Linux package registration                                                           | Missing     | Add native package declarations and runtime validation.                                         |
| Default PDF application        | Platform-specific request and confirmation behavior                                                      | Missing     | Replace Electron integration with native platform adapters.                                     |
| Application and document menus | Native application menu plus in-window Nova menu, enablement state, accelerators                         | Visual only | GPUI draws menu labels but has no complete command/menu state model.                            |
| Close and quit protection      | Dirty-tab and dirty-application confirmation, save/discard/cancel ordering                               | Missing     | Closing must cross the workspace interface and cannot bypass unsaved state.                     |
| Theme                          | Native theme snapshot/change events and Nova light/dark tokens                                           | Partial     | The GPUI spike has only a fixed light token set.                                                |
| Drag-and-drop PDF opening      | Path extraction, main-process authorization, duplicate handling                                          | Missing     | Add validated native drop paths and duplicate-tab behavior.                                     |
| Camera permission policy       | Camera access restricted to the approved signature flow and renderer origin                              | Missing     | Recreate only if the native signature workflow still needs camera access.                       |
| Diagnostics                    | Startup milestones, per-process metrics, system resources and display refresh                            | Partial     | GPUI emits scenario events but lacks maintained diagnostics breadth.                            |
| Test hooks                     | Disposable user data, fixture resolution, window control, metrics and startup inspection                 | Partial     | Keep test-only hooks out of release builds and expose equivalent native fixtures.               |

### 2. Document workspace and commands

| Maintained behavior                     | GPUI status | Missing contract                                                                                                                                                                           |
| --------------------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Open one or many PDFs                   | Partial     | Multi-select dialog, system-open batching, progress, storage-source labels, authorization, duplicate focus, and failure isolation.                                                         |
| Multiple document tabs                  | Partial     | Dirty indicators, reorder by pointer and keyboard, focus restoration, accessible tab semantics, horizontal overflow, and close confirmation.                                               |
| Per-tab sessions                        | Partial     | GPUI keeps per-document PDF and annotation state and rejects stale render generations, but still lacks the complete workspace/session resource policy.                                     |
| Dirty state                             | Partial     | Representative annotation commands use saved revisions and shared dirty state; complete workspace/menu/close integration remains.                                                          |
| Undo and redo                           | Partial     | Representative annotations share per-document undo/redo history; bounded product policy, every command, shortcuts, and menu enablement remain.                                             |
| Cut, copy, paste, delete and select all | Partial     | Lock-aware delete exists for the representative set; clipboard, select-all, paste offsets, and full multi-selection remain missing.                                                        |
| Save                                    | Partial     | The focused adapter serializes representative native annotations and survives two reopen cycles, but source replacement, page rotation/scale, and complete product compatibility remain.   |
| Save As                                 | Partial     | The focused adapter syncs a same-directory temporary file and refuses an existing output path; capability scope, full revalidation, source replacement, and post-save tab behavior remain. |
| Page rotation                           | Missing     | Per-page left/right rotation, layout refresh, cache invalidation, history, save and reopen.                                                                                                |
| Status and error feedback               | Partial     | GPUI reports some document errors but lacks the maintained status lifecycle and action-specific recovery.                                                                                  |
| Keyboard navigation                     | Partial     | Open, page, zoom, and fit shortcuts exist; editing, tabs, menus, close, undo/redo, copy/paste and tool shortcuts do not.                                                                   |

### 3. PDF engine and render pipeline

| Maintained behavior     | Current Electron implementation                                                                        | GPUI status | Migration requirement                                                                                                                                                                                                  |
| ----------------------- | ------------------------------------------------------------------------------------------------------ | ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Source validation       | Capability registry owned by the privileged process                                                    | Partial     | The PDFium worker receives a read-only inherited descriptor on Unix. Windows handle inheritance, owner-scoped capabilities, revalidation, and hostile-input policy remain.                                             |
| Open progress           | Streaming reads with bytes, source name, phase and estimated time                                      | Missing     | Provide observable progress without blocking the GPUI event loop.                                                                                                                                                      |
| Metadata and page model | PDF.js/pdf-lib inspection including boxes, rotation and `/UserUnit`                                    | Partial     | PDFium supplies page geometry and display sizes; the complete maintained metadata, malformed-document, password, box, rotation, and `/UserUnit` contract remains.                                                      |
| Annotation import       | 21 markup variants, source IDs, metadata and untouched fingerprints                                    | Partial     | Rectangle, Highlight, Text Box, Length, and Image have native import mappings; untouched unknown annotations are preserved exactly in the focused fixture. The other variants and general vendor compatibility remain. |
| Page rasterization      | PDF.js canvas/bitmap/blob surfaces                                                                     | Partial     | The isolated PDFium worker writes bounded BGRA mapped surfaces consumed directly as GPUI `RenderImage` values. Production PDFium supply, sandboxing, and six-target qualification remain.                              |
| Cancellation            | Abortable render tasks and stale-result rejection                                                      | Partial     | Worker job cancellation, generation rejection, and cleanup exist; full scheduling, failure recovery, and native stress evidence remain.                                                                                |
| Render scheduling       | Priority classes, promotion, adoption, visible/prefetch budgets and adaptive concurrency               | Partial     | Viewer plans bound visible tiles and reject stale generations, but maintained promotion/adoption and adaptive concurrency policy remain.                                                                               |
| Page caches             | Entry/byte limits, reference counting, retired surfaces, previews and deep-zoom crops                  | Partial     | Bounded tile and `RenderImage` caches exist; product preview/full/detail lifecycle and complete resource accounting remain.                                                                                            |
| Thumbnails              | Virtualized, prioritized, cached, annotation-aware previews                                            | Partial     | GPUI virtualizes thumbnails and paints the shared annotation thumbnail scene; complete prioritization, quality, and accessibility policy remain.                                                                       |
| Continuous pages        | Visible-range calculation, placeholder/preview/full/detail quality progression                         | Partial     | Basic continuous page surfaces exist; staged quality and motion policy do not.                                                                                                                                         |
| Single page             | Page switching, wheel threshold and thumbnail synchronization                                          | Partial     | Basic behavior exists and has deterministic state tests.                                                                                                                                                               |
| Deep zoom               | High-quality and target-crop renders up to the product zoom limit                                      | Partial     | A deterministic 1024-pixel tile plan remains bounded through 1600%; density, visual quality, native presentation, cancellation stress, and preview/full/detail transitions still need real GPU evidence.               |
| Geometry index          | PDF content paths, generated page grids and per-page caching                                           | Missing     | Required for content and grid snapping.                                                                                                                                                                                |
| PDF writing             | Native annotations, appearance streams, fonts, images, source reconciliation, scale data and rotations | Partial     | `lopdf` writes and reimports the five representative native mappings with appearances and two reopen cycles. Full fonts, every tool, rotation, source reconciliation, vendor compatibility, and hostile inputs remain. |
| Safe publication        | Revalidation, symlink rejection, source hash checks, synced temporary output and atomic publication    | Partial     | The focused adapter syncs a same-directory temporary output and refuses overwrite. Full source/hash/symlink/capability checks and atomic source replacement remain.                                                    |
| Blank PDF generation    | A0–A5/custom paper plus five patterned paper types and page-grid metadata                              | Partial     | GPUI only writes three fixed blank page sizes without patterns or grid metadata.                                                                                                                                       |

#### PDF foundation decision gate

The application render path now uses the isolated PDFium worker; Poppler remains
only an independent `pdfinfo` test validator. The worker boundary, clipped
rendering, cancellation, bounded shared surfaces, direct `RenderImage` adapter,
and exact development artifact pins are implemented and deterministic on Linux.
This is a comparison-candidate foundation, not acceptance of a production
PDFium supply chain. The final decision must still cover:

- licensing and redistributability;
- macOS ARM64/x64, Windows ARM64/x64, and Linux ARM64/x64 packaging;
- page boxes, rotation, `/UserUnit`, metadata and malformed-PDF behavior;
- annotation import and preservation;
- raster cancellation, concurrency and deep zoom;
- content geometry extraction;
- font and image embedding;
- appearance-stream compatibility with Adobe, Preview and Bluebeam;
- deterministic save/reopen and failure-safe publication; and
- performance on the public corpus and Hibbeler corpus.

The current UI consumes Butter Paper worker and render-image types rather than
Poppler-specific paths. Keep PDFium types isolated in the worker and replace the
development supplier binary with an audited application-owned build before
distribution.

### 4. Viewer and shell behavior

| Feature                         | GPUI status | Remaining behavior                                                                                                                                                                                                                                 |
| ------------------------------- | ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Nova visual tokens              | Partial     | Light colors and several dimensions exist; dark mode, complete typography, elevation, motion and state atlas do not.                                                                                                                               |
| Button family                   | Partial     | Basic variants, sizes, toggles, disabled state and focus exist; matched hover/focus/accessibility/constrained evidence remains.                                                                                                                    |
| Split buttons                   | Partial     | Primary/menu separation and two shell consumers exist; placement, overflow, dismissal and full keyboard contracts need platform evidence.                                                                                                          |
| Menus/popups                    | Partial     | Small radio-style menus exist; nested menus, typeahead, focus return, collision handling and full application menus do not.                                                                                                                        |
| Inputs/selects/sliders/switches | Missing     | Required by zoom, properties, templates, scale, snap and update settings.                                                                                                                                                                          |
| Dialogs/popovers/tooltips       | Missing     | Required by close confirmation, templates, page scale, signatures, updates and destructive actions.                                                                                                                                                |
| Window title and menu bars      | Visual only | Product/platform visibility rules, actions and native integration are missing.                                                                                                                                                                     |
| Tab bar                         | Partial     | Select, close, open, and new-blank controls exist; dirty, reorder, overflow, confirmation and accessibility remain.                                                                                                                                |
| Viewer toolbar                  | Partial     | Zoom, fit, view mode and wheel preferences exist; direct zoom input, columns/CAD layout, hints, tooltips and responsive overflow remain.                                                                                                           |
| Left rail/sidebar               | Partial     | Page thumbnails and show/hide state exist; resize, constrained overflow, accessibility and exact shell behavior remain.                                                                                                                            |
| Right rail                      | Partial     | Rectangle, Highlight, Text Box, Length, and Image select real typed tools; shared undo/redo, lock/delete, Rectangle grid snap, and representative properties are visible. The other tools, signature, page scale, and complete mutation-disabled behavior remain. |
| Properties sidebar              | Partial     | Rectangle stroke width, Rectangle grid snap, Text and Image edits, plus shared selection actions exist for the representative slice; the maintained property matrix and tool defaults remain incomplete.                                           |
| Custom two-axis scrolling       | Missing     | Product scrollbar geometry, drag math, nested wheel handling and constrained behavior are absent.                                                                                                                                                  |
| Page columns/CAD overview       | Missing     | Columns/rows organization, pages-per-column and overview rendering are absent.                                                                                                                                                                     |
| Opening and error overlays      | Partial     | Simple errors exist; progress, placeholders and recovery do not.                                                                                                                                                                                   |
| Responsive/constrained layout   | Missing     | The native shell has not passed the maintained 800×600 and overflow contracts.                                                                                                                                                                     |
| Accessibility                   | Partial     | Some controls have focus and labels; roles, relationships, announcements, keyboard ordering and native accessibility inspection are incomplete.                                                                                                    |

### 5. Annotation engine

The maintained registry contains two navigation tools, 20 creation tools, and
one non-rail imported-annotation fallback. Every creation tool has a combination
of geometry, hit testing, render primitives, selection chrome, interaction,
property defaults, and PDF mapping. GPUI now implements one shared engine for
the five comparison representatives. The table still measures full family
parity.

| Tool family       | Maintained tools         | Important behavior to migrate                                                                                | GPUI status                                                                                                                                                                                            |
| ----------------- | ------------------------ | ------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Navigation        | Select, Pan              | Hover/focus, shift selection, marquee, moving, Space-pan, cursors                                            | Partial: representative hit selection exists; full Select/Pan contract remains.                                                                                                                        |
| Rectangular       | Rectangle, Ellipse       | Click-or-drag placement, fill/stroke, eight resize handles, rotation, locked state                           | Partial: Rectangle and Ellipse have real gesture, scene, hit, selection, shared actions, thumbnail, and PDF mapping. Ellipse additionally proves the exact three-pixel threshold, curve-correct handles, move/resize preview, rotation reset, and checksum-pinned real PDFium create/edit/delete Save As/reopen. Complete properties, snapping/group parity, native accessibility/visuals, and packaged platforms remain. |
| Text              | Text Box                 | Text layout, editing, fonts, alignment, resize, rotation and PDF font mapping                                | Partial: real GPUI text input, content/layout edits, scene/thumbnail, Helvetica `/FreeText`, and reopen exist; full shaping, fallback, embedding, rotation, and viewer parity remain.                  |
| Straight paths    | Line, Arrow              | Click-or-drag, endpoints, arrow appearance, move and PDF mapping                                             | Visual only                                                                                                                                                                                            |
| Arc               | Arc                      | Three-point placement, bulge snapping, reshape handles and sampled path                                      | Partial Linux development journey passed: real GPUI Component Button, three-click creation, sampled paint/hit path, retained body/start/mid/end edits, Shift snapping, canonical CircleArc Save As/reopen, changed PDFium pixels, and resource cleanup pass. Complete live pointer editing, properties, third-party compatibility, native visual/accessibility, and packaged platforms remain. |
| Vertex paths      | Polyline, Polygon        | Multi-click draft, completion, vertices, open/closed geometry and fill                                       | Visual only                                                                                                                                                                                            |
| Ink               | Pen, Highlight           | Sample collection, smoothing, opacity/blend mode and multi-path export                                       | Partial: streamed Highlight, coalesced history, Multiply domain/PDF state, `/Ink`, scenes, and reopen exist; GPUI preview uses source alpha and full Pen/multipath behavior remains.                   |
| Cloud             | Cloud                    | Click/drag node paths, intensity, generated scallops, vertices and PDF intent                                | Partial Linux development journey passed: a real GPUI Component tool, click-node creation, ten-screen-pixel closure, deterministic scallops, stable vertex/body edits, lock/history, typed PolygonCloud import and canonical save/reopen, real PDFium pixels, `qpdf`, and worker/resource cleanup exist. Rectangle-drag creation, live workspace pointer editing, exact Electron/Bluebeam cubic scallops, properties, native visual/accessibility, and packaged platforms remain. |
| Compound callouts | Callout, Cloud+          | Text box, routed leader, cloud path, obstacle routing, grouped selection and multi-object PDF reconciliation | Partial Linux development journey passed: Callout has a real GPUI Component tool and Textarea, two-click leader/text-box creation, multiline initial text in one history step, direct text-box/knee edits, typed canonical Save As/reopen, changed PDFium pixels, `qpdf`, and worker/resource cleanup. Existing-object pointer editing, exact text appearance/rich text, complete handles/properties, Cloud+, obstacle routing, native visual/accessibility, and packaged platforms remain. |
| Dimension         | Dimension                | Line offset, caption, endpoints, knee/offset handle and text appearance                                      | Partial Linux development journey passed: real GPUI Component Button/Textarea, two-click creation, caption, retained endpoint/body/offset edit commands, canonical unmeasured LineDimension Save As/reopen, changed PDFium pixels, and resource cleanup pass. Complete live pointer editing, properties, dimension-increment snapping, native visual/accessibility, and packaged platforms remain. |
| Measurement       | Length, Polylength, Area | Page scale, units, precision, captions, multi-point paths and area calculation                               | Partial Linux development journey passed: Length plus independent Polylength/Area creation, completion/cancellation, calibrated captions, vertex/body edits, scene/thumbnail state, canonical native dictionaries and appearances, legacy/current import classification, guarded Save As, real PDFium pixels, fresh-workspace reopen, and resource release exist. Complete style/caption properties, semantic snapping, cross-family duplicate imported-ID rejection, native accessibility/visuals, third-party corpus breadth, and packaged platforms remain.   |
| Media             | Image, Snapshot          | Sanitized asset ingestion or page crop, aspect ratio, resize/rotate, embedding and opacity                   | Partial Linux development journeys passed for both families. Regular Image proves bounded decode, placement, pointer move/free-resize, shared page/thumbnail rendering, DeviceRGB XObject with soft mask, canonical staged reopen, owned-object cleanup, and renderer release. Snapshot now proves exact two-click annotation-free base-raster capture, body move/eight-handle resize, retained rotation/opacity state, canonical StampSnapshot Form/Image/SMask persistence, external/malformed Stamp preservation, real PDFium Save As/fresh reopen/delete, unchanged annotation-disabled pixels, changed annotation-enabled pixels, and worker/surface cleanup. Native picker authority, complete Snapshot rotation/opacity UI, vendor/private payload compatibility, hostile-input isolation, native visual/accessibility, and packaged platforms remain. |
| Redaction mark    | Redact                   | Pending redaction geometry and explicit semantics that page content is not yet removed                       | Partial Linux development journey passed: real GPUI Component tool and warning, retained pointer create/move/eight-handle resize, canonical no-AP pending `/Redact` Save As/reopen/delete, unchanged annotation-disabled PDFium pixels, and worker/resource cleanup pass. Apply Redactions/content destruction, secure sanitization, properties, native visual/accessibility, and packaged platforms remain blocked or incomplete. |
| Imported fallback | Imported Annotation      | Preserve and display unsupported annotations without destructive rewriting                                   | Partial: exact untouched unknown dictionary and appearance preservation passes two saves; general fallback display and arbitrary import remain missing.                                                |

#### Shared annotation behavior and current representative proof

The representative slice now proves a Rust command model with stable IDs,
revision-based dirty state, per-document history, lock-aware delete, typed
gesture lifetimes, type-aware hit testing, shared document/thumbnail scenes,
bounded image assets, ordinary Rectangle grid snapping, and native PDF
mappings. The remaining full-migration requirements include:

- Complete stable serialization and compatibility fixtures for every markup.
- PDF-to-viewport and viewport-to-PDF transforms for rotated boxes and
  `/UserUnit`.
- A complete tool registry for every tool's geometry, scene primitives, hit
  results, selection chrome, states, properties and PDF mapping.
- Complete hovered, focused, multi-selected and grouped interaction states.
- Click, click-or-drag, multi-click and freehand draft lifecycles.
- Z-ordered hit testing with edge, interior, handle, vertex and leader regions.
- Window and crossing selection marquees with replace/add/toggle behavior.
- Group moves, component-specific moves, endpoint moves, resize, rotation,
  vertex reshape, arc reshape and knee/offset changes.
- Lock-aware mutation and destructive commands.
- Post-placement focus and property editing.
- Thumbnail annotation rendering.
- Document history integration.
- Save/reopen verification for every supported kind.

### 6. Snapping, scale and measurement

| Maintained behavior                                          | GPUI status |
| ------------------------------------------------------------ | ----------- |
| Snap to PDF content geometry                                 | Missing     |
| Snap to existing markup geometry                             | Missing     |
| Snap to generated or detected page grid                      | Partial: ordinary Rectangle translation snaps to a configured point grid; detected and template grids remain missing |
| Endpoint, midpoint, center, intersection and nearest targets | Missing     |
| Alignment, equal-size and equal-spacing guides               | Missing     |
| Configurable sensitivity                                     | Partial: the product engine accepts grid spacing and CSS-pixel sensitivity; the current UI exposes one 18 pt / 8 px preset |
| Construction grid visibility and spacing                     | Missing     |
| Dimension increments                                         | Missing     |
| Per-page preset, custom and calibrated scales                | Missing     |
| Two-point visual calibration                                 | Missing     |
| Decimal and fractional precision                             | Missing     |
| Length, polyline length and polygon area conversion          | Missing     |
| Scale and rotation persistence in saved PDFs                 | Missing     |

### 7. Template subsystem

The maintained template subsystem is substantially larger than the GPUI menu.

| Maintained behavior                  | Electron implementation                                             | GPUI status                                        |
| ------------------------------------ | ------------------------------------------------------------------- | -------------------------------------------------- |
| Built-in templates                   | Blank, dots, square grid, ruled, isometric and triangle             | Partial: three blank sizes only                    |
| Paper sizes                          | A0–A5, portrait/landscape and custom 10–5000 mm dimensions          | Partial: Letter portrait/landscape and A4 portrait |
| Pattern settings                     | Type, spacing presets/custom spacing and color presets/custom color | Missing                                            |
| Page-grid metadata                   | Embedded structured subject used by snapping                        | Missing                                            |
| Last-used template                   | Persistent selection and primary split-button action                | Visual selection only; not persistent              |
| Custom generated templates           | Named records with validation and previews                          | Missing                                            |
| Imported PDF templates               | Managed private source copy and index                               | Missing                                            |
| Save document as template            | Imports the active document into the managed library                | Missing                                            |
| Template picker                      | List, selection, preview, summary, create and manage action         | Partial three-item menu only                       |
| Template manager                     | Create, inspect, import, remove and preview                         | Missing                                            |
| Migration from legacy blank settings | Versioned local-storage migration                                   | Missing                                            |
| Safe storage                         | Atomic index and private source files under product user data       | Missing                                            |

### 8. Signature subsystem

| Maintained behavior                                           | GPUI status                                             |
| ------------------------------------------------------------- | ------------------------------------------------------- |
| Typed, drawn, camera/photo and image signature appearances    | Missing                                                 |
| Phone transfer sessions and authenticated encrypted envelopes | Missing                                                 |
| Relay polling, expiry, cancellation and ownership             | Missing                                                 |
| Image sanitization in an isolated runtime                     | Missing; native replacement requires a new threat model |
| Recent-signature encrypted store                              | Missing                                                 |
| Add, remove and clear recent signatures                       | Missing                                                 |
| Signature menu and accessible subflows                        | Missing                                                 |
| Placement through the image tool                              | Missing                                                 |
| PDF image embedding and save/reopen                           | Missing                                                 |

### 9. Update, release and packaging

| Maintained behavior                                     | GPUI status             | Migration requirement                                                            |
| ------------------------------------------------------- | ----------------------- | -------------------------------------------------------------------------------- |
| macOS DMG and ZIP, Windows NSIS, Linux AppImage/DEB/RPM | Missing                 | Select native packaging without changing artifact contracts casually.            |
| ARM64 and x64                                           | Partial build evidence  | Produce and verify every maintained target.                                      |
| Signing and notarization                                | Missing                 | Preserve Developer ID, hardened runtime, Windows signing and offline key policy. |
| Stable/beta isolation                                   | Missing                 | Preserve distinct IDs, names, data, caches and feeds.                            |
| TUF trust on Windows/Linux                              | Missing                 | Port reviewed-root validation and rejection behavior.                            |
| macOS updater                                           | Missing                 | Replace Electron updater while preserving native N-1 requirements.               |
| Update frequency/status UI                              | Missing                 | Port policy, state machine, dialogs and menus.                                   |
| Release asset contracts                                 | Electron-specific today | Rewrite tests around native artifacts before cutover.                            |
| THIRD_PARTY_NOTICES and license gate                    | Decision gate           | Resolve GPUI transitive GPL chain and generate native dependency notices.        |

## Proposed native module design

The migration should produce deep modules with small interfaces. It should not
translate `app.tsx` into one large `Gallery` entity or expose every internal
render and interaction detail to the shell.

### `butter_model`

Pure in-process module. Owns document, page, markup, appearance, scale,
selection, history and command invariants.

Example interface:

```rust
pub struct DocumentEditor { /* private */ }

impl DocumentEditor {
    pub fn apply(&mut self, command: EditCommand) -> Result<EditOutcome, EditError>;
    pub fn snapshot(&self) -> DocumentSnapshot;
    pub fn can_undo(&self) -> bool;
    pub fn can_redo(&self) -> bool;
}
```

The implementation may contain many markup-specific helpers. Callers should
not reconstruct history, locking, selection or dirty-state rules.

### `pdf_engine`

Deep module at a real seam. Production uses the accepted native PDF adapter;
tests use an in-memory deterministic adapter.

The interface should cover open, page information, annotation import, render,
geometry and save. Render scheduling, cache keys, cancellation and source
reconciliation should remain inside the module instead of leaking into GPUI
views.

### `document_workspace`

Owns open tabs, active tab, per-document sessions, reorder, close decisions,
resource activation, duplicate paths and save state. The shell dispatches
workspace actions and reads a compact snapshot.

### `annotation_engine`

Pure module that accepts normalized pointer/key events and produces a document
command plus a scene description. It owns tool registration, draft state, hit
testing, selection, transforms, properties and snapping coordination. GPUI
renders the returned scene and forwards normalized input; it must not contain
markup geometry rules in each view.

### `viewer_session`

Owns viewport layout, visible pages, zoom, fit modes, page mode, scroll intent,
render requests and thumbnail synchronization. The PDF engine remains an
internal dependency. This prevents the shell from becoming a second render
coordinator.

### `template_library`

Owns built-in, custom and imported template records, validation, last-used
selection, storage migration, previews and creation. Its filesystem adapter is
replaceable by a temporary-directory adapter in tests.

### `signature_workflow`

Owns signature appearance values, recent assets, phone-session state,
sanitization and placement output. Network transfer, secure storage and image
decoding are internal seams with production and test adapters.

### Platform modules

Do not create one shallow `PlatformHost` with dozens of pass-through methods.
Use separate deep modules where behavior genuinely varies:

- application lifecycle and open-file routing;
- validated file access and publication;
- menu and command routing;
- product-scoped settings and secure storage;
- updater and release trust; and
- performance/system inspection.

Each module has native macOS, Windows and Linux behavior plus a deterministic
test adapter where appropriate.

### `butter_ui`

Owns ordinary Nova controls and accessibility behavior. It should stay smaller
than the product modules and must not absorb document, PDF or annotation
behavior. Domain views consume `document_workspace`, `viewer_session` and
`annotation_engine` snapshots.

## Dependency-ordered implementation sequence

This is the original full-migration roadmap. Preserve it as planning history.
The later comparison-candidate work intentionally implemented representative
parts of M1 through M6 before completing every earlier release gate. Passing a
representative test below does not mark the corresponding full milestone done.

Each milestone below ends in an observable, releasable-quality behavior. A
milestone is not complete when its toolbar control merely changes color.

### M0 — Foundation decisions

1. Resolve the pinned GPUI source and GPL-transitive dependency gate.
2. Research and accept the production PDF engine and packaging strategy.
3. Define supported target triples and minimum operating-system versions.
4. Define a versioned cross-language fixture format for document models,
   markups, PDF mappings and template records.

Exit gate: exact native dependency graphs pass license, advisory, source and
target compile checks. A representative PDF opens, renders, imports one known
annotation and saves safely on all target families.

### M1 — Rust product model

1. Port points, rectangles, transforms, page boxes and rotations.
2. Port all 21 markup variants and appearance resolution.
3. Port page scales, units, precision and measurement calculations.
4. Port selection, locking, commands, history and dirty revisions.
5. Run shared JSON fixtures through TypeScript and Rust and compare results.

Exit gate: Rust reproduces the maintained pure-domain fixtures without GPUI or
filesystem dependencies.

### M2 — Production document and PDF session

1. Implement validated open paths and progress.
2. Import metadata, pages and annotations into `butter_model`.
3. Implement cancelable rendering, priority scheduling and bounded caches.
4. Implement thumbnails and page surfaces with stale-result rejection.
5. Implement geometry indexing.
6. Implement safe save and Save As with source reconciliation.

Exit gate: open, view, save without changes, and reopen are non-destructive on
the public compatibility corpus. Unknown annotations remain preserved.

### M3 — Viewer parity

1. Move current spike logic into `viewer_session` and split the 2,800-line
   gallery entity into module consumers.
2. Complete continuous, single-page, fit, manual zoom and wheel contracts.
3. Complete thumbnails, current-page synchronization and page rotation.
4. Add staged preview/full/detail quality and deep zoom.
5. Add columns/CAD overview and two-axis scroll behavior.
6. Complete tabs, sidebars, constrained layout and opening/error states.

Exit gate: matched Electron and GPUI viewer scenarios pass on the public corpus
at normal, 400% and 1600% zoom with bounded caches.

### M4 — First complete editor slice: Rectangle

Implement Rectangle end to end:

1. Tool selection and cursor.
2. Click-or-drag draft state.
3. PDF coordinate mapping.
4. Scene rendering and thumbnail overlay.
5. Edge/interior hit testing.
6. Selection, shift selection and window/crossing marquee.
7. Move, resize, rotate and lock behavior.
8. Stroke, fill, width and opacity defaults and selected properties.
9. Undo, redo, copy, paste and delete.
10. Save, close, reopen and compare geometry/appearance.

Exit gate: the native E2E scenario performs create, transform, property edit,
undo/redo, save and reopen. This is the first point at which the GPUI branch is
a demonstrated editor rather than a viewer.

### M5 — Shared editing infrastructure

1. Generalize scene primitives and selection chrome without weakening M4.
2. Complete keyboard shortcuts, context menus and command enablement.
3. Complete tool defaults and selected-markup properties.
4. Complete group movement, post-placement focus and locked selections.
5. Add imported-annotation fallback rendering and preservation.

Exit gate: the annotation engine interface remains small while supporting the
next tool families without shell-specific geometry logic.

### M6 — Tool families

Port one family at a time in this order because each reuses proven behavior:

1. Ellipse and Redact.
2. Line and Arrow.
3. Text Box.
4. Polyline and Polygon.
5. Pen and Highlight.
6. Arc.
7. Dimension.
8. Length, Polylength and Area.
9. Image and Snapshot.
10. Cloud.
11. Callout.
12. Cloud+.

Every family must pass creation, draft, render, hit, selection, transform,
properties, undo, save/reopen and compatibility fixtures before the next
family begins.

### M7 — Snapping and construction workflows

1. Markup geometry index and candidates.
2. PDF content geometry and page-grid candidates.
3. Target filtering and sensitivity.
4. Alignment/equal-size/equal-spacing guides.
5. Construction grid and dimension increments.
6. Page-scale presets, custom scales and visual calibration.
7. Measurement labels, units and precision.

Exit gate: deterministic snapping fixtures and native calibration/measurement
scenarios match the maintained application.

### M8 — Full template subsystem

1. Port blank-PDF dimensions, patterns and metadata.
2. Implement the versioned template library and last-used selection.
3. Implement custom generated templates.
4. Implement imported PDF storage and creation.
5. Implement Save Document as Template.
6. Implement picker, previews and manager with keyboard/accessibility gates.

Exit gate: every built-in pattern, one custom template and one imported PDF
template survive restart and create the expected document.

### M9 — Signature subsystem

1. Port signature appearance and image-processing fixtures.
2. Choose and prove native image sanitization.
3. Port recent signatures and secure storage.
4. Port phone transfer and relay protocol.
5. Port the signature menu and image-tool placement.
6. Save and reopen signatures through the PDF engine.

Exit gate: typed/drawn/image/phone inputs produce sanitized, persistent,
placeable and reopenable signatures without weakening the current threat model.

### M10 — Application and platform parity

1. Native menus and command enablement.
2. Open-file, second-instance, drag/drop and file associations.
3. Window state, full screen, close/quit and dirty-document coordination.
4. Light/dark theme and all remaining dialogs/popovers/tooltips.
5. Default-PDF registration and release-page actions.
6. Native diagnostics and test hooks.

Exit gate: platform behavior passes on macOS, Windows and Linux without an
Electron process.

### M11 — Packaging, updates and cutover

1. Produce stable and beta packages for every maintained format and arch.
2. Port signing, notarization, TUF trust and updater state.
3. Rewrite package and release contract tests around native artifacts.
4. Run native N-1 updater replacement where required.
5. Run accessibility, constrained-window and platform-specific gates.
6. Run randomized matched Electron/GPUI performance comparisons.
7. Run the public corpus and Hibbeler long-document suite.

Exit gate: the native packaged candidate passes all deterministic, native,
compatibility, integrity, performance and update gates. Electron remains the
rollback product until this gate passes.

## Verification model for each piece

Use four layers of evidence:

1. **Pure deterministic tests** at a module interface. These cover model,
   geometry, interaction transitions, properties, serialization and policy.
2. **PDF differential fixtures**. Run the same source and commands through the
   maintained TypeScript and Rust paths, then compare imported models and saved
   results semantically.
3. **Native interaction tests**. Drive real GPUI pointer, keyboard, focus,
   accessibility and constrained-window scenarios.
4. **Packaged cross-platform tests**. Verify file associations, dialogs,
   permissions, signing, updates, native rendering and platform behavior.

A screenshot is evidence only for visible output. It does not prove focus,
keyboard behavior, accessibility, saving, PDF compatibility or performance.

## Skill-assisted workflow

Use the available skills at these points:

- `research`: PDF engine, native packaging, updater and accessibility options,
  using primary sources and a recorded decision artifact.
- `domain-modeling`: Rust terminology and an architecture decision record for
  the document/annotation model and PDF engine.
- `codebase-design`: each module interface and seam before implementation.
- `prototype`: uncertain GPUI pointer, text-editing, accessibility or popup
  behavior, with throwaway code and a specific question.
- `tdd`: every vertical feature slice, starting with Rectangle.
- `diagnosing-bugs`: render, interaction, PDF compatibility and performance
  regressions.
- `code-review`: standards and migration-spec review before merging a slice.
- `test-cross-platform-desktop-app`: risk-based Linux GPU, macOS and Windows
  qualification, including bounded paid GPU leases.
- `grilling`: stress-test the accepted PDF foundation and final cutover plan.

Do not use UI prototyping as a substitute for a vertical slice. Do not mark a
tool migrated until its persistence and reopen behavior pass.

## Immediate next actions

The following 2026-08-25 correction supersedes the older v5/GPUI-CE wording in
this section: the measurement target is the exact Longbridge/Zed
`gpui-component-compat` application and its real `DocumentWorkspace`. Port the
v6 correctness, milestone, native-input, resource, and paired-statistics seams
to that candidate first. GPUI-CE gallery results remain historical and cannot
answer the cutover question.

1. Freeze the exact Electron and GPUI development-runtime candidates, locked
   fixture/reference identities, runner configuration, and paid-lane lease
   ceiling without expanding the final representative feature boundary.
2. Run the scoped deterministic v5 workload, adapter, orchestrator, validator,
   analyzer, property-edit/undo, snap, and dynamic-fidelity checks.
3. Exercise the public fixtures through each actual GUI and worker. Verify
   cancellation, stale rejection, 1600% tile density, close/reopen resource
   recovery, the native XTest target/replay, and retained two-cycle persistence
   before timing.
4. Run the predeclared six-pair calibration and only then select 24–40 final
   pairs within the paid Linux GPU lease. Retain every failure and timeout.
5. Use the result only for the migration-investment decision. If it passes,
   resume the full roadmap for remaining tools, templates, signatures,
   accessibility, platforms, packaging, updates, signing, and release gates.

## Evidence still blocked or not run

- The Hibbeler corpus is blocked/not transferred. It is optional supplementary
  evidence; locked public fixtures must carry the paid Linux comparison.
- Matched macOS visual capture evidence is blocked/not transferred. Current
  PDFium-based macOS visual and accessibility checks were not run.
- Windows inherited PDF source-handle transfer is not implemented. Windows
  native build, rendering, interaction, accessibility, and packaging were not
  run for this candidate.
- The current GPUI preview does not prove Multiply blend parity for Highlight.
- Complete text shaping, fallback, embedding, and cross-viewer font parity were
  not run. The focused PDF mapping uses Helvetica.
- Expanded GPUI semantic and Electron CDP-input scenarios are diagnostics, not
  operating-system native-input or decision-timing evidence. The Linux/X11
  XTest implementation exists but remains unproven for this candidate until it
  completes on a real GPU GUI.
- Writer-side persistence now has exact typed-state, untouched annotation-graph,
  `qpdf`, `pdfinfo`, and fixed Poppler-crop evidence across two reopen cycles.
  GPUI/PDFium window crops and matched Electron/GPUI structural and SSIM checks
  are still blocked.
- The paid Linux GPU preflight, calibration, and final randomized v5 comparison
  were not run for the PDFium/representative-annotation candidate. Live
  qualification and a yes/no investment decision therefore remain open.
- A packaged native candidate does not exist.
- GPUI-CE is pinned as the immutable compatible foundation candidate. Native
  compilation, accessibility, runtime, and packaging qualification remain open
  on macOS, Windows, Linux ARM64, X11, and Wayland.

## Generated-template journey update

One representative template now crosses the real native document boundary:
A3 landscape Square Grid generation, owned temporary source, PDFium page and
thumbnail rendering, dirty-close protection, failed Save As preservation,
staged validation, atomic publication, independent reopen, and worker/source
cleanup. This converts the template subsystem from UI-only mock evidence to a
partial real journey. The remaining template picker/manager contract,
additional patterns, imported/custom templates, persistence, visual acceptance,
and packaged platforms remain incomplete.

## Template bridge status

The built-in picker is no longer UI-only mock evidence. Its single real GPUI
Component control now dispatches into `DocumentWorkspace`; the representative
Square Grid path creates a stable temporary native session, renders through
checksum-pinned PDFium, requires Save As, atomically publishes and independently
reopens, then releases the old worker and temporary source. All six built-ins
have deterministic generator coverage. Custom/imported records, manager and
library persistence, asynchronous generation/cancellation, cleanup-error retry
UI, Windows source-handle transfer, native visual/accessibility acceptance, and
packaged platforms remain incomplete.

This does not make the migration complete. The next dependency-ordered cutover
proof is a representative rectangle annotation created and edited in the real
workspace, saved, closed, reopened in a fresh session, and compared by stable
identity and appearance. Further micro-control parity is lower priority than
that end-to-end journey.

## 2026-08-26 Rectangle journey result

That cutover proof now passes as Linux development evidence. The GPUI candidate
uses the real component tool buttons and line-width Popover, rendered native
pointer input, the application-owned session, transactional Save As, clean
close, and a distinct-workspace reopen. The saved `/Square` survives by stable
identity and appearance, changes PDFium's annotation-enabled pixels, passes
`qpdf` and `pdfinfo`, and leaves no worker or mapped surface. The PDF
compatibility boundary permits only 0.00001 pt coordinate drift caused by
`/Rect` edge reconstruction; every other persisted field remains exact.

This closes the representative Rectangle cutover journey, not the complete
Rectangle feature. Electron still has free-form stroke/fill/opacity, hatch,
position, size, rotation, lock and double-click inspector behavior that the
native property surface does not fully expose. The next higher-risk slice is
real worker/render failure recovery with dirty-state preservation. After that,
complete the Rectangle inspector before expanding less-used tool families.

## Worker recovery result

The native candidate now passes the real failure journey that previously made a
live session unusable. Killing the exact checksum-pinned PDFium worker causes a
recoverable presentation error while retaining the last page raster, dirty
Rectangle, selection/history, current page, zoom, mode, and scroll. The real
GPUI Component Retry control replaces only the failed resource after source and
page-geometry validation. Stale replacements are released, the crashed process
is reaped, and every mapped surface is removed on close.

Linux development evidence passes: deterministic 1/1, real worker 1/1, and
all-targets 134 with six gated ignores. Native pixels/accessibility, shipping
PDFium, Windows source-handle transfer, packages, physical devices, Hibbeler,
IME, and the matched performance decision remain blocked or not run. The next
critical path is the complete Rectangle inspector through transactional Save As
and independent reopen, not another isolated toolbar behavior.

## Rectangle inspector result

The maintained Rectangle property contract is now implemented through a real
GPUI Component form and the application-owned document session. The real
100-page journey proves create, pointer move, the complete working property
matrix, one history revision per effective edit, lock suppression, stable-ID
Save As, PDFium pixels, structural PDF validation, close/reap, and fresh
workspace reopen. Dotted style is represented in native overlay and PDF dash
state. Hatch is excluded as an Electron no-op; Cloud remains an Electron
baseline defect because its current Rectangle path renders and exports Solid.

Linux development gates pass: focused inspector 1/1, real journey 1/1, typed
model 1/1, PDF persistence, annotation adapter, source/guard 17/17, 870-package
single-GPUI policy, and all-targets 137 plus six gated ignores. Native visual,
live accessibility, packaged platforms, physical input/IME, shipping PDFium,
Windows source handles, Hibbeler, and matched performance remain outside this
evidence. The next work is selected by complete cutover journey value and
release risk, not by the number of individual controls migrated.

## In-place Save cutover update (2026-08-26)

The largest remaining gap in the previously partial safe-publication row is now
closed for Unix development runtimes. `DocumentWorkspace` owns an explicit
opened-source save request; `PdfPersistenceSession` owns no-follow source
identity, digest, same-directory stage, independent validation, mode-preserving
atomic replacement, parent sync, and a post-publication receipt. Application
state remains immutable while Save is active, and any pre-publication failure
retains the original dirty session. The checksum-pinned real 100-page test
proves Save-button dispatch, worker replacement/release, structural validity,
stable document/view state, and fresh-workspace reopen.

Passed: focused 7/7, real 1/1, warm all-targets 144 plus seven gated ignores,
source/guard 17/17, exact prepared digest, single GPUI identity, dependency
policy, and host-storage bounds. Failed: retained TDD red/diagnostic runs only.
Blocked: Windows replacement and inherited source-handle semantics, production
PDFium redistribution, and native visual/accessibility. Not run: packages,
physical platforms, IME, Hibbeler, and matched performance. The cutover path now
connects normal in-place Save to the dirty-tab and application-close queues;
generated/temporary documents must continue through Save As.

## Application-close cutover update (2026-08-26)

The ordinary-document close path now uses the real guarded in-place Save
transaction. The application close owner freezes stable identities and dirty
revisions, dispatches saves asynchronously in tab order, and releases every
session before quit. Generated documents still select Save As. Cancellation,
pre-publication failure, stale result, release failure, and a published file
with a durability warning remain distinct interruption states. A duplicate
close request cannot replace the active snapshot.

Passed: pure 10/10, integration 13/13 plus one gated ignore, real two-PDF
rendered-modal journey 1/1, warm all-targets 150 plus seven gated ignores,
source/guard 17/17, exact prepared digest, single-GPUI graph, dependency policy,
and storage bounds. Failed: retained red tests only. Blocked: native visual and
live accessibility on this VPS, shipping PDFium, and Windows in-place
replacement/source-handle semantics. Not run: packaged platforms, physical
input, IME, Hibbeler, and matched performance. The next cutover-critical gap is
the native Save As picker continuation for generated documents during close;
pointer micro-parity remains lower priority.

## Generated Save As close update (2026-08-26)

The generated-document branch now uses GPUI's native path request and returns
the selected `PathBuf` to the same frozen application-close transaction. It
suppresses duplicate prompts and rejects cancel, platform failure, non-PDF
targets, stale transaction results, and mutation after the snapshot. The
checksum-pinned real mixed journey saves two ordinary documents and one
generated document, independently validates and reopens all three, deletes the
temporary generated source, releases all workers and surfaces, and only then
emits quit.

Passed: pure 11/11, integration 17/17 plus one gated ignore, real mixed PDFium
1/1, warm all-targets 155 plus seven gated ignores, source/guard 17/17, exact
prepared digest, single-GPUI graph, dependency policy, and storage bounds.
Failed: retained TDD red/compile diagnostics only. Blocked: the pinned picker is
app-global and lacks title/filter/default-extension/window-owner controls,
shipping PDFium, Windows replacement/source-handle semantics, and live native
visual/accessibility. Not run: packaged platforms, physical input, IME,
Hibbeler, and matched performance. The next cutover-critical boundary is a
capability-bound save target with visible retry/recovery for save and release
failures.

## Save-target authority and recovery cutover update (2026-08-26)

The Unix development candidate now binds Save As to a one-shot native target
authority before background work begins. It retains the selected parent
directory, rejects source/target aliases and replacement races, publishes
without overwrite, and cleans staging files only after an inode identity
check. Non-UTF-8 PDF names remain native paths. This is a stricter ownership
boundary than the current Electron path-token flow, but it does not yet provide
the equivalent Windows directory-handle implementation.

The application-close shell renders typed recovery for picker, target, save,
post-publication warning, and release failures. The Electron store records some
save errors without a current renderer consumer, so the exact recovery copy and
actions are a documented GPUI correctness improvement rather than claimed
pixel/behavior parity.

Passed: authority 7/7; close state 11/11; integration 19/19 plus one gated
ignore; real mixed PDFium 1/1; warm all-targets 163 plus seven gated ignores;
source/guard 17/17; exact prepared digest; one GPUI identity across 870
packages; and dependency policy with warnings but no denial. Failed: retained
red iterations only. Blocked: Windows target authority, production PDFium, and
live native visual/accessibility. Not run: packages, physical platforms, IME,
Hibbeler, and matched performance. This improves the cutover-critical save
boundary but does not establish release readiness.

## Native multi-document open cutover update (2026-08-26)

Electron's picker, menu, startup, second-instance, and drop paths ultimately
share one sequential renderer open contract. The GPUI candidate now has the
matching application-owned coordinator for picker/menu/system/drop origin
policy. The real picker, Control/Command+O, and normal startup are connected.
Ordinary duplicates focus the existing stable session without resetting its
page; drop-origin duplicates receive independent resources. A mixed batch keeps
all successes, removes each failed transient session, retains failures after a
later success, selects the first successful new document, and exposes a real
non-modal GPUI Component Alert. Native path bytes are preserved.

Passed: focused 5/5; launch 4/4; warm all-targets 174 plus seven gated ignores;
story/worker build; source/guard 17/17; exact prepared digest; one GPUI identity
across 870 packages; dependency policy; scoped formatting; storage bounds.
Failed: retained red-first diagnostics only. Blocked: native visual and live
accessibility on this VPS, production PDFium distribution, and Windows save
authority. Not run: the native application menu, pointer drop adapter, macOS
`open-file`, second-instance delivery, packaged platforms, physical input/IME,
Hibbeler, and matched performance. Electron remains the shipping rollback
product.

## Native application boundary cutover update (2026-08-26)

The native candidate now has one no-fork application adapter for startup paths,
macOS file URLs, pointer file drops, application menus, and close/quit command
ownership. Local file URLs are parsed before they enter the retained system-open
batch. Real GPUI `ExternalPaths` drops enter with Drop origin and preserve
Electron's force-new duplicate behavior. GPUI owns the operating-system menu on
macOS; Linux and Windows render the same model with the real GPUI Component
`AppMenuBar`.

Menu Open, Save, and Save As use global retained-document commands, so they do
not depend on a child control already owning focus. Close Window and Quit use
one `RequestApplicationClose` action. That action opens the real dirty-close
transaction and cannot terminate the application while a dirty document or
unreleased worker remains.

Passed: native-application 6/6; exact story/worker build; warm all-targets 180
plus seven gated ignores; source/guard 17/17; prepared digest
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
one GPUI identity across 870 packages; dependency policy; scoped diff; and host
storage bounds. Failed: retained red-first and compile diagnostics only.
Blocked: production PDFium and Windows save authority. Not run: physical macOS
menu/Open With proof, Linux/Windows single-instance delivery and file
associations, native visual/accessibility, packages, physical input/IME,
Hibbeler, and matched performance. The pinned GPUI Linux/Windows backends do not
invoke `on_open_urls`, so their second-instance adapter remains product-owned
work rather than a GPUI Component capability.

## Native-shell Rectangle transaction evidence (2026-08-26)

One deterministic Linux development test now proves the native application
journey from a real GPUI Component window through Rectangle creation, dirty
application close, Save, validated PDF publication, resource release, and a
fresh-window reopen. This removes the prior evidence gap where UI activation,
save/close orchestration, and independent reopen were only proved by separate
tests.

Passed: focused checksum-pinned PDFium 1/1, build-policy 13/13, warm all-targets
180 plus eight gated ignores, structural PDF validation, both worker exits,
mapped-surface cleanup, and storage guards. Failed: no accepted gate; two
retained TDD red iterations are diagnostic history. Blocked: production
PDFium distribution and Windows save-target authority. Not run: packaged or
physical-platform qualification, live accessibility and screenshot review,
IME, Hibbeler, and the matched performance decision. This evidence remains
inside the experiment and does not authorize cutover or release.

## Ordinary Save As collision recovery evidence (2026-08-26)

Electron records ordinary Save and Save As failures in
`viewerStore.errorMessage`, but the current renderer has no consumer for that
value. The GPUI candidate now models these failures as typed
`NativeDocumentSession` state and renders a real GPUI Component Alert with
operation-safe actions. This is a documented correctness improvement, not a
claim of strict behavior parity with an invisible Electron defect.

Passed Linux development evidence: an occupied Save As destination remains
byte-identical; the source path, dirty Rectangle, current session, and original
worker remain live; choosing a fresh target succeeds; `qpdf` accepts the result;
the stable Rectangle reopens; and every worker and mapped surface is released.
The real gate passes 1/1, ordinary focused recovery passes, warm all-targets
passes 181 plus nine ignores, source/guard passes 17/17, the exact prepared
digest and 870-package single-GPUI graph are unchanged, and dependency policy
has no denial. Failed evidence is one retained red run fixed by the typed
failure seam. Blocked: production PDFium, Windows target authority, and live
visual/accessibility. Not run: packaged platforms, physical input, IME,
Hibbeler, and matched Electron/GPUI performance. This closes one high-value
failure journey but does not make the native candidate cutover-ready.

## Controlled real-document viewer evidence (2026-08-26)

The prior audit separated the proven GPUI Component toolbar from the real PDF
workspace. That seam is now closed for the daily read-only viewer journey.
One `DocumentWorkspace` renders the real component toolbar and two stable-ID
native PDF sessions. Each session independently owns page mode, fit/manual
zoom, wheel policy, current page, scroll, viewer generation, tile cache, worker,
and mapped surfaces. The presentation controls do not own duplicate product
state and do not synchronize during render.

Passed Linux development evidence: rendered tab selection, thumbnail page
navigation, Single Page/Continuous, Fit Page/Fit Width, 400%/1600%, independent
scroll, real tile pixels, 32-tile and 256 MiB bounds, stale-result rejection,
and complete two-worker cleanup pass in one checksum-pinned journey. The
focused gate is 1/1, warm all-targets is 181 plus ten ignores, source/guard is
17/17, and the exact prepared digest and single-GPUI graph remain unchanged.
No accepted gate fails. Blocked: shipping PDFium, Windows target authority,
and live native visual/accessibility. Not run: packaged or physical platforms,
IME, Hibbeler, sustained pressure, and the final matched Electron/GPUI
performance run. This raises cutover confidence but does not establish release
readiness or justify deleting Electron.

## Real-session tab consolidation evidence (2026-08-26)

The Document Tab Bar proof no longer stops at mock tab records for selection,
clean/dirty close, keyboard reorder, and pointer reorder. `DocumentWorkspace`
now reorders the actual `Entity<NativeDocumentSession>` vector by stable
`DocumentId` while retaining active view state, annotations, pending close
identity, and resource ownership. Standard visible controls remain real pinned
GPUI Component `TabBar`, `Tab`, `Button`, and `Popover` primitives. The
six-pixel pointer sensor and polite status are transparent product adapters for
behavior the component library does not own.

Passed Linux development evidence: focused rendered journey 2/2 in 31 seconds;
warm all-targets 183 active plus ten gated ignores in 47 seconds; source/guard
17/17; exact prepared digest; one GPUI identity across 870 packages; configured
dependency policy; and host storage bounds. Failed evidence is retained TDD
diagnostic output only. Blocked: pointer edge auto-scroll, order persistence,
shipping PDFium, Windows target authority, and live visual/accessibility proof.
Not run: packages, physical macOS/Windows input, IME, Hibbeler, and final
matched performance. This removes a mock-only multi-document seam but does not
establish cutover or release readiness.

## Polyline and Polygon cutover evidence (2026-08-26)

The previously visual-only vertex-path row now has one real cutover-critical
Linux development journey. The GPUI candidate uses real GPUI Component Buttons
for the standard tool controls and application-owned state for path drafts,
history, stable annotation identity, PDF persistence, and resource ownership.
The raw GPUI layer is confined to PDF and annotation rendering, pointer hit
testing, and path geometry.

The frozen Electron completion rule is now explicit: Enter or Escape commits a
valid Polyline or Polygon and discards an invalid draft. Deterministic tests
also cover the exact pointer edit threshold, Polygon start closure and closing
edge validation, selection, movement, locking, undo/redo, independent path
state, and page-major ordering. The real PDF journey proves canonical native
objects, `/Vertices`, appearance streams, changed PDFium annotation pixels,
`qpdf`, clean close, distinct-workspace reopen, and complete worker/surface
release.

Passed: rendered workspace 1/1, adapter 32/32, real PDFium 1/1, warm all-targets
184 plus eleven gated ignores, source/guard 17/17, exact prepared digest,
single-GPUI graph, configured dependency policy, and storage bounds. Failed:
retained red-first diagnostics only. Blocked: shipping PDFium, Windows save
authority, and live native visual/accessibility. Not run: packages, physical
macOS/Windows input, IME, Hibbeler, and matched performance. This completes the
representative Polyline/Polygon journey but not migration or release readiness.

## Polylength and Area cutover evidence (2026-08-26)

The calibrated multi-point measurement path is now a real end-to-end Linux
development journey rather than a visual or model-only claim. The visible
tools are pinned GPUI Component `Button`s. The retained application session
owns page scale, drafts, stable IDs, captions, selection, edits, history,
dirty/save state, and worker ownership. Raw GPUI is restricted to PDF and
annotation painting, hit testing, and path geometry.

The persistence boundary now classifies current intent/Measure dictionaries
and legacy subject-only Polylength/Area annotations without absorbing ordinary
Polyline, Polygon, PolygonCloud, or direct legacy dictionaries. Calibration
precedence is annotation `/BPScale`, annotation `/Measure`, then page
`/BPPageScale`; a present malformed `/Measure` fails closed. The real journey
proves calibrated creation, double-click and Enter completion, independent
Area state during a Polylength vertex edit, canonical native measurement
identity, changed PDFium appearance pixels, `qpdf`, clean close, a distinct
workspace reopen, and complete worker/mapped-surface release.

Passed: real PDFium 1/1, measurement model 1/1, adapter 34/34, persistence
18/18, page-scale interaction 5/5, warm all-targets 185 plus twelve gated
ignores, source/guard 17/17, exact prepared digest, single-GPUI graph,
configured dependency policy, and storage bounds. Failed: retained red-first
diagnostics only. The final broad gate initially found two tests that clicked
the now-off-viewport Page Scale trigger; the focused red loop proved the
horizontal scroll ownership and the corrected visible-scroll precondition. No
accepted gate remains failing. Blocked: shipping PDFium, Windows save
authority, and live native visual/accessibility. Not run: packages, physical
macOS/Windows input, IME, Hibbeler, and matched performance. Duplicate
normalized imported IDs are still rejected only within a managed family; the
cross-family fail-closed rule remains explicit hardening work. This completes
the Polylength/Area journey but not migration or release readiness.

## Cloud cutover evidence (2026-08-26)

The standalone Cloud row has moved from visual-only to a cutover-critical Linux
development journey. The standard tool is a real pinned GPUI Component Button.
Application-owned state retains click-node drafts, stable Cloud identity,
deterministic scallop geometry, selection, vertex/body edits, lock/history,
dirty/save state, and worker ownership. Raw GPUI remains limited to PDF and
annotation painting, hit testing, pointer geometry, and control handles.

The persistence boundary classifies Cloud before ordinary Polygon and writes
canonical `/Polygon`, `/IT /PolygonCloud`, `/BE << /S /C /I 2 >>`,
`/Subj (Cloud)`, stable `/NM`, `/Vertices`, and `/AP /N`. The real fixture
journey proves creation, a stable vertex edit, guarded Save As, typed
independent reopen, changed PDFium appearance pixels, `qpdf`, clean close, a
distinct-workspace reopen, and complete worker/mapped-surface release.

Passed: model 1/1, persistence 18/18, adapter 35/35, rendered workspace 1/1,
real PDFium 1/1, warm all-targets 186 plus thirteen gated ignores,
source/guard 17/17, exact prepared digest, single-GPUI graph, configured
dependency policy, and storage bounds. Failed: retained red-first diagnostics
only; one focused failure exposed and then fixed a missing Cloud branch in the
shared lock command. No accepted gate remains failing. Partial: rectangle-drag
creation, live workspace pointer edit, exact cubic scallop parity, and
intensity/style properties. Blocked: shipping PDFium, Windows save authority,
and live native visual/accessibility. Not run: packages, physical macOS/Windows
input, IME, Hibbeler, a third-party Cloud corpus, and matched performance. This
completes one Cloud development journey, not migration or release readiness.

## Callout cutover evidence (2026-08-26)

Callout is no longer visual-only. The candidate composes the existing retained
Line and Text Box domains behind one stable Callout identity. Standard visible
controls are real pinned GPUI Component `Button` and `Textarea` primitives;
raw GPUI owns only document/annotation paint, hit testing, pointer geometry,
and selection handles. The two-click placement and editor keep product state
outside component internals and combine creation plus initial multiline text
into one undo transaction.

The real public-fixture journey proves create, text input, text-box move, knee
edit, guarded Save As, canonical Callout classification, typed independent
reopen, changed PDFium pixels, `qpdf`, fresh-workspace hydration, and complete
worker/mapped-surface release. Passed: rendered workspace 1/1, focused
persistence green, real PDFium 1/1, corrected Length regression 1/1, warm
all-targets 187 plus fourteen gated ignores, source/guard 17/17, exact prepared
digest, one GPUI identity across 870 packages, configured dependency policy,
and storage bounds. Failed: retained red-first diagnostics only; the real red
test exposed local-Form coordinate clipping, and the broad gate exposed a stale
off-screen Length test click overlapping Rotate Left. Both are fixed and no
accepted gate fails. Partial: existing-Callout editing, complete pointer
handles/body drag, exact font/rich-text parity, properties, and Cloud+.
Blocked: shipping PDFium, Windows save authority, and live native
visual/accessibility. Not run: packages, physical macOS/Windows input, IME,
Hibbeler, third-party Callout corpus, and matched performance. This closes one
Callout development journey but not migration or release readiness.

## Dimension cutover evidence (2026-08-26)

Dimension is no longer visual-only. The candidate has a dedicated retained
Dimension aggregate rather than reusing calibrated Length. Standard visible
controls are real pinned GPUI Component `Button` and `Textarea` primitives;
raw GPUI owns only document/annotation paint, hit testing, pointer geometry,
arrowheads, extension lines, captions, and selection handles.

The real public-fixture journey proves two-click creation, caption input, a
stable offset edit, guarded Save As, canonical unmeasured LineDimension
classification, typed independent reopen, changed PDFium pixels, fresh-
workspace hydration, and complete worker/mapped-surface release. A
LineDimension with `/Measure` remains Length. Passed: focused model/adapter and
workspace gates, persistence 2/2, real PDFium 1/1, warm all-targets 189 plus
sixteen gated ignores, source/guard 17/17, exact prepared digest, one GPUI
identity across 870 packages, configured dependency policy, and storage
bounds. Failed: retained red-first diagnostics only; the broad gate exposed
two stale off-screen Text Box clicks, which are fixed. No accepted gate fails.

Partial: complete workspace pointer endpoint/body/offset editing, property
controls, dimension-increment snapping, exact visual parity, and imported
corpus breadth. Blocked: shipping PDFium, Windows save authority, and live
native visual/accessibility. Not run: packages, physical macOS/Windows input,
IME, Hibbeler, third-party Dimension corpora, and matched performance. This
closes one Dimension development journey but not migration or release
readiness.

## Arc cutover evidence (2026-08-26)

Arc is no longer visual-only. The candidate has a dedicated retained Arc
aggregate and three-point geometry rather than approximating the tool as an
Ellipse. The standard visible control is a real pinned GPUI Component `Button`;
raw GPUI owns only PDF/annotation paint, curved hit testing, pointer geometry,
sampled-path rendering, and selection handles.

The real public-fixture journey proves three-click creation, stable midpoint
edit, guarded Save As, canonical CircleArc classification, typed independent
reopen, changed PDFium pixels, fresh-workspace hydration, and complete worker/
mapped-surface release. The focused persistence journey also proves create,
edit, delete, native identity, appearance stream, and independent `qpdf`
validation. Passed: model 1/1, focused adapter and persistence, rendered
workspace 1/1, real PDFium 1/1, warm all-targets 190 plus seventeen gated
ignores, source/guard 17/17, exact prepared digest, one GPUI identity across
870 packages, configured dependency policy, and storage bounds.

Failed: retained red-first diagnostics only. Compiler reds found a duplicate
helper and one missed exhaustive family label. Runtime reds found only
PDF-number tolerance assertions. The broad gate found one old Pen click that
did not first scroll its overflowed toolbar target into view; it is fixed. No
accepted gate fails. Partial: complete live pointer body/start/end/mid editing,
appearance properties, malformed/non-square imported CircleArc policy, exact
native visuals, and corpus breadth. Blocked: shipping PDFium, Windows save
authority, and live native visual/accessibility. Not run: packages, physical
macOS/Windows input, IME, Hibbeler, third-party Arc corpora, and matched
performance. This closes one Arc development journey but not migration or
release readiness.

## Pending Redact cutover evidence (2026-08-26)

Pending Redact is no longer visual-only. The standard tool and truthful
pending-content warning use real pinned GPUI Component `Button` and `Alert`
primitives. Application-owned state retains stable identity, pointer draft,
body move, eight-handle resize, lock/history state, PDF reconciliation, and
resource ownership. Raw GPUI owns only page/annotation paint, hit testing,
pointer geometry, and handles.

The real public-fixture journey proves click-or-drag creation, move, resize,
guarded Save As, canonical typed `/Redact` with no `/AP`, clean close,
fresh-workspace reopen, deletion from an experiment copy, unchanged
annotation-disabled PDFium pixels, worker exit, and mapped-surface cleanup.
The visible warning states that underlying text and graphics remain. No code in
this slice applies redactions, edits page content streams, sanitizes, or
flattens the PDF.

Passed: model geometry 2/2, focused adapter and persistence, rendered workspace
1/1, checksum-pinned real PDFium 1/1, warm all-targets 191 plus eighteen gated
ignores, source/guard 17/17, exact prepared digest, one GPUI identity across
870 packages, configured dependency policy, and storage bounds. Failed:
retained red-first diagnostics exposed exact PDF edge-number quantization; the
fixed equivalence rule now models the persisted representation and rejects a
representable 0.001-point change. No accepted gate fails.

Partial: external/noncanonical Redacts remain read-only opaque objects and
relationship/equal-size snapping is deferred. Blocked: Apply Redactions,
content destruction, sanitization, flattening, production PDFium
redistribution, Windows save-target authority, and live native visual/
accessibility evidence. Not run: packages, physical macOS/Windows input, IME,
Hibbeler, third-party Redact corpora, and matched performance. Electron remains
the shipping rollback. Secure redaction requires a fresh product and security
decision; this development evidence does not imply it.

## Snapshot cutover evidence (2026-08-26)

Snapshot has advanced from an unmigrated Media tool to a complete Linux
development journey. The standard visible control is a real pinned GPUI
Component `Button`. Application-owned retained state owns the exact two-click
contract, annotation-free page capture, stable identity, move/eight-handle
resize, rotation/opacity data, lock/history, persistence reconciliation, and
resource ownership. Raw GPUI owns only page/annotation paint, hit testing,
pointer geometry, crop mapping, and selection handles.

The real public-fixture journey proves nonuniform capture, move, resize,
guarded Save As, canonical StampSnapshot classification, typed independent
reopen, unchanged annotation-disabled pixels, changed annotation-enabled
pixels, fresh-workspace hydration, experiment-copy deletion, and complete
worker/mapped-surface release. The persistence gate also proves same-asset
Form replacement, Image/SMask reuse, owned-object cleanup, and preservation of
external or malformed Stamp objects.

Passed: model 3/3, adapter 2/2, persistence 1/1, rendered workspace 1/1,
checksum-pinned real PDFium 1/1, warm all-targets 192 plus nineteen gated
ignores, source/guard 17/17, exact prepared digest, one GPUI identity across
870 packages, configured dependency policy, and storage bounds. Failed:
retained red-first diagnostics only. No accepted gate fails.

Partial: live rotation-handle gesture, opacity property controls, rotated or
cropped page breadth, capture fallback semantics, vendor/private Snapshot
payloads, hostile appearance graphs, exact native visuals, and accessibility.
Blocked: shipping PDFium, Windows save authority, and live native visual/
accessibility. Not run: packages, physical macOS/Windows input, IME, Hibbeler,
third-party Snapshot corpora, and matched performance. Electron remains the
shipping rollback.

## Semantic-snapping cutover evidence (2026-08-26)

Semantic snapping has advanced from an unmigrated cross-cutting behavior to a
bounded real Line-and-Length journey. The standard settings controls use real
pinned GPUI Component `Button`, `Popover`, and `Checkbox` primitives.
Application-owned state retains the settings, indexed source candidates,
transient decision, and history. Raw GPUI owns only canvas pointer geometry and
guide paint.

The engine proves the frozen inclusive eight-window-pixel Euclidean tolerance,
source and target toggles, active-owner exclusion, bounded intersections, and
Shift constraint before on-axis candidate selection. Rectangle, straight
Line/Arrow, Dimension, and Length contribute endpoints, midpoints, centers,
intersections, and optional nearest candidates. Real rendered Line and Length
creation share that engine. The checksum-pinned public-fixture journey proves
one revision per commit, guarded Save As, typed independent reopen, effective
persistence equivalence after PDF-number normalization, changed PDFium pixels,
fresh-workspace hydration, worker exit, and mapped-surface cleanup.

Passed: engine 7/7, workspace 2/2, adapter 2/2, real PDFium 1/1, warm all-
targets 194 plus twenty gated ignores, source/guard policy 17/17 across 18
shared inputs, exact prepared digest, one GPUI identity across 870 packages,
configured dependency policy, and host storage. Failed evidence consists only
of retained red-first diagnostics for the missing constrained seam, fixture
unit, PDF-number tolerance, and effective Length calibration; all are fixed and
no accepted gate fails.

This is partial, not complete snapping parity. Missing work includes vertex and
ellipse source families, PDF content, page and construction grids, dimension
increments, object tracking, alignment, equal-size/equal-spacing guidance,
move/resize/edit snapping, and rotated-page/UserUnit qualification. Blocked:
shipping PDFium, Windows save authority, and live native visual/accessibility
on this headless host. Not run: packages, physical macOS/Windows input, IME,
Hibbeler, third-party geometry corpora, and matched performance. The exact
story compiles but was not launched. Electron remains authoritative for the
unmigrated snapping breadth and remains the shipping rollback.

## Shared Rectangle/Ellipse inspector correction (2026-08-26)

The current experiment uses one application-owned inspector seam for Rectangle
and Ellipse and real pinned GPUI Component form controls. The latest accepted
Linux development evidence is focused shared inspector 4/4 plus one ignored
real test, gallery persistence 3/3, real checksum-pinned PDFium 1/1, and warm
all-targets 198 active plus 21 gated ignores. Source/guard policy is 17/17;
the prepared digest and one-GPUI graph are unchanged. Failed status contains
only retained red-first diagnostics whose defects are fixed. Partial status is
the two-family scope and the pinned ColorPicker disabled API gap. Blocked
status is production PDFium redistribution, Windows save-target authority, and
fresh native visual/live accessibility on the headless host. Packages,
physical macOS/Windows input, IME, Hibbeler/third-party corpora, and matched
performance are not run.

This does not imply migration completion. The next audit boundary is the
coordinate-space contract for non-default CropBox origins, inherited rotation,
and `/UserUnit`; the contract must be proven across page rendering,
thumbnails, tiles, annotation geometry, hit testing, Save As, and fresh reopen.

## Canonical PDF coordinate-space slice (2026-08-26)

The native spine now has one application-owned, GPUI-free coordinate-space
record per opened page. `lopdf` resolves inherited `MediaBox`, `CropBox`,
`Rotate`, and `/UserUnit`; the worker protocol reports the same values; and
`PdfiumWorkerResource`, `NativeDocumentSession`, the viewer tiles, and the
annotation layer consume the effective space after a retained page rotation.
Raw annotation points remain in the PDF coordinate system. Legacy scripted
openers use an explicit zero-origin/unit-one fallback and are not accepted as
non-default-space proof.

Passed Linux development evidence: gallery parser/transform 7/7, protocol 2/2,
workspace retention 1/1, compat worker build 1/1, real checksum-pinned PDFium
100-page open/navigation/failure/release journey 1/1 (including the existing
non-zero CropBox/Rotate fixture), source/build policy 17/17, and warm
all-targets 199 active plus 21 gated ignores. Logs:
`button-probe-20260826T233731Z-2728570.log`,
`button-probe-20260826T230354Z-2701558.log`,
`button-probe-20260826T233901Z-2729301.log`,
`button-probe-20260826T233517Z-2726975.log`,
`button-probe-20260826T234720Z-2733533.log`, and
`button-probe-20260826T234756Z-2734176.log`. The prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`; the
graph remains one pinned GPUI identity across 870 packages; dependency policy
has no denial; and host storage stayed green.

Failed: one gallery PDF-worker build hit the four GiB disposable-target cap
and was cleaned under the safety policy; it is diagnostic history only.
Partial: the real fixture covers a non-zero CropBox and `/Rotate 90`, but no
real fixture carries `/UserUnit`. Highlight precomposition, snapshot capture,
every hit-test path, and coordinate-sensitive persistence need a dedicated
non-default-space journey. Blocked: production PDFium redistribution, Windows save-target
authority, and fresh native visual/live accessibility on the headless VPS.
Not run: packaged candidates, physical macOS/Windows input, IME, Hibbeler,
and matched Electron/GPUI performance. Electron remains the shipping rollback.

Recommended next slice: add one provenance-controlled non-default CropBox plus
`/UserUnit` fixture and prove page/thumbnail/tile/annotation/hit-test agreement
through Save As and fresh reopen before expanding annotation breadth.

## Real `/UserUnit` cutover evidence (2026-08-27)

The prior missing-corpus boundary is now closed for the core document,
Rectangle, Highlight, and Snapshot journeys. A checksum-locked deterministic
PDF combines non-zero CropBox, `/Rotate 90`, and `/UserUnit 2`. One exact real
session renders page, thumbnail, and tile pixels; creates and hit-tests a
Rectangle through the live GPUI canvas; precomposes Highlight pixels; captures
a spatially varied Snapshot raster; saves; independently reopens with identical
canonical metadata; hydrates all three annotations in a fresh workspace; and
proves all worker and mapped resources are released.

Passed: fixture oracle 8/8, qpdf/pdfinfo seven-PDF validation, guard 13/13,
exact real journey 1/1 (`button-probe-20260827T055130Z-2805880.log`), warm
all-targets 199 active plus 22 gated ignores
(`button-probe-20260827T055441Z-2807434.log`), source preparation 4/4, exact
prepared digest, one GPUI identity across 870 packages, dependency policy with
no denial, and green storage. Failed: retained TDD diagnostics only; the broad
rustfmt gate still reports inherited experiment-wide drift and was not applied
over unrelated user work. Blocked:
production PDFium redistribution, Windows save-target authority, and live
native visual/accessibility. Not run: packaged platforms, physical input, IME,
Hibbeler, third-party coordinate corpora, and matched performance. This remains
Linux development-only evidence and does not authorize cutover. The next
cutover-critical journey is the custom/imported template-library lifecycle
through the real document-session seam.

## Imported template-library spine (2026-08-27)

The missing storage and ownership boundary now exists as a GPUI-free deep
module. It persists custom generated and imported-PDF records, last-used
selection, private checksum-bound sources, and a versioned atomic index under
an experiment-owned sentinel root. Materialization creates an independent
temporary source and then enters the established real `DocumentWorkspace`
session, dirty-close, Save As, reopen, and resource lifecycle.

Passed: deterministic library restart/import/checksum/materialization/removal
1/1; non-real retained workspace 1/1; real checksum-pinned 100-page PDFium
render, thumbnail, failed-import isolation, removal, Save As/reopen, two-worker
exit, and mapped-surface cleanup 1/1; all-targets 200 active plus 23 ignored;
guard/source gates 17/17; exact prepared digest; one GPUI across 870 packages;
dependency policy without denial; and green storage. Exact logs are
`button-probe-20260827T061249Z-2816433.log`,
`button-probe-20260827T060924Z-2814015.log`,
`button-probe-20260827T061315Z-2816889.log`, and
`button-probe-20260827T061454Z-2817891.log`.

Failed: two retained TDD diagnostics, both fixed. Partial: dynamic
custom/imported rows and the manager UI are not yet rendered through the real
GPUI Component shell. Blocked: production PDFium redistribution and Windows
save-target authority. Not run: fresh native visual/accessibility, IME,
packaged candidates, physical macOS/Windows, Hibbeler, and matched performance.
The next slice is the real GPUI Component manager/list/input command surface;
the new storage module remains the sole persistence owner.

### Template manager implementation checkpoint (2026-08-27)

The native stack now has one persistent template authority and a real pinned
GPUI Component manager surface in the runnable story. Focused manager and
manager/control tests pass 16/16, native application tests pass 6/6, and
dynamic tab-bar command tests pass 23/23.
A custom Square Grid and a checksum-controlled
imported 100-page PDF each pass real page/thumbnail rendering, Save As,
independent reopen, worker replacement/exit, temporary-source release, and
mapped-surface cleanup. The warm all-targets gate passes 217 active tests with
23 explicitly gated ignores.

This is Linux development evidence. Production PDFium remains blocked. Native
visual/accessibility, screen reader, IME, packages, and physical platforms were
not run. The complete settings editor, dynamic picker, persistent authority,
and authorized-source save route are connected. Native Dialog Escape and
outside-click handling lacks fresh native input proof. Issue #84 is complete as
Linux development evidence only.

## 2026-08-27 native viewer-shell and CAD evidence

Issue #85 replaces the earlier isolated viewer mock with a session-owned native
workspace. Real PDF pages and lazy thumbnails render through the pinned worker;
fit, zoom, page mode, CAD Columns/Rows, page rotation, scroll, and keyboard
navigation remain independent across documents. Real GPUI Component controls,
resizable rails, and two-axis scrollbars form the shell.

Preview/Full/Detail now represent distinct raster requests and bounded caches.
The runtime implements the frozen Electron dwell, motion hysteresis, settle,
thumbnail-target, detail, cancellation, stale-result, and lower-quality fallback
rules. Failure recovery is page-local and does not discard the live session.
Deterministic geometry covers normal, actual Electron minimum, issue minimum,
narrow, short, dark, and fractional-scale cases without overlap.

Later Issue #85 evidence adds the application-owned adaptive controller, real
opening and page-render progress/status, application-state synchronization for
both native scroll axes, real pointer rail resizing, a 190 px long-label cap,
exact 24×24 named close Buttons, and exact Lucide Zoom In/Out geometry. The
shallow name adapter applies accessibility metadata to the same pinned real
GPUI Component Button and does not fork it. The pinned graph still has no
portable process-resource sampler or platform event timestamp.

Passed evidence is Linux development-only: focused guards and final all-targets
232 active plus 23 gated ignores in 40 seconds
(`button-probe-20260827T135549Z-3041567.log`), source/guard policy 21/21, exact
prepared digest, one GPUI identity across 870 packages, dependency policy with
warnings but no denial, and green host storage. Failed evidence is fixed
red-first history. Blocked: production PDFium distribution and fresh native
visual/live accessibility on this headless host. Not run: tooltip-popup pixels,
scrollbar-thumb drag, native high contrast, packages, physical devices, screen
reader, IME, Hibbeler/hostile corpora, and matched performance.
