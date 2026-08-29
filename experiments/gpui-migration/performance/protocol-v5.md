# Butter Paper paired comparison protocol v5

Protocol: `bp-perf-v5`

Scenario contract: `bp-perf-v5-representative-1`

Decision contract: `bp-perf-v5-decision-1`

Decision contract artifact SHA-256:
`2acdab1dc3f62c1eed82f5d9af9f50c525617cac49c3c4b60fd885116563cfb1`

This file defines the additions to `protocol.md` and the v4 protocol. Rules not
replaced here remain unchanged.

## Runtime artifact

Runners must read `comparison-workload-v5.materialized.json`. They must verify
its canonical artifact SHA-256
`cc4f8b8940556390b8d16a6baae43e8a5a022541fba90beea08869e692ee920e`
and byte SHA-256
`e7b2540c7d455a30e52ee64a6819745fe0ad49a6512f887df4631aac72054f6d`
before launch.

Each v5 report must identify the candidate, materialized workload, scenario
contract, every fixture, and every command receipt. A hard-component command
receipt passes only when `live` and `passed` are true, its evidence SHA-256 is
valid, and its ordered `proven_milestones` array exactly matches the workload.

## Benefit timing boundary

Correctness evidence and benefit evidence are separate. A semantic diagnostic,
direct-model command, or other non-native lane can prove correctness, but it
cannot contribute CPU, memory, latency, frame-pacing, or GPU benefit metrics.

A native component contributes benefit metrics only when both candidates retain
the same implementation-neutral `common_benefit_timing_boundary` receipt. The
receipt starts with trusted XTEST input and ends with X11
`PresentCompleteNotify`. Both timestamps use `CLOCK_MONOTONIC`, and an
independent observer records at least one sample and a finite positive p95.
The receipt must identify
`x11-present-complete-after-xtest-v1`, set
`observer_process_independent`, `passed`, and `decision_timing_eligible` to
true, and state `physical_scanout_observed: false`.

Electron's callback at `requestAnimationFrame` and GPUI's platform draw
submission remain useful application diagnostics. They are not equivalent end
boundaries and cannot make a component benefit-eligible. If the common receipt
is absent or invalid, the orchestrator omits the component measurements and
blocks the paid comparison. The analyzer repeats this check from retained raw
and hard-report evidence, including the GPUI-only multi-document safety gate.

`native-x11-present-observer.c` is the shared implementation-neutral observer.
It runs outside the candidate process and owns the real click, key, pointer, or
wheel action used by the benchmark. It selects `PresentCompleteNotifyMask` on
the verified candidate window before input, drains older events immediately
before the terminal XTEST event, and timestamps that event and the next matching
Pixmap `PresentCompleteNotify` with `CLOCK_MONOTONIC`. Each sample retains a
unique action token and sequence, the exact action count, window identity, and
Present serial/UST/MSC. The held dynamic-fidelity helper uses the same binding
inside its exact 3,841-sample replay so it does not change that trajectory. The
Electron open action records the independently verified native chooser as the
input window and the candidate BrowserWindow as the presentation window. The
observer verifies `XGetInputFocus` immediately before it injects the chooser's
terminal Return. Other actions require the input and presentation window to be
the same. The
observer rejects a missing Present extension, non-viewable window, non-Pixmap
completion, duplicate correlation, invalid timestamp, or timeout. The receipt
does not claim physical scanout. Building the observer requires the XPresent
development header and library. On Debian/Ubuntu the explicit dependency is
`libxpresent-dev`.

The current `bp-perf-v5` hashes remain frozen and must not be changed silently.
V5 still maps these benefit-declared components to a semantic diagnostic lane,
so the new native-only gate correctly makes them ineligible:
`viewer-layout`, `page-navigation`, `cache-pressure`, `close-reopen`,
`fit-modes`, `zoom`, `high-zoom-pan`, `cache-pressure-recovery`,
`annotation-properties-history`, `editor-workload`, and
`persistence-workload`.

The minimum follow-up is a versioned `bp-perf-v6` workload, scenario contract,
decision contract, and reviewed hashes. Until those 11 components have real
native replay, v6 must run each once per candidate as correctness-only and omit
it from benefit weights. The eight native component names remain eligible:
`open-pdf`, `continuous-scroll`, `viewer-dynamic-fidelity`,
`annotation-create`, `annotation-transform`, `editor-create`,
`native-snap-transform-120hz`, and `multi-document-session`. Because `open-pdf`
appears in three journeys, this is 10 eligible component launches per
implementation per pair: 600 paired benefit launches for six calibration plus
24 final pairs. Add 22 one-time semantic correctness launches and the existing
two property correctness launches for a minimal 624-launch v6 schedule. A
semantic component becomes benefit-eligible only in a later reviewed contract
that gives it real native input and this common Present receipt.

## Multi-document session receipts

Fixture order and hashes:

| Fixture                      | SHA-256                                                            |
| ---------------------------- | ------------------------------------------------------------------ |
| `bp-single-page-v1`          | `f31adeeb3f17ef180012fe707cb2f2650854305dab4b16bba34d73652b6d8fdc` |
| `nasa-apollo-summary-526-v1` | `68d0f3bb93fd1c4e4f3adf99d483e9161f14293a311e25aa6c31241bbbb84049` |
| `bp-engineering-sheet-v1`    | `49b417e4652a5fc0efb3b59b1f482b443bf3133f810f652559931a08b68a2b91` |
| `bp-annotation-density-v1`   | `1dad337b97d573ff971e886eb6509e80e5e1cd07f6ac610ae4f2f9419c666682` |

Command milestone arrays:

- `session:open-four-fixtures`: `application-process-id-recorded`,
  `four-documents-opened`, `tab-order-exact`,
  `document-identities-distinct`, `current-raster-after-each-open`,
  `aggregate-resource-observations-complete`.
- `session:switch-four-fixtures`: `application-process-id-stable`,
  `trusted-native-input-complete`, `switch-sequence-exact`,
  `per-document-state-isolated`, `current-raster-after-each-switch`,
  `aggregate-resource-observations-complete`.
- `session:edit-dense-rectangle`: `application-process-id-stable`,
  `trusted-native-input-complete`, `dense-rectangle-created-once`,
  `dense-rectangle-property-gesture-observed`, `dense-document-dirty`,
  `other-document-states-unchanged`, `thumbnail-current`.
- `session:close-three-and-recover`: `application-process-id-stable`,
  `close-three-sequence-exact`, `closed-document-resources-released`,
  `memory-recovery-recorded`, `one-document-remains`,
  `dense-document-active`, `dense-rectangle-property-current`,
  `interactive-document-shell`.

The report must retain one stable process ID, zero restarts, eight current-raster
receipts across open and switch, one dense Rectangle property user gesture, the
observed history revision delta, a final four-point stroke, three
closed-document resource-release receipts, and one remaining active dense
document.

The maintained Electron control can instead retain the exact
`electron-multi-document-second-nasa-visible-pages-empty-v1` defect when the
second activation selects the NASA tab but reports no visible page indices,
zero queued and in-flight raster work, no presented raster, and no error. This
exact outcome remains a product correctness failure. Its CPU, memory, latency,
frame-acknowledgement, and GPU benefit metrics are explicitly missing and
ineligible. It does not block execution of the retained Electron control.
GPUI must pass the normal multi-document contract; a generic timeout, GPU
failure, or other empty-view outcome is not this allowance.
The stopped open command must retain `live:true`, `passed:false`, and only its
actually proven milestone prefix. The three unexecuted command receipts must
retain `live:false`, `passed:false`, and an empty milestone array. The analyzer
rejects a fabricated complete milestone receipt.

## GPUI multi-document absolute safety budgets

Every live GPUI `multi-document-session` report must retain all seven safety
measurements. This gate is mandatory when the exact Electron second-NASA defect
makes comparative multi-document metrics ineligible, and it also applies when
the Electron control passes. The analyzer uses the maximum across all retained
final GPUI multi-document reports and applies every budget conjunctively.

| Measurement                                     | Maximum              | Unit                | Provenance                                             |
| ----------------------------------------------- | -------------------- | ------------------- | ------------------------------------------------------ |
| `cpu_seconds`                                   | `120`                | process CPU seconds | `new-v5-safety-cap-no-v4-absolute`                     |
| `cgroup_peak_memory_bytes`                      | `1610612736`         | bytes               | `summarize-paired-v4 process_memory absoluteBudgetFor` |
| `product_wall_or_latency_ms`                    | `60000`              | milliseconds        | `new-v5-safety-cap-no-v4-multi-document-map`           |
| `application_frame_interval_p95_ms`             | `25`                 | milliseconds        | `v4 native frame absoluteBudgetFor`                    |
| `native_input_to_application_frame_ack_p95_ms`  | `33.333333333333336` | milliseconds        | `v4 1000/30 native ack absoluteBudgetFor`              |
| `baseline_adjusted_gpu_peak_memory_mib`         | `2048`               | MiB                 | `v4 GPU memory absoluteBudgetFor`                      |
| `baseline_adjusted_gpu_utilization_p95_percent` | `100`                | percent             | `new-v5 physical percentage ceiling`                   |

`product_wall_or_latency_ms` must retain
`product_wall_or_latency_source` as either `product-latency` or
`component-wall`. A finite, structurally valid maximum above any budget is a
measured technical **NO**. A missing, nonfinite, or structurally invalid value
is **BLOCKED — not decision ready**. The live gate
`gpui-multi-document-absolute-safety-budgets-resolved` records that the outcome
was validly resolved; it does not turn an exceeded budget into a pass.

## Native property edit and undo receipts

`annotation:native-property-edit-undo` changes Rectangle stroke width from 1.5
points to 4 points and then performs one undo. It must prove:

`trusted-native-input-complete`, `property-user-gesture-complete`,
`property-state-current-before-undo`,
`native-property-presentation-acknowledged`,
`application-undo-applied-once`, `implementation-history-outcome-recorded`,
and `thumbnail-current`.

GPUI must record one effective history revision and restore the 1.5-point
canonical state with one application undo. The maintained Electron control
records two real history revisions because native text input updates once and
blur updates the unchanged value again. One application undo therefore leaves
the stroke at four points. The exact
`electron-numeric-property-input-blur-duplicate-history-v1` receipt is a
retained baseline defect, not a reason to change production or imitate the bug
in GPUI. This component contributes no timing or resource benefit metrics.

## Matched view-state receipts

Every benefit-eligible component must emit exactly two
`comparison-view-state` events: `measurement-start` before its first measured
input and `measurement-end` after its last presented result. Each event must be
a live `live-application-render-state` observation. It must contain the logical
window and document-viewport bounds, display scale factor, single-page or
continuous layout mode, fit-page/fit-width/manual zoom mode plus the observed
zoom percentage, left and right sidebar visibility plus occupied width, and
the active fixture ID, tab index, and open-document count.
Before an open operation, both active-document fields can be `null` only when
the observed open-document count is zero.

`matched-view-state-v5.mjs` validates and hashes these observations. After the
second implementation finishes a pair, `run-paired-v5.mjs` compares Electron
and GPUI before it runs more paid work. Geometry can differ by at most 0.5
logical pixel, zoom by 0.05 percentage point, and display scale by `1e-6`.
Layout, zoom mode, sidebar visibility, active fixture, tab index, and document
count must match exactly. Missing, duplicated, declared-only, internally
inconsistent, or unmatched observations block benefit metrics. A component
with an explicit retained benefit-ineligible result is the only exception.

The completed manifest and analyzer input retain 180 hashed pair assessments:
six journeys times 30 calibration/final pairs. The analyzer reconstructs each
receipt from raw live events and recomputes every Electron/GPUI match. This
prevents a matching declaration or a modified bundle projection from making
different viewport geometry appear comparable.

## Native snap transform receipts

`annotation:native-snap-transform-120hz` translates the exact Rectangle through
361 timestamped samples over three seconds. The 18-point grid uses the
maintained inclusive 8 CSS-pixel per-axis L-infinity sensitivity. Each report
retains the observed CSS-pixels-per-point scale and its derived point threshold.
The unsnapped `(97,83)`-point translation receives the `(-7,+7)`-point
correction when the live scale places it within that sensitivity and produces
the final rectangle
`{x1:162,y1:234,x2:342,y2:450}`. Maximum coordinate deviation is 0.01 point.

It must prove `timestamped-native-input-complete`, `snap-target-acquired`,
`snap-guide-presented`, `snapped-geometry-exact`, `gesture-committed-once`,
`undo-redo-exact`, and `thumbnail-current`.

## Dynamic fidelity receipts

`viewer:dynamic-fidelity-scroll` retains the exact 20-second forward phase,
2-second pause, and 10-second reverse path. The forward phase contains 19,250
ms of motion plus three real 250 ms zero-input checkpoint holds. Each hold has
30 zero-input intervals and 31 inclusive 120 Hz samples. The full trajectory
still has 3,841 inclusive positions. Its independent observer has 1,921
inclusive 60 Hz samples. Native
event encoding can differ only where the platform requires multiple wheel
clicks for the same frozen distance; phase, timing, and trajectory receipts
must remain exact.

The command must prove `timestamped-native-input-complete`,
`fixed-cadence-fidelity-samples-exact`,
`presented-screenshot-crops-three-matched`,
`presented-scale-comparability-proven`, `checkpoint-holds-stable`,
`visible-page-ready-fraction-recorded`,
`visible-raster-ready-area-fraction-recorded`,
`visible-raster-pixel-density-recorded`, `virtual-page-window-bounded`, and
`finish-page-current`.

The report retains each sample and three presented-screenshot crop receipts.
Each receipt includes the screenshot, native candidate crop, unchanged Poppler
reference, and registered reference hashes. It also includes the v2 filtered
SSIM, dark-content precision, recall, and F1 values. Missing observations, a
filtered SSIM below `0.97`, any dark-content value below `0.99`, or a nonfinite
metric makes the hard component fail.

Each sample stores its zero-based index and ideal scheduled offset. The runner
also retains the actual monotonic observation timestamp as diagnostic scheduler
evidence; it must never manufacture catch-up observations for missed callbacks.
The three quality fields use the exact reducer in `dynamic-fidelity-v5.mjs`.
Missing or stale raster area contributes zero to both ready area and pixel
density. A visible page counts as ready only when a current-generation raster
covers its complete visible intersection.

Both candidates must use fixed 100% zoom on the scale-one X11 lane. The actual
painted page bounds must measure 1.00 device pixel per PDF point on each axis,
with `0.01` tolerance for each value, axis agreement, and paired-candidate
agreement. Fit modes and manually reconstructed layout bounds are invalid.

The runner extracts the candidate from the presented client drawable at its
native dimensions. Candidate upscaling or downscaling is forbidden. The
unchanged 144-DPI Poppler reference must be at least as large on both axes; the
runner downsamples only that reference to the candidate dimensions with fixed
Lanczos3. It must reject a smaller reference. The workload pins the exact
transferred reference PNG SHA-256, so a runner cannot regenerate or substitute
another reference.

`measureCrossEngineScanFidelityV2` defines the exact
`bp-cross-engine-binary-scan-fidelity-v2` receipt. It permits a bounded one-pixel
phase, applies a sigma-2 Gaussian filter with radius 6 for filtered luma SSIM,
and separately compares pixels at or below luma 192 with a one-pixel match
radius. The separate dark-content gates preserve sensitivity to missing and
added thin strokes after filtering.

The PDFium calibration candidates under
`fixtures/scan-fidelity-v2` pin calibration manifest SHA-256
`e04ffb02bba0e7b279c25c5f835509b56c46dc3bfcf1548e6c3ca5a26d272345`.
They calibrate the cross-engine gate but are not acceptance references. The
three Poppler PNGs and their workload-pinned hashes remain unchanged.

Each screenshot must be bracketed by before and after painted-state receipts
inside its 250 ms zero-input hold. State-sequence fields are monotonic bracket
evidence and need not be equal. The semantic scroll offset, painted page
bounds, render generation, and current-raster readiness must remain unchanged
across capture. A stale geometry receipt or a page advance during capture is a
hard failure.

On Linux/X11, the runner keeps one native capture helper connection open for
the verified client window. The helper uses `XGetImage` on that exact presented
drawable and writes two exclusive-create P6 PPM artifacts during each hold.
The helper receipt includes monotonic capture start/end times, window ID,
width, height, and depth. Both capture intervals must fall between the first
and last native helper samples for the matching hold. External screenshot
process startup and an internal PDF/raster export are not valid substitutes.

The runner converts PPM to PNG only after the timed input and observer streams
finish. It verifies decoded pixel SHA-256 equality across conversion and never
resizes the candidate. Each output path owns a fresh per-iteration evidence
directory; a nonempty directory fails before application launch. GPUI keeps
the presented drawable alive after application evidence until it reads the
runner's exact passed/failed result file. A missing, malformed, wrong-source,
or timed-out result fails closed.

## Analyzer boundary

The v5 paired implementation provides the report validator, dynamic quality
extractor, paired quality bootstrap, runner adapters, orchestrator, and
fail-closed analyzer. Their existence is not live qualification. The analyzer
can produce a final decision only after both implementations emit complete,
hard-valid v5 bundles for the declared calibration and final matrix. Missing or
incomplete live evidence keeps execution readiness and the decision blocked.
