# Phase 4 interaction parity

Reference contract and native assessment for canvas interaction appearance and behaviour. Source audits completed 2026-09-17 against the Electron implementation (main checkout) and the native crate (`experiments/gpui-migration/gpui-migration/src/`). This file records step 1 of the delivery order (reference and contracts); it claims no implementation or visual acceptance.

## Reference contract (Electron)

### Selection feedback roles

`apps/desktop/src/renderer/src/pdf-tools/interactionChrome.ts:53-115`, rendered in `components/AnnotationLayer.tsx:3094-3216`.

| State | Stroke | Width | Dash | Halo | Handle |
| --- | --- | --- | --- | --- | --- |
| Selected | `#2563eb` | 1.5 | `5 4` | 5 | 7px `#facc15` / `#111827` |
| Focused (primary selected id) | `#1d4ed8` | 1.75 | `5 4` | 5 | 7px |
| Hovered (includes live marquee candidates) | `#93c5fd` | 1.25 | `4 3` | 4 | 6px |
| Draft | `#0f766e` | 1.5 | `6 4` | 5 | none (stroke = draft colour) |

Neutral halo `rgba(255,255,255,0.92)` under-strokes bounds; hot handle fills `#facc15` with 2px `#111827` stroke and +1px size; rotate handle is circular and hidden while hovered. `boundsOutsetPx: 0` in all states.

### Marquee selection

`pdf-tools/selectionMarquee.ts`, `AnnotationLayer.tsx:855-986,1414-1562`.

- Initiation: pointer drag on empty canvas starts a lasso; sub-threshold pointer-up arms a click-click box completed by the next pointer-down.
- Kind: end.x >= start.x is window (containment: every point of every component inside); otherwise crossing (intersection: any vertex inside or segment intersection, epsilon 1e-8, collinear on-segment counts).
- Operation: Alt removes (wins over Shift), Shift adds, otherwise replace. Empty active marquee with replace clears; add/remove leave selection unchanged.
- Thresholds: marquee activation 6px latched; placement-draft drag 3px; polygon close 10px. Do not conflate.
- Direction lock: lasso kind latches on first |dx| > 6 and never updates; box recomputes every move; unresolved lasso defaults to window.
- Rendered region (`AnnotationLayer.tsx:2278-2307`, only when active): window fill `rgba(37,99,235,0.13)` with solid `#2563eb` 1.5px stroke; crossing fill `rgba(34,197,94,0.14)` with dashed `#22c55e` 1.5px stroke (`7 5`). Cursor is crosshair while any marquee object exists.
- Click-away on empty canvas with non-empty selection and replace clears immediately and starts no marquee. `pointercancel` clears draft, marquee and capture without commit.

### Tool activation and persistence

`components/RightRail.tsx`, `App.tsx:1090-1120`, `AnnotationLayer.tsx:1020-1044`, `state/viewerStore.ts:368-413`.

- Rail single click selects (double-click second press is dropped by the `clickCount <= 1` guard: one selection plus one properties toggle per double-click). Double-click toggles properties without re-selecting; disabled tools are inert.
- Shortcuts: Select `V`, Pan `Space` (hold), Text `T`, Rect `R`, Ellipse `E`, Arc `Shift+C`, Line `L`, Arrow `A`, Dimension `Shift+L`, Length `Shift+Alt+L`, Polylength `Shift+Alt+Q`, Area `Shift+Alt+A`, Polyline `Shift+N`, Polygon `Shift+P`, Pen `P`, Highlight `H`, Cloud `C`, Cloud+ `K`, Callout `Q`, Image `I`, Snapshot `G`.
- Tool stays active after placement; the new markup becomes selected with click-away disarm armed (`postPlacement`). Fresh text-box commits neither select nor arm. Tab switch restores per-tab tool (`image` reverts to select) and clears post-placement and pending assets.

### Hold versus click

`App.tsx:1311-1445`, `utils/toolShortcuts.ts:42-175`.

- Space keydown (non-interactive target, repeat ignored) stashes the current tool and activates pan; keyup restores it. Hold works on keydown alone; no movement threshold.
- Two Space keydowns within 300ms toggle pan/select permanently and clear any hold stash. Explicit tool change clears the stash.
- Shortcut guards: tool shortcuts blocked with meta/ctrl held and in editable/combobox/dialog/grid/listbox/menu/tree targets; Space-pan additionally requires a non-interactive target.
- No release-outside-canvas special case (pointer capture delivers pointer-up). No focus-loss or window-deactivation handler exists: held-pan and drafts survive deactivation.

### Placement contracts

`AnnotationLayer.tsx`, `pdf-tools/annotationLifecycle.ts:4-5,137-241`. Global minimums: rectangle w&h > 2pt, line length > 2pt; reverse direction is normalised everywhere; no outside-page clamping.

| Family | Gesture | Commit / cancel |
| --- | --- | --- |
| Rectangle, ellipse, line/arrow, redact | Drag, or click-click second press | Pointer-up past 3px threshold, or second click; Escape resets; sub-threshold drag silently drops |
| Length | Two clicks | Second click commits past 2pt; Escape discards |
| Polylength / area | Pointer press per node | Double-click appends and commits (>= 2 / 3 points), Enter commits, Escape discards unconditionally |
| Polyline / polygon | Pointer press per node, start-marker closure at >= minimum (3 / 2) within 10px | Enter and Escape both finish a valid draft and discard an invalid one |
| Cloud freehand | Drag past threshold | Commits past length gate; sub-threshold pointer-up converts to node mode |
| Cloud node mode | Pointer press per node, 10px closure at >= 3 | Enter/Escape commit iff >= 3 nodes, else discard |
| Arc | Three clicks (start, end, bulge; 8px minimum bulge) | Third click commits; no Enter/Escape branch beyond global reset |
| Dimension | Two clicks, then text edit | Second click commits past line gate; global Escape only |
| Text box | Pointer-down ghost, in-place editor | Blur, outside pointer-down, or Escape commit; empty text discards |
| Image / snapshot | Click consumes pending asset / click-click rect plus rasterise | No asset: click ignored; failures clear asset with error |
| Pen / highlight | Drag with 0.5pt sample spacing | Pointer-up commits; tap without drag places nothing |
| Select move / handle transform | Pointer-down on body or handle with capture | Sub-threshold release is click-select with no mutation |
| Calibration pick (modal) | All pointer-down diverted to calibration points | Second point must share the page |

Cursors: select `default`, pan `grab`, marquee and calibration `crosshair`, image `none` under preview ghost, transform-drag `crosshair`/`none` when snapped. Select All is current-page scope; Delete keeps locked markups with a locked status.

### Modifier table

- Shift constrains creation orthogonally (line, measurement and cloud nodes to anchor; ellipse to circle; arc bulge to angle/minimum) and is re-read per pointer event, so mid-drag changes apply live.
- Shift-click toggles selection; Shift-marquee adds; Alt-marquee removes with Alt dominant over Shift regardless of order.
- Cmd/Ctrl own zoom, select-all and clipboard; tool shortcuts reject Cmd/Ctrl (macOS menus own accelerators).
- Double-click on a markup opens properties without moving; rotate-handle double-click resets rotation.

### File-drop open

Reference (`app.tsx:1290-1307`, `utils/droppedPdfFiles.ts`): drops filter to `.pdf` names, open every match in new tabs (`forceNewTabs`), track a per-drop busy counter, authorise each file through the main process, and surface failures as an error message. Empty and non-PDF-only drops are silent no-ops.

Native (`application_close_workspace.rs:1329-1347`, `native_application.rs:329-339`, `document_workspace.rs:5881-5899`): root drop handler forwards to `open_documents` with `Drop` origin; `stable_pdf_paths` plus `is_pdf_path` reproduce the PDF-only filter; `Drop` origin forces new tabs. Drop-failure error surfacing and busy indication are unconfirmed and join the queue below.

### Fullscreen transition

Reference (`app.tsx:343,416,1490-1499,1579`, `utils/macosFullScreenLayout.ts`): the renderer subscribes to window fullscreen changes, exposes `data-window-fullscreen`, and in macOS fullscreen hides the custom title bar and the app menu bar while keeping menu-bar visibility state.

Native (`bin/gpui-migration.rs:356-364`, `native_application.rs:13,255`, `application_shell.rs:40`): `ToggleApplicationFullScreen` is menu-wired and calls `window.toggle_fullscreen()` plus refresh. No `is_fullscreen` layout branch was observed, so title-bar/menu adaptation and traffic-light clearance in fullscreen join the queue below.

### Interruption hierarchy

Highest precedence first: open snap popover consumes Escape; active marquee cancels; vertex-path draft finishes on Enter or Escape; measurement-path draft discards on Escape; cloud-node draft commits iff >= 3 nodes else discards; text-box editor commits on Escape while callout/cloud-plus/dimension editors cancel; otherwise global Escape clears held-pan and resets to Select. Tool switch with any draft discards draft, preview, marquee and hover/snap state. Drafts live in per-page layer state, so tab switch unmounts them.

## Native assessment

Crate `experiments/gpui-migration/gpui-migration/src/`, audited 2026-09-17.

- Gesture state is emergent: `ActivePointer` (~30 variants) for in-drag gestures, six pointer-less staged drafts (vertex-path, measurement-path, cloud, cloud-plus, arc, snapshot), and a workspace-retained pointer gating move/up delivery (`annotation_adapter.rs:532-909`, `document_workspace.rs:2155,11976-12966`).
- Marquee: every marquee starts as lasso; box exists only as the post-click armed state applied on the next press. Thresholds match the reference (6px marquee, 3px drag). Direction is X-only with lasso kind latched after first |dx| > 6. Hit-testing branches containment versus intersection (`selection_geometry.rs:88-177`). Alt-remove wins over Shift-add (`selection_geometry.rs:34-44`).
- Shared feedback layer owns blue selection, yellow handles, white halo, dark outline, inert locked grey and both marquee styles (`interaction_chrome.rs`); hover, focus and draft roles are absent from the shared layer.
- Staged completion: Enter finishes vertex/measurement/cloud/cloud+ drafts; double-press commits measurement/cloud-plus/Select while Cloud uses press-plus-finish; polygon/cloud/cloud-plus auto-close within 10px. Minimum points and 2pt/8px gates mirror the reference.
- Interruption: ordered Escape handler exits pan, cancels picks, finishes vertex/cloud/cloud+ drafts, else resets to Select; pointer-ID guards cancel on capture loss; tool change and tab/page switches cancel retained gestures. Focus-loss-to-cancel wiring is unconfirmed in source, except hold-pan focus-loss, which Gap 2 replicates deliberately (no handler, matching the reference).

## Parity gaps (implementation queue)

Observed in source; each needs a focused correction plus a regression, then live Mac evidence. Not exhaustive; the matrix rows in `gpui-migration.md` remain the acceptance authority.

1. Hover, keyboard-focus and draft feedback roles landed in the shared layer (`interaction_chrome.rs`: colours, style table, unit tests; source guard extended). Focused id is maintained per document in the model and the hover candidate is tracked on the workspace with Select hit-test parity, cleared on exit and press. Paint scope threads hovered/focused ids through `annotation_layer` into every family painter and resolves outline colour per annotation, with draft handle suppression and handle-only draft gates. Verified 2026-09-18: 173 lib tests, 141/141 harness suite, 25/25 real journeys, full `pnpm check`; visual acceptance open. Explicitly deferred to the geometry slice: stroke widths, dash patterns, halo under-strokes, handle sizes (7/6px plus hot state), hover handles on unselected items, and marquee-candidate hover highlighting.
2. Hold-Space temporary pan and the 300ms double-tap toggle landed (`space_pan.rs` state machine plus workspace wiring): Space keydown on canvas-level focus stashes the tool and pans, keyup restores exactly; two keydowns within 300ms toggle pan/select permanently and clear the stash; repeats ignored; explicit tool changes funnel through `set_annotation_tool` and clear the stash; Escape clears the stash; platform/ctrl+Space never pans. Canvas presses claim `workspace_focus` (DOM mousedown parity) only when focus sits inside the workspace subtree and no popover-class overlay owns the interaction. Middle-button pan remains absent (canvas stays left-button only). Decisions: focus-loss and window-deactivation REPLICATED, not fixed (no handler, matching the reference; a stuck hold self-heals on the next Space keyup, and explicit changes clear the stash first so nothing stale is restored); grab cursor deferred to the cursor-policy slice. Regression cover: 8 state-machine unit tests, a real-window journey (hold/restore, repeat-ignore, double-tap both directions, explicit-clear, tab-focus guard, canvas focus claim), and the existing recent-signature journey, which caught an ungated focus-steal wedge during placement. Verified 2026-09-18: 181 lib tests, 142/142 document_workspace harness suite, 25/25 real journeys, full `pnpm check`; plus live visual verification on this Mac (native app, disposable doc, isolated data dir, driven via CUA background input and timed key events): double-tap Space toggles Hand selected with persistence after release, and toggles back to Select, confirmed in pixels (`gap2-visual/win-pan-on.png`, `win-pan-off.png`), AX selected flags, and handler probe log (`BeginHold`/restore pairs, `TogglePermanent`, release-no-restore); single taps hold and restore exactly; frontmost app unchanged across background input (no focus steal). The transient hold shows no visible change by design (no cursor affordance until the gap-10 slice). Evidence under the approved temp dir `/private/var/folders/w1/p2m47ml11dxdgdf0djz_dzf00000gn/T/opencode/gap2-visual/`. (The all-targets harness run shows 6 frozen-geometry failures in the document_tab_bar target that reproduce identically at pristine HEAD 4163cac, so they are pre-existing and unrelated to this slice.)
3. One-shot reset divergence: native commits reset straight-line, callout, ellipse, redact, arc, snapshot, polyline/polygon/polylength/area/cloud-plus/image to Select, while the reference keeps the tool active with post-placement click-away disarm.
4. Length/Dimension reject pointer-drag creation entirely (pointer-down errors); only the explicit two-click API works.
5. Escape draft coverage is partial: measurement-path, arc, snapshot and length/dimension-pending states fall through to generic reset instead of their reference outcomes.
6. No Enter-to-commit for Length/Dimension/Arc/Snapshot.
7. Marquee never starts as a box drag; direction is X-only with no top/bottom branch.
8. No time-based hold disambiguation; no touch/double-tap gesture path beyond desktop click counts.
9. `cancel()` forwards only Domain gestures to the document; other in-flight gestures are dropped without a compensating cancel command.
10. Focus-loss cancellation (other than hold-pan, decided under Gap 2), per-tab tool restore, double-click-to-properties, page-scoped Select All with locked retention, live marquee-candidate hover feedback, and the per-tool cursor policy are unconfirmed or absent and each needs an explicit audit against its matrix row before implementation.
11. File-drop open: filtering and new-tab placement exist natively, but drop-failure error surfacing and busy indication are unconfirmed against the reference (`app.tsx:1290-1307`).
12. Fullscreen transition: the native toggle is menu-wired but no fullscreen layout adaptation was observed; the reference hides the custom title bar and app menu bar in macOS fullscreen (`macosFullScreenLayout.ts`). Traffic-light clearance and state subscription across repeated transitions need an explicit check.

## Next action

Work the gaps in order: shared hover/focus/draft roles first, then hold/click behaviour and tool persistence, then per-family placement and modifier corrections. Each correction ships with a focused regression; matrix rows close only on deterministic checks plus representative real Mac input and visual evidence with user acceptance. Do not infer acceptance from green tests.
