# GPUI migration roadmap

## Objective and working agreement

Recreate the Electron application's workflows and recognisable workspace region by region in GPUI Migration. Use the pinned Longbridge GPUI Component controls and standard tokens, with product-owned PDF and annotation canvas rendering. Preserve document compatibility and safe saves. The existing native implementation is the starting point.

Mac remains the production qualification host. The accepted Phase 1 checkpoint also has a development-only Linux orb trial using the reviewed OpenGL fallback; this does not replace native Mac qualification. Keep the Electron reference read-only. Xcode is a Mac build dependency, not a restriction on the application's cross-platform architecture.

For each region: show and obtain approval of the exact reference crop; agree its included/excluded behaviour and stock component composition; implement; compare typography, alignment, icons, states and interactions; test; show reference/before/after at matching dimensions for implementation sessions; obtain user acceptance. Planning sessions show reference/current only. Short interaction recordings may explain intent but do not justify custom animations. Obvious unapproved differences or missing functional evidence fail acceptance.

### Spatial and boundary acceptance gates

Spacing verification is an explicit pass/fail check, not inferred from visibility or clickability. Inspect each control's full hit target and hover/focus treatment inside its container, including clearance from rounded corners, icon centring, label-to-icon spacing, internal leading/trailing padding, repeated inter-control gaps and region/window-edge insets. Check short and truncated labels, selected/unselected states and crowded layouts. A visible clickable icon that crowds its container still fails visual acceptance. Record measured geometry or close-up screenshot evidence; use standard spacing/component APIs for corrections rather than shrinking hit targets or adding arbitrary offsets.

Current flow: Phase 1 workspace-shell acceptance is recorded at checkpoint `95bfb62725cf1248bcdfb24c28796c686bdb58ae`. The Phase 2 annotation-tool family slices pass the guarded Linux development matrix: shared Rectangle/Ellipse, Line/Arrow, Pen/Highlight, Text Box, Polyline/Polygon, Length/Polylength/Area, Cloud, Callout, Cloud+, Dimension, Arc, semantic snapping, Redact, Snapshot, Image and Signature. The follow-on parity slice now also closes the declared Length appearance, Image/Signature opacity and Polylength/Area text-style gaps across defaults, selected-object controls, rendering and PDF save/reopen. This closes the agreed tool-slice traversal and its declared appearance gaps, not whole-product parity or production qualification. Phase 3 covers secondary surfaces and workflows before Mac production qualification. Lessons are consolidated in ux-review.md. Local Markdown is the only durable execution state.

Compare separator lines and borders (presence, ownership, position, thickness and semantic colour), internal button padding, gaps between buttons/groups, and insets against the app and region edges. Compare each whole group's position and alignment relative to its correct containing region; incorrect centring or anchoring fails acceptance even when individual controls look correct. Use stock component sizes and spacing/border tokens, not custom pixel imitation.

Show each proposed region with a labelled context margin sufficient to expose its neighbouring boundaries. Distinguish the approved implementation bounds from the surrounding comparison-only context; obtain crop approval before a new brief or implementation. Retain native-resolution detail crops as well as contextual captures. Include representative empty/loaded and constrained-window states, and measure intended equal gaps/alignment rather than relying on an impression.

Integration must verify which regions abut or intersect, continuity and ownership of separators (no duplicate or missing borders), and the effect of opening/resizing sidebars and rails on region widths and group positions. In particular, the top canvas viewer toolbar belongs between the left and right sidebars/toolbar rails, below the full-width shell bands; it must not span above those side regions. Its button group is centred within the canvas column, not the whole app. The current full-width isolated viewer candidate is provisional until those neighbours exist and this integration gate passes. Record this as deferred integration work, not visual parity or an accepted exception.

## Current state

| Region | State | Next action |
| --- | --- | --- |
| Comparison baseline and ownership | Agreed | Reuse frozen evidence; refresh deliberately if it becomes unavailable |
| Title bar | Phase 1 accepted | Preserve the separate stock draggable row, centred dynamic title and stock bottom separator |
| Application menu | Phase 1 accepted; production actions deferred | Preserve [menu.md](menu.md) semantics and the reviewed accessibility patch |
| Document tabs | Phase 1 accepted | Preserve actual-workspace tab regressions; keep the legacy seam audit in tabs.md |
| Top viewer controls | Phase 1 accepted | Preserve canvas-column ownership and constrained reachability |
| Left toolbar rail | Phase 1 accepted | Preserve [left-rail.md](left-rail.md) behaviour and stock focus |
| Left sidebar | Phase 1 accepted | Preserve [left-sidebar.md](left-sidebar.md) tests; broader native qualification remains separate |
| Right toolbar rail | Phase 1 accepted | Preserve right-rail grouping, resizing and tool routing |
| Right expanded sidebar | Phase 1 accepted baseline | Qualify annotation families incrementally without redesigning the accepted shell |
| Property inspectors | Phase 2 tool slices and declared appearance gaps complete on Linux | Qualify the completed matrix on Mac separately |
| Overlays and canvas-shell integration | Phase 2 tool slices and declared appearance gaps complete on Linux | Preserve the accepted shell while closing production-host gaps |
| Secondary surfaces and workflows | Phase 3 native-local slices implemented on Linux; secure/platform signature capabilities remain open | Add a reviewed secure-storage boundary plus camera/relay adapters before claiming complete signature parity; then qualify the whole phase on Mac |

Use one shared command path and active-session-derived menu state. The shell owns region layout/visibility; regions own local focus and scrolling; domain state stays outside standard controls. Extract capabilities incrementally from DocumentWorkspace as required rather than restarting the implementation.

### Phase 3: secondary surfaces and workflows

Phase 3 closes user-facing workflow parity outside the completed annotation-family traversal. Work in bounded slices and preserve the accepted shell. For each visual surface, capture and approve the Electron reference scope before implementation, then exercise the real GPUI workflow rather than treating model presence or compilation as parity.

Implement and qualify the slices in this order:

1. **Templates:** audit the existing native template manager and persistent library; complete the New from template picker, last-used-template affordance, built-in/custom/imported/saved-template journeys, previews, removal confirmation and New Blank PDF entry path.
2. **Snap settings:** extend the existing semantic-point popover to the Electron-visible source, construction-grid, spacing, dimension-increment and guide settings only where the corresponding runtime behaviour is real; persist settings at the same ownership boundary as the Electron app.
3. **Signatures:** retain the working local draw/import path and add typed signatures, camera/phone transfer where supported, recent-signature storage/reuse and confirmed removal. Treat transfer security, sensitive-state cleanup and platform capability failures as product requirements, not cosmetic states.
4. **Contextual actions:** add the canvas context menu and reconcile page/sidebar action menus for tool switching, zoom/fit, page scale and page rotation without duplicating command logic.
5. **Resilience and interaction polish:** qualify destructive/reset and colour-preset confirmations; loading, failed-open, import/save errors, save collisions, external changes, worker recovery and session restore; keyboard/focus/IME, shortcut and tooltip discoverability; popup containment and minimum-window behaviour.

A slice is complete only when its applicable Electron workflow is reachable in the native app, its state and mutations use the established application/domain authorities, deterministic tests distinguish success and failure paths, and representative default, open, error and constrained-window states have been inspected in the running UI. Save-related workflows additionally require independent close/reopen evidence and must retain unknown PDF content. Record unsupported platform capabilities explicitly rather than presenting inert controls.

Current Phase 3 implementation status on the Linux development host:

- Templates retain the persistent built-in/generated/imported library, last-used selection, split New-from-template entry point, generation/import/materialisation flows, constrained manager layout and failure recovery. Removing a custom template now opens a stock destructive confirmation before mutating the library.
- Snap Settings exposes only sources with real native geometry: annotation candidates, constant-time construction-grid snapping/visibility/spacing, dimension-increment fallback, targets and guide display controls. PDF-content and generated-page-grid sources remain hidden because native candidate extraction does not exist. Like Electron, these settings remain session-owned rather than persisted.
- Signatures support bounded local drawing, imported-image sanitisation and deterministic typed-signature rasterisation through one placement/history/PDF path. Secure Recent work is in progress: `recent_signature_store.rs` provides a tested fail-closed XChaCha20-Poly1305 file store whose key is isolated in the native OS credential service, with a five-item newest-first cap, deduplication, confirmed-removal plumbing and no plaintext fallback. Its focused eight-test store mode passes, and the workspace UI/state plumbing compiles, but the native application has not yet bound the store at startup or received final rendered interaction coverage; do not claim the Recent workflow complete. Camera capture is not implemented because it has no reviewed native capture adapter. Phone draw/image transfer is not implemented because the native app has no reviewed HTTPS relay client, QR encoder or secret-zeroisation lifecycle. No insecure relay fallback or inert capability controls were added.
- The page canvas uses a stock context menu whose tool, zoom/fit, page-scale and rotation entries dispatch the existing workspace actions rather than duplicate mutations. Existing page and sidebar controls remain the alternate discoverable paths.
- Existing native open/import/save errors, save collision recovery, external-change detection, worker recovery, session restore and reset/destructive confirmation coverage were requalified rather than reimplemented. Platform production qualification remains separate.

Focused guarded evidence is stored in the active crate's ignored `.prepared/evidence/` directory. The accepted local checks include `semantic-snapping-workspace`, `compat-signature`, `template-manager` and `contextual-actions-workspace`; retain the exact summary receipts generated by the bounded runner when handing this phase to Mac qualification.

Native updater UI and scheduling, release links, default-PDF registration, distributable PDFium, application identities, signing/notarisation and packaging remain production work in [backlog.md](backlog.md). They are not Phase 3 completion gates. After Phase 3, perform Mac whole-product production qualification before promotion or Electron retirement.

### Approved region crop: left toolbar rail

Frozen Electron source in the main checkout: test-results/parallel-pilot/reference-two-tabs-full.png (1152×768). Proposed implementation bounds: x=0,y=108,w=46,h=660, from below the tab band to the window bottom. Includes the narrow rail, its page-thumbnail toggle, app-edge inset and right separator. Excludes the thumbnail sidebar contents, viewer toolbar and tabs. The expanded sidebar is a distinct region at x=46,y=108,w=288,h=660; its header, page cards, actions, scrolling and resize boundary will receive a separate brief. Opening/closing that panel and the resulting rail selected state remain rail interaction checks.

Revised context capture: x=0,y=92,w=390,h=676, showing the entire expanded sidebar and a small canvas margin, not an ambiguous sliver. Evidence: test-results/parallel-pilot/proposed-left-regions-labelled.png; blue solid outline marks proposed rail scope, amber dashed outline marks sidebar context only. The labels and outlines are review annotations, not application styling. This replaces proposed-left-rail-context.png for approval. This historical reference has loading thumbnails outside scope; it is boundary/scope evidence, not settled-content or interaction proof. No region brief or implementation begins before approval of the revised crop.

## Source and evidence

Approved region: right tool rail, x=1068,y=108,w=84,h=660 in the frozen 1152×768 Electron reference. Main-checkout evidence test-results/parallel-pilot/proposed-right-rail-region.png includes unscaled context x=1008,y=92,w=144,h=676. Blue outline identifies the rail; canvas scrollbar and tab edge are context only. Scope includes top controls, grouped tool buttons, headings, separators and boundary insets. Excludes expanded properties sidebar, inspectors and annotation-engine redesign. The user approved the crop and explicitly rejected the GPUI horizontal annotation strip. See right-rail.md for implementation and remaining gates. Preserve stock GPUI focus behaviour per the user's latest direction.

Approved region: expanded Page Thumbnails sidebar. Frozen reference bounds x=46,y=108,w=288,h=660, including header, thumbnail cards and their page actions, scrolling and right resize boundary. The accepted left rail, tabs and canvas are context only. Main-checkout evidence test-results/parallel-pilot/proposed-sidebar-region.png uses the same x=0,y=92,w=390,h=676 context crop with a blue outline on the sidebar. User approved this spatial scope; the source's loading previews are not loaded-content acceptance. Sidebar top must meet tabs beside the viewer toolbar, not sit underneath that toolbar or the legacy tool row. See left-sidebar.md for implementation and remaining gates.

Active branch: codex/gpui-component-migration-spike. Active crate: experiments/gpui-migration/gpui-migration. Build and dependency instructions remain in that crate's README, FOUNDATION and source-preparation policy. The former gallery and other native prototypes are retired.

Historical reference/menu captures are preserved in the main checkout under test-results/gpui-migration-archive: comparison-baseline, title-bar, menu-baseline, application-menu-region, menu-development, menu-focus and descender-fix. Archived scripts and orchestration receipts are historical, not runnable planning sources. Current comparison evidence remains under test-results/parallel-pilot. These are local disposable evidence, not committed assets. Several worktree renames, deletions and code changes predate this cleanup and remain uncommitted; preserve them.

Approved component exceptions: exact official Button/Tab descender backport; narrowly scoped local disabled-submenu property/open-path fix; opt-in Outline-tab Button states; and menu-row AccessKit disabled/checked-state projection. Each remains separately checksum-bound by source preparation. No general styling fork was approved.

## Completion rules

### Verified execution model

Use disposable Mac bundles with distinct IDs and BP_GPUI_DATA_DIR/TMPDIR for scoped interaction checks. The earlier two-instance smoke test demonstrated app targeting without noticeable foreground interference, not guaranteed simultaneous input, clipboard isolation or general hover support. Evidence remains under test-results/parallel-probe in the active checkout. Coordinate shared inputs and keep one owner per instance.

Menu, tabs and viewer changes are integrated into the active migration source. Native-application tests now pass 10/10, including current-workspace hover enter/exit and stable geometry; viewer window tests pass 10/10 after standard edge-inset correction. The old standalone tab/template suite now executes: 18 pass and six expose legacy geometry/drag assumptions; it is not the current rendered tab bar. Do not silently delete those contracts or claim the whole suite passed. Region briefs contain current remaining work.

Phase 1 closure evidence on Linux: clean preparation reproduced component digest `35254d5f899bb03514766c834996cc9025f16e06ad19ffd4fcfb3e32c105dd69`; the guarded native-application suite passed 25/25; the guarded native reader build passed; and `pnpm check` passed 1088 application plus 23 relay tests. The freshly rebuilt reader rendered the checksum-locked 100-page public fixture on the orb desktop, and Page Down advanced to page 2 without a black client. Vulkan under llvmpipe remains incompatible with GPUI in this orb, so the trial uses a development-only OpenGL fallback. The shared-source receipt set was audited against accepted checkpoint `95bfb627`: five stale consolidation-time hashes were corrected to those exact tracked bytes, all other receipts already matched, and full `prepare verify` now passes. Production qualification remains Mac-owned.

Phase 2 Rectangle evidence on Linux: the guarded real-shell journey creates, moves, resizes, undoes/redoes, saves on close, validates with qpdf/pdfinfo, freshly reopens, deletes and reopens again while preserving stable identity and releasing workers/surfaces. The focused real-inspector journey additionally preserves stroke/fill/opacity/style, geometry, rotation and lock through Save As and a distinct-workspace reopen, with independent PDFium pixel evidence. Passing receipts are `.prepared/evidence/button-probe-20260914T110717Z-130378.*` and `.prepared/evidence/button-probe-20260914T110707Z-130045.*`. The live native reader created a Rectangle through actual pointer input, enabled Fill through the rendered inspector, saved the owned fixture copy, closed and freshly reopened it with a clean tab and the Fill toggle still enabled; qpdf reported no syntax or stream errors. Relaunching at 1280×720 renders the complete client, but maximizing a window created at the earlier larger desktop size leaves the newly exposed lower OpenGL area black until relaunch. This is a development Linux resize limitation, not Mac production qualification.

Phase 2 remaining-family evidence on Linux: the final source-bound matrix exercises the rendered family controls and identity-checked workspace paths through pointer/keyboard edits, history, locking where supported, Save As or in-place save, qpdf/pdfinfo validation, typed persistence, fresh-workspace reopen, independent PDFium pixel checks where applicable, deletion and worker/surface cleanup. Snapshot now includes real capture, move/resize, 30-degree rotation, double-click reset, Undo, 45% opacity, lock/no-op gestures, save/reopen and object-graph deletion. PDF numeric reopen comparisons accept only bounded serialization rounding; materially different values remain rejected. Signature now has the same source hashes and runtime verification contract as the other focused real-PDF modes. The final receipt paths are recorded in `right-properties.md`. This matrix is Linux development qualification only and does not fill absent native capabilities.

Record passed, failed, blocked and not-run checks separately. A build or a source-pattern test does not establish runtime behaviour. User acceptance, development verification and production qualification are distinct. Do not mark a gate complete while a required test is blocked or while a command's effect is unverified.

The earlier statement that the full menu development gate passed was too broad; menu.md is the corrected current state. Production/default-app/updater work and broader native-alpha qualification remain in [backlog.md](backlog.md).

## Replaced planning

This roadmap supersedes GitHub migration maps/specifications #41, #82 and #107 and their open work items #85–90 and #112–114. Closed historical decisions #108–111 informed the agreed baseline, ownership and title bar. Historical issue text and archived research remain reference material, not an execution queue. Old performance protocols and Linux evidence do not authorise current non-Mac execution.
