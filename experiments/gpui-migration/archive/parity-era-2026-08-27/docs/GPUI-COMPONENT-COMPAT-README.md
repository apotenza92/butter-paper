# GPUI Component compatibility probe

This crate is the isolated Phase 0 dependency, component, and build-safety
probe. It does not feed the GPUI-CE gallery or the production Electron
application.

## Reviewed inputs

- Longbridge GPUI Component:
  `c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4`
- Longbridge source tree:
  `027dd3ea35614ddd365ac352987047c190ae051f`
- Exact Zed GPUI revision required by that component source:
  `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc`
- Zed source tree: `85eccaf309692769ec7458482ec7b39c6faf430f`
- Rust toolchain: `1.97.1`

`source-preparation-policy.json` pins every source revision, tree, license
checksum, preparation patch checksum, allowed Git source, and the deterministic
prepared-tree digest. `THIRD_PARTY_NOTICES.md` records the reviewed license and
asset provenance.

The preparation patch keeps Longbridge on its intended Zed API, replaces the
reachable GPL-marked tracing packages with the local Apache-2.0
`ztracing::instrument` compatibility shim, and removes the forced `profiler`
and `runtime_shaders` features. It does not copy or relabel GPL source.

## Development proof

`tests/component_stack.rs` initializes GPUI Component, installs `Root`, and
uses the real published component APIs from the prepared source:

- a real `Button` receives non-zero rendered bounds and one simulated primary
  click;
- a real two-child `ButtonGroup` gives both children non-zero rendered bounds
  and reports the exact single-selection result `[0]`.
- a real `DropdownButton` preserves the Continuous primary action, opens a real
  `PopupMenu` and transfers focus to it, selects the Zoom wheel behavior by
  keyboard, updates feature-owned state, and preserves stable control IDs
  across the resulting rerender.

`tests/viewer_toolbar_strip.rs` starts each slice from a failing rendered
tracer and composes retained Continuous, Single Page, Zoom, and CAD View
controls with real GPUI Component primitives:

- exact toolbar, scroll-owner, fit-action, paired split, and paired primary IDs
  remain stable;
- primary pointer activation keeps Continuous and Single Page selection
  exclusive while their Scroll/Zoom settings remain independent;
- both real menus select by keyboard, Escape dismisses and returns focus, and
  disabled state suppresses every primary, caret, fit, and double-click path;
- Continuous double click invokes Fit width and Single Page double click invokes
  Fit page without counting either as a second primary activation;
- the real Zoom Out and Zoom In buttons preserve pointer and Enter/Space
  behavior, while the real percentage Popover/PopupMenu preserves ordered
  preset selection, checked state, Escape return to the prior toolbar focus
  owner, disabled suppression, bound no-ops, and double-click reset to 100%;
- the real CAD primary `Button`, settings `Popover`, organisation
  `ButtonGroup`, and page-count `NumberInput` preserve activation, Columns/Rows
  pointer and keyboard selection, 1–100 clamping, Escape dismissal/focus
  return, disabled suppression, document reset, and independent fit, zoom, and
  wheel state. The feature state remains in an experiment-owned retained
  entity;
- 720 px keeps every target fully visible. Content without CAD is exactly 607
  px and the CAD composition is exactly 667 px, so 480 px and 320 px
  intentionally use non-wrapping horizontal overflow. Compact targets retain
  their exact size, never overlap or shrink, remain inside the scroll content,
  and the trailing target becomes fully visible after horizontal scrolling.

`tests/document_tab_bar.rs` freezes the separate Electron Document Tab Bar
contract and renders a real GPUI Component `TabBar`, `Button`s, and controlled
`Popover` around experiment-owned document/template state:

### Frozen Electron pointer-drag contract

The pointer contract comes from `DocumentTabBar.tsx`,
`ClosableDocumentTab.tsx`, `DocumentTabBar.test.ts`, and the exact installed
`@dnd-kit/core` 6.3.1 and `@dnd-kit/sortable` 10.0.0 sources:

- only the primary pointer (`isPrimary` and button zero) can arm a drag. The
  sensor measures Euclidean distance from pointer down and activates only when
  `sqrt(dx² + dy²) > 6`; exactly six pixels remains a normal pending click;
- the sensor installs move, up, and cancel listeners on the owner document. It
  does not call the browser pointer-capture API. Pointer up before activation
  aborts the sensor and leaves the ordinary tab click available. Activation
  captures subsequent click propagation, clears text selection, and suppresses
  further selection changes until teardown;
- Escape, `pointercancel`, window resize, and document visibility change cancel
  without committing. A normal pointer release after activation calls
  `onDragEnd`; release with no target does nothing;
- collision uses `closestCenter` across the registered tab rectangles. The
  horizontal sorting strategy translates the dragged tab and only the siblings
  between its source and current target. Final order removes the stable ID from
  its source index and inserts it at the target index. An identical or invalid
  target is the original-array no-op;
- the close button is outside the sortable trigger and does not receive drag
  listeners. A committed pointer reorder preserves the active document and the
  complete document object. It emits the same polite move text as keyboard
  reorder but deliberately does not request keyboard-style focus restoration;
- there is no explicit disabled/loading/dirty/temporary drag branch. Those
  states travel with the document object by stable ID. The source defines no
  distinct drop-target decoration beyond sortable translation, no wrapping,
  and no custom drag announcement beyond the post-commit polite status;
- `DndContext` leaves auto-scroll enabled. Its timer scrolls a scrollable
  ancestor when the pointer enters the library threshold near an edge. A
  faithful GPUI auto-scroll port is a separate interaction surface and remains
  the boundary after the non-scrolling drag probe; this slice must not invent a
  different edge policy.

- domain-stable tab and close IDs render with exact non-zero bounds, and each
  close action retains the frozen 24 by 24 px hit target without increasing the
  380 px intrinsic strip;
- pointer selection, ordinary double click, looping Left/Right, Home/End
  activate-on-focus traversal, clean active and inactive close, next/previous
  successor selection, last-tab empty state, and successor focus pass through
  deterministic retained events;
- literal Alt+Shift+Left/Right moves the focused tab by stable document ID
  without wrapping. Both directions, first/last boundaries, repeated moves,
  missing and additional modifiers, active and inactive focused tabs, dirty and
  template-created tab data, same-ID focus retention, and exact move text pass.
  A stable one-pixel `Role::Status` node uses AccessKit `Live::Polite`; boundary
  no-ops do not duplicate its retained announcement;
- close is pointer-inert at rest, becomes active only on close-button hover,
  remains keyboard activatable for the active tab, and dirty close opens one
  real non-modal GPUI Component `Popover` without removing the tab;
- short labels keep their natural size, while a long full document name uses
  the real `Tab` ellipsis path under a 190 px cap derived from the unchanged
  380 px strip. Stable experiment label bounds do not replace the real text;
- pointer entry renders a 34 px fade with a 14 px solid tail over the label and
  pointer exit removes it without moving the tab or its 24 px close action.
  Electron provides no label tooltip, `title`, delay, content, or placement, so
  pointer and keyboard focus both prove that no native label tooltip is added;
- the dirty-close surface renders the frozen title/body and ordered real
  `Cancel`, `Discard`, and `Save` buttons. It records deterministic intents
  only: Cancel and Discard dismiss, Save enters `Saving…` busy state and stays
  open, and no action closes a tab or touches storage;
- pointer and keyboard actions, first-focus Cancel, non-trapped Tab traversal,
  Escape and outside-click cancellation, exact keyboard-trigger focus return,
  repeated-request suppression, active/inactive target identity, and busy-state
  dismissal/action suppression pass without changing template, CAD, viewer, or
  document state;
- the primary action creates from the retained last template and appends one
  active temporary dirty tab without changing the two existing tabs;
- all six ordered built-in rows select without creating, Create commits the
  transient selection, row double click creates immediately, and Manage emits
  only an experiment counter;
- the pending-create state disables only Create, matching the Electron source;
- standard Tab traversal plus Enter/Space activation, primary pointer and ordinary double-click
  activation, Escape dismissal, and exact prior-owner focus return pass;
- the 1200 px default and 900 px minimum widths contain all targets. At 320 px,
  the 380 px intrinsic non-wrapping content scrolls horizontally without
  overlap, shrinking, or clipping. The independent viewer toolbar remains
  exactly 667 px.

The 480 px center-strip fixture follows the maintained Electron shell contract:
a 900 px minimum window minus the 180 px and 220 px minimum sidebars, with a
conservative allowance for pane handles and shell borders. The explicit
horizontal scroll policy is required because the expanded default-component
toolbar needs 607 px without CAD and 667 px with CAD rather than silently
shrinking interactive targets.

This pinned `PopupMenuItem` API does not expose a product `ElementId` or
accessibility ID for its standard rows. The probe therefore preserves exact
menu labels and verifies their state-changing keyboard result, but it cannot
carry Electron's menu-row test IDs without replacing the standard menu item.
That is recorded as a component limitation, not hidden by a custom row.

`src/bin/component_story.rs` is the smallest native story for those same
components, including selectable document tabs, clean close, the dirty-close
confirmation, the separate template action, and the paired page-view,
controlled zoom, and CAD toolbar. The locked all-targets gate compiles it with
the exact Zed platform adapter.
Compilation is not a native screenshot or physical-input result.

The paired-toolbar Linux x86_64 cold all-targets run passed in 459 seconds with
one Cargo job, incremental compilation disabled, about 1.22 GiB maximum
resident memory, a 2.52 GiB disposable target, and about 109 GiB free after the
run. The exact paired story extension then passed from the retained target in 5
seconds with about 1.17 GiB maximum resident memory. This is development-only
evidence. It is not packaging or release evidence.

The zoom-toolbar Linux x86_64 cold all-targets run passed in 455 seconds with
one Cargo job, incremental compilation disabled, about 1.22 GiB maximum
resident memory, a 2.52 GiB disposable target, and about 109 GiB free after the
run. The exact zoom story extension then passed from the retained target in 5
seconds. This is also development-only evidence.

A final warm guarded run passed in 10 seconds after adding an explicit
JavaScript-compatible half-up percentage-rounding check. Exact pointer-open
focus restoration to the percentage trigger remains a component gap: this
GPUI Button prevents pointer focus and Popover restores the prior focus owner.

The CAD-toolbar Linux x86_64 cold all-targets run passed in 492 seconds with
one Cargo job, incremental compilation disabled, about 1.22 GiB maximum
resident memory, a 2.52 GiB disposable target, and about 109 GiB free after the
run. The exact CAD story extension then passed from the retained target in 7
seconds. These are development-only compile and deterministic-render evidence,
not native visual, packaged, or physical-device evidence.

The Document Tab Bar template slice passed its final Linux x86_64 cold
all-targets run in 489 seconds with one Cargo job, incremental compilation
disabled, about 1.21 GiB maximum resident memory, a 2.60 GiB disposable target,
and about 109 GiB free afterward. The exact story extension then passed from
the retained target in 6 seconds. These are development-only compile and
deterministic-render evidence.

The active-selection and clean-close slice passed its final Linux x86_64 cold
all-targets run in 484 seconds with one Cargo job, incremental compilation
disabled, about 1.21 GiB maximum resident memory, a 2.60 GiB disposable target,
and about 109 GiB free afterward. All 18 Rust integration tests passed. The
exact selectable and clean-closable story extension then passed from the
retained target in 8 seconds. These are development-only compile and
deterministic-render evidence. After owned-file formatting, the exact final
all-targets source passed again from the retained target in 13 seconds; the
target remained about 2.60 GiB and free space remained about 109 GiB.

The dirty-close confirmation slice passed its final Linux x86_64 cold
all-targets run in 475 seconds with one Cargo job, incremental compilation
disabled, about 1.22 GiB maximum resident memory, a 2.60 GiB disposable target,
and about 109 GiB free afterward. All 21 Rust integration tests passed. The
exact story update then passed from the retained target in 8 seconds, growing
the target by about 1.9 MiB. These are development-only compile and
deterministic-render results, not native visual, packaged, or physical-device
evidence.

The document-label and pointer-mask slice passed its final Linux x86_64 cold
all-targets run in 510 seconds with one Cargo job, incremental compilation
disabled, about 1.22 GiB maximum resident memory, a 2.60 GiB disposable target,
and about 109 GiB free afterward. All 23 Rust integration tests passed. The
exact story now contains a capped long label and passed again from the retained
target in 8 seconds. The default 1200/900/320 fixtures retain the exact 380 px
tab strip, 24 px close actions, and 667 px viewer toolbar. These are
development-only compile and deterministic-render results.

The keyboard-reorder slice passed its final Linux x86_64 cold all-targets run
in 486 seconds with one Cargo job, incremental compilation disabled, about
1.22 GiB maximum resident memory, a 2.60 GiB allowlisted target, and about
109 GiB free afterward. All 27 integration tests passed. The exact story text
then compiled from the retained target in 8 seconds. The rendered tests prove
stable-ID order and bounds, active identity, same-ID focus, exact position
announcements, boundary and modifier behavior, whole-tab data movement,
pending dirty-confirmation identity, later clean-close semantics, and unchanged
380 px tab-strip and 667 px toolbar contracts. This is Linux development-only
evidence, not native visual, packaged, or physical-device evidence.

The pointer-drag slice passes the frozen non-scrolling drag contract. Four
focused rendered tests prove the strict greater-than-six-pixel activation,
below-threshold click behavior, stable-ID reorder in both directions, adjacent
and multi-position movement, same-position and boundary no-ops, active/focus
and complete tab-data retention, close-button and non-primary-button isolation,
deterministic cancellation, pending dirty-confirmation identity, exact move
announcement, and unchanged 1200/900/320 geometry. The story now advertises
both pointer and keyboard reorder. The story all-targets gate passed 32
integration tests in 9 seconds from the retained target. After restoring two
formatting-only upstream files to the verified prepared digest, the final exact
source gate passed the same 32 tests in 46 seconds, used one Cargo job, and
reached about 1.21 GiB maximum resident memory. The target remained about 2.60
GiB with about 108.6 GiB free. This is development-only evidence.

GPUI at the pinned revision has no direct live-region builder on `Div`. The
experiment uses the public GPUI accessibility subtree seam only to set the
status node's AccessKit live property to `Polite`; it does not patch GPUI or
GPUI Component. The deterministic state and compiled accessibility
configuration pass. A real assistive technology announcement remains blocked
without a graphical session and must not be inferred from compilation.

The pinned `Tab` does not expose an ID for its internal label box, so a
zero-impact experiment tracer records the allocated label bounds while the real
component keeps text shaping and ellipsis. Electron's CSS also masks a label
when its sibling close button is keyboard-focused. The pinned icon `Button`
keeps its `FocusHandle` private, so its parent cannot express that sibling-focus
mask without replacing or patching the component. Pointer masking passes;
keyboard-focus masking remains an explicit upstream capability gap. The close
button itself remains visible and activatable from the keyboard.

This pinned `ButtonGroup` does not provide the Electron organisation group's
roving Left/Right/Home/End selection contract or radio-group semantics. A
shallow application-owned key handler supplies that frozen behavior while the
real `ButtonGroup` retains presentation and pointer selection. Exact
pointer-open restoration to the settings trigger is also not proved: the
pinned GPUI `Button` does not take pointer focus, so the deterministic test
proves Escape return to the prior toolbar focus owner. Both limits remain
explicit component gaps. The exact CAD tooltip/title/description strings are
bound and tested, but this pinned icon-only `Button` derives its accessibility
label only from a visible label and has no separate accessibility-label setter;
the tooltip does not name it. The missing icon-button accessible name is an
upstream capability gap and is not disguised with a custom button.

The tab close action has a stable accessibility ID and its frozen exact
`Close {full document name}` label is helper-tested. The pinned icon-only
`Button` cannot carry that separate live accessible name, and Electron defines
no close tooltip to reuse. The probe does not invent one. Live close-button
naming therefore remains an explicit upstream capability gap.

The repository `pnpm check` gate also passes: repository hygiene, generated
icons, type checking, package builds, 1,088 Vitest tests, and 23
signature-relay tests. pnpm warns that this VPS has Node 22.22.1 rather than the
declared Node 24.16.0; the gate still exits successfully.

The configured dependency audit passes advisories, licenses, and sources. It
reports upstream warnings for two Zed crates without manifest license fields;
their exact-revision Apache-2.0 clarification evidence is checksum-bound in the
policy. It also reports unmaintained transitive packages. No denied security
advisory, rejected license, unknown source, forbidden feature, `zlog`, or
`ztracing_macro` is resolved.

## Storage guard

Every component build uses the allowlisted disposable target
`../.build-targets/gpui-component-compat`. The local guard requires 30 GiB free
before starting, stops below 20 GiB to protect an absolute 18 GiB floor, caps
the target at 4 GiB, uses one Cargo job, and disables incremental compilation.
A successful run, ordinary Cargo failure including status 101, wall timeout,
session interruption, or memory stop retains a valid owned target. Automatic
cleanup occurs only for a preflight/runtime free-space breach or target-size
breach. Explicit cleanup remains available through `build-guard.mjs cleanup`
for corruption or storage recovery. Every summary records whether the target
was retained or cleaned and why. Logs and summaries remain bounded under the
ignored `.prepared/evidence/` directory.

Deterministic tests cover cleanup disposition, fixed reviewed runner modes,
preflight failure, low-space abort logic, exact path allowlisting, target-size
enforcement, explicit owned cleanup, unowned-directory rejection, symlink
rejection, and a Cargo temporary directory disappearing between `lstat` and
`readdir`, all without filling the disk.

## Reproduce

Run the non-build gates first:

```sh
node --test tests/build-guard.test.mjs tests/source-preparation.test.mjs tests/foundation-truth.test.mjs
node scripts/foundation-truth.mjs
node scripts/prepare.mjs verify
node scripts/verify-cargo-graph.mjs
cargo deny --config deny.toml --exclude-dev --locked check \
  --warn unmaintained advisories licenses sources
```

Then run the locked build through both guard layers:

```sh
host-storage-guard check
host-storage-guard run -- bash scripts/run-bounded-button-probe.sh pointer-drag
host-storage-guard run -- bash scripts/run-bounded-button-probe.sh all-targets
```

The reviewed modes include `pointer-drag`, `document-tab-bar`, `document-spine`,
`document-spine-real`, `page-scale`, `application-close`,
`application-close-integration`, `application-close-real`, the fixed gallery
backend modes, `all-targets`, and the controlled `retention-proof`. The runner
rejects extra arguments and unknown modes. Use a focused mode for red/green
iteration and reserve `all-targets` for acceptance. Never delete the owned Cargo
target merely because a test-driven-development red test failed.

Do not run a component build outside the bounded wrapper. The compatibility
application is the current isolated candidate. The GPUI-CE gallery remains a
historical evidence source; do not add its GPUI identity to this graph.

### Foundation reconciliation evidence

GitHub issue #83 records the current pass, fail, blocked, and not-run evidence.
Keep this guide limited to durable policy and reproduction commands. A Linux
development pass does not approve production PDFium, production promotion,
native visual/accessibility behavior, packaging, updater replacement, or
physical-device behavior.

## Evidence status

- Passed: exact source preparation, deterministic checksum and policy tests,
  one GPUI identity, dependency license/source audit, Linux all-targets compile,
  Button render/activation, ButtonGroup render/selection, Continuous
  DropdownButton render/primary activation/menu focus/keyboard selection/state
  change/stable rerender IDs, paired Single Page render/primary activation/menu
  keyboard selection/independent state/exclusive selection/stable IDs, composed
  toolbar pointer/menu/Escape/focus/disabled/double-click behavior, controlled
  zoom pointer and keyboard stepping, preset selection, formatting, checked
  state, clamping, bound no-ops, Escape/focus return, disabled suppression and
  double-click reset, CAD activation/configuration/keyboard/dismissal/reset and
  independence, 720/480/320 px layout with exact 607 px non-CAD and 667 px CAD
  overflow thresholds, Document Tab Bar template creation/selection/manage/
  disabled/keyboard/dismissal/independence behavior, 1200/900/320 px tab-bar
  geometry with a 380 px constrained intrinsic threshold, stable document-tab
  and close IDs, 24 px close targets, pointer and looping keyboard selection,
  ordinary double-click activation, active/inactive/last clean close,
  next/previous successor selection and focus, dirty-close deferral, exact
  confirmation copy/order/IDs, pointer and keyboard Cancel/Discard/Save intents,
  first-action focus, non-modal traversal, Escape/outside dismissal, keyboard
  focus return, busy and repeated-request suppression, active/inactive target
  identity, confirmation geometry and complete state independence, compiled
  exact Document Tab Bar plus CAD native story, stable short/long label bounds,
  the 190 px long-label cap, pointer mask entry/exit with exact 34/14 px
  geometry, explicit no-tooltip behavior, strict six-pixel pointer drag,
  translated preview, stable-ID pointer reorder and announcement, retained
  dirty-confirmation identity, and guarded target-retention behavior.
- Passed in this keyboard-reorder slice: exact stable status and tab IDs,
  rendered order/bounds in both directions, no-wrap boundaries, duplicate
  suppression, literal Alt+Shift matching with extra Control/Super allowed,
  same-ID focus, active-ID preservation, inactive dirty-tab and template-tab
  data movement, pending dirty-close target preservation, clean-close behavior
  after reorder, feature-state independence, fixed-width geometry, and the
  compiled `Role::Status` plus AccessKit polite-live adapter.
- Passed in this pointer-drag slice: below-threshold click/no-reorder, strict
  activation beyond six Euclidean pixels, both directions, adjacent and
  multi-position movement, same-target and edge no-ops, primary-button and close
  isolation, release and deterministic cancellation, whole-identity movement,
  active/focus retention, pending-confirmation identity, exact polite text, and
  fixed 1200/900/320 geometry. The final exact all-targets run passed 3
  component-stack, 21 document-tab, and 8 viewer-toolbar tests.
- Failed during development and resolved: the first bounds lookup used an
  `ElementId` instead of GPUI's test-only `debug_selector`; the first story
  builds omitted explicit extension-trait imports and listener type annotation;
  the new tracer first used the wrong entity-read context, then produced the
  intended red failure for the missing menu; a disabled custom menu heading
  exposed that this upstream revision's arrow iterator does not skip disabled
  or label rows, so the final probe uses only the two standard actionable rows.
  The toolbar tracer then produced the intended red missing-composition failure;
  one cold green attempt exposed a missing extension-trait import, and the next
  proved that 320 px is the constrained overflow fixture rather than the
  supported minimum. The paired tracer produced its intended red missing-Single
  Page failure. Its first green attempt passed all behavior but found a 509 px
  composition clipping the 480 px supported strip; the resolved implementation
  uses the component system's compact toolbar size and passes 480 px without
  shrinking. Every failed probe cleaned its disposable output. `cargo
  fmt --all --check` also reports formatting drift inside the checksum-bound
  prepared upstream tree; the owned probe files were formatted directly
  without changing that reviewed tree.
- Failed during this zoom slice and resolved: the red tracer failed on the
  absent Zoom Out ID; the first green compile found overlapping mutable borrows
  in the test helper; the next run established the 607 px overflow threshold;
  two keyboard runs showed that the subgroup needed its own GPUI tab-group
  focus boundary and the focused frame must be drawn before key dispatch.
  Every failed run cleaned only the owned disposable target.
- Failed during this CAD slice and resolved: the stable-ID tracer first failed
  on the absent primary; the configuration tracer then failed on the absent
  Columns target; initial green compiles exposed a missing `StyledExt` import,
  a missing `AppContext` import, and the actual zero-argument `InputState::value`
  API. The full contract tracer failed as intended on the absent document-reset
  seam. The first geometry assertion incorrectly ordered nested split-primary
  bounds as siblings; the corrected tracer then measured 667 px rather than
  the estimated 675 px. A standalone formatting command also needed the
  crate's explicit 2021 edition before the story build. Each failed Rust probe
  cleaned only the exact owned disposable target.
- Failed during this template slice and resolved: the rendered tracer first
  failed on the absent template primary and picker. One green attempt exposed a
  build-guard race when Cargo removed a temporary metadata directory between
  `lstat` and `readdir`; a deterministic injected-filesystem regression now
  covers that case. The full red contract then failed on the absent retained
  tab/template APIs as intended. Its first green run compiled and passed the
  stable contract, then measured the 380 px intrinsic constrained threshold and
  showed that the Escape test had moved focus out of the Popover before sending
  Escape. The corrected focus path and measured geometry passed. Each failed
  Rust run cleaned only the exact allowlisted target.
- Failed during this active-selection and clean-close slice and resolved: the
  rendered tracer first failed because the close sites did not participate in
  layout. Initial green attempts exposed unsupported icon-button accessibility
  labeling in the pinned API, incorrect close offset/hover routing, an 8 px
  intrinsic-width increase caused by using tab suffixes, and a test-only keyed
  state that could not be captured outside its render path. The final seam uses
  real Tab order, keyboard-origin click events, and zero-width absolute close
  overlays. Every failed Rust probe cleaned only the exact allowlisted target.
- Failed during this dirty-close slice and resolved: the rendered red tracer
  first failed on the absent confirmation bounds, and the full behavioral red
  tracer then failed on the absent intent/busy APIs. Three cold green attempts
  exposed a missing `FluentBuilder` import, a moved focus-handle clone, and a
  real focus-timing defect where traversal ran before the Popover actions were
  mounted. Moving first-action focus to the first rendered content frame made
  the exact contract green. Every failed run cleaned only the exact allowlisted
  disposable target.
- Failed during this label/mask slice and resolved: the intended red tracer
  first failed on the absent stable label bound. One cold compile exposed a
  theme-token/color mismatch. Two following runs showed that a custom text
  child and then a 160 px cap each reduced the frozen strip from 380 to 350 px;
  the final composition restores the real `Tab` label and derives a 190 px cap
  without weakening geometry. A keyed-state and Tab-key focus adapter could not
  observe the pinned `Button` focus handle, so it was removed and recorded as a
  component gap. One final test corrected a comparison between full-width and
  intrinsic toolbar bounds. Every failed run cleaned only the exact allowlisted
  disposable target.
- Failed during this keyboard-reorder slice and resolved: the two intended red
  tracers failed on the absent status bound and unchanged tab order. One
  expanded test incorrectly expected focus to remain on the dirty tab after
  ordinary Shift+Left/plain Left selection, and one pending-identity test
  compared the 1200 px flex content box with the 320 px intrinsic threshold.
  The corrected same-fixture and explicit-focus assertions pass. Every failed
  guarded run cleaned only the exact allowlisted target.
- Failed during this pointer-drag slice and resolved: the intended missing-drag
  tracer failed first; early cold runs exposed pinned `Tab` capture behavior,
  two borrow errors, contract mismatches, and a dirty-confirmation pointer-order
  defect. The superseded runner deleted the target after each status 101. After
  the feedback-loop fix, a real status-101 test and a controlled retention proof
  both retained the exact target. The corrected focused gate passed warm in 3
  seconds. The story all-targets gate passed in 9 seconds, and the final
  verified-digest all-targets gate passed in 46 seconds.
- Blocked: this VPS has no existing X11/Wayland graphical session, so the exact
  story could not be launched for a fresh screenshot or live accessibility
  tree without new infrastructure. Hibbeler corpus and transferred macOS visual
  evidence are absent.
- Not run: input method editor (IME), Wayland/X11 breadth, macOS/Windows compile
  and package, packaged candidate, physical-device input, and performance
  comparison.

## Current cutover slice

The editable `DocumentWorkspace` is now the cutover spine. It retains stable-ID
`NativeDocumentSession` entities, renders application-owned raster snapshots,
dispatches thumbnail navigation, rejects stale generations, isolates a failed
second open, and closes the owned resource without applying later work. A thin
worker adapter reuses `butter-paper-gpui-gallery` with `default-features =
false`; no GPUI-CE crate enters the resolved graph. The local worker target
includes the already-reviewed experiment worker shell and exact
`pdfium-render` revision `6cee8b9a…`.

Passed:

- the first focused red gate failed on the absent module and retained the
  2.60 GiB target;
- three deterministic document tests pass: stable public-fixture SHA-256,
  stable-ID entity/raster/thumbnail lifecycle, failed-second-open isolation,
  two-generation stale rejection, abstract resource release, and post-close
  rejection;
- the strengthened `document-spine-real` gate passes against the reviewed
  development PDFium 7881 library. It proves a real 100-page open, non-uniform
  page pixels, 12 real thumbnail buffers, a rendered stable-ID thumbnail click
  that advances the retained page through the real background worker, a failed
  second open that leaves that page and worker usable, worker PID exit, and no
  mapped-surface residue;
- the existing fetch policy accepted only
  `pdfium-linux-x64.tgz` from the pinned `chromium/7881` release at 3,644,759
  bytes and SHA-256
  `1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d`.
  The extracted development-only library SHA-256 is
  `f728930966f503652b92acc89b9374a2eeca00ce42e26dccd3e4b5c5161b2d64`;
- the first strengthened real run found an empty surface directory left by a
  failed open. The constructor now removes only that identity-specific output
  on failure; the same guarded regression is green;
- the last pre-rotation exact-source guarded all-targets gate passed 79 non-ignored tests
  with two separately gated ignored tests in 24 seconds, one Cargo job, no
  incremental compilation, and about 106.6 GiB host free space. A
  package-scoped safety cleanup removed 1.7 GiB of obsolete copies of this
  experiment's own binaries before the next build, while preserving shared
  dependency artifacts; the retained target is about 2.65 GiB after the fresh
  real-PDFium and story-build gates;
- the prepared tree digest remains
  `630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
  13 shared gallery inputs are checksum-bound; the 870-package graph has one
  GPUI identity and no forbidden package; and the configured advisory, license,
  and source audit passes with the previously reviewed missing-license-field
  and unmaintained warnings.

The representative Rectangle and Save As journey now also passes:

- the real GPUI Component Rectangle button selects the application-owned tool;
  native pointer events project through the contained page bounds into PDF
  bottom-left points and commit once through the shared annotation adapter;
- main-page selection chrome and thumbnail geometry render from the retained
  scenes without placing feature state inside GPUI Component;
- injected save failure preserves the source path, worker, selected Rectangle,
  revision, and dirty state;
- the exact development journey writes a new 100-page PDF, reopens the typed
  Rectangle through lopdf, passes `qpdf --check` and `pdfinfo`, reopens the
  current page through PDFium, proves changed pixels, swaps workers only after
  validation, marks the revision saved, and releases every worker;
- 13 shared gallery inputs are checksum-bound, including the annotation
  adapter/model, editor math, PDF writer, viewer planner, worker, and PDFium
  development manifest.

The visible bounded viewer slice also passes:

- a private per-session `DocumentViewerState` owns mode, zoom, measured scroll,
  raster revision, planner generation, a two-job asynchronous queue, and a
  256 MiB byte-accounted cache;
- deterministic rendered tests prove stable page/tile IDs, Continuous and
  Single Page replanning, 400% to 800% zoom changes, multiple scroll positions,
  bounded queue/cache state, stale completion rejection, and atomic Save As
  planner/cache invalidation;
- the workspace keeps a valid full-page preview mounted while real tiles arrive
  and composes accepted tiles at exact crop/device-scale coordinates;
- the story removes its duplicate mode toggle and observes the proven GPUI
  Component toolbar and zoom entities to update application-owned viewer state;
- the exact real PDFium gate passes in 6 seconds and proves visible asynchronous
  800% page-0 tiles, real thumbnail navigation, a new page-1 generation and
  visible tiles, spatially varied complete BGRA pixels, cache bounds, worker
  exit, and run-scoped mapped-surface cleanup;
- the earlier viewer-checkpoint two-layer guarded all-targets gate passed 44 non-ignored tests plus
  one separately gated real test in 18 seconds and retains the 3.23 GiB target.

The imported Rectangle reconciliation slice now passes too. The real PDF
opener hydrates all five typed annotation families into application-owned
session state. Rectangle evidence proves clean import, stable-ID selection,
previewed native-pointer move and resize, one history entry per commit,
property edit, lock suppression, undo/redo, deletion, and native Save As
absence after independent reopen. Real GPUI Component `ButtonGroup`, `Button`,
and `Popover` controls dispatch Select, Rectangle, Undo, Redo, line-width,
Lock/Unlock, and Delete commands. The raw PDF canvas remains Butter Paper
domain UI and paints eight resize handles plus a rotation handle. The writer
removes the exact `/Annots` reference without pruning unrelated PDF objects.

The Pen vertical slice now passes as Linux development evidence. The real
GPUI Component Pen button delivers native pointer gestures to the shared
adapter, ignores short drags, and creates a stable selected smooth path with
the Electron default red, 1 pt, 100% appearance. Main-page and thumbnail scenes
paint the same retained path. A real GPUI Component opacity Popover updates
application-owned state; movement, opacity, lock, delete, undo, and redo remain
in the shared history. Typed PDF Save As reconciles Pen create, edit, and
deletion, validates exact typed reopen, removes deleted raw objects, and refuses
source SHA-256 drift. The separately gated PDFium journey creates Rectangle and
Pen annotations in the real 100-page fixture, saves, reopens, rehydrates both
stable IDs, renders the changed page, swaps workers, and leaves no orphan.

The Text Box vertical slice now passes as Linux development evidence. A real
GPUI Component `Textarea` owns native multiline input while the retained
workspace owns pending edit state. Active-window tests prove Enter inserts a
newline, Escape and guarded blur commit once, and an empty new edit creates no
markup or history. The page and thumbnail domain canvas shape and paint the
retained text with its PDF-space bounds. Application commands prove content,
move, resize, lock, delete, undo, and redo. Typed FreeText create/edit/delete
Save As passes exact reopen checks. The real 100-page PDFium journey now saves,
rehydrates, and renders stable Rectangle, Pen, and Text Box identities. Native
IME, clipboard, exact font metrics, in-page editor placement, and physical
accessibility remain unproved.

The Length vertical slice now passes its cutover-critical Linux development
journey. A real GPUI Component `Button` and the exact Electron `Shift+Alt+L`
binding select application-owned Length state. Two-click placement requires a
page-scoped scale, renders a stable-ID preview, applies the Electron Shift-axis
rule, rejects a distance at or below two PDF points, and cancels with Escape.
Atomic body translation, endpoint edits, lock suppression, delete, and undo use
the shared retained history. Typed create/edit/delete Save As survives exact
reopen, preserves the canonical `bp:` identity, emits a standard `/Measure`
dictionary, and does not misclassify an ordinary `LineDimension`. The focused
green gate passed warm in 19 seconds, the real PDFium worker gate passed in six
seconds, and the all-targets gate passed in 17 seconds. Pointer body drag,
caption hit-testing, semantic object snapping, exact arrow/caption appearance,
page-scale configuration UI, and native visual/accessibility proof remain
explicit gaps.

The Page Scale vertical slice now passes the Electron-defined model and
persistence contract as Linux development evidence. Real GPUI Component
`Dialog`, `Select`, `Checkbox`, `NumberInput`, `ButtonGroup`, and `Button`
primitives render application-owned preset, custom, and calibrated state. The
model preserves all five units, independent X/Y scales, every decimal and
fraction precision, current/all/range targets, built-in presets, session-only
saved presets, and atomic preset-plus-scale undo/redo. Exact `/BPPageScale`
source, name, units, X/Y factors, and precision survive two independent reopens;
replacing the full scale set removes stale page metadata. The checksum-pinned
real PDFium journey also renders, saves, and reopens a custom separate-Y,
fraction-precision scale while proving worker cleanup. Focused Page Scale tests
pass 5/5 in 17 seconds, the GPUI-free model gate passes 1/1 in 9 seconds, PDF
persistence passes 15/15 in 10 seconds, and the real worker journey passes 1/1
in 11 seconds.

The remaining Page Scale gaps are native input and visual acceptance, exact
calibration hover-line and object/content snapping, thumbnail and canvas trigger
routes, live focus/accessibility proof, and packaged platform evidence. The
pinned GPUI dialog test host does not expose nested modal children through
`debug_bounds`; the experiment therefore records the conditional stable-ID
render branch without claiming native pixels.

The page-rotation and presented-crop authority slice now passes as Linux
development-only evidence. Real GPUI Component Rotate Left and Rotate Right
buttons dispatch through the retained workspace. Rotation changes one document
revision, page and thumbnail geometry, annotation projection/hit testing, and
the viewer generation as one application-owned transaction. Deterministic
tests cover all quarter-turn raster/crop transforms, stale-result rejection,
failure-state preservation, undo/redo and dirty-close behavior, Save As
blocking while pixels are pending, canonical PDF `/Rotate`, independent reopen,
and unchanged source PDF points under rotated contained bounds.

`PaintedPageEvidence` is emitted only at the native prepaint boundary after all
current-generation visible tiles are cache-backed at one device-pixel ratio. It
binds document/page ID, open-request generation, resource generation, viewer
generation, source points, contained window-logical bounds, device-pixel ratio,
and a stable painted-state sequence. The pure capture coordinator freezes that
evidence, consumes one pending SIGUSR1 request, schedules exactly one later GPUI
frame, rejects any authority drift, and withholds success cleanup until the
matching `viewer-native-presented-state` receipt. Post-open failure paths restore
the signal handler, close the document, and wait for worker and mapped-surface
release without emitting success qualification. The story monitor holds a weak
entity and stops after entity loss.

Exact retained development logs for this checkpoint are: `perf-protocol` 14/14
in 11 seconds (`button-probe-20260825T165419Z-1572493`), `perf-story` 3/3 in
3 seconds (`button-probe-20260825T165442Z-1573166`), `perf-capture-signal` 2/2
in 3 seconds (`button-probe-20260825T165454Z-1573623`), corrected
`document-spine` 44 passed with one separately gated ignored test in 12 seconds
(`button-probe-20260825T165728Z-1576309`), and the checksum-pinned PDFium 7881
gate 1/1 in 6 seconds (`button-probe-20260825T165812Z-1577534`).
The benchmark story plus worker build passed in 11 seconds; a fresh
warning-free rerun is pending and is not treated as optimized or packaged
evidence. Performance Node tests pass 99/99 in 2703.916708 ms and build-guard
tests pass 10/10 in 337.808436 ms. The prepared source digest is unchanged at
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`.

The application-close slice now passes as Linux development-only evidence.
`ApplicationCloseShell` is the sole owner of the GPUI Component modal layer;
the workspace and close coordinator remain application-owned. The transaction
uses stable document IDs plus an exact token-to-`PathBuf` map, including a
deterministically tested non-UTF-8 Unix path. Save All drives one save at a
time, rejects stale Save As results, and Save As cancellation closes the modal
without closing a document or requesting quit. Discard All acknowledges each
document only after its worker and mapped surfaces are released. Releases run
serially. A cleanup failure cancels the transaction and reports its message
without `ReleaseAcknowledged` or quit promotion. It preserves the targeted live
session plus pending dirty-close and close-after-save identities. Clean close
and dirty Discard both surface `ReleaseFailed`, and a fresh retry succeeds. A
PDFium resource marks itself released only after worker close and
`remove_dir_all`; cleanup remains retryable after failure. The pure gate passed
9/9 in 18 seconds (`button-probe-20260825T181935Z-1667638.log`). The integration
gate passed 7 tests with one separately gated real test ignored in 6 seconds
(`button-probe-20260825T182000Z-1668397.log`); the checksum-pinned real two-PDF
lifecycle passed 1/1 in 3 seconds
(`button-probe-20260825T182023Z-1669025.log`). Deterministic injection proves
failure recovery; the real PDFium test proves normal release but does not
inject a real IPC cleanup failure.

The current warm all-targets gate passed 125 non-ignored tests with three
separately gated ignored tests in 45 seconds
(`button-probe-20260825T205849Z-1788476.log`). The exact story and PDF worker
build then passed warm in 10 seconds
(`button-probe-20260825T205946Z-1789689.log`). The preceding accepted cold gate
passed in 520 seconds with symbols stripped and a 2,708,840 KiB retained target
(`button-probe-20260825T173734Z-1615153.log`). An earlier attempt exceeded the
4 GiB owned-target cap and was stopped and cleaned by the safety policy
(`button-probe-20260825T173634Z-1613961.log`); it is failed build evidence and
not part of the accepted application proof.

The modal has deterministic state, stable IDs, bounds, and draw coverage, but
pointer hit-testing, live focus containment/return, and the accessibility tree
are not proved without an existing `DISPLAY`. No packaged or physical-device
evidence was run. This vertical slice advances cutover
readiness; it does not make the earlier micro-control parity work equivalent to
migration completion.

Line/Arrow now passes as Linux development evidence. The GPUI-free
model/history/scene and interaction gate passes 26/26, including controlled
color, width, and opacity changes, no-op history suppression, validation,
locking, and independent Line/Arrow state
(`button-probe-20260825T193425Z-1732122.log`). The real GPUI Component controls
pass 3/3 for stable IDs, pointer and shortcut selection, drag and retained
click-click placement, Shift constraints, pointer exit, scene geometry,
arrowhead geometry, and separate color/width/opacity popovers with exact
history, undo/redo, locking, and scene updates
(`button-probe-20260825T193334Z-1731229.log`). The PDF gate
passes 12/12 with Electron-compatible `/Line` standard fields, flags,
subject/contents preservation, width and opacity fallbacks, and Arrow intent
(`button-probe-20260825T190235Z-1707917.log`). The real PDFium journey creates,
saves, reopens, rehydrates, and renders both types with distinct retained
appearance values, then proves worker and mapped-surface cleanup
(`button-probe-20260825T193457Z-1732886.log`). The exact
source receipt, prepared digest, 870-package single-GPUI graph, and configured
dependency policy pass after the shared-source receipts were refreshed.
The current property popovers expose representative presets, not Electron's
complete free-form color and numeric/slider inputs. `/BPAppearance`, fresh
`/M`, native overlay capture/accessibility, and packaged-platform proof remain
incomplete.

The current multi-selection milestone is end-to-end but still development-only.
Ordered plain/Shift selection and group movement cover all six maintained
annotation families with locked-member filtering. Select All,
application-memory copy/paste with the exact repeated 12-point offset, mixed
locked deletion through the real Delete button, filter-only undo/redo
selection, fresh-reopen generated-ID advancement, and Save As/PDFium reopen
pass. A GPUI-free selection-geometry module plus the real workspace pointer
bridge prove strict greater-than-six-pixel lasso activation, the sub-threshold
two-click box form, window/crossing geometry, ordered replace/Shift-add/
Alt-remove, cancellation without mutation, and a native transient overlay.
The adapter gate is 29/29
(`button-probe-20260825T204604Z-1779890.log`), the focused real workspace route
passes two multi-selection tests
(`button-probe-20260825T205607Z-1785910.log`), the complete workspace file is
50 passed plus one separately gated ignored test in the all-targets evidence,
the PDF persistence gate is 13/13
(`button-probe-20260825T205410Z-1784109.log`), the focused stable-order model
gate is 1/1 (`button-probe-20260825T205821Z-1788015.log`), and the real PDFium
gate is 1/1 with exact page-major cross-family order after Save As and reopen
(`button-probe-20260825T205630Z-1786456.log`). Cross-page policy,
Length-caption selection bounds, rotated Text/Image geometry, and native
accessibility remain open. The overlay compiled but was not captured because
this host has no existing graphical session.

The Highlight and regular PNG Image journeys now pass as Linux development
evidence. Highlight retains application-owned defaults and history, writes
canonical multi-path Ink, and precomposes Multiply into annotation-free page,
thumbnail, and tile rasters while keeping selection chrome separate. The
regular Image journey decodes the checksum-locked 512×384 PNG through a bounded
GPUI-free decoder, places it at the Electron 45% page cap, moves at the exact
three-CSS-pixel threshold, free-resizes through the eight-handle seam, renders
one shared GPUI asset on page and thumbnail surfaces, saves through staging,
and reopens the exact RGBA identity under canonical `/NM`. Replacement and
deletion remove unreferenced Form, Image, and SMask objects. Document close
explicitly evicts the GPUI image-atlas entry and releases the retained CPU
render object. The focused `document-image` gate passes warm in 12 seconds;
the complete document-workspace gate passes 28 tests with one separately gated
PDFium test ignored; that image-slice warm all-targets gate passed in 17 seconds with
about 3.38 GiB retained target use and about 105 GiB host free space.

Failed:

- the intentional red proof failed first on missing read-only raster accessors,
  then exposed the failed-open surface-directory leak. Both ordinary Cargo 101
  results retained the valid target. No current required test is failing.

Blocked:

- native screenshot and live accessibility evidence are blocked because this
  VPS has no existing X11 or Wayland graphical session;
- production PDFium redistribution remains blocked by the checked-in
  `productionApproved: false` supplier manifest.
- native presented crop, page-rotation screenshot, and live accessibility
  evidence are blocked by the absence of an existing X11 or Wayland `DISPLAY`.
  No synthetic display was created. The one-bit SIGUSR1 adapter can coalesce
  repeated standard signals, and non-Linux worker liveness probing is not yet a
  cross-platform proof.
- application-close modal pointer/hit-testing and live focus/accessibility
  proof are blocked by the same missing graphical session.

Not run:

- production persistence, packaging, macOS,
  Windows, physical-device input, IME, Hibbeler, and performance comparison.
- an optimized release for this exact source, a packaged candidate, and native
  macOS/Windows/Linux physical-device rotation and crop evidence.
- packaged or physical-device application-close qualification.

The exact Longbridge/Zed performance scaffold now fails closed. Injected
protocol tests prove reserved-field ownership, one monotonic process/scenario
stream, stable document and generation authority, real-pixel and worker
requirements, the 250 ms unchanged-generation gate, a later GPUI frame, and
explicit worker/surface cleanup before the sole terminal event. Native command
IDs now match the X11 driver, and the `benchmark-evidence` build measures a
real GPUI input-to-platform-draw histogram delta instead of declaring a draw.
The story accepts only the frozen v4 `open-pdf` contract and reviewed
`longbridge-gpui-component-v1` profile, uses explicit worker/PDFium paths and a
per-run surface root, and builds a fixed 1200×800 window.

The JavaScript gate no longer treats fixture metadata as pixel proof. This
profile requires one XGetImage presented-client-drawable crop with exact
fixture registration, geometry, display scale, stable painted generation, and
either an exact hash or the registered scan-fidelity-v2 metric. Missing or
mismatched capture fails the iteration. A separate allowlisted release target
and fixed `perf-story-release` mode build both story and worker without GPUI
`test-support`; candidate sealing binds the toolchain, source, lockfile, graph,
build receipt, binaries, and development PDFium. No current manifest is timing
eligible. Native capture remains blocked on this VPS because no real graphical
session exists; no synthetic display was created.

The multi-document slice now passes as Linux development evidence. Real GPUI
Component `TabBar`/`Tab` session tabs now render the actual retained document
sessions and switch stable document identities. The controlled
Popover uses the exact Electron dirty-close title and warning with
Cancel/Discard/Save order. Deterministic tests prove failed close-save retry,
validated close-save, Discard, successor selection, and isolated release. The
real PDFium journey keeps two workers live and proves clean close kills only the
target worker.

The shipping Electron app has no persisted document-session restore contract,
so document-session recovery is not a parity requirement and will not be
invented during migration. Ellipse creation/edit/Save As/reopen now passes
through the existing document workspace and checksum-pinned real PDFium worker.
The focused adapter test passes the exact three-pixel threshold, curve-correct
handles, move/resize preview, rotation, double-click reset, and lock suppression.
The real journey preserves stable identity through create/edit/delete and two
validated saves while releasing replaced and closed workers plus mapped
surfaces. The guarded focused workspace gate passes 2/2 in 8 seconds, the real
PDFium gate passes 1/1 in 12 seconds, and the warm all-targets gate passes 128
tests plus three gated ignores in 25 seconds. Blank or template create-to-save
is next. The older directional comparison
remains historical and cannot decide the cutover. Package-relative worker and
PDFium loading, remaining annotation families, native visual/accessibility
acceptance, and macOS/Windows package qualification remain critical. Pointer
edge auto-scroll remains deferred.

## Generated template document journey

The experiment now connects the single real `TemplateSplitControl` to a real
`DocumentWorkspace` command. The control emits typed intents and never owns
documents. The workspace maps all six built-ins—Blank, Dots, Square Grid,
Ruled, Isometric, and Triangle—to GPUI-free, checksum-bound A3 landscape vector
generators. Each generated `Untitled.pdf` lives in a sentinel-owned temporary
store beside its stable `DocumentId`, so Save As and dirty-close behavior do not
depend on mock tab state.

Passed development-only evidence:

- all-pattern geometry, metadata, validation, element cap, safe key handling,
  source ownership, and allowlisted cleanup pass 3/3;
- deterministic workspace tests prove initial-open failure preservation,
  explicit Discard cleanup, Cancel preservation, duplicate-command suppression,
  failed Save As preservation, staged reopen, atomic publication, clean-state
  transition, and resource release;
- a rendered tracer selects Square Grid through the real picker and Create
  button, creates exactly one real session, and proves no legacy
  `template-document-*` mock tab is rendered;
- the checksum-pinned PDFium test proves real non-uniform page and thumbnail
  pixels from the same `built-in-grid` command, old-worker exit before
  temporary-source deletion, a distinct reopened worker, independent
  persistence reopen, final worker exit, and no mapped-surface residue;
- focused deterministic, tab-control, and real gates pass. The warm all-targets
  gate passes 133 tests with four gated ignores in 26 seconds.

Failed evidence retained for diagnosis: a cold red build hit the 4 GiB target
cap because the default non-incremental test profile fanned out many codegen
objects. The reviewed profile now uses one codegen unit and completed cold at
about 2.1 GiB without weakening the cap. Owned cleanup now requests bounded
retries for transient `ENOTEMPTY`; the fast guard gate passes 13/13. Ordinary
compile/test failures retain the valid target, as required.

Blocked: Windows inherited PDF source-handle transfer, production PDFium
redistribution, native screenshot/live accessibility, and packaged candidates.
Not run: custom/imported templates, template management/persistence,
macOS/Windows packages, physical-device input, IME, and final performance
qualification. Only Square Grid has the full rendered and real-PDFium journey;
the other five have deterministic generator coverage.

## Rectangle cutover journey (2026-08-26)

The representative Rectangle now crosses the complete public native seam. The
focused test clicks the real GPUI Component Rectangle and Select buttons,
creates and moves the annotation through rendered GPUI pointer input, changes
line width through the real Popover, performs a validated Save As, cleanly
closes the worker-backed session, and opens the saved PDF in a distinct
`DocumentWorkspace`. Stable identity, geometry within the explicit 0.00001 pt
PDF edge-reconstruction tolerance, appearance, clean import state, empty
selection/history, page/thumbnail scene coherence, PDFium annotation pixels,
`qpdf`, `pdfinfo`, worker exit, and mapped-surface cleanup all pass.

Passed development evidence:

- `rectangle-cutover-real` passes 1/1 in 4 seconds through both storage guards
  (`button-probe-20260826T004425Z-1946419.log`);
- the GPUI-free geometry regression passes 1/1
  (`button-probe-20260826T004403Z-1946037.log`);
- the warm all-targets gate passes 133 tests with five separately gated ignores
  in 26 seconds (`button-probe-20260826T004441Z-1946743.log`);
- guard/source policy passes 17/17, the prepared Longbridge tree remains
  `630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
  the shared annotation-model receipt is
  `74bb4230896c485a44939d8925aa7839aa550a139327602239d75ea53153fa7f`,
  and the 870-package graph still contains one Zed GPUI identity.

Retained red evidence found a real defect: bitwise equality rejected valid PDF
`/Rect` edge reconstruction after native f32 pointer projection. Validation now
uses a domain-owned 0.00001 pt geometry equivalence while keeping identity,
page, rotation, appearance, and lock state exact. A later red assertion also
confirmed that the workspace base raster is intentionally annotation-free;
the accepted pixel oracle uses PDFium's annotation-enabled render against that
base rather than changing the overlay architecture.

Blocked: fresh native screenshot/live accessibility, production PDFium
redistribution, and Windows inherited source-handle transfer. Not run: packaged
Linux/macOS/Windows candidates, physical-device input, IME, Hibbeler, and the
matched Electron/GPUI performance decision. The next cutover-critical slice is
document failure recovery around a real worker/render failure while preserving
dirty annotation state, followed by the complete Rectangle property inspector.

## Worker-crash recovery journey (2026-08-26)

`DocumentWorkspace` now treats an active PDFium worker crash as a recoverable
presentation failure rather than a failed document session. It retains the last
good raster and all application-owned state, renders the real GPUI Component
recovery Alert and Retry Button, checksum-validates a replacement resource,
rejects stale generations, and swaps only the resource presentation. Typed
`WorkerCrashed` cleanup kills and waits for the owned child and removes the
owned mapped-surface directory even when normal IPC shutdown is unavailable.

Passed development-only evidence: `document-recovery` passes 1/1 in 3 seconds
(`button-probe-20260826T012812Z-1971336.log`),
`document-recovery-real` passes 1/1 in 9 seconds after `SIGKILL` of the exact
owned PID (`button-probe-20260826T012751Z-1970947.log`), and the warm
`--all-targets` gate passes 134 tests with six gated ignores in 36 seconds
(`button-probe-20260826T012941Z-1972499.log`). Source/guard policy passes 17/17;
the exact prepared digest, single-GPUI graph, configured dependency policy, and
host-storage bounds remain green. Ordinary red failures retained the warm
target as required.

Failed evidence consists of retained red iterations that exposed missing
recovery APIs, unreachable Retry geometry, and a test assumption invalidated by
GPUI's async-draining click simulator. No accepted recovery gate remains
failing. Scoped Rust formatting passes; full Cargo formatting remains blocked
by formatting drift in the immutable prepared upstream tree.

Blocked: fresh native pixels and live accessibility because no real graphical
session exists, production PDFium redistribution, and Windows source-handle
transfer. Not run: packages, physical-device input, IME, Hibbeler, and the
matched performance qualification. The recovery Alert is currently an inline
development surface and still needs physical constrained-window visual review.
The next journey is the full Rectangle property inspector and save/reopen proof.

## Rectangle property-inspector journey (2026-08-26)

The experiment now has an application-owned, stable-identity Rectangle
property surface built from real GPUI Component `Accordion`, `Field`,
`Scrollable`, `Switch`, `ColorPicker`, `Slider`, `NumberInput`, and `Select`
primitives. It exposes the maintained Electron fields for lock, stroke color,
overall opacity, line width, Solid/Dashed/Dotted style, fill color and opacity,
X/Y/width/height, and rotation. Hatch is omitted because the maintained
Electron fields do not read, write, render, or persist it. Cloud is also
omitted because the Electron Rectangle path currently renders and exports that
choice as Solid. Those are Electron baseline defects, not native parity gaps.

Feature values remain in `NativeDocumentSession`; the retained inspector owns
only component state and emits document-ID plus annotation-ID bound patches.
Invalid numeric drafts do not mutate. Blur/Enter commits numeric values,
sliders commit on release, no-ops do not add history, lock suppresses mutation,
and stale selection or document events are rejected. The inspector is a fixed
300 px sibling of the viewport with its own vertical scroll owner. A red test
found and fixed an integration error that had placed it after the scrollable
PDF content rather than beside it.

Passed Linux development evidence: the rendered focused gate passes 1/1 in 21
seconds (`button-probe-20260826T021923Z-2003998.log`); the checksum-pinned real
100-page PDF journey passes 1/1 in 33 seconds after the final formatting pass
(`button-probe-20260826T022732Z-2012104.log`); the typed Rectangle model passes
1/1 in 11 seconds (`button-probe-20260826T022119Z-2006151.log`); PDF persistence
and the annotation adapter pass their focused gates; and the warm all-targets
gate passes 137 tests with six gated ignores in 30 seconds
(`button-probe-20260826T022819Z-2012960.log`). The real journey creates and
moves a Rectangle, applies every maintained inspector property, saves, proves
different PDFium annotation pixels, passes `qpdf` and `pdfinfo`, closes and
reaps the worker, then reopens in a fresh workspace with stable identity,
geometry tolerance, rotation, lock, fill, opacity, width, and Dotted style.

Source/guard policy passes 17/17. The prepared Longbridge/Zed digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one GPUI identity across 870 packages; configured dependency
policy passes with the reviewed upstream warnings. The build runner now pins a
16 MiB Rust test-thread stack because this large end-to-end GPUI test otherwise
exhausted the test harness default before entering the test body. Disk, target,
job, timeout, offline, and retention limits are unchanged.

Failed evidence is limited to retained red iterations for compile APIs, Switch
hit geometry, the misplaced panel, float tolerance, and the test-thread stack.
No accepted inspector gate remains failing. Blocked: fresh native pixels and a
live accessibility tree on this headless VPS, production PDFium redistribution,
and Windows inherited source-handle transfer. The pinned `ColorPicker` wrapper
also lacks a disabled API; the experiment visibly dims it and rejects events,
but physical proof that a locked picker cannot open is still required. Not run:
packaged Linux/macOS/Windows candidates, physical-device input, accessibility,
IME, Hibbeler, and the matched Electron/GPUI performance qualification.

## In-place Save journey (2026-08-26)

Opened regular PDFs now use an explicit `OpenedSource` save destination instead
of inferring replacement from path equality. The persistence boundary opens the
source without following a final symlink, binds SHA-256 plus stable Unix file
and parent identities, writes and syncs a same-directory stage, independently
reopens it through the real PDFium worker, preserves Unix mode, rechecks the
source immediately before atomic rename, and syncs the parent directory. A
publication receipt distinguishes durable success from the case where the new
file is already published but a post-publication durability operation warns.
The workspace keeps the published resource live and surfaces that warning.

Passed Linux development evidence: focused `document-save` passes 7/7 with one
separately gated real test ignored in 28 seconds
(`button-probe-20260826T030340Z-2034975.log`); `document-save-real` passes 1/1
in 3 seconds (`button-probe-20260826T030424Z-2035698.log`); and the final warm
all-targets gate passes 144 tests with seven gated ignores in 16 seconds
(`button-probe-20260826T030656Z-2037788.log`). The real 100-page journey edits a
Rectangle, activates the real Save button, preserves document identity/path,
page and selection, validates `qpdf` plus 100-page `pdfinfo`, proves inode
replacement and mode preservation, reaps the old worker, then closes and
reopens with clean typed state and no orphan resource. Guard/source policy is
17/17; the prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
the shared PDF engine receipt is
`5974f9088e3b37a523b3afbf0605e69302f0e9545b2da44e1b0f83bce7fce904`,
and the 870-package single-GPUI and configured dependency gates pass with the
reviewed upstream warnings.

Failed evidence is limited to retained red/diagnostic iterations for the
missing publication receipt, one stale field use, and an older integration test
that attempted an overlapping Save. No accepted Save gate remains failing.
Blocked: Windows in-place replacement/source-handle semantics, production
PDFium redistribution, native visual/live accessibility, and metadata policy
beyond Unix mode. Not run: packaged candidates, macOS/Windows physical-device
proof, IME, Hibbeler, and matched performance. The next cutover slice connects
in-place Save to dirty document and application close while generated documents
continue to require Save As.

## Dirty-document and application-close Save journey (2026-08-26)

Ordinary dirty tabs now route Save to the verified opened source; generated
documents retain the Save As branch. `ApplicationCloseWorkspace` freezes one
stable-ID snapshot, rejects duplicate close requests without replacing it, and
dispatches PDF writing on GPUI's background executor. Save All runs one save at
a time, then releases every frozen session before it emits a quit intent. Save
failure, stale completion, release failure, Save As cancellation, and a file
that was published with a directory-sync warning each stop quit with distinct
retained evidence. The warning path records the published document as already
saved so a retry cannot silently repeat it.

Passed Linux development evidence: the final pure coordinator gate passes
10/10 (`button-probe-20260826T033919Z-2059387.log`); integration passes 13/13
with one separately gated real test ignored
(`button-probe-20260826T033940Z-2059920.log`); and the checksum-pinned real
two-document journey passes 1/1
(`button-probe-20260826T033954Z-2060269.log`). The real GPUI Component alert
dialog accepts Enter as its default Save All action, publishes owned copies of
the 100-page and two-page fixtures in place, validates them with `qpdf` and
`pdfinfo`, reopens their typed Rectangles, releases both replacement workers and
mapped surfaces, closes the modal, and emits one quit intent. The warm
all-targets gate passes 150 tests with seven gated ignores in 34 seconds
(`button-probe-20260826T034013Z-2060623.log`). Source/guard policy is 17/17;
the prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
and the 870-package graph still contains one pinned Zed GPUI identity.

Failed evidence is retained TDD output only: missing async API/type imports, a
test lifetime error, unavailable nested-modal debug bounds, and the warning
accounting defect all failed before the accepted fixes. No accepted gate is
failing. The dependency policy passes with two missing-license-metadata and
five unmaintained-transitive warnings. Blocked: production PDFium
redistribution, Windows replacement/source-handle semantics, and live native
visual/accessibility evidence on this headless VPS. Not run: packaged
Linux/macOS/Windows candidates, physical-device input, IME, Hibbeler, and the
matched performance qualification. The next cutover seam is a native Save As
picker continuation for generated documents during dirty-tab and application
close; the current deterministic adapter still requires an external path
response.

## Generated Save As close journey (2026-08-26)

`ApplicationCloseWorkspace` now calls GPUI's pinned `prompt_for_new_path` API
for generated documents and binds the response to the frozen transaction token.
It permits one in-flight picker, validates a case-insensitive `.pdf` extension,
and separates cancellation from picker/platform failure. A stale response or a
document revision change cannot start a save or emit quit.

Passed Linux development evidence: pure 11/11
(`button-probe-20260826T035650Z-2075885.log`), integration 17/17 plus one gated
ignore (`button-probe-20260826T035444Z-2074339.log`), checksum-pinned real mixed
PDFium 1/1 (`button-probe-20260826T035633Z-2075516.log`), and warm all-targets
155 plus seven gated ignores in 34 seconds
(`button-probe-20260826T035700Z-2076199.log`). The real test saves two opened
PDFs and one generated `Untitled.pdf`, validates all three with `qpdf`,
`pdfinfo`, and independent typed reopen, removes the generated source, releases
all workers and mapped surfaces, closes the modal, and emits one quit intent.
Source/guard policy is 17/17; the prepared digest and 870-package single-GPUI
graph are unchanged; configured dependency checks have no denial.

Failed evidence is retained test-first and compile diagnostics only. Blocked:
the pinned picker API has no title, file filter, automatic extension, or window
owner; production PDFium, Windows replacement/source handles, and live native
visual/accessibility also remain blocked. Not run: packages, physical-device
input, IME, Hibbeler, and matched performance. The next cutover seam is
capability-bound save-target authority plus visible save/release failure
recovery.

## Save-target authority and close recovery (2026-08-26)

Generated-document Save As now crosses a move-only target authority rather
than carrying an ambient pathname into the PDF writer. On Unix, the authority
binds the absolute native `PathBuf`, canonical parent identity, retained parent
directory descriptor, and destination leaf before work starts. It permits one
stage only. Publication rechecks the parent, target absence, open staging-file
identity, and staging name before a no-overwrite link. Cleanup removes a stage
only when the name still identifies the inode created by this authority. A
competing target, replaced parent, renamed stage, non-UTF-8 `.pdf` leaf, direct
symlink parent, source alias, and repeated use all have deterministic coverage.

The application-close shell now renders a real GPUI Component `Alert` for
picker failure, rejected target, save failure, post-publication warning, and
resource-release failure. Each state preserves the frozen document identity
and exposes a typed recovery action. Retrying starts a fresh transaction; a
document already published with a warning is clean and is not written again.
The surface records intents only where a safe operation cannot yet be repeated
automatically.

Passed Linux development evidence: the authority gate passes 7/7
(`button-probe-20260826T043911Z-2101950.log`), application-close pure state
passes 11/11 (`button-probe-20260826T044012Z-2102996.log`), integration passes
19/19 with one separately gated real test ignored
(`button-probe-20260826T043956Z-2102669.log`), and the real mixed-document
PDFium close journey passes 1/1
(`button-probe-20260826T044026Z-2103315.log`). The final warm all-targets gate
passes 163 active tests with seven explicit real-fixture ignores in 15 seconds
(`button-probe-20260826T044419Z-2107051.log`). Source and guard policy passes
17/17; the prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph has one pinned Zed GPUI identity across 870 packages; and the
configured dependency audit has no denial. The retained target is about 3.0
GiB and the host had about 102 GiB free.

Failed evidence is limited to retained red tests that exposed the staging-name
cleanup race and three stale relative-path/test-name assumptions. No accepted
gate is failing. Blocked: Windows target-handle authority, production PDFium
redistribution, and native visual/live-accessibility proof on this headless
host. Not run: packaged Linux, macOS, or Windows candidates; physical-device
input; IME; Hibbeler; and the matched Electron/GPUI performance qualification.
The next cutover work must exercise a complete runnable application journey,
not return to isolated control parity.

## Native multi-document ingress journey (2026-08-26)

`DocumentWorkspace` now owns one typed batch-open command for native picker,
menu, system, and drop origins. The real picker and Command-or-Control+O feed
that seam. Normal `component_story` startup also submits all reviewed launch
paths as one system batch instead of calling the low-level single-open method
once per argument. Positional and explicit `--open` PDFs resolve against the
launch working directory; unrelated flags and non-PDF arguments are ignored.
Native `PathBuf` bytes are never round-tripped through UTF-8.

The coordinator opens candidates sequentially, focuses an existing ordinary
session without resetting its page, permits a drop-origin duplicate, selects
the first successful new document, removes a failed transient session, and
retains every ordered failure after later successes. A shallow real GPUI
Component `Alert` plus labeled `Dismiss` Button presents failure feedback
without interrupting the surviving document. Dismiss clears feedback only; it
does not alter the batch result or any document resource.

Passed Linux development evidence: focused native-open 5/5
(`button-probe-20260826T052644Z-2132976.log`), native launch 4/4
(`button-probe-20260826T052511Z-2131689.log`), warm all-targets 174 active plus
seven explicit real-fixture ignores in 32 seconds
(`button-probe-20260826T052728Z-2133554.log`), and exact story/worker build in
14 seconds (`button-probe-20260826T052814Z-2134534.log`). Guard/source policy
passes 17/17; the prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the 870-package graph contains one pinned Zed GPUI identity; and configured
dependency policy has no denial.

Failed evidence is retained red TDD output for the missing interface, Alert
selector, ordered-failure collection, and Control+O binding before their fixes.
No accepted gate is failing. Blocked: real native visual/live-accessibility
proof on this headless VPS, production PDFium redistribution, and Windows save
authority. Not run: native application-menu, drag/drop, macOS `open-file`,
second-instance delivery, packaged platforms, physical input, IME, Hibbeler,
and the matched Electron/GPUI performance qualification. The next cutover
journey connects the already-real annotation edit and save/reopen path through
this exact runnable shell and records complete resource ownership.

## Native application ingress and menu boundary (2026-08-26)

The runnable story now owns one asynchronous document-ingress channel before
`Application::run`. Startup paths and macOS `application:openURLs:` delivery
join the existing `DocumentWorkspace` system-open batch. Only local `file:`
URLs are accepted; percent decoding rejects malformed input, NUL, remote hosts,
unsupported schemes, and non-PDF paths. Separate GPUI `ExternalPaths` drops
reach the top-level `ApplicationCloseShell` and retain the Electron force-new
duplicate policy.

One application menu model now drives both surfaces supported by the pinned
stack: GPUI installs the real operating-system menu on macOS, while the real
GPUI Component `AppMenuBar` renders the same owned menu on Linux and Windows.
Open, Save, and Save As route to the retained document owner. Close Window and
Quit route to the application-close transaction and cannot call raw quit before
dirty documents are resolved and resources are released. Save enablement tracks
active-document and save-busy state.

Passed Linux development evidence: focused native-application 6/6
(`button-probe-20260826T054823Z-2156256.log`), exact story/worker build in 15
seconds (`button-probe-20260826T054911Z-2156866.log`), and warm all-targets 180
active plus seven separately gated real-fixture ignores in 52 seconds
(`button-probe-20260826T055039Z-2157848.log`). Source/guard policy passes 17/17;
the prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the 870-package graph has one pinned GPUI identity; and configured dependency
policy has no denial. The direct `async-channel` edge uses the already-resolved
2.5.0 package and does not change the package set.

Failed evidence is retained red TDD/compile output only: the missing seam,
unsupported receiver iterator, missing test worker PID, and initially
focus-dependent close action all failed before the accepted fixes. No accepted
gate is failing. Blocked: production PDFium redistribution, Windows save-target
authority, and live native visual/accessibility proof on this headless host.
Not run: physical macOS Open With/native-menu delivery, Linux/Windows
single-instance IPC and package associations, packaged candidates, physical
input, IME, Hibbeler, and matched Electron/GPUI performance. The pinned Linux
and Windows GPUI backends store `on_open_urls` callbacks but never invoke them;
their second-instance path therefore requires an application-owned local IPC
adapter rather than an upstream fork.

## Native-shell Rectangle save/close/reopen transaction

The fixed runner mode `native-shell-rectangle-real` proves a single real
application journey through the GPUI Component root and dirty-close surface.
The test opens the checksum-controlled 100-page PDF, creates a Rectangle,
dispatches application close, accepts Save, validates the persisted file with
`qpdf`, confirms the first worker and surfaces are released, opens a distinct
native window, hydrates the Rectangle through a fresh PDFium worker, and closes
that document without an orphan worker or mapped surface.

Passed: focused real journey 1/1 in seven seconds
(`.prepared/evidence/button-probe-20260826T060214Z-2164273.log`), build guard
13/13, and warm all-targets 180 active plus eight gated ignores in 15 seconds
(`.prepared/evidence/button-probe-20260826T060239Z-2164640.log`). Failed:
retained red diagnostics only; no accepted gate remains failing. Blocked:
shipping PDFium, Windows save-target authority, and live native visual or
accessibility proof. Not run: packages, physical platforms, IME, Hibbeler, and
the matched performance decision. This is Linux development-only evidence.

## Ordinary Save and Save As recovery transaction

`DocumentWorkspace` now owns typed ordinary-save failure state. The rendered
surface is a real GPUI Component `Alert` composed with real `Button` actions;
PDF pixels, annotation state, path authority, and worker ownership remain
application-owned. Save As target-exists and source-replacement errors use
stable product copy and never offer a same-target retry.

The fixed `document-save-collision-real` runner proves an occupied destination
is not modified, the dirty source session and original worker remain live, a
fresh target can be selected from the recovery action, the new PDF passes
`qpdf`, the Rectangle reopens, and old/new workers and mapped surfaces are
released. Passed: real 1/1 in 25 seconds
(`.prepared/evidence/button-probe-20260826T062254Z-2177145.log`), focused
ordinary recovery in four seconds
(`.prepared/evidence/button-probe-20260826T062331Z-2177744.log`), all-targets
181 active plus nine gated ignores in 34 seconds
(`.prepared/evidence/button-probe-20260826T062346Z-2178041.log`), and
source/guard 17/17. Failed: the retained red run
(`.prepared/evidence/button-probe-20260826T062027Z-2174478.log`) identified the
missing typed collision state and is fixed. Blocked: shipping PDFium, Windows
save-target authority, and live visual/accessibility. Not run: packages,
physical platforms, IME, Hibbeler, and matched performance.

## Controlled real-document viewer journey

`DocumentWorkspace` now renders one real GPUI Component viewer toolbar inside
the retained native document shell. `NativeDocumentSession.view_state` remains
the only document authority. `ViewerToolbarStrip`, its paired real
`DropdownButton`s, fit `ButtonGroup`, zoom `Button`s, `Popover`, and
`PopupMenu` are controlled presentation and emit semantic user intent only.
Silent synchronization runs from stored active-session observers and explicit
tab activation or close seams; it never mutates child entities during render.
The runnable story's older second toolbar and polling counters were removed,
which also removes the duplicate window-keyed zoom-menu cache identity. CAD
View remains a separate compatibility control until CAD state has a real
per-document owner.

The fixed `viewer-state-real` runner opens two copies of the checksum-controlled
100-page fixture with separate real PDFium workers. Through rendered controls
it selects Single Page, Fit Page, 400%, thumbnail page 2, and a measured scroll
for the first document; then Continuous, Fit Width, 1600%, page 2, and an
independent scroll for the second. It renders real tile pixels at both zooms,
keeps each plan within 32 tiles and 256 MiB, rejects a stale 400% plan after the
1600% generation becomes authoritative, switches back through the real TabBar,
and proves worker exit and mapped-surface cleanup for both documents.

Passed Linux development evidence: the accepted real journey passes 1/1 in 24
seconds (`.prepared/evidence/button-probe-20260826T065400Z-2196719.log`); warm
all-targets passes 181 active tests plus ten explicit development-fixture
ignores in 34 seconds
(`.prepared/evidence/button-probe-20260826T065440Z-2197341.log`); source and
guard policy passes 17/17; the prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; and configured
dependency policy has no denial. Failed evidence is retained red-first output
for the missing toolbar, an incorrect evidence-field assertion, and an
over-broad viewport-size equality; all are fixed and no accepted gate fails.
Blocked: production PDFium redistribution, Windows save-target authority, and
fresh native visual/live-accessibility proof on this headless VPS. Not run:
packaged candidates, physical macOS/Windows input, IME, Hibbeler, sustained
cache pressure, and the matched Electron/GPUI performance qualification.

## Real-session Document Tab Bar journey

`DocumentWorkspace` now applies the previously isolated tab contract to real
stable-ID `NativeDocumentSession` entities. The visible path uses the pinned
GPUI Component `TabBar`, `Tab`, `Button`, and `Popover` primitives. Butter
Paper owns document order, stable focus and bounds, the greater-than-six-pixel
pointer adapter, and polite reorder state. Reordering moves the session entity;
view state, annotations, dirty state, pending close identity, and resources stay
attached to the stable `DocumentId`.

Passed Linux development evidence: the focused rendered journey passes 2/2 in
31 seconds
(`.prepared/evidence/button-probe-20260826T072211Z-2214915.log`). It proves
loading and failed tab retention, close-origin drag isolation, pointer
selection, Alt+Shift reorder, exact six-pixel no-drag, seven-pixel pointer
reorder, dirty Cancel and Discard, successor focus, stable annotation identity,
and complete resource release. Warm all-targets passes 183 active tests plus ten
gated ignores in 47 seconds
(`.prepared/evidence/button-probe-20260826T072301Z-2216384.log`). Source and
guard policy passes 17/17; the prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; dependency
policy has no denial; and host storage remains green. Failed evidence is
retained red-first output only; no accepted gate fails. Blocked: pointer edge
auto-scroll, persisted order, production PDFium, Windows save-target authority,
and fresh native visual/live-accessibility proof. Not run: packages, physical
macOS/Windows input, IME, Hibbeler, and matched performance.

## Polyline and Polygon create/edit/save/reopen journey

`DocumentWorkspace` now renders real pinned GPUI Component Polyline and Polygon
tool Buttons over application-owned retained annotation state. The document
canvas alone uses raw GPUI because PDF pixels, path painting, hit testing, and
pointer geometry are domain rendering rather than standard controls. A valid
vertex path commits on Enter or Escape; a too-short draft is discarded. A
Polygon closes when the pointer returns within the Electron-defined start
radius without duplicating the first vertex.

The fixed real journey opens the checksum-controlled 100-page fixture, creates
both paths through rendered controls and real pointer input, edits a Polyline
vertex, saves through the guarded Save As seam, validates canonical native
annotation identity and appearance pixels, closes cleanly, and hydrates both
stable paths in a distinct workspace. It proves both PDF worker PIDs exit and
all mapped surfaces are released.

Passed Linux development evidence: rendered workspace 1/1
(`.prepared/evidence/button-probe-20260826T083207Z-2275902.log`), adapter 32/32
(`.prepared/evidence/button-probe-20260826T083251Z-2276897.log`), real PDFium
1/1 (`.prepared/evidence/button-probe-20260826T084142Z-2284003.log`), and warm
all-targets 184 active plus eleven gated ignores in 64 seconds
(`.prepared/evidence/button-probe-20260826T083819Z-2281603.log`). The build
guard remains 13/13, source/guard policy 17/17, prepared digest and
870-package single-GPUI graph are unchanged, configured dependency policy has
no denial, and the host has about 102 GiB free.

Failed evidence is retained red-first diagnostic history only; no accepted
gate fails. Blocked: production PDFium redistribution, Windows target
authority, and live native visual/accessibility proof on this headless host.
Not run: packages, physical macOS/Windows input, IME, Hibbeler, and the final
matched Electron/GPUI performance qualification.

## Frozen Polylength and Area contract

The public seams under test are the application-owned annotation model and
adapter, the rendered `DocumentWorkspace`, the PDF persistence boundary, and a
fresh-workspace reopen. Standard visible controls must be real GPUI Component
Buttons with stable `tool-polylength` and `tool-area` identities. Raw GPUI may
own only document and annotation pixels, hit testing, and pointer geometry.

- Polylength uses Shift+Alt+Q and Area uses Shift+Alt+A. A page scale is a hard
  placement precondition. Missing scale reports the existing tool error and
  leaves no draft or annotation.
- Each committed point must be at least 0.5 PDF pt from the prior point.
  Polylength requires two points; Area requires three. Enter commits a valid
  path, an insufficient Enter leaves the draft active, double-click adds the
  final point and commits, and Escape cancels without creating an annotation.
- Polylength is open. Area is closed and selectable by its interior, edge, or
  caption. Body movement translates the full path; stable vertex handles edit
  one point. Selection, locking, no-op suppression, undo, and redo retain their
  existing application-owned semantics.
- Captions derive from the page scale, unit, and precision. Defaults are red
  stroke and text, 1 pt stroke, 12 pt text, and opacity 1.
- Persistence must keep the two measurement identities distinct from ordinary
  Polyline and Polygon: `/PolyLine` plus `/IT /PolyLineDimension` and
  `Polylength Measurement`; `/Polygon` plus `/IT /PolygonDimension` and
  `Area Measurement`. Both require stable `/NM`, `/Vertices`, `/Measure`, and
  a renderable appearance stream.

## Polylength and Area create/edit/save/reopen journey

`DocumentWorkspace` now carries both calibrated multi-point measurement tools
through the real pinned GPUI Component shell. The standard visible controls are
real `Button`s with stable `tool-polylength` and `tool-area` identities. The
application-owned session retains calibration, drafts, stable annotation IDs,
selection, history, dirty state, and save/resource ownership. Raw GPUI remains
limited to PDF and annotation pixels, hit testing, pointer geometry, and
caption painting.

The checksum-pinned real journey opens the public 100-page PDF, applies a page
scale through the retained workspace seam, creates Polylength with double-click
completion and Area with Enter, edits one Polylength vertex, saves through the
guarded Save As path, validates canonical `/PolyLine`/`Polygon` measurement
identities and real PDFium appearance pixels, closes, and hydrates both stable
measurements in a distinct workspace. It also proves the original, replacement,
independent-oracle, and reopened workers exit and the mapped-surface root is
empty.

Passed Linux development evidence: real PDFium 1/1
(`.prepared/evidence/button-probe-20260826T094541Z-2318142.log`), measurement
model 1/1 (`.prepared/evidence/button-probe-20260826T094623Z-2318657.log`),
adapter 34/34 (`.prepared/evidence/button-probe-20260826T094636Z-2319016.log`),
persistence 18/18
(`.prepared/evidence/button-probe-20260826T094654Z-2319446.log`), page-scale
interaction 5/5
(`.prepared/evidence/button-probe-20260826T094926Z-2322117.log`), and warm
all-targets 185 active plus twelve explicit gated ignores in 49 seconds
(`.prepared/evidence/button-probe-20260826T095332Z-2325829.log`). Source and
guard policy passes 17/17; the prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; dependency
policy has no denial; and host storage remains green.

Failed evidence is retained red-first diagnostic history. The first complete
gate exposed two page-scale tests that clicked a correctly rendered but
horizontally clipped trigger after the toolbar gained two controls. The focused
red loop proved that scrolling the stable trigger into its sole scroll owner
restored the real interaction; no product control or state change was needed.
No accepted gate is failing. Blocked: production PDFium redistribution,
Windows save-target authority, and live native visual/accessibility proof on
this headless host. Not run: packaged candidates, physical macOS/Windows input,
IME, Hibbeler, and the matched Electron/GPUI performance qualification. The
story compiles but was not launched, so this remains Linux development-only,
not packaged or physical-device evidence.

## Frozen Cloud contract

The public seams under test are the application-owned annotation model and
adapter, the rendered `DocumentWorkspace`, the PDF persistence boundary, and a
fresh-workspace reopen. The standard visible control is a real pinned GPUI
Component `Button` with stable `tool-cloud` identity. Raw GPUI owns only PDF
and annotation pixels, hit testing, pointer geometry, and control-handle paint.

- The tool uses the unmodified C shortcut. Click placement retains nodes until
  the pointer closes within ten screen pixels of the first node, double-click,
  Enter, or Escape. Enter and Escape commit at least three nodes and otherwise
  cancel. The click-node seam is implemented; Electron's three-pixel
  rectangle-drag creation mode remains explicit follow-up work.
- A Cloud is a separate annotation family, not a Rectangle style or ordinary
  Polygon. Its defaults are red one-point stroke, intensity 2, no fill, and
  opacity 1. Stable vertex identity, whole-body translation, selection, lock,
  no-op suppression, undo, and redo stay application-owned.
- Persistence requires `/Polygon`, `/IT /PolygonCloud`,
  `/BE << /S /C /I 2 >>`, `/Subj (Cloud)`, stable `/NM`, `/Vertices`, and a
  renderable `/AP /N`. Cloud classification must run before ordinary Polygon
  import so neither family absorbs the other.
- The current scallop generator is deterministic and transparent, but it is a
  sampled approximation. It is not claimed as exact Electron/Bluebeam cubic
  parity. Exact geometry remains a cutover gap.

## Cloud create/edit/save/reopen journey

`DocumentWorkspace` now carries a retained Cloud through the real pinned GPUI
Component shell. The rendered Button selects the tool; application state owns
the draft, stable annotation ID, selection, edit/history state, save
reconciliation, and worker resources. The deterministic workspace journey
creates a valid click-node Cloud, commits it, proves scalloped geometry, edits
one stable vertex, and keeps unrelated toolbar and annotation state unchanged.

The checksum-pinned real journey opens the public 100-page fixture, creates and
edits a Cloud, saves through the guarded Save As seam, validates the canonical
native dictionary and an independent typed reopen, proves real PDFium pixels
changed, passes `qpdf`, closes cleanly, and hydrates the same stable identity in
a distinct workspace. It proves the original, replacement, independent-oracle,
and reopened workers exit and the mapped-surface root is empty.

Passed Linux development evidence: model 1/1
(`.prepared/evidence/button-probe-20260826T100534Z-2332753.log`), persistence
18/18 (`.prepared/evidence/button-probe-20260826T101050Z-2335767.log`), adapter
35/35 (`.prepared/evidence/button-probe-20260826T102950Z-2346504.log`), rendered
workspace 1/1 (`.prepared/evidence/button-probe-20260826T103023Z-2347054.log`),
real PDFium 1/1
(`.prepared/evidence/button-probe-20260826T103109Z-2347805.log`), and warm
all-targets 186 active plus thirteen explicit gated ignores in 41 seconds
(`.prepared/evidence/button-probe-20260826T103128Z-2348162.log`). Build-guard
policy passes 13/13 and source policy passes 4/4. The prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; configured
dependency policy has no denial; and host storage remains green.

Failed evidence is retained red-first diagnostic history, including the test
that found Cloud missing from the shared lock path. The omission is fixed and
no accepted gate fails. Partial gaps are rectangle-drag creation, live pointer
vertex/body editing in the workspace, exact cubic scallop parity, and the
intensity/style property surface. Blocked: production PDFium redistribution,
Windows save-target authority, and live native visual/accessibility proof on
this headless host. Not run: packaged candidates, physical macOS/Windows input,
IME, Hibbeler, a third-party Cloud corpus, and matched Electron/GPUI
performance. The story compiles but was not launched. This is Linux
development-only evidence, not packaged or physical-device acceptance.

## Callout create/edit/save/reopen journey

`DocumentWorkspace` now composes the maintained Line and Text Box domains into
one application-owned Callout while using real pinned GPUI Component `Button`
and `Textarea` primitives for the standard visible controls. Two pointer
clicks retain the leader tip, knee, connection point, and text rectangle under
one stable Callout ID. The editor selects the default text, accepts multiline
input, commits on plain Enter, and merges creation plus initial text into one
undo step. Raw GPUI remains confined to the leader, arrow, text, selection
outline, handles, PDF pixels, hit testing, and document geometry.

The checksum-pinned real journey opens the public 100-page fixture, creates a
Callout through the rendered control, enters `field\nnote`, moves the text box,
edits the knee, saves through the guarded Save As seam, and independently
reopens the same logical annotation. The typed parser preserves one native
Callout identity without misclassifying it as Cloud or ordinary Text Box.
PDFium annotation pixels differ from the annotation-free page, `qpdf` accepts
the result, every original/replacement/oracle/reopen worker exits, and the
mapped-surface root is empty. The appearance writer uses coordinates local to
its Form `/BBox`; the red real-PDF test caught and fixed the prior clipped
absolute-coordinate stream.

Passed Linux development evidence: rendered workspace 1/1
(`.prepared/evidence/button-probe-20260826T111401Z-2370982.log`), persistence
focused green (`.prepared/evidence/button-probe-20260826T111615Z-2372567.log`),
real PDFium 1/1
(`.prepared/evidence/button-probe-20260826T111642Z-2373049.log`), Length
scroll-regression 1/1
(`.prepared/evidence/button-probe-20260826T112819Z-2380348.log`), and warm
all-targets 187 active plus fourteen explicit gated ignores in 33 seconds
(`.prepared/evidence/button-probe-20260826T112905Z-2380861.log`). Source and
guard policy pass 17/17. The prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; dependency
policy has no denial; and host storage is green.

Failed evidence is retained red-first diagnostic history. The final broad gate
also exposed an old Length test clicking its off-screen bounds after Callout
expanded the sole horizontal toolbar. The focused red loop proved that the
stale coordinate overlapped the fixed Rotate Left control; scrolling the real
Length target into view restores the intended user interaction and proves
non-overlap. No accepted gate remains failing. Partial gaps are existing-
Callout double-click editing, full live pointer handle/body manipulation,
exact Electron font wrapping and rich-text policy, property controls, and
Cloud+. Blocked: production PDFium redistribution, Windows save-target
authority, and fresh native visual/live-accessibility proof. Not run: packaged
candidates, physical macOS/Windows input, IME, Hibbeler, Callout corpus breadth,
and matched Electron/GPUI performance. The exact story compiles but was not
launched on this headless host. This remains development-only evidence.

## Cloud+ create/edit/save/reopen journey

`DocumentWorkspace` now carries one logical Cloud+ aggregate through the exact
pinned GPUI Component shell. Real GPUI Component `Button` and `Textarea`
primitives own the visible tool and editor. Application-owned retained state
owns the Cloud+ identity, cloud path, optional three-point leader, text box,
multiline content, selection, history, dirty state, PDF reconciliation, and
resource lifetime. Raw GPUI remains limited to document and annotation paint,
hit testing, pointer geometry, and selection handles.

The editor now follows the frozen Electron multiline rule. Four 12-point lines
grow the 44-point box to 67.2 points, preserve its center, reroute the leader,
and keep creation plus initial text as one undo step. Persistence exports one
logical object as an adjacent PolygonCloud and FreeTextCallout pair with stable
`:cloud` and `:text` identities. Reopen recognizes the pair independent of
physical order and quarantines incomplete fragments instead of importing a
standalone Cloud or Callout.

Passed Linux development evidence: Cloud+ model 1/1
(`.prepared/evidence/button-probe-20260826T114607Z-2396280.log`), routing 1/1
(`.prepared/evidence/button-probe-20260826T115112Z-2399225.log`), adapter
focused green
(`.prepared/evidence/button-probe-20260826T115612Z-2401763.log`), paired PDF
persistence 4/4
(`.prepared/evidence/button-probe-20260826T123825Z-2423002.log`), rendered
multiline workspace 1/1
(`.prepared/evidence/button-probe-20260826T123146Z-2418988.log`), checksum-
pinned real PDFium Save As/close/fresh-workspace reopen 1/1
(`.prepared/evidence/button-probe-20260826T123846Z-2423425.log`), and warm
all-targets 188 active plus fifteen explicit gated ignores in 22 seconds
(`.prepared/evidence/button-probe-20260826T124217Z-2426397.log`). The real
journey also passes `qpdf`, proves changed annotation pixels, and proves every
original, replacement, independent-oracle, and reopened worker exits with no
mapped surface left behind.

Source/guard policy passes 17/17. The prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; configured
dependency policy has no denial; and host storage is green. Failed evidence is
retained red-first diagnostic history for missing multiline growth, overly
strict PDF-number validation, and an old Image test that clicked a newly
off-screen toolbar target. All three defects are fixed and no accepted gate
fails.

Partial gaps remain: existing-object Cloud+ hit selection and live pointer
body/vertex/text-box/leader manipulation, eight text-box resize handles, page-
and obstacle-aware routing in the workspace adapter, exact Electron scallop
geometry, complete properties, per-markup editor accessibility identity, and
native IME/clipboard behavior. Blocked: production PDFium redistribution,
Windows save-target authority, and fresh native visual/live-accessibility proof
on this headless host. Not run: packaged candidates, physical macOS/Windows
input, Hibbeler or third-party Cloud+ corpora, and matched Electron/GPUI
performance. The story compiles but was not launched. This is development-only
evidence and does not authorize cutover or release.

## Dimension create/edit/save/reopen journey

`DocumentWorkspace` now carries an uncalibrated Dimension through the exact
pinned GPUI Component shell. Real GPUI Component `Button` and `Textarea`
primitives own the visible tool and caption editor. Application-owned retained
state owns the stable Dimension identity, two-click placement, caption, signed
offset, selection, history, dirty state, PDF reconciliation, and resource
lifetime. Raw GPUI is restricted to PDF and annotation painting, hit testing,
pointer geometry, arrowheads, extension lines, captions, and selection handles.

The PDF writer emits canonical `/Subtype /Line`, `/IT /LineDimension`,
`/Subj (Dimension)`, `/L`, `/LE [ClosedArrow ClosedArrow]`, `/LL`, `/LLE 4`,
`/Cap true`, `/Contents`, `/AP`, and stable `/NM` fields without `/Measure`.
Import preserves the distinction: a LineDimension with `/Measure` is Length;
an unmeasured LineDimension is Dimension.

Passed Linux development evidence: focused adapter 1/1
(`.prepared/evidence/button-probe-20260826T130821Z-2444042.log`), rendered
workspace 1/1
(`.prepared/evidence/button-probe-20260826T131547Z-2449298.log`), focused
persistence 2/2
(`.prepared/evidence/button-probe-20260826T132309Z-2454651.log`), checksum-
pinned real PDFium Save As/close/fresh-workspace reopen 1/1
(`.prepared/evidence/button-probe-20260826T132649Z-2458213.log`), and warm
all-targets 189 active plus sixteen explicit gated ignores in 27 seconds
(`.prepared/evidence/button-probe-20260826T133014Z-2460821.log`). The real
journey proves changed annotation pixels, accepted clean revision, every worker
exit, and no mapped surface left behind.

Source/guard policy passes 17/17. The prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; configured
dependency policy has no denial; and host storage is green. Failed evidence is
retained red-first diagnostic history for missing APIs, a missing offset
wrapper, and two old Text Box tests that clicked an off-screen toolbar target.
All are fixed and no accepted gate fails.

Partial gaps remain: workspace pointer proof for all endpoint/body/offset
edits, complete property controls, dimension-increment snapping, exact native
visual acceptance, and imported corpus breadth. Blocked: production PDFium
redistribution, Windows save-target authority, and fresh native visual/live-
accessibility proof on this headless host. Not run: packaged candidates,
physical macOS/Windows input, IME, Hibbeler or third-party Dimension corpora,
and matched Electron/GPUI performance. The exact story compiles but was not
launched. This is development-only evidence and does not authorize cutover or
release.

## Arc create/edit/save/reopen journey

`DocumentWorkspace` now carries Arc through the exact pinned GPUI Component
shell. A real GPUI Component `Button` owns the visible `tool-arc` control and
Shift+C action. Application-owned retained state owns the stable Arc identity,
three-click placement, 64-segment sampled path, start/mid/end controls,
translation, selection, history, dirty state, PDF reconciliation, and resource
lifetime. Raw GPUI is restricted to PDF and annotation painting, curved hit
testing, pointer geometry, and selection handles.

The PDF writer emits `/Subtype /Circle`, `/IT /CircleArc`, `/Subj (Arc)`,
`/Rect`, `/Angle1`, `/Angle2`, `/RD [0.5 0.5 0.5 0.5]`, `/C`, `/Border`,
`/CA`, `/ca`, `/AP /N`, `/BPAppearance`, flags, and canonical `/NM`. Its
appearance stream uses bounded cubic segments of at most 22.5 degrees. Import
classifies CircleArc before ordinary Ellipse and derives the three control
points from the native rect and angles.

Passed Linux development evidence: model 1/1
(`.prepared/evidence/button-probe-20260826T142230Z-2486828.log`), adapter
focused green
(`.prepared/evidence/button-probe-20260826T142245Z-2487137.log`), persistence
focused green
(`.prepared/evidence/button-probe-20260826T142300Z-2487609.log`), rendered
workspace 1/1
(`.prepared/evidence/button-probe-20260826T142457Z-2489563.log`), checksum-
pinned real PDFium Save As/close/fresh-workspace reopen 1/1
(`.prepared/evidence/button-probe-20260826T142615Z-2490982.log`), and warm
all-targets 190 active plus seventeen explicit gated ignores in 33 seconds
(`.prepared/evidence/button-probe-20260826T142758Z-2492765.log`). The real
journey proves changed annotation pixels, accepted clean revision, every worker
exit, and no mapped surface left behind. The persistence test independently
validates create/edit/delete output with `qpdf`.

Source/guard policy passes 17/17. The prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; configured
dependency policy has no denial; and host storage is green. Failed evidence is
retained red-first diagnostic history for the missing model APIs, a duplicate
degree helper, one missed exhaustive match, binary PDF-number assertions, and
an old Pen test that clicked an off-screen toolbar target. All are fixed and no
accepted gate fails.

Partial gaps remain: complete live pointer body/start/end/mid edit proof,
appearance property controls, an explicit policy for malformed or non-square
third-party CircleArc dictionaries, and exact native visual/accessibility
acceptance. Blocked: production PDFium redistribution, Windows save-target
authority, and fresh native visual/live-accessibility proof on this headless
host. Not run: packaged candidates, physical macOS/Windows input, IME,
Hibbeler or third-party Arc corpora, and matched Electron/GPUI performance. The
exact story compiles but was not launched. This is development-only evidence
and does not authorize cutover or release.

## Pending Redact create/edit/save/reopen journey

`DocumentWorkspace` now carries a pending Redact mark through the exact pinned
GPUI Component shell. A real GPUI Component `Button` owns the tool control and
a real warning `Alert` states that the mark does not securely remove text or
graphics. Application-owned retained state owns the stable identity, pointer
draft, body move, eight-handle resize, lock/history state, dirty state, PDF
reconciliation, and resource lifetime. Raw GPUI is limited to PDF/annotation
paint, hit testing, pointer geometry, and handles.

The writer emits canonical `/Subtype /Redact`, `/Rect`, `/QuadPoints`, `/IC`,
optional `/OverlayText`, flags, metadata, and stable `/NM`, while deliberately
omitting `/AP`. Only canonical Butter Paper `bp:` marks hydrate as editable;
external and noncanonical Redacts remain opaque and untouched. The real
fixture journey proves Save As, typed independent reopen, deletion from an
experiment copy, unchanged annotation-disabled page pixels, `qpdf`, worker
exit, and mapped-surface cleanup. It never invokes Apply Redactions.

Passed Linux development evidence: deterministic workspace 1/1
(`.prepared/evidence/button-probe-20260826T151308Z-2525858.log`), real PDFium
1/1 (`.prepared/evidence/button-probe-20260826T152029Z-2531291.log`), focused
model/adapter/persistence gates, and warm all-targets 191 active plus eighteen
gated ignores in 74 seconds
(`.prepared/evidence/button-probe-20260826T152132Z-2532416.log`). Source/guard
policy passes 17/17. The prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; configured
dependency policy has no denial; and host storage is green.

Retained red evidence exposed and then fixed exact PDF edge quantization at the
persistence boundary. No accepted gate fails. Apply Redactions, content
destruction, sanitization, and flattening are explicitly blocked pending a new
product/security decision. Production PDFium redistribution and Windows save
authority remain blocked. Native visuals/accessibility, packages, physical
platform input, IME, Hibbeler, third-party corpus coverage, and matched
performance were not run. This is development-only evidence.

## Snapshot capture/edit/save/reopen journey

`DocumentWorkspace` now carries Snapshot through the exact pinned GPUI
Component shell. A real GPUI Component `Button` owns the visible
`tool-snapshot` control and G action. Application-owned retained state owns the
strict two-click draft, stable identity, synchronous page capture, body move,
eight-handle resize, rotation/opacity values, lock/history state, dirty state,
PDF reconciliation, and resource lifetime. Raw GPUI is restricted to PDF and
annotation pixels, hit testing, pointer geometry, selection chrome, and the
base-raster crop adapter.

The capture reads the current annotation-free displayed page raster at the
second click, maps the PDF rectangle with the active page rotation, rounds and
clamps the pixel crop, and converts premultiplied BGRA to canonical RGBA. The
writer emits canonical `/Subtype /Stamp`, `/IT /StampSnapshot`, `/Subj
(Snapshot)`, empty Contents, flags, opacity, optional Rotation, stable `bp:`
NM, and a Form/Image/Gray-SMask appearance graph. Import hydrates only
checksum-identical canonical Butter Paper Snapshots. External or malformed
Stamps remain untouched.

Passed Linux development evidence: model 3/3
(`.prepared/evidence/button-probe-20260826T155301Z-2561017.log`), adapter 2/2
(`.prepared/evidence/button-probe-20260826T155328Z-2561619.log`), persistence
1/1 (`.prepared/evidence/button-probe-20260826T155341Z-2562007.log`), rendered
workspace 1/1 (`.prepared/evidence/button-probe-20260826T155729Z-2565436.log`),
checksum-pinned real PDFium capture/Save As/close/fresh-workspace reopen/delete
1/1 (`.prepared/evidence/button-probe-20260826T155833Z-2566369.log`), and warm
all-targets 192 active plus nineteen explicit gated ignores in 39 seconds
(`.prepared/evidence/button-probe-20260826T155909Z-2567089.log`). The real
journey proves nonuniform captured pixels, unchanged annotation-disabled page
pixels, changed annotation-enabled pixels after move/resize, source checksum
preservation, canonical typed reopen, worker exit, and no mapped surface left
behind.

Source/guard policy passes 17/17. The prepared digest remains
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph remains one pinned GPUI identity across 870 packages; dependency
policy has no denial and retains two missing-license-metadata plus five
unmaintained-transitive warnings; and host storage is green with about 101 GiB
free. Retained red evidence caught two adapter integration omissions, 21 old
Snapshot-literal exhaustiveness sites, and an initially blank crop region.
Each is fixed and no accepted gate fails.

Partial gaps remain: a live rotation-handle gesture and visible opacity
controls, rotated/cropped-page corpus breadth, exact fallback behavior when a
base raster is unavailable, private/vendor Snapshot payload compatibility,
hostile appearance graphs, and fresh native visual/accessibility acceptance.
Blocked: production PDFium redistribution, Windows save-target authority, and
live native visual/accessibility evidence on this headless VPS. Not run:
packaged candidates, physical macOS/Windows input, Hibbeler or third-party
Snapshot corpora, and matched Electron/GPUI performance. This is
development-only evidence and does not authorize cutover or release.

## Semantic snapping Line and Length journey

The compatibility workspace now retains application-owned
`SemanticSnapSettings`, a shared GPUI-free `SemanticSnapIndex`, and one
transient `SemanticSnapDecision`. The visible settings use real pinned GPUI
Component `Button`, `Popover`, and `Checkbox` primitives. The canvas guide is a
domain rendering exception: raw GPUI paints its square, triangle, cross, X, or
diamond geometry through the live page transform.

The engine matches the frozen Electron contract for an inclusive eight-window-
pixel Euclidean tolerance, owner exclusion, source/target toggles, and
Shift-first orthogonal filtering. It currently indexes Rectangle, straight
Line/Arrow, Dimension, and Length sources. The rendered workspace proves Line
and Length pointer creation, stable settings and guide IDs, no preview history,
one revision per commit, and guide cleanup. The real 100-page fixture journey
then proves Save As, typed fresh-workspace reopen, effective Line/Length
geometry, changed PDFium pixels, worker exit, and mapped-surface cleanup.

Passed: engine 7/7 (`button-probe-20260826T165107Z-2605452.log`), workspace
2/2 (`button-probe-20260826T165248Z-2607166.log`), adapter 2/2
(`button-probe-20260826T165323Z-2607738.log`), real PDFium 1/1
(`button-probe-20260826T165349Z-2608205.log`), and warm all-targets 194 active
plus twenty gated ignores in 40 seconds
(`button-probe-20260826T165507Z-2609253.log`). Source/guard policy passes 17/17
with 18 checksum-bound shared sources, the exact prepared digest, one GPUI
identity across 870 packages, configured dependency policy, and green storage.

This remains partial. Vertex paths, ellipses and other source families, PDF
content, embedded and construction grids, dimension increments, tracking,
alignment, equal size/spacing, move/resize/edit snapping, and rotated-page or
UserUnit breadth are not implemented. Fresh native visual/accessibility,
packages, physical platform input, IME, Hibbeler, third-party geometry corpora,
and matched performance were not run. Production PDFium and Windows save
authority remain blocked. `rustfmt` was not run because the pinned toolchain
does not include it; the toolchain was not changed.

## Shared Rectangle/Ellipse property inspector correction (2026-08-26)

The retained workspace now uses one application-owned property-inspector seam
for Rectangle and Ellipse. Real pinned GPUI Component controls render both
families; raw GPUI remains limited to page and annotation pixels, hit testing,
pointer geometry, and selection chrome. Ellipse persistence emits the standard
`/Circle` representation with its appearance resources, rotation, dash, and
alpha state, and replacing or deleting an Ellipse releases owned resources.

Passed Linux development evidence: focused shared inspector 4/4 plus one
explicitly ignored real test (`button-probe-20260826T175054Z-2646520.log`),
gallery Ellipse persistence 3/3 (`button-probe-20260826T175145Z-2647590.log`),
real checksum-pinned PDFium journey 1/1 in six seconds
(`button-probe-20260826T175223Z-2648310.log`), and warm all-targets 198 active
plus 21 gated ignores in 41 seconds
(`button-probe-20260826T175238Z-2648636.log`). Source and guard policy pass
17/17. The prepared digest is
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`; the
dependency graph remains one pinned GPUI identity across 870 packages, and
the retained owned targets are within the four GiB cap with host storage
green.

Failed evidence is retained red-first diagnostic history only; the missing
shared event seam, Ellipse reopen comparison, panel persistence, global lock,
and appearance clipping defects are fixed. Partial: only Rectangle and
Ellipse use the shared inspector; other shape families still need contract
audits. The pinned ColorPicker has no disabled API, so the wrapper dims it and
application state rejects disabled events. Blocked: production PDFium
redistribution, Windows save-target authority, and native visual/live
accessibility on this headless host. Not run: packaged candidates, physical
macOS/Windows input, IME, Hibbeler or third-party Ellipse corpora, and matched
Electron/GPUI performance. This remains development-only evidence.

The next cutover-critical slice is non-default PDF coordinate-space support:
preserve CropBox origin, inherited rotation, and `/UserUnit` through rendering,
thumbnails, tiles, annotations, hit testing, Save As, and fresh reopen.

## Canonical PDF coordinate-space slice (2026-08-26)

`PageCoordinateSpace` is now the shared GPUI-free contract for raw PDF points,
CropBox origin, inherited quarter-turn rotation, and `/UserUnit`. The checked
worker protocol carries the same metadata. `PdfiumWorkerResource` derives
full-page and tile transforms from that space, and `NativeDocumentSession`
retains it across open, recovery, and validated Save As/reopen. The annotation
layer consumes the effective space after a retained page rotation; older mock
openers use a clearly bounded zero-origin/unit-one fallback.

Passed: gallery coordinate parser/transform 7/7, protocol 2/2, retained
workspace metadata 1/1, compat worker build 1/1, real checksum-pinned PDFium
journey 1/1 (including the existing non-zero CropBox/Rotate fixture),
source/build policy 17/17, and warm all-targets 199 active plus 21 gated
ignores. Evidence logs are `button-probe-20260826T233731Z-2728570.log`,
`button-probe-20260826T230354Z-2701558.log`,
`button-probe-20260826T233901Z-2729301.log`,
`button-probe-20260826T233517Z-2726975.log`,
`button-probe-20260826T234720Z-2733533.log`, and
`button-probe-20260826T234756Z-2734176.log`. The prepared digest is
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`; the
resolved graph is one pinned GPUI identity across 870 packages; dependency
policy has no denial; and both storage guards stayed green.

Failed: one gallery worker build reached the four GiB disposable-target cap
and was cleaned by the safety policy; it is retained diagnostic history, not
accepted evidence. Partial: the real fixture now exercises a non-zero CropBox
and `/Rotate 90`, but no real fixture carries `/UserUnit`; highlight
precomposition, snapshot capture, hit testing, and persistence need a dedicated
non-default-space journey. Blocked/not run: production PDFium redistribution, native visual and
live accessibility on the headless host, packaged candidates, physical
macOS/Windows input, IME, Hibbeler, and matched performance. This is Linux
development-only evidence and does not authorize cutover.

Next boundary: add one provenance-controlled non-default-space fixture, then
prove page, thumbnail, tile, annotation, hit-test, Save As, and fresh-reopen
agreement before expanding annotation coverage.

## Real `/UserUnit` qualification (2026-08-27)

The locked fixture `bp-coordinate-space-v1.pdf` combines CropBox
`[18 24 342 216]`, `/Rotate 90`, and `/UserUnit 2` under SHA-256
`dc450b09b502f23518ed361986d9a939ed6b9c2dc1fdb6890af30fae4b253a7d`.
The exact real runner now proves page, thumbnail, and tile pixels; raw-PDF
pointer Rectangle creation and stroke hit testing; canonical Highlight
precomposition for page, thumbnail, and tile surfaces; non-empty two-click
Snapshot capture; Save As; unchanged canonical metadata on independent reopen;
fresh-workspace Rectangle, Highlight, and Snapshot hydration; worker PID exit;
and mapped-surface cleanup.

Passed: fixture oracle 8/8 and independent seven-PDF validation; build guard
13/13; exact real test 1/1
(`button-probe-20260827T055130Z-2805880.log`); warm all-targets 199 active plus
22 gated ignores (`button-probe-20260827T055441Z-2807434.log`); source
preparation 4/4; exact prepared digest; one GPUI identity/870 packages;
dependency policy without denial; and green storage. Failed: retained red-first
fixture and pointer-test diagnostics only; all accepted gates pass. The broad
rustfmt check still finds inherited formatting drift, so unrelated dirty files
were not reformatted. Blocked/not run: production PDFium,
Windows save authority, native visual/accessibility on this headless host,
packages, physical-device input, IME, Hibbeler, coordinate corpora, and matched
performance.

Next: migrate the custom/imported template-library lifecycle through the real
document-session seam, with deterministic persistence, failure, dirty-state,
and reopen proof.

## Imported template-library spine (2026-08-27)

`TemplateLibrary` now owns versioned experiment storage, checksum-bound managed
PDF copies, stable custom/imported IDs, last-used selection, atomic index
publication, restart validation, removal, and independent dirty-document
materialization. `DocumentWorkspace::create_owned_template_document` consumes
the existing owned temporary-document token, so imported and generated records
share the same open, Save As, dirty-close, stale-result, and release lifecycle.

Passed: pure library 1/1 (`button-probe-20260827T061249Z-2816433.log`), focused
workspace 1/1 (`button-probe-20260827T060924Z-2814015.log`), real PDFium 1/1
(`button-probe-20260827T061315Z-2816889.log`), and warm all-targets 200 active
plus 23 ignored (`button-probe-20260827T061454Z-2817891.log`). Guard/source
tests are 17/17; the prepared digest and 870-package single-GPUI graph are
unchanged; dependency policy has no denial. Retained red diagnostics are fixed.
The broad inherited rustfmt drift remains outside this slice.

Partial: the existing GPUI Component split control still renders built-ins
only. Next, connect a library snapshot and manager surface using the pinned real
`Dialog`, list, input, and button APIs. Native visual/accessibility, IME,
packages, physical platforms, Hibbeler, and matched performance are not run.
Production PDFium and Windows save-target authority remain blocked.

## Native template manager journey (2026-08-27)

The runnable story now contains an experiment-owned persistent template
manager. It composes the pinned real GPUI Component `Dialog`, `ListItem`,
`Input`, `Field`, `ButtonGroup`, `Button`, `Alert`, and scrollbar primitives.
The view owns transient draft, selection, request-generation, error, and focus
state only. `PersistentTemplateManager` remains the sole durable authority.

Passed Linux development evidence: final manager/control 16/16
(`button-probe-20260827T112727Z-2951378.log`); native application menu 6/6
(`button-probe-20260827T111502Z-2943954.log`); dynamic Document Tab Bar picker
and commands 23/23 (`button-probe-20260827T105517Z-2932292.log`); real custom
Square Grid journey 1/1 (`button-probe-20260827T111543Z-2944461.log`); real
imported 100-page journey 1/1 (`button-probe-20260827T111606Z-2945127.log`);
and guarded warm all-targets 217 active plus 23 gated ignores in 57 seconds
(`button-probe-20260827T112800Z-2951882.log`). The fast policy gate is 21/21.

Failed evidence is fixed red-first history only. Blocked: production PDFium
redistribution and native visual/live accessibility on this headless VPS. Not
run: packages, physical macOS/Windows input, screen reader, IME, Hibbeler, and
matched performance. Dynamic rows, the complete settings editor, persistent
manager, and authorized-source Save Document as Template route are implemented.
Native Escape/outside-click behavior is configured through the pinned Dialog,
but fresh native input proof is not run and is not inferred from tests.

## Native viewer shell, CAD, and constrained layout (2026-08-27)

The experiment now composes the proven controls into one real-document
`DocumentWorkspace`. Application-owned sessions retain page-view, zoom, fit,
CAD organisation, pages per lane, scroll, render authority, and error state.
The pinned GPUI Component graph supplies the actual controls, resizable panels,
and scrollbars. The existing GPUI-free gallery planner supplies Columns/Rows
CAD layout. No upstream fork or graph change was needed.

The thumbnail rail uses raw GPUI `uniform_list` because virtualization is
domain behavior. It exposes stable IDs and list semantics for every page,
supports End navigation, and lazily adds real thumbnails. The pinned API has no
public exact six-row overscan or 24 px pixel-prefetch setting, so those Electron
numbers remain a transparent upstream capability gap.

Preview, Full, and Detail are actual raster jobs, not labels. Their bounded
caches are 32, 160, and 64 MiB. A per-document scheduler implements the frozen
0-160 ms adaptive Full dwell table, 1.5/0.75 px/ms rapid-motion hysteresis,
180 ms settle, immediate thumbnail targets, and 1200 ms Detail dwell. Timer,
plan, raster, and document cancellation reject stale work. Upgrade failure
keeps prior pixels; a page with no pixels renders one real Alert plus Retry.

Focused guarded evidence: quality policy/scheduler/cache 5/5
(`button-probe-20260827T124423Z-2994413.log`), rendered timer promotion 1/1
(`button-probe-20260827T124646Z-2996072.log`), recovery 1/1
(`button-probe-20260827T124711Z-2996544.log`), and tab/667 px geometry 23/23
(`button-probe-20260827T125039Z-2999005.log`). Runner modes are
`viewer-cad-state`, `viewer-cad-workspace`, `gallery-viewer-cad`,
`viewer-thumbnails`, `viewer-quality`, `viewer-quality-workspace`, and
`viewer-render-recovery`. Every Rust run uses both storage guards and retains
the warm target after ordinary red failures.

The final guarded all-targets gate passes 227 active tests with 23 explicitly
gated ignores in 63 seconds
(`button-probe-20260827T130148Z-3006866.log`). The separate toolbar suite passes
8/8 with the 607 px base and 667 px CAD thresholds
(`button-probe-20260827T130109Z-3006192.log`). Source/guard policy passes 21/21;
the prepared digest is
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`;
the graph has one GPUI identity across 870 packages; dependency policy has no
denial; and host storage is green.

Blocked/not run: native screenshot and live accessibility on this headless
host, high contrast, packaged candidates, physical macOS/Windows, production
PDFium redistribution, Hibbeler, and matched performance. Pointer tooltip,
scrollbar-thumb, and rail-resize evidence remains not run. The pinned Button
has no independent accessible-name setter for custom-child icon buttons.

### Viewer-shell acceptance addendum (2026-08-27)

The application-owned adaptive controller now ports the frozen Electron sample
windows, refresh-rate inference, frame/input/render pressure, direct stability
degradation, and six-evaluation recovery policy. Live document sessions feed
real GPUI frame and input receipt times into the controller and limit
level-three page work to one active job. The pinned graph exposes no portable
process-resource sampler or platform event timestamp, so those inputs remain
explicit partial gaps.

The workspace now renders real GPUI Component `Progress` primitives for PDF
opening and page-quality work inside polite status regions. The absolute overlay
does not change page bounds. Native horizontal and vertical scrolling is copied
back into application-owned per-document state at the planning boundary. A real
pointer drag resizes the GPUI Component thumbnail rail by 40 px while preserving
the 180–360 px range and non-overlap geometry. Long labels remain capped at 190
px. Session close actions are real 24×24 GPUI Component icon Buttons with stable
accessibility IDs, independent names, and tooltips through the shallow
experiment-only accessibility adapter. Zoom In and Zoom Out now use the exact
Lucide 24 px paths used by Electron; ISC provenance remains in the pinned pnpm
source and license graph.

Passed: adaptive policy/viewer 2/2
(`button-probe-20260827T132205Z-3018822.log`), live adaptive workspace 1/1
(`button-probe-20260827T132629Z-3021066.log`), progress and two-axis state 1/1
(`button-probe-20260827T133824Z-3031722.log`), CAD/layout/rail/long-label/close
journey 1/1 (`button-probe-20260827T134945Z-3038427.log`), toolbar 9/9
(`button-probe-20260827T134608Z-3036456.log`), source/guard policy 21/21,
prepared digest `630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`,
one GPUI identity across 870 packages, and final guarded all-targets 232 active
plus 23 gated ignores in 40 seconds
(`button-probe-20260827T135549Z-3041567.log`). Dependency policy passes with
the reviewed two missing-license-field and five unmaintained warnings but no
denial. Storage ended with about 104 GiB free and a retained 3.04 GiB target.

Failed: retained red-first diagnostics exposed missing status surfaces,
scroll-state synchronization, and the 24 px workspace close target; each is
fixed. Blocked: production PDFium redistribution and native visual/live
accessibility on this headless host. Not run: tooltip-popup pixels,
scrollbar-thumb pointer drag, native high contrast, packaged candidates,
physical macOS/Windows input, screen reader, IME, Hibbeler, hostile corpora,
and matched performance. The pinned tooltip overlay has no deterministic
debug-selector seam, so hover and layout stability pass but visible popup
acceptance is not inferred.
