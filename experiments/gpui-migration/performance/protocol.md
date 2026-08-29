# Butter Paper Electron and GPUI performance acceptance protocol

This protocol compares the maintained Electron/Nova application with the native
GPUI replacement candidate. It defines development, beta, and release evidence.
It does not let one operating system, development runtime, or stale report prove
another operating system or a packaged candidate.

The protocol version is `bp-perf-v2`. A result without this version and a
complete evidence manifest is not comparable evidence.

## Acceptance rule

A GPUI result passes only when all four conditions pass:

1. **Correctness:** the scenario completes with the required document state and
   settled raster quality. A faster placeholder, lower-resolution surface,
   stale generation, missing annotation, or failed save is a failure.
2. **Completeness:** every predeclared iteration and raw sample is present. Do
   not remove crashes, errors, timeouts, warm-up runs, or statistical outliers.
3. **Absolute user-experience budget:** the GPUI candidate meets the numeric
   ceiling in this protocol and any tighter corpus-manifest ceiling.
4. **Paired non-inferiority:** the upper bound of the 95% paired-bootstrap
   confidence interval for the GPUI/Electron ratio does not exceed the
   metric-specific margin below.

An underpowered comparison is **not run to conclusion**, not a pass. A blocked
fixture or native trace is **blocked**, not a pass. Correctness failures are not
converted into slow timing samples.

## Artifact and evidence levels

| Claim                       | Required artifact                                                                               |
| --------------------------- | ----------------------------------------------------------------------------------------------- |
| Instrumentation development | Development runtime from the active worktree                                                    |
| Local performance direction | Matched Electron and GPUI development runtimes on one host                                      |
| Beta qualification          | Exact packaged beta candidates on each matching operating system                                |
| Stable replacement          | Exact release candidates on macOS, Windows, and Linux, plus required native presentation traces |

Record source revision, dirty-state hash, executable and package SHA-256,
runtime type, renderer and dependency revisions, scenario version, and corpus
identity. A source build does not prove package, signing, update, or release
performance.

## Corpus

Use the corpus IDs and verified bytes from
[`research/pdf-qualification-corpus.md`](../research/pdf-qualification-corpus.md).

### Required repository and public workloads

- `bp-single-page-v1`: small cold-open and shell-to-document control.
- `bp-multi-page-v1`: deterministic page navigation and cache control.
- `bp-long-document-v1`: 1,000-page virtualization workload when its generator
  is implemented.
- `bp-annotation-density-v1`: drawing, selection, resize, hit-test, save, and
  memory workload when its generator is implemented.
- `nasa-apollo-summary-526-v1`: public long-document navigation, continuous
  scroll, cache, and memory workload.
- `usgs-usa-geology-sheet-v1`: public large-sheet zoom, pan, tiling,
  cancellation, upload, and GPU-memory workload.

### Private workload

`private-hibbeler-935-v1` is supplementary owner-authorized local evidence.
Never upload its bytes, images, extracted text, or thumbnails to the repository,
hosted continuous integration, or a paid GPU host. A missing private file is
blocked unless the release decision explicitly waives that optional gate.

Verify SHA-256, byte count, page count, storage class, and allowed lane before
every run. Reports use the neutral corpus ID rather than a private filename.

## Matched environment

Compare Electron and GPUI only on the same host, operating system, architecture,
display, GPU, driver, refresh rate, scale factor, theme, colour profile, power
state, thermal band, fixture bytes, window geometry, and cache class.

Do not normalize results from different hosts with a synthetic score. If the
host or hardware changes, collect a new Electron baseline on that host in the
same session. Normalize only these dimensionless values:

- application frame intervals divided by the measured display refresh period;
- native input receipt to application frame/draw acknowledgement divided by the
  refresh period;
- CPU work as process-tree CPU-seconds for the fixed scenario;
- renderer bytes per visible device pixel where the scenario changes display
  scale or viewport size.

Record physical and logical CPU count, memory, GPU and dedicated/shared memory,
driver, display mode, compositor/window system, power source and policy, thermal
state where available, and other active benchmark workloads. Do not change a
user device power or interactive-session state without current-task authority.

## Cache classes

Label every iteration with exactly one cache class:

- **App-cold:** new disposable user data and raster cache, new process. The
  operating-system file cache is not flushed and is recorded as uncontrolled.
- **Process-warm:** same process and document, target surface not in the app
  cache unless the scenario explicitly tests a cache hit.
- **Cache-warm:** same process and document with the required raster or tile
  already verified in the app cache.
- **Reopen-warm:** new process with an explicitly retained on-disk application
  cache. Use only for the cache-reopen scenario.

Never call an app-cold run a machine-cold run. Do not require privileged cache
flushing or a reboot for routine comparisons.

## Scenario contract

The runners must use typed in-application commands and semantic completion
events. Pointer automation is not needed for repeatable development timing.
Packaged native qualification adds real input and presentation tracing for the
same semantic sequences.

### 1. Empty shell

Launch without a document. Record process start, window creation, first
submitted frame, first native presentation, shell interactive, idle settled,
process-tree CPU, resident set size (RSS), committed/private memory where the
platform provides it, and GPU memory.

### 2. Open document

Open each required fixture from an already interactive shell. Record:

- request to document metadata ready;
- request to first visible placeholder;
- request to acceptable preview;
- request to settled current-generation raster;
- first visible thumbnail and visible-thumbnail fill milestones;
- parse, decode/raster, upload, and cache work separately;
- peak and settled resources.

An **acceptable preview** covers the visible page area with the current render
generation, has at least 0.75 source pixels per visible device pixel, and passes
the fixed visual crop oracle. A **settled raster** covers the visible area at at
least 1.0 source pixel per device pixel, passes the same oracle, and remains the
current generation for 250 ms with no required tile pending. High zoom uses
visible tiles; it never requires an unbounded full-page bitmap.

### 3. Page navigation

For a document with `N` pages, visit this normalized sequence, clamped to valid
pages and with duplicates removed only after clamping:

`N, 0.08N, 0.72N, 0.25N, 0.90N, 0.50N, 11, 0.958N, 0.33N, 1`

For every step, record command receipt, prompt visible response, acceptable
preview, settled raster, stale/superseded completion count, cache result, raster
dimensions, and next native presentation.

### 4. Zoom and pan

Use this zoom sequence:

`100, 200, 400, 800, 1600, 400, 100, 800, 200, 100, 1200, 100`

The first 100% step is an explicit measured refresh. On the USGS sheet, pan at
400% through centre, upper-left, upper-right, lower-right, lower-left, and centre.
Issue the next high-zoom command before selected earlier rasters complete to
prove cancellation and generation rejection. Record prompt response, visible
tile coverage, settled raster, uploads, cache activity, and memory recovery.

### 5. Continuous scroll

On the NASA workload, scroll forward for 20 seconds at a fixed velocity that
crosses 50 initial viewport heights, pause for 2 seconds, reverse for 10 seconds,
then return to page 1. Record application frame/draw acknowledgements, native
input-to-application acknowledgement latency, visible preview coverage, blank
or stale frames, render queue depth, cache pressure, and resource samples. The
two runtimes use the same timestamped position curve, not wheel-event counts.

### 6. Annotation drawing and resize

On `bp-annotation-density-v1`, replay a three-second 120 Hz rectangle draw, a
three-second move, and a three-second resize. Repeat over a dense page and an
empty page. Record input-to-preview presentation, hit-test time, spatial-index
query count, interaction reduction time, paint time, lost/coalesced inputs, and
the final semantic command. Each gesture must create one history entry.

### 7. Cache and resource pressure

Run five navigation/zoom cycles, close the document, wait 10 seconds, reopen
using the declared cache class, and repeat once. Record cache hits, misses,
evictions, item and byte high-water marks, decoded surface bytes, texture
allocation/free events, upload count/bytes, process-tree resources, and
post-close recovery. No cache may grow without a declared bound.

### 8. Save and reopen

Apply the fixed annotation command set, save, reopen, save again, and reopen
again. Measure command-to-worker-start, serialization, write, validation,
publication, and reopen-to-settled-raster. Performance cannot pass unless the
semantic and visual round-trip oracles pass.

## Metrics

Keep these metric families separate:

- **Product latency:** process-to-window, shell interactive, open-to-preview,
  navigation/zoom response, input-to-preview presentation, save/reopen.
- **Native interaction and application frame pacing:** application frame
  intervals, native input receipt to application frame/draw acknowledgement,
  long frames, and maximum stall. In the Linux v4 lane, Electron uses trusted
  DOM event receipt to the next animation-frame callback and GPUI uses its
  input-latency histogram through platform draw submission. This is an
  application proxy. It does not observe compositor scanout or photon latency
  and must not be described as physical presentation timing.
- **Renderer work:** metadata, decode/raster, tile scheduling, cancelled work,
  stale results, paint, and upload duration.
- **Resources:** CPU-seconds, CPU utilization, RSS, committed/private memory,
  GPU memory, file descriptors/handles, threads, cache bytes, and decoded bytes.
- **Quality:** generation, visible coverage, pixel density, raster dimensions,
  crop-oracle result, annotation semantic result, and blank/stale frame count.

Sample process resources every 100 ms and GPU resources at the highest stable
platform-supported rate no slower than 500 ms. Record counters at every
operation boundary. A missing metric is `not-run` unless the platform records
an explicit unsupported reason.

## Absolute budgets

All latency budgets are p95 unless the row says otherwise. Corpus manifests may
set tighter resource or timeout limits.

| Metric                                              | Budget                                                                                |
| --------------------------------------------------- | ------------------------------------------------------------------------------------- |
| Process start to native window presentation         | <= 1,500 ms                                                                           |
| Window presentation to shell interactive            | <= 250 ms                                                                             |
| Small generated PDF open to acceptable preview      | <= 750 ms                                                                             |
| Public/private heavy PDF open to acceptable preview | <= 3,000 ms                                                                           |
| Command to prompt navigation or zoom presentation   | <= 50 ms                                                                              |
| Uncached navigation to acceptable preview           | <= 1,000 ms; p99 <= 2,000 ms                                                          |
| Zoom/pan settle after final input                   | <= 1,000 ms through 400%; <= 1,500 ms above 400%                                      |
| Draw/move/resize input to preview presentation      | <= 2 refresh periods at p95; <= 4 at p99                                              |
| Scroll/pan presentation interval                    | p95 <= 1.5 refresh periods; intervals above 2 periods <= 1%; none above 6 periods     |
| Blank or stale current-page presentations           | 0                                                                                     |
| Lost semantic input or incorrect final command      | 0                                                                                     |
| Settled idle process-tree CPU                       | <= 1% of one logical core at p95 after 2 seconds                                      |
| App raster/cache storage for one open document      | <= 1.5 GiB unless a lower manifest ceiling applies                                    |
| Post-close memory recovery                          | within the larger of 15% or 128 MiB of the pre-open settled baseline after 10 seconds |
| Crash, scenario error, or per-iteration timeout     | 0                                                                                     |

For memory-constrained reference machines, peak committed memory must also stay
below 25% of physical memory. Dedicated GPU allocation must stay below the lower
of 25% of dedicated video memory or 2 GiB. Integrated/shared GPU systems report
the platform working-set metric and use the combined committed-memory ceiling.

## Relative non-inferiority margins

For each metric, calculate the paired GPUI/Electron ratio. Lower is better.

| Metric family                                                   | Maximum upper 95% confidence bound |
| --------------------------------------------------------------- | ---------------------------------- |
| Native interaction and application-frame acknowledgement p95    | 1.05                               |
| Open, navigation, zoom, pan, draw, resize, save, and reopen p95 | 1.10                               |
| Process-tree CPU-seconds                                        | 1.10                               |
| Peak and settled process memory                                 | 1.15                               |
| Peak GPU memory, upload bytes, decoded bytes, and cache bytes   | 1.15                               |

A GPUI improvement claim requires the upper confidence bound below 1.0 and at
least a 10% point-estimate improvement. Otherwise report parity or uncertainty.
Meeting a relative margin does not excuse an absolute-budget failure.

## Samples and statistics

### Development direction

- Run five independent paired processes per scenario.
- Run native launch/open plus full editor and persistence correctness as
  explicit untimed preflights for each implementation before any timed sample.
- Include cold/open, layout, navigation, zoom, high-zoom pan, continuous scroll,
  cache pressure, close/reopen, annotation create/transform, and editor create.
  A smaller set remains diagnostic-only.
- Use a deterministic balanced order: AB, BA, AB, BA, then the order selected
  by the recorded seed. `A` and `B` assignments are recorded and may swap.
- Report every raw value, median, p95, p99, maximum, and budget count.
- Treat p95 and confidence intervals as descriptive only. Five pairs cannot
  approve beta or stable performance.

### Beta and stable decisions

- Collect at least 20 successful paired iterations per discrete scenario.
- Use ten AB and ten BA pairs in seed-recorded randomized block order.
- Continuous scenarios must also provide at least 30 seconds and 1,000 native
  presentation intervals per runtime in every iteration.
- Stop only at the predeclared attempt limit. Any crash, error, timeout, missing
  milestone, or quality failure fails completeness and remains in the report.
  Fix the cause and run a new versioned comparison; do not replace a bad pair.

For each pair, calculate that iteration's median and p95 from its operation or
presentation samples. Calculate the GPUI/Electron ratio for the paired value.
Use a deterministic 10,000-resample paired percentile bootstrap over iteration
pairs and report the 95% confidence interval. Keep the pair as the sampling
unit so thousands of frames from one process do not create false confidence.

Do not remove statistical outliers. An iteration can be marked environmentally
aborted only when independent host evidence identifies a predeclared condition,
such as a driver reset, operating-system update, loss of the required display,
or thermal limit outside the allowed band. Retain it and its reason.

## Native evidence lanes

- **Current Linux VPS:** deterministic CPU-only runner validation, schema
  checks, fixture preparation, builds, and software-render smoke only. Xvfb or
  software rendering is not GPU performance evidence.
- **Paid Linux GPU:** public corpus only; hardware-accelerated X11 and Wayland
  runs; exact Linux packaged candidates; native presentation, Vulkan/driver,
  upload, GPU-memory, and package evidence. This is Linux evidence only.
- **Local macOS:** shared scenarios plus Metal presentation, Retina scaling,
  native input, Instruments energy/memory, and the private Hibbeler lane when
  authorized. Use the exact macOS packaged candidate for beta/stable claims.
- **Local Windows:** shared scenarios plus DirectX presentation, display-scale
  transitions, native input, Event Tracing for Windows/PresentMon resources,
  and the exact Windows packaged candidate.

Each operating system must run ordinary shared workflows as well as its native
metrics. No lane substitutes for another.

## Paid Linux GPU lease

Prepare and hash candidate artifacts, public corpus files, runner commands, and
artifact destinations before provisioning. The task agent must declare:

- expected setup, calibration, execution, collection, and cleanup durations;
- a task time limit;
- a 15-minute cleanup grace period;
- an absolute lease time-to-live (TTL);
- the provider price and maximum estimated cost;
- expected report count, hashes, and acceptance checks.

Use `ceil(1.5 * expected task duration)` as the initial task limit. The absolute
TTL is that task limit plus 15 minutes of cleanup grace. Cap an ordinary lease
at two hours. A task that needs more than two hours requires a new explicit
maximum before provisioning. The TTL is a safety ceiling, not a target, and may
not be extended silently.

Destroy the droplet and attached billable storage as soon as the expected
evidence is collected or a conclusive failure or blocker is recorded. Verify
destruction. Early completion is successful only if report counts, completion
events, raw samples, hashes, and cleanup all pass.

## Required report groups

Report separately:

- **Passed:** checks that completed against the stated runtime or artifact;
- **Failed:** checks that ran and missed correctness, completeness, absolute,
  or relative acceptance;
- **Blocked:** missing authority, fixture, host, native metric, or external
  state prevented the check;
- **Aborted:** a predeclared environmental condition invalidated a lane;
- **Timed out:** the declared scenario or lease maximum was reached;
- **Not run:** checks intentionally outside the current evidence level.

Every paid report also includes expected and actual duration, price estimate,
artifact identity, environment facts, downloaded evidence hashes, and verified
resource destruction.
