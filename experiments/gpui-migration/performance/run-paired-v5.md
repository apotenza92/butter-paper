# v5 paired GPU orchestration

`run-paired-v5.mjs` plans and executes the same-host Electron versus GPUI v5
comparison. It does not provision or destroy a GPU host. The root orchestrator
must create one paid host, install an independent expiry mechanism, transfer
the reviewed artifacts, and destroy the host after evidence collection.

The runtime is an optimized, unpackaged development candidate. It is not an
installed or packaged release qualification. Linux GPU results do not replace
the separate macOS and Windows shared-product tests.

## Plan before provisioning

Plan mode is the default and launches no application process. Supply the
current provider hourly price so the plan records a useful cost ceiling.

```bash
node experiments/gpui-migration/performance/run-paired-v5.mjs \
  --plan \
  --output /absolute/results/bp-v5 \
  --plan-output /absolute/results/bp-v5-plan.json \
  --fixture bp-single-page-v1=/absolute/fixtures/single-page.pdf \
  --fixture nasa-apollo-summary-526-v1=/absolute/fixtures/nasa.pdf \
  --fixture bp-engineering-sheet-v1=/absolute/fixtures/engineering.pdf \
  --fixture bp-annotation-density-v1=/absolute/fixtures/annotation-density.pdf \
  --fixture bp-annotation-all-v1=/absolute/fixtures/annotation-all.pdf \
  --reference-crop-directory /absolute/fixtures/reference-crops-v5 \
  --electron /absolute/repo/node_modules/electron/dist/electron \
  --gpui-binary /absolute/repo/experiments/gpui-migration/gpui-gallery/target/release/butter-paper-gpui-gallery \
  --electron-candidate-artifact /absolute/candidates/electron-optimized-candidate-v4.json \
  --gpui-candidate-artifact /absolute/candidates/gpui-optimized-candidate-v4.json \
  --electron-candidate-sha256 <exact-manifest-sha256> \
  --gpui-candidate-sha256 <exact-manifest-sha256> \
  --hourly-usd <current-hourly-price>
```

The default matrix contains:

- Six representative journeys.
- Five frozen representative PDF fixtures. The multi-document journey uses an
  ordered subset of four.
- Six excluded calibration pairs per journey.
- Twenty-four final pairs per journey in balanced blocks of four.
- Twenty-one repeated benefit components.
- One property correctness run per candidate outside benefit metrics.
- 1,262 process launches: 1,260 measured and two correctness-only.

The checked-in duration assumptions estimate 24,452,000 ms, approximately 6
hours 48 minutes. The default 35 percent task headroom gives a recommended task
limit of 33,010,201 ms. A 15-minute cleanup grace gives a recommended absolute
lease time-to-live (TTL) of 33,910,201 ms, approximately 9 hours 25 minutes.
The plan recalculates these values and both expected and maximum cost from the
supplied hourly price. The 120-second per-process timeout ceiling is also
reported as a diagnostic worst case; it is not the recommended paid TTL.

Use repeated `--expected-component-ms <component>=<milliseconds>` options when
calibration data supports a better duration estimate. Review the resulting
`selected_task_limit_ms`, `selected_absolute_lease_ttl_ms`, and
`maximum_cost_usd` before provisioning.

The plan JSON includes every exact runner command, environment value, raw
report path, hard-report path, candidate hash, workload hash, reference hash,
and acceptance check.

## Execute a reviewed plan

Execution requires the reviewed task limit and lease TTL. The TTL must include
the cleanup grace. The independent infrastructure reaper must use the same TTL.

```bash
node experiments/gpui-migration/performance/run-paired-v5.mjs \
  --execute \
  --task-limit-ms <selected_task_limit_ms> \
  --lease-ttl-ms <selected_absolute_lease_ttl_ms> \
  --output /absolute/results/bp-v5 \
  --fixture bp-single-page-v1=/absolute/fixtures/single-page.pdf \
  --fixture nasa-apollo-summary-526-v1=/absolute/fixtures/nasa.pdf \
  --fixture bp-engineering-sheet-v1=/absolute/fixtures/engineering.pdf \
  --fixture bp-annotation-density-v1=/absolute/fixtures/annotation-density.pdf \
  --fixture bp-annotation-all-v1=/absolute/fixtures/annotation-all.pdf \
  --reference-crop-directory /absolute/fixtures/reference-crops-v5 \
  --electron /absolute/repo/node_modules/electron/dist/electron \
  --gpui-binary /absolute/repo/experiments/gpui-migration/gpui-gallery/target/release/butter-paper-gpui-gallery \
  --electron-candidate-artifact /absolute/candidates/electron-optimized-candidate-v4.json \
  --gpui-candidate-artifact /absolute/candidates/gpui-optimized-candidate-v4.json \
  --electron-candidate-sha256 <exact-manifest-sha256> \
  --gpui-candidate-sha256 <exact-manifest-sha256> \
  --hourly-usd <current-hourly-price>
```

Execution sets `BP_PERF_REQUIRE_NVIDIA=1`. A missing baseline, run, or adjusted
GPU sample fails the launch. The orchestrator also fails closed on a missing or
forged command receipt, changed candidate or workload bytes, changed fixture,
changed transferred reference crop, incomplete bundle, or global task timeout.

After each runner exits, the v5 orchestrator stamps the raw report with a
unique authenticated launch binding. The binding contains the exact schedule
index, phase, pair, position, implementation, component, lane, candidate and
fixture identities, report path, and wall-clock and monotonic boundaries. The
normalized hard report, bundle component, property reference, final hard
reference, and each dynamic capture carry the same identity. Capture IDs,
paths, file identities, and capture intervals must be unique per launch.
Byte-identical decoded page pixels remain valid when their independent capture
provenance is unique.

The one-time Electron property check must either pass the normal one-undo
contract or retain the exact
`electron-numeric-property-input-blur-duplicate-history-v1` outcome. The GPUI
property check must restore the canonical 1.5-point stroke with one undo.

The Electron multi-document control may retain only the predeclared
`electron-multi-document-second-nasa-visible-pages-empty-v1` outcome. That
receipt requires the second NASA activation, an empty visible-page list, zero
queued and in-flight raster work, no raster, no error, and the exact explicit
list of missing benefit metrics. GPUI must pass the normal multi-document path.

Every GPUI multi-document hard report must also expose all seven normalized
safety measurements. The normalized measurement object must include
`product_wall_or_latency_source` with `product-latency` or `component-wall`.
The final analyzer must aggregate the maximum across retained final GPUI
multi-document reports, compare it with the frozen contract budgets, and emit
one of two distinct outcomes: `measured-no` for a valid finite exceedance, or
`blocked-not-decision-ready` for missing, nonfinite, or structurally invalid
evidence. The resolved live gate confirms that classification was completed;
it is not a budget-pass flag.

## Retained paths

The output directory contains:

- `run-manifest-v5.json` and `run-manifest-v5.sha256`: finalized launch,
  bundle, outcome, duration, and cost evidence. The checksum authenticates the
  exact completed manifest used to build the analyzer input.
- `<launch>.json`: each exact raw Electron or GPUI runner report.
- `<launch>-hard-report-v5.json`: normalized exact reports for the four v5
  hard components.
- Dynamic-fidelity screenshots, native candidate crops, and registered
  reference crops. The source references remain in the declared transferred
  reference tree.
- `<bundle>-bundle-manifest-v5.json`: ordered component evidence for one
  implementation side of one pair.
- `analyzer-input-v5.json`: verified bundle references, one-time property
  correctness reports, 180 matched view-state pair receipts, every final hard
  report, and 24 paired dynamic-fidelity quality inputs for
  `analyze-paired-v5.mjs`.

## Produce the funding decision

Run the final analyzer only after `run-paired-v5.mjs --execute` completes:

```bash
node experiments/gpui-migration/performance/analyze-paired-v5.mjs \
  --input /absolute/results/bp-v5/analyzer-input-v5.json \
  --output /absolute/results/bp-v5
```

The analyzer authenticates the input against the completed run manifest and
checksum. It revalidates both candidate manifests and executables, then
rehashes every referenced bundle, raw report, hard report, and PNG artifact.
It also validates the frozen workload and crop identities, exact reviewed-seed
pair order, one host and GPU identity, strict NVIDIA sample arithmetic, exact
Electron pass-or-defect allowances, GPUI correctness, recomputed registered
crop metrics and presentation scales, shared v4 resource statistics, and
eligible v5 benefit metrics. It writes deterministic
`paired-decision-v5.json` and human-readable `paired-decision-v5.md` files.

The analyzer reconstructs every hard semantic report and its CPU, cgroup peak
memory, product latency, application-frame, native acknowledgement, and
baseline-adjusted GPU measurements from the retained raw iterations, events,
cgroup observations, frame/acknowledgement samples, and NVIDIA samples. An
embedded hard-report convenience projection is untrusted and must exactly
match this independent reconstruction.

Benefit eligibility also requires the shared
`x11-present-complete-after-xtest-v1` observer receipt. Semantic/direct-model
lanes are correctness-only. Electron `requestAnimationFrame` and GPUI draw
submission receipts are diagnostics with different end boundaries; the runner
must not aggregate their measurements. A missing shared receipt blocks the run
and the analyzer independently rejects retained evidence that claims otherwise.

Benefit eligibility also requires the two live view-state checkpoints defined
in `protocol-v5.md`. The executor stops as soon as paired Electron and GPUI
window/viewport geometry, layout, zoom, sidebar state, or active document do not
match. The analyzer rebuilds the receipts from raw events and verifies all 180
pair assessments before it evaluates a benefit metric.

`analyzer-input-v5.schema.json` and `final-decision-v5.schema.json` publish the
machine-readable input and output boundaries. Structural crop, scale, path,
count, or identity failures produce `BLOCKED`. Only valid measured metric or
absolute-safety regressions produce `NO`.

A complete eligible comparison ends with one `YES` or `NO` on funding the
continued GPUI migration. Missing, forged, unbalanced, or otherwise invalid
evidence ends as `BLOCKED`; the analyzer never guesses `NO`. The decision is a
whole-stack comparison of Electron/PDF.js with GPUI/PDFium. It does not claim
packaged release qualification. macOS visual capture and Windows platform
qualification remain not run in this Linux lane. The private Hibbeler corpus
remains blocked-not-transferred.

## Focused checks

```bash
node --test experiments/gpui-migration/performance/run-paired-v5.test.mjs
node --test experiments/gpui-migration/performance/analyze-paired-v5.test.mjs
node --test experiments/gpui-migration/performance/analyze-paired-v5.e2e.test.mjs
npx prettier --check \
  experiments/gpui-migration/performance/run-paired-v5.mjs \
  experiments/gpui-migration/performance/run-paired-v5.test.mjs \
  experiments/gpui-migration/performance/analyze-paired-v5.mjs \
  experiments/gpui-migration/performance/analyze-paired-v5.test.mjs \
  experiments/gpui-migration/performance/analyze-paired-v5.e2e.test.mjs \
  experiments/gpui-migration/performance/analyzer-input-v5.schema.json \
  experiments/gpui-migration/performance/final-decision-v5.schema.json \
  experiments/gpui-migration/performance/run-paired-v5.md
node --check experiments/gpui-migration/performance/run-paired-v5.mjs
node --check experiments/gpui-migration/performance/analyze-paired-v5.mjs
```
