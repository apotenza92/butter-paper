# GPUI performance runner

The predeclared binary investment question, expanded corpus, identical viewer,
annotation and persistence journeys, statistical rule, and paid-compute start
gate are in [`investment-decision-plan.md`](investment-decision-plan.md). Do not
start the final paid comparison until that document's local readiness gates
pass.

Frozen v5 retains `native-x11-present-observer.c` and
`x11-present-observer.mjs` with its reviewed XPresent contract. V6 uses
`native-x11-damage-observer.c` through `x11-damage-observer.mjs`. The v6
observer requires `libxdamage-dev` on Debian/Ubuntu, runs as an independent
process, and measures `CLOCK_MONOTONIC` XTEST injection to the first matching
server-side `DamageNotify` on the target drawable after a damage reset. The
observer injects the actual benchmark click, key, pointer, or wheel action and
retains an action token and sequence. The exact held dynamic-fidelity helper
performs the same v6 binding without changing its 3,841-sample trajectory. A
timeout is a blocked sample. The receipt explicitly states that drawable
damage is not presentation completion or physical scanout.

The additive v6 workload and 624-launch schedule are documented in
[`run-paired-v6.md`](run-paired-v6.md). V6 moves every semantic diagnostic
component out of benefit inference and leaves the frozen v5 artifacts intact.
The v6 runner supports authenticated group-aligned resume. The v6 analyzer
authenticates retained artifacts and emits `YES`, `NO`, or `BLOCKED`.
Before full execution, `run-paired-v6.mjs --qualify` runs one two-launch
`small-shell-open/open-pdf` native pair and writes an authenticated paid-GPU
qualification receipt. Full `--execute` requires that receipt and revalidates
its exact candidates, corpus, NVIDIA/X11/Vulkan/D-Bus binding, raw reports,
server-side XDamage evidence, and matched view state. Qualification uses its own
8-minute task and 30-minute absolute TTL ceilings; it does not provision a
host. Run `--qualify` and `--execute` inside one enclosing `dbus-run-session`;
separate wrappers create a different bus ID and fail closed. The runner also
binds the X server PID/start identity and each candidate's exact active NVIDIA
renderer UUID, re-seals both recursive optimized candidate closures immediately
before every launch, and preserves immutable time/cost deadlines across resume.
Before either application starts, the paid lane runs a disposable child-cgroup
probe and requires exact cgroup-v2 CPU time plus `memory.peak`. Missing
`memory.peak` blocks the run even when CPU accounting works. Summed
process-tree RSS is diagnostic only and is never substituted into the decision
metrics. See [`run-paired-v6.md`](run-paired-v6.md) for the host checks and
remediation evidence.

The additive `decision-contract.mjs`, `decision-contract.schema.json`,
`decision-statistics.mjs`, and `decision-evaluator.mjs` files make the frozen
`bp-perf-v3-decision-3` policy executable as deterministic pure functions. The
contract deliberately reports `blocked-candidate-parity` and `executable:
false`; it does not upgrade or authorize the current v2 runners.

## Local V6 correctness preflight

`run-local-correctness-v6.mjs` runs only the frozen 24 V6 correctness
launches: 22 semantic launches and the two native property launches. It uses
the same Electron and GPUI component runners as paid V6, but it does not use a
paid qualification receipt, require NVIDIA, enable the common Damage observer,
collect benefit metrics, or authorize a migration decision. The separate
`local-correctness-manifest-v6.json` retains each raw report, property hard
report, candidate seal, and artifact SHA-256. It identifies itself as
`local-development-host-correctness-only-non-decision` and always has
`decision_eligible: false`.

Use a new output directory and one existing local X11/D-Bus session:

```sh
dbus-run-session -- node experiments/gpui-migration/performance/run-local-correctness-v6.mjs \
  --output test-results/local-correctness-v6-<run-id> \
  --electron <optimized-electron-executable> \
  --gpui-binary <optimized-gpui-executable> \
  --electron-candidate-artifact <electron-candidate-manifest.json> \
  --gpui-candidate-artifact <gpui-candidate-manifest.json> \
  --electron-candidate-sha256 <sha256> \
  --gpui-candidate-sha256 <sha256> \
  --fixture nasa-apollo-summary-526-v1=<nasa.pdf> \
  --fixture bp-engineering-sheet-v1=<engineering.pdf> \
  --fixture bp-annotation-density-v1=<density.pdf> \
  --fixture bp-annotation-all-v1=<all-annotations.pdf>
```

The preflight validates both optimized candidate closures and the exact four
fixture hashes before launch. It runs one application at a time and stops at
the first unexpected failure. Exact reviewed Electron baseline defects remain
`correctness_passed: false`, but the preflight retains them and continues so it
can collect the remaining correctness evidence. The final status is
`completed-with-known-baseline-defects`, never an unqualified pass.

GPUI launches freeze `GPUI_X11_SCALE_FACTOR=1` for the native property lane.
The manifest records physical memory and desktop-session diagnostics. A local
Electron portal-chooser PID mismatch or a memory-related high-zoom failure is
an ordinary fail-closed host result; this preflight does not skip or allow
either failure. Raw resource and GPU diagnostics can still appear inside the
runner reports, but the local manifest never promotes them to benefit evidence.

Decision revision 3 preserves the revision-2 shaped Text auto-size and natural,
page-contained Image placement corrections. It also selects the no-fill
Rectangle at the maintained quarter-edge point, away from resize handles. The
one-device-pixel transform tolerance and native 64-point Highlight geometry and
smoothing oracle remain unchanged.

`comparison-workload.json` is the shared, versioned command manifest for that
decision. It freezes the viewer, rectangle, highlight, text, length, image,
unknown-annotation, and two-cycle save/reopen journeys. Its command stream,
milestone stream, expected state, and complete artifact have deterministic
SHA-256 identities. `comparison-workload.schema.json` documents its wire shape,
while `comparison-workload.mjs` validates the semantic obligations and hashes.
Run the following command to print the current machine-readable runner coverage:

```sh
node comparison-workload.mjs > /tmp/butter-paper-comparison-coverage.json
```

The command exits with status 1 while either runner lacks any required command
or milestone. GPUI now declares 30 of 31 commands ready after live native
editor, final editor-frame, and WGPU cache-pressure qualification. Its remaining
blocker is the USGS high-zoom pan stress command. Electron remains at 27 of 31,
so the aggregate command still exits with status 1. Each development
report embeds the same coverage record. A successful implementation-specific
iteration cannot override incomplete global coverage.

`matched-editor-create.mjs` is the fail-closed readiness seam for the frozen
Text, Length-scale, Length-create, and Image-create commands. Its assessment
functions separate exact semantic state from live input, layout, paint, and
graphics processing unit (GPU) evidence. GPUI `editor-create` now replays the
frozen Text, scale, Length, and Image actions through native XTEST input and
proves their exact committed state. Image annotations and high-zoom PDF tiles
use GPUI's production `paint_image` path. A successful WGPU paint is retained as
an atlas-upload queue receipt with the exact decoded byte count; the report
states that this is not a physical-bus byte measurement. Prepared
red-green-blue-alpha (RGBA) bytes alone do not count as a GPU upload. The GPUI
`editor-workload` also proves final Image resize/history and thumbnail state.
Electron now has
a focused `editor-create` CDP diagnostic which
selects the maintained Text, scale, Length, and Image controls, enters the
locked values, loads the exact checker PNG through an intercepted file chooser,
and observes the committed document model plus live SVG layout. Its exact-state
gate now accepts the maintained Text auto-size, selected scale precision,
quantized Length geometry, and natural Image placement. It still rejects the
absent per-image GPU upload-byte receipt. Set `BP_ELECTRON_IMAGE_TRACE=1` for an
untimed diagnostic trace around the locked checker-image action. The trace
summary distinguishes exact 512 by 384 image decode, compositor resource
preparation, unattributed GPU upload events, and physical transfer bytes. An
Xvfb run recorded the checker in `SoftwareImageDecodeCache` and compositor
preparation, but no checker-attributable GPU upload event. Chromium's
[`GpuImageDecodeCache` trace points](https://chromium.googlesource.com/chromium/src/+/refs/tags/144.0.7551.1/cc/tiles/gpu_image_decode_cache.cc#1559)
have no per-image identity or byte-count arguments. The runner therefore does
not substitute decoded RGBA bytes, texture allocation, timing proximity, or an
unrelated `SharedImage` size for upload evidence. These are measured product or
instrumentation differences, not runner passes. This seam does not add or
change any command in the 31-command manifest. Electron coverage now recognizes
both Image commands and their live-proven decode, create, paint, aspect-ratio,
history, and thumbnail milestones. `image:create` remains blocked only on
`bitmap-upload-recorded`; `image:resize-history` remains blocked only on
`upload-byte-count-recorded`.

The Electron `cache-pressure` runner now implements the five frozen
`navigate`, `zoom`, `pan`, `return-page-1` cycles. It samples visible surfaces
and the maintained caches on every animation frame, requires a real cancelled
or obsolete render, rejects visible stale presentation, checks the current page
after every cycle, and compares a SHA-256 identity of the returned page-1
raster. `LocalPdfSession` exposes separate page-URL, decoded-bitmap, and
thumbnail byte counters with their production limits; aggregate cache bytes are
not misreported as decoded bytes. The pure fail-closed assessor and session
accounting tests pass. The new live replay has not run on the rebuilt desktop
candidate, so none of its non-upload milestones are promoted yet. Even after
that semantic replay passes, `upload-byte-count-recorded` requires a physical
GPU receipt and cannot be inferred from decoded bytes or total GPU memory.

`electron-untimed-correctness.mjs` is the separate, timing-ineligible
correctness seam for the manifest commands that do not belong in an expensive
timed replay. It builds actions only from generic test-mode hooks on the
maintained Electron store and tool implementations. It does not contain a
second annotation model. Its exact edit/history plans cover
`highlight:edit-history`, `text:edit-resize-history`,
`length:edit-endpoint-history`, and `image:resize-history`. Each plan captures
the maintained document and `{past, future, currentRevision, savedRevision}`
history before and after actions. The evaluator requires the exact final
geometry or text, the exact pre-commit state after Undo, the exact committed
state after Redo, and the expected history depths. Highlight compares the
maintained smoothed path with its own translated pre-edit path; the manifest's
four input control points are not treated as the smoothed stored path.

Current result categories for this seam are:

- Passed deterministic checks: plan extraction from the frozen manifest,
  exact maintained-state/history assessment, action ID remapping, and
  fail-closed blocker classification. One live Xvfb `editor-workload` replay
  passed every Highlight, Text, and Length edit/history milestone. Image passed
  exact 180 by 135 bounds and aspect ratio, undo/redo, thumbnail paint, and
  512 by 384 decode/presentation. A separate clean Xvfb replay also passed
  `rectangle:repeat-dense`: the maintained page started with the exact frozen
  100-Rectangle geometry, all three 361-sample source streams were received,
  the final 101-Rectangle state and full history replay were exact within the
  declared one-device-pixel geometry oracle, the maintained spatial query was
  bounded and hit the target, and both overlay and thumbnail paint were current.
  This semantic CDP evidence is explicitly not timing eligible.
- Failed commands: none after applying the manifest's Image requirement to the
  exact replacement bounds and ratio. Normal maintained images do not require
  the signature-only `aspectRatioLocked` field.
- Blocked commands: Image create and resize remain blocked only on their exact
  per-image GPU upload-byte receipts. The live Electron `persistence-workload`
  now applies the canonical five-annotation state through one generic
  maintained-store hook, then uses the production Save As and reopen paths for
  two cycles. Each retained PDF passes `qpdf --check` and `pdfinfo -box`; the
  independent probe finds the exact 11 expected native annotations, preserves
  the unknown annotation dictionary and normal appearance stream byte-for-byte,
  preserves page content, boxes, rotation, and metadata, and produces identical
  cycle-one/cycle-two Poppler crops. Canonical Rectangle and Highlight now have
  valid normal Form XObject appearance streams. Butter Paper serializes each
  maintained page scale into an allowlisted page dictionary entry and validates
  it during document inspection, so the calibrated scale and exact `3.50 m`
  Length state survive both reopens. The live lane passes all eight commands,
  which are now promoted as Electron diagnostic coverage. Live PDFs, validator
  output, structural hashes, and crops are retained only below ignored
  `results/electron-persistence-*` directories.
- Not run: decision-eligible hardware-GPU upload tracing.

Run the focused deterministic gate checks with:

```sh
node --test electron-untimed-correctness.test.mjs
```

Prepare the two large public inputs before starting paid compute:

```sh
node public-corpus.mjs \
  --cache /absolute/path/to/content-addressed-public-corpus \
  --id all
```

The fetcher accepts only the locked NASA and USGS identities. It downloads to a
temporary file, checks the declared byte count and SHA-256, and then publishes
the file under its digest without replacing an existing cache entry. A source
change or corrupt cache fails closed. Never transfer the private Hibbeler file
to the paid Linux lane.

`gpui-runner.mjs` measures a deterministic scenario emitted by the standalone
Butter Paper GPUI application. It does not alter production sources or use the
Electron application.

The GPUI runner implements open, single/continuous layout, page navigation,
zoom, high-zoom pan, cache pressure, close/reopen, `annotation-create`, and
`continuous-scroll` diagnostics. The manifest-backed scenarios use
`bp-perf-v2-development-subset-7`. They read their exact commands and
milestones from `comparison-workload.json`. The runner rejects the wrong public
fixture hash before launch and rejects a nominal `scenario-complete` event when
any exact per-command milestone is absent. Missing NASA or USGS bytes are
reported as a locked-corpus blocker before launch. `annotation-create` drives the real
typed adapter through the 361-sample Rectangle and Highlight pointer streams.
`continuous-scroll` drives the real continuous `ScrollHandle` for the frozen
20-second forward, 2-second pause, and 10-second reverse path. The frozen v3
development subset still requires zero samples with a missing visible raster.
The v5 decision lane redistributes 750 ms of the forward phase into three real
250 ms zero-input screenshot holds without changing the 32-second duration or
3,841 inclusive 120 Hz samples. It compares native presented crops at fixed
100% zoom and one device pixel per PDF point. Candidate resampling is forbidden;
only the unchanged Poppler reference is downsampled with Lanczos3. The v2 scan
gate requires filtered SSIM of 0.97 plus dark-content precision, recall, and F1
of 0.99 for every stable crop.
The v4 representative contract instead requires at least one visible-raster
readiness observation. It retains the total observation count, missing-raster
count, and readiness rate as diagnostic evidence because Electron and GPUI
sample different presentation boundaries. The v4 hard gates still require the
exact input schedule, a bounded virtual page window, and the final current page.
The GPUI native path counts all 4,800 physical receipts at the full viewport,
coalesces their 50-viewport path to frame updates, and prunes queued page-surface
work that is no longer visible. The viewer scenarios additionally gate the exact normalized page and
zoom sequences, a bounded 1600% tile path, decoded and tile cache byte limits,
and resource release before a declared-warm reopen. These are semantic
development actions from inside the app, not
operating-system native input replay or decision-eligible timing.

The native X11 continuous-scroll lane remains decision-ineligible until a live
optimized candidate run emits both application-frame interval p95 and the GPUI native-input to
platform-draw acknowledgement proxy. Its
replay evidence now distinguishes 3,600 timed intervals from 4,800 physical
wheel clicks, rejects coalesced wheel deltas, measures the actual peak offset
against 50 viewport heights with a five-percent tolerance, and requires native
input to return the viewer to page 1. The application does not force the final
page. A current live X11 run has not yet proved those gates.

The v4 native timing gate does not claim physical scanout. Electron records a
trusted DOM native-event receipt to the next `requestAnimationFrame` callback.
GPUI records its input-latency histogram through platform draw submission. The
analyzer names these application-frame/draw acknowledgement proxies and fails
closed when either native component summary or its non-scanout receipt metadata
is absent.

The GPUI native X11 `annotation-transform` lane is a real three-stage replay.
XTEST first creates `rectangle:create-sparse`, then moves it from the
decision-3 no-fill top-edge/body point, then resizes from the east handle that
the GPUI app reports after the move. All three paths use the manifest's
inclusive 120 Hz schedule. The application requires one history transaction
for create, move, and resize; proves that the edge hit selected the rectangle;
and checks final geometry against the manifest's standard-scale
one-device-pixel tolerance. Completion waits for the matching rectangle and
scene revision to enter GPUI's platform draw. The JavaScript runner recomputes
the device-pixel error independently and rejects missing draw or history
receipts. This is a platform-draw submission receipt, not proof of physical
scanout. `rectangle:properties-history` remains a separate untimed semantic
correctness gate.

The subset still lacks native-presentation metrics, per-process GPU allocation,
visual crop oracles, and the rest of the scenarios in `protocol.md`, so its
reports remain development diagnostics rather than beta or stable acceptance
evidence. On Linux, the runners also record cgroup-v2 process-tree CPU-seconds
and memory peak, and sample an otherwise-idle NVIDIA GPU. Paid v6 execution
requires both cgroup metrics to be proven before launch. The NVIDIA samples
cover the whole device, including the X server and window manager.

The separate GPUI persistence semantic scenario now provides a stronger
writer-side oracle. It compares the complete typed annotation collection and
every untouched annotation dictionary/appearance graph across two save/reopen
cycles. It also runs `qpdf`, `pdfinfo`, and a fixed 72-DPI `pdftoppm` crop; the
saved crop must change from the source and remain pixel-exact across cycle 2.
An explicit evidence-directory API retains both non-overwritten PDFs and the
three crops. This does not yet satisfy the workload's application-to-application
visual milestones: Poppler is an independent persistence validator, not a GPUI
window capture or an Electron/GPUI SSIM comparison.

Opt in to retained process evidence only for the persistence scenario:

```sh
node gpui-runner.mjs \
  --scenario persistence-workload \
  --pdf /absolute/path/to/bp-annotation-all-v1.pdf \
  --evidence-directory /absolute/path/to/new-evidence-directory \
  --output /absolute/path/to/persistence-report.json
```

Each iteration uses a distinct `iteration-NNN` directory. Existing PDF or crop
artifacts make the run fail closed. Without `--evidence-directory`, the runner
keeps only hashes and validator output in its diagnostic report and removes the
disposable cache. Retained evidence remains diagnostic-only and never makes
timing decision-eligible by itself.

The GPUI application must emit one JSON object per stdout line and exit after
the scenario. Each event must contain:

```json
{
  "schema_version": 1,
  "event": "first_page_visible",
  "t_ms": 184.2,
  "duration_ms": 91.7
}
```

`duration_ms` is optional. When present, the runner groups and summarizes it by
event name. Any non-empty stdout line that does not match this schema makes the
iteration fail so logging cannot silently corrupt benchmark evidence. Normal
diagnostics belong on stderr.

## Run

Build the independent GPUI app bundle first, then run:

```sh
node gpui-runner.mjs \
  --scenario open-pdf \
  --pdf /absolute/path/to/verified-public-fixture.pdf \
  --iterations 5 \
  --output results/gpui-open-pdf.json
```

Options:

- `--scenario` and `--pdf` are required.
- `--iterations` defaults to 3 separate application processes.
- `--output` defaults to `gpui-<scenario>.json` beside the runner.
- `--timeout-ms` defaults to 120000 for each iteration.
- `--binary` can override the expected executable in
  `../gpui-gallery/target/Butter Paper GPUI.app/Contents/MacOS/ButterPaperGPUI`.

For each process, the runner sets `BP_GPUI_PERF_SCENARIO` and
`BP_GPUI_PERF_ITERATION`. It samples the root process and all descendants with
`/bin/ps` every 100 ms. Each iteration gets a new cache directory beside the
report, so the measured run is cold. The runner records the cache file and byte
counts, then removes that disposable directory. A timeout terminates the
isolated process group. It also passes `-ApplePersistenceIgnoreState YES` so a
previous crash dialog cannot become a benchmark result; the gallery filters
that macOS launch pair before treating the remaining arguments as PDF paths.

The runner also preflights the sibling `butter-paper-pdf-worker` executable and
the host-specific, checksum-pinned development PDFium 7881 artifact. The
development fetch helper verifies its extraction receipt before reuse. The
runner sets `BP_PDF_WORKER_EXE` and `BP_PDFIUM_LIBRARY` for each child and
records the byte size and SHA-256 of both runtime artifacts in provenance.
Missing or modified runtime artifacts fail before a measured application
launch.

## Report contents

The JSON report retains the raw application events, stderr, invalid stdout, and
process-tree samples for every iteration. Aggregate results include:

- wall-clock and application event timing distributions;
- grouped `duration_ms` event distributions;
- frame callback interval distributions and counts above 8.33, 16.67, and
  33.33 ms;
- peak and median process-tree CPU usage;
- peak and median resident set size (RSS);
- PDF path, byte count, modification time, and SHA-256;
- executable identity, Git revision, Node runtime, CPU, memory, OS, and host
  provenance.

Use the same PDF bytes, scenario contract, iteration count, machine, power
state, display state, and sampling interval when comparing GPUI with Electron.
The paired orchestrator defaults to the complete representative timed set:
cold launch/open, viewer layout, navigation, zoom, high-zoom pan, continuous
scroll, cache pressure, close/reopen, annotation create, annotation transform,
and editor create. An explicit `--scenarios` subset remains fail-closed for the
investment decision; it cannot silently reproduce the historical seven-scenario
diagnostic.

Before any warmup or measured process, the orchestrator requires six retained,
untimed preflights: native launch/open, full editor correctness, and full
persistence correctness for Electron and GPUI. Native launch/open must use the
`native-x11-xtest` lane and emit its own matching proof. Evidence from another
scenario does not satisfy it. Editor and persistence use the semantic diagnostic
lane and never contribute timing samples. Any missing, duplicate, failed, or
wrong-lane proof stops the run before timing.

The orchestrator also refuses to start until both runners support every timed
scenario and both declare complete 31-command coverage. GPUI is at 30 of 31;
its remaining blocker is the USGS high-zoom pan stress command. Electron is at
27 of 31; its blockers are native high-zoom pan proof,
cache-pressure GPU upload proof, and the two Image upload receipts. Global
decision coverage therefore remains incomplete. GPUI editor and persistence
implementations can still be exercised as untimed qualification preflights
without changing the frozen decision-3 workload or its oracles; GPUI readiness
cannot promote a comparison while Electron remains incomplete.

Use alternating order for small development diagnostics. Use
`--order-mode randomized-blocks --seed <recorded-integer>` with a pair count
divisible by four for final balanced AB/BA blocks. The manifest retains the
exact order of every pair. Treat development-runtime metrics as local evidence,
not packaged or cross-platform performance proof.

After `run-paired.mjs` finishes, summarize its retained directory with:

```sh
node summarize-paired.mjs \
  --input results/paired \
  --output results/paired/paired-summary.json
```

The summarizer uses `run-manifest.json` when present, including its complete
comparison plan, preflight proofs, global command coverage, requested scenario
list, recorded pair order, per-scenario fixture mapping, failed runner records,
and missing report files. A schema-2 manifest is not `decision_ready` unless all
six preflights, all eleven timed scenarios, both complete coverage reports, and
every recorded pair order pass. Without a manifest, it derives scenarios and
pair IDs from measured report filenames but marks the result as an unscoped
diagnostic, never decision-ready. A pair contributes aggregate metrics only when
both reports match the active protocol and scenario contract, use that
scenario's locked fixture, pass their iteration, match workloads and order, and
explicitly declare `decision_timing_eligible: true`. Rejected runs and their
failure details remain in the summary but never contribute timing or resource
statistics. Paired confidence intervals use the shared geometric-mean
log-ratio bootstrap from `decision-statistics.mjs`.

An iteration succeeds only when the application exits with status 0 and emits
`scenario-complete`. Expanded scenarios must also emit every exact manifest
milestone. A process start without a native window or visible-frame milestone
remains a failed iteration.

## Electron comparison runner

`electron-runner.mjs` runs the current tracked Electron implementation through
its existing test-only Chrome DevTools Protocol (CDP) bridge. It uses a fresh
user-data directory for every iteration and fixes the window at 1200 by 800.
For `open-pdf`, `viewer-layout`, `page-navigation`, `zoom`, `high-zoom-pan`,
`annotation-create`, `annotation-transform`, `editor-create`, and
`continuous-scroll`, pass `--input-lane native-x11-xtest` to use
operating-system XTest pointer, click, keyboard, and wheel input. Pointer and
wheel timing use the small tracked `native-x11-xtest.c` helper with absolute
monotonic deadlines. Native control and file-chooser input uses xdotool. In
that lane, Chrome DevTools Protocol (CDP) only observes verified DOM/page
geometry, application state, paint evidence, and instrumentation. It does not
inject the native action. The runner fails closed if the visible exact-title
window does not belong to the launched PID or is not exactly 1200 by 800.
Build the current development app first:

```sh
node ../../../scripts/prepare-desktop-dev.mjs
cd ../../../
pnpm build:desktop
cd experiments/gpui-migration/performance
```

Then run the same PDF and iteration count as GPUI:

```sh
node electron-runner.mjs \
  --scenario open-pdf \
  --pdf /absolute/path/to/verified-public-fixture.pdf \
  --iterations 5 \
  --output results/electron-open-pdf.json
```

The three hardware follow-up lanes use the locked USGS sheet for zoom and pan,
and the generated annotation-density fixture for editor creation. Run them on
one exclusive Xorg display after the desktop build. Do not run two native X11
replays on the same display at the same time:

```sh
DISPLAY=:98 node electron-runner.mjs \
  --scenario zoom \
  --input-lane native-x11-xtest \
  --pdf results/public-corpus-v1/f058179e193ccbc15ca662feff3554102f64ff2114436a5ce6116d6fa5d2a6e2.pdf \
  --iterations 1 \
  --timeout-ms 120000 \
  --output results/electron-zoom-native-xorg-hardware.json

DISPLAY=:98 node electron-runner.mjs \
  --scenario high-zoom-pan \
  --input-lane native-x11-xtest \
  --pdf results/public-corpus-v1/f058179e193ccbc15ca662feff3554102f64ff2114436a5ce6116d6fa5d2a6e2.pdf \
  --iterations 1 \
  --timeout-ms 120000 \
  --output results/electron-high-zoom-pan-native-xorg-hardware.json

DISPLAY=:98 BP_ELECTRON_IMAGE_TRACE=1 node electron-runner.mjs \
  --scenario editor-create \
  --input-lane native-x11-xtest \
  --pdf results/public-fixtures-v1/bp-annotation-density-v1.pdf \
  --iterations 1 \
  --timeout-ms 120000 \
  --output results/electron-editor-create-native-xorg-hardware.json

DISPLAY=:98 node electron-runner.mjs \
  --scenario cache-pressure \
  --pdf results/public-fixtures-v1/bp-multi-page-v1.pdf \
  --iterations 1 \
  --timeout-ms 240000 \
  --output results/electron-cache-pressure-gpu.json
```

Set `BP_ELECTRON_CDP_PORT` only if another process owns the deterministic
default port. The editor lane remains fail-closed unless Text preserves the
exact `#111827` color, Page Scale preserves two decimal places, Length creates
the exact `(90, 510)` to `(306, 510)` PDF-space segment and `3.00 m` label, and
Image creates one markup with a trace-backed per-image GPU upload-byte receipt.

For GPUI editor qualification, build with the evidence feature and use an
exclusive hardware-backed Xorg display. `GPUI_X11_SCALE_FACTOR=1` is a
process-local GPUI CE override that keeps the requested logical 1200 by 800
window at the runner's required physical 1200 by 800 size. The native command
must pass Text, Page Scale, Length, and Image exact milestones. The following
semantic command is timing-ineligible and exists only to diagnose the final
text shaping, overlay paint, image decode, atlas upload, and platform-draw
receipts:

```sh
cd ../gpui-gallery
cargo build --features benchmark-evidence --bins
cd ../performance

GPUI_X11_SCALE_FACTOR=1 DISPLAY=:98 node gpui-runner.mjs \
  --scenario editor-create \
  --input-lane native-x11-xtest \
  --pdf "$PWD/results/public-fixtures-v1/bp-annotation-density-v1.pdf" \
  --iterations 1 \
  --timeout-ms 120000 \
  --binary "$PWD/../gpui-gallery/target/debug/butter-paper-gpui-gallery" \
  --output "$PWD/results/gpui-editor-create-native-xorg-hardware.json"

GPUI_X11_SCALE_FACTOR=1 DISPLAY=:98 node gpui-runner.mjs \
  --scenario editor-workload \
  --pdf "$PWD/results/public-fixtures-v1/bp-annotation-density-v1.pdf" \
  --iterations 1 \
  --timeout-ms 120000 \
  --binary "$PWD/../gpui-gallery/target/debug/butter-paper-gpui-gallery" \
  --output "$PWD/results/gpui-editor-workload-xorg-hardware.json"
```

Do not run these commands concurrently on one display. Preserve the scale
override in the report provenance. A software-Xorg failure is diagnostic only;
it cannot replace the hardware-backed run.

Supported exact scenarios in the GPUI runner are:

- `viewer-layout`: single-page and continuous layout on the generated 100-page
  control, with exact page membership, geometry, bounded virtualization, and a
  current annotation-thumbnail revision;
- `page-navigation`: the locked NASA normalized sequence;
- `zoom`: the locked USGS sequence through 1600%, with current full-density
  tiles where a bounded full-page surface cannot satisfy device density;
- `high-zoom-pan`: the locked five-second, 120 Hz normalized USGS pan path;
- `cache-pressure`: five generated-document navigate/zoom/pan/return cycles
  exercise decoded and tile-cache limits, but the exact scenario remains
  blocked because GPUI CE does not expose native GPU-upload byte observations;
  decoded tile-cache insertion bytes are reported separately and cannot satisfy
  `upload-byte-count-recorded`;
- `close-reopen`: resource release and a declared-warm reopen of the generated
  100-page control;
- `annotation-create` and `continuous-scroll`: the exact paths described above.

The Electron runner currently supports:

- `open-pdf`: first interactive shell frame, PDF open, and an acceptable visible
  preview;
- `page-navigation`: the normalized sequence from `protocol.md`, rounded up,
  clamped to the document, and deduplicated after clamping;
- `zoom`: `100, 200, 400, 800, 1600, 400, 100, 800, 200, 100, 1200, 100`
  percent.
- `annotation-create`: exact 361-sample Rectangle and Highlight paths. The
  runner selects Fit Page, disables the three active snap sources, and selects
  each tool through native XTest clicks. It records an explicit one-to-one map
  from each application-generated Electron ID to the manifest ID before
  canonical comparison.
- `continuous-scroll`: the exact common 120 Hz XTest wheel event schedule. X11
  wheel buttons do not carry pixel deltas, so the semantic finish page remains
  a required application observation. A matching event count alone does not
  pass the command.
- `editor-workload`: a timing-ineligible CDP diagnostic creates the maintained
  Highlight, Text, Length, and Image prerequisites, remaps their generated IDs,
  and executes the four edit/history plans through generic application hooks.
  It records selection, exact pre-commit/commit/Undo/Redo documents and history
  depths, live Length label, thumbnail paint, and Image decode/presentation.
  Highlight, Text, Length, and Rectangle dense repetition are supported. Image
  remains blocked on upload bytes; persistence uses its separate semantic lane.

The native annotation lane uses `bp-native-ui-style-v1`: Rectangle is red,
1 pt, solid, transparent fill, full opacity; Highlight is yellow, 12 pt, full
opacity, Multiply. These are the maintained Electron defaults and are also
required of the GPUI benchmark adapter. Every field is observed and compared;
the alias map changes IDs only and does not remove style or geometry checks.
`bp-native-ui-geometry-v1` makes unavoidable native pixel quantization explicit:
Rectangle endpoints use the verified PDF-point-to-pixel scale, and smoothed
Highlight centerlines are arc-length resampled to 64 points with a maintained
two-native-pixel smoothing deviation plus the unavoidable diagonal from
half-pixel XTEST rounding on each axis. The report exposes both budget
components; the quantization term does not replace or enlarge the smoothing
budget. Electron smoothing evidence also requires a
nontrivial retained path with fewer points than the submitted 120 Hz stream;
the geometry oracle alone does not prove smoothing. The GPUI document and
thumbnail paint paths use the tested cubic interpolation builder while keeping
the raw committed points for hit testing and persistence. The report records
the scale, errors, and tolerance.

The first zoom value intentionally repeats the initial 100% state. The gallery
re-requests the viewport for that active benchmark step, so an equal-value
operation still produces a measured completion.

Page-navigation durations end after the selected page has an acceptable PDF.js
preview and two animation frames. Zoom durations measure the prompt UI
response only: the requested zoom is active and two animation frames have run.
Each zoom step also retains its raster counters after a fixed 250 ms observation
window. Do not interpret the zoom duration as full high-quality raster
completion.

The report matches the GPUI runner's top-level schema where practical and adds
Electron-specific evidence: application diagnostics, the existing renderer
performance snapshot, Chromium metrics, DOM counters, post-garbage-collection
heap use, requestAnimationFrame intervals, and Electron process metrics. Both
runners sample the full process tree every 100 ms.

Both runners retain the active renderer proof at
`iterations[0].active_gpu_adapter`. Electron derives the active device from
Chromium `SystemInfo.getInfo`, its active OpenGL renderer, and enabled GPU
compositing. GPUI derives it from the first-frame `window.gpu_specs()` receipt,
which describes the wgpu adapter selected for that window. On a required
NVIDIA lane, the receipt receives a `GPU-...` device UUID only after that active
device name and driver match the single `nvidia-smi` adapter. A missing,
software-emulated, ambiguous, or mismatched adapter fails the runner when
`BP_PERF_REQUIRE_NVIDIA=1`; available-device enumeration alone does not pass.

Electron uses PDF.js while this GPUI spike uses the pinned PDFium worker and
bounded shared BGRA surfaces. End-to-end interaction results are comparable
product evidence, but raster time and memory are still not a clean
framework-only comparison because the PDF engines differ. Native presentation
traces remain a separate gate.

The Electron runner uses a deterministic loopback CDP port by default because
some restricted shells deny a temporary `listen(2)` probe. Set
`BP_ELECTRON_CDP_PORT` when another local process owns the derived port.

The disposable GPUI app bundle is rebuilt from an empty target directory and
ad-hoc signed by `scripts/build-app-bundle.sh`. This prevents stale executable
names and unsealed resources from confusing Launch Services. It does not
override an inherited Codex sandbox denial for
`com.apple.hiservices-xpcservice`.

## Prepare optimized v4 candidates

The v4 inference lane rejects development-server Electron assets and Cargo
debug binaries. Prepare both unpackaged candidates before leasing the GPU host:

```sh
node experiments/gpui-migration/performance/optimized-candidates-v4.mjs \
  --output test-results/gpui-v4-candidates
```

The command builds the shared packages, removes only the disposable
`apps/desktop/.vite` output tree, and then builds the desktop Vite production
bundles with `NODE_ENV=production`. The Electron manifest records this reset
provenance, and validation rejects older manifests that omit it. This prevents
obsolete hashed chunks from an earlier Vite build from entering a candidate.
It also builds `butter-paper-gpui-gallery` and
`butter-paper-pdf-worker` with Cargo's `release` profile, default features
disabled, the exact `gallery,benchmark-evidence,pdfium-worker` feature set, and
one Cargo build job. The benchmark feature enables GPUI's input-latency
histogram but not GPUI `test-support`. The bounded job count avoids concurrent
release code-generation and linking on small development hosts. The command
writes separate Electron and GPUI candidate manifests with hashes for every
Electron bundle asset, the Electron executable, the GPUI executable, and the
PDF worker.

Pass the two generated manifests as `--electron-candidate-artifact` and
`--gpui-candidate-artifact` to `run-paired-v4.mjs`. The paired runner rehashes
all artifacts before it creates the run manifest. It fails if the Electron
bundle contains a Vite development-server marker, if an artifact changed, or
if the GPUI executable is not the expected `target/release` binary. The
Electron runner removes inherited development-server environment variables,
sets `BP_DISABLE_RENDERER_DEV_SERVER=1`, and sets `NODE_ENV=production` before
launch.

This is release-like performance evidence, not package qualification. The
workflow does not build an installer, sign, notarize, install, publish, or
modify an installed application.

## Current Linux GPU matched-run result

On 2026-08-23, an authorized disposable DigitalOcean RTX 4000 Ada host ran the
current Electron development runtime and direct-GPUI development runtime. Both
used the same 27,842,805-byte, 526-page public NASA PDF with SHA-256
`68d0f3bb93fd1c4e4f3adf99d483e9161f14293a311e25aa6c31241bbbb84049`.
The 1280×800 X11 lane ran one application at a time, used a disposable app cache
for every launch, excluded one warmup per implementation and scenario, and
alternated order for five measured pairs. Electron's actual accelerated backend
was ANGLE/OpenGL on the NVIDIA driver. GPUI's actual backend was Vulkan.

The paired result is incomplete and must not qualify a migration:

- Cold open has five valid pairs. Median open-request-to-visible time was
  1,163.7 ms for Electron and 199.7 ms for GPUI. Median peak process-tree RSS
  was 812,800 KiB and 240,568 KiB. Median cgroup CPU use was 2.119 and 0.946
  seconds. GPUI used more whole-device peak GPU memory in this scenario: 161
  MiB versus 123 MiB. One otherwise-successful GPUI launch delayed its first
  frame for 69.3 seconds, so the process-to-visible confidence interval does
  not establish a startup advantage.
- Page navigation has four valid pairs and one rejected pair. For the valid
  pairs, the median of the ten quality-gated jumps was 270.4 ms for Electron
  and 166.6 ms for GPUI. Median peak process-tree RSS was 925,536 KiB and
  288,774 KiB. Median cgroup CPU use was 5.858 and 5.287 seconds. A fifth GPUI
  launch created a window but never emitted a first frame and timed out at 120
  seconds, so this scenario does not meet the five-pair development gate.
- Zoom has zero valid pairs. Electron failed to settle the 200% step in every
  measured run. Four GPUI runs completed, but the 4096-pixel full-page raster
  cap produced only 0.420 and 0.560 display density at 1600% and 1200%. The
  fifth GPUI launch timed out before its first frame. Resource samples from all
  rejected pairs are retained but excluded from aggregate metrics.

The ignored evidence directory is
`results/gpu-compare-20260823/`. `paired-summary.json` retains every validation
error and reports only valid-pair aggregates. `evidence-sha256.txt` verifies all
raw reports, provenance, and the exact 4,272,991-byte source snapshot. The
complete evidence bundle has SHA-256
`2f888694e26adc8a5989a017bb96c1a9278b5ff8ce20e72a20baf2e12eea85fa`.

An earlier macOS run remains rejected historical evidence because Electron
stopped below the 0.75 density floor and the older GPUI report omitted its
density. The Linux run fixes those evidence gaps for cold open and four
navigation pairs. Native presentation timing and a matched visual crop oracle
remain not run. The private Hibbeler corpus was not transferred.

## Generate the HTML comparison

Supply one Electron and one GPUI report for each matching scenario:

```sh
node generate-comparison.mjs \
  --electron results/electron-open-pdf.json \
  --electron results/electron-page-navigation.json \
  --electron results/electron-zoom.json \
  --gpui results/gpui-open-pdf.json \
  --gpui results/gpui-page-navigation.json \
  --gpui results/gpui-zoom.json \
  --output comparison.html
```

The page keeps failed iteration counts visible. It suppresses latency and
resource cells when an implementation has no successful iteration, so a
HIServices-denied or timed-out idle process cannot appear to be a comparable
workload result. The current generated page uses three successful native GPUI
reports alongside the retained one-run Electron smoke reports.
