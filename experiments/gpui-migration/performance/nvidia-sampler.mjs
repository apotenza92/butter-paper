import { spawn } from "node:child_process";

const queryFields = [
  "timestamp",
  "index",
  "utilization.gpu",
  "memory.used",
  "memory.total",
  "power.draw",
];

export const nvidiaBaselineDurationMs = 1_000;
export const nvidiaSampleIntervalMs = 200;
export const minimumNvidiaBaselineSamples = 3;

function percentile(values, fraction) {
  const position = (values.length - 1) * fraction;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  return lower === upper
    ? values[lower]
    : values[lower] + (values[upper] - values[lower]) * (position - lower);
}

function summarize(values) {
  const valid = values
    .filter(Number.isFinite)
    .sort((left, right) => left - right);
  if (valid.length === 0) return null;
  return {
    count: valid.length,
    min: valid[0],
    median: percentile(valid, 0.5),
    p95: percentile(valid, 0.95),
    max: valid.at(-1),
    mean: valid.reduce((sum, value) => sum + value, 0) / valid.length,
  };
}

function sampleSummary(samples) {
  return {
    gpu_utilization_percent: summarize(
      samples.map((sample) => sample.gpu_utilization_percent),
    ),
    memory_used_mib: summarize(samples.map((sample) => sample.memory_used_mib)),
    power_draw_watts: summarize(
      samples.map((sample) => sample.power_draw_watts),
    ),
  };
}

function runnerSampleSummary(samples) {
  const summary = sampleSummary(samples);
  return {
    utilization_percent: summary.gpu_utilization_percent,
    memory_used_mib: summary.memory_used_mib,
    power_draw_watts: summary.power_draw_watts,
  };
}

export function parseNvidiaSample(line, elapsedMs) {
  const values = line.split(",").map((value) => value.trim());
  if (values.length !== queryFields.length) return null;
  const [
    timestamp,
    index,
    gpuUtilizationPercent,
    memoryUsedMib,
    memoryTotalMib,
    powerDrawWatts,
  ] = values;
  const sample = {
    elapsed_ms: elapsedMs,
    timestamp,
    index: Number(index),
    gpu_utilization_percent: Number(gpuUtilizationPercent),
    memory_used_mib: Number(memoryUsedMib),
    memory_total_mib: Number(memoryTotalMib),
    power_draw_watts: Number(powerDrawWatts),
  };
  return Object.values(sample).some((value) => Number.isNaN(value))
    ? null
    : sample;
}

export function buildNvidiaBaselineResult({
  baselineSamples,
  transitionSamples = [],
  runSamples,
  invalidLines = [],
  spawnError = null,
  baselineDurationMs = nvidiaBaselineDurationMs,
  intervalMs = nvidiaSampleIntervalMs,
}) {
  const indexes = new Set(
    [...baselineSamples, ...runSamples].map(({ index }) => index),
  );
  const commandAvailable = spawnError === null;
  let reason = null;
  if (!commandAvailable) reason = spawnError;
  else if (indexes.size > 1)
    reason = "multiple-gpus-are-not-supported-by-the-v4-baseline";
  else if (baselineSamples.length < minimumNvidiaBaselineSamples) {
    reason = `nvidia-baseline-requires-${minimumNvidiaBaselineSamples}-samples`;
  } else if (runSamples.length === 0) reason = "nvidia-run-returned-no-samples";

  const baselineSummary = sampleSummary(baselineSamples);
  const subtraction = {
    gpu_utilization_percent:
      baselineSummary.gpu_utilization_percent?.p95 ?? null,
    memory_used_mib: baselineSummary.memory_used_mib?.max ?? null,
    power_draw_watts: baselineSummary.power_draw_watts?.median ?? null,
  };
  const adjustedSamples =
    reason === null
      ? runSamples.map((sample) => ({
          elapsed_ms: sample.elapsed_ms,
          timestamp: sample.timestamp,
          index: sample.index,
          gpu_utilization_percent: Math.max(
            0,
            sample.gpu_utilization_percent -
              subtraction.gpu_utilization_percent,
          ),
          memory_used_mib: Math.max(
            0,
            sample.memory_used_mib - subtraction.memory_used_mib,
          ),
          power_draw_watts: Math.max(
            0,
            sample.power_draw_watts - subtraction.power_draw_watts,
          ),
        }))
      : [];
  return {
    supported: reason === null,
    command_available: commandAvailable,
    reason,
    scope:
      "whole-gpu; benchmark application was the only application under test",
    requested_interval_ms: intervalMs,
    baseline_duration_ms: baselineDurationMs,
    invalid_lines: invalidLines,
    transition_samples: transitionSamples,
    // Preserve the original run-only surface for v3 diagnostics.
    samples: runSamples,
    summary: sampleSummary(runSamples),
    baseline: {
      sample_count: baselineSamples.length,
      samples: baselineSamples,
      summary: baselineSummary,
    },
    run: {
      sample_count: runSamples.length,
      samples: runSamples,
      summary: sampleSummary(runSamples),
    },
    baseline_adjusted: {
      sample_count: adjustedSamples.length,
      subtraction_policy:
        "subtract baseline p95 utilization, maximum memory, and median power from each run sample; clamp each result at zero",
      subtraction,
      floors: {
        gpu_utilization_percent: 0,
        memory_used_mib: 0,
        power_draw_watts: 0,
      },
      samples: adjustedSamples,
      summary: sampleSummary(adjustedSamples),
    },
  };
}

export function qualifyNvidiaEvidence(result, { required = false } = {}) {
  const evidenceRequired = required || result?.command_available === true;
  const passed = !evidenceRequired || result?.supported === true;
  return {
    required: evidenceRequired,
    passed,
    status: passed
      ? evidenceRequired
        ? "passed-required-nvidia-evidence"
        : "not-applicable-no-nvidia"
      : "failed-required-nvidia-evidence",
    blocker: passed
      ? null
      : (result?.reason ?? "required NVIDIA evidence is missing"),
  };
}

export function summarizeNvidiaIterations(iterations) {
  const baseline = iterations.flatMap(
    (iteration) => iteration.gpu?.baseline?.samples ?? [],
  );
  const run = iterations.flatMap(
    (iteration) => iteration.gpu?.run?.samples ?? [],
  );
  const adjusted = iterations.flatMap(
    (iteration) => iteration.gpu?.baseline_adjusted?.samples ?? [],
  );
  const qualificationPassed = iterations.every(
    (iteration) => iteration.gpu?.qualification?.passed === true,
  );
  return {
    gpu_whole_device_baseline: {
      sample_count: baseline.length,
      ...runnerSampleSummary(baseline),
    },
    gpu_whole_device: {
      sample_count: run.length,
      ...runnerSampleSummary(run),
    },
    gpu_whole_device_baseline_adjusted: {
      sample_count: adjusted.length,
      qualification_passed: qualificationPassed,
      subtraction_policy:
        "per iteration: subtract baseline p95 utilization, maximum memory, and median power; clamp at zero",
      floors: {
        gpu_utilization_percent: 0,
        memory_used_mib: 0,
        power_draw_watts: 0,
      },
      ...runnerSampleSummary(adjusted),
    },
  };
}

export async function startNvidiaBaselineRunSampler({
  intervalMs = nvidiaSampleIntervalMs,
  baselineDurationMs = nvidiaBaselineDurationMs,
} = {}) {
  const samplerStarted = process.hrtime.bigint();
  const baselineSamples = [];
  const transitionSamples = [];
  const runSamples = [];
  const invalidLines = [];
  let phase = "baseline";
  let buffer = "";
  let spawnError = null;
  let unavailableResolve;
  const unavailable = new Promise((resolvePromise) => {
    unavailableResolve = resolvePromise;
  });
  const child = spawn(
    "nvidia-smi",
    [
      `--query-gpu=${queryFields.join(",")}`,
      "--format=csv,noheader,nounits",
      `--loop-ms=${intervalMs}`,
    ],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  child.once("error", (error) => {
    spawnError = error.message;
    unavailableResolve();
  });
  child.once("close", () => unavailableResolve());
  child.stdout?.setEncoding("utf8");
  child.stdout?.on("data", (chunk) => {
    buffer += chunk;
    const lines = buffer.split("\n");
    buffer = lines.pop() ?? "";
    for (const line of lines) {
      if (!line.trim()) continue;
      const elapsedMs = Number(process.hrtime.bigint() - samplerStarted) / 1e6;
      const sample = parseNvidiaSample(line, elapsedMs);
      if (sample) {
        if (phase === "baseline") baselineSamples.push(sample);
        else if (phase === "run") runSamples.push(sample);
        else transitionSamples.push(sample);
      } else invalidLines.push(line);
    }
  });
  await Promise.race([
    new Promise((resolvePromise) =>
      setTimeout(resolvePromise, baselineDurationMs),
    ),
    unavailable,
  ]);
  phase = "transition";
  return {
    startRun() {
      phase = "run";
    },
    async stop() {
      if (child.exitCode === null && child.signalCode === null) {
        child.kill("SIGTERM");
        await Promise.race([
          new Promise((resolvePromise) => child.once("close", resolvePromise)),
          new Promise((resolvePromise) => setTimeout(resolvePromise, 2_000)),
        ]);
      }
      const result = buildNvidiaBaselineResult({
        baselineSamples,
        transitionSamples,
        runSamples,
        invalidLines,
        spawnError,
        baselineDurationMs,
        intervalMs,
      });
      result.qualification = qualifyNvidiaEvidence(result, {
        required: process.env.BP_PERF_REQUIRE_NVIDIA === "1",
      });
      return result;
    },
  };
}

// Frozen v3 callers can retain the old run-only behavior. New v4-capable
// runners use startNvidiaBaselineRunSampler so the app starts after baseline.
export function startNvidiaSampler(startedMonotonic, intervalMs = 200) {
  const samples = [];
  const invalidLines = [];
  let buffer = "";
  let spawnError = null;
  const child = spawn(
    "nvidia-smi",
    [
      `--query-gpu=${queryFields.join(",")}`,
      "--format=csv,noheader,nounits",
      `--loop-ms=${intervalMs}`,
    ],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  child.once("error", (error) => {
    spawnError = error.message;
  });
  child.stdout?.setEncoding("utf8");
  child.stdout?.on("data", (chunk) => {
    buffer += chunk;
    const lines = buffer.split("\n");
    buffer = lines.pop() ?? "";
    for (const line of lines) {
      if (!line.trim()) continue;
      const elapsedMs =
        Number(process.hrtime.bigint() - startedMonotonic) / 1e6;
      const sample = parseNvidiaSample(line, elapsedMs);
      if (sample) samples.push(sample);
      else invalidLines.push(line);
    }
  });
  return {
    async stop() {
      if (child.exitCode === null && child.signalCode === null) {
        child.kill("SIGTERM");
        await Promise.race([
          new Promise((resolvePromise) => child.once("close", resolvePromise)),
          new Promise((resolvePromise) => setTimeout(resolvePromise, 2_000)),
        ]);
      }
      return {
        supported: spawnError === null && samples.length > 0,
        reason:
          spawnError ??
          (samples.length === 0 ? "nvidia-smi-returned-no-samples" : null),
        scope:
          "whole-gpu; benchmark application was the only application under test",
        requested_interval_ms: intervalMs,
        invalid_lines: invalidLines,
        samples,
        summary: sampleSummary(samples),
      };
    },
  };
}
