import assert from "node:assert/strict";
import test from "node:test";

import {
  buildNvidiaBaselineResult,
  parseNvidiaSample,
  qualifyNvidiaEvidence,
  summarizeNvidiaIterations,
} from "./nvidia-sampler.mjs";

function sample({ elapsed = 0, utilization = 0, memory = 0, power = 0 } = {}) {
  return {
    elapsed_ms: elapsed,
    timestamp: "2026/08/23 12:00:00.123",
    index: 0,
    gpu_utilization_percent: utilization,
    memory_used_mib: memory,
    memory_total_mib: 20_475,
    power_draw_watts: power,
  };
}

test("parses the unitless NVIDIA CSV record", () => {
  assert.deepEqual(
    parseNvidiaSample("2026/08/23 12:00:00.123, 0, 47, 812, 20475, 31.25", 250),
    {
      elapsed_ms: 250,
      timestamp: "2026/08/23 12:00:00.123",
      index: 0,
      gpu_utilization_percent: 47,
      memory_used_mib: 812,
      memory_total_mib: 20475,
      power_draw_watts: 31.25,
    },
  );
});

test("subtracts the fixed pre-launch baseline and clamps adjusted samples at zero", () => {
  const result = buildNvidiaBaselineResult({
    baselineSamples: Array.from({ length: 5 }, (_, index) =>
      sample({
        elapsed: index * 200,
        utilization: 2,
        memory: 100,
        power: 10,
      }),
    ),
    transitionSamples: [
      sample({ elapsed: 1_050, utilization: 99, memory: 999, power: 99 }),
    ],
    runSamples: [
      sample({ elapsed: 1_200, utilization: 12, memory: 125, power: 15 }),
      sample({ elapsed: 1_400, utilization: 1, memory: 90, power: 8 }),
    ],
  });
  assert.equal(result.supported, true);
  assert.equal(result.baseline.sample_count, 5);
  assert.equal(result.run.sample_count, 2);
  assert.equal(result.transition_samples.length, 1);
  assert.equal(
    result.run.samples.some(({ memory_used_mib: memory }) => memory === 999),
    false,
  );
  assert.deepEqual(result.baseline_adjusted.subtraction, {
    gpu_utilization_percent: 2,
    memory_used_mib: 100,
    power_draw_watts: 10,
  });
  assert.deepEqual(
    result.baseline_adjusted.samples.map((entry) => ({
      utilization: entry.gpu_utilization_percent,
      memory: entry.memory_used_mib,
      power: entry.power_draw_watts,
    })),
    [
      { utilization: 10, memory: 25, power: 5 },
      { utilization: 0, memory: 0, power: 0 },
    ],
  );
});

test("requires complete baseline and run evidence when NVIDIA is available or explicitly required", () => {
  const unavailable = buildNvidiaBaselineResult({
    baselineSamples: [],
    runSamples: [],
    spawnError: "spawn nvidia-smi ENOENT",
  });
  assert.deepEqual(qualifyNvidiaEvidence(unavailable), {
    required: false,
    passed: true,
    status: "not-applicable-no-nvidia",
    blocker: null,
  });
  assert.equal(
    qualifyNvidiaEvidence(unavailable, { required: true }).passed,
    false,
  );

  const incompleteAvailable = buildNvidiaBaselineResult({
    baselineSamples: [sample()],
    runSamples: [],
  });
  const qualification = qualifyNvidiaEvidence(incompleteAvailable);
  assert.equal(qualification.required, true);
  assert.equal(qualification.passed, false);
  assert.match(qualification.blocker, /baseline-requires-3-samples/);
});

test("emits the exact baseline-adjusted summary surface consumed by the v4 analyzer", () => {
  const gpu = buildNvidiaBaselineResult({
    baselineSamples: Array.from({ length: 5 }, () =>
      sample({
        utilization: 2,
        memory: 100,
        power: 10,
      }),
    ),
    runSamples: [sample({ utilization: 12, memory: 125, power: 15 })],
  });
  gpu.qualification = qualifyNvidiaEvidence(gpu);
  const summary = summarizeNvidiaIterations([{ gpu }]);
  assert.equal(summary.gpu_whole_device_baseline.sample_count, 5);
  assert.equal(summary.gpu_whole_device.sample_count, 1);
  assert.equal(summary.gpu_whole_device_baseline_adjusted.sample_count, 1);
  assert.equal(
    summary.gpu_whole_device_baseline_adjusted.utilization_percent.p95,
    10,
  );
  assert.equal(
    summary.gpu_whole_device_baseline_adjusted.memory_used_mib.max,
    25,
  );
  assert.equal(
    summary.gpu_whole_device_baseline_adjusted.qualification_passed,
    true,
  );
});
