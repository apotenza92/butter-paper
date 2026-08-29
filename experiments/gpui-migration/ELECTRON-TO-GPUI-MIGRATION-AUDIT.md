# Electron-to-GPUI migration audit

This document maps the maintained Electron application at commit
`2dc72b86f3e02a618412dd1fed5a91e0676b283d` to the isolated direct-GPUI
experiment. It describes product behavior, module seams, dependencies,
verification, and a dependency-ordered migration sequence. It is not a worklog
or a claim that the listed behavior already exists in GPUI.

## Executive conclusion

The direct-GPUI experiment is a functional viewer spike with an early owned
control layer. It is not yet an editor migration.

The experiment currently proves:

- a native GPUI process and window on macOS and NVIDIA/X11 Linux;
- owned button, button-group, split-button, separator, menu, icon, and light
  theme controls;
- opening PDFs through a native file dialog or command-line path;
- basic metadata inspection and rasterization through Poppler command-line
  tools;
- multiple selectable and closable tabs;
- thumbnail and page-surface queues;
- continuous and single-page viewing;
- page selection, zoom, Fit Width, Fit Page, and wheel-mode preferences;
- three hard-coded one-page blank PDF sizes;
- visual tool selection state; and
- basic scenario timing events.

The experiment does not contain the maintained document model, annotation
engine, PDF annotation import/export, secure save path, render coordinator,
template library, signature workflows, updater, packaging, or platform
integration. Every visible annotation tool in the GPUI rail is currently only
a selectable icon.

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

The maintained core, PDF, and desktop source set contains 305 files, including
106 focused test files. The repository gate currently reports 1,081 primary
tests plus 23 signature-relay tests. The GPUI gallery currently has 26 Rust
tests plus six foundation-contract tests.

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

| Maintained behavior | Current implementation | GPUI status | Migration requirement |
| --- | --- | --- | --- |
| Stable and beta identities | Distinct product names, package names, application IDs, icons, data directories, feeds, and artifacts | Missing | Preserve distinct identities in every native package and runtime path. |
| Window creation and recovery | Hidden-until-ready window, failure page, renderer-crash handling, background color, constrained geometry | Partial | GPUI opens a window, but recovery and product window policy are missing. |
| Window-state persistence | Validated and clamped size/position with atomic persistence | Missing | Add native per-product state with multi-display validation. |
| macOS full screen | Native state notifications and layout changes | Missing | Implement and test native full-screen transitions without duplicating title chrome. |
| Native/open-file launches | macOS `open-file`, Windows/Linux argv, second-instance routing, focused-window reveal | Partial | Initial command-line paths work; OS events, second-instance behavior, and focus routing do not. |
| PDF file associations | macOS, Windows, and Linux package registration | Missing | Add native package declarations and runtime validation. |
| Default PDF application | Platform-specific request and confirmation behavior | Missing | Replace Electron integration with native platform adapters. |
| Application and document menus | Native application menu plus in-window Nova menu, enablement state, accelerators | Visual only | GPUI draws menu labels but has no complete command/menu state model. |
| Close and quit protection | Dirty-tab and dirty-application confirmation, save/discard/cancel ordering | Missing | Closing must cross the workspace interface and cannot bypass unsaved state. |
| Theme | Native theme snapshot/change events and Nova light/dark tokens | Partial | The GPUI spike has only a fixed light token set. |
| Drag-and-drop PDF opening | Path extraction, main-process authorization, duplicate handling | Missing | Add validated native drop paths and duplicate-tab behavior. |
| Camera permission policy | Camera access restricted to the approved signature flow and renderer origin | Missing | Recreate only if the native signature workflow still needs camera access. |
| Diagnostics | Startup milestones, per-process metrics, system resources and display refresh | Partial | GPUI emits scenario events but lacks maintained diagnostics breadth. |
| Test hooks | Disposable user data, fixture resolution, window control, metrics and startup inspection | Partial | Keep test-only hooks out of release builds and expose equivalent native fixtures. |

### 2. Document workspace and commands

| Maintained behavior | GPUI status | Missing contract |
| --- | --- | --- |
| Open one or many PDFs | Partial | Multi-select dialog, system-open batching, progress, storage-source labels, authorization, duplicate focus, and failure isolation. |
| Multiple document tabs | Partial | Dirty indicators, reorder by pointer and keyboard, focus restoration, accessible tab semantics, horizontal overflow, and close confirmation. |
| Per-tab sessions | Partial | GPUI stores basic `PdfDocument` values but has no deep document-session module, lifecycle cleanup, or inactive-session resource policy. |
| Dirty state | Missing | Revision-based dirty state relative to the last saved revision. |
| Undo and redo | Missing | Bounded per-document past/future history with menu and shortcut enablement. |
| Cut, copy, paste, delete and select all | Missing | Lock-aware selection, stable clipboard clones, page-local select-all, and paste offsets. |
| Save | Missing | Annotation/page-scale/page-rotation serialization and safe source replacement. |
| Save As | Missing | Non-overwrite publication, capability-scoped destination, directory identity checks, atomic durable output, and post-save tab replacement. |
| Page rotation | Missing | Per-page left/right rotation, layout refresh, cache invalidation, history, save and reopen. |
| Status and error feedback | Partial | GPUI reports some document errors but lacks the maintained status lifecycle and action-specific recovery. |
| Keyboard navigation | Partial | Open, page, zoom, and fit shortcuts exist; editing, tabs, menus, close, undo/redo, copy/paste and tool shortcuts do not. |

### 3. PDF engine and render pipeline

| Maintained behavior | Current Electron implementation | GPUI status | Migration requirement |
| --- | --- | --- | --- |
| Source validation | Capability registry owned by the privileged process | Missing | Native file operations must retain owner-scoped validated paths. |
| Open progress | Streaming reads with bytes, source name, phase and estimated time | Missing | Provide observable progress without blocking the GPUI event loop. |
| Metadata and page model | PDF.js/pdf-lib inspection including boxes, rotation and `/UserUnit` | Partial | Poppler text parsing only captures title, count and size. |
| Annotation import | 21 markup variants, source IDs, metadata and untouched fingerprints | Missing | Import known annotations and preserve unknown/untouched annotations safely. |
| Page rasterization | PDF.js canvas/bitmap/blob surfaces | Partial | Poppler PNG subprocesses work as spike evidence, not yet as an accepted packaged engine. |
| Cancellation | Abortable render tasks and stale-result rejection | Partial | GPUI rejects stale generations and limits subprocess time, but cannot cancel work with maintained granularity. |
| Render scheduling | Priority classes, promotion, adoption, visible/prefetch budgets and adaptive concurrency | Partial | The GPUI spike has small FIFO queues only. |
| Page caches | Entry/byte limits, reference counting, retired surfaces, previews and deep-zoom crops | Partial | GPUI has disk PNG reuse but not product cache semantics or memory budgets. |
| Thumbnails | Virtualized, prioritized, cached, annotation-aware previews | Partial | GPUI virtualizes and rasterizes thumbnails but does not overlay annotations or match cache policy. |
| Continuous pages | Visible-range calculation, placeholder/preview/full/detail quality progression | Partial | Basic continuous page surfaces exist; staged quality and motion policy do not. |
| Single page | Page switching, wheel threshold and thumbnail synchronization | Partial | Basic behavior exists and has deterministic state tests. |
| Deep zoom | High-quality and target-crop renders up to the product zoom limit | Missing | Implement quality transitions and bounded memory at 400% and 1600% before claiming parity. |
| Geometry index | PDF content paths, generated page grids and per-page caching | Missing | Required for content and grid snapping. |
| PDF writing | Native annotations, appearance streams, fonts, images, source reconciliation, scale data and rotations | Missing | This is a release-blocking deep module, not a final export helper. |
| Safe publication | Revalidation, symlink rejection, source hash checks, synced temporary output and atomic publication | Missing | Preserve the current integrity guarantees. |
| Blank PDF generation | A0–A5/custom paper plus five patterned paper types and page-grid metadata | Partial | GPUI only writes three fixed blank page sizes without patterns or grid metadata. |

#### PDF foundation decision gate

The Poppler command-line adapter is sufficient for the current viewer spike but
is not yet an accepted production foundation. Before annotation work expands,
choose and prove the native PDF implementation for all supported targets. The
decision must cover:

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

Do not build the annotation UI around Poppler-specific PNG paths. Define the
PDF engine interface first, then place the accepted implementation behind it.

### 4. Viewer and shell behavior

| Feature | GPUI status | Remaining behavior |
| --- | --- | --- |
| Nova visual tokens | Partial | Light colors and several dimensions exist; dark mode, complete typography, elevation, motion and state atlas do not. |
| Button family | Partial | Basic variants, sizes, toggles, disabled state and focus exist; matched hover/focus/accessibility/constrained evidence remains. |
| Split buttons | Partial | Primary/menu separation and two shell consumers exist; placement, overflow, dismissal and full keyboard contracts need platform evidence. |
| Menus/popups | Partial | Small radio-style menus exist; nested menus, typeahead, focus return, collision handling and full application menus do not. |
| Inputs/selects/sliders/switches | Missing | Required by zoom, properties, templates, scale, snap and update settings. |
| Dialogs/popovers/tooltips | Missing | Required by close confirmation, templates, page scale, signatures, updates and destructive actions. |
| Window title and menu bars | Visual only | Product/platform visibility rules, actions and native integration are missing. |
| Tab bar | Partial | Select, close, open, and new-blank controls exist; dirty, reorder, overflow, confirmation and accessibility remain. |
| Viewer toolbar | Partial | Zoom, fit, view mode and wheel preferences exist; direct zoom input, columns/CAD layout, hints, tooltips and responsive overflow remain. |
| Left rail/sidebar | Partial | Page thumbnails and show/hide state exist; resize, constrained overflow, accessibility and exact shell behavior remain. |
| Right rail | Visual only | Tool selection state works; properties, snap, signature, page scale, resizing and mutation-disabled behavior do not. |
| Properties sidebar | Missing | Tool defaults and selected-markup properties are absent. |
| Custom two-axis scrolling | Missing | Product scrollbar geometry, drag math, nested wheel handling and constrained behavior are absent. |
| Page columns/CAD overview | Missing | Columns/rows organization, pages-per-column and overview rendering are absent. |
| Opening and error overlays | Partial | Simple errors exist; progress, placeholders and recovery do not. |
| Responsive/constrained layout | Missing | The native shell has not passed the maintained 800×600 and overflow contracts. |
| Accessibility | Partial | Some controls have focus and labels; roles, relationships, announcements, keyboard ordering and native accessibility inspection are incomplete. |

### 5. Annotation engine

The maintained registry contains two navigation tools, 20 creation tools, and
one non-rail imported-annotation fallback. Every creation tool has a combination
of geometry, hit testing, render primitives, selection chrome, interaction,
property defaults, and PDF mapping. GPUI currently implements none of this
annotation engine.

| Tool family | Maintained tools | Important behavior to migrate | GPUI status |
| --- | --- | --- | --- |
| Navigation | Select, Pan | Hover/focus, shift selection, marquee, moving, Space-pan, cursors | Visual only |
| Rectangular | Rectangle, Ellipse | Click-or-drag placement, fill/stroke, eight resize handles, rotation, locked state | Visual only |
| Text | Text Box | Text layout, editing, fonts, alignment, resize, rotation and PDF font mapping | Visual only |
| Straight paths | Line, Arrow | Click-or-drag, endpoints, arrow appearance, move and PDF mapping | Visual only |
| Arc | Arc | Three-point placement, bulge snapping, reshape handles and sampled path | Visual only |
| Vertex paths | Polyline, Polygon | Multi-click draft, completion, vertices, open/closed geometry and fill | Visual only |
| Ink | Pen, Highlight | Sample collection, smoothing, opacity/blend mode and multi-path export | Visual only |
| Cloud | Cloud | Click/drag node paths, intensity, generated scallops, vertices and PDF intent | Visual only |
| Compound callouts | Callout, Cloud+ | Text box, routed leader, cloud path, obstacle routing, grouped selection and multi-object PDF reconciliation | Visual only |
| Dimension | Dimension | Line offset, caption, endpoints, knee/offset handle and text appearance | Visual only |
| Measurement | Length, Polylength, Area | Page scale, units, precision, captions, multi-point paths and area calculation | Visual only |
| Media | Image, Snapshot | Sanitized asset ingestion or page crop, aspect ratio, resize/rotate, embedding and opacity | Visual only |
| Redaction mark | Redact | Pending redaction geometry and explicit semantics that page content is not yet removed | Visual only |
| Imported fallback | Imported Annotation | Preserve and display unsupported annotations without destructive rewriting | Missing |

#### Shared annotation behavior that must exist before most tools

- A Rust document and markup value model with stable serialization fixtures.
- PDF-to-viewport and viewport-to-PDF transforms for boxes, rotation and
  `/UserUnit`.
- A tool registry that returns geometry, scene primitives, hit results,
  selection chrome, state transitions, properties and PDF mapping.
- Pointer capture and explicit idle, draft, hovered, selected, focused and
  dragging states.
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

| Maintained behavior | GPUI status |
| --- | --- |
| Snap to PDF content geometry | Missing |
| Snap to existing markup geometry | Missing |
| Snap to generated or detected page grid | Missing |
| Endpoint, midpoint, center, intersection and nearest targets | Missing |
| Alignment, equal-size and equal-spacing guides | Missing |
| Configurable sensitivity | Missing |
| Construction grid visibility and spacing | Missing |
| Dimension increments | Missing |
| Per-page preset, custom and calibrated scales | Missing |
| Two-point visual calibration | Missing |
| Decimal and fractional precision | Missing |
| Length, polyline length and polygon area conversion | Missing |
| Scale and rotation persistence in saved PDFs | Missing |

### 7. Template subsystem

The maintained template subsystem is substantially larger than the GPUI menu.

| Maintained behavior | Electron implementation | GPUI status |
| --- | --- | --- |
| Built-in templates | Blank, dots, square grid, ruled, isometric and triangle | Partial: three blank sizes only |
| Paper sizes | A0–A5, portrait/landscape and custom 10–5000 mm dimensions | Partial: Letter portrait/landscape and A4 portrait |
| Pattern settings | Type, spacing presets/custom spacing and color presets/custom color | Missing |
| Page-grid metadata | Embedded structured subject used by snapping | Missing |
| Last-used template | Persistent selection and primary split-button action | Visual selection only; not persistent |
| Custom generated templates | Named records with validation and previews | Missing |
| Imported PDF templates | Managed private source copy and index | Missing |
| Save document as template | Imports the active document into the managed library | Missing |
| Template picker | List, selection, preview, summary, create and manage action | Partial three-item menu only |
| Template manager | Create, inspect, import, remove and preview | Missing |
| Migration from legacy blank settings | Versioned local-storage migration | Missing |
| Safe storage | Atomic index and private source files under product user data | Missing |

### 8. Signature subsystem

| Maintained behavior | GPUI status |
| --- | --- |
| Typed, drawn, camera/photo and image signature appearances | Missing |
| Phone transfer sessions and authenticated encrypted envelopes | Missing |
| Relay polling, expiry, cancellation and ownership | Missing |
| Image sanitization in an isolated runtime | Missing; native replacement requires a new threat model |
| Recent-signature encrypted store | Missing |
| Add, remove and clear recent signatures | Missing |
| Signature menu and accessible subflows | Missing |
| Placement through the image tool | Missing |
| PDF image embedding and save/reopen | Missing |

### 9. Update, release and packaging

| Maintained behavior | GPUI status | Migration requirement |
| --- | --- | --- |
| macOS DMG and ZIP, Windows NSIS, Linux AppImage/DEB/RPM | Missing | Select native packaging without changing artifact contracts casually. |
| ARM64 and x64 | Partial build evidence | Produce and verify every maintained target. |
| Signing and notarization | Missing | Preserve Developer ID, hardened runtime, Windows signing and offline key policy. |
| Stable/beta isolation | Missing | Preserve distinct IDs, names, data, caches and feeds. |
| TUF trust on Windows/Linux | Missing | Port reviewed-root validation and rejection behavior. |
| macOS updater | Missing | Replace Electron updater while preserving native N-1 requirements. |
| Update frequency/status UI | Missing | Port policy, state machine, dialogs and menus. |
| Release asset contracts | Electron-specific today | Rewrite tests around native artifacts before cutover. |
| THIRD_PARTY_NOTICES and license gate | Decision gate | Resolve GPUI transitive GPL chain and generate native dependency notices. |

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

1. Complete M0 research for the production PDF engine and native packaging.
2. Record the accepted PDF engine and document-schema decisions.
3. Create `butter_model` fixtures shared with the maintained TypeScript model.
4. Refactor the experiment so `Gallery` consumes `document_workspace` and
   `viewer_session` instead of owning all state directly.
5. Implement M4 Rectangle as the first end-to-end editor slice.

Templates can proceed after the blank-PDF and storage portions of M0–M2 are
settled. The remaining tool icons should not receive more visual polish until
the shared annotation engine and Rectangle slice exist.

## Evidence still blocked or not run

- The Hibbeler corpus is not present in this checkout.
- Current matched Electron/macOS captures were not transferred.
- Native macOS accessibility inspection is not current.
- Windows native GPUI behavior has not been qualified.
- A packaged native candidate does not exist.
- GPUI-CE is pinned as the immutable compatible foundation candidate. Native
  compilation, accessibility, runtime, and packaging qualification remain open
  on macOS, Windows, Linux ARM64, X11, and Wayland.
