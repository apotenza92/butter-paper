# GPUI migration stack audit

Date: 2026-08-24; cutover-readiness update: 2026-08-25

This audit records the historical governance reset and the durable migration
architecture. GitHub spec #82 and its dependency-linked tickets now control
execution. This document is not a per-slice approval boundary. It does not
promote the experiment, change production Electron code, or approve a
distributable native candidate.

## Current foundation reconciliation

As of 2026-08-27, the isolated application candidate is the prepared
Longbridge GPUI Component revision
`c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4` on exact Zed GPUI revision
`8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc`. The prepared source digest is
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`.
The graph contains one GPUI identity. The checksum-bound Apache-2.0 tracing
shim replaces the rejected GPL-marked tracing chain without copying its code.
PDFium remains a checksum-pinned development dependency with
`productionApproved: false`.

The GPUI-CE gallery and its foundation policy are historical evidence only.
Where older sections below describe an active GPUI-CE gallery, read them as a
preserved account of the predecessor experiment, not the current candidate.
The combined deterministic foundation suite rejects conflicting candidate
guidance, revision or license drift, malformed or checksum-drifted source
receipts, a second resolved GPUI identity, and production approval of the
development PDFium artifact. `foundation-truth.mjs` checks the documentation,
policy identities, active manifest pins, receipt shape, and PDFium boundary.
`prepare.mjs verify` checks source contents and receipt checksums.
`verify-cargo-graph.mjs` checks resolved dependency identities and sources.
Execution evidence is recorded on GitHub issue #83.

## Historical preserved checkpoint

The following state was recorded before the governance reset. It is not a
current resume instruction.

- Branch: `codex/gpui-component-migration-spike`
- HEAD: `2dc72b86f3e02a618412dd1fed5a91e0676b283d`
- Worktree: dirty; every existing tracked and untracked change is preserved as
  user work.
- Current slice files:
  `gpui-gallery/src/annotation_model.rs`,
  `gpui-gallery/src/annotation_adapter.rs`,
  `gpui-gallery/src/main.rs`, and
  `gpui-gallery/tests/annotation_adapter.rs`.
- Last completed verification: `cargo test --features gallery -q` passed for
  the complete `gpui-gallery` suite after the rotated-rectangle spatial-index
  and zoom-scaled rotation-handle changes.
- Current failing test: none. The two test-first failures were resolved before
  this checkpoint.
- Live test processes: none. No paid resource was created for this reset.
- Safe resume seam: resolve and prove one coherent, distributable
  GPUI/GPUI-Component dependency graph before replacing any Butter Paper UI.

## Component-system decision

Longbridge GPUI Component is now the default component system for the isolated
migration. Use a library component whenever the reviewed source supplies the
required semantic control. Keep Butter Paper wrappers shallow and product
specific. Use `gpui-base` for reusable difficult behavior or geometry when the
application must own presentation. A hand-built primitive requires a recorded
capability gap in the parity ledger.

The reviewed upstream source is pinned to:

| Item | Exact value |
| --- | --- |
| Repository | `https://github.com/longbridge/gpui-component` |
| Revision | `c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4` |
| Commit date | `2026-08-24T19:06:53+08:00` |
| Component package | `gpui-component 0.5.2` |
| Base package | `gpui-base 0.5.2` |
| Upstream locked Zed revision | `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc` |

Do not follow either repository's moving `main` branch.

## Phase 0 compatibility result

The active gallery still uses `gpui` and `gpui_platform` from GPUI-CE revision
`c738623ffbcec2aeddc44a645cc6b74646d5cf97`. It has no active
`gpui-component` or `gpui-base` dependency. Its lockfile and foundation policy
match that source.

The reviewed Longbridge source is not API compatible with GPUI-CE. The approved
probe therefore uses the exact Zed revision required by Longbridge and keeps
that graph isolated in `gpui-component-compat/`; it does not load both GPUI
identities into one application.

The probe prepares the reviewed Longbridge tree deterministically. Its policy
pins the component and Zed revisions and trees, license files and checksums,
preparation patch checksum, allowed Git sources, asset provenance, and final
prepared-tree digest. The minimal patch removes the forced `profiler` and
`runtime_shaders` features. A local Apache-2.0 shim supplies the only reachable
`ztracing::instrument` interface without copying or disguising GPL source. The
resolved graph contains no `zlog`, `ztracing_macro`, or forbidden feature.

The dependency and policy gates pass: nine deterministic preparation/storage
tests, one GPUI identity across 851 packages, exact Git revisions, configured
advisory/license/source checks, and the locked Linux x86_64 all-targets build.
The license audit retains explicit warnings for two exact Zed crates without a
manifest license field; checksum-bound Apache-2.0 clarification evidence is in
the policy. Upstream unmaintained-package warnings also remain visible.

The sentinel initializes `gpui_component`, installs `Root`, renders and
activates a real `Button`, then renders a real two-child `ButtonGroup` and
verifies the exact single-selection result. The next test-first tracer renders
a real `DropdownButton`, activates its primary half, opens its real `PopupMenu`,
verifies focus transfer, selects Zoom by keyboard, updates feature-owned state,
and verifies stable split/primary identities after rerender. The toolbar tracer
then composes retained Continuous and Single Page entities with a controlled
real fit `ButtonGroup` and proves exclusive view selection, independent wheel
settings, pointer, keyboard menu, Escape/focus-return, disabled, both
double-click, and horizontal-overflow behavior. A native story using those same
paired entities compiles as part of the all-targets gate. Native launch, visual
capture, accessibility inspection, packaging, and redistribution review remain
separate gates; compilation is not visible acceptance.

The paired-toolbar cold all-targets run used one Cargo job, disabled incremental
compilation, passed in 459 seconds, peaked at about 1.22 GiB resident memory and
a 2.52 GiB disposable target, and kept about 109 GiB free. The exact paired
story extension then passed from the retained target in 5 seconds with about
1.17 GiB peak resident memory. The local guard now requires
30 GiB before build and stops below 20 GiB while retaining an 18 GiB absolute
floor. The current runner retains a valid owned target after success, ordinary
Cargo failure, timeout, interruption, or memory stop. It cleans automatically
only for free-space or target-size safety breaches; explicit cleanup remains
available for corruption or storage recovery. See
`gpui-component-compat/README.md` for fixed runner modes, reproduction commands,
and evidence categories.

## Installed skills and discovery

Exact portable copies of the reviewed upstream bundles and all their
references are present at:

- `.agents/skills/gpui/`
- `.agents/skills/gpui-component/`

The copied content matches revision `c27f5d5c…`; two reference files differ
only by a normalized final newline. Both `SKILL.md` files and the relevant
design and coding guides were read before this plan.

The current agent session discovers both bundles from those project-local
paths. Their `SKILL.md` files plus the relevant GPUI context, testing,
`ElementId`, usage, design, and coding guides were read before the probe and
story changes.

The upstream `.claude/skills/gpui-component-dev` bundle was reviewed for new
component, story, documentation, and pull-request workflows. It is not
installed. Its `description` frontmatter contains an unquoted colon that a
strict YAML parser can reject. Butter Paper should not copy that contributor
skill unless it must change the upstream library; ordinary application stories
are already covered by the portable GPUI Component guidance.

No extra Butter Paper orchestration skill is justified yet. The two upstream
skills, test-driven-development workflow, cross-platform desktop-app workflow,
this audit, and the parity ledger already give one reviewable route without
duplicating upstream material.

## Source-confirmed component mappings

| Butter Paper surface | Default reviewed API | Product-owned layer |
| --- | --- | --- |
| Ordinary and icon actions | `button::Button` | command labels, AEC icons, shortcuts |
| Joined selection controls | `button::ButtonGroup` | fit/view domain state |
| Split action plus settings | `button::DropdownButton` | action and wheel/template commands |
| Menus and popup commands | `menu::{DropdownMenu, PopupMenu}` | enablement and command policy |
| Tooltips | `tooltip` plus `Root` | accessible product copy |
| Tabs | `tab` | dirty/save/close/reorder document policy |
| Resizable shell panes | `Resizable` / `gpui-base` resize behavior | pane persistence and minimums |
| Scrolling | `Scrollable`, scrollbars, virtual lists | PDF viewport and page-cache policy |
| Fields | `Input`, `Select`, `Slider`, `Switch` | validation and document properties |
| Transient decisions | `Dialog`, `AlertDialog`, `Popover`, `Sheet` | save/close/template workflows |
| Notifications | `notification` through window `Root` | document and operation status |
| PDF/annotation canvas | no general component equivalent | Butter Paper domain UI on GPUI/Base |

Every exact signature must still be taken from revision `c27f5d5c…` source
when a slice starts. React or historical GPUI examples are not API evidence.

## Cutover-readiness reset

The pointer-drag and fast-runner milestones are accepted as Linux
development-only evidence. Pointer edge auto-scroll is not the next slice.
Control-by-control parity has proved the component stack, but it does not prove
that Butter Paper can open, edit, save, or recover a real document through that
stack. Further work is ordered by complete document journeys.

### Critical-path matrix

`Already real` means that the named implementation performs the operation with
real document data. It does not imply that the operation is integrated into the
cutover candidate.

| Journey | Shipping Electron | GPUI-CE gallery | GPUI Component compatibility app | Cutover status | Linux development evidence needed | macOS / Windows package evidence needed |
| --- | --- | --- | --- | --- | --- | --- |
| Open a PDF and preserve an open failure | Already real | Already real through the isolated PDFium worker; error is gallery-global | Stable-ID retained session, real worker adapter, and story compile. The checksum-pinned Linux worker opens the public fixture; a failed second open preserves the first worker and page state; deterministic stale-result rejection passes | Partial | Add the native file-open surface and fresh graphical/accessibility evidence without changing the dependency graph | Exact packaged file dialog, file association/system-open, sandbox/capability, malformed/password PDF, and recovery proof |
| Render pages, navigate, and use thumbnails | Already real | Already real: bounded viewport/tile rasters, cached thumbnails, virtual lists, page and zoom navigation | Real non-uniform page pixels and 12 thumbnail buffers pass; clicking the rendered stable-ID second thumbnail advances the retained workspace through the real background worker; PID exit and mapped-surface cleanup pass; cancellation is deterministic | Partial | Prove native scrolling/cache pressure and collect fresh graphical/accessibility evidence | Packaged Metal/DirectX/Linux backend visual, scroll, scale, memory, accessibility, and device-input proof |
| Select and create one representative annotation | Already real across the maintained tool model | Already real for the five representative families; Rectangle is the strongest complete interaction | Missing on a real document | Partial | Rectangle selection and creation run against the shell-owned session, update the page and thumbnail scene, and set one shared dirty revision | Packaged pointer/keyboard/accessibility proof plus matched PDF-coordinate and visual evidence |
| Save, reopen, and preserve document data | Already real | Real two-cycle `lopdf` adapter proof, but not connected to the visible gallery session or close flow | Missing | Partial | The exact session snapshot is saved to a new path, independently validated, reopened through PDFium, compared, and marked clean only after successful reopen | Audited writer/PDFium packages, native dialogs and permissions, crash-safe publication, corpus compatibility, signing, and platform-viewer checks |
| Switch and close documents while preserving dirty/failure state | Already real | Real multi-document open/switch/release; visible close removes a dirty document without confirmation | Real component interactions over experiment-only counters and mock tabs | Partial | Two real sessions keep page/annotation/error state independent; clean close releases resources; dirty close routes through real Save/Discard/Cancel intents without loss | Packaged session restoration, OS close/quit, focus/accessibility, crash recovery, and physical-device proof |
| Distribute the native application | Already real Electron packages remain the rollback product | Development bundle scripts only | No package | Blocked | A coherent Linux development runtime is necessary but not sufficient | Exact stable/beta macOS and Windows candidates, PDFium provenance, signing/notarization/installers, updates, N-1, and rollback must pass |

### Consolidation decision

Do not place the two GPUI identities in one process. `Entity`, `Window`,
`IntoElement`, `RenderImage`, focus handles, and component state from GPUI-CE
revision `c738623f…` cannot cross the boundary to the Zed GPUI revision
`8b1497db…` required by Longbridge. A direct wrapper around the current gallery
UI is therefore impossible.

Consolidation is nevertheless viable without forking either upstream:

1. Keep the prepared Longbridge/Zed graph as the sole UI graph.
2. Consume `butter-paper-gpui-gallery` as a path dependency with default
   features disabled, or extract the same modules into experiment-owned feature
   crates. The manifest's GPUI-CE dependencies are optional; a downstream
   no-default-features library dependency does not activate them or the
   gallery's GPUI-CE dev-dependencies.
3. Share the GPUI-free `annotation_model`, `annotation_adapter`,
   `annotation_paint_path`, `pdf_engine`, `pdf_worker`, `viewer`, and focused
   scenario/oracle code. Keep these below the application and UI layers.
4. Split the binary-local `pdf_document.rs`: retain the worker/session,
   cancellation, cache-key, and bounded BGRA surface behavior below the UI;
   replace its GPUI-CE `RenderImage` conversion with a small Zed-GPUI adapter.
5. Do not share `main.rs` or the hand-built `butter_ui` layer. Port their
   product behavior into feature-owned entities rendered by GPUI Component.

The exact application seam is a `DocumentWorkspace` entity that owns stable
document IDs and `Entity<NativeDocumentSession>` children. Each session owns a
lifecycle state (`opening`, `ready`, `saving`, or `failed`), the PDF worker
capability, viewer/cache state, annotation state and saved revision. The shell
observes immutable session snapshots and dispatches commands; it never owns PDF
or annotation data. Background PDF work returns identity- and revision-tagged
results to the foreground session, which rejects stale results and notifies
once. Save publishes through `PdfPersistenceSession`, reopens the published
file through the worker, and marks the annotation revision saved only after the
reopen and compatibility checks succeed. A failure keeps the prior ready
snapshot and dirty state available. Tab selection and close actions address a
stable document ID, not a vector index.

This is a source-level reuse boundary, not a claim of complete model
compatibility. The maintained TypeScript model has 21 markup variants; the Rust
model has five representative annotation families. Page rotation now has an
application-owned revision, raster/geometry, canonical `/Rotate`, and
independent-reopen development proof. Page scale has a partial calibrated
schema. Both still need the full differential corpus and packaged platform
proof. Metadata, vendor annotations, and unsupported-document behavior still
need an explicit compatibility schema before the Rust model can own production
files.

No new development-license blocker was found for this integration shape. The
prepared Longbridge/Zed policy is checksum-bound and currently passes its
configured gate. Production redistribution remains open because the local
source preparation and tracing shim need release review. PDFium is a harder
distribution block: the pinned `bblanchon/pdfium-binaries` artifacts are marked
`productionApproved: false`, and `pdfium-render` is pinned to an unpublished
Git revision. A distributable candidate needs an audited application-owned
PDFium build, complete license notices, native library loading and signing, and
qualification on every supported target. Windows inherited source-handle
transfer is also not implemented.

### Ranked end-to-end slices

| Rank | Slice | Cutover value | Risk | Exit evidence |
| --- | --- | --- | --- | --- |
| 1 | Read-only native document spine in the GPUI Component shell | Highest: joins the two real halves and invalidates the largest architecture risk | Medium: Zed API port plus worker/image boundary, but no writer or destructive action | A runnable story opens the public multi-page fixture, renders page 1 and thumbnails, activates a thumbnail to show another page, reports a failed second open without losing the first document, then closes and releases the worker. A deterministic backend test proves the same state transitions and stale-result rejection. |
| 2 | Rectangle edit, dirty state, Save As, and reopen | Very high: proves the first non-destructive editor journey | High: model mapping, PDF publication, PDFium refresh, and compatibility oracles converge | One rectangle is selected or created, the page and thumbnail update, dirty state changes once, Save As survives independent validation and reopen, and failure leaves the original session dirty and usable. |
| 3 | Two-document workspace and protected close | High: proves the application can preserve work during normal document management | Medium-high: resource ownership, active-session focus, async failures, and close decisions | Two real PDFs keep view/annotation/failure state independent; switching is stable by ID; clean close releases resources; dirty Save/Discard/Cancel routes to the correct session without accidental loss. |

Ranks 1 through 3 now pass as Linux development-only evidence. Rank 2 uses a real
GPUI Component Rectangle tool, native GPUI pointer delivery, the shared
annotation adapter, page and thumbnail presentation, retained dirty revision,
transactional Save As, typed lopdf reopen, `qpdf`/`pdfinfo`, independent PDFium
reopen, changed page pixels, and worker swap/cleanup. An injected save/reopen
failure preserves the original live resource and dirty state. The exact-source
all-targets gate now passes 44 non-ignored tests in 18 seconds with the target
retained. Rank 3 adds real GPUI Component session tabs and an exact-copy
three-action dirty-close Popover. Deterministic tests cover Cancel, Discard,
failed and successful close-save, successor choice, and isolated resource
release. The real worker journey keeps two PDFium sessions alive and proves a
clean close kills only its own worker. The bounded visible viewer slice now
passes with per-session mode/zoom/scroll state, a two-job async queue, raster
generation, a 256 MiB cache, stable rendered tile IDs, real thumbnail-driven
page-0/page-1 transitions, and resource-swap invalidation. Imported Rectangle
reconciliation now also passes: clean real-PDF hydration, retained selection,
native-pointer move/resize previews, property/lock/delete controls, undo/redo,
exact `/Annots` deletion, Save As, independent typed reopen, and resource
replacement. The next slice is full presentation/edit/save reconciliation for
the remaining typed annotation families. Keep it inside `gpui-component-compat` and
keep ordinary red failures warm.

## Phased execution plan

### Phase 0: coherent foundation — development probe passed

The exact source-prepared dependency graph, real Button, real ButtonGroup, and
native story compile in the isolated probe. This resolves the crate-identity
and minimum compilation question. It does not promote the graph to the gallery
or prove native runtime, accessibility, packaging, redistribution, or release
fitness.

### Phase 1a: split-control compatibility probe — development probe passed

The representative Continuous-view control now uses the real pinned
`DropdownButton` and `PopupMenu` APIs behind a shallow feature-owned state
wrapper. Its deterministic tracer and compiled story pass. This does not yet
replace the gallery or prove the full Electron contract, native visuals,
accessibility, minimum-window behavior, or platform breadth.

The probe also confirmed two component gaps at this revision. The internal
caret uses a fixed `popup` ID without a product accessibility identifier,
expanded state, tooltip seam, or radio-menu semantics. Menu arrow navigation
also considers disabled and label rows selectable because it filters only
separators. Standard `PopupMenuItem` rows also expose no product `ElementId` or
accessibility-ID builder. The representative menu therefore contains only two
standard actionable items, preserves their exact labels and result, and does
not hide these gaps behind custom rows.

### Phase 1b: smallest viewer-toolbar strip — development probe passed

The experiment now composes a controlled real fit `ButtonGroup` with the
retained Continuous `DropdownButton` behind `ViewerToolbarStrip`. Feature state
stays in the experiment-owned entities; the GPUI Component controls own their
presentation, focus, disabled, selection, and menu behavior.

The red-green rendered tests prove the exact toolbar/scroll/content/group/
action/split IDs, fit and Continuous primary pointer changes, real menu
selection, Escape dismissal and focus return, post-dismissal key isolation,
disabled suppression, and Continuous double-click Fit width without a second
primary activation. The non-wrapping overflow policy matches Electron: 720 px
normal and 480 px minimum-supported strips show every target; a 320 px
constrained strip keeps target sizes and ordering without overlap and scrolls
the full Continuous target into view.

No existing X11/Wayland graphical session or display socket exists on this
VPS, so native screenshot and live accessibility evidence are blocked rather
than inferred from the compile. The disabled/label-row arrow-navigation
behavior remains a limitation of the pinned component revision. Butter Paper
does not use disabled headings as actionable product behavior in this probe.

### Frozen Electron contract: paired page-view controls

The Single Page slice is frozen from `ViewerToolbar.tsx`, its toolbar tests,
and `viewerStore.ts` before GPUI implementation:

- primary action `viewer-scroll-single-page` selects `single-page` scroll mode
  and disables page columns; selected styling is visible only while the viewer
  is enabled and Single Page is the active mode;
- its settings trigger is `viewer-scroll-single-page-settings`; the radio
  choices are `viewer-single-page-wheel-zoom` (`Mousewheel Zoom`) followed by
  `viewer-single-page-wheel-scroll` (`Mousewheel Scroll`), with Single Page
  defaulting to Zoom independently of Continuous View's Scroll default;
- disabled state removes selected presentation, disables both split segments,
  closes an open menu, and suppresses primary, menu, and double-click changes;
- a recognized primary double click invokes Fit Page and hides its gesture
  hint; the ordinary primary action remains the view-mode selection command;
- Escape dismisses the settings menu and returns focus to its trigger through
  the standard menu contract; selection closes the menu and retains the chosen
  wheel behavior;
- Continuous and Single Page retain independent wheel behavior. Their active
  view selection is exclusive and remains coherent after either primary action.

The GPUI probe uses stable experiment IDs `single-page-view-split` and
`single-page-view-primary`, plus the already frozen Continuous IDs. It uses the
real pinned `DropdownButton` and actionable `PopupMenuItem` rows. The upstream
disabled/label-row arrow-navigation limitation is excluded from product
behavior rather than reproduced.

### Phase 1c: paired page-view controls — development probe passed

The retained control seam is now shared by Continuous and Single Page while
the toolbar remains the owner of exclusive view selection. Continuous retains
its Scroll default and Fit width double click. Single Page retains its
independent Zoom default and Fit page double click. Both real
`DropdownButton`/`PopupMenu` paths pass stable-ID, pointer activation, keyboard
menu selection, Escape focus return, disabled suppression, and post-dismissal
key-isolation checks.

The initial paired red tracer failed because Single Page was not rendered. The
first green implementation passed all behavior but measured a 509 px strip and
failed the frozen 480 px supported-width gate. Applying the library's standard
small toolbar size at the composition owner reduced the resolved strip without
shrinking targets. The final 720/480/320 px run passes with no overlap or
clipped target and scrolls the complete Single Page split into view at 320 px.
Both failed probes cleaned only the exact disposable target.

No existing X11/Wayland graphical session, display variable, display socket, or
compositor process exists. The exact paired story therefore compiles but has no
fresh screenshot or live accessibility tree. No synthetic display was created.

### Frozen Electron contract: zoom controls

The zoom slice is frozen from `ViewerToolbar.tsx`, `app.tsx`,
`renderZoom.ts`, and the toolbar interaction tests before GPUI implementation:

- source order is Zoom Out, Zoom In, then the single percentage/menu trigger;
  stable action IDs are `viewer-zoom-out`, `viewer-zoom-in`, and
  `viewer-zoom-menu`;
- ordered presets are 6.25%, 10%, 25%, 50%, 75%, 100%, 125%, 150%, 200%,
  400%, 800%, 1600%, 3200%, and 6400%. Their numeric values are 0.0625, 0.1,
  0.25, 0.5, 0.75, 1, 1.25, 1.5, 2, 4, 8, 16, 32, and 64;
- Zoom Out divides by 1.1 and Zoom In multiplies by 1.1. All updates clamp to
  0.0625 through 64 and round to three decimal places. A non-finite input
  clamps to 0.0625;
- the percentage trigger shows the current value. Values below 10% show at
  most two percentage decimals; other values show the nearest integer percent.
  A menu row is selected only when its preset differs from the current zoom by
  less than 0.001. A valid non-preset value therefore has no selected row;
- the two step buttons remain enabled at the numeric bounds. A step beyond a
  bound is a clamped no-op. The global viewer-disabled state disables all three
  controls, closes an open menu, and suppresses pointer and keyboard changes;
- ordinary buttons use standard pointer, Enter, and Space activation. The menu
  uses standard arrow, Enter, and Escape behavior; selection dismisses it and
  Escape returns focus to the percentage trigger. Application-wide
  Command/Control plus, minus, and zero shortcuts are outside this component
  slice and are not silently recreated here;
- double-clicking the percentage trigger closes the menu and resets zoom to
  100%. There is no separate reset button. Zoom Out and Zoom In define no
  custom double-click behavior, so the probe must not invent one;
- preset-row test IDs follow `viewer-zoom-preset-{rounded percent}`. The pinned
  standard `PopupMenuItem` cannot carry a product `ElementId`, so the probe
  preserves exact labels, order, checked state, and selected result while
  recording that row-ID capability gap.

The controlled value and limits remain in the experiment-owned retained zoom
entity. Presentation uses real GPUI Component `Button`, `Popover`, and
`PopupMenuItem` APIs. The percentage trigger is not a split button because the
Electron contract has one trigger surface.

### Phase 1d: controlled zoom group — development probe passed

The experiment-owned `ZoomControl` retains the numeric value, limits, disabled
state, controlled menu-open state, and reset count. Its presentation uses real
GPUI Component `Button`, `Popover`, and `PopupMenuItem` APIs. A shallow
controlled-Popover composition is necessary because the convenience
`DropdownMenu` trait does not expose controlled open state, which Electron's
disabled and double-click-reset contracts require.

The rendered red tracer first failed at the absent `viewer-zoom-out` ID. The
green tracer proves all three stable IDs, pointer stepping, Enter/Space button
activation, keyboard preset selection, exact and non-preset checked-state
semantics, percentage formatting, three-decimal rounding, clamping and bound
no-ops, Escape dismissal with focus return to the prior toolbar focus owner,
disabled suppression, and the recognized percentage-trigger double click that
closes the menu and resets to 100%. Fit selection, view selection, and both
independent wheel settings remain unchanged after zoom interactions. Exact
pointer-open restoration to the percentage trigger is not claimed: this GPUI
Button prevents pointer focus and Popover restores the previous focus owner.

The expanded content resolves to exactly 607 px in the deterministic fixture.
All targets fit at 720 px. The 480 px center-strip allowance and 320 px
constrained width are therefore intentional non-wrapping horizontal-overflow
cases. Both preserve exact target sizes and ordering with no overlap or clipped
interactive target, and scrolling brings the complete trailing Single Page
control into view.

The guarded cold all-targets run passed in 455 seconds with about 1.22 GiB
maximum resident memory and a 2.52 GiB disposable target. The exact zoom story
extension then passed from the retained target in 5 seconds. No existing
X11/Wayland session, display variable, display socket, or compositor exists, so
fresh native screenshot and live accessibility evidence remain blocked rather
than inferred from compilation.

The repository deterministic gate also passes: hygiene, generated icons,
TypeScript type checking, all package builds, 144 Vitest files with 1,088 tests,
and the signature-relay's 23 tests. The host uses Node 22.22.1 while the project
declares Node 24.16.0, so pnpm reports an engine warning even though the gate
exits successfully.

A final warm guarded run passed in 10 seconds after the percentage formatter
was made to use JavaScript-compatible half-up rounding and the 112.5% to 113%
edge case was added to the tracer.

Resolved development failures remain evidence: the first green compile found
only overlapping mutable borrows in the new test helper; the next run exposed
the intended 607 px overflow threshold; and two keyboard runs proved that
focus tracking alone was insufficient and that the zoom subgroup must use a
real tab-group boundary plus a draw after focus changes, matching GPUI's own
Button test contract. Every failed run cleaned only the exact disposable
target.

### Frozen Electron contract: CAD View controls

The CAD View slice is frozen from `ViewerToolbar.tsx`, `app.tsx`,
`viewerStore.ts`, `featureFlags.ts`, and their existing tests before GPUI
implementation:

- the complete control is absent unless the developer-only
  `BP_CAD_VIEW_ENABLED=1` feature flag is set. If the flag becomes unavailable,
  the app clears page-column mode; the experiment story explicitly enables the
  representative control instead of changing that product flag policy;
- the primary action has ID `viewer-cad-view`, accessible name and tooltip
  `CAD View`, and uses a pressed state only while the viewer is enabled,
  continuous mode is active, and page columns are enabled. Activating it enters
  continuous mode and enables page columns. Activating the already selected
  control does not turn CAD View off; another page-view control performs that
  transition;
- the settings trigger has ID `viewer-cad-view-settings`, accessible name and
  tooltip `CAD View settings`, and shares the active selected presentation. It
  opens a contextual Popover, not an action menu. The content ID is
  `viewer-cad-settings`, its title is `CAD View`, and its description is
  `Organise drawing sheets. Mousewheel always zooms in CAD View.`;
- configuration order is the single-select `Organise by` group followed by the
  page-count field. The group choices are `Columns` then `Rows`, with IDs
  `viewer-cad-organisation-columns` and `viewer-cad-organisation-rows`.
  Pointer selection and unmodified Left/Right arrows wrap; Home selects Columns
  and End selects Rows. Modified arrow input has no CAD-specific meaning;
- the count label is `Pages/column` in Columns mode and `Pages/row` in Rows
  mode. The corresponding IDs are `viewer-pages-per-column` and
  `viewer-pages-per-row`. The integer range is 1 through 100, step 1, with a
  default of 10. Finite changes are rounded and clamped; a non-finite store
  update resets to 10;
- global viewer-disabled state removes active presentation, disables both
  triggers, closes an open Popover, and suppresses primary and configuration
  changes. Escape and outside dismissal close the Popover. Electron explicitly
  returns focus to the settings trigger through `finalFocus`;
- CAD state belongs to the viewer store. It resets to inactive, Columns, and 10
  when a document is loaded. It is otherwise captured and restored per open tab
  while that tab remains in the application; no durable cross-launch
  persistence is defined by these sources;
- enabling page columns also changes the zoom preset to `manual`. CAD
  organisation and page count do not change fit selection, zoom value, either
  wheel-behavior preference, or the inactive page-view preference;
- no CAD-specific double-click behavior, application shortcut, or explicit
  accessibility description for the count field is defined. The GPUI probe
  therefore treats a double click as the ordinary primary command once and
  does not invent a fit command or shortcut.

The agreed public test seams are the retained CAD entity state and the rendered
stable IDs/bounds inside `ViewerToolbarStrip`. Exact native accessibility and
focus restoration remain separate native evidence gates rather than inferred
from deterministic rendering.

### Phase 1e: CAD View control — development probe passed

The experiment-owned `CadViewControl` keeps activation, organisation, page
count, disabled state, and document reset in retained application state. It
renders the real GPUI Component `Button`, controlled `Popover`, `ButtonGroup`,
and `NumberInput`. `ViewerToolbarStrip` coordinates CAD with the paired page
views while leaving fit, numeric zoom, and both wheel preferences independent.
The native story uses the same entity and composition.

Deterministic rendered tests prove the frozen primary and settings IDs,
selected-primary no-op, ordinary double-click activation, pointer and keyboard
Columns/Rows transitions, dynamic page-count identity, step changes, clamp
function, Escape dismissal and return to the prior toolbar focus owner,
disabled suppression, and reset to inactive/Columns/10. They also prove that a
standard page-view activation exits CAD and that CAD interactions do not alter
fit, numeric zoom, or either wheel preference. The test does not claim the
production zoom-preset=`manual` store side effect because the isolated strip
does not yet own that product store boundary.

The expanded toolbar has a measured intrinsic width of 667 px with CAD, versus
607 px without it. All targets fit at 720 px. The 480 px and 320 px fixtures
retain the explicit non-wrapping horizontal-scroll policy: every target keeps
its size, stays inside the scroll content, does not overlap, and the trailing
CAD controls become fully visible after scrolling.

The final guarded Linux x86_64 all-targets run passed 3 component-stack tests
and 8 toolbar tests in 492 seconds from an empty disposable target. It used one
Cargo job, disabled incremental compilation, about 1.22 GiB maximum resident
memory, a 2.52 GiB target, and left about 109 GiB free. The exact CAD story
extension then passed from the retained target in 7 seconds. This is
development-only evidence.

Resolved failures remain recorded. The stable-ID and configuration tracers
first failed on the absent CAD targets. Early green compiles found missing
extension imports and the current `InputState::value()` signature. The full
contract tracer failed on the absent document-reset seam. The first geometry
assertion treated nested split-primary bounds as siblings; after that was
corrected, the test measured the exact 667 px threshold instead of the 675 px
estimate. Every failed Rust probe removed only the allowlisted disposable
target.

Known component gaps remain explicit. The pinned `ButtonGroup` does not provide
the Electron organisation group's roving Left/Right/Home/End selection or
radio-group semantics, so a shallow application-owned handler supplies those
frozen keys. The pinned GPUI `Button` prevents pointer focus, so deterministic
Escape proof returns to the prior toolbar focus owner rather than proving
Electron's exact pointer-open restoration to the settings trigger. Native
roles, count-field semantics, outside-click focus behavior, and IME are not
inferred from compilation. The exact tooltip/title/description strings are
bound and tested, but the pinned icon-only `Button` derives an accessibility
label only from a visible label and exposes no separate label setter. Its
tooltip does not name it, so the missing CAD icon-button accessible names are
recorded as an upstream capability gap rather than hidden with a custom button.

### Frozen Electron contract: Document Tab Bar template split action

The template split slice is frozen from `DocumentTabBar.tsx`,
`TemplatePickerPopover.tsx`, `LastTemplatePreviewTooltip.tsx`,
`ClosableDocumentTab.tsx`, `templateLibrary.ts`, `app.tsx`, and their focused
tests before GPUI implementation:

- the control is always present in the Document Tab Bar's trailing
  `Document actions` region, after `Open PDF`. It does not belong in the viewer
  toolbar. The actions region never shrinks; only the document-tab list owns
  horizontal overflow. The maintained window defaults are 1200×800 with a
  900×600 minimum;
- the primary action ID is `document-tab-new-pdf`. It is an icon-only action
  with accessible name `New from {current template name}`. Its larger preview
  tooltip shows the current template and `Click to create`. Activation creates
  from the persisted last template. Success appends a new temporary dirty tab,
  activates it, and only then records that template as the last-used template;
  existing tabs and their state remain unchanged. Failure does not update the
  last-used template;
- the picker trigger ID is `document-tab-template-picker`, accessible name
  `New from template`, and tooltip `New from template…`. It opens contextual
  Popover `template-picker`, titled `New from template`, and explicitly returns
  focus to the picker trigger when dismissed;
- each time the Popover opens, its transient selection resets to the persisted
  last template. Template rows are ordered as six built-ins—`built-in-blank`
  (`Blank Paper`), `built-in-dots` (`Dot Grid`), `built-in-grid` (`Square
  Grid`), `built-in-lined` (`Ruled Paper`), `built-in-isometric` (`Isometric
  Grid`), and `built-in-triangle` (`Triangle Grid`)—followed by custom templates
  and then imported templates. Row IDs are
  `template-picker-item-{template id}`. A single click selects and previews a
  row without creating a document or changing persisted last-template state;
- the footer is a separate group with `Manage templates…` on the leading side
  and `Create` on the trailing side. Electron has no stable test ID for Manage;
  the experiment will use `template-picker-manage` as an explicit
  experiment-owned seam rather than claiming it is an Electron ID. Manage
  closes the Popover, then opens the manager after a 150 ms delay. The isolated
  probe represents that result with an event/counter and does not open or
  mutate real template storage;
- Create has ID `template-picker-create`. It creates from the transiently
  selected template; success records the template as last used and closes the
  Popover. While that async operation is in flight, only Create is disabled and
  its label is `Creating…`; the primary, picker, rows, and Manage are not given
  a general disabled state by these sources. Failure re-enables Create, keeps
  the Popover open, and does not update the last-used template;
- double-clicking a template row selects it, creates it immediately, records it
  as last used on success, and closes the Popover. No bespoke double-click
  handler is defined for the primary or picker triggers. The GPUI probe treats
  one dispatched double-click event on the primary as one ordinary activation
  and does not invent a second command;
- the source defines standard Button Tab-order and Enter/Space activation, but
  no custom Arrow/Home/End navigation for template rows. The GPUI probe must
  preserve deterministic keyboard traversal/selection through the real
  component focus path without inventing an Electron arrow-key contract;
- last-template selection persists under
  `butter-paper.template-library.v1`. Invalid or missing state falls back to
  `built-in-blank`; removing the selected template also falls back to Blank
  Paper. Popover row selection is transient and resets from that durable value
  on every open. The isolated probe retains these values in memory and emits
  deterministic document/template events only; it must not access production
  local storage, template bridges, or document sessions.

The agreed test seams are the retained experiment-owned tab/template state,
semantic events/counters, and rendered stable IDs/bounds in a separate
Document Tab Bar entity. Exact native accessibility, focus restoration,
tooltip preview, and platform behavior remain separate native evidence gates.

### Phase 1f: Document Tab Bar template action — development probe passed

The experiment now owns a separate retained `DocumentTabBarTemplateSeam`. It
uses the real GPUI Component `TabBar`, `Tab`, `Button`, and controlled `Popover`
APIs. Document tabs, active selection, persisted-last and transient template
selection, pending creation, creation events, and the Manage counter remain in
application-owned state. No production document or template storage is read or
changed.

Deterministic red-green tests prove the primary action, all six ordered built-in
row selections, Create, row double click, Manage, pending-Create suppression,
standard Tab traversal plus Enter/Space row and primary activation, ordinary primary double-click activation,
Escape dismissal and exact return to the prior Document Tab Bar focus owner.
Creation preserves the two existing representative tabs and appends one active
temporary dirty tab. All template interactions leave fit, page-view, wheel,
zoom, and CAD state unchanged.

The real Document Tab Bar fits at the maintained 1200 px default and 900 px
minimum widths. Its intrinsic content is 380 px in the 320 px constrained
fixture. The horizontal-scroll owner preserves every target's size and order,
prevents overlap and clipping within the content, and brings both trailing
template targets fully into view after scrolling. The viewer-toolbar 667 px
contract is unchanged.

The final cold guarded Linux x86_64 all-targets run passed in 489 seconds with
one Cargo job, incremental compilation disabled, about 1.21 GiB maximum
resident memory, a 2.60 GiB disposable target, and about 109 GiB free after the
run. The exact combined story extension passed from the retained target in 6
seconds. This is development-only evidence. The prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`, and
the configured advisory/license/source audit passes with the reviewed
unmaintained and missing-license-field warnings.

Resolved failures remain evidence. The first stable-ID tracer failed on the
missing primary. The first attempted green build exposed a guard race when
Cargo removed a temporary metadata directory between `lstat` and `readdir`.
An injected-filesystem test now proves that concurrent disappearance is safe.
The full contract tracer then failed on the absent state/events as intended.
The first full green attempt measured the 380 px intrinsic threshold and showed
that the Escape test had moved focus outside the Popover before dispatch. The
corrected real focus path and measured geometry passed. Every failed Rust probe
cleaned only the allowlisted disposable target.

The pinned `ButtonGroup` accepts only concrete `Button` children and cannot
contain the Popover-wrapped picker, so the two actions use one shallow layout
wrapper. This is a recorded component capability gap. The probe represents
Manage as a counter and persistence in memory; it does not claim the 150 ms
manager handoff, asynchronous success/failure, preview surface, custom/imported
templates, or production storage. The pinned icon-only Button also cannot carry
the Electron primary's separate accessible name. Live roles, exact pointer-open
return to the picker trigger, and preview tooltip semantics are not inferred
from deterministic rendering.

### Frozen Electron contract: active document selection and clean close

The active-selection and clean-close slice is frozen from
`DocumentTabBar.tsx`, `ClosableDocumentTab.tsx`, `ConfirmationPopover.tsx`,
the tab-bar and confirmation tests, and the document lifecycle in `app.tsx`
before GPUI implementation:

- `Tabs.Root` is controlled by the active document ID. A primary pointer click
  on an inactive tab activates that document. The horizontal `Tabs.List` has
  accessible name `Open documents` and `activateOnFocus`; Left/Right moves and
  activates with looping, while Home/End activates the first/last enabled tab.
  Base UI only reserves Enter/Space activation for the non-`activateOnFocus`
  mode, so this slice does not invent a second command for those keys;
- Electron's rendered test IDs are index-based (`document-tab-{index}` and
  `document-tab-close-{index}`), while `data-document-tab-id` carries the
  document ID. The experiment uses domain-derived
  `document-tab-{document id}` and `document-tab-close-{document id}` IDs so
  retained identity survives removal and reorder. This is an explicit stable
  experiment seam, not a claim that Electron uses the same test IDs;
- a tab label removes only a trailing `.pdf` suffix. A dirty tab adds a visible
  `*` with accessible label `Unsaved changes`. The tab has reorder description
  and key-shortcut metadata, but pointer drag and Alt+Shift+Left/Right reorder
  belong to a later slice;
- each close action is a 24 by 24 px icon button with accessible name
  `Close {full document name}` and no tooltip. It is transparent and pointer
  inert at rest, becomes visible and pointer-active when its tab is hovered,
  and remains visible when keyboard-focused. Only the active tab's close action
  is in Tab order; an inactive close action remains available by pointer hover;
- closing a clean inactive tab disposes/removes that document and preserves the
  current active tab. Closing a clean active tab activates the next tab at the
  same index, or the previous tab when the closed tab was last. After an active
  close, the next animation frame focuses that successor tab. Closing the only
  tab leaves no active document and no focus target. The trailing Open PDF and
  template controls remain available;
- a clean close disposes its session before removal. Activating a successor
  snapshots the outgoing document/view state, deactivates the other sessions,
  restores the successor's document/view/CAD state, and clears document state
  when no tabs remain. The experiment represents these effects only with
  retained state and deterministic events; it must not access real sessions,
  storage, or production state;
- a dirty close does not remove or activate a different tab. It records a
  pending-close ID and opens the anchored confirmation branch. The production
  surface offers Cancel, Save, and Discard, blocks dismissal while saving, and
  uses title `Save changes to {document name}?` plus the data-loss description.
  This slice stops at that boundary: it emits a deterministic deferred-dirty
  close event and adds no dialog, persistence, or destructive action;
- the tab-bar item model contains only document ID, name, and dirty state.
  Loading completes before a tab is added, and temporary documents are ordinary
  dirty tabs. There is no separate clean-tab loading, temporary, or disabled
  close state to invent. The confirmation busy state applies only after the
  deferred dirty branch opens;
- the real Document Tab Bar retains its proven 1200, 900, and 320 px fixtures,
  380 px intrinsic non-wrapping content, and one horizontal-scroll owner. The
  trailing Open PDF/template controls stay fixed. The independent viewer
  toolbar remains exactly 667 px.

The pinned GPUI Component `Tab` supplies pointer activation, selected state,
tab/tab-list roles, set position, and labels, but its source explicitly says it
does not participate in keyboard focus. The experiment therefore keeps the
real `TabBar` and `Tab` and adds one shallow application-owned roving-focus key
handler. A real GPUI Component `Button` remains the close action. This is a
recorded component capability gap, not permission to replace the primitive.

### Phase 1g: active document selection and clean close — development probe passed

The experiment-owned retained tab entity now renders the proven template seam
with real GPUI Component `TabBar`, `Tab`, and 24 px icon `Button` close actions.
Stable IDs derive from document identity. Pointer selection, ordinary double
click, looping Left/Right and Home/End activate-on-focus traversal, active and
inactive clean close, next/previous successor selection, last-tab empty state,
and successor focus produce deterministic events. A dirty close records only
the deferred confirmation branch and does not remove or activate another tab.

The close actions use zero-width absolute overlays so their hit targets do not
increase the proven 380 px intrinsic strip. Pointer close remains inert until
the close target itself is hovered; keyboard activation remains available for
the active tab. Seven focused document-tab tests pass. The separate viewer
toolbar remains exactly 667 px, and the 1200, 900, and 320 px tab-bar fixtures
retain one horizontal-scroll owner with no shrinking, overlap, or clipped
interactive target.

The final guarded Linux x86_64 all-targets run passed in 484 seconds. It used
one Cargo job, disabled incremental compilation, reached about 1.21 GiB maximum
resident memory, retained a 2.60 GiB allowlisted disposable target, and left
about 109 GiB free. All 18 Rust integration tests passed. The exact story
extension passed from the retained target in 8 seconds. The source digest stays
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`, and
the exact Longbridge/Zed graph and single-GPUI identity remain unchanged. A
post-format exact-source all-targets run then passed from the retained target in
13 seconds with the target and free-space measurements unchanged.

Resolved development failures include the intended missing-layout red tracer,
unsupported icon-button accessibility labeling, close offset and hover routing,
an 8 px suffix-induced width regression, and a test-only keyed-state capture
that did not match GPUI render scope. Every failed guarded probe removed only
the exact allowlisted disposable target.

The pinned `Tab` needs the shallow application-owned keyboard handler. The
pinned icon-only `Button` exposes the stable close accessibility ID but cannot
carry Electron's separate live `Close {full document name}` accessible name.
That exact label is helper-tested, and no tooltip is invented because Electron
defines none. Real document-session effects, reorder, live accessibility, and
native visual evidence remain outside this passed development probe.

### Frozen Electron contract: dirty-document close confirmation

The confirmation boundary is frozen from `DocumentTabBar.tsx`,
`ClosableDocumentTab.tsx`, `ConfirmationPopover.tsx`,
`DocumentTabBar.confirmation.test.ts`, the Base UI 1.6.0 Popover source, and the
pending-close lifecycle in `app.tsx` before GPUI implementation:

- requesting close for a clean document bypasses confirmation. Requesting
  close for any dirty document, active or inactive, stores that document ID and
  opens one popover anchored to its 24 px close action. The tab, active
  selection, document state, and every unrelated feature remain unchanged;
- the title is `Save changes to {full document name}?`. The body is `Your
  changes will be lost if you close this tab without saving.`. The full name,
  including `.pdf`, is used. The popup test ID is `confirmation-popover`;
- actions appear in this exact order: outline `Cancel`, destructive `Discard`,
  then default-styled `Save`. During save the final label is `Saving…`. The
  source defines no tooltip, form submission, global Enter shortcut, or
  separate default-button command. Enter and Space activate only the currently
  focused standard Button;
- the uncontrolled Base UI keyboard entry would focus the first focusable
  action, so Cancel is first. Popover `modal` remains its default `false`: the
  surface has role `dialog`, no page backdrop, and no focus trap. Standard Tab
  order visits Cancel, Discard, then Save and may leave the non-modal surface;
- Escape and an outside pointer press request `open=false`. While not busy,
  that runs the same Cancel callback, dismisses, and returns focus to the
  trigger. Clicking the open trigger also requests that same cancellation.
  While busy, `onOpenChange` ignores every dismissal request, so Escape,
  outside press, and trigger toggle leave the confirmation open;
- Cancel clears the pending ID and performs no close. Discard clears the
  pending ID before the production owner disposes/removes the targeted tab.
  This experiment stops at the Discard intent and must not remove anything.
  Save records the intent, enters busy state, and remains open while production
  awaits save. Success later clears the pending ID and removes the tab; failure
  leaves it open after busy clears. This experiment stops at the Save intent;
- the target ID, not active-tab position, owns every action. Repeating the same
  dirty-close state request is idempotent; it must not append another request
  or change the target. A request for another dirty tab retargets the one
  controlled confirmation, matching `setPendingCloseTabId`;
- all three actions are disabled while busy. No separate loading or disabled
  state exists before Save. Electron does not define a double-click action for
  the confirmation buttons;
- the popup uses the standard Popover surface at `w-80` (20 rem, 320 px at the
  default 16 px root), `p-3`, `gap-3`, eight-pixel side offset, `bottom` side,
  and `end` alignment. The acceptance fixtures remain 1200 px normal, the
  maintained 900 px minimum window, and a 320 px constrained case. The popup
  must remain internally usable without growing the 380 px tab strip or the
  independent 667 px viewer toolbar.

Electron explicitly asserts that neither a Dialog nor AlertDialog backdrop is
present. The nearest faithful pinned composition is therefore the real GPUI
Component `Popover` plus real `Button`s. The pinned `Dialog` and Base
`AlertDialog` are window-modal/backdrop surfaces and would change the frozen
contract. GPUI Component Popover also does not expose title/description semantic
parts, and its pinned Button owns an internal keyed focus handle that callers
cannot name for exact pointer-trigger restoration. Those limits must remain
explicit rather than being disguised with a custom modal or button.

### Phase 1h: dirty-document close confirmation — development probe passed

The retained Document Tab Bar now renders the frozen boundary with a real
controlled GPUI Component `Popover` and ordered real `Cancel`, `Discard`, and
`Save` buttons. The experiment records semantic intents only. Cancel and
Discard dismiss; Save records its target, changes the label to `Saving…`, and
keeps the surface open. No action closes a tab, saves data, disposes a session,
or changes document/template/CAD/viewer state.

Ten document-tab tests now prove the stable confirmation IDs and copy,
pointer and keyboard activation for all three actions, first-focus Cancel,
non-trapped Tab traversal, Escape and outside-click cancellation, exact return
to a keyboard close trigger, active/inactive target identity, repeated-request
suppression, and complete busy-state action/dismissal suppression. The 1200,
900, and 320 px fixtures retain a 296 px inner confirmation surface inside the
standard 320 px Popover, ordered accessible actions, 24 px close targets, the
380 px sole-scroll-owner tab strip, and the unchanged 667 px viewer toolbar.

The final guarded Linux x86_64 all-targets run passed in 475 seconds with one
Cargo job, incremental compilation disabled, about 1.22 GiB maximum resident
memory, a 2.60 GiB allowlisted target, and about 109 GiB free. All 21 Rust
integration tests passed. The exact story description passed from the retained
target in 8 seconds and added about 1.9 MiB. The prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`; the
pinned Longbridge/Zed revisions and single-GPUI graph are unchanged.

Resolved development evidence includes the intended missing-surface and
missing-intent red tracers, a missing extension-trait import, a moved
focus-handle clone, and a real focus-timing defect. The final implementation
advances focus only after the Popover actions mount. Every failed guarded run
cleaned the exact owned disposable target.

No existing X11 or Wayland graphical session was present, so the exact story
screenshot and live accessibility tree are blocked. The pinned Popover has a
dialog role but no semantic title/description parts. Its icon Button does not
take pointer focus, so exact pointer-trigger restoration remains unproved; the
keyboard-trigger path returns exactly. IME, macOS/Windows, packaged, and
physical-device evidence were not run.

### Frozen Electron contract: document-label truncation and close hover mask

The label slice is frozen from `DocumentTabBar.tsx`,
`ClosableDocumentTab.tsx`, `styles.css`, the Nova `TabsTrigger`, and focused
tab tests before GPUI implementation:

- a tab is 32 px high. The trigger has 6 px horizontal padding and a 6 px child
  gap. Its outer wrapper and tab list do not wrap. The label removes only a
  trailing case-insensitive `.pdf`, uses `white-space: nowrap`, hidden overflow,
  and `text-overflow: ellipsis`. Electron declares no per-tab pixel cap; its
  actual truncation point follows browser flex allocation;
- a dirty tab owns a separate non-shrinking `*` with accessible label `Unsaved
  changes`. Active tabs use the semantic muted surface and foreground; inactive
  tabs use the shell background and muted foreground treatment;
- the close target stays 24 by 24 px, four pixels from the right edge. It is
  transparent, pointer-inert, and opacity zero at rest. Hovering the tab makes
  it pointer-active and visible. Keyboard focus also keeps the close visible;
- while the tab is hovered, or while its close target is `focus-visible`, the
  label uses a right-edge CSS mask. The label stays opaque until 34 px from its
  right edge, fades over 20 px, and is transparent for the final 14 px. Pointer
  exit removes the mask. The mask changes no layout or hit target;
- the document label has no tooltip wrapper, native `title`, test ID, delay,
  content, placement, or dismissal contract. The shared 180 ms Base UI tooltip
  provider and bottom-side Document Tab Bar action tooltips apply to other
  controls only. This slice must not invent a label tooltip for pointer or
  keyboard focus;
- Electron's stable tab and close test IDs remain index-based, while its DOM ID
  and `data-document-tab-id` retain separate position and domain identity. The
  experiment continues its documented domain-derived stable-ID mapping;
- the fixed fixtures remain 1200, 900, and 320 px. The default content stays an
  exact 380 px non-wrapping strip with one horizontal-scroll owner, fixed
  trailing actions, 24 px close targets, and an independent exact 667 px viewer
  toolbar.

The pinned `TabBar::max_width` is the real component truncation API. A 190 px
cap is the smallest cap that preserves the frozen 380 px strip and its existing
dirty `structural-details` label; this is an explicit reversible native mapping,
not a claim that Electron declares 190 px. The pinned `Tab` does not expose an
ID for its internal label box, so a zero-impact absolute tracer records the
allocated label bounds while the real component retains shaping and ellipsis.
GPUI has no equivalent of the CSS text mask in this component. The experiment
therefore uses a shallow 34 px color fade adapter for pointer hover. The pinned
icon `Button` keeps its internal `FocusHandle` private, so the parent cannot
apply the same fade to sibling content on keyboard focus without replacing or
patching the real component. That exact focus-mask semantic remains a recorded
upstream gap; the close button itself stays keyboard visible and activatable.

### Frozen Electron contract: keyboard tab reorder and move announcement

This slice is frozen from `DocumentTabBar.tsx`,
`domain-ui/ClosableDocumentTab.tsx`, `app.tsx`, and the focused tab-order tests
before GPUI implementation:

- the tab trigger advertises `Alt+Shift+ArrowLeft Alt+Shift+ArrowRight` and
  handles only those two arrow keys while both Alt and Shift are pressed. This
  is the literal chord on every platform; it does not substitute the primary
  Command/Control modifier. The handler does not reject an additional Control
  or Meta modifier, so the native probe must not silently make that condition
  stricter;
- the handler belongs to each document tab trigger. The focused trigger is the
  move target. The surrounding Electron tab list activates on focus, so a
  normal keyboard path keeps focused and active identity aligned, but the move
  handler itself has no separate active-tab guard;
- Left swaps the target with its immediate predecessor and Right swaps it with
  its immediate successor. First/Left and last/Right are no-ops. Movement does
  not wrap. A valid chord prevents the browser default and stops propagation
  even when the boundary produces no move;
- ordering is submitted as the complete stable-ID sequence. `app.tsx` applies
  that order to the existing tab objects, so document/session snapshots,
  clean/dirty data, and every other tab-owned field travel with identity. The
  separately controlled active tab ID does not change;
- after a successful keyboard move, Electron requests one animation frame and
  focuses the trigger with the same `data-document-tab-id`. Pointer drag does
  not request this restoration and is outside this slice;
- a successful move immediately replaces the status text with exactly
  `Moved {documentName} to position {one-based position} of {tab count}.` The
  stable status seam is `document-tab-reorder-status` with `aria-live="polite"`.
  Invalid and boundary moves do not change or duplicate the status. The source
  has no further de-duplication or timing policy;
- `DocumentTabItem` exposes only ID, document name, and dirty state. Dirty tabs
  reorder like clean tabs. Loading and temporary states are not represented at
  this component boundary. An open dirty-close popover naturally owns focus,
  so a tab trigger does not receive the chord through the normal path; the
  reorder handler defines no additional suppression rule;
- the experiment keeps domain-derived tab IDs and must preserve the pending
  dirty-confirmation target, close behavior, template events, and viewer/CAD
  state by stable identity. This slice records deterministic reorder and
  announcement evidence only. It does not persist order or mutate production
  sessions.

### Phase 1i: document-label truncation and pointer mask — development probe passed

The deterministic rendered red tracer first failed on the absent stable label
bound. The final retained seam proves natural short labels, a genuinely long
name capped at 190 px through the real `Tab` ellipsis path, active/inactive and
clean/dirty bounds, pointer mask entry/exit, exact 34/14 px mask geometry,
preserved 24 px close targets, and no label tooltip under pointer or keyboard
focus. Template, fit, page-view, wheel, zoom, CAD, selection, clean-close, and
dirty-confirmation state remain independent.

The 1200, 900, and 320 px fixtures retain the exact 380 px strip, sole scroll
owner, fixed trailing actions, and 667 px toolbar. The final guarded Linux
x86_64 all-targets run passed in 510 seconds with one Cargo job, incremental
compilation disabled, about 1.22 GiB maximum resident memory, a 2.60 GiB
allowlisted target, and about 109 GiB free. All 23 integration tests passed. The
long-label story then passed from the retained target in 8 seconds.

Resolved development evidence includes the intended red tracer, a theme-token
type mismatch, two 350 px regressions from custom text and a 160 px cap, an
unreliable attempt to observe private Button focus state, and a full-width versus
intrinsic toolbar assertion. Every failed probe cleaned only the exact owned
target. Exact source preparation, digest, single-GPUI identity, and dependency
policy pass. No graphical Linux session exists, so the screenshot and live
accessibility tree are blocked. IME, packaged candidates, macOS, Windows, and
physical-device evidence were not run.

### Phase 1j: keyboard tab reorder and polite status — development probe passed

The deterministic rendered red tracers first failed because
`document-tab-reorder-status` had no resolved bound and Alt+Shift+Right left
the stable tab bounds in their original order. The final retained command
moves the complete tab object by domain ID, recomputes the active position from
the unchanged active ID, retains focus handles by ID, records one exact
experiment event, and sets the frozen text
`Moved {documentName} to position {one-based position} of {tab count}.`

The real pinned `TabBar` and `Tab` remain the presentation and input surface.
Each tab exposes a stable accessibility ID, the literal Electron shortcut
metadata, and the keyboard-only part of the source description. The pointer
drag sentence is deliberately not advertised until pointer drag exists. The
status is a stable one-pixel `Role::Status` node. GPUI at this pin has no direct
live-region builder, so the smallest transparent application adapter uses the
public accessibility-subtree API to set the same node's AccessKit property to
`Live::Polite`. This is not an upstream patch or a replacement component.

Rendered tests prove Left and Right movement, first/last no-wrap boundaries,
multiple moves, unchanged announcement/event state after a boundary no-op,
missing Alt or Shift suppression, the source-defined acceptance of additional
Control or platform modifiers, same-ID focus retention, unchanged active ID,
and a programmatically focused inactive dirty tab. A template-created dirty
tab moves with its name, dirty flag, focus handle, and creation state. The
pending dirty-confirmation target remains attached to its stable ID through a
move, and the same tab later follows the already-proven clean-close contract.
Template, fit, page-view, wheel, zoom, and CAD state remain unchanged.

The fixed 1200, 900, and 320 px fixtures still prove 24 px close targets, the
190 px long-label cap, 34 px fade and 14 px solid tail, exact 380 px intrinsic
tab strip, sole horizontal-scroll owner, fixed trailing actions, and exact
667 px viewer toolbar. Reordering changes only sibling position: target sizes,
containment, and overflow policy do not change.

The final guarded Linux x86_64 cold all-targets run passed in 486 seconds with
one Cargo job, incremental compilation disabled, about 1.22 GiB maximum
resident memory, a 2.60 GiB allowlisted target, and about 109 GiB free. All 27
integration tests passed. After the deterministic gate, the exact story text
was extended and compiled again from the retained target in 8 seconds. The 10
build/source policy tests, prepared digest
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
851-package single-GPUI graph, and configured dependency policy pass. The
dependency audit retains the previously reviewed missing-license-field and
unmaintained-transitive warnings; advisories, licenses, and sources pass.

Resolved development evidence includes the two intended red failures, one
test assumption that did not restore the dirty tab's focus after ordinary
Shift+Left/plain Left selection, and one comparison between the 1200 px flex
content box and the 320 px intrinsic threshold. Every failed guarded run
cleaned only the exact owned disposable target.

No X11 or Wayland graphical session exists. Fresh screenshot and live
assistive-technology announcement evidence are therefore blocked, and no
synthetic display was created. Pointer drag, persistence, production session
effects, the Hibbeler corpus, and transferred macOS visual evidence were not
run or are absent as recorded. IME, packaged candidates, macOS, Windows, and
physical-device evidence were not run. The prior full `pnpm check` result
(1,088 Vitest plus 23 signature-relay tests) remains applicable because no
production-facing input changed.

### Phase 1k: pointer tab reorder without persistence — development probe passed

The pointer contract was frozen from `DocumentTabBar.tsx`, the installed
`@dnd-kit/core` 6.3.1 and sortable 10.0.0 sources, Electron styles, and focused
tests before implementation. Only primary button zero and the browser
`isPrimary` pointer can arm the sensor. Activation uses strict Euclidean
distance `sqrt(dx² + dy²) > 6`; exactly six pixels remains a click. The owner
document observes move, up, cancel, resize, and visibility without DOM pointer
capture. Collision uses closest center over registered tab rectangles, and
horizontal sorting translates only the dragged tab and intervening siblings.
A commit moves the stable ID, preserves the complete tab object and active ID,
and emits the keyboard reorder's polite text without keyboard-style focus
restoration. The close button remains outside the trigger. Installed dnd-kit
auto-scroll is enabled, but its edge timer is an explicit later boundary.

The experiment keeps the real pinned GPUI Component `TabBar`, `Tab`, and close
`Button`. A zero-size owner-window input bridge is the smallest transparent
application adapter for the document-level sensor. Feature state, stable-ID
order, focus handles, pending dirty-confirmation identity, and deterministic
events remain in the experiment-owned retained entity. No upstream source,
production session, persistence, saving, or storage is changed.

Four focused rendered tests prove below-threshold click/no-reorder, strict
activation beyond six pixels, translated drag and target tracers, both
directions, adjacent and multi-position movement, same-target and boundary
no-ops, primary-button and close isolation, release, Escape and lost-button
cancellation, complete identity/data movement, active and pointer-focus
retention, exact announcement text, pending dirty-confirmation identity, and
unchanged template/CAD/viewer state. The 1200, 900, and 320 px fixtures preserve
the 380 px strip, 24 px close targets, 190 px label cap, 34/14 px hover fade,
sole horizontal scroll owner, fixed trailing actions, and exact 667 px viewer
toolbar without overlap, clipping, width growth, or layout jump.

The superseded runner deleted the 2.60 GiB target for every test status 101.
Test-first policy seams now prove that success, Cargo failure, wall timeout,
interruption, and memory stop retain the exact owned target. Only free-space and
target-size safety breaches, or an explicit cleanup command, clean it. Fixed
modes reject shell injection and separate focused, controlled-failure, and
final all-targets runs. A real failing pointer test retained its 2.44 GiB
focused target; the controlled status-101 proof retained it again; the
corrected focused gate passed warm in 3 seconds. The story all-targets gate
passed 3 component-stack, 21 document-tab, and 8 viewer-toolbar tests in 9
seconds. After two formatting-only prepared-source files were restored to the
verified digest, the final exact-source gate passed the same 32 tests in 46
seconds, reached about 1.21 GiB maximum resident memory, retained a 2.60 GiB
target, and left about 108.6 GiB free.

This is Linux development-only deterministic-render and compile evidence. GPUI
has no browser-style `isPrimary` flag and no public pointercancel, resize, or
visibility parity seam at this pin; non-primary mouse suppression, Escape, and
lost-button cancellation pass, but physical interruption parity is not
claimed. Edge auto-scroll is not implemented. No existing X11/Wayland session
exists, so fresh native screenshot and live accessibility evidence are blocked.
The Hibbeler corpus and transferred macOS visuals remain absent. IME,
macOS/Windows compile and packaging, packaged candidates, physical-device input,
production session effects, persistence, and performance comparison were not
run. The prior full `pnpm check` result remains applicable because no production
input changed.

### Phase 2: cutover-critical document journeys

Pointer edge auto-scroll is deferred. The first read-only native document spine
is implemented under `gpui-component-compat` without an upstream fork. Its
`DocumentWorkspace` owns stable-ID `NativeDocumentSession` entities. Session
state owns document identity, source path, status, page geometry, current and
requested page, generation, raster presentations, and the worker-resource
capability. GPUI Component and raw domain surfaces render snapshots and
dispatch commands only.

The compatibility crate consumes the gallery library with
`default-features = false`. This reuses the GPUI-free worker protocol and keeps
GPUI-CE and its development application out of the resolved component graph.
The only ported presentation seam converts bounded BGRA worker surfaces to the
pinned Zed `RenderImage`. The exact worker entry includes the already-reviewed
experiment worker shell and builds against `pdfium-render` revision
`6cee8b9a3951832ac0ff62ce4c32800278001cb8` with `pdfium_7881`.

Passed development-only evidence:

- the source red test failed on the absent workspace module, and the ordinary
  Cargo 101 retained the valid target;
- the public 100-page fixture is present at SHA-256
  `517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b`;
- deterministic rendered tests prove stable workspace/page/thumbnail IDs,
  retained entity identity, three thumbnail presentations, thumbnail command
  dispatch, two-generation stale-result rejection, failed-second-open
  isolation, abstract resource close, entity removal, and rejection of work
  after close;
- the exact story and local worker binary compile. The separately approved
  existing fetch policy restored only `pdfium-linux-x64.tgz` from
  `chromium/7881`, pinned at 3,644,759 bytes and archive SHA-256
  `1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d`.
  The extracted library SHA-256 is
  `f728930966f503652b92acc89b9374a2eeca00ce42e26dccd3e4b5c5161b2d64`,
  and the manifest remains `productionApproved: false`;
- the real guarded test proves a 100-page open, non-uniform initial-page
  pixels, 12 real thumbnail buffers, a rendered stable-ID thumbnail click that
  advances the retained page through the real background worker, a failed
  second open that preserves that page and the first live worker, continued
  page rendering, worker PID exit, and mapped-surface cleanup;
- the first strengthened run exposed an empty identity-specific surface
  directory left by failed construction. The constructor now removes its own
  surface output on every failure path, and the same real regression passes;
- the current exact-source, two-layer guarded all-targets gate passes 53
  non-ignored tests in 17 seconds and retains about 3.24 GiB;
- 14 storage/source tests, exact prepared digest, 11 checksum-bound shared
  gallery inputs, one GPUI package across 870 resolved packages, no forbidden
  package, and the configured advisory, license, and source audit pass.
  Existing upstream missing-license-field and unmaintained warnings remain
  visible.

Failed and blocked evidence:

- the intentional red proof failed first on missing read-only raster accessors
  and then on the failed-open directory leak. Both expected Cargo 101 results
  retained the valid target; no current required test is failing;
- no existing X11 or Wayland graphical session exists. Native screenshot and
  live accessibility evidence are blocked. Production PDFium redistribution
  remains blocked by the supplier manifest's `productionApproved: false`.

Rectangle edit, transactional Save As/reopen, and real multi-document protected
close now pass as later Linux development slices. The first visible viewer
slice also passes. A private `DocumentViewerState` per retained session owns
Continuous/Single Page mode, zoom, measured scrolling, raster revision,
planner generation, a two-job asynchronous queue, and a 256 MiB cache. The
workspace retains the previous preview while tiles arrive and composes accepted
tiles under stable page/tile IDs. The story observes the real GPUI Component
toolbar and zoom entities instead of keeping duplicate view state.

The deterministic viewer tests prove queue bounds, stale completion rejection,
multiple mode/zoom/scroll plans, cache reuse, exact resource-swap invalidation,
and complete-pixel spatial variation. The 6-second real gate proves visible
800% page-0 PDFium tiles, thumbnail navigation, a new visible page-1 tile
generation, cache bounds, worker exit, and run-scoped mapped-surface cleanup.
The two expected diagnostic failures were the page-space/output-space crop
`InvalidRequest` and a shared historical surface-root assertion; both are
resolved without cleaning the retained Cargo target.

Blocked: production PDFium redistribution, fresh native screenshot/live
accessibility on this headless VPS, and packaging approval.

Not run: production persistence, full annotation compatibility, packages,
macOS/Windows, physical devices, input method editor (IME), Hibbeler, and the
matched performance comparison.

Pen now passes the complete Linux development seam: native pointer creation,
smooth page/thumbnail paint, retained move/opacity/lock/delete/history,
source-fingerprint-bound typed Save As, raw-object deletion, exact reopen, and
the real PDFium worker journey. Text Box now also passes real GPUI Component
multiline input, Escape/guarded-blur commit, empty discard, page/thumbnail
painting, retained content/geometry/lock/delete/history, typed FreeText
create/edit/delete Save As, exact reopen, and the real PDFium worker journey.
Length now passes the cutover-critical Linux development seam: a real GPUI
Component control and `Shift+Alt+L` binding, page-scoped scale guard, two-click
preview/commit, Shift constraint, strict two-point rejection, Escape,
application-owned atomic edits, lock/delete/undo, standard-Measure typed
create/edit/delete Save As, exact reopen, and ordinary-Dimension isolation.
Pointer body drag, semantic snapping, caption hits, exact arrow/caption paint,
scale configuration UI, and native visual/accessibility acceptance remain
partial. The immediate resume seam is Highlight create/edit/save/reopen,
followed by the CPU-precomposed Multiply oracle and then Image. Multiply
compositing remains an explicit pinned-API gap rather than accepted visual
parity.
Fit Width/Fit Page math, viewport keyboard commands, cache-pressure eviction,
and platform-native visual/input evidence remain explicit viewer follow-ups.
Component work remains pulled by complete journeys.

### Phase 3: product capabilities

Continue the existing annotation, PDF, template, signature, persistence, and
platform work through the explicit ledger. Component conversion must not be
confused with feature parity.

### Phase 4: distribution and cutover

Qualify the exact packaged candidate on macOS and Windows physical devices and
on the maintained Linux backends. Run realistic PDF, accessibility, input
method editor (IME), native menu/shortcut, minimum-window, overflow, packaging,
update, rollback, and release gates before any Electron removal.

## Files changed by the compatibility slice

- `experiments/gpui-migration/gpui-component-compat/src/continuous_view_control.rs`
- `experiments/gpui-migration/gpui-component-compat/src/cad_view_control.rs`
- `experiments/gpui-migration/gpui-component-compat/src/page_view_control.rs`
- `experiments/gpui-migration/gpui-component-compat/src/viewer_toolbar_strip.rs`
- `experiments/gpui-migration/gpui-component-compat/src/document_tab_bar.rs`
- `experiments/gpui-migration/gpui-component-compat/src/document_workspace.rs`
- `experiments/gpui-migration/gpui-component-compat/src/lib.rs`
- `experiments/gpui-migration/gpui-component-compat/tests/component_stack.rs`
- `experiments/gpui-migration/gpui-component-compat/tests/viewer_toolbar_strip.rs`
- `experiments/gpui-migration/gpui-component-compat/tests/document_tab_bar.rs`
- `experiments/gpui-migration/gpui-component-compat/tests/document_workspace.rs`
- `experiments/gpui-migration/gpui-component-compat/src/bin/butter-paper-pdf-worker.rs`
- `experiments/gpui-migration/gpui-component-compat/src/bin/component_story.rs`
- the build-guard implementation/tests, parity ledger, audit, and focused experiment
  documentation

Do not add the GPUI-CE graph to the component application. The next approved
implementation should leave the gallery graph intact and reuse only its
GPUI-free modules from the sole Longbridge/Zed application graph. A later
gallery retirement or manifest rewrite needs its own reviewed slice.

No production source is in the proposed set.

## Verification gates

Passed evidence must be reported separately from failed, blocked, and not-run
evidence. Development-runtime, packaged-candidate, and physical-device proof
must also remain separate.

The component-control slices and deterministic read-only document spine pass
exact source/revision checks, one GPUI crate identity,
advisory/license/provenance review, owned-file formatting, the storage-guard
regression suite, the guarded all-targets build/tests, and the separately gated
real PDFium 7881 worker test. Real page/thumbnail pixels, failed-second-open
isolation, stale-result rejection, worker PID exit, and mapped-surface cleanup
are Linux development evidence. The exact native Linux story and live
accessibility tree remain blocked because this VPS has no existing graphical
session; neither gate is inferred from compilation. Production PDFium
redistribution also remains blocked. macOS and Windows compile/package
evidence, IME, native menus, and physical-device evidence were not run. The
Hibbeler corpus and prior macOS capture are not present in this checkout and
remain blocked, not inferred.

## Agent lanes and automation

No subagent is running and none is needed for this approval checkpoint. After
approval, use independent read-only lanes only for Electron contract inventory,
current-source component lookup, verification audit, and platform risk review.
Keep one writer for shared files and one stateful GUI lane.

No recurring automation is justified. Use bounded waits for long commands.
No paid GPU lane is approved by this plan. A later paid run requires a declared
budget, task TTL and hard maximum, an independent reaper, copied-off evidence,
and verified deletion after fresh user approval.

## 2026-08-25 cutover-spine update

The exact Longbridge/Zed application is now the runnable document candidate,
not only a component probe. `DocumentWorkspace` renders actual retained
sessions through real GPUI Component `TabBar`/`Tab` primitives. The last full
pre-rotation guarded all-targets gate passed 79 non-ignored tests with two
ignored, separately gated tests. The page-rotation/capture checkpoint then
passed focused guarded gates: performance protocol 14/14 in 11 seconds, story
configuration 3/3 in 3 seconds, Linux signal guard 2/2 in 3 seconds, corrected
document workspace 44 passed plus one ignored in 12 seconds, and real PDFium
1/1 in 6 seconds. Performance Node tests pass 99/99 in 2703.916708 ms and the
fresh build-guard suite passes 10/10 in 337.808436 ms. The story plus worker
development build passed in 11 seconds; its warning-only source fix awaits a
fresh warning-free rerun and is not optimized or packaged evidence. The
prepared source digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`.

The checksum-pinned PDFium 7881 gate uses the reviewed 100-page public fixture.
It proves page/thumbnail pixels, navigation, failed-second-open isolation,
annotation editing, page-rotation Save As/reopen, stale rejection, worker exit,
and mapped-surface cleanup.

The accepted application-close slice now proves the next ownership boundary as
Linux development-only evidence. A single `ApplicationCloseShell` owns the GPUI
Component modal layer. Application-owned stable document IDs and an exact
transaction/request/document-to-`PathBuf` map drive serial Save All, including
non-UTF-8 Unix path preservation. Save As cancellation dismisses the modal and
preserves both live sessions; stale cancellation cannot advance or quit the
transaction. Discard All releases the two real PDFium workers and mapped
surfaces before the final quit intent. The pure application-close gate passes
9/9 in 18 seconds
(`button-probe-20260825T181935Z-1667638.log`). The integration gate passes 7
tests with one separately gated test ignored in 6 seconds
(`button-probe-20260825T182000Z-1668397.log`), and the real two-PDF lifecycle
passes 1/1 in 3 seconds (`button-probe-20260825T182023Z-1669025.log`). Releases
run serially. Failure cancels the transaction and reports the message, emits no
`ReleaseAcknowledged`, and cannot promote quit. The targeted live session plus
pending dirty-close and close-after-save identities remain intact. Clean close
and dirty Discard both report `ReleaseFailed`; a fresh retry succeeds. The
PDFium resource marks release only after worker close plus `remove_dir_all` and
can retry cleanup. Deterministic injection proves failure recovery. The real
PDFium test proves normal release but not an injected real IPC failure.

The current exact-source warm all-targets gate passed 125 non-ignored tests
with three separately gated ignored tests in 45 seconds
(`button-probe-20260825T205849Z-1788476.log`). The exact story and worker build
passed warm in 10 seconds (`button-probe-20260825T205946Z-1789689.log`). The accepted cold all-targets
gate passed in 520 seconds with symbols stripped and retained 2,708,840 KiB
(`button-probe-20260825T173734Z-1615153.log`). The immediately earlier attempt
was correctly stopped at the target-size guard and cleaned only the owned
target (`button-probe-20260825T173634Z-1613961.log`); it is retained failed-build
and passed-safety evidence, not accepted application evidence.

Line/Arrow now passes as a Linux development-only document journey. Its shared
model/history/scene and interaction gate passes 26/26, including controlled
appearance editing, no-op suppression, bounds validation, locking, and
Line/Arrow independence (`button-probe-20260825T193425Z-1732122.log`); the real
component workspace gate passes 3/3 with separate real GPUI Component
color/width/opacity popovers, history, undo/redo, and scene updates
(`button-probe-20260825T193334Z-1731229.log`); the exact PDF contract
passes 12/12 (`button-probe-20260825T190235Z-1707917.log`); and the real PDFium
create/edit/save/reopen/rehydrate/render/release journey passes 1/1 with exact
distinct Line and Arrow appearance values
(`button-probe-20260825T193457Z-1732886.log`). The refreshed shared-source
receipts, prepared digest, 870-package single-GPUI graph, and configured
advisory/license/source policy pass. The real popovers currently expose
representative presets, not the complete free-form Electron property inputs.
`/BPAppearance`, fresh `/M`, workspace endpoint/body edits, native overlay
capture/accessibility, and packaged-platform evidence remain open.

The partial multi-selection cutover slice now has guarded Linux development
evidence. Ordered selection remains application-owned and outside historical
snapshots. Real plain/Shift pointer input and one group gesture cover all six
maintained annotation families while locked members remain selected and fixed.
Copy/paste preserves document order for the live imported vector, applies the
exact repeated 12-point offset, assigns reconciled stable IDs, and selects the
pasted group. Mixed Delete removes unlocked members and retains locked
selection; all-locked Delete and drag are no-op transactions.
The real component Delete button uses group capability rather than the primary
member's lock bit. Save As and typed/PDFium reopen preserve pasted geometry,
kind, stable identity, and lock state while selection remains transient. The
GPUI-free geometry seam and native pointer bridge prove strict greater-than-six
CSS pixel activation, lasso window/crossing, the sub-threshold two-click box,
ordered replace/Shift-add/Alt-remove, cancellation without mutation, and a
transient paint overlay. The shared adapter passes 29/29
(`button-probe-20260825T204604Z-1779890.log`), the focused workspace route passes
2/2 (`button-probe-20260825T205607Z-1785910.log`), and page-major cross-family
order remains covered by the focused model, persistence, and real PDFium gates.
The current complete acceptance gate passes 126 tests plus three gated ignores
(`button-probe-20260825T215104Z-1819463.log`).

This does not close multi-selection. Cross-page policy, Length caption bounds,
rotated Text/Image marquee geometry, native menu routing, fresh overlay
capture, and an accessible canvas/status model remain explicit cutover gaps.

Page Scale is now in the revisioned document snapshot and PDF save boundary.
The Linux development slice covers the Electron-defined preset, custom, and
calibrated models; all five units; separate X/Y; decimal and fractional
precision; current/all/range targets; and session-only saved presets. Exact
`/BPPageScale` JSON survives two independent reopens, full replacement removes
stale page metadata, and optional preset creation plus scale application shares
one undo revision. The real GPUI Component dialog uses Dialog, Select, Checkbox,
NumberInput, ButtonGroup, and Button primitives. Focused UI state and behavior
passes 5/5 (`button-probe-20260825T214834Z-1817201.log`), the model passes 1/1
(`button-probe-20260825T214909Z-1817804.log`), persistence passes 15/15
(`button-probe-20260825T214925Z-1818177.log`), and the checksum-pinned real
PDFium render/save/reopen/cleanup journey passes 1/1
(`button-probe-20260825T215036Z-1818946.log`). Native modal pixels, complete
pointer/keyboard focus proof, calibration hover/snap visuals, accessibility,
and packaged platforms remain blocked or not run.

The shipping Electron app does not persist open tabs, active document, dirty
state, history, or viewer state across restart. Document-session recovery is
therefore not an Electron parity requirement and is intentionally omitted.
Ellipse create/edit/Save As/reopen now passes as Linux development evidence.
The retained adapter and real GPUI canvas prove the three-pixel threshold,
curve-correct eight-handle resize, move/resize previews, rotation,
double-click reset, stable identity, and lock suppression. The checksum-pinned
real PDFium journey proves create/edit/delete across validated saves and
independent reopen, plus worker replacement/exit and mapped-surface cleanup.
The guarded workspace gate passes 2/2 in 8 seconds, the real gate passes 1/1 in
12 seconds, and all-targets passes 128 plus three gated ignores in 25 seconds.
Blank/template create-to-save is the next cutover-critical journey.

Page rotation is no longer a missing cutover slice. Real GPUI Component left
and right controls mutate application-owned page state. One turn is one
document revision and one viewer-generation invalidation. Tests cover all
quarter-turn raster/crop transforms, page and thumbnail geometry, rotated
annotation projection and hit testing, stale completion rejection, injected
failure preservation, undo/redo, dirty close, the pending-pixel Save As guard,
canonical PDF `/Rotate`, and independent reopen. `PaintedPageEvidence` binds
document/page identity, request/resource/viewer generations, native prepaint
sequence, source PDF points, contained window-logical bounds, and rendered
device-pixel ratio.

The first exact-candidate measurement scaffold now passes deterministic tests,
but it is intentionally not a benchmark result. Protocol-owned fields cannot
be overwritten; time, process, scenario, painted generation, GPUI next-frame,
and worker/surface cleanup receipts fail closed. Native command IDs match the
X11 driver and the profiler build requires a measured input-to-platform-draw
sample delta. The strict Longbridge profile rejects the former metadata-only
`fixed-crops-matched` claim and requires a real XGetImage presented crop with
frozen geometry/scale plus an exact hash or registered scan-fidelity-v2 pass.
The capture coordinator freezes the painted authority, consumes one pending
SIGUSR1 request, schedules exactly one later GPUI frame, rejects post-signal
authority drift, and withholds cleanup until the matching presented-state
receipt. Failure cleanup restores the signal handler and proves worker/surface
release without success qualification. This is deterministic development
protocol evidence, not a native crop, live accessibility result, or benchmark.

The fixed guarded release mode uses a separate allowlisted disposable target,
builds story and worker together, excludes GPUI `test-support` from normal
dependencies, and can seal an optimized checksum-bound candidate. That
candidate remains development-only, unpackaged, and timing-ineligible until
native crop and graphical execution pass. This VPS has no existing graphical
session or `DISPLAY`, and no synthetic display was created. Native crop,
page-rotation screenshot, and live accessibility evidence are blocked. The
one-bit SIGUSR1 adapter can coalesce repeated standard signals, so process-level
duplicate delivery is not claimed; non-Linux worker liveness probing remains a
known gap. Continue session recovery while retaining the exact v6 graphical run
as an explicit later gate. Do not use old GPUI-CE results as a cutover decision. Full Page
Scale, package-relative worker/PDFium loading, and packaged platform gates
remain. Optimized release, the exact packaged candidate, macOS/Windows proof,
and physical-device evidence were not run for this source slice.

Application-close modal pointer/hit-testing, live focus containment/return, and
the live accessibility tree are also blocked by the missing graphical session;
deterministic draw coverage is not a substitute. No application-close packaged
candidate or physical-device evidence was run.
The accepted close journey improves cutover readiness but does not turn
micro-control parity into migration completion.

## 2026-08-25 generated-template cutover update

The former mock template event is no longer the only native evidence. A new
GPUI-free, checksum-bound generator and owned temporary-store seam feed a real
`NativeDocumentSession`. The accepted representative contract is A3 landscape,
10 mm Square Grid, `#d1d5db`, temporary `Untitled.pdf`, dirty until Save As,
and worker-before-source release ordering.

Passed development evidence: two deterministic lifecycle tests, one separately
gated real PDFium render/Save As/reopen/release test, source digest
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
and the warm all-targets gate at 130 passed plus four gated ignores. The build
guard now also waits for the full Cargo process group before safety cleanup;
its fast policy gate passes 11/11.

Known gaps: generation still runs synchronously on the GPUI thread; shutdown
after an unrecoverable resource-close failure can retain the owned temporary
directory; cleanup failure after successful publication needs a visible retry
surface; Windows PDFium source-handle transfer is unavailable. The next
cutover-critical journey should connect the real template split control to
this session command, then cover the remaining built-ins and manager/persistence
only to the extent required by the frozen Electron contract.

## 2026-08-26 template-control/session bridge

The previously separate seams are now one dependency-correct journey. A single
retained `TemplateSplitControl` owns GPUI Component presentation and emits typed
intents. `DocumentWorkspace` owns the command, duplicate reservation, generated
source, stable session identity, dirty state, Save As, worker, and cleanup. The
legacy `DocumentTabBarTemplateSeam` remains only as a compatibility adapter over
the same control; the runnable workspace never creates `template-document-*`
mock tabs.

All six built-ins map to GPUI-free bounded vector generators. Square Grid has
the full rendered and checksum-pinned PDFium command journey. Passed evidence:
template command/workspace 4/4, tab-control 22/22, all-pattern generator 3/3,
real PDFium 1/1, source/guard policy 17/17, exact prepared digest
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
one GPUI identity across 870 packages, and all-targets 133 plus four gated
ignores in 26 seconds.

The build policy remains at a 4 GiB target cap. One cold diagnostic exceeded
it; `codegen-units = 1` reduced the clean target to about 2.1 GiB. Cleanup now
requests bounded `ENOTEMPTY` retries, and ordinary failures retain the target.
The next cutover-critical slice is the representative rectangle
create/edit/Save As/close/reopen journey through this same runnable workspace,
followed by lifecycle recovery gaps. Native visuals/accessibility, Windows
source handles, production PDFium redistribution, and packages remain blocked
or not run.

## 2026-08-26 Rectangle cutover update

The recommended Rectangle journey now passes through one public application
boundary rather than a collection of backend calls. The exact rendered path is
GPUI Component tool button → native GPUI pointer bridge → application-owned
annotation/session state → real properties Popover → transactional PDF writer
→ validated PDFium resource swap → clean close → distinct-workspace reopen.
The fresh workspace rehydrates the stable `workspace:rectangle:1` identity,
edited geometry and 4 pt stroke without carrying selection, undo, redo, dirty,
or prior entity state.

The first real Save As exposed a representation defect. Native f32 projection
produced values such as `89.999995`, while PDF edge reconstruction returned
`89.999992`. Bitwise equality was not a valid PDF compatibility boundary. The
GPUI-free model now defines a 0.00001 pt geometry equivalence and the saver uses
it only for Rectangle coordinates; page, identity, rotation, appearance, and
lock state remain exact. A deterministic regression rejects material drift.

Passed: focused real journey 1/1, focused geometry 1/1, all-targets 133 plus
five gated ignores, source/guard 17/17, prepared digest
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
shared annotation-model SHA-256
`74bb4230896c485a44939d8925aa7839aa550a139327602239d75ea53153fa7f`,
one GPUI identity across 870 packages, configured dependency policy, and host
storage with about 103 GiB free. Failed evidence is limited to the retained red
compile/lifetime and exact-float assertions that led to the accepted fix; no
required test remains failing.

Blocked: native screenshot and live accessibility on this headless VPS,
production PDFium redistribution, and Windows inherited source-handle transfer.
Not run: packages, physical devices, IME, Hibbeler, and the exact matched
performance run. The next dependency-ordered journey is real worker/render
failure recovery that preserves dirty annotation state and can cleanly reopen;
the complete Rectangle property inspector follows before declaring that tool
fully migrated.

## 2026-08-26 worker-recovery update

The recovery boundary is now application-owned and resource-only. A failed
render does not demote a live dirty session to `Failed`. The workspace retains
its document/annotation/history/view authority, exposes a real GPUI Component
Alert and Retry Button, opens a checksum-bound replacement worker, validates
page geometry and rotation, rejects stale candidates, and atomically swaps the
resource presentation. Crashed-worker cleanup now has a typed terminal path
that waits for the child and removes the owned surface root.

Passed: focused deterministic 1/1 in 3 seconds, real `SIGKILL` recovery 1/1 in
9 seconds, warm all-targets 134 plus six gated ignores in 36 seconds,
source/guard 17/17, exact prepared digest, one GPUI identity across 870
packages, configured dependency policy, and scoped Rust formatting. Failed:
only retained red TDD iterations; no required gate remains failing. Blocked:
native visual/accessibility, production PDFium supply, and Windows inherited
source handles. Not run: packages, physical devices, Hibbeler, IME, and matched
performance. The complete Rectangle inspector is now the next cutover-critical
journey; micro-control parity remains subordinate to that saved-document path.

## 2026-08-26 Rectangle inspector update

The retained Rectangle inspector is now integrated as a viewport sibling on
the pinned Longbridge/Zed graph. GPUI Component owns presentation controls;
stable application document and annotation entities own values, history,
locking, saving, and reopen. The full working Electron property matrix passes a
real checksum-pinned PDFium Save As and fresh-workspace reopen with exact
identity/appearance/rotation/lock and the established coordinate tolerance.
Hatch and Cloud are explicitly recorded as Electron baseline defects rather
than silently copied into the native contract.

Passed: focused 1/1, real 1/1, model 1/1, PDF persistence, adapter, all-targets
137 plus six gated ignores, source/guard 17/17, exact prepared digest, one GPUI
identity across 870 packages, dependency policy, and scoped formatting. Failed:
only retained red iterations; no required gate remains failing. Blocked or not
run: native visual/accessibility, packaged candidates, physical input/IME,
shipping PDFium, Windows inherited handles, Hibbeler, and matched performance.
The runner pins a fixed 16 MiB Rust test-thread stack for this large journey;
all disk, target, job, timeout, offline, and retention bounds remain unchanged.

## 2026-08-26 in-place Save stack update

The application-owned document session and the GPUI-free PDF persistence module
now share one explicit destination contract. Opened-source saves use guarded
atomic replacement; generated and new-path saves retain the no-overwrite Save
As path. The persistence module returns a publication receipt, so an error after
the namespace already changed cannot make the shell retain stale state or claim
that the old file remains authoritative.

Passed development evidence: focused 7/7, real PDFium 1/1, all-targets 144 plus
seven ignores, guard/source 17/17, prepared digest
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
shared PDF-engine SHA-256
`5974f9088e3b37a523b3afbf0605e69302f0e9545b2da44e1b0f83bce7fce904`,
one Zed GPUI identity across 870 packages, and configured dependency policy.
Blocked or not run: Windows replacement/source-handle ownership, packaged
platforms, native accessibility/visual proof, production PDFium, Hibbeler, IME,
and matched performance. The next stack seam is the serialized application
close owner choosing in-place Save for ordinary documents and Save As only for
generated documents.

## 2026-08-26 application-close stack update

The close stack now has one application-owned transaction from the GPUI
Component modal to `DocumentWorkspace` and the GPUI-free persistence boundary.
Ordinary document saves run on the background executor, generated documents
pause for Save As, and all document releases complete before the shell emits a
quit intent. Stable tokens bind every completion; duplicate request, stale
result, published-warning, Save failure, and release failure paths cannot
advance the transaction incorrectly.

Passed development evidence: pure 10/10, integration 13/13 plus one gated
ignore, checksum-pinned real PDFium 1/1, all-targets 150 plus seven ignores,
guard/source 17/17, prepared digest
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
one Zed GPUI identity across 870 packages, and configured dependency policy.
The policy currently emits two missing-license-metadata and five
unmaintained-transitive warnings without a forbidden source, license, or
vulnerability failure. Blocked or not run: generated-close native picker
continuation, Windows replacement/source handles, production PDFium, native
visual/accessibility, packaged platforms, physical input, IME, Hibbeler, and
matched performance.

## 2026-08-26 native-ingress stack update

One deep application-owned interface now coordinates PDF open selection and
execution for picker, menu, system, and drop origins. `DocumentWorkspace` owns
stable IDs, sequential open order, duplicate policy, batch generation,
activation, failure retention, and resource disposal. GPUI Component owns only
the rendered Alert and Dismiss Button. The normal native entry point submits a
single system batch, so startup no longer bypasses duplicate or partial-failure
policy. Native paths stay as `PathBuf`; only display copy is lossy.

Passed development evidence: native-open 5/5, native launch 4/4, all-targets
174 plus seven ignores, story/worker build, guard/source 17/17, exact prepared
digest, 870-package single-GPUI graph, configured dependency policy, scoped
formatting, and host-storage bounds. Failed evidence is retained red TDD output
only. Blocked or not run: native menu/drop and platform event adapters, live
visual/accessibility, production PDFium, Windows save authority, packages,
physical devices, IME, Hibbeler, and matched performance.

## 2026-08-26 save-target authority and recovery stack update

The Unix development stack no longer passes a picker pathname as ambient
authority. `SaveAsTargetAuthority` binds the exact native target, a retained
parent directory descriptor and identity, the destination leaf, and a one-shot
consumption state. `AuthorizedPdfStage` owns exclusive staging, revalidates both
the open stage and its directory name before publication, uses no-overwrite
linking, and performs identity-checked cleanup. The outer persistence owner now
records cleanup ownership explicitly, so an error after moving the stage cannot
fall back to unsafe pathname deletion.

The application-close owner also has a rendered typed recovery layer. Picker,
target, save, post-publication warning, and resource-release failures retain
stable transaction/document identity and expose the smallest safe next action.
The state remains application-owned; GPUI Component `Alert` and `Button`
instances render it and dispatch commands only.

Passed development evidence: authority 7/7, pure close state 11/11, close
integration 19/19 plus one gated ignore, real mixed-document PDFium 1/1, warm
all-targets 163 plus seven gated ignores, source/guard 17/17, exact prepared
digest, 870-package single-GPUI graph, configured dependency policy, and scoped
formatting. Failed evidence is retained red output only. Blocked or not run:
Windows target-handle authority, shipping PDFium, native visual/accessibility,
packaged candidates, physical input/IME, Hibbeler, and matched performance.
The migration remains an experiment; these proofs do not authorize promotion
or release.

## 2026-08-26 generated Save As close-stack update

The close stack now owns one native-path request token for a generated
document. GPUI supplies only the selected path; the application-owned
transaction validates the `.pdf` extension, snapshot revision, and token before
it dispatches the existing background saver. A cancel, picker error, stale
result, invalid extension, or changed document cannot advance save or quit.

Passed development evidence: pure 11/11, integration 17/17 plus one gated
ignore, checksum-pinned real mixed-document PDFium 1/1, all-targets 155 plus
seven ignores, guard/source 17/17, prepared digest
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
one Zed GPUI identity across 870 packages, and configured dependency policy.
The policy still emits two missing-license-metadata and five
unmaintained-transitive warnings, with no forbidden source, license, or
vulnerability failure. Blocked or not run: capability-bound cross-platform
target authority, visible failure recovery, native picker ownership/filter
semantics, Windows replacement/source handles, production PDFium, native
visual/accessibility, packaged platforms, physical input, IME, Hibbeler, and
matched performance.

## 2026-08-26 native application adapter update

The pinned graph supports this application boundary without a fork. GPUI
provides the macOS operating-system menu, `on_open_urls`, and real
cross-platform `ExternalPaths` drop events. GPUI Component provides the
Linux/Windows `AppMenuBar` over the same `OwnedMenu` model. Application-owned
adapters now feed startup/macOS/drop requests into `DocumentWorkspace` and
route File-menu close and quit actions through `ApplicationCloseWorkspace`.

The exact limitation is explicit: at Zed revision
`8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc`, only macOS invokes
`on_open_urls`. Linux and Windows retain the callback but have no delivery
callsite, and GPUI has no single-instance service. A later platform slice must
add Unix-domain-socket delivery on Linux and named mutex/pipe delivery on
Windows, then prove package file associations on the physical platforms.

Passed development evidence: native-application 6/6, exact story/worker build,
all-targets 180 plus seven gated ignores, source/guard 17/17, exact prepared
digest, one GPUI identity across 870 packages, and dependency policy with no
denial. No accepted gate fails. Native visuals/accessibility, physical macOS
delivery, Windows/Linux second-instance delivery, packages, production PDFium,
Windows target authority, IME, Hibbeler, and matched performance remain blocked
or not run as categorized in the parity ledger.

## 2026-08-26 native-shell transaction stack update

The retained stack now proves one complete Rectangle transaction through a
real GPUI Component-owned window surface instead of composing completion from
separate proofs. `ApplicationCloseShell` owns presentation and command routing;
`ApplicationCloseWorkspace` owns the close transaction; `DocumentWorkspace`
owns stable session and annotation identity; and the GPUI-free persistence and
PDF worker modules own publication and resource release. A fresh GPUI window
then reopens the published PDF and independently hydrates the stable Rectangle.

Passed Linux development evidence: exact real journey 1/1 in seven seconds,
build guard 13/13, and warm all-targets 180 active plus eight gated ignores in
15 seconds. The owned target remained below four GiB and no worker or Cargo
process remained. Failed evidence is retained TDD output only. Blocked or not
run: shipping PDFium, Windows save authority, live native visual/accessibility,
packages, physical devices, IME, Hibbeler, and matched performance. The next
cutover slice should deepen ordinary save/open failure recovery before adding
platform IPC or further micro-interaction parity.

## 2026-08-26 ordinary-save recovery stack update

Ordinary Save and Save As now use one typed failure boundary owned by
`NativeDocumentSession`. GPUI Component remains a strict presentation layer:
its real `Alert` and `Button` primitives render the state and dispatch Retry,
Save As, or Dismiss commands. The document owner retains annotations, path
authority, page state, and worker/resource lifetime. The separate dirty-close
owner is unchanged.

Passed Linux development evidence: the real occupied-target recovery journey
passes 1/1, the focused in-place recovery gate passes, all-targets passes 181
plus nine gated ignores, source/guard passes 17/17, prepared digest
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`
and the 870-package single-GPUI graph are unchanged, and dependency policy has
no denial. The retained red run is diagnostic only and is fixed. Blocked or not
run: Windows target authority, shipping PDFium, live native
visual/accessibility, packaged candidates, physical input/IME, Hibbeler, and
matched performance. Electron remains the rollback product.

## 2026-08-26 controlled viewer stack update

The real document shell now owns one controlled GPUI Component toolbar. The
dependency direction is unchanged: `NativeDocumentSession` owns view state and
the PDF worker/planner/cache; GPUI Component owns standard control presentation
and overlay behavior; a shallow experiment adapter maps semantic control events
to session commands. Stored session observers update the controlled component
snapshot outside render. No upstream patch, fork, or second component system is
required.

Passed Linux development evidence: the two-document checksum-pinned viewer
journey passes 1/1, all-targets passes 181 plus ten gated ignores, source/guard
passes 17/17, the prepared digest is unchanged, the graph remains one GPUI
identity across 870 packages, and dependency policy has no denial. The exact
runnable story contains one toolbar and one zoom-menu cache identity. CAD stays
at the isolated compatibility boundary until its state enters the real document
model. Production PDFium, Windows target authority, native
visual/accessibility, packages, physical input/IME, Hibbeler, cache-pressure
qualification, and matched performance remain blocked or not run.

## 2026-08-26 real-session tab consolidation update

The real document shell now owns the core rich tab journey. Actual
`NativeDocumentSession` entities are reordered by stable `DocumentId`; the
active, dirty, annotation, pending-close, view, and resource state does not move
by index. Pinned GPUI Component `TabBar`, `Tab`, `Button`, and `Popover`
primitives remain the standard presentation. Experiment-owned code is limited
to the Electron six-pixel gesture boundary, focus/bounds identity, and polite
status adapter. No upstream fork or graph change is required.

Passed Linux development evidence: focused 2/2 in 31 seconds; warm all-targets
183 active plus ten gated ignores in 47 seconds; source/guard 17/17; unchanged
prepared digest; one GPUI identity across 870 packages; dependency policy; and
host storage bounds. Loading/failed retention, close-origin isolation, keyboard
and pointer reorder, dirty Cancel/Discard, successor focus, stable annotation
identity, and resource release pass. Pointer edge auto-scroll and persisted
order are deferred. Production PDFium, Windows target authority, native
visual/accessibility, packages, physical input/IME, Hibbeler, and matched
performance remain blocked or not run.

## 2026-08-26 Callout stack update

The dependency direction remains unchanged for Callout. `DocumentWorkspace`
owns the two-click workflow and text-editor lifecycle; the GPUI-free annotation
model/adapter owns stable identity, history, and geometry; the persistence
layer owns canonical native reconciliation; GPUI Component owns the real
`Button` and `Textarea`; and raw GPUI owns only document/annotation paint and
pointer geometry. The exact pinned Longbridge/Zed graph required no fork or
revision change.

Passed Linux development evidence: rendered Callout 1/1, focused persistence
green, checksum-pinned real PDFium Save As/reopen 1/1, corrected Length
interaction 1/1, warm all-targets 187 active plus fourteen gated ignores in 33
seconds, source/guard 17/17, unchanged prepared digest, one GPUI identity across
870 packages, configured dependency policy, and host storage bounds. Failed
evidence is retained TDD diagnostics for the clipped appearance stream and the
off-screen Length test click; both are fixed. Existing-Callout editing, full
pointer handles/body drag, text appearance parity, properties, and Cloud+
remain partial. Production PDFium, Windows authority, native
visual/accessibility, packages, physical input/IME, Hibbeler, corpus breadth,
and matched performance remain blocked or not run.

## 2026-08-26 Cloud+ stack update

Cloud+ preserves the established dependency direction without an upstream
fork. `DocumentWorkspace` owns the creation/editor lifecycle and stable session
identity. The GPUI-free annotation model, adapter, routing module, and PDF
persistence boundary own the logical aggregate, history, geometry, paired
native representation, and typed reopen. GPUI Component owns the real `Button`
and `Textarea`. Raw GPUI owns only PDF and annotation paint, hit testing,
pointer geometry, and selection handles. The exact Longbridge/Zed revisions and
single-GPUI graph did not change.

The source receipt now covers the new checksum-bound `cloud_plus_routing.rs`
input as well as the changed model, adapter, persistence, and export surface.
The real fixture journey proves multiline box growth and leader rerouting,
stable geometry edits, canonical adjacent PolygonCloud/FreeTextCallout output,
independent PDFium pixels, `qpdf`, worker/resource release, and one logical
Cloud+ after a fresh-workspace reopen. The real red loop also corrected the
Cloud+ rectangle comparison to the same 0.0001-point PDF round-trip tolerance
already used for its control and leader points.

Passed Linux development evidence: model 1/1, routing 1/1, focused adapter,
paired persistence 4/4, rendered workspace 1/1, checksum-pinned real PDFium
1/1, corrected Image-toolbar interaction 1/1, warm all-targets 188 active plus
fifteen gated ignores in 22 seconds, source/guard 17/17, unchanged prepared
digest, one GPUI identity across 870 packages, configured dependency policy,
and host storage bounds. Failed evidence is retained TDD output for multiline
growth, PDF-number tolerance, and the clipped test target; all are fixed and no
accepted gate fails.

Existing-object Cloud+ pointer selection/manipulation, eight resize handles,
page/obstacle routing context, exact scallop output, complete properties, and
native accessibility/IME remain partial or not run. Production PDFium
redistribution, Windows target authority, and live native visual/accessibility
are blocked. Packages, physical macOS/Windows input, Hibbeler/third-party
Cloud+ corpora, and matched Electron/GPUI performance are not run. Electron
remains the shipping rollback.
