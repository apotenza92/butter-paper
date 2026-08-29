# Butter Paper GPUI investment decision protocol v5

Contract: `bp-perf-v5-decision-1`

Decision contract artifact SHA-256:
`2acdab1dc3f62c1eed82f5d9af9f50c525617cac49c3c4b60fd885116563cfb1`

Materialized workload artifact SHA-256:
`cc4f8b8940556390b8d16a6baae43e8a5a022541fba90beea08869e692ee920e`

Materialized workload byte SHA-256:
`e7b2540c7d455a30e52ee64a6819745fe0ad49a6512f887df4631aac72054f6d`

This contract extends the decision-grade v4 workload. It does not modify or
reinterpret v4. The v5 loader refuses to resolve unless the checked-in v4
materialized file still has both of these identities:

- canonical artifact SHA-256
  `4a826bd3c19f3c7693128961f43064e5b7414e799a0f791b7f3381ed59e897b1`;
- byte SHA-256
  `8828be7c4c7c05a19007bd315c4fc1844a93278c967d49614387c2ae6cfeff52`.

V5 remains an investment decision for development runtimes. It is not a
packaged-candidate, installer, updater, signing, accessibility, or release
qualification gate.

## Why v5 exists

V4 represents shell open, long-document viewing, engineering-sheet viewing,
dense editing, and persistence. Its editor boundary contains five annotation
families: Rectangle, Highlight/Freehand, Text Box, Length Measurement, and
Image. Those families exercise creation, selection, geometry, appearance,
history, PDF mapping, and the retained save/reopen compatibility path. V5 adds
four behaviors that can materially change the Electron-versus-GPUI result:

1. `multi-document-session` measures one process that owns four live documents
   and proves document isolation and partial resource recovery.
2. `native-property-edit-undo` proves a native properties-panel commit,
   presentation acknowledgement, and exact undo. It is a hard correctness gate
   but does not contribute timing or resource benefit metrics.
3. `native-snap-transform-120hz` measures a snap-enabled Rectangle translation
   driven by 361 native samples over three seconds.
4. `viewer-dynamic-fidelity` measures raster quality throughout the frozen
   32-second native continuous-scroll trajectory. It retains 1,921 independent
   60 Hz observations and three registered visual crops.

All four are representative and conjunctive. A failure cannot be hidden by a
faster component. GPUI property edit must restore canonical state with one
undo. The exact known Electron duplicate-history baseline outcome is retained
as a product defect without blocking the resource comparison. The exact
maintained Electron second-NASA empty-page outcome is also predeclared. When
that outcome occurs, the multi-document benefit metrics are explicitly missing
and ineligible. GPUI must pass both correctness paths. Snap and dynamic fidelity
contribute benefit metrics. Multi-document contributes only when its normal
path passes. Property edit and undo never contributes.

The Electron exception cannot make unbounded GPUI resource use acceptable.
Every final GPUI multi-document report must therefore retain seven absolute
safety measurements. The analyzer takes the GPUI maximum for each measurement
and applies these caps conjunctively: 120 process CPU seconds; 1,610,612,736
bytes of cgroup peak memory; 60,000 ms of product latency or component wall
time; 25 ms application-frame p95; 1000/30 ms native input-to-application-frame
acknowledgement p95; 2,048 MiB baseline-adjusted GPU peak memory; and 100%
baseline-adjusted GPU utilization p95. Established v4 caps and their provenance
are reused where they exist.

A finite valid exceedance is a measured **NO**. Missing, nonfinite, or invalid
evidence remains **BLOCKED**, not a technical no. This rule applies even when
Electron passes and comparative multi-document metrics remain eligible.

## Exact representative changes

The original five v4 journeys remain exact prefixes. V5 appends:

- `viewer:dynamic-fidelity-scroll` to `nasa-long-document-v1`;
- `annotation:native-property-edit-undo` and
  `annotation:native-snap-transform-120hz` to
  `dense-mixed-editing-v1`;
- `multi-document-session-v1`, with these four fixture IDs in order:
  `bp-single-page-v1`, `nasa-apollo-summary-526-v1`,
  `bp-engineering-sheet-v1`, and `bp-annotation-density-v1`.

The multi-document component executes these commands in one process:

1. `session:open-four-fixtures`;
2. `session:switch-four-fixtures`;
3. `session:edit-dense-rectangle`;
4. `session:close-three-and-recover`.

The close command leaves `bp-annotation-density-v1` open and active. The
retained Rectangle must still have a four-point stroke after the other three
documents close.

## Final representative decision boundary

The v5 comparison boundary is the existing v4 five-family editor and
persistence workload plus these v5 additions:

- a four-document session in one process, including tab switches, an edit, and
  partial resource recovery after three documents close;
- a native Rectangle property edit, presentation acknowledgement, and exact
  one-step undo;
- a maintained inclusive 8 CSS-pixel per-axis snap transform, expressed as an
  L-infinity threshold and derived at run time from the observed CSS-pixel to
  point scale; and
- dynamic fidelity during the locked 120 Hz scroll trajectory, measured by an
  independent 60 Hz observer and three registered crops.

This is the smallest final boundary that exercises the important rendering,
input, editing, history, persistence, multi-document, and live-resource costs
that can change the investment decision. It is sufficient for a controlled
development-runtime comparison. It is not evidence of complete feature parity,
a packaged candidate, an installed application, or release readiness.

## Dynamic fidelity quality family

The dynamic fidelity path retains the frozen v4 trajectory while adding real
capture holds:

- 20 seconds forward through 50 viewport heights, with 19,250 ms of motion and
  three 250 ms zero-input holds;
- 2 seconds paused at the forward apex;
- 10 seconds reverse;
- final page 1.

The analyzer requires 1,921 samples, including both endpoints, at a fixed 60 Hz
observer cadence. Each sample must contain finite values for:

- `visible_page_ready_fraction` — ready visible pages divided by visible pages;
- `visible_raster_ready_area_fraction` — ready visible page-intersection area
  divided by all visible page-intersection area in CSS square pixels;
- `visible_raster_pixel_density` — area-weighted ready raster device pixels per
  visible CSS device pixel, with missing raster area contributing zero.

Missing or nonfinite samples are hard failures. The first two comparison values
are time-series means. Pixel density uses the time-series tenth percentile.
Each final pair forms a higher-is-better GPUI/Electron ratio. The lower 95%
paired-bootstrap bound must be at least `0.95` for every metric.

Three NASA crop registrations cover page 1 at the start, page 15 during the
forward path, and page 29 at the forward apex. Both candidates use fixed 100%
zoom and scale-one presentation. Each painted page must measure 1.00 device
pixel per PDF point within `0.01`, and the paired scales must agree within
`0.01`.

The runner extracts each candidate from a stable presented client screenshot
without resampling it. It downsamples only the unchanged 144-DPI Poppler
reference with Lanczos3 and rejects a smaller reference. Each crop must pass
`bp-cross-engine-binary-scan-fidelity-v2`: filtered luma SSIM at least `0.97`
and dark-content precision, recall, and F1 each at least `0.99`. Before and
after painted-state receipts must prove the page bounds, scroll offset, render
generation, state sequence, and raster readiness stayed stable across capture.

## Implementation and execution status

The v5 workload, runner adapters, paired orchestrator, hard-report validator,
and paired analyzer are implemented. This removes the earlier static
implementation blocker. It does not mean that live qualification is complete
or that execution readiness can be assumed.

Before starting a paid GPU lane, freeze both development-runtime candidate
hashes and the exact fixture/reference identities, pass the deterministic v5
checks, and prove the native X11/XTest target and receipts during excluded
preflight and calibration runs. A decision is valid only after both runtimes
emit complete bundles for the full declared matrix, required GPU samples,
application-frame and milestone receipts, fidelity observations, crop
registrations, persistence evidence, and hard correctness checks. Readiness is
derived from that retained evidence; it is not inferred from the existence of
the runner or analyzer.

Use the same six excluded calibration pairs and 24 through 40 final pairs per
journey in balanced blocks of four. The existing v4 lower-is-better CPU,
memory, native application-frame, product-latency, and GPU families remain in
force for benefit-eligible components.

## Separate lanes

The following work is deliberately outside the v5 inference boundary:

- **Templates** are a separate creation, library, import, preview, migration,
  and storage subsystem. They do not add an independent test of the native
  renderer, input path, or core resource hypothesis, so implementing them would
  delay the investment answer without making its statistics stronger.
- **Remaining annotation variants** remain required for product parity. The
  five representative families already cover the distinct vector, streamed
  freehand, text, measurement, bitmap, history, and persistence seams needed
  for this comparison; extra variants would mostly repeat those cost classes.
- **Signatures** add sanitization, secure storage, privacy, image processing,
  and phone-transfer concerns. The representative Image family does not prove
  the signature workflow, and signature qualification cannot be inferred from
  this test.
- **Packaging, signing, and the updater** concern distributable and installed
  candidates, operating-system integration, release trust, and N-1 replacement.
  An unpackaged Linux development-runtime comparison cannot qualify them.
- **Full accessibility** requires complete keyboard, focus, semantic-tree,
  screen-reader, input-method, and constrained-window checks. Representative
  native controls do not establish full accessibility parity.
- **macOS and Windows** use different render, font, input, accessibility, and
  package stacks. Linux GPU results cannot be used as evidence for Metal or
  Direct3D behavior, native platform integration, or platform-specific defects.
  Shared application flows and platform-specific gates must run later on each
  real operating system.
- **The 180 MB USGS PDF** remains a non-inferential robustness stress lane. Its
  unusually large-sheet workload can diagnose limits, but it is a different
  fixture class and cannot decide the representative comparison.
- **The private Hibbeler corpus** remains optional supplementary evidence. It is
  blocked/not transferred and cannot gate or influence the paid Linux result.

These deferrals limit what the result can support. A positive v5 result funds
continued migration work; it does not prove that the deferred areas will pass.
