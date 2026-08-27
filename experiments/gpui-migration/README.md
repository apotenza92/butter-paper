# Butter Paper GPUI migration review

This directory is the first migration milestone. It is independent from the
tracked Butter Paper application and is preserved on the dedicated
`codex/gpui-component-migration-spike` branch so the experiment can continue on
another machine without changing production sources.

The branch contains the review pages, standalone Rust source, and build and
benchmark scripts. To keep the cross-machine handoff small, visual captures and
other reproducible output are deliberately excluded: Rust `target/`, the
generated application bundle, Poppler files, renderer caches, temporary
Electron user data, the nested runtime source copy, raw machine-specific
benchmark reports, and the Hibbeler PDF corpus. The macOS captures and Hibbeler
corpus were not transferred to this Linux VPS. The superseded Poppler-based
native macOS shell was inspected live through the approved read-only Computer
Use lane on 2026-08-22, but no portable screenshot file was transferred back.
The PDFium-based candidate has not had a current macOS visual run. Treat every
HTML reference to a missing capture as historical structure, not current
evidence.

## Open the review

From this directory, run:

```sh
python3 -m http.server 4177
```

Then open <http://127.0.0.1:4177/index.html>.

The review maps the Electron/Nova implementation at commit
`9e947ae4b43eb05c15e60b1ef9bb6c7f16444081` against the existing raw-GPUI
experiment and the planned coherent Longbridge GPUI Component recreation. The
older published 0.5.1 milestone and rejected 0.5.2 prototype remain historical
comparison evidence; the reviewed target is revision `c27f5d5c…`.

Electron/Nova is the product behavior contract and a visual comparison
baseline. Start from GPUI Component defaults instead of recreating Nova pixel
by pixel. Compare typography, density, geometry, hierarchy, component states,
responsive behavior, and native interaction. Record each intentional
difference as a better or worse user-experience tradeoff before accepting it.

`component-gates.html` is the component-system review. It records source
mappings, interaction and accessibility contracts, responsive behavior, and
coherence checkpoints for primitives, tabs, toolbar groups, both rails,
thumbnails, and constrained windows. Its image crops remain blocked until a
portable matched Electron/GPUI capture set is recreated.

[`ELECTRON-TO-GPUI-MIGRATION-AUDIT.md`](ELECTRON-TO-GPUI-MIGRATION-AUDIT.md)
maps the complete maintained Electron product to the current direct-GPUI
experiment. It separates the implemented comparison candidate from the much
larger full migration, defines the native module seams, and orders the remaining
work by dependency and end-to-end acceptance gates.

The current comparison candidate is no longer only a viewer shell. It uses an
isolated PDFium worker and bounded shared BGRA surfaces that become GPUI
`RenderImage` values without a PNG or base64 hot path. Its visible annotation
surface implements representative Rectangle, Highlight, Text Box, Length, and
Image workflows through one typed document model. These representatives share
selection, lock/delete, history, dirty state, document scenes, and thumbnail
scenes. A separate `lopdf` persistence adapter writes native annotation
dictionaries and appearance streams, reimports the representative annotations,
survives two save/reopen cycles, and preserves an untouched unknown annotation
plus original page content, boxes, and metadata in the focused Linux fixture.

This is enough implementation breadth to prepare a technical investment
comparison. It is not a completed Butter Paper migration or a distributable
candidate. Multi-selection now has a real all-family pointer and group-command
slice with lasso/window/crossing selection and the two-click box form.
Page-major cross-family annotation order now survives save and two independent
reopen cycles while untouched PDF annotation references retain their slots.
Cross-page selection policy remains. The
remaining tools, snapping, templates, signatures, platform integration,
accessibility, packaging, updates, and release qualification remain outside the
comparison-candidate boundary.

## Current native foundation source of truth

The current isolated application candidate is the prepared Longbridge GPUI
Component revision
`c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4` on exact Zed GPUI revision
`8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc`. Its prepared tree digest is
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`.
The compatibility graph has one Zed GPUI identity and uses a checksum-bound
Apache-2.0 tracing shim in place of the rejected GPL-marked tracing chain.
The PDFium manifest remains development-only with `productionApproved: false`.

The GPUI-CE gallery is historical implementation and comparison evidence. It
is not the application candidate and cannot qualify the current graph. Run
`node scripts/foundation-truth.mjs` from `gpui-component-compat/` before a
guarded Rust acceptance build.

## Cutover strategy

The accepted component probes prove that the pinned Longbridge/Zed stack can
express Butter Paper controls. They do not prove a migrated application. Work
now advances by complete document journeys, not by isolated micro-interactions.
Pointer edge auto-scroll is deferred.

The two Rust applications cannot be combined by rendering the GPUI-CE gallery
inside the Zed-GPUI component shell: their GPUI types have different Cargo
identities. The no-fork route is to keep the prepared Longbridge/Zed graph as
the sole UI graph and reuse the gallery's GPUI-free model, PDF worker, writer,
and viewer-planning modules through a no-default-features dependency or small
experiment-owned feature crates. The GPUI-CE image conversion and raw gallery
shell must be ported; the PDF/annotation domain code does not need to be
rewritten.

The editable `DocumentWorkspace` spine now exists in `gpui-component-compat`.
It owns stable-ID `NativeDocumentSession` entities, renders page and thumbnail
rasters through a thin Zed `RenderImage` adapter, dispatches thumbnail
navigation, rejects stale generations, preserves a live document across a
failed second open, and releases its abstract resource on close. Real GPUI
Component Rectangle, Pen, Text Box, and Length tools drive the GPUI-free annotation
adapter through native pointer and text input. Page and thumbnail overlays
share the PDF bottom-left transform and retained dirty revision.

The real Linux development journey now passes with the separately approved,
checksum-pinned PDFium input. The fetch policy accepted only the reviewed
`chromium/7881` x86_64 archive at 3,644,759 bytes and SHA-256
`1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d`.
It installed only below the ignored experiment cache and retained
`productionApproved: false`. The guarded real test proves non-uniform page and
thumbnail pixels from the checksum-pinned 100-page fixture, a rendered stable-ID
thumbnail click that advances the retained page through the real background
worker, failed-second-open isolation, continued use of the first worker, worker
PID exit, and mapped-surface cleanup. Its first strengthened run exposed
an empty failed-open surface directory; the resource constructor now removes
only its identity-specific output on failure, and the regression passes.

The first non-destructive editing journey also passes. `SaveDocumentRequest`
captures stable document identity, a dedicated save generation, target path,
current page, and exact annotation revision. An injected failure leaves the
original path, worker, annotations, selection, and dirty state unchanged. Save
As also binds the open-time source SHA-256 and refuses an externally changed
source. The real guarded journey publishes a new 100-page PDF, independently
reopens typed Rectangle, Pen, and Text Box annotations through lopdf and the page through PDFium, passes `qpdf
--check` and `pdfinfo`, proves changed page pixels, swaps the worker only after
validation, marks the captured revision saved, and releases the old and final
workers.

The multi-document ownership journey now passes too. The runnable workspace
uses real GPUI Component `TabBar`/`Tab` primitives for the actual stable
`DocumentId` sessions. The exact Electron dirty-close
title, warning, and Cancel/Discard/Save order render in a controlled real
Popover. Deterministic tests prove Cancel, failed close-save retry, validated
close-save, Discard, successor selection, and targeted resource release. The
real PDFium test keeps two worker processes live, closes one clean session, and
proves the other worker and saved page state remain usable until their own
close.

The first visible native viewer slice now passes too. Each retained document
owns a private `DocumentViewerState` with its own Continuous/Single Page mode,
zoom, scroll handle, raster revision, planner generation, two-job queue, and
256 MiB byte-accounted tile cache. The real GPUI tree preserves the existing
page preview while asynchronous PDFium tiles arrive, then composes those tiles
at crop/device-scale coordinates under stable page and tile IDs. Viewport
measurement and scroll changes replan on the next frame. A changed mode, zoom,
page, close, or validated Save As rejects stale work; Save As also cancels the
old planner and clears pixels from the replaced worker. The exact story now
binds the proven GPUI Component toolbar mode and zoom controls to this
application-owned state instead of keeping a duplicate story-only toggle.

The real guarded journey renders visible page-0 tiles at 800%, selects the real
second thumbnail, invalidates the prior generation, renders visible page-1
tiles, preserves the bounded cache, and still proves worker and mapped-surface
cleanup. Spatial evidence now compares complete BGRA pixels instead of adjacent
color channels.

This is development evidence, not native visual, accessibility, package,
shipping, or redistribution approval. No graphical session exists on this
VPS, so the exact story was compiled but not launched. The imported Rectangle,
Ellipse, Pen, Highlight, Text Box, Length, and regular Image cutover slices now hydrate
real PDF annotations at clean revision 0, select and edit them through native
pointer input, paint resize/rotation selection chrome and smooth Pen paths,
expose real GPUI Component Select,
Rectangle, Pen, history, line-width, Pen-opacity, lock, and delete controls,
and preserve stable identity through undo, redo, Save As, independent reopen,
and native deletion. The Text Box slice uses a real GPUI Component `Textarea`,
commits multiline content on Escape or guarded blur, discards empty creation,
and paints retained text on the page and thumbnail scenes. Deletion removes the exact page `/Annots` reference and
the retired annotation object without rewriting unrelated objects. Length adds
page-scoped calibration, real `Shift+Alt+L`, two-click preview/commit, strict
minimum distance, Escape cancellation, atomic retained edits, and canonical
standard-Measure create/edit/delete Save As while leaving ordinary Dimension
annotations untouched. Pointer body drag, semantic snapping, and exact visual
appearance remain partial. Page Scale is now a revisioned application-owned
document mutation rather than an auxiliary control map. Real GPUI Component
Dialog, Select, Checkbox, NumberInput, ButtonGroup, Button, and visible pick
alert primitives cover preset, custom, and calibrated modes; all five units;
independent X/Y; decimal/fraction precision; current/all/range targets; and
session-only saved presets. Preset creation plus scale application is one undo
revision. Exact `/BPPageScale` data survives independent and checksum-pinned
real PDFium Save As/reopen, and full replacement removes stale metadata.
Calibration hover/snap visuals, complete native dialog input/accessibility,
platform packages, and physical-device proof remain. Highlight now precomposes stable Multiply bodies
into annotation-free PDFium page, thumbnail, and tile rasters. Regular PNG
Image now proves bounded decode, placement, move/free-resize, shared page and
thumbnail rendering, staged typed Save As/reopen, PDF appearance cleanup, and
renderer-resource release.

Ellipse now passes a cutover-critical Linux development journey. The retained
adapter and real GPUI canvas prove the exact three-pixel activation threshold,
filled-interior and unfilled-edge movement, eight curve-correct resize handles,
live move/resize preview, rotation, double-click rotation reset, lock
suppression, and stable IDs. The checksum-pinned real PDFium journey proves
create/edit/delete, typed Save As, independent reopen, stable identity, native
rendering, worker replacement, worker exit, and mapped-surface cleanup. Native
pixels, accessibility, packaging, and cross-platform qualification are still
not proved.

The first real template-to-document journey now passes as Linux development
evidence. A GPUI-free generator creates the Electron-default A3 landscape
Square Grid as an owned temporary `Untitled.pdf`. `DocumentWorkspace` opens it
through the checksum-pinned PDFium worker, renders non-uniform page and
thumbnail pixels, treats the unannotated temporary document as dirty, preserves
it across Cancel and failed Save As, validates a staged reopen, publishes the
selected target atomically, releases the old worker before deleting the
temporary source, and closes the reopened worker without mapped-surface
residue. This covers one representative built-in template. It does not cover
the full template library or manager, imported/custom templates, production
PDFium distribution, or packaged platforms.

Page rotation now passes as a Linux development-only document journey. Real
GPUI Component Rotate Left and Rotate Right buttons dispatch application-owned
page state. One rotation is one document revision; it updates page and
thumbnail geometry, rejects stale pixel results, preserves prior visible state
after an injected failure, participates in undo/redo and dirty-close state,
blocks Save As while replacement pixels are pending, writes canonical PDF
`/Rotate`, and survives independent reopen. Raster, crop, pointer projection,
annotation hit-testing, and source-point geometry cover all quarter turns.
`PaintedPageEvidence` binds stable document/page identity, request and resource
generations, viewer generation, native prepaint sequence, source PDF points,
contained window-logical bounds, and rendered device-pixel ratio. The capture
handshake freezes that authority before SIGUSR1, schedules exactly one later
GPUI frame, rejects post-signal drift, and authorizes cleanup only after the
matching presented-state receipt. This deterministic protocol proof is not a
native crop or benchmark result.

The application-close journey now passes as Linux development-only evidence.
One experiment-owned `ApplicationCloseShell` owns the GPUI Component modal
layer for the whole workspace. The retained close transaction snapshots stable
document identities and exact native `PathBuf` values, maps each Save As choice
to its transaction/request/document token, saves dirty documents serially, and
releases workers and mapped surfaces before emitting the final quit intent.
Save As cancellation dismisses the modal and preserves every live document;
stale cancellation is rejected; a non-UTF-8 Unix path survives the exact path
map. The real two-PDF lifecycle proves ordered Save All and Discard All effects,
worker exit, and surface cleanup. Releases run serially. If cleanup fails, the
transaction cancels and reports the message without `ReleaseAcknowledged` or
quit promotion. The targeted live session plus pending dirty-close and
close-after-save identities remain intact. Clean close and dirty Discard both
report `ReleaseFailed`, and a fresh retry succeeds. The PDFium resource marks
itself released only after worker close and `remove_dir_all`; failed cleanup can
be retried. Deterministic injected failure proves the retry path. The real
PDFium test proves normal release, but does not inject a real IPC cleanup
failure. This does not yet prove modal pointer hit testing, live focus
containment/return, or accessibility because this VPS has no `DISPLAY`. This
accepted journey does not make micro-control parity or the migration complete.

Line/Arrow now passes as a Linux development-only document journey. The shared
model/history/scene and interaction gate passes 26/26, and the component
workspace proves real GPUI Component Line and Arrow buttons plus separate
color, width, and opacity popovers. Stable IDs, shortcuts, drag and retained
click-click placement, Shift constraints, pointer exit behavior, scene
geometry, arrowhead geometry, one history revision per property change,
no-op suppression, bounds validation, lock suppression, undo/redo, and
Line/Arrow state independence pass. Typed `/Line` Save As and reopen preserve
the edited appearance plus Electron-defined standard fields, flags, subject,
contents, opacity, width fallback, and Arrow intent. The real PDFium journey
creates both types in the reviewed 100-page fixture, applies distinct retained
appearances, saves, reopens, rehydrates their stable identities and exact
appearance values, renders changed pixels, and releases both workers and
mapped surfaces. The current popovers expose representative presets rather
than Electron's complete free-form property inputs. `/BPAppearance`, fresh
`/M`, native overlay capture/accessibility, and packaged-platform proof remain
incomplete.

Multi-selection now passes as a bounded Linux development slice. Ordered
plain/Shift selection and group movement cover every maintained annotation
family with locked-member filtering. Application-memory copy/paste with the
exact repeated 12-point offset, mixed locked deletion, Select All, filter-only
undo/redo selection, generated-ID reconciliation after a fresh reopen, and real
Save As/PDFium reopen all pass. The native pointer bridge also proves the exact
six-pixel lasso threshold, two-click box form, window/crossing selection,
ordered replace/Shift-add/Alt-remove, cancellation, and transient overlay state.
The real Delete button remains enabled when the primary member is locked but an
unlocked peer can be deleted, and all-locked group drags no longer emit a false
edit event. The shared adapter passes 29/29
(`button-probe-20260825T204604Z-1779890.log`), the focused real workspace route
passes two multi-selection tests
(`button-probe-20260825T205607Z-1785910.log`). The PDF persistence gate passes
13/13 (`button-probe-20260825T205410Z-1784109.log`), the focused stable-order
model gate passes 1/1 (`button-probe-20260825T205821Z-1788015.log`), and the real
PDFium journey passes 1/1 with exact page-major order after Save As and reopen
(`button-probe-20260825T205630Z-1786456.log`). This is not full Electron parity:
cross-page selection policy is unresolved, Length-caption and rotated
Text/Image marquee geometry remain partial, and fresh native
visual/accessibility evidence is absent.

The next measurement decision must target this exact
Longbridge/Zed candidate; the existing GPUI-CE performance results are
historical. The exact-candidate protocol and guarded release profile now fail
closed, but native XGetImage crop proof and a timing-eligible run remain
blocked without a real graphical lane. Remaining annotation families,
recovery, package-relative worker/PDFium loading, and platform packaging
remain. See
the critical-path matrix and ownership seam in
[`MIGRATION-STACK-AUDIT.md`](MIGRATION-STACK-AUDIT.md).

## GPUI Component migration policy

Longbridge GPUI Component is the default component system for the isolated
migration. The reviewed upstream source is pinned to immutable revision
`c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4` from 24 August 2026. Do not follow
moving `main`.

The active gallery still pins community-maintained GPUI-CE commit
`c738623ffbcec2aeddc44a645cc6b74646d5cf97`; the reviewed Longbridge source is
not API compatible with that GPUI identity. The isolated
`gpui-component-compat/` probe instead uses Longbridge's intended exact Zed
commit `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc` through a checksum-bound source
preparation. The minimal preparation removes forced forbidden features and
replaces the reachable GPL-marked tracing interface with a local Apache-2.0
shim without copying GPL source.

That prepared graph now passes exact source, identity, license/source,
build-safety, real `Button`, real `ButtonGroup`, real Continuous-view
`DropdownButton`/`PopupMenu`, the paired Single Page control, the controlled
Zoom Out/Zoom In/percentage-preset group, the retained CAD View
`Button`/`Popover`/`ButtonGroup`/`NumberInput`, the composed viewer-toolbar strip,
the separate Document Tab Bar template action built from a real `TabBar`,
`Button`s, and controlled `Popover`, active document selection, and clean-tab
closing, plus the real non-modal dirty-close `Popover` with ordered Cancel,
Discard, and Save intent buttons, and capped document labels with the Electron
pointer-hover close mask, plus stable-ID Alt+Shift+Left/Right tab reorder with
an exact polite position-status seam, plus experiment-owned pointer drag with
Electron's strict greater-than-six-pixel activation and stable-ID reorder, and
Linux x86_64 all-targets development gates. The
tracers prove
stable IDs, exclusive view selection, independent wheel and zoom state,
pointer and keyboard actions, preset selection, Escape focus return, disabled
and double-click behaviors, CAD configuration/reset semantics, template
selection/create/manage state, active/inactive/last clean close, deterministic
successor selection and focus, dirty-close deferral, clamping, and non-shrinking
horizontal overflow. Dirty confirmation actions never close a tab or mutate
storage in this experiment; the tests prove exact copy and IDs, first-action
focus, non-trapped traversal, Escape/outside dismissal, keyboard focus return,
busy/repeated-request suppression, target identity, and unrelated-state
independence.
Keyboard reorder now proves both directions, no-wrap boundaries, literal
modifier matching, active-ID and same-tab focus retention, whole dirty and
template-tab data movement, pending dirty-confirmation identity, and unchanged
clean-close semantics. The stable `document-tab-reorder-status` renders as an
experiment-owned `Role::Status` node configured with AccessKit
`Live::Polite`. GPUI exposes no direct live-region builder at this pin, so the
probe uses its public accessibility subtree seam without changing upstream.
This compiles and passes deterministic state tests; it is not live assistive
technology evidence.
Pointer drag now proves below-threshold click behavior, translated preview,
both directions, adjacent and multi-position movement, same-position and edge
no-ops, close and non-primary-button isolation, active/focus and full tab-data
retention, cancellation, pending dirty-confirmation identity, exact move text,
and unchanged fixed geometry. Pointer edge auto-scroll remains deferred and is
not implied by this non-scrolling proof. The guarded final story gate passes 32
integration tests. The build runner now retains its owned Cargo target after
ordinary red tests, timeouts, and interruptions; it cleans automatically only
for free-space or target-size safety breaches, and records the disposition in
each summary.
The label slice preserves the real `Tab` label and its ellipsis path, applies a
190 px native cap derived from the unchanged 380 px strip, and proves stable
label/mask bounds for clean/dirty and active/inactive tabs. Electron defines no
document-label tooltip, so the native slice deliberately defines none. The
pointer mask preserves Electron's 34 px fade region and 14 px solid tail. The
pinned icon `Button` does not expose its internal focus handle to sibling label
content, so the matching keyboard-focus mask remains a recorded component gap;
the close button itself remains keyboard visible and activatable.
The viewer-toolbar content needs exactly 607 px without CAD and 667 px with CAD,
so its 480 and 320 px cases intentionally overflow. The Document Tab Bar fits
at the Electron 1200 px default and 900 px minimum; its 320 px constrained case
scrolls a measured 380 px intrinsic strip. A native story compiles, but this VPS has
no existing graphical session, so that
exact story has not been launched or captured. Do not add the
GPUI-CE and Zed identities side by side, and do not treat this development
proof as packaging or redistribution approval. See
[`MIGRATION-STACK-AUDIT.md`](MIGRATION-STACK-AUDIT.md) and
[`gpui-component-compat/README.md`](gpui-component-compat/README.md) for the
remaining gates.

Use GPUI Component whenever a reviewed equivalent exists. Use `gpui-base` for
difficult reusable behavior or geometry when Butter Paper owns presentation.
Keep application wrappers shallow. Record any hand-built primitive and its
capability gap in
[`COMPONENT-PARITY-LEDGER.md`](COMPONENT-PARITY-LEDGER.md).

Use this order within each cutover journey:

1. Record the production Nova component's visual, interaction, keyboard, accessibility, and constrained-window contract.
2. Select the exact GPUI Component or GPUI Base API from the pinned current source.
3. Implement one coherent vertical slice that ends in a runnable document journey and exercise it in the exact native story.
4. Add deterministic state and interaction tests before moving to the next component family.
5. Keep the PDF surface, annotations, selection handles, two-axis viewport behavior, and meaningful Butter Paper tool icons as domain UI.
6. Compare current Electron/Nova and the GPUI recreation at matching window sizes. Record improvements and regressions instead of assuming either implementation is better.

`pixel-overlay.html` remains a diagnostic harness for geometry regressions. It
does not replace interaction, accessibility, native platform, or
constrained-window checks.

## Historical gpui-component 0.5.2 prototype

`gpui-next-prototype/` is a rejected visual trial. It is a separate native
crate and does not import production sources. Its switcher presents three
different compositions from the same component defaults:

- **Workbench**: document sidebar, document tabs, compact markup tools, and a
  central page.
- **Focus**: low-chrome reader with a floating markup strip.
- **Review**: comments, document, and review-details columns.

The page is a representative Butter Paper-owned surface. It is not a PDF
renderer and is not Hibbeler evidence. On Linux Rust 1.93, run the isolated
compatibility gate:

```sh
cd gpui-next-prototype
./scripts/check-linux.sh
```

The pinned upstream revisions use `cold_path` and `atomic_try_update`, which
are newer than the VPS compiler. The script scopes the required compiler flags
to this throwaway crate. It does not change the repository or production
toolchain.

On Alex's Mac, Rust 1.96 builds the same lockfile normally:

```sh
cd gpui-next-prototype
TOOLCHAINS=com.apple.dt.toolchain.Metal.32023.917 \
DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer \
./scripts/build-macos-app.sh
open "target/Butter Paper GPUI Next.app"
```

The current Mac run passed native ARM64 compilation and created a verified
1320×892 on-screen window with bundle ID
`dev.butterpaper.gpui-next-prototype`. In the final instrumented launch, the
initial Workbench element-tree composition took 500 microseconds and the next
ten took 211–301 microseconds. This measures Rust element composition, not GPU
present time. macOS screenshot capture remains blocked by Screen
Recording permission, so the live window is not yet portable visual evidence.

## Historical raw-GPUI milestone on Linux

The gallery contains the current representative behavior slice. Its historical hand-built component
layer is under `gpui-gallery/src/butter_ui/`. The first slice contains the Nova
token mapping, embedded Geist font, icon, button variants and sizes, toggles,
button groups, and separators. The gallery state matrix and the real viewer
toolbar use the same components. The thumbnail toggle and tool rail also use
the button for real shell state.

The pinned GPUI revision uses `cold_path` and `atomic_try_update`, which are
newer than Rust 1.93 on this VPS. The gallery now pins the upstream-selected
Rust 1.97.1 toolchain in `rust-toolchain.toml`; do not use `RUSTC_BOOTSTRAP` as
foundation evidence. Use one Cargo job on memory-constrained hosts:

```sh
cd gpui-gallery
cargo check -j 1 --locked --features gallery --bin butter-paper-gpui-gallery

cargo check -j 1 --locked --features pdfium-worker --bin butter-paper-pdf-worker

cargo test -j 1 --locked --features gallery --bin butter-paper-gpui-gallery butter_ui::

rustc --edition 2024 --test src/component_model.rs -o target/component-model-tests
./target/component-model-tests
cargo run -j 1 --locked --no-default-features --bin component_state_benchmark
```

The historical gallery foundation gate is documented in
[`gpui-gallery/FOUNDATION.md`](gpui-gallery/FOUNDATION.md). Its prior GPUI-CE
dependency, license, provenance, advisory, and native results qualify only the
predecessor gallery. They do not qualify the current Longbridge/Zed candidate
or a distributable application. Current execution evidence belongs on the
applicable GitHub ticket.

The benchmark is headless and emits one JSON record. With a graphical session,
use the existing `BP_GPUI_PERF_SCENARIO` gallery scenarios. They emit explicit
first-frame, frame, and operation events. Rust element-tree construction and
state-loop time are not GPU present-time substitutes.

The current VPS exposes a Linux Direct Rendering Infrastructure (DRI) device.
The owned gallery builds and links here. The latest bounded Xvfb attempts with
D-Bus and Openbox did not map a GPUI window. A black root-window image is not
evidence. Those local Xvfb attempts remain failed iterations rather than GPU
performance evidence.

An authorized disposable NVIDIA RTX 4000 Ada lane on 2026-08-21 exposed and
then verified a Linux backend fix in the owned gallery. `gpui_platform` enabled
X11 and Wayland, but the direct `gpui` dependency had both backend features
disabled. At the pinned revision this made `gpui::guess_compositor()` select
the headless client even with a valid `DISPLAY`. The manifest now enables the
same explicit backends on both dependencies, and the foundation contract test
keeps that requirement visible.

The repaired binary mapped a viewable 1200×779 client inside the requested
1200×800 Openbox window. X.Org exposed DRI3 and Present, and Vulkan selected the
discrete NVIDIA device with driver 580.173.02. Captures verify Continuous,
the Continuous Mousewheel Behaviour menu in both Scroll and Zoom states,
Single Page, and the tab-bar New From Template menu. Local ignored evidence is
under `captures/gpu-owned-2026-08-21/` and
`performance/results/gpu-owned-2026-08-21/`. The transferred source-and-public-
fixture archive was SHA-256
`a2794e114d9a48d806a2e7ae746552f62e8d08e37cd17c4541b4b818f22b41ae`;
the six-page public fixture was SHA-256
`b9ea482bb4ee07f9124729141258723a6ff643866c8626e725b3416d45ea9fe9`.

Three cold public-fixture `open-first-page` runs reported first frame at
420.51–471.72 ms after process start and a visible viewport 171.44–202.71 ms
after the open request. One page-navigation run completed all 10 visible
operations in 1.02 s, and one zoom run completed all 12 in 1.72 s. These are
development smoke timings on a simple generated PDF. They are not GPU present
times, Hibbeler results, a packaged-app gate, or a matched Electron comparison.
The paid lane was destroyed after about 21 minutes 54 seconds; estimated compute
cost was $0.28 at $0.76/hour.

An earlier authorized disposable Linux GPU lane compiled the historical
gpui-component source and rendered it through NVIDIA Vulkan on X11. The RTX 6000 Ada process produced a
verified 1100×720 window, a 41.17 ms first-frame event, a 16.69 ms next-frame
interval, and visible interaction events for zoom, fit mode, and tool selection
at 29.37 ms, 27.51 ms, and 24.49 ms. Element-tree construction remained below
0.14 ms in the observed events. These are one-run smoke measurements, not a
statistical Electron comparison.

That historical run exposed a constrained-window failure at 800×600: the fixed toolbar
clips and is partly covered instead of collapsing or moving secondary actions
into overflow. Local ignored evidence is in
`captures/gpui-component-linux-gpu-*.png` and
`performance/results/gpui-component-linux-gpu-events.jsonl`. The paid lane was
destroyed after about 12 minutes; estimated compute cost was $0.31.

## Build and capture the GPUI gallery

Xcode 27 beta keeps the Metal compiler in an on-demand component. The component is installed on this machine as `com.apple.dt.toolchain.Metal.32023.917`. Keep selection scoped to each command:

```sh
cd gpui-gallery
node scripts/generate-lucide-assets.mjs
TOOLCHAINS=com.apple.dt.toolchain.Metal.32023.917 \
DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer \
cargo build
```

The generator reads the exact installed `lucide-react@1.8.0` package from the
local pnpm content store. It creates normal 1.5-stroke SVGs, 16 px rail variants
that reproduce Lucide `absoluteStrokeWidth`, and Butter Paper's composite
Continuous icon. GPUI loads these files with its native `AssetSource` and
`svg()` APIs. The app bundle copies the same assets into `Contents/Resources`.

Launch the gallery normally, or launch the equal-size shell capture mode:

```sh
BP_GPUI_CAPTURE_SHELL=1 \
TOOLCHAINS=com.apple.dt.toolchain.Metal.32023.917 \
DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer \
./target/debug/butter-paper-gpui-gallery
```

Pass one or more PDF paths to launch the shell with real documents in closable
tabs. The current vertical slice opens each source through an inherited
read-only descriptor, obtains page geometry from the isolated PDFium worker,
and renders bounded BGRA crops into shared mapped surfaces. The host converts
those bytes directly to GPUI `RenderImage` values. Poppler command-line tools
and PNG files are no longer part of the application render path. Set
`BP_GPUI_CACHE_DIR` to isolate a development or benchmark run, and provide the
development worker and checksum-pinned PDFium library when they are not inside
an app bundle:

```sh
BP_GPUI_CAPTURE_SHELL=1 \
BP_PDF_WORKER_EXE="$(pwd)/target/debug/butter-paper-pdf-worker" \
BP_PDFIUM_LIBRARY="$(node scripts/fetch-pdfium-development.mjs)" \
TOOLCHAINS=com.apple.dt.toolchain.Metal.32023.917 \
DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer \
./target/debug/butter-paper-gpui-gallery "/absolute/path/to/document.pdf"
```

The `+` action, File → Open, and Cmd/Ctrl+O open the same path through a native
file picker. Metadata, page, zoom, and thumbnail requests run on GPUI's
background executor. Page Up and Page Down move between pages. The thumbnail
sidebar and continuous document surface are GPUI `uniform_list` instances:
each virtualizes the complete document and renders only its visible range.
Mixed page sizes use authoritative per-page geometry, and continuous scrolling
updates the active page from the viewport center. Generation-tagged 1024-pixel
tiles, stale-result rejection, cancellation, a 32-tile render-plan limit, and a
declared 256 MiB tile cache bound the 1600% design case instead of allocating a
full 46,080 by 34,560 raster. The application image cache has a separate 128
MiB bound. Set `BP_GPUI_INITIAL_PAGE=8` for deterministic navigation and
`BP_GPUI_ZOOM=400` to launch at 400%.

The page surface and thumbnails also paint the same authoritative annotation
scenes. Rectangle and streamed Highlight use real pointer gesture lifecycles.
Text Box uses GPUI text input and editable layout bounds. Length derives a
scaled label and supports endpoint editing. Image uses a bounded decoded asset,
the Electron three-pixel move threshold, and eight-handle free resize. The shell
exposes shared undo, redo, lock, delete, and type-appropriate property actions.
Highlight state and saved PDF appearance retain `Multiply`; the maintained
workspace precomposes committed Highlight bodies. CropBox/rotation and fresh
native visual proof remain incomplete.

The viewer toolbar follows the current Electron action order and joined-control
geometry. Zoom Out and Zoom In use the production 1.1× step. The supported
range is 6.25–6400%. Fit Width and Fit Page use the production 24 px page gap
and 2% downward quantization rule against the current native window size.
Continuous now lays out a virtualized multi-page column; Single Page renders
only the active page. Cmd/Ctrl +, Cmd/Ctrl −, and Cmd/Ctrl 0 route through GPUI
actions and are also exposed in the native View menu. Continuous and Single
Page are separate two-segment controls: the primary segment selects the page
mode, and the chevron opens an anchored Mousewheel Behaviour menu with
independent Scroll/Zoom state. The selected wheel behavior is active, Ctrl
temporarily inverts it, and single-page scrolling accumulates trackpad/wheel
movement before changing pages.
The tab bar also has a two-segment New From Template control. Its primary
segment creates a valid blank PDF from the selected Letter/A4 template, while
its chevron opens the template menu. Each split control uses the Nova button-
group geometry: the first segment owns the left and shared-edge border, and the
second removes only its left border. This produces one outer outline and one
straight join without a second wrapper frame. Popup menus focus their composite root, expose the active item
to accessibility, support Up/Down/Enter/Escape, and restore the trigger focus
after keyboard activation or dismissal. The percentage chevron remains
visual-only. Matched native visual capture and native accessibility inspection
remain open behavior gates.

## Launch the standalone macOS app

Build the disposable app bundle with the worker and checksum-pinned development
PDFium artifact:

```sh
cd gpui-gallery
./scripts/build-app-bundle.sh
open "target/Butter Paper GPUI.app"
```

To launch the bundle with a PDF directly:

```sh
open -n "target/Butter Paper GPUI.app" --args "/absolute/path/to/document.pdf"
```

The script builds the gallery and `butter-paper-pdf-worker`, fetches the exact
reviewed PDFium 7881 development binary, copies both runtime artifacts into the
bundle, and ad-hoc signs the result. This is still disposable development
output. The supplier binary is not an audited, reproducible Butter Paper build,
and the rebuilt PDFium bundle has not yet passed a current macOS launch, visual,
accessibility, or packaging smoke. Earlier Poppler bundle results below are
historical evidence and do not qualify this runtime.

The capture mode creates the same 1200×800 logical surface used by the prior
Electron evidence. Export a future native 2× window capture at 1152×768 to
reproduce that evidence's 0.96 logical scale. The prior
`captures/gpui-native-current-1152x768.png` and its side-by-side, overlay,
difference, and region metrics were not transferred. The 2026-08-22 live Mac
inspection verified visible page-1 fixture text on a white surface and clean
joins for the New From Template, Continuous, and Single Page split controls.
It also found low-contrast toolbar borders and missing accessibility semantics
for the window title, thumbnails, PDF pages, and PDF text. Treat all named
capture files as historical references until a portable matched set is
recreated.

## Continuation gates

Continue in this order, keeping each gate independent and reviewable:

1. Complete the owned button-family atlas against Electron/Nova: normal, hover,
   focus, selected, disabled, long-title, keyboard, accessibility, and
   constrained states. Current Linux compile, render, and focused click checks
   pass. Live macOS inspection passes split geometry and PDF visibility, while
   low-contrast borders and incomplete PDF accessibility remain open.
2. Port the next family only after the current family has a real shell consumer.
   The likely order is tabs, tooltip, popup/menu, input/select, dialog, then
   contextual property controls. Record better and worse user-experience
   tradeoffs for each family.
3. Finish the exact comparison manifest on both runners. The current expanded
   scenarios are semantic diagnostics from inside each application. They are
   not operating-system native input or decision-eligible timing evidence. A
   Linux/X11 XTest lane is implementation work until a real GUI run proves it.
4. Close the remaining comparison-candidate correctness gaps: complete
   Rectangle transforms and properties, native-input replay, visual/density
   oracles, GPUI Highlight Multiply compositing, full Text Box font shaping,
   and production-grade PDF source-handle transfer on Windows.
5. Run the locked public corpus locally, then run calibration and the final
   randomized paired comparison on the authorized paid Linux GPU lane. The
   Hibbeler corpus remains optional blocked evidence because it was not
   transferred.
6. Treat a successful comparison as an investment decision only. Electron
   remains the rollback product until the full migration passes visual,
   behavior, accessibility, integrity, cross-platform, packaging, update, and
   release gates.

## Current check status

Passed:

- The real GPUI Component template split control now dispatches typed creation
  intents into the retained `DocumentWorkspace`. All six built-ins have
  deterministic bounded-vector generator coverage; the rendered Square Grid
  journey creates exactly one real temporary session and no mock tab. The
  focused gate passes 4/4 plus one separately gated ignore
  (`button-probe-20260826T002334Z-1934162.log`), the checksum-pinned real PDFium
  command passes 1/1 (`button-probe-20260826T002434Z-1935008.log`), and the warm
  all-targets gate passes 133 non-ignored tests plus four ignores in 26 seconds
  (`button-probe-20260826T002456Z-1935382.log`).

- Linux x86_64 deterministic coverage passes for the shared annotation model,
  GPUI annotation adapter, viewer layout and tile planning, PDF worker protocol,
  direct `RenderImage` adapter, and focused PDF persistence path.
- The representative native PDF fixture passes two save/reopen cycles,
  `qpdf --check`, `pdfinfo`, semantic reimport, native dictionary/appearance
  checks, original-document oracles, and byte-exact unknown-annotation probes.
- Foundation policy tests cover the exact GPUI-CE and `pdfium-render` Git
  revisions plus the checksum-pinned six-target development PDFium manifest.
- The performance contract, manifest, fixture-oracle, runner, cgroup, NVIDIA,
  paired-statistics, and decision-policy JavaScript tests pass locally. The
  current performance Node gate is 99/99 in 2703.916708 ms; the build-guard
  policy gate is 10/10 in 337.808436 ms.
- The page-rotation/capture-authority checkpoint passes the guarded focused
  protocol gate (14/14 in 11 seconds), story configuration (3/3 in 3 seconds),
  Linux signal guard (2/2 in 3 seconds), corrected document workspace (44
  passed with one separately gated ignored test in 12 seconds), and the real
  PDFium gate (1/1 in 6 seconds). The prepared Longbridge tree digest remains
  `630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`.
- The pure application-close gate passes 9/9 in 18 seconds
  (`button-probe-20260825T181935Z-1667638.log`). The integration gate passes 7
  tests with one separately gated test ignored in 6 seconds
  (`button-probe-20260825T182000Z-1668397.log`). It includes stale Save As,
  non-UTF-8 native paths, cleanup-failure state preservation, and fresh retry.
  The separately gated real two-PDF lifecycle passes 1/1 in 3 seconds
  (`button-probe-20260825T182023Z-1669025.log`). The current Line/Arrow
  acceptance passes the shared interaction gate 26/26, focused workspace gate
  3/3, PDF persistence gate 12/12, and real PDFium gate 1/1. The latest warm
  all-targets gate passes 125 non-ignored tests with three separately gated
  ignored tests in 45 seconds
  (`button-probe-20260825T205849Z-1788476.log`). The exact story and worker
  build passes warm in 10 seconds (`button-probe-20260825T205946Z-1789689.log`).
- The preceding cold all-targets gate passed in 520 seconds with symbols
  stripped and retained a 2,708,840 KiB target
  (`button-probe-20260825T173734Z-1615153.log`).
- Public generated fixture construction and validation pass for single-page,
  multi-page, annotation-density, and representative-annotation inputs.

Failed or interrupted checks:

- A cold template-bridge red build exceeded the fixed 4 GiB target cap and
  stopped safely. The profile now fixes `codegen-units = 1`; a clean cold red
  compile completed at about 2.1 GiB without weakening the cap. Owned cleanup
  requests bounded retries for transient `ENOTEMPTY`, and the fast guard gate
  passes 13/13. Compile/test reds retain the target. The current guarded warm
  gates above replace all diagnostic attempts as application evidence.

- The intentional red multi-selection proofs first exposed a false all-locked
  edit event, a mixed-selection Delete-button suppression defect, and imported
  generated-ID reuse (`button-probe-20260825T200853Z-1754718.log`,
  `button-probe-20260825T200938Z-1755518.log`, and
  `button-probe-20260825T201003Z-1755945.log`). Their targets were retained and
  the current guarded focused and full gates replace them. No current required
  deterministic test is failing.
- An earlier cold all-targets attempt correctly stopped on the 4 GiB target
  limit and cleaned only the owned target
  (`button-probe-20260825T173634Z-1613961.log`). This was a passed safety
  response to a failed build attempt, not application evidence. The subsequent
  strip-symbols cold run is the accepted replacement.

Blocked or not transferred:

- The private Hibbeler corpus and its current raw benchmark reports were not
  transferred. Public NASA evidence now replaces it for reproducible long-PDF
  development work, while the optional private lane remains blocked.
- Current matched Electron/GPUI macOS screenshots were not transferred. The
  prior live Mac inspection covers the superseded Poppler runtime and is not
  current PDFium-candidate or portable pixel-comparison evidence.
- Windows source-handle inheritance is not implemented. The worker currently
  receives an inherited read-only descriptor only on Unix.
- Highlight Multiply composition passes deterministic CPU and PDFium-oracle
  tests for the current unrotated zero-origin fixtures. CropBox/rotation and
  native screenshot evidence remain incomplete.
- Complete font shaping, embedding, fallback, and cross-viewer Text Box parity
  are outside the current Helvetica-based persistence slice.
- The exact full-workload runner coverage remains blocked at 10 of 31 Electron
  commands and 13 of 31 GPUI commands. The GPUI editor scenario now executes all
  13 representative editor commands and emits typed per-command evidence, but
  it remains a semantic diagnostic for milestones that require presented
  frames, native input, text shaping, or GPU upload observations.
- The Linux/X11 XTest driver and exact target checks are implemented. They are
  not current native-input evidence until this PDFium candidate completes a
  live GPU run.
- Native crop, page-rotation screenshot, and live accessibility evidence are
  blocked because this VPS has no existing X11 or Wayland `DISPLAY`. No
  synthetic display was created. The SIGUSR1 adapter stores one pending bit, so
  multiple standard signals can coalesce; duplicate-signal rejection is not
  claimed at the process boundary. Non-Linux worker liveness probing also
  remains incomplete.
- Application-close modal pointer/hit-testing and live focus/accessibility
  evidence are blocked by the same missing graphical session. Deterministic
  dialog state and draw calls do not substitute for native input or a live
  accessibility tree.
- Two-cycle persistence can retain both PDFs and three fixed Poppler crops in
  an explicit non-overwritten evidence directory. This is independent writer
  evidence, not a GPUI/PDFium window crop or matched Electron/GPUI visual proof.

Not run:

- Current PDFium-based macOS or Windows build, visual, interaction,
  accessibility, and packaging checks.
- The optimized release after this source slice, an exact packaged candidate,
  and physical-device page-rotation/capture evidence.
- Any packaged or physical-device application-close evidence.
- Linux ARM64, Wayland, and multi-driver breadth checks for this exact slice.
- Production signing, notarization, installer, update, and N-1 checks.
- Hibbeler 400%/1600% and matched Electron/GPUI performance trials.
- Current paid-Linux-GPU calibration and final randomized comparison, native
  presentation timing, per-process GPU allocation, operating-system input
  replay, and matched visual crop oracles.

Historical failed evidence from the superseded Poppler/full-page-raster
candidate remains relevant to test design: its copied Poppler prefix depended
on 23 Homebrew dynamic libraries, Linux GPUI startup had timeouts, Electron did
not settle at 200% in the zoom run, and GPUI's 4096-pixel full-page cap failed
the 1200%/1600% density floor. The PDFium worker and bounded-tile implementation
remove those specific architectures, but only a new native run can show whether
their observed failures are fixed.

## Evidence boundaries

- `captures/current-*.png` names refer to actual prior current-runtime captures,
  but those files were not transferred to this VPS. They are blocked evidence
  in this checkout.
- `captures/gpui-component-linux-gpu-*.png` and
  `performance/results/gpui-component-linux-gpu-events.jsonl` are historical,
  ignored evidence from the rejected gpui-component X11 lane. They do not prove
  the owned component layer and will not transfer with a branch checkout.
- `captures/gpu-owned-2026-08-21/` and
  `performance/results/gpu-owned-2026-08-21/` are current ignored evidence from
  the owned component layer's disposable RTX 4000 Ada X11 lane. They prove only
  the source archive, environment, states, and public-fixture scenarios recorded
  above. They do not transfer with a branch checkout.
- `performance/results/gpu-compare-20260823/` is the ignored RTX 4000 Ada
  Electron-versus-GPUI evidence. It contains the raw 36-launch manifest,
  corrected paired summary, environment provenance, file-hash manifest, and
  exact source snapshot. The local bundle is
  `performance/results/butter-paper-gpu-evidence-20260823.tgz` with SHA-256
  `2f888694e26adc8a5989a017bb96c1a9278b5ff8ce20e72a20baf2e12eea85fa`.
  These ignored artifacts do not transfer with a branch checkout.
- HTML proposals are review mockups. They are not evidence that GPUI can provide the pictured behavior. The files named `gpui-shell-*` and `comparison-*electron-vs-gpui*` come from the compiled native gallery.
- `gpui-gallery/` is the current standalone source spike. It pins GPUI-CE
  commit `c738623ffbcec2aeddc44a645cc6b74646d5cf97` and does not depend on
  gpui-component. The unwired historical `component_milestone.rs` source is
  rejected evidence, not a build target.
- `gpui-next-prototype/` is the rejected styled-default trial. Its component and
  Zed revisions remain fixed in `Cargo.lock`; its page surface is representative
  only and it has no PDF or annotation implementation.
- Historical macOS evidence says the direct-GPUI gallery can open a PDF, display and scroll a real page surface,
  hold multiple document tabs, virtualize all page thumbnails, and navigate
  through thumbnails or Page Up/Page Down. The asynchronous PDF path and stale
  request tests passed with the 935-page Hibbeler corpus. Three-iteration native
  open, page-navigation, and zoom runs now pass in an approved GUI-capable
  execution context. Some root-shell launches still hit a macOS `HIServices`
  application-registration denial; those failed attempts remain documented in
  `performance/results/gpui-native-launch-blocker.md`. Those claims were not
  rerun on this VPS because the corpus and macOS session were not transferred. Raw reports are local
  output and are intentionally not part of the portable branch.
- Current 2026-08-22 macOS evidence is limited to native ARM64 build/tests,
  strict signature checks before and after launch, a live read-only Computer
  Use inspection, and the public six-page fixture. The fixture hash is
  `b9ea482bb4ee07f9124729141258723a6ff643866c8626e725b3416d45ea9fe9`.
  No current screenshot artifact or Hibbeler corpus was transferred.
- The gallery is not linked to production packages and does not import production source.
- The runtime copy, dependency tree, browser data, Rust target files, and new
  captures produced during later runs are disposable. This checkout has no
  selected capture baseline; a later macOS pass must recreate it.

## Reproduce the performance comparison

`performance/protocol.md` defines the versioned matched protocol, public and
private corpus lanes, semantic scenarios, quality milestones, absolute budgets,
paired non-inferiority margins, native evidence, and paid-GPU lease limits. The
current standalone runners retain raw events and process samples for the open,
page-navigation, and zoom subset. Run the paired orchestrator against a
verified public fixture:

```sh
node performance/run-paired.mjs \
  --fixture "nasa-apollo-summary-526-v1=/absolute/path/to/nasa.pdf" \
  --fixture "usgs-usa-geology-sheet-v1=/absolute/path/to/usgs.pdf" \
  --fixture "bp-multi-page-v1=/absolute/path/to/bp-multi-page-v1.pdf" \
  --fixture "bp-annotation-density-v1=/absolute/path/to/bp-annotation-density-v1.pdf" \
  --output performance/results/paired \
  --warmups 1 \
  --pairs 5
```

The orchestrator runs all seven default scenarios implemented by both runners and
alternates implementation order. Each scenario uses its locked fixture and the
orchestrator rejects any fixture whose SHA-256 does not match before launch.
GPUI-only viewer scenarios and the unmatched editor and persistence workloads
remain blocked as paired evidence.
Both runners use a fresh disposable application cache for each launch. Summarize
the retained reports with `performance/summarize-paired.mjs`; see
`performance/README.md` for the evidence boundaries. Five alternating pairs
provide development direction only. Beta
and stable decisions require at least 20 successful randomized alternating
pairs, absolute-budget evaluation, paired bootstrap confidence intervals, and
native presentation evidence. A successful GPUI run needs functioning GUI
services and approved native app access. A launch abort before `window-created`
remains a failed iteration and cannot supply visible-frame evidence. The ignored
2026-08-23 Linux GPU reports satisfy only the development subset and are
incomplete because navigation has four valid pairs and zoom has none. The older
generated `performance/comparison.html` remains historical evidence and does
not satisfy `bp-perf-v2`.

## Reproduce the current-runtime capture

1. Set `HIBBELER_PDF` to the local Hibbeler corpus file.
2. From the repository root, restore dependencies with `CI=true npx --yes pnpm@10.33.0 install --frozen-lockfile`.
3. Build the linked packages with `CI=true npx --yes pnpm@10.33.0 --filter @butter-paper/core build` and `CI=true npx --yes pnpm@10.33.0 --filter @butter-paper/pdf build`.
4. Launch the isolated development runtime:

   ```sh
   BP_TEST_MODE=1 \
   BP_TEST_USER_DATA_DIR="$(mktemp -d)" \
   BP_DEFAULT_SAMPLE_PDF="$HIBBELER_PDF" \
   BP_DISABLE_UPDATE_CHECKS=1 \
   npx --yes pnpm@10.33.0 --filter @butter-paper/desktop dev
   ```

5. Capture the empty shell, fit-width workspace, 400% zoom, properties panel, selected annotation, thumbnail navigation, and multiple-tab states.
6. Replace only the corresponding files in `captures/` and record the new commit in `index.html`.

Matched current-Electron, transient-menu, and constrained-window capture files
remain blocked in this pass. The superseded Poppler-based GPUI shell was
inspected live on the Mac, but that inspection did not create portable
pixel-comparison evidence and does not qualify the PDFium candidate.

## 2026-08-26 Rectangle cutover evidence

A real GPUI Component Rectangle tool and Select tool now drive rendered pointer
creation and movement on the public 100-page PDF. The selected annotation is
edited through the real line-width Popover, saved through staging and validated
reopen, cleanly closed, and rehydrated in a distinct `DocumentWorkspace` with
stable identity and no transient selection or history. PDF geometry uses an
explicit 0.00001 pt edge-reconstruction equivalence because PDF `/Rect` stores
two edges and native f32 projection cannot round-trip bitwise; all other
persisted state remains exact.

Independent PDFium annotation-enabled pixels differ from the annotation-free
application base, and `qpdf`, `pdfinfo`, worker exit, and mapped-surface cleanup
pass. The focused real gate is 1/1, the GPUI-free geometry gate is 1/1, and the
warm all-targets gate is 133 passed plus five gated ignores. Source/guard policy
is 17/17, the prepared Longbridge digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
and the shared annotation-model receipt is
`74bb4230896c485a44939d8925aa7839aa550a139327602239d75ea53153fa7f`.

This is Linux development evidence, not native visual, accessibility, package,
or release evidence. The next cutover-critical journey is real worker/render
failure recovery while preserving dirty annotation state. The complete
Rectangle property inspector follows before that tool can be called fully
migrated.

## 2026-08-26 worker-crash recovery evidence

The native `DocumentWorkspace` now recovers a real crashed PDFium worker
without replacing application-owned document state. A page-render failure keeps
the session ready and preserves the last good raster, current page, dirty
Rectangle, selection/history, zoom, view mode, and scroll intent. A real GPUI
Component `Alert` and `Button` expose the retry. Recovery verifies the original
source SHA-256 and page geometry, rejects stale replacements, swaps only the
resource-backed presentation, and releases crashed, rejected, replaced, and
closed resources.

Passed development-only evidence: the deterministic recovery gate passes 1/1
in 3 seconds; the checksum-pinned real worker test passes 1/1 in 9 seconds after
killing the exact owned PID; and the warm all-targets gate passes 134 tests with
six separately gated ignores in 36 seconds. Source and build-guard policy passes
17/17. The prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
the 870-package graph contains one Zed GPUI identity, and dependency policy
passes with the existing reviewed warnings. Scoped Rust formatting passes.

Failed evidence: retained red compile, hit-target, and async-completion
assertions led to the implementation and no required recovery test remains
failing. The repository-wide Cargo formatting command still reports differences
inside the immutable prepared upstream tree; that tree was not modified.

Blocked: native screenshot and live accessibility on this headless VPS,
production PDFium redistribution, and Windows inherited source-handle transfer.
Not run: packaged Linux/macOS/Windows candidates, physical-device input, IME,
Hibbeler, and the matched Electron/GPUI performance decision. The next
cutover-critical journey is the complete Rectangle property inspector through
the existing edit, transactional Save As, close, and independent-reopen seam.

## 2026-08-26 Rectangle property-inspector evidence

The complete maintained Rectangle property journey now passes as Linux
development evidence. A 300 px, independently scrollable retained inspector
uses real GPUI Component form primitives while `NativeDocumentSession` remains
the sole feature-state and history authority. Stable document and annotation
identities bind every patch. Stroke/fill colors, overall/fill opacity, width,
Solid/Dashed/Dotted style, X/Y/width/height, rotation, and lock survive
transactional Save As, real PDFium rendering, clean close, and a distinct
workspace reopen. The journey also proves `qpdf`, `pdfinfo`, worker exit, and
mapped-surface cleanup.

Passed: focused rendered 1/1, real PDF 1/1, typed model 1/1, PDF persistence,
annotation adapter, source/guard 17/17, one GPUI identity across 870 packages,
configured dependency policy, scoped formatting, and warm all-targets 137 plus
six gated ignores. The exact prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`.
Retained red tests found the viewport-composition and test-stack defects; no
accepted gate remains failing.

Hatch is not implemented because the current Electron fields are no-ops.
Cloud is not treated as a working Rectangle style because Electron renders and
exports it as Solid. Blocked: headless native visual/live accessibility,
production PDFium redistribution, Windows source-handle transfer, and physical
proof for the pinned `ColorPicker` disabled-state API gap. Not run: packages,
macOS/Windows physical-device input, IME, Hibbeler, and matched performance.
The migration now returns to cutover-level journey and release-risk ranking;
micro-control parity is not used as a completion proxy.

## 2026-08-26 in-place Save evidence

The native workspace now saves an opened regular PDF back to its source through
an explicit, guarded replacement transaction. It rejects symlinks and source or
directory identity drift, preserves the live dirty document on every
pre-publication failure, suppresses overlapping saves and annotation mutation,
validates the stage through real PDFium before atomic publication, preserves
Unix mode, and distinguishes durable completion from an already-published
directory-sync warning.

Passed Linux development evidence: focused 7/7, real checksum-pinned PDFium
1/1, warm all-targets 144 plus seven gated ignores, source/guard 17/17, exact
prepared digest, one GPUI identity across 870 packages, configured dependency
policy, and 2.5 GiB retained target with about 103 GiB host space free. Failed:
only retained red and obsolete-test diagnostics; no accepted gate is failing.
Blocked or not run: Windows replacement/source handles, production PDFium,
native visual/accessibility, packages, physical devices, IME, Hibbeler, and
matched performance. Application-close integration is the next cutover seam.

## 2026-08-26 dirty close and application-close evidence

The application-owned close transaction now composes with real in-place Save.
Ordinary dirty PDFs save to their verified sources on a background executor;
generated documents pause at the Save As authority boundary. The frozen
stable-ID transaction survives duplicate requests, runs saves and releases
serially, distinguishes pre-publication failure from post-publication
durability warning, and never emits quit before every live session reports
worker and surface release.

Passed Linux development evidence: pure 10/10, integration 13/13 plus one gated
ignore, real rendered-modal/PDFium 1/1, warm all-targets 150 plus seven gated
ignores, source/guard 17/17, exact prepared digest, one GPUI identity across 870
packages, dependency policy, and host-storage bounds. Failed evidence consists
only of retained red iterations; no accepted gate remains failing. Blocked:
production PDFium redistribution, Windows source replacement/handle ownership,
and native visual/accessibility on the headless VPS. Not run: packages,
physical macOS/Windows/Linux input, IME, Hibbeler, and matched performance. The
next dependency is native Save As prompt routing for a generated document in an
active close transaction.

## 2026-08-26 generated Save As application-close evidence

The application-close owner now drives GPUI's pinned native path request for
each generated document. Only one picker token can be active. Cancel, platform
error, a non-PDF selection, a stale prior picker result, and document mutation
while the picker is open all stop the frozen transaction without saving or
quitting. Successful selection resumes the same serial background-save and
resource-release pipeline; application state remains outside GPUI Component.

Passed Linux development evidence: pure coordinator 11/11
(`button-probe-20260826T035650Z-2075885.log`), integration 17/17 plus one gated
ignore (`button-probe-20260826T035444Z-2074339.log`), and checksum-pinned real
mixed-document PDFium 1/1
(`button-probe-20260826T035633Z-2075516.log`). The real journey saves two
ordinary fixtures in place, selects a target for a generated `Untitled.pdf`,
validates and independently reopens all three PDFs, removes the generated
temporary source, reaps all workers, clears mapped surfaces, closes the modal,
and emits one quit intent. The warm all-targets gate passes 155 tests plus seven
gated ignores in 34 seconds
(`button-probe-20260826T035700Z-2076199.log`). Guard/source policy is 17/17;
the prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
the 870-package graph has one pinned GPUI identity, and the configured
dependency policy has no denied source, license, or vulnerability.

Failed evidence is retained red/compile output for the new race tracers and
runner-name change; no accepted gate is failing. Blocked: the pinned GPUI API
does not expose picker title, PDF filter, default extension, or window ownership;
production PDFium redistribution, Windows replacement/source-handle behavior,
and live native visual/accessibility evidence also remain blocked. Not run:
packaged candidates, physical-device input, Input Method Editor (IME), Hibbeler,
and matched performance. The next cutover slice is capability-bound save-target
authority plus visible failure recovery; micro-interaction parity remains lower
priority.

## 2026-08-26 save-target authority and recovery evidence

The generated Save As path is now capability-bound on Unix. A move-only
authority retains the chosen absolute native path, canonical parent identity,
directory descriptor, destination leaf, and one-shot state. PDF publication
uses an exclusive same-directory stage, rejects parent/target/stage identity
drift, never overwrites a destination, and removes a failed stage only when its
name still refers to the authority-owned inode. This closes the known
path-reopen and pathname-cleanup races without changing the pinned dependency
graph or production Electron sources.

Application close now renders typed GPUI Component recovery alerts for picker,
target, save, published-warning, and resource-release failures. A recovery
keeps the affected stable document and the frozen transaction context visible;
retry cannot rewrite a document that already crossed publication.

Passed Linux development evidence: target authority 7/7, application-close
state 11/11, integration 19/19 plus one gated ignore, checksum-pinned real
mixed-document PDFium 1/1, and warm all-targets 163 plus seven gated ignores.
Source/guard policy passes 17/17; the prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the 870-package graph contains one pinned GPUI identity; and dependency policy
has no denial. Failed evidence consists only of retained red tests fixed before
acceptance. Blocked: Windows target authority, production PDFium distribution,
and live native visual/accessibility on this host. Not run: packaged platforms,
physical input, IME, Hibbeler, and matched performance. Further work remains
organized around complete cutover journeys rather than component counts.

## 2026-08-26 native document-ingress update

The runnable GPUI Component candidate now routes normal startup and the real
native picker through one application-owned `DocumentWorkspace` batch-open
seam. It accepts native paths without UTF-8 rewriting, ignores unrelated launch
arguments, resolves relative launch PDFs against the working directory,
deduplicates ordinary opens, preserves the selected page when focusing an
existing session, and keeps drop-origin duplicate sessions independent.
Sequential partial failures preserve successful documents and resources,
remove failed transient tabs, retain all failure records in order, and render a
real non-modal GPUI Component Alert with a labeled Dismiss Button.

Passed Linux development evidence: focused ingress 5/5, launch parser 4/4,
warm all-targets 174 plus seven gated ignores, exact story/worker build,
source/guard 17/17, prepared digest
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
one pinned GPUI identity across 870 packages, and dependency policy with no
denial. Failed evidence is retained red-first output only. Blocked or not run:
native menu, drag/drop, macOS open-file, second-instance delivery, live visual
and accessibility evidence, production PDFium, Windows save authority,
packaged candidates, physical input/IME, Hibbeler, and matched performance.

## 2026-08-26 native application boundary update

The exact runnable GPUI Component candidate now connects startup paths, macOS
local-file URL delivery, and real cross-platform file drops to the retained
multi-document coordinator. A single menu model uses GPUI's operating-system
menu on macOS and the real GPUI Component `AppMenuBar` on Linux/Windows. Open,
Save, and Save As target application-owned document state; Close Window and
Quit enter the dirty-document application-close transaction and have no raw
quit bypass.

Passed Linux development evidence: native-application 6/6, exact story/worker
build, warm all-targets 180 plus seven gated ignores, source/guard 17/17,
prepared digest
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
one GPUI identity across 870 packages, and configured dependency policy with no
denial. Failed evidence is retained red-first output only. Blocked or not run:
physical macOS menu/Open With proof, Linux/Windows single-instance IPC, package
file associations, live visual/accessibility evidence, production PDFium,
Windows save authority, packaged candidates, physical input/IME, Hibbeler, and
matched performance.

## 2026-08-26 native-shell Rectangle transaction update

The exact GPUI Component application shell now has one real cutover tracer that
crosses the previously separate UI and persistence proofs. It opens the public
100-page fixture in a native window, creates a retained Rectangle, enters the
real dirty-application-close surface, accepts its default Save action, validates
the updated PDF, releases the original PDF worker and mapped surfaces, opens a
fresh GPUI window, hydrates the same stable Rectangle, and releases the fresh
worker on document close.

Passed Linux development evidence: the checksum-pinned real PDFium journey is
1/1 in seven seconds
(`button-probe-20260826T060214Z-2164273.log`), the fast build-policy gate is
13/13, and the warm all-targets gate is 180 active tests plus eight explicit
real-fixture ignores in 15 seconds
(`button-probe-20260826T060239Z-2164640.log`). Both storage guards remained
green and retained the approximately 4.0 GiB owned target. Failed evidence is
limited to retained red iterations for the private snapshot call and reuse of
a deliberately closed test window; both are fixed and no accepted gate fails.
Blocked: production PDFium redistribution, Windows save-target authority, and
live native visual/accessibility evidence on this headless VPS. Not run:
packaged candidates, physical macOS/Windows input, IME, Hibbeler, and the final
matched Electron/GPUI performance qualification.

## 2026-08-26 ordinary Save As collision recovery update

The native workspace now exposes ordinary Save and Save As failures through
application-owned typed state instead of the annotation-status string channel.
The visible recovery surface uses a real GPUI Component `Alert` and real
`Button` actions. An in-place failure offers Retry, Save As, and Dismiss. A
Save As authority failure omits unsafe same-target retry and offers a fresh
native target choice plus Dismiss. The dirty-close transaction remains a
separate owner and keeps its previously proved behavior.

The checksum-pinned real journey opens the public 100-page fixture, creates a
Rectangle, selects an occupied Save As destination, proves that destination is
unchanged, and preserves the source path, dirty annotation state, page state,
and live PDF worker. It then selects a fresh destination through the rendered
recovery action, validates the PDF with `qpdf`, reopens the stable Rectangle,
and proves both worker and mapped-surface release.

Passed Linux development evidence: focused real journey 1/1 in 25 seconds
(`button-probe-20260826T062254Z-2177145.log`), focused in-place recovery in four
seconds (`button-probe-20260826T062331Z-2177744.log`), and warm all-targets 181
active plus nine explicit real-fixture ignores in 34 seconds
(`button-probe-20260826T062346Z-2178041.log`). Source/guard policy passes 17/17;
the prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the 870-package graph has one pinned GPUI identity; and dependency policy has
no denial. Failed evidence is the retained red journey that exposed the
missing typed collision state; the accepted gate is green. Blocked: production
PDFium redistribution, Windows save-target authority, and live native
visual/accessibility evidence. Not run: packaged candidates, physical
macOS/Windows input, IME, Hibbeler, and final matched performance.

## 2026-08-26 controlled real-document viewer update

The exact runnable GPUI Component candidate now renders its fit, page-mode,
wheel, and zoom controls inside the real multi-document `DocumentWorkspace`.
The active `NativeDocumentSession` owns all view state. Real GPUI Component
controls dispatch intent and receive silent session snapshots through stored
observers; no toolbar state is synchronized from `render`. The older
story-owned duplicate toolbar was removed. CAD remains explicitly isolated
because it has not yet been added to per-document view state.

Passed Linux development evidence: a checksum-pinned two-document real-PDFium
journey drives rendered tabs, thumbnails, Single Page/Continuous, Fit Page/Fit
Width, 400%, 1600%, and independent scroll state; renders bounded real tiles;
rejects a stale generation; and releases both workers and mapped surfaces. The
focused gate passes 1/1 in 24 seconds, warm all-targets passes 181 active plus
ten gated ignores in 34 seconds, source/guard passes 17/17, the prepared digest
is unchanged, the 870-package graph has one GPUI identity, and dependency
policy has no denial. Failed evidence is retained red-first diagnostic output
only. Blocked or not run: production PDFium, Windows target authority, native
visual/accessibility, packages, physical input/IME, Hibbeler, sustained cache
pressure, and matched Electron/GPUI performance.

## 2026-08-26 real-session tab consolidation update

The runnable native workspace now applies the proven selection, close,
keyboard reorder, and pointer reorder contract to real stable-ID document
sessions. It uses pinned GPUI Component `TabBar`, `Tab`, `Button`, and
`Popover` controls. Butter Paper owns only session/domain state and small
pointer/accessibility adapters that the component library does not provide.

Passed Linux development evidence: the rendered focused journey passes 2/2 in
31 seconds and warm all-targets passes 183 active tests plus ten gated ignores
in 47 seconds. Loading and failed tabs remain visible. Close-origin movement
cannot reorder. The exact six-pixel boundary, seven-pixel drag, keyboard
reorder, dirty Cancel/Discard, successor focus, stable annotation identity,
and resource release pass. Source/guard is 17/17, the prepared digest and
870-package single-GPUI graph are unchanged, dependency policy has no denial,
and host storage is green. No accepted gate fails. Blocked or not run: edge
auto-scroll, order persistence, production PDFium, Windows target authority,
native visuals/accessibility, packages, physical input/IME, Hibbeler, and
matched performance.

## 2026-08-26 Polyline and Polygon cutover evidence

The exact GPUI Component application shell now carries Polyline and Polygon
through a complete Linux development journey. Real GPUI Component Buttons
select each tool. Application-owned retained state handles pointer vertex
creation, valid Enter or Escape completion, Polygon start-point closure,
selection, vertex editing, body movement, locking, history, and page-major
identity. Raw GPUI remains limited to document pixels and annotation geometry;
it does not replace any standard control.

The persistence seam writes canonical `/PolyLine` and `/Polygon` annotations,
`/Vertices`, appearance streams, stable names, compatible metadata, and bounds
derived from stroke width. It excludes specialized external path annotations
that the maintained model cannot preserve. The real fixture journey proves
changed PDFium annotation pixels, `qpdf` validation, clean close, a distinct
workspace reopen, stable edited points and appearance, worker exit, and mapped
surface cleanup.

Passed Linux development evidence: focused rendered workspace 1/1
(`button-probe-20260826T083207Z-2275902.log`), adapter 32/32
(`button-probe-20260826T083251Z-2276897.log`), checksum-pinned real PDFium 1/1
(`button-probe-20260826T084142Z-2284003.log`), and guarded warm all-targets 184
active plus eleven explicit gated ignores in 64 seconds
(`button-probe-20260826T083819Z-2281603.log`). Source/guard passes 17/17; the
prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; configured
dependency policy has no denied source, license, advisory, or vulnerability;
and host storage is green. Its two missing-license-metadata and five
unmaintained-transitive findings remain warnings.

Failed evidence is retained red-first output for pointer targeting, focus,
PDF-number equality, dirty-state reconciliation, and one over-broad metadata
preservation attempt; each is fixed and no accepted gate fails. Blocked:
production PDFium redistribution, Windows save authority, and fresh native
visual/live-accessibility evidence. Not run: packaged candidates, physical
macOS/Windows input, IME, Hibbeler, and matched performance.

## 2026-08-26 Polylength and Area cutover evidence

The exact GPUI Component application shell now carries calibrated Polylength
and Area annotations through a complete Linux development journey. Real pinned
GPUI Component `Button`s select the tools. Application-owned retained state
owns page scale, multi-click drafts, stable measurement identity, captions,
selection, vertex edits, history, save reconciliation, and worker ownership.
Raw GPUI remains confined to PDF and annotation pixels, hit testing, path
geometry, and caption paint.

The persistence boundary distinguishes measurements from ordinary Polyline and
Polygon using canonical subtype, intent, subject, `/Vertices`, `/Measure`,
stable `/NM`, and appearance streams. It imports current intent dictionaries
and legacy subject-only paths with page-scale fallback while leaving ordinary
paths, PolygonCloud, and direct legacy dictionaries in their correct families.
The real 100-page fixture journey proves double-click and Enter completion,
independent Polylength/Area state, a pointer vertex edit, guarded Save As,
typed independent reopen, changed PDFium annotation pixels, `qpdf`, clean close,
a distinct-workspace reopen, worker exit, and mapped-surface cleanup.

Passed Linux development evidence: real PDFium 1/1
(`button-probe-20260826T094541Z-2318142.log`), model 1/1
(`button-probe-20260826T094623Z-2318657.log`), adapter 34/34
(`button-probe-20260826T094636Z-2319016.log`), persistence 18/18
(`button-probe-20260826T094654Z-2319446.log`), page-scale interaction 5/5
(`button-probe-20260826T094926Z-2322117.log`), and guarded warm all-targets
185 active plus twelve gated ignores in 49 seconds
(`button-probe-20260826T095332Z-2325829.log`). Source/guard passes 17/17; the
prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; configured
dependency policy has no denial; and host storage is green.

Failed evidence is retained red-first diagnostic history. The expanded toolbar
made two old tests click the off-viewport Page Scale bound; the minimized red
loop proved the sole scroll owner and the corrected user-visible scroll
precondition. No accepted gate fails. A known import hardening gap remains:
duplicate normalized IDs are rejected within each managed family but not yet
across different annotation families. Blocked: production PDFium
redistribution, Windows save authority, and fresh native visual/live
accessibility evidence. Not run: packaged candidates, physical macOS/Windows
input, IME, Hibbeler, and matched performance. This evidence is development
only and does not authorize cutover or release.

## 2026-08-26 Cloud cutover evidence

The standalone Cloud tool is now a real Linux development journey in the exact
GPUI Component application shell. A real pinned GPUI Component `Button` owns
the standard visible `tool-cloud` control. Application-owned retained state
owns click-node drafts, stable Cloud identity, scallop geometry, selection,
vertex/body edits, lock/history, native PDF reconciliation, dirty state, and
worker resources. Raw GPUI is restricted to PDF and annotation pixels, hit
testing, pointer geometry, and control-handle paint.

Cloud is classified before ordinary Polygon and persists as `/Polygon` with
`/IT /PolygonCloud`, `/BE << /S /C /I 2 >>`, `/Subj (Cloud)`, stable `/NM`,
`/Vertices`, and `/AP /N`. The checksum-pinned real 100-page fixture journey
proves click-node creation, a stable vertex edit, guarded Save As, independent
typed reopen, changed real PDFium annotation pixels, `qpdf`, clean close,
distinct-workspace hydration, worker exit, and mapped-surface cleanup.

Passed Linux development evidence: model 1/1
(`button-probe-20260826T100534Z-2332753.log`), persistence 18/18
(`button-probe-20260826T101050Z-2335767.log`), adapter 35/35
(`button-probe-20260826T102950Z-2346504.log`), rendered workspace 1/1
(`button-probe-20260826T103023Z-2347054.log`), real PDFium 1/1
(`button-probe-20260826T103109Z-2347805.log`), and guarded warm all-targets
186 active plus thirteen gated ignores in 41 seconds
(`button-probe-20260826T103128Z-2348162.log`). Source/guard passes 17/17; the
prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; configured
dependency policy has no denial; and host storage is green.

Failed evidence is retained red-first diagnostic history. A focused adapter
test found that the shared lock command omitted the new Cloud family; the
fixed path now passes the focused and broad gates. No accepted gate fails.
Partial gaps remain: Electron's rectangle-drag Cloud creation, live workspace
pointer vertex/body edit, exact Electron/Bluebeam cubic scallop output, and
intensity/style properties. Blocked: production PDFium redistribution,
Windows save authority, and fresh native visual/live-accessibility evidence.
Not run: packaged candidates, physical macOS/Windows input, IME, Hibbeler, a
third-party Cloud corpus, and matched performance. This evidence is development
only and does not authorize cutover or release.

## 2026-08-26 Callout cutover evidence

The exact GPUI Component application shell now carries a composed Callout
through a complete Linux development journey. Real pinned GPUI Component
`Button` and `Textarea` primitives own the standard visible tool and text-input
surfaces. Application-owned retained state owns the two-click leader/text-box
draft, stable identity, multiline content, one-step initial transaction,
selection, leader and text-box edits, history, native PDF reconciliation,
dirty state, and worker resources. Raw GPUI remains limited to PDF and
annotation pixels, hit testing, pointer geometry, and selection handles.

The checksum-pinned real 100-page fixture journey proves rendered creation,
multiline text, independent text-box and knee edits, guarded Save As, typed
independent reopen, one canonical native Callout rather than Cloud/Text Box
misclassification, changed real PDFium pixels, `qpdf`, clean close,
distinct-workspace hydration, worker exit, and mapped-surface cleanup.

Passed Linux development evidence: rendered workspace 1/1
(`button-probe-20260826T111401Z-2370982.log`), persistence focused green
(`button-probe-20260826T111615Z-2372567.log`), real PDFium 1/1
(`button-probe-20260826T111642Z-2373049.log`), focused Length regression 1/1
(`button-probe-20260826T112819Z-2380348.log`), and guarded warm all-targets
187 active plus fourteen gated ignores in 33 seconds
(`button-probe-20260826T112905Z-2380861.log`). Source/guard passes 17/17; the
prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; configured
dependency policy has no denial; and host storage is green.

Failed evidence is retained red-first diagnostic history. The PDFium red test
caught an appearance stream using page coordinates inside a local Form
`/BBox`; the fixed stream now renders. The broad gate then caught an old Length
test clicking a newly off-screen tool bound on top of Rotate Left; the focused
test now scrolls the real target into the sole toolbar owner and proves no
overlap. No accepted gate fails. Partial gaps remain: existing-Callout
double-click editing, complete live pointer handles/body drag, exact font
wrapping and rich-text policy, style properties, and Cloud+. Blocked:
production PDFium redistribution, Windows save authority, and fresh native
visual/live-accessibility evidence. Not run: packaged candidates, physical
macOS/Windows input, IME, Hibbeler, third-party Callout corpus, and matched
performance. This evidence is development only and does not authorize cutover
or release.

## 2026-08-26 Cloud+ cutover evidence

The pinned GPUI Component application shell now carries one logical Cloud+
through rendered creation, retained multiline editing, direct stable-identity
geometry edits, native paired PDF persistence, and a fresh-workspace reopen.
Real GPUI Component `Button` and `Textarea` primitives own the standard visible
surfaces. Application-owned state owns the composite and its history. Raw GPUI
is restricted to PDF/annotation pixels, hit testing, pointer geometry, and
selection handles.

The checksum-pinned public 100-page journey proves four-line text growth and
leader rerouting, a cloud-vertex edit, an independent text-box move, guarded
Save As, one canonical PolygonCloud plus FreeTextCallout pair, changed PDFium
annotation pixels, `qpdf`, clean close, stable typed hydration in a distinct
workspace, worker exit, and mapped-surface cleanup. Incomplete or mismatched
pair fragments remain quarantined rather than becoming standalone Cloud,
Callout, or Text Box objects.

Passed Linux development evidence: model 1/1, routing 1/1, focused adapter,
paired persistence 4/4, rendered workspace 1/1, real PDFium 1/1, corrected
Image-toolbar regression 1/1, and guarded warm all-targets 188 active plus
fifteen gated ignores in 22 seconds. Source/guard passes 17/17; the prepared
digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; dependency
policy has no denial; and host storage is green.

Failed evidence is retained red-first diagnostic history for multiline layout,
PDF-number tolerance, and the off-screen Image test. Each failure is fixed and
no accepted gate fails. Partial: existing-object pointer selection and full
body/handle manipulation, page/obstacle routing context, exact scallop output,
properties, and editor accessibility/IME. Blocked: production PDFium
redistribution, Windows save-target authority, and live native visual/
accessibility evidence on this headless VPS. Not run: packages, physical
macOS/Windows input, Hibbeler/third-party Cloud+ corpora, and matched
Electron/GPUI performance. Electron remains the shipping rollback.

## 2026-08-26 Dimension cutover evidence

The pinned GPUI Component application shell now carries an uncalibrated
Dimension through one complete Linux development journey. Real GPUI Component
`Button` and `Textarea` primitives own the standard tool and caption-input
surfaces. Application-owned retained state owns the two-click draft, stable
identity, caption, signed offset, endpoint/body/offset edits, history, dirty
state, PDF reconciliation, and worker resources. Raw GPUI remains limited to
PDF and annotation pixels, hit testing, pointer geometry, arrows, extension
lines, caption paint, and selection handles.

The checksum-pinned public 100-page journey proves rendered creation, retained
caption editing, a 40-point offset edit, guarded Save As, clean accepted
revision, typed independent reopen, one canonical unmeasured native
`/LineDimension`, changed PDFium annotation pixels, fresh-workspace hydration,
worker replacement and exit, and mapped-surface cleanup. A measured
`/LineDimension` remains Length, so the two product families do not absorb one
another.

Passed Linux development evidence: focused model/adapter and workspace gates,
focused persistence 2/2, real PDFium 1/1
(`button-probe-20260826T132649Z-2458213.log`), and guarded warm all-targets 189
active plus sixteen explicit gated ignores in 27 seconds
(`button-probe-20260826T133014Z-2460821.log`). Source/guard policy passes 17/17;
the prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; configured
dependency policy has no denial; and host storage is green.

Failed evidence is retained red-first diagnostic history. The final broad gate
also exposed two old Text Box tests clicking a newly off-screen target after
the Dimension tool expanded the sole horizontal toolbar. The corrected tests
scroll the real target into view and now pass. No accepted gate fails. Partial
gaps remain: complete live pointer endpoint/body/offset manipulation, property
controls, dimension-increment snapping, exact visual acceptance, and imported
corpus breadth. Blocked: production PDFium redistribution, Windows save-target
authority, and live native visual/accessibility evidence on this headless VPS.
Not run: packaged candidates, physical macOS/Windows input, IME, Hibbeler,
third-party Dimension corpora, and matched Electron/GPUI performance. Electron
remains the shipping rollback.

## 2026-08-26 Arc cutover evidence

The pinned GPUI Component application shell now carries Arc through one
complete Linux development journey. A real GPUI Component `Button` owns the
standard visible `tool-arc` surface. Application-owned retained state owns the
three-click draft, stable identity, sampled path, start/mid/end controls,
translation, lock/history state, dirty state, PDF reconciliation, and worker
resources. Raw GPUI remains limited to PDF/annotation pixels, curved hit
testing, pointer geometry, sampled-path paint, and selection handles.

The checksum-pinned public 100-page journey proves rendered three-click
creation, an independent retained midpoint edit, guarded Save As, clean
accepted revision, typed independent reopen, one canonical native `/Circle`
with `/IT /CircleArc`, changed PDFium annotation pixels, fresh-workspace
hydration, worker exit, and mapped-surface cleanup. The persistence gate also
proves create/edit/delete and independent `qpdf` validation.

Passed Linux development evidence: model 1/1
(`button-probe-20260826T142230Z-2486828.log`), adapter focused green
(`button-probe-20260826T142245Z-2487137.log`), persistence focused green
(`button-probe-20260826T142300Z-2487609.log`), rendered workspace 1/1
(`button-probe-20260826T142457Z-2489563.log`), real PDFium 1/1
(`button-probe-20260826T142615Z-2490982.log`), and guarded warm all-targets 190
active plus seventeen explicit gated ignores in 33 seconds
(`button-probe-20260826T142758Z-2492765.log`). Source/guard policy passes
17/17; the prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; configured
dependency policy has no denial; and host storage is green.

Failed evidence is retained red-first diagnostic history. Compiler passes
caught one duplicate helper and one missed exhaustive family label. Runtime
reds exposed only PDF-number tolerance in two assertions. The broad gate then
caught an old Pen test clicking a newly off-screen toolbar target; the fixed
test scrolls the real target into view. No accepted gate fails. Partial gaps
remain: full live pointer proof for body/start/end/mid edits, property controls,
ambiguous non-square imported CircleArc policy, native visual/accessibility,
and third-party corpus breadth. Blocked: production PDFium redistribution,
Windows save-target authority, and live native visual/accessibility evidence on
this headless VPS. Not run: packaged candidates, physical macOS/Windows input,
IME, Hibbeler, third-party Arc corpora, and matched Electron/GPUI performance.
Electron remains the shipping rollback.

## 2026-08-26 shared Rectangle/Ellipse property-inspector evidence

The native workspace now uses one application-owned retained inspector seam
for Rectangle and Ellipse. The visible surface uses real pinned GPUI Component
`Accordion`, `Field`, `Scrollable`, `Switch`, `ColorPicker`, `Slider`,
`NumberInput`, and `Select` primitives. Rectangle keeps its existing stable
identifiers. Ellipse has its own stable identifiers and copy while sharing the
same control and event implementation. Raw GPUI remains limited to PDF and
annotation pixels, hit testing, pointer geometry, and selection chrome.

The Ellipse journey covers lock, stroke and fill colors, overall and fill
opacity, line width, Solid/Dashed/Dotted line style, X/Y/width/height, and
rotation. Application state owns identity checks, no-op suppression, one-step
history, locked and Save-in-progress suppression, and nonnegative property
dimensions. The two-point creation threshold remains separate. Persistence now
emits a standard `/Circle` annotation with `/C`, `/IC`, `/CA`, `/ca`, `/BS`, a
stable `/NM`, and a normal appearance Form with cubic ellipse geometry, dash,
fill/stroke alpha, and rotation. Replacing or deleting the Ellipse releases
unreferenced owned appearance resources.

Passed Linux development evidence: deterministic Ellipse inspector and
Rectangle regression tests; standard Ellipse persistence and appearance-stream
tests; checksum-pinned real PDFium Save As, worker replacement, close, fresh-
workspace reopen, exact retained state, worker exit, and mapped-surface cleanup
in 37 seconds (`button-probe-20260826T173853Z-2639118.log`); and the final
guarded warm all-targets gate with 195 active tests, 21 explicit gated ignores,
and no failures in 50 seconds
(`button-probe-20260826T173940Z-2640183.log`). Source and build-policy tests pass
17/17. The prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; configured
dependency policy has no denial; and storage remains green with about 101 GiB
free and a retained 2.68 GiB owned target.

Failed evidence is retained red-first diagnostic history only. It exposed the
missing shared event seam, the need to separate property dimensions from the
creation threshold, the old solid-only Ellipse writer, and appearance-resource
cleanup. These are fixed; no accepted gate fails. Partial: only Rectangle and
Ellipse consume the shared inspector; other compatible shape families still
need product-specific contract audits. The pinned `ColorPicker` has no real
disabled API, so a shallow wrapper dims it and the application rejects events;
physical popup suppression is not proved. Blocked: production PDFium
redistribution, Windows save-target authority, and fresh native visual/live
accessibility proof on this headless VPS. Not run: packaged candidates,
physical macOS/Windows input, IME, Hibbeler and third-party Ellipse corpora, and
matched Electron/GPUI performance. This evidence is development-only and does
not authorize cutover or release.

The next cutover-critical slice is non-default PDF coordinate-space
compatibility. It must retain CropBox origin, inherited page rotation, and
`/UserUnit` in one GPUI-free page-coordinate-space module, then prove page,
thumbnail, tile, annotation, hit-test, Save As, and fresh-reopen agreement. This
precedes more annotation-family work because a coordinate error would affect
every migrated tool and persisted document.

## 2026-08-26 pending Redact cutover evidence

The pinned GPUI Component workspace now carries a pending Redact mark through
one truthful Linux development journey. A real GPUI Component `Button` owns the
`tool-redact` control and a real warning `Alert` states that saving the mark
does not remove the underlying PDF content. Application-owned retained state
owns the stable identity, click-or-drag creation, move, eight-handle resize,
lock/history state, dirty state, PDF reconciliation, and worker resources. Raw
GPUI is restricted to page and annotation pixels, hit testing, pointer
geometry, and selection handles.

The checksum-pinned public 100-page journey proves creation, move, resize,
guarded Save As, canonical typed `/Redact` reopen with no appearance stream,
clean close, fresh-workspace hydration, deletion from an experiment-owned
copy, worker exit, and mapped-surface cleanup. Annotation-disabled PDFium
pixels are identical before and after both save operations. This is evidence
that the page content remains present; it is not evidence of secure redaction.

Passed Linux development evidence: model geometry 2/2
(`button-probe-20260826T152004Z-2530685.log`), focused adapter and persistence
gates, rendered workspace 1/1
(`button-probe-20260826T151308Z-2525858.log`), real PDFium 1/1 in 37 seconds
(`button-probe-20260826T152029Z-2531291.log`), and guarded warm all-targets 191
active plus eighteen explicit gated ignores in 74 seconds
(`button-probe-20260826T152132Z-2532416.log`). Source and build-guard policy
passes 17/17; the prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; configured
dependency policy has no denial; and host storage is green with about 102 GiB
free. Its accepted dependency warnings remain two missing-license metadata
warnings and five unmaintained transitive dependencies.

Failed evidence is retained red-first diagnostic history. The real journey
found that persisted PDF numbers quantize rectangle edges through `f32`; the
model now compares the exact persisted edge representation and still rejects a
representable 0.001-point change. No accepted gate fails. Partial gaps remain:
external or noncanonical Redacts are preserved as read-only opaque objects;
relationship and equal-size snapping are deferred; and fresh native visual,
input, and accessibility proof is unavailable on this headless VPS. Blocked:
Apply Redactions, content destruction, sanitization, and flattening require a
fresh product and security decision; production PDFium redistribution and
Windows save-target authority also remain blocked. Not run: packages,
physical macOS/Windows input, IME, Hibbeler, third-party Redact corpora, and
matched performance. Electron remains the shipping rollback.

## 2026-08-26 Snapshot cutover evidence

The pinned GPUI Component workspace now completes one real Snapshot journey.
A real GPUI Component `Button` owns `tool-snapshot`; application-owned state
owns the strict two-click draft, synchronous annotation-free base-raster crop,
stable identity, move/eight-handle resize, appearance values, lock/history,
PDF reconciliation, and resources. Raw GPUI remains limited to document and
annotation pixels, hit testing, pointer geometry, and selection chrome.

The checksum-pinned public 100-page journey proves nonuniform real page-pixel
capture, move and resize, guarded Save As, canonical typed `/StampSnapshot`
with Form/Image/SMask appearance resources, clean close, distinct-workspace
reopen, experiment-copy deletion, unchanged annotation-disabled PDFium pixels,
changed annotation-enabled pixels, worker exit, and mapped-surface cleanup.
External or malformed Stamp annotations are preserved rather than converted.

Passed: model 3/3, adapter 2/2, persistence 1/1, rendered workspace 1/1, real
PDFium 1/1, warm all-targets 192 plus nineteen gated ignores, source/guard
17/17, exact prepared digest, one GPUI identity across 870 packages,
configured dependency policy, and storage bounds. Failed evidence is retained
red-first diagnostic history only; no accepted gate fails.

Partial: live rotation-handle interaction, opacity controls, rotated/cropped
page breadth, capture fallback policy, vendor/private payload compatibility,
hostile appearance graphs, and exact native visual/accessibility. Blocked:
production PDFium redistribution, Windows save authority, and live native
visual/accessibility on this headless VPS. Not run: packages, physical macOS/
Windows input, IME, Hibbeler, third-party Snapshot corpora, and matched
performance. Electron remains the shipping rollback.

## 2026-08-26 semantic-snapping cutover evidence

The GPUI Component workspace now proves one bounded semantic-snapping journey
without claiming complete snapping parity. Real GPUI Component `Button`,
`Popover`, and `Checkbox` primitives own the visible settings surface.
Application-owned state owns the settings, source index, current decision, and
annotation history. Raw GPUI is restricted to document and annotation pixels,
pointer geometry, and the transient canvas guide.

The shared engine matches Electron's inclusive eight-window-pixel Euclidean
tolerance and Shift-first ordering. It indexes endpoint, midpoint, center,
bounded-intersection, and optional nearest candidates for Rectangle, straight
Line/Arrow, Dimension, and Length sources. The checksum-pinned public PDF
journey creates a real Rectangle reference, snaps a Line and calibrated Length,
saves, independently reopens typed state, proves changed PDFium annotation
pixels, closes every worker, and leaves no mapped surface.

Passed Linux development evidence: engine 7/7
(`button-probe-20260826T165107Z-2605452.log`), rendered workspace 2/2
(`button-probe-20260826T165248Z-2607166.log`), adapter 2/2
(`button-probe-20260826T165323Z-2607738.log`), real PDFium 1/1
(`button-probe-20260826T165349Z-2608205.log`), and guarded warm all-targets 194
active plus twenty gated ignores in 40 seconds
(`button-probe-20260826T165507Z-2609253.log`). Source and build-guard policy
passes 17/17 across 18 checksum-bound shared sources. The prepared digest
remains `630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; configured
dependency policy has no denial; and host storage remains green.

Failed evidence is retained red-first diagnostic history. It exposed the
missing constrained engine seam, a bad fixture unit, PDF-number quantization,
and effective Length-calibration provenance normalization. These are fixed;
no accepted gate fails. Partial: other annotation source families, PDF content,
page and construction grids, dimension increments, object tracking, alignment,
equal size/spacing, move/resize/edit snapping, and rotated-page/UserUnit breadth
remain. Blocked: production PDFium redistribution, Windows save authority, and
fresh native visual/accessibility proof on this headless VPS. Not run: packaged
candidates, physical macOS/Windows input, IME, Hibbeler, third-party geometry
corpora, and matched Electron/GPUI performance. `rustfmt` was not run because
the pinned toolchain lacks that component; no toolchain mutation was made.
Electron remains the shipping rollback.

## 2026-08-26 shared Rectangle/Ellipse inspector correction

The native workspace now shares one application-owned property-inspector seam
between Rectangle and Ellipse while keeping feature state outside GPUI
Component internals. Ellipse persistence is canonical `/Circle` with standard
stroke/fill alpha, dash, rotation, and owned appearance-resource cleanup.

Passed: focused shared inspector 4/4 plus one ignored real test
(`button-probe-20260826T175054Z-2646520.log`), gallery persistence 3/3
(`button-probe-20260826T175145Z-2647590.log`), checksum-pinned real PDFium
1/1 (`button-probe-20260826T175223Z-2648310.log`), warm all-targets 198 active
plus 21 gated ignores in 41 seconds
(`button-probe-20260826T175238Z-2648636.log`), source/guard 17/17, exact
prepared digest, one GPUI identity across 870 packages, configured dependency
policy, and green host storage. Failed: retained red-first diagnostics only;
no accepted gate fails. Partial: Rectangle and Ellipse only; ColorPicker
disabled-state suppression is an application wrapper because the pinned
primitive has no disabled API. Blocked: production PDFium redistribution,
Windows save-target authority, and native visual/live accessibility on this
headless host. Not run: packages, physical macOS/Windows input, IME, Hibbeler,
third-party Ellipse corpora, and matched performance. This is development-only
evidence and does not authorize cutover.

The next slice is non-default PDF coordinate-space compatibility. Freeze a
GPUI-free contract for CropBox origin, inherited page rotation, and
`/UserUnit`, then prove page, thumbnail, tile, annotation, hit-test, Save As,
and fresh-reopen agreement before adding another annotation family.

## 2026-08-26 canonical PDF coordinate-space slice

The native document spine now retains one canonical, GPUI-free coordinate space
per opened page. The parser resolves inherited `MediaBox`, `CropBox`, `Rotate`,
and `/UserUnit` values from the PDF page tree. The worker protocol carries
`media_box`, `crop_box`, rotation, display dimensions, and `user_unit`; the
native workspace uses that metadata for full-page, thumbnail, tile, and
annotation-layer transforms. Application annotation coordinates remain raw PDF
points. Legacy mock openers use an explicit zero-origin/unit-one fallback until
they provide the metadata seam.

Passed Linux development evidence: coordinate parser/transform 7/7
(`button-probe-20260826T233731Z-2728570.log`), protocol compatibility 2/2
(`button-probe-20260826T230354Z-2701558.log`), retained workspace metadata 1/1
(`button-probe-20260826T233901Z-2729301.log`), worker build 1/1
(`button-probe-20260826T233517Z-2726975.log`), real checksum-pinned PDFium
100-page journey 1/1 (`button-probe-20260826T234720Z-2733533.log`), and warm
compat all-targets 199 active plus 21 gated ignores in 29 seconds
(`button-probe-20260826T234756Z-2734176.log`). Source-preparation and
build-guard tests pass 17/17; the prepared component digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`; the
graph remains one pinned GPUI identity across 870 packages; configured
dependency policy has no denial; and host storage is green with about 103 GiB
free. The real journey proves non-uniform page and thumbnail pixels,
thumbnail navigation, failed second-open isolation, stale-result safety,
worker PID exit, mapped-surface cleanup, and bounded viewer tile/cache output.

Failed evidence is retained red-first diagnostic history only. One gallery
PDF-worker build hit the four GiB disposable-target cap and was cleaned by the
safety policy; it is not accepted evidence. Partial: the real all-annotation
fixture now covers a non-zero CropBox and `/Rotate 90`, but no real fixture
carries `/UserUnit`; highlight precomposition, snapshot capture, and every
hit-test/save/reopen path require a dedicated non-default-space journey.
Blocked: production PDFium redistribution, Windows save-target authority, and
fresh native visual/live accessibility on this headless VPS. Not run: packaged
Linux/macOS/Windows candidates, physical-device input, IME, Hibbeler, and
matched Electron/GPUI performance. Electron remains the shipping rollback.

Next cutover-critical slice: construct one provenance-controlled non-default
CropBox plus `/UserUnit` fixture and prove page/thumbnail/tile/annotation
hit-test agreement through Save As and fresh reopen, then qualify the shared
annotation compositor against that same coordinate space.

## 2026-08-27 real `/UserUnit` qualification

The deterministic public fixture bundle now includes
`bp-coordinate-space-v1.pdf`. Its locked page dictionary combines a non-zero
CropBox, `/Rotate 90`, and `/UserUnit 2`; the PDF SHA-256 is
`dc450b09b502f23518ed361986d9a939ed6b9c2dc1fdb6890af30fae4b253a7d`.
The real native journey opens that exact file with the checksum-pinned
development PDFium library, renders page, thumbnail, and bounded tile pixels,
creates and hit-tests a Rectangle in raw PDF points, precomposes a Highlight
through the same transform, captures a non-empty two-click Snapshot raster,
performs Save As, independently reopens the result, opens it again in a fresh
workspace, and releases every worker and mapped surface. Rectangle, Highlight,
and Snapshot identities and coordinate-space metadata survive both reopens.

Passed Linux development evidence: fixture oracle 8/8 with qpdf and pdfinfo
accepting all seven PDFs; fixed runner policy 13/13; exact real journey 1/1
(`button-probe-20260827T055130Z-2805880.log`); guarded warm all-targets 199
active plus 22 gated ignores in 50 seconds
(`button-probe-20260827T055441Z-2807434.log`); source preparation 4/4; prepared
digest `630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
one GPUI identity across 870 packages; dependency policy with warnings but no
denial; and both storage guards with about 102 GiB free.

Failed evidence is retained red-first history: the fixture initially omitted
`/UserUnit`, the new fixture lacked locked hashes, and early pointer checks used
an over-strict floating tolerance, a filled-body assumption for an unfilled
Rectangle, stale pre-draw bounds, and a rotated-page Highlight compositor
bypass. All are fixed. The broad rustfmt check is not green because the
inherited dirty experiment has formatting drift outside this slice; unrelated
user files were not rewritten. Blocked: production PDFium
redistribution, Windows save-target authority, and fresh native visual/live
accessibility on this headless VPS. Not run: packaged candidates, physical
macOS/Windows input, IME, Hibbeler, third-party coordinate corpora, and matched
performance. Electron remains the shipping rollback.

Next cutover-critical slice: complete the custom/imported template-library
lifecycle through the existing real document-session seam, including failure,
dirty-state, persistence, and reopen evidence without touching production
template storage.

## 2026-08-27 imported template-library spine

The experiment now has a GPUI-free, versioned `TemplateLibrary` ownership
boundary. It uses a sentinel-owned root, strict stable IDs, normalized names,
checksum-bound private PDF copies, atomic index publication, persistent
last-used selection, and safe materialization into the existing disposable
generated-document store. The managed template and the dirty document created
from it have separate ownership and lifetimes.

Passed Linux development evidence: pure restart/import/materialization/removal
journey 1/1 (`button-probe-20260827T061249Z-2816433.log`); retained workspace
journey 1/1 (`button-probe-20260827T060924Z-2814015.log`); checksum-pinned real
100-page PDFium render, failed-import isolation, removal, Save As/reopen, worker
replacement, and resource release 1/1
(`button-probe-20260827T061315Z-2816889.log`); warm all-targets 200 active plus
23 gated ignores in 46 seconds
(`button-probe-20260827T061454Z-2817891.log`); guard/source policy 17/17;
prepared digest `630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
one GPUI identity across 870 packages; dependency policy with warnings but no
denial; and about 102 GiB free after the builds.

Failed evidence is retained TDD history only: the first run proved the module
was absent, and the second exposed a borrowed test snapshot across mutation.
Both are fixed and the Cargo targets were retained. Partial: the existing real
GPUI Component template split control still shows only the six built-ins;
custom/imported manager rendering and user input are not connected. Blocked:
production PDFium redistribution and Windows save-target authority. Not run:
fresh native visual/live accessibility, IME, packages, physical macOS/Windows,
Hibbeler, and matched performance. Electron remains the shipping rollback.

Next cutover slice: render the persistent snapshot through real GPUI Component
`Dialog`, list, input, and button primitives, then connect import/select/create/
remove commands to this library without duplicating storage logic.
