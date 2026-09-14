# GPUI migration roadmap

## Objective and working agreement

Recreate the Electron application's workflows and recognisable workspace region by region in GPUI Migration. Use the pinned Longbridge GPUI Component controls and standard tokens, with product-owned PDF and annotation canvas rendering. Preserve document compatibility and safe saves. The existing native implementation is the starting point.

All implementation, screenshots and runtime checks currently run on the Mac. Use Computer Use for app interaction and captures. Keep the Electron reference read-only. Xcode is a Mac build dependency, not a restriction on the application's cross-platform architecture.

For each region: show and obtain approval of the exact reference crop; agree its included/excluded behaviour and stock component composition; implement; compare typography, alignment, icons, states and interactions; test; show reference/before/after at matching dimensions for implementation sessions; obtain user acceptance. Planning sessions show reference/current only. Short interaction recordings may explain intent but do not justify custom animations. Obvious unapproved differences or missing functional evidence fail acceptance.

### Spatial and boundary acceptance gates

Spacing verification is an explicit pass/fail check, not inferred from visibility or clickability. Inspect each control's full hit target and hover/focus treatment inside its container, including clearance from rounded corners, icon centring, label-to-icon spacing, internal leading/trailing padding, repeated inter-control gaps and region/window-edge insets. Check short and truncated labels, selected/unselected states and crowded layouts. A visible clickable icon that crowds its container still fails visual acceptance. Record measured geometry or close-up screenshot evidence; use standard spacing/component APIs for corrections rather than shrinking hit targets or adding arbitrary offsets.

Current flow: user approved the whole-workspace integration region after the properties toolkit corrections and joined template/CAD removal review. Execute [workspace-integration.md](workspace-integration.md); keep existing menu/tab/rail/properties gaps in their owning briefs rather than treating visual approval as a full functional pass. Lessons are consolidated in ux-review.md. Local Markdown is the only durable execution state.

Compare separator lines and borders (presence, ownership, position, thickness and semantic colour), internal button padding, gaps between buttons/groups, and insets against the app and region edges. Compare each whole group's position and alignment relative to its correct containing region; incorrect centring or anchoring fails acceptance even when individual controls look correct. Use stock component sizes and spacing/border tokens, not custom pixel imitation.

Show each proposed region with a labelled context margin sufficient to expose its neighbouring boundaries. Distinguish the approved implementation bounds from the surrounding comparison-only context; obtain crop approval before a new brief or implementation. Retain native-resolution detail crops as well as contextual captures. Include representative empty/loaded and constrained-window states, and measure intended equal gaps/alignment rather than relying on an impression.

Integration must verify which regions abut or intersect, continuity and ownership of separators (no duplicate or missing borders), and the effect of opening/resizing sidebars and rails on region widths and group positions. In particular, the top canvas viewer toolbar belongs between the left and right sidebars/toolbar rails, below the full-width shell bands; it must not span above those side regions. Its button group is centred within the canvas column, not the whole app. The current full-width isolated viewer candidate is provisional until those neighbours exist and this integration gate passes. Record this as deferred integration work, not visual parity or an accepted exception.

## Current state

| Region | State | Next action |
| --- | --- | --- |
| Comparison baseline and ownership | Agreed | Reuse frozen evidence; refresh deliberately if it becomes unavailable |
| Title bar | Previously accepted; merged-surface follow-up ready for review | Keep separate stock draggable row, centred dynamic title and native controls; matching menu background with stock bottom separator retained, per latest user decision |
| Application menu | Implemented; verification and acceptance still open | Resolve the specific gaps in [menu.md](menu.md) |
| Document tabs | Outline style accepted; full gate open | Verify coupled hover/close states and current-workspace regressions; see tabs.md |
| Top viewer controls | Narrow-layout repair tested; integration open | Final visual/accessibility review and later canvas-column adjacency; see viewer-controls.md |
| Left toolbar rail | User visually accepted, including corrected Files icon; remaining checks open | Preserve [left-rail.md](left-rail.md) verification gaps |
| Left sidebar | User visually accepted; scoped functional development gate verified | Preserve [left-sidebar.md](left-sidebar.md) tests and stock focus behaviour; broader integration/qualification remains separate |
| Right toolbar rail | Vertical rail, resizing and Highlight double-click verified; final comparison acceptance open | See right-rail.md for current evidence and property-family boundaries |
| Right expanded sidebar | Two-column and supported-slider correction implemented; focused tests and Mac comparisons ready for review | User acceptance pending for this correction; native capability gaps, remaining selected-family visual states and save/reopen coverage remain open in right-properties.md |
| Property inspectors | Queued | Decide grouping when the relevant region starts |
| Overlays and canvas-shell integration | Queued | Review shared focus, dismissal and integration once surrounding regions are agreed |

Use one shared command path and active-session-derived menu state. The shell owns region layout/visibility; regions own local focus and scrolling; domain state stays outside standard controls. Extract capabilities incrementally from DocumentWorkspace as required rather than restarting the implementation.

### Approved region crop: left toolbar rail

Frozen Electron source in the main checkout: test-results/parallel-pilot/reference-two-tabs-full.png (1152×768). Proposed implementation bounds: x=0,y=108,w=46,h=660, from below the tab band to the window bottom. Includes the narrow rail, its page-thumbnail toggle, app-edge inset and right separator. Excludes the thumbnail sidebar contents, viewer toolbar and tabs. The expanded sidebar is a distinct region at x=46,y=108,w=288,h=660; its header, page cards, actions, scrolling and resize boundary will receive a separate brief. Opening/closing that panel and the resulting rail selected state remain rail interaction checks.

Revised context capture: x=0,y=92,w=390,h=676, showing the entire expanded sidebar and a small canvas margin, not an ambiguous sliver. Evidence: test-results/parallel-pilot/proposed-left-regions-labelled.png; blue solid outline marks proposed rail scope, amber dashed outline marks sidebar context only. The labels and outlines are review annotations, not application styling. This replaces proposed-left-rail-context.png for approval. This historical reference has loading thumbnails outside scope; it is boundary/scope evidence, not settled-content or interaction proof. No region brief or implementation begins before approval of the revised crop.

## Source and evidence

Approved region: right tool rail, x=1068,y=108,w=84,h=660 in the frozen 1152×768 Electron reference. Main-checkout evidence test-results/parallel-pilot/proposed-right-rail-region.png includes unscaled context x=1008,y=92,w=144,h=676. Blue outline identifies the rail; canvas scrollbar and tab edge are context only. Scope includes top controls, grouped tool buttons, headings, separators and boundary insets. Excludes expanded properties sidebar, inspectors and annotation-engine redesign. The user approved the crop and explicitly rejected the GPUI horizontal annotation strip. See right-rail.md for implementation and remaining gates. Preserve stock GPUI focus behaviour per the user's latest direction.

Approved region: expanded Page Thumbnails sidebar. Frozen reference bounds x=46,y=108,w=288,h=660, including header, thumbnail cards and their page actions, scrolling and right resize boundary. The accepted left rail, tabs and canvas are context only. Main-checkout evidence test-results/parallel-pilot/proposed-sidebar-region.png uses the same x=0,y=92,w=390,h=676 context crop with a blue outline on the sidebar. User approved this spatial scope; the source's loading previews are not loaded-content acceptance. Sidebar top must meet tabs beside the viewer toolbar, not sit underneath that toolbar or the legacy tool row. See left-sidebar.md for implementation and remaining gates.

Active branch: codex/gpui-component-migration-spike. Active crate: experiments/gpui-migration/gpui-migration. Build and dependency instructions remain in that crate's README, FOUNDATION and source-preparation policy. The former gallery and other native prototypes are retired.

Historical reference/menu captures are preserved in the main checkout under test-results/gpui-migration-archive: comparison-baseline, title-bar, menu-baseline, application-menu-region, menu-development, menu-focus and descender-fix. Archived scripts and orchestration receipts are historical, not runnable planning sources. Current comparison evidence remains under test-results/parallel-pilot. These are local disposable evidence, not committed assets. Several worktree renames, deletions and code changes predate this cleanup and remain uncommitted; preserve them.

Approved component exceptions: exact official Button/Tab descender backport; narrowly scoped local disabled-submenu property/open-path fix. Both remain documented and checksum-bound in the preparation patch. No general styling fork was approved.

## Completion rules

### Verified execution model

Use disposable Mac bundles with distinct IDs and BP_GPUI_DATA_DIR/TMPDIR for scoped interaction checks. The earlier two-instance smoke test demonstrated app targeting without noticeable foreground interference, not guaranteed simultaneous input, clipboard isolation or general hover support. Evidence remains under test-results/parallel-probe in the active checkout. Coordinate shared inputs and keep one owner per instance.

Menu, tabs and viewer changes are integrated into the active migration source. Native-application tests now pass 10/10, including current-workspace hover enter/exit and stable geometry; viewer window tests pass 10/10 after standard edge-inset correction. The old standalone tab/template suite now executes: 18 pass and six expose legacy geometry/drag assumptions; it is not the current rendered tab bar. Do not silently delete those contracts or claim the whole suite passed. Region briefs contain current remaining work.

Current scope is still Mac-only. Full canvas-column integration awaits sidebars/rails. New regions require crop approval; already approved repairs do not require repeated implementation permission.

Record passed, failed, blocked and not-run checks separately. A build or a source-pattern test does not establish runtime behaviour. User acceptance, development verification and production qualification are distinct. Do not mark a gate complete while a required test is blocked or while a command's effect is unverified.

The earlier statement that the full menu development gate passed was too broad; menu.md is the corrected current state. Production/default-app/updater work and broader native-alpha qualification remain in [backlog.md](backlog.md).

## Replaced planning

This roadmap supersedes GitHub migration maps/specifications #41, #82 and #107 and their open work items #85–90 and #112–114. Closed historical decisions #108–111 informed the agreed baseline, ownership and title bar. Historical issue text and archived research remain reference material, not an execution queue. Old performance protocols and Linux evidence do not authorise current non-Mac execution.
