# Butter Paper GPUI investment decision protocol v4

Contract: `bp-perf-v4-decision-1`

This protocol supersedes the representative workload design in
`bp-perf-v3-decision-3`. The v3 files remain frozen historical evidence. This
is still an investment decision, not packaged release qualification.

## Representative inference

The final paired comparison has five macro journeys:

1. `small-shell-open`: cold shell, native presentation, interactive state, and
   settled `bp-single-page-v1` open.
2. `nasa-long-document`: NASA-526 layout, navigation, continuous scroll,
   virtualization, cache pressure, close recovery, and reopen.
3. `engineering-sheet`: generated `bp-engineering-sheet-v1` fit modes, zoom
   through 1600%, fixed pan, visible tiling, cancellation, cache bounds, and
   recovery.
4. `dense-mixed-editing`: sparse and dense Rectangle, Highlight, Text, Length,
   and Image creation and editing with selection, properties, history, and
   annotation-aware thumbnails.
5. `persistence`: representative and unknown annotation import, safe save,
   independent validation, and two reopen cycles.

The machine-readable list and exact fixture/command mappings are exported by
`scenario-contract-v4.mjs`. Each journey remains execution-ineligible until
the runner supplies exact live receipts. A static capability declaration is
planning metadata only.

Every command is assigned to exactly one `current_runner_components` entry or
to an explicit blocker. Fit Modes and post-close cache memory recovery on the
generated engineering sheet remain blocked because no current runner component
proves those exact commands. Duplicate, unknown, or silently unassigned command
mappings fail validation.

## Stress and supplementary evidence

`usgs-usa-geology-sheet-v1` is a separate non-inferential robustness stress
lane. Its result is reported independently:

- Electron pass plus GPUI failure blocks migration.
- GPUI pass plus Electron failure is a robustness advantage.
- Both pass is comparable stress evidence.
- Both fail blocks huge-sheet release qualification, not representative
  performance inference.

`private-hibbeler-935-v1` remains supplementary and blocked/not transferred.
It is allowed only in the owner-authorized local macOS lane.

## Graphics resource evidence

Both implementations must report the same application-observable boundary:

- exact decoded payload bytes;
- exact bytes submitted to the renderer resource path;
- native presentation of the resulting image or annotation.

Physical GPU bus-upload bytes are nullable and optional because Chromium does
not expose an equivalent attributable receipt. Baseline-adjusted whole-device
GPU samples remain diagnostic. Missing physical bus bytes do not replace the
required renderer-resource submission or native-presentation proof.

## Native timing boundary

The v4 decision lane does not measure physical display scanout. It compares two
application-observable proxies after trusted XTest input:

- Electron records trusted DOM native-event receipt to the next
  `requestAnimationFrame` callback.
- GPUI records its input-latency histogram through platform draw submission.

Both runners also report application frame-callback interval p95. These
measurements show application scheduling and draw acknowledgement only. They do
not include monitor scanout, compositor-to-photon delay, or an external input
injection timestamp. The analyzer therefore uses
`native_input_to_application_frame_ack_p95_ms` and
`application_frame_interval_p95_ms`. A native component that lacks either
summary fails the hard evidence gate.

## Execution readiness

`assessDecisionExecutionV4` derives readiness from retained live evidence. It
requires frozen candidate hashes, exact fixture and command receipts, semantic,
visual, persistence, native application-frame, and resource evidence for Electron
and GPUI across all five representative journeys. The contract has no static
`executable: true` switch.

One journey sample is an isolated paired execution bundle. Each component in
`current_runner_components` runs in a fresh process and in the frozen listed
order. Benefit metrics use the declared equal component weights within each
journey. Non-inferiority remains conjunctive for every component, so a strong
component cannot hide another component's regression.

Use six excluded calibration pairs per journey. Use 24 through 40 final pairs
per journey in seed-recorded balanced blocks of four. A missing or failed live
gate returns `not-decision-ready`; only complete evidence can produce the final
yes or no investment result.

The checked-in `comparison-workload-v4.materialized.json` is the self-contained
runtime input. It is deterministically materialized from the v4 descriptor and
the frozen v3 source workload. Focused tests require structural equality and
the same canonical and artifact hashes.
