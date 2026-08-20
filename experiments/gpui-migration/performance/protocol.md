# Butter Paper Electron and GPUI performance protocol

This protocol compares the current Electron development runtime with the
independent GPUI migration spike. It is local development evidence. It is not a
packaged-app, signing, updater, Windows, Linux, or public-release result.

## Fixed input

- PDF: `Statics and Mechanics of Materials in SI Units (Russell C. Hibbeler) (Z-Library).pdf`
- SHA-256: `f4ef16810f9b68dad7fe55d2c28028a3e08a3a6ee39193c565ac96713bc0b6a6`
- Bytes: `127532074`
- Pages: `935`
- Window: `1200 x 800` logical pixels
- Theme: light
- Process sampling: every `100 ms`
- Runs: five independent processes per scenario, alternated by implementation

The report must record the executable hash, source revision, host, operating
system, CPU, memory, display state, PDF hash, iteration number, and timestamps.

## Matched scenarios

### Empty shell

Launch without a document. Measure process start, window creation, first frame,
shell ready, process-tree resident set size (RSS), and process-tree CPU.

### Open PDF

Launch the shell, request the fixed Hibbeler PDF, then wait for page 1 to be
painted. Measure request-to-document-ready, request-to-first-page-frame, page
raster duration, thumbnail work, peak RSS, and peak CPU.

### Page navigation

Open the fixed PDF and visit pages in this order:

`935, 75, 674, 234, 842, 468, 11, 896, 309, 1`

For each page, measure request-to-raster-complete and request-to-next-visible
frame. Retain frame intervals during the complete sequence.

### Zoom

Open page 1 and apply these percentages in order:

`100, 200, 400, 800, 1600, 400, 100, 800, 200, 100, 1200, 100`

For each change, measure request-to-raster-complete and request-to-next-visible
frame. Retain output dimensions, cache state, peak RSS, and frame intervals.

## Shared result fields

- process start to window creation;
- process start to first frame;
- PDF open request to document ready;
- PDF open request to first page frame;
- operation request to raster completion;
- operation request to next rendered frame;
- frame interval median, p95, p99, and maximum;
- counts above `8.33 ms`, `16.67 ms`, and `33.33 ms`;
- process-tree median and peak CPU;
- process-tree median and peak RSS;
- raster cache hit, miss, item count, and bytes where available;
- page number, zoom, raster dimensions, and error or timeout state.

## Interpretation boundaries

Electron uses PDF.js. The GPUI spike currently uses Poppler subprocesses and PNG
files. Raster and cache results are renderer-specific. They do not isolate the
cost of React, Electron, or GPUI.

Application frame callbacks are useful latency markers, but they are not display
presentation timestamps. Final 120 Hz acceptance needs an unlocked desktop plus
the Animation Hitches and Metal System Trace Instruments templates.

The GPUI application must move Poppler metadata and raster work off its UI
thread before its frame results are accepted as representative of the proposed
architecture. Stale asynchronous completions must be rejected by generation
number so rapid page and zoom input cannot paint an old result.

## Acceptance for this milestone

The migration is ready for a first performance comparison when both runners:

1. open the exact PDF from a deterministic command;
2. perform the same page and zoom sequences without pointer automation;
3. emit equivalent operation and visible-frame milestones;
4. exit without an external timeout;
5. retain raw events and process samples for every iteration;
6. identify failures rather than silently excluding them;
7. generate one combined HTML summary that links the raw JSON reports.
