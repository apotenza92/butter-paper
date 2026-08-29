# Native GPUI Component candidate

This crate is the current native Butter Paper candidate and the reviewed
dependency foundation for the functional rebuild. It is development-only and
does not modify or replace the production Electron application.

## Foundation

- Longbridge GPUI Component:
  `c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4`
- Zed GPUI:
  `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc`
- Rust: `1.97.1`
- Prepared source digest:
  `630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`

`source-preparation-policy.json`, `THIRD_PARTY_NOTICES.md`, and the preparation
scripts pin source identity, patches, checksums, allowed dependencies, and
license provenance. The PDFium development manifest remains
`productionApproved: false`.

## Product direction

Use GPUI Component for ordinary controls and GPUI for the document canvas.
Application state owns documents, annotations, viewer state, persistence, and
commands. UI controls render snapshots and dispatch intent.

The existing implementation proves many required behaviors, but its
`DocumentWorkspace` has grown into an application-sized module. Treat it as a
source of working behavior while the active rebuild extracts these capability
modules:

- document session and resource ownership;
- PDF viewer and render scheduling;
- annotation editor and scene generation;
- safe persistence and recovery;
- document tabs and application commands;
- GPUI Component application shell.

Do not add another micro-parity behavior to the monolithic workspace. New work
must complete one of the functional chunks in `../README.md` and leave a
runnable user journey. GitHub issue #82 owns the specification; issues #85
through #90 own the current implementation chunks.

## Reusable implementation

Keep and consolidate:

- PDFium worker lifecycle, mapped surfaces, cancellation, stale-result
  rejection, page and thumbnail rendering, and bounded caches;
- safe Save and Save As publication and source reconciliation;
- multi-document sessions and dirty-close protection;
- annotation editing, history, geometry, and PDF round trips;
- template generation, import, storage, and document creation;
- current real GPUI Component controls and source-preparation policy.

`../gpui-gallery` is consumed with default features disabled. This exposes its
GPUI-free model and PDF modules without adding its GPUI-CE identity to the
candidate. Its old gallery binary and custom `butter_ui` controls are
historical and must not be extended.

## Verification

Run non-build gates first:

```sh
node --test tests/build-guard.test.mjs tests/source-preparation.test.mjs tests/foundation-truth.test.mjs
node scripts/foundation-truth.mjs
node scripts/prepare.mjs verify
node scripts/verify-cargo-graph.mjs
cargo deny --config deny.toml --exclude-dev --locked check \
  --warn unmaintained advisories licenses sources
```

Run Rust compilation and tests through both storage guards:

```sh
host-storage-guard check
host-storage-guard run -- bash scripts/run-bounded-button-probe.sh <focused-mode>
host-storage-guard run -- bash scripts/run-bounded-button-probe.sh all-targets
```

The wrapper uses the allowlisted target
`../.build-targets/gpui-component-compat`, one Cargo job, locked dependencies,
a 30 GiB preflight floor, a 20 GiB runtime stop floor, and a 5 GiB target cap.
It enters this compatibility crate before launching Cargo and clears any
inherited `RUSTUP_TOOLCHAIN`, so the local `rust-toolchain.toml` pin selects
Rust 1.97.1 even when the wrapper is called from the repository root.
Ordinary compile or test failure retains a valid target. Safety failures may
clean only the owned target. Use focused modes while iterating and
`all-targets` once at functional-chunk acceptance.

Build the normal Linux native reader and its sibling PDF worker through both
storage guards:

```sh
host-storage-guard run -- bash scripts/run-bounded-button-probe.sh native-reader-build
```

The focused `native-shell-rectangle-real` mode is also the current alpha-spine
tracer. It opens two checksum-pinned real PDFs, preserves independent reader
state across a failed sibling open, edits and saves a Rectangle through the
native shell, reopens the result, and proves worker and mapped-surface cleanup.
The mode remains ignored in ordinary test runs because it needs the reviewed
development PDFium cache. Before Cargo starts, the runner binds the public
fixture to its reviewed fixture index and binds the loaded PDFium path to the
exact manifest, extraction receipt, and build marker. It rejects an ambient
`BP_PDFIUM_LIBRARY` outside that receipt. The focused summary records hashes
for the relevant application and worker sources, the fixture, PDFium manifest,
receipt and library, and the exact worker and test executables that ran.

The focused `native-shell-text-box-real` mode opens a checksum-pinned public
PDF through the same worker boundary. It creates composed multiline text,
moves and resizes the existing Text Box through the rendered canvas, proves
cancel and Unicode edit history, then edits color, size, opacity, and horizontal
alignment through the rendered Text Box appearance controls. It saves through
application close and checks typed and rendered fresh reopens plus worker
cleanup. Its evidence summary hashes the inspector, Text Box interaction,
persistence, worker, test, and runner sources. Use `text-box-properties` for the
focused retained-control contract and `gallery-text-box-style` for the atomic
model/history contract. Persisted Text Box appearances split multiline content
into deterministic Base-14 Helvetica lines and position each line from the
stored Left, Center, or Right alignment. Appearance strings use WinAnsi bytes;
unsupported Unicode glyphs render as `?`. The separate `/Contents` PDF text
string always uses hexadecimal UTF-16BE with an `FE FF` byte order mark, so it
preserves exact ASCII and Unicode text independently of that appearance
fallback. Import accepts UTF-16BE and UTF-16LE byte order marks and preserves
legacy raw UTF-8 Text Box strings. For a BOM-less string, import tries UTF-8
first, then PDFDocEncoding; a byte sequence valid in both encodings is therefore
interpreted as UTF-8 for backward compatibility. Full Unicode appearance
rendering requires an embedded-font and `/ToUnicode` design outside this
experiment slice.

The focused `native-shell-pen-highlight-real` mode creates exactly one Pen and
one Highlight on that real-PDF boundary. For each exact single selection, it
previews color through `ColorPicker`, commits RGB and alpha once through the
real `Apply color` Button, types line width into `NumberInput`, releases the
real opacity `Slider`, and clicks the real Lock `Switch`. It asserts one history
revision per committed control action, saves through application close, and
proves that the typed PDF session and a fresh workspace reopen contain exactly
those two Ink annotations. Its evidence summary binds the inspector, annotation
model and adapter, persistence, worker, test, and runner sources. Multi-selection
never resolves to a first item.

After that build, launch the existing `component_story` reader with the public
multi-page fixture, or pass one explicit PDF path:

```sh
bash scripts/run-native-reader.sh
bash scripts/run-native-reader.sh /absolute/path/to/document.pdf
```

The launcher uses only the existing checksum-receipted PDFium development
library under `../gpui-gallery/target/pdfium-development`. This community
binary is approved only for prototype development. It is not approved for
packaging or production redistribution, and the launcher never downloads it.

The current mode registry is intentionally injection-safe but overgrown. Its
replacement should allow a small reviewed test-target/filter interface instead
of adding one permanent mode for every test.

## Current milestone evidence (2026-08-29)

This is a factual snapshot of the current local development evidence. GitHub
issues remain the owner of changing work state.

**Passed — Linux development runtime:**

- Regular PNG Image is accepted at the focused real-PDF seam. The exact
  `document-image-real` receipt
  `.prepared/evidence/button-probe-20260829T032517Z-1426946.summary.json`
  records status 0 for one exact test in 25 seconds. It proves the rendered
  Image picker and pointer placement, history-free move and free-aspect resize
  previews, exact Undo/Redo and Lock/Unlock history, locked edit suppression,
  Save As worker replacement, typed and fresh-workspace reopen, canonical raw
  PDF dictionaries, exact RGB and soft-mask stream bytes, localized PDFium
  pixels, worker and atlas release, and disposable-file cleanup. The receipt
  binds the public fixture index (`f57f940674c2e152fcb1667d68b93a5879aa5ca1dd377263d830e067c206b87a`),
  the 100-page PDF (`517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b`),
  and the 512 by 384 PNG (`fcc714d1ac60ed4b88abf7297830479c7557cb9d219033e7a5a5ad4d6ec18dda`).
  It also binds PDFium API build 7881, archive pin
  `1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d`,
  manifest `87abef82f86d5a2d03242a64bbc44fd7d53f26eaea81c088e6989a15e0d30b8b`,
  extraction receipt `e3b428424df6399e215270bee56a258e34f41f100107413939a30fd41796c92d`,
  and loaded library `f728930966f503652b92acc89b9374a2eeca00ce42e26dccd3e4b5c5161b2d64`.
  This is Linux development-only evidence, not packaged or production
  acceptance.
- Arc is accepted at the focused real-PDF seam. The exact
  `arc-cutover-real` receipt
  `.prepared/evidence/button-probe-20260829T000705Z-1333906.summary.json`
  records status 0 for one exact test in 28 seconds. It covers rendered Arc
  editing, Save As, typed and fresh-workspace reopen, PDFium pixel evidence,
  worker exit, and mapped-surface cleanup.
- Measurement Path is accepted for Polylength and Area. The exact
  `measurement-path-cutover-real` receipt
  `.prepared/evidence/button-probe-20260829T014855Z-1384341.summary.json`
  records status 0 for one exact test in 61 seconds. The separate encoding
  compatibility receipt
  `.prepared/evidence/button-probe-20260829T014828Z-1383868.summary.json`
  records status 0 for the exact PDFDocEncoding-or-UTF-16BE persistence test
  in 15 seconds.
- Two-document Save As failure isolation is accepted. The latest exact
  `two-document-save-failure-real` receipt
  `.prepared/evidence/button-probe-20260829T025647Z-1410110.summary.json`
  records status 0 for one exact test in 25 seconds. It proves distinct A/B
  workers, preservation of B's clean view and history while A fails on an
  occupied target, recovery of A to a fresh target, typed and PDFium oracles,
  fresh reopen, and complete worker, surface, and disposable-file cleanup.
- Line and Length semantic snapping is accepted at a narrow Linux development
  seam. The exact `semantic-snapping-cutover-real` receipt
  `.prepared/evidence/button-probe-20260829T041605Z-1445310.log` records status
  0 for the exact
  `real_semantic_snapping_line_and_length_save_close_and_fresh_workspace_reopen`
  test: the receipt completed in 26 seconds and one test passed in 2.58
  seconds. It directly proves real rendered Line and Length controls, pointer
  geometry within 0.001 point, retained semantic
  owner and role for midpoint and endpoint snaps, history-free transient
  guides, calibration revision and exact commit geometry, direct
  `save_as_path`, replacement worker, unchanged source, typed persisted state,
  canonical native identities and annotation order, aggregate PDFium
  annotation-enabled versus annotation-free pixel change, clean `qpdf` and
  `pdfinfo`, fresh-workspace state reset and reopen, and complete cleanup.
  Frozen semantic source and unit coverage, not this real journey alone, covers
  the inclusive 8-pixel threshold, scoring, and locked-target eligibility.
  The receipt binds semantic source
  `405f43714285cd220ecee6245e693393740abe0c79c4917ceb0545a365d63436`,
  exact-test source
  `77eccb7c3cfe72c92860d2f464edfc715e139a3e256fe5b6e1c73b75c77e79d6`,
  fixture index
  `f57f940674c2e152fcb1667d68b93a5879aa5ca1dd377263d830e067c206b87a`,
  fixture `517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b`,
  PDFium API build 7881, archive
  `1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d`,
  manifest `87abef82f86d5a2d03242a64bbc44fd7d53f26eaea81c088e6989a15e0d30b8b`,
  extraction receipt
  `e3b428424df6399e215270bee56a258e34f41f100107413939a30fd41796c92d`,
  and loaded library
  `f728930966f503652b92acc89b9374a2eeca00ce42e26dccd3e4b5c5161b2d64`,
  worker `7c7a7394749d7cc272f5cf60b54d6eacae18cdcefae4dd388001148d8b634040`,
  and test executable
  `aef4a6093f3b583f821d9ca3e79b80bf90b85a053cc7a9380fd8cb0cab9d486b`.
  This is not isolated PDFium pixel attribution for each Line and Length, and
  direct `save_as_path` is not proof of the rendered Save UI.
- Pending Redact marks are accepted at a narrow Linux development seam. The
  exact `redact-cutover-real` receipt
  `.prepared/evidence/button-probe-20260829T043303Z-1455673.log` records status
  0 in 25 seconds for the exact
  `real_redact_edit_save_close_and_fresh_workspace_reopen` test; one test
  passed in 4.07 seconds. It proves the real Redact Button and exact truthful
  Alert, a scene-only history-free nominal preview, pointer creation, move and
  resize within 0.001 point with exact history, rendered Lock and locked
  move/resize/Delete suppression, unlock, direct `save_as_path`, replacement
  worker and unchanged source, typed persistence and canonical identity, clean
  `qpdf` and `pdfinfo`, fresh-workspace reopen with clean state and exact
  annotation order, deletion and re-save, typed-name and recorded raw-object
  absence, and complete worker, surface, disposable-file, owned-root, and
  fixture-integrity cleanup. The canonical qpdf JSON v1 dictionary proves
  `/Type /Annot`, `/Subtype /Redact`, exact `/NM`, `/Rect`, ordered
  `/QuadPoints`, `/IC`, `/Subj`, `/Contents`, Print and lock-bit state, complete
  `/BPAppearance`, `/CA` and `/ca`, default omission of `/OverlayText`, and
  critically no `/AP`.
  The annotation-disabled PDFium page-zero raster is identical for the source,
  saved pending-mark PDF, and post-delete PDF. This is raster-limited
  non-removal evidence only. It does not prove secure redaction, structural
  content destruction, or that PDFium renders the pending mark. Direct
  `save_as_path` is not proof of the rendered Save UI. The receipt binds exact
  test `43ebc688950279cdfcb19f1bc403b7c1177950dc805272c933ea900936410483`,
  fixture index
  `f57f940674c2e152fcb1667d68b93a5879aa5ca1dd377263d830e067c206b87a`,
  fixture `517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b`,
  PDFium API build 7881, archive
  `1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d`,
  manifest `87abef82f86d5a2d03242a64bbc44fd7d53f26eaea81c088e6989a15e0d30b8b`,
  extraction receipt
  `e3b428424df6399e215270bee56a258e34f41f100107413939a30fd41796c92d`,
  library `f728930966f503652b92acc89b9374a2eeca00ce42e26dccd3e4b5c5161b2d64`,
  worker `7c7a7394749d7cc272f5cf60b54d6eacae18cdcefae4dd388001148d8b634040`,
  and test executable
  `21d06b9bf9e4ed63093036084779bcf3c314a6c6b0b6199587903b1b1ecb76d5`.
  The still-present `.prepared/real-redact-cutover-surfaces` path is an
  unrelated stale historical artifact; the accepted run used and removed its
  separate owned root. Do not treat or delete that historical path as current
  evidence.
- Two-document viewer-state isolation is accepted at a narrow Linux
  development seam. The exact `viewer-state-real` receipt
  `.prepared/evidence/button-probe-20260829T044730Z-1465444.log` records status
  0 in 26 seconds for the exact
  `real_native_shell_preserves_independent_view_state_through_fit_scroll_thumbnail_zoom_and_document_switch`
  test; one test passed in 4.75 seconds. The documents are two independent
  copies of the same checksum-pinned 100-page fixture. They are not distinct
  content. The real GPUI Component tabs and toolbar preserve field-by-field
  mode, zoom preset, manual zoom, scroll, and both wheel behaviors: the first
  copy uses Single Page, Fit Page, 400%, page 2, and `(120, 700)`; the second
  uses Continuous, Fit Width, 1600%, page 2, and `(0, 1000)`. The receipt also
  proves non-overlapping controls, distinct workers, non-uniform real-PDF tiles
  for both copies, stale-plan rejection, at most 32 planned tiles and a 256 MiB
  cache bound, lazy page-50 thumbnail materialization and navigation on the
  first copy, worker and mapped-surface release, explicit scratch cleanup,
  PID-root removal, and unchanged fixture bytes.
  Immediate Fit Page and Fit Width state checks prove fit intent, not exact
  fitted geometry or the computed fit percentage. Non-uniform raster tiles are
  decoded-content evidence, not native paint evidence. Cache-byte accounting
  within the configured limit does not prove cache reuse or eviction. Both
  copies have the same page-2 current-page value during the main comparison,
  so this does not prove isolation between different current-page values.
  The receipt binds native view state
  `3d103de9b62ddf4f3d249252ed80f0df177176b861d9bdd8d5b51aeb32af6a4d`,
  component viewer
  `f307249befbb9ccd03265bd5930653f746d1667e3b906be283e7f44c73ab8eb3`,
  workspace `2c595bfdcf408e49d3a2309e4d97065c68622ed3e5e1f52add0463f925bd1770`,
  gallery viewer
  `b2d3b308e355f271cf4357c44b275e93c30975ca9e94e8c38e8189a8f1c56479`,
  exact test
  `50d074e1ee7e0b3502bf6fe540bfc83a93b61b72bceb8e5a9e969209758bf2c5`,
  guard `5db6655f04fe59be56f92f4b2d80194fede71c73d012a56da953a23704bd0449`,
  runner `04caec8c86598a5482d04e557cfe5cd41a069a239cf10d569caf71c07d4c82ac`,
  fixture index
  `f57f940674c2e152fcb1667d68b93a5879aa5ca1dd377263d830e067c206b87a`,
  fixture `517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b`,
  PDFium API build 7881, archive
  `1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d`,
  manifest `87abef82f86d5a2d03242a64bbc44fd7d53f26eaea81c088e6989a15e0d30b8b`,
  extraction receipt
  `e3b428424df6399e215270bee56a258e34f41f100107413939a30fd41796c92d`,
  library `f728930966f503652b92acc89b9374a2eeca00ce42e26dccd3e4b5c5161b2d64`,
  worker `7c7a7394749d7cc272f5cf60b54d6eacae18cdcefae4dd388001148d8b634040`,
  and test executable
  `3bc1074fb59f7b14f76ffe2015d945153ff998aac06237ad08a96b2395a5e1c7`.

**Failed — historical Image diagnostics, superseded by the accepted receipt:**

- `.prepared/evidence/button-probe-20260829T031222Z-1422239.summary.json`
  reached real pointer placement and failed because the new test used an
  exact floating-point geometry comparison for pointer-derived coordinates.
  The corrected test uses an explicit 0.001-point component and edge tolerance
  for pointer placement, move, and resize, while retaining exact geometry for
  Undo/Redo and persistence. This was a test-oracle defect and is not evidence
  of a product failure.
- `.prepared/evidence/button-probe-20260829T032100Z-1425409.summary.json`
  passed placement, history, locking, Save As, typed reopen, `qpdf --check`,
  `pdfinfo`, and the raw annotation dictionary, then failed because the new
  test expected the appearance Form `/BBox` in page coordinates. A Form
  bounding box is local, so the accepted test expects
  `[0, 0, width, height]`. This was also a test-oracle defect and is not
  evidence of a product failure.

**Failed — current partial work:**

- Snapshot remains partial and unaccepted. The latest exact receipt
  `.prepared/evidence/button-probe-20260829T040423Z-1439428.log` failed because
  the scene preview rotation was not `+30`. Preserve this as partial evidence,
  not acceptance.
- The Dimension stable-inspector journey remains red. The latest exact
  `dimension-workspace` receipt
  `.prepared/evidence/button-probe-20260829T023942Z-1403455.summary.json`
  records status 101 for one focused test in 40 seconds. Pointer creation and
  caption editing reach the property trigger without changing selection or
  history, and the permanent inspector slot renders. The failure is that the
  retained `DIMENSION_PROPERTY_INSPECTOR_ID` root is absent after the trigger
  click. Resume at the trigger-to-permanent-slot mount and retained-view
  identity seam; do not reopen unrelated inspectors or change the already
  accepted Dimension persistence cutover.

**Failed — superseded semantic-snapping diagnostic:**

- `.prepared/evidence/button-probe-20260829T041428Z-1444122.log` records status
  101 because the first exact-test patch referenced `fixture_sha256` outside
  its scope. The accepted receipt supersedes this test-oracle compile-scoping
  defect. It is not evidence of a product failure.

**Failed — superseded pending-Redact diagnostics:**

- `.prepared/evidence/button-probe-20260829T042857Z-1452735.log` records status
  101 because the new test used strict persisted-geometry equality for
  pointer-derived movement instead of the specified 0.001-point tolerance.
- `.prepared/evidence/button-probe-20260829T043009Z-1454035.log` records status
  101 because the new qpdf oracle required one non-semantic JSON object-key
  order. The accepted receipt compares the complete parsed object instead.
  Both are superseded test-oracle diagnostics, not product failures.

**Blocked:**

- Production PDFium redistribution is not approved. The pinned PDFium binary
  used by these receipts remains development-only.
- Measurement Path and Hibbeler corpus acceptance beyond generated public
  fixtures is blocked until authorized third-party fixtures are available.

**Not run for these milestones:**

- Packaged Linux, macOS, or Windows candidates; physical macOS or Windows
  input; native Input Method Editor (IME), visual, or accessibility acceptance;
  available third-party Arc and Dimension corpora; and matched Electron/GPUI
  performance.
- A warm full current-candidate gate after these milestones. Focused success
  does not replace that chunk-level gate.
- The rendered Save UI path and isolated per-Line and per-Length PDFium pixel
  checks for the semantic-snapping candidate.
- Apply Redactions, content destruction or removal, sanitization, flattening,
  pending-mark appearance generation or vendor conversion, vendor-import
  behavior, copy/cut/paste, the rendered Save UI path, and annotation-enabled
  PDFium rendering for the pending-Redact candidate.
- Rotation, failed-open isolation, search, annotations, save or persistence,
  distinct-content documents, exact fitted geometry, different current-page
  values, cache reuse or eviction, native paint, constrained-window behavior,
  performance qualification, and accessibility for the two-document
  viewer-state candidate.

## Evidence boundary

The current source includes strong Linux development tests for PDF rendering,
annotations, persistence, templates, document sessions, and GPUI Component
composition. This does not establish current native visual or accessibility
acceptance, production PDFium redistribution, packaged macOS/Windows/Linux
behavior, updater replacement, or production promotion.

The persistence backend now reports an immutable compile-time in-place
publication capability. Linux and macOS keep verified atomic replacement.
Windows routes ordinary Save, retry, dirty-tab close, and application-close
Save/Save All through the existing non-overwriting Save As authority and
suppresses duplicate native destination prompts. While that picker is pending,
the active document's save commands are disabled and the visible picker action
uses the truthful `Save As…` label. Normal downstream code can publish a new
target only through `SaveAsTargetAuthority`; the ambient-path helper is
crate-private and retained only for the explicitly named persistence comparison
scenario. Cross-target library compilation is development evidence only. The
narrow Windows integration-test compile remains blocked at the unmodified
transitive `psm` MSVC assembler boundary, and the Windows picker and publication
journey still requires native functional testing before platform acceptance.

Current progress and acceptance evidence belong in GitHub issues. The former
2,000-line chronological README is preserved in
`../archive/parity-era-2026-08-27/docs/GPUI-COMPONENT-COMPAT-README.md`.
