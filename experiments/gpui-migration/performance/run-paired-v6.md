# V6 paired comparison

V6 is an additive correction to the frozen v5 comparison. It does not change
the v5 workload, decision contract, materialized bytes, or reviewed hashes.
`comparison-workload-v6.json` records those source identities and has its own
exact byte identity.

## Schedule

The v6 schedule contains exactly 624 launches:

- 600 benefit launches: 10 native component occurrences per implementation,
  six calibration pairs, and 24 final pairs.
- 22 semantic correctness launches: 11 components run once in Electron and
  once in GPUI.
- Two native property correctness launches, one per implementation.

The eight benefit-eligible component names are `open-pdf`,
`continuous-scroll`, `viewer-dynamic-fidelity`, `annotation-create`,
`annotation-transform`, `editor-create`, `native-snap-transform-120hz`, and
`multi-document-session`. `open-pdf` occurs in three journeys, which produces
the required 10 occurrences.

The correctness-only semantic components are `viewer-layout`,
`page-navigation`, `cache-pressure`, `close-reopen`, `fit-modes`, `zoom`,
`high-zoom-pan`, `cache-pressure-recovery`,
`annotation-properties-history`, `editor-workload`, and
`persistence-workload`. They cannot contribute CPU, memory, latency, frame, or
GPU benefit metrics until a later reviewed contract gives them real native
input.

Electron retains one narrowly classified semantic baseline defect:
`electron-engineering-zoom-density-and-raster-bound-v1`. The exact 12-state
engineering zoom sequence reaches every requested zoom with current
presentation and zero stale-generation frames, but it does not prove the
`visible-tiles-bounded` or `settled-density-at-least-1` quality milestones.
The command receipt and both milestones remain failed, the run records
`correctness_passed: false`, and the analyzer surfaces the defect. Only this
exact observation can continue to the paired benefit schedule. A changed
sequence, stale frame, render error, receipt shape, or missing-milestone set
still fails closed.

## Execution boundary

`run-paired-v6.mjs` constructs and sequentially executes all runner invocations.
Both native runners now retain a temporal server-observation receipt at
`iterations[0].native_input.evidence.common_benefit_timing_boundary`. The
observer sample must cover an actual benchmark XTEST action. A separate hover
or diagnostic click is not valid.

Every benefit launch sets `BP_PERF_COMMON_DAMAGE_OBSERVER=1`. Inherited v4
benefit components also receive `--v6-scenario` and retain the v4 command
contract. V5 hard components keep their v5 evidence context and activate the
observer through the environment.

The required receipt starts at the actual XTEST injection and ends when the
X11 server delivers the first matching `DamageNotify` for the measured target
drawable after an explicit damage reset. Both timestamps use
`CLOCK_MONOTONIC`. This boundary proves server-observed drawable damage. It
does not prove presentation completion or physical scanout. The separate
Electron frame callback and GPUI draw-submission metrics retain their distinct
implementation-specific meanings. Missing events, timeouts, application-only
callbacks, or an uncorrelated observer sample block execution.

Before the 624-launch executor, run the paid-GPU qualification mode. It runs
only one `small-shell-open` / `open-pdf` native pair: Electron, then GPUI. The
qualification must prove both XDamage receipts, required NVIDIA
baseline/run/adjusted samples on one adapter, GPUI NVIDIA Vulkan selection,
Electron Chromium NVIDIA selection, and the matched measurement-start/end view
state. It also binds the exact candidate manifests, frozen v6 and source-v5
workload identities, all fixtures and reference crops, X11 display state, and
one shared D-Bus session.

Before either qualification application starts, the paid host must also pass
`bp-linux-cgroup-v2-accounting-v1`. This preflight creates a disposable child
cgroup, runs a small CPU and memory probe inside it, and requires positive
`cpu.stat` `usage_usec` and `memory.peak` values from that child. It then
removes the probe cgroup. A host that reports CPU time but has no
`memory.peak` is blocked before launch. Process-tree resident set size (RSS)
remains diagnostic evidence and cannot replace either required cgroup metric.

If this preflight blocks, preserve its remediation evidence and check the paid
host before extending its lease:

```bash
stat -fc %T /sys/fs/cgroup
cat /sys/fs/cgroup/cgroup.controllers
cat /sys/fs/cgroup/cgroup.subtree_control
```

The filesystem must be `cgroup2fs`; `cgroup.controllers` must include `cpu`
and `memory`; and the benchmark account must be able to create a child cgroup,
write the probe PID to `cgroup.procs`, and read `cpu.stat`, `memory.peak`, and
`memory.events` from that child.

Qualification has a separate short lease ceiling. The task limit cannot exceed
8 minutes. The lease time-to-live (TTL) cannot exceed 30 minutes and must cover
the task limit plus cleanup grace. A reviewed default is an 8-minute task limit,
15-minute cleanup grace, and 23-minute absolute TTL:

The command writes a canonical-payload SHA-256 inside the receipt and a
detached file SHA-256 beside it. A changed candidate, fixture, reference crop,
active renderer UUID, NVIDIA adapter or driver, Vulkan/X11 state, X server PID
or start time, D-Bus address or bus ID, raw report, damage receipt, or
view-state receipt invalidates it.

Immediately before every qualification and full-run launch, the runner
recursively revalidates both optimized candidate closures. The launch binding
retains the hash-authenticated seal, including the Electron bundle and runtime
dependency trees plus the GPUI executable and PDF worker. Any mutation fails
closed before that launch consumes measurement time.

Run qualification and full execution under one enclosing `dbus-run-session`.
Do not invoke `dbus-run-session` separately for the two commands. The exact X
server process identity and D-Bus bus ID must remain unchanged:

```bash
dbus-run-session -- bash -lc '
  set -e
  node run-paired-v6.mjs --qualify \
    --task-limit-ms 480000 \
    --cleanup-grace-ms 900000 \
    --lease-ttl-ms 1380000 \
    --qualification-receipt <run-root>/qualification-receipt-v6.json \
    "$@"
  node run-paired-v6.mjs --execute \
    --task-limit-ms <reviewed-full-task-limit-ms> \
    --lease-ttl-ms <reviewed-full-lease-ttl-ms> \
    --qualification-receipt <run-root>/qualification-receipt-v6.json \
    "$@"
' bash <shared-candidate-fixture-reference-output-and-price-options>
```

Run qualification and full execution on the same paid host and inside the same
D-Bus/X11 session. Full execution revalidates the receipt, detached checksum,
candidate and corpus identities, current environment binding, retained raw
report hashes, XDamage evidence, and matched view state before launch 1 of 624.
It checks the X server and D-Bus identities again at every launch-group
boundary and re-seals the candidates before every launch. Resume requires the
same qualification receipt hash and the original
immutable task deadline, lease expiry, hourly rate, and maximum-cost digest. It
cannot reset or expand the time or cost envelope.

Use `--resume` with the exact same output, candidates, fixtures, seed, timeout,
and cooldown after an interruption. An atomically replaced small checkpoint
stores only the manifest filename and SHA-256, not a second copy of the growing
manifest. The manifest retains raw-report paths and hashes instead of embedding
each complete raw report. Resume verifies that checkpoint or the
run-manifest checksum, candidate bindings, raw and hard report hashes, bundle
payloads, and each retained launch again. It reuses only a passed schedule
prefix ending at a complete correctness launch or benefit bundle. It discards
a partial or failed bundle and continues without changing order.

Analyze a complete run with:

```bash
node analyze-paired-v6.mjs \
  --manifest <run-root>/run-manifest-v6.json \
  --output <run-root>/analysis-v6.json
```

The analyzer rehashes retained artifacts, authenticates the exact 624-launch
schedule, excludes all correctness-only components from benefit statistics,
preserves hard correctness and absolute safety gates, uses 100,000 paired
bootstrap resamples, and reports `YES`, `NO`, or `BLOCKED`. This is a Linux
GPU-host migration-investment decision, not release qualification.

## Current evidence and paid-host ceiling

The post-fix real `open-pdf` matched-view smoke passes with zero failures. The
Electron receipt SHA-256 is
`0425f69b3181a1404125ff96c0c0b2598ce0fa5e785ff34d15af4824fcccccd6`,
the GPUI receipt SHA-256 is
`314138f349debb22884fccac916d9059ece5cd1d900334ad0dbacf4db00d683c`,
and the rebuilt pair evidence SHA-256 is
`0874146fb8a54fbcfd8e4c6a7687073c89ec5ef8b4383ecd5f260708893e2d8d`.
The local artifact is ignored runtime evidence, not a tracked fixture.

The paid qualification invalidated the earlier XPresent boundary. Electron
verified chooser focus, opened the document through the chooser's native Open
button, rendered on the exact RTX 4000 adapter, and still emitted no
main-window `PresentCompleteNotify`. XPresent observes client Present requests
and cannot serve as an implementation-neutral boundary for Electron ANGLE and
GPUI.

V6 now uses the separate `x11-damage-notify-after-xtest-v1` contract. On
2026-08-24, the two-launch qualification passed on Ubuntu 24.04 with an NVIDIA
RTX 4000 Ada Generation GPU. Both candidates proved the exact adapter,
candidate and corpus identities, matched view state, cgroup-v2 CPU and memory
accounting, and observer-owned XTEST-to-XDamage receipts. The receipt correctly
reports `presentation_completion_observed: false` and
`physical_scanout_observed: false`.

The full run then passed its first 11 semantic launches and stopped at the
Electron engineering zoom quality defect described above. After the exact
retained-defect classifier was added, a fresh paid-host zoom rerun completed
successfully while preserving the false receipt and the two missing quality
milestones. The complete 624-launch run has not been rerun. The paid droplet
and its ephemeral SSH key were deleted after the focused proof.

At the frozen default durations, 624 launches require 12,128,000 ms of runner
time plus 1,246,000 ms of cooldown: 13,374,000 ms expected wall time. At the
reviewed RTX 4000 price of $0.76/hour, expected compute is about $2.82. A 35%
task headroom and 15-minute cleanup reserve produce an absolute TTL of
18,954,900 ms (5.265 hours) and a $4.0016 ceiling. The previous lease was
deleted before this ceiling, but its elapsed cost belongs to that completed
lease. Treat a full rerun as a new paid task with a new explicit maximum and
independent expiry.

Print and review the schedule by supplying the same required candidate,
fixture, output, reference-crop, and hourly-price options as execution:

```bash
node experiments/gpui-migration/performance/run-paired-v6.mjs --plan <all-required-options> \
  > /tmp/bp-v6-plan.json
```

Do not lease the full paid run without a passed, authenticated qualification
receipt from the same paid host and shared X11/D-Bus session.
