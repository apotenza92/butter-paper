# GPUI performance runner

`gpui-runner.mjs` measures a deterministic scenario emitted by the standalone
Butter Paper GPUI application. It does not alter production sources or use the
Electron application.

The GPUI application must emit one JSON object per stdout line and exit after
the scenario. Each event must contain:

```json
{"schema_version":1,"event":"first_page_visible","t_ms":184.2,"duration_ms":91.7}
```

`duration_ms` is optional. When present, the runner groups and summarizes it by
event name. Any non-empty stdout line that does not match this schema makes the
iteration fail so logging cannot silently corrupt benchmark evidence. Normal
diagnostics belong on stderr.

## Run

Build the independent GPUI app bundle first, then run:

```sh
node gpui-runner.mjs \
  --scenario open-first-page \
  --pdf /absolute/path/to/Hibbeler.pdf \
  --iterations 5 \
  --output results/gpui-open-first-page.json
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
Run both implementations in alternating order. Treat these metrics as local
development evidence, not packaged or cross-platform performance proof.

An iteration succeeds only when the application exits with status 0 and emits
`scenario-complete`. A process start without a native window or visible-frame
milestone remains a failed iteration.

## Electron comparison runner

`electron-runner.mjs` runs the current tracked Electron implementation through
its existing test-only Chrome DevTools Protocol (CDP) bridge. It uses a fresh
user-data directory for every iteration and fixes the window at 1200 by 800.
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
  --pdf /absolute/path/to/Hibbeler.pdf \
  --iterations 5 \
  --output results/electron-open-pdf.json
```

Supported scenarios are:

- `open-pdf`: shell launch, PDF open, and the first `pageRenderReady` frame;
- `page-navigation`: pages `935, 75, 674, 234, 842, 468, 11, 896, 309, 1`;
- `zoom`: `100, 200, 400, 800, 1600, 400, 100, 800, 200, 100, 1200, 100`
  percent.

The first zoom value intentionally repeats the initial 100% state. The gallery
re-requests the viewport for that active benchmark step, so an equal-value
operation still produces a measured completion.

Page-navigation durations end after the selected page has a resolved PDF.js
raster surface and two animation frames. Zoom durations measure the prompt UI
response only: the requested zoom is active and two animation frames have run.
Each zoom step also retains its raster counters after a fixed 250 ms observation
window. Do not interpret the zoom duration as full high-quality raster
completion.

The report matches the GPUI runner's top-level schema where practical and adds
Electron-specific evidence: application diagnostics, the existing renderer
performance snapshot, Chromium metrics, DOM counters, post-garbage-collection
heap use, requestAnimationFrame intervals, and Electron process metrics. Both
runners sample the full process tree every 100 ms.

Electron uses PDF.js while this GPUI spike uses Poppler. End-to-end interaction
results are comparable product evidence. Raster time and memory are not a clean
framework-only comparison. Use a Metal System Trace for GPU upload and display
presentation evidence on the same unlocked 120 Hz desktop.

The Electron runner uses a deterministic loopback CDP port by default because
some restricted shells deny a temporary `listen(2)` probe. Set
`BP_ELECTRON_CDP_PORT` when another local process owns the derived port.

The disposable GPUI app bundle is rebuilt from an empty target directory and
ad-hoc signed by `scripts/build-app-bundle.sh`. This prevents stale executable
names and unsealed resources from confusing Launch Services. It does not
override an inherited Codex sandbox denial for
`com.apple.hiservices-xpcservice`.

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
