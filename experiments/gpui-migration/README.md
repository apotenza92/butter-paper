# Butter Paper native GPUI rebuild

This experiment is a functional rebuild of Butter Paper in Rust and GPUI. It
is not a line-by-line Electron port and does not target Nova pixel parity.

The shipping Electron application remains the product reference for user
capabilities and document compatibility. The native application may use a
different layout, interaction detail, or implementation when it provides the
same useful outcome with a coherent desktop experience.

## Active direction

- Use the pinned Longbridge GPUI Component and Zed GPUI graph in
  `gpui-component-compat`.
- Use GPUI Component for ordinary application controls: buttons, menus,
  dialogs, tabs, inputs, lists, popovers, tooltips, progress, and resizable
  regions.
- Use product-owned GPUI rendering for PDF pages, annotation scenes, selection
  handles, hit testing, and other document-canvas behavior.
- Keep document, PDF, annotation, persistence, and session state outside UI
  components.
- Build complete user journeys in functional chunks. Test a chunk when its
  journey is usable; do not run the full acceptance gate after each small code
  change.

GitHub issue #82 is the active migration specification. The current functional
implementation graph is:

- #85: consolidated native reader and viewer;
- #86: multi-document workspace and command routing;
- #87: text and input-method editing;
- #88: shape and path annotation editing;
- #89: engineering measurement and snapping; and
- #90: media, pending redaction, and imported-annotation compatibility.

These issues contain changing work state and acceptance evidence. Issues #83
and #84 are completed foundation and template inputs. Later packaging,
distribution, and promotion work remains outside the current implementation
chunks.

## Keep and reuse

The existing experiment contains substantial reusable work:

- the checksum-bound Longbridge/Zed source preparation and license policy;
- the bounded build and storage guard;
- the PDFium worker, cancellation, stale-result rejection, mapped surfaces,
  page and thumbnail rendering, and resource release;
- the GPUI-free annotation model, geometry, persistence, generated-document,
  template, and PDF modules currently exposed by `gpui-gallery` without its UI
  features;
- native document sessions, safe Save and Save As, dirty-close protection,
  multi-document ownership, and template storage;
- real GPUI Component examples that are useful in the new application shell.

The current `gpui-component-compat` shell is a source of proven behavior, not a
required final architecture. Its large workspace module will be replaced by
capability modules rather than extended indefinitely.

## Historical material

The previous parity program, HTML reviews, prototypes, and chronological
ledgers are preserved under
`archive/parity-era-2026-08-27/`. They are historical reference only and must
not be used as the active migration plan.

`gpui-gallery` also contains the historical GPUI-CE application and its custom
`butter_ui` component layer. Do not extend that UI. Reuse only its GPUI-free
domain and PDF modules until those modules are extracted into capability
crates.

The versioned performance harness under `performance/` is preserved as
historical evidence. A later functional chunk will select the minimum reusable
runner needed for one matched Electron/GPUI journey. Old v4-v7 protocols are
not active product requirements.

## Functional chunks

1. **Consolidated native reader**
   Open multiple real PDFs, render pages and thumbnails, navigate, zoom, fit,
   rotate, scroll, switch documents, close safely, and recover from a failed
   open in one runnable GPUI Component application.
2. **Core annotation editor**
   Select and edit Rectangle/Ellipse, Line/Arrow, Pen/Highlight, Text Box, and
   Image annotations. Include properties, undo/redo, Save, close, and reopen.
3. **Engineering workflows**
   Add paths, clouds/callouts, measurement and calibration, useful snapping,
   Snapshot, and visibly pending redaction as coherent workflows.
4. **Documents and reusable content**
   Complete templates, imported-annotation preservation, multi-document dirty
   state, clipboard commands, recovery, and safe publication.
5. **Native alpha and decision**
   Build the same candidate for Linux, macOS, and Windows; run a matched
   correctness and performance journey; then decide whether to fund PDFium
   distribution, signatures, updater replacement, and production promotion.

Each chunk ends with a runnable journey, focused deterministic tests, one warm
full gate, and fresh native evidence on the applicable platform.

## Build boundaries

All work remains under `experiments/gpui-migration` until production promotion
is separately approved. Production Electron sources remain unchanged by this
experiment.

Rust builds on this VPS use the experiment build wrapper and
`host-storage-guard`. Ordinary test failures retain the owned Cargo target.
Safety failures may clean only the allowlisted disposable target.

Remote GPU testing is a chunk-level acceptance lane. Before leasing paid
compute, record the task budget, expected time-to-live (TTL), hard maximum TTL,
and independent cleanup. Copy evidence off the machine before verifying its
deletion. macOS and Windows functional testing uses the approved local devices
when the relevant chunk is ready.

## Evidence terms

- **Passed:** a current check completed successfully.
- **Failed:** a current check ran and did not pass.
- **Blocked:** a named external decision or unavailable capability prevents it.
- **Not run:** the check has not been attempted for the current candidate.
- **Development-only:** source or development runtime evidence.
- **Packaged:** evidence from the exact package candidate.
- **Physical-device:** evidence from a real supported device and session.

Compilation and deterministic tests do not imply visual, accessibility,
packaging, or physical-device acceptance.

## Current Image evidence

Regular PNG Image has focused Linux development-only acceptance in the active
candidate. The exact `document-image-real` receipt is
`gpui-component-compat/.prepared/evidence/button-probe-20260829T032517Z-1426946.summary.json`.
It covers the rendered picker, pointer placement, move and free-aspect resize,
Undo/Redo, Lock/Unlock and locked edit suppression, Save As, typed PDF and raw
dictionary/stream validation, PDFium pixel localization, fresh-workspace
reopen, and worker, mapped-resource, atlas, and disposable-file cleanup.

The receipt binds fixture-index SHA-256
`f57f940674c2e152fcb1667d68b93a5879aa5ca1dd377263d830e067c206b87a`,
public-PDF SHA-256
`517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b`,
and regular-PNG SHA-256
`fcc714d1ac60ed4b88abf7297830479c7557cb9d219033e7a5a5ad4d6ec18dda`.
The PDFium development boundary is API build 7881 with manifest SHA-256
`87abef82f86d5a2d03242a64bbc44fd7d53f26eaea81c088e6989a15e0d30b8b`,
extraction-receipt SHA-256
`e3b428424df6399e215270bee56a258e34f41f100107413939a30fd41796c92d`,
and loaded-library SHA-256
`f728930966f503652b92acc89b9374a2eeca00ce42e26dccd3e4b5c5161b2d64`.
That community PDFium binary remains approved only for development evidence.

Two earlier red receipts are diagnostic history, not product failures.
`gpui-component-compat/.prepared/evidence/button-probe-20260829T031222Z-1422239.summary.json`
used an incorrectly exact floating-point oracle for pointer-derived geometry.
`gpui-component-compat/.prepared/evidence/button-probe-20260829T032100Z-1425409.summary.json`
incorrectly treated an appearance Form `/BBox` as page coordinates instead of
local coordinates. The accepted receipt supersedes both after correcting only
those test oracles.

Production PDFium redistribution remains blocked. The focused receipt does not
replace the warm full candidate gate. Packaged Linux, macOS, and Windows
candidates; physical macOS and Windows input; native Input Method Editor (IME),
visual, and accessibility acceptance; authorized third-party annotation
corpora; and matched Electron/GPUI performance remain not run for this
candidate.

## Current semantic-snapping evidence

Line and Length semantic snapping has narrow Linux development-only acceptance.
The exact receipt is
`gpui-component-compat/.prepared/evidence/button-probe-20260829T041605Z-1445310.log`.
It records status 0 for the exact
`real_semantic_snapping_line_and_length_save_close_and_fresh_workspace_reopen`
test: the receipt completed in 26 seconds and one test passed in 2.58 seconds.
The real rendered controls exercised
midpoint and endpoint snapping, transient guide cleanup, calibrated commit
geometry, direct `save_as_path`, typed persistence, canonical Line and Length
identities, clean `qpdf` and `pdfinfo` checks, fresh-workspace reopen, and
worker, surface, saved-file, and root cleanup. Frozen source and unit coverage,
not this one real journey alone, covers the inclusive 8-pixel threshold,
scoring, and locked-target eligibility.

The receipt binds semantic-snapping source SHA-256
`405f43714285cd220ecee6245e693393740abe0c79c4917ceb0545a365d63436`,
exact-test source SHA-256
`77eccb7c3cfe72c92860d2f464edfc715e139a3e256fe5b6e1c73b75c77e79d6`,
the public fixture index and PDF, PDFium API build 7881, archive SHA-256
`1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d`,
manifest SHA-256
`87abef82f86d5a2d03242a64bbc44fd7d53f26eaea81c088e6989a15e0d30b8b`,
extraction-receipt SHA-256
`e3b428424df6399e215270bee56a258e34f41f100107413939a30fd41796c92d`,
and loaded-library SHA-256
`f728930966f503652b92acc89b9374a2eeca00ce42e26dccd3e4b5c5161b2d64`,
worker SHA-256
`7c7a7394749d7cc272f5cf60b54d6eacae18cdcefae4dd388001148d8b634040`,
and test-executable SHA-256
`aef4a6093f3b583f821d9ca3e79b80bf90b85a053cc7a9380fd8cb0cab9d486b`.
Its PDFium pixel check is aggregate annotation-enabled versus
annotation-free evidence; it does not isolate or attribute pixels separately
to Line and Length. The test calls `save_as_path` directly and does not prove
the rendered Save control.

The earlier
`gpui-component-compat/.prepared/evidence/button-probe-20260829T041428Z-1444122.log`
is a superseded diagnostic. Its status 101 compile error came from a test-only
`fixture_sha256` scoping mistake and is not a product failure. Snapshot remains
partial and unaccepted; its latest receipt is
`gpui-component-compat/.prepared/evidence/button-probe-20260829T040423Z-1439428.log`,
which failed because the scene preview rotation was not `+30`. The full gate,
rendered Save UI, isolated per-Line and per-Length PDFium pixel checks, packaged
and cross-platform candidates, and native visual and accessibility acceptance
were not run. Production PDFium redistribution remains blocked.

## Current pending-Redact evidence

Pending Redact marks have narrow Linux development-only acceptance. The exact
`redact-cutover-real` receipt is
`gpui-component-compat/.prepared/evidence/button-probe-20260829T043303Z-1455673.log`.
It records status 0 in 25 seconds for the exact
`real_redact_edit_save_close_and_fresh_workspace_reopen` test; one test passed
in 4.07 seconds. The rendered Redact Button and truthful warning lead to a
history-free scene preview, creation, move, resize, Lock and locked-edit
suppression, unlock, direct `save_as_path`, typed persistence, fresh-workspace
reopen, deletion and re-save. `qpdf` JSON v1 proves the canonical native
`/Redact` dictionary, including exact ordered geometry and stored appearance,
and critically proves that the pending mark has no `/AP`. After deletion, the
typed identity and recorded raw object are absent. All workers, mapped
surfaces, disposable files, and the current run's owned root were cleaned, and
the source fixture remained unchanged.

The PDFium check compares annotation-disabled page-zero rasters for the source,
saved pending-mark PDF, and post-delete PDF. Their equality is raster-limited
evidence that this workflow did not remove visible page content. It does not
prove secure redaction, content destruction, or that PDFium renders the pending
mark. The test calls `save_as_path` directly and does not prove the rendered
Save UI.

The receipt binds exact-test source SHA-256
`43ebc688950279cdfcb19f1bc403b7c1177950dc805272c933ea900936410483`,
fixture-index SHA-256
`f57f940674c2e152fcb1667d68b93a5879aa5ca1dd377263d830e067c206b87a`,
fixture SHA-256
`517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b`,
PDFium API build 7881 and library SHA-256
`f728930966f503652b92acc89b9374a2eeca00ce42e26dccd3e4b5c5161b2d64`,
worker SHA-256
`7c7a7394749d7cc272f5cf60b54d6eacae18cdcefae4dd388001148d8b634040`,
and test-executable SHA-256
`21d06b9bf9e4ed63093036084779bcf3c314a6c6b0b6199587903b1b1ecb76d5`.
The two earlier status-101 receipts are superseded test-oracle diagnostics, not
product failures:
`gpui-component-compat/.prepared/evidence/button-probe-20260829T042857Z-1452735.log`
used strict persisted-geometry equality for pointer-derived movement, and
`gpui-component-compat/.prepared/evidence/button-probe-20260829T043009Z-1454035.log`
required a non-semantic JSON object-key order.

Production PDFium redistribution remains blocked. Apply Redactions, content
destruction or removal, sanitization, flattening, appearance generation or
vendor conversion, vendor-import behavior, copy/cut/paste, the rendered Save
UI, annotation-enabled PDFium rendering, the warm full gate, packaged and
cross-platform candidates, and native visual and accessibility acceptance were
not run or proved.

## Current two-document viewer-state evidence

Two-document viewer-state isolation has narrow Linux development-only
acceptance. The exact `viewer-state-real` receipt is
`gpui-component-compat/.prepared/evidence/button-probe-20260829T044730Z-1465444.log`.
It records status 0 in 26 seconds for the exact
`real_native_shell_preserves_independent_view_state_through_fit_scroll_thumbnail_zoom_and_document_switch`
test; one test passed in 4.75 seconds. The two documents are separate copies of
the same checksum-pinned 100-page fixture, not different-content documents.

The real rendered toolbar and tabs preserve independent field-by-field mode,
zoom-preset, manual zoom, scroll, and wheel-behavior state through switching.
The first copy uses Single Page, Fit Page, 400%, page 2, and scroll
`(120, 700)`; the second uses Continuous, Fit Width, 1600%, page 2, and scroll
`(0, 1000)`. Both produce non-uniform real-PDF tiles within the planned cache
bound. The journey rejects a stale tile plan, retains the 32-tile and 256 MiB
limits, lazily materializes and opens page 50 on the first copy, then releases
both workers and all mapped surfaces, removes its PID-scoped root, and leaves
the fixture checksum unchanged.

The immediate Fit Page and Fit Width checks prove fit intent, not exact fitted
geometry or the computed fit percentage. Non-uniform raster tiles prove decoded
content variation, not native-window painting. Bounded cache accounting does
not prove cache reuse or eviction policy. Both copies are on the same current
page during the main state comparison, so the receipt does not prove isolation
between two different current-page values.

The receipt binds native-view-state SHA-256
`3d103de9b62ddf4f3d249252ed80f0df177176b861d9bdd8d5b51aeb32af6a4d`,
component-viewer SHA-256
`f307249befbb9ccd03265bd5930653f746d1667e3b906be283e7f44c73ab8eb3`,
workspace SHA-256
`2c595bfdcf408e49d3a2309e4d97065c68622ed3e5e1f52add0463f925bd1770`,
gallery-viewer SHA-256
`b2d3b308e355f271cf4357c44b275e93c30975ca9e94e8c38e8189a8f1c56479`,
exact-test SHA-256
`50d074e1ee7e0b3502bf6fe540bfc83a93b61b72bceb8e5a9e969209758bf2c5`,
fixture-index SHA-256
`f57f940674c2e152fcb1667d68b93a5879aa5ca1dd377263d830e067c206b87a`,
fixture SHA-256
`517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b`,
PDFium API build 7881 and library SHA-256
`f728930966f503652b92acc89b9374a2eeca00ce42e26dccd3e4b5c5161b2d64`,
worker SHA-256
`7c7a7394749d7cc272f5cf60b54d6eacae18cdcefae4dd388001148d8b634040`,
and test-executable SHA-256
`3bc1074fb59f7b14f76ffe2015d945153ff998aac06237ad08a96b2395a5e1c7`.

Production PDFium redistribution remains blocked. Rotation, failed-open
isolation, search, annotations, save or persistence behavior, distinct-content
documents, exact fit geometry, cache reuse or eviction, native paint,
constrained-window behavior, performance qualification, accessibility,
packaged and cross-platform candidates, and the warm full gate were not run or
proved by this receipt.
