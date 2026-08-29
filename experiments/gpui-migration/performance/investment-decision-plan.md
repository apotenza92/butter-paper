# Butter Paper GPUI investment decision plan

Status: predeclared plan. The current GPUI candidate is not ready for the final
paired run.

This plan answers one binary question:

> Should Butter Paper fund and complete the GPUI migration, while retaining the
> Electron application as the rollback implementation until native release
> qualification passes?

This is an investment decision. It is not a claim that the GPUI candidate is
ready to ship. The later shipping decision still requires packaged macOS,
Windows, and Linux qualification.

## Binary rule

The answer is **YES** only when all correctness, reliability, evidence, and
performance gates below pass. The answer is **NO** if any hard gate fails. An
incomplete or statistically inconclusive final run is **NO** for further
migration investment; it is not rounded up to a pass.

Hard correctness gates:

1. Both implementations execute the same semantic workload and reach the same
   canonical document and annotation state.
2. Every required page preview and settled raster passes its density and crop
   oracle. Blank or stale current-generation frames are zero.
3. Rectangle, freehand/highlight, text, measurement, and image/signature
   representatives can be created, selected, edited, undone, redone, saved,
   reopened twice, and independently validated.
4. Existing unknown or unsupported PDF annotations survive both save cycles.
5. No crash, timeout, corrupt save, lost PDF content, or replaced failed pair is
   present in the final evidence set.
6. The exact candidate, fixture, command manifest, raw samples, screenshots,
   semantic snapshots, and host facts are complete and hashed.

Hard reliability gates:

- 100 app-cold launch/open attempts per implementation complete successfully.
- The 95% Clopper-Pearson lower confidence bound is reported for each observed
  success rate. Any observed failure fails the zero-failure gate even if the
  confidence interval is otherwise high.
- Before the final run, GPUI completes 30 of 30 empty-shell starts with native
  first presentation below 5 seconds, Electron completes 10 of 10 100-to-200%
  quality promotions, and GPUI completes 10 of 10 high-zoom tile sequences at
  or above the required density.

Performance gates use paired GPUI/Electron ratios. Lower is better. All absolute
budgets in `protocol.md` must also pass.

| Metric family | Required upper 95% confidence bound |
| --- | ---: |
| Process-tree CPU-seconds | `<= 0.80` |
| Peak and settled process memory | `<= 0.75` |
| Native input-to-present and frame pacing | `<= 1.05` |
| Open, navigation, zoom, annotation, save, and reopen latency | `<= 1.10` |
| GPU allocation, decoded bytes, cache bytes, and upload bytes | `<= 1.15` |

CPU and memory require a material improvement because a framework migration has
large delivery and maintenance cost. The other families are non-inferiority
guards: GPUI may not purchase lower CPU or memory by making interaction, image
quality, or GPU pressure meaningfully worse.

## Frozen candidate and corpus

Freeze the source archive, executable hashes, dependency revisions, fixture
bytes, command manifest, seed, window geometry, display configuration, driver,
and cache policy before calibration. Run as an ordinary unprivileged user. Run
one application at a time on the same host and session.

Required public and generated fixtures:

- `bp-single-page-v1`: small open and shell control.
- `bp-multi-page-v1`: deterministic 100-page navigation and cache control.
- `bp-annotation-density-v1`: 1,000 generated annotations over 100 pages, with
  an empty interaction page and a dense interaction page.
- `bp-annotation-all-v1`: every representative annotation family, appearances,
  page geometry variants, and one untouched unknown imported annotation.
- `nasa-apollo-summary-526-v1`: long-document navigation, continuous scroll,
  cache, and memory.
- `usgs-usa-geology-sheet-v1`: large-sheet zoom, pan, tiling, cancellation, and
  GPU memory.

`private-hibbeler-935-v1` remains blocked/not transferred on paid compute. It is
supplementary local macOS evidence and cannot be a condition for starting or
completing the Linux GPU decision run.

## Minimum comparison-candidate feature set

The comparison candidate is deliberately smaller than the complete migration.
It must perform every distinct expensive class of Butter Paper work, but it
does not need every product variation before the investment decision.

Required viewer work:

- app-cold shell and document open;
- single-page and continuous-page layout with correct per-page geometry;
- virtualized pages and annotation-aware thumbnails;
- navigation, fit modes, timestamped continuous scroll, pan, and zoom;
- bounded visible-tile rendering through 1600% with cancellation, stale-result
  rejection, preview/full/detail quality, and declared cache limits; and
- document close, resource recovery, and declared cache-warm reopen.

Required editor work:

- Rectangle as the complete architecture proof: create, select, hit test, move,
  resize, style, lock, delete, undo/redo, thumbnail projection, import, save,
  and reopen twice;
- Pen or Highlight for streamed point input, smoothing, opacity/blending, and
  path-heavy paint;
- Text Box for text input, shaping/layout, resize, and font persistence;
- Length measurement for derived geometry, scale/unit calculation, labels, and
  endpoint editing;
- Image for bitmap decode, placement, resize, upload, memory, and PDF embedding;
- an untouched unknown imported annotation that must survive both saves; and
- one shared property, selection, history, dirty-state, and error-recovery
  implementation used by every representative family.

Required persistence work:

- real PDF annotation dictionaries and appearance streams rather than a sidecar
  or in-memory simulation;
- validated open and non-destructive safe publication to a new output path;
- independent PDF validation plus canonical semantic and visual comparison;
- two save/reopen cycles; and
- preservation of original page content, boxes, rotation, metadata, and
  unsupported untouched annotations.

Required benchmark integration:

- both implementations consume the same versioned command manifest and locked
  expected state;
- real input is used for native interaction evidence, while direct semantic
  commands remain a separate diagnostic lane;
- every timed operation is correctness-, density-, visual-, and persistence-
  gated before it enters paired statistics; and
- native presentation, process-tree CPU and memory, GPU allocation, uploads,
  cache bytes, and semantic outcomes are captured together.

The comparison candidate does **not** require every geometric tool variation,
snapping mode, multi-selection workflow, template feature, signature transport,
updater, installer, signing flow, or final accessibility/platform qualification.
Those remain migration and release requirements. They do not add a distinct
CPU, memory, GPU, input, text, geometry, bitmap, or persistence workload to the
technical investment comparison.

## Identical journeys

The versioned manifest drives both implementations. It contains semantic
commands, timestamped pointer paths, expected milestones, final canonical
state, and visual crop coordinates. Both runners retain every attempt.

### Viewer journey

1. Start an empty app-cold shell.
2. Open each fixture and wait for preview and settled-current-generation
   milestones.
3. Run the normalized page sequence.
4. Run the 100-to-1600% zoom sequence and the fixed USGS pan path.
5. Scroll NASA forward for 20 seconds across 50 viewport heights, pause for 2
   seconds, reverse for 10 seconds, and return to page 1.
6. Run five navigation/zoom cache-pressure cycles, close, verify recovery, and
   reopen under the declared cache class.

### Annotation journey

On both an empty page and a page with 100 visible annotations:

1. Draw a rectangle for 3 seconds using the same 120 Hz PDF-space path.
2. Select it by hit test, move it for 3 seconds, and resize its east handle for
   3 seconds.
3. Change stroke colour, opacity, width, and style.
4. Undo and redo each committed mutation.
5. Repeat representative create/edit actions for freehand or highlight, text,
   measurement, and image or signature annotations.
6. Verify one history entry per gesture and exact final selection, geometry,
   style, dirty state, and command trace.

Record command receipt, native input timestamp, next native presentation,
preview paint, hit-test and spatial-index work, reducer work, annotation paint,
lost/coalesced input, CPU, process memory, GPU allocation, uploads, cache, and
frame intervals.

### Persistence journey

Apply the fixed annotation command set, save, validate with an independent PDF
reader, reopen, save again, and reopen again. Compare canonical Butter Paper
state, native PDF annotation dictionaries, fixed raster crops, page content,
page boxes, and the untouched imported annotation after both cycles. A sidecar,
mock writer, or in-memory reopen does not satisfy this journey.

## Visual and semantic oracles

- Preview raster density is at least `0.75`; settled density is at least `1.0`.
- Settled current-generation output remains stable for 250 ms with no required
  tile pending.
- Fixed PDF crops meet `SSIM >= 0.985` after documented colour conversion.
- Annotation shape intersection-over-union is at least `0.98`.
- Control-point and annotation bounds differ by no more than 1 device pixel at
  standard scale or 2 at fractional scale.
- Reviewed colour deltas are `Delta E <= 2`; fixed text crops must have the
  expected optical-character-recognition text.
- Canonical semantic snapshots and unknown-annotation preservation match
  exactly. Semantic correctness overrides pixel similarity.

## Sampling and analysis

1. Before any timed sample, pass the explicit native launch/open, full editor,
   and full persistence untimed preflights for both implementations. Do not let
   evidence from another scenario substitute for one of these six proofs.
2. Require all 11 timed representative scenarios and complete 31-command
   coverage for both runners. A smaller development subset is not decision
   evidence.
3. Run six randomized calibration pairs for every required journey. Do not use
   calibration samples in the final estimate.
4. Estimate the variance of paired log ratios for the primary CPU, memory,
   input-to-present, and product-latency metrics.
5. Select a final sample count from the predeclared power calculation, clamped
   to 24 through 40 pairs. Record the calculation before the final run.
6. Use seed-recorded randomized AB/BA blocks. Keep the process pair as the
   sampling unit. Do not treat frames within one process as independent pairs.
7. Report raw values, arithmetic median, p50/p95/p99/maximum, geometric mean
   paired ratio, median paired ratio, and a deterministic 100,000-resample 95%
   paired bootstrap confidence interval over log ratios.
8. Report Clopper-Pearson reliability intervals, order effects, fixture effects,
   and a sensitivity analysis that includes every failed or timed-out attempt.
9. Never remove an outlier or replace a failed pair. A predeclared independent
   environmental abort remains visible and does not become a successful pair.

## Required implementation before paid calibration

The current candidate must complete these local milestones first:

1. Generate and independently verify the four `bp-*` fixtures and their
   semantic, native-PDF, and visual oracles.
2. Implement the real GPUI Rectangle vertical slice: domain state, hit testing,
   pointer capture, selection, move, resize, properties, undo/redo, batched
   overlay paint, accessibility projection, and compatible save/reopen.
3. Add the remaining representative annotation families used by the manifest.
4. Replace the GPUI 4096-pixel full-page high-zoom cap with bounded visible
   tiling and current-generation cancellation.
5. Instrument GPUI startup through render entry, callback schedule/fire,
   map/expose, first submit, first native presentation, and interactive state.
6. Correct Electron's retained-low-resolution 100-to-200% quality-promotion
   failure and add its deterministic regression test.
7. Implement the manifest-driven `bp-perf-v3-decision-3` runners, native event
   capture, visual/semantic oracle collection, absolute-budget enforcement, and
   statistical decision report.

Items 2 through 5 stay inside this migration experiment. Item 6 changes the
maintained Electron product and therefore needs explicit scope authorization.

## Paid Linux GPU lease

Provision only after every local and GPU smoke start gate passes.

- Expected setup and verification: 10 minutes.
- Expected calibration: 15 minutes.
- Expected final execution: 35 minutes.
- Expected collection and validation: 10 minutes.
- Expected task duration: 70 minutes.
- Task time limit: 105 minutes (`ceil(1.5 * 70 minutes)`).
- Cleanup grace: 15 minutes.
- Absolute TTL: 120 minutes.
- Target price: USD 0.76/hour for the previously qualified RTX 4000 Ada size.
- Absolute estimated compute maximum: USD 1.52, excluding tax.

Provision with an automatic 120-minute reaper from an independent control path.
Destroy the droplet and any attached billable storage immediately after valid
evidence collection or a conclusive blocker. Verify absence through the provider
API. Early completion is valid only when expected report counts, completion
events, artifact hashes, and resource destruction all pass.

## Current disposition

The prior Linux GPU run is useful directional evidence: GPUI used substantially
less process memory and opened the NASA PDF faster in valid pairs. It is not a
decision run. It had GPUI first-frame stalls, a failed GPUI navigation pair,
zero valid zoom pairs, no native presentation trace, and no annotation or
persistence workload.

Under the binary rule above, the present answer is **NO**. This means “do not
fund the full migration from the current evidence,” not “GPUI can never be the
right implementation.” The answer can change only after the required identical
functionality and correctness gates make the final paid run valid.
