import assert from "node:assert/strict";
import test from "node:test";

import {
  buildDirectionalSafetyScheduleV7,
  directionalSafetyPlanSha256V7,
  directionalSafetyPlanV7,
} from "./directional-safety-v7.mjs";
import { analyzeDirectionalSafetyManifestV7 } from "./analyze-directional-safety-v7.mjs";

function completeManifest({ reverse = false, abort = false } = {}) {
  const schedule = buildDirectionalSafetyScheduleV7();
  return {
    schema_version: 1,
    protocol_version: "bp-perf-v7-directional-safety-1",
    source_v6: structuredClone(directionalSafetyPlanV7.source_v6),
    directional_safety_plan: structuredClone(directionalSafetyPlanV7),
    directional_safety_plan_sha256: directionalSafetyPlanSha256V7,
    complete: true,
    outcome: "completed",
    schedule,
    launches: schedule.map((run) => ({
      ...run,
      identity: `directional-safety-pair${run.pair}-${run.implementation}-high-zoom-pan`,
      termination: { bounded_wait_complete: true },
    })),
    trials: schedule.map((run) => ({
      pair: run.pair,
      implementation: run.implementation,
      outcome:
        abort && run.schedule_index === 0
          ? "ABORT"
          : reverse
            ? run.implementation === "electron"
              ? "PASS"
              : "FAIL"
            : run.implementation === "gpui"
              ? "PASS"
              : "FAIL",
      structural_failures:
        abort && run.schedule_index === 0 ? ["session changed"] : [],
      product_failures:
        run.implementation === (reverse ? "gpui" : "electron")
          ? ["candidate did not pass"]
          : [],
      measurements: {
        cgroup_peak_memory_bytes:
          run.implementation === "gpui" ? 450_000_000 : 700_000_000,
        application_frame_interval_p95_ms:
          run.implementation === "gpui" ? 16 : 17,
        baseline_adjusted_gpu_peak_memory_mib:
          run.implementation === "gpui" ? 500 : 3_600,
        baseline_adjusted_gpu_utilization_p95_percent:
          run.implementation === "gpui" ? 14 : 100,
      },
      known_baseline_defect_id: null,
    })),
  };
}

test("reports only a scoped GPUI directional advantage from six fresh pairs", async () => {
  const analysis = await analyzeDirectionalSafetyManifestV7(completeManifest());
  assert.equal(analysis.decision, "GPUI_DIRECTIONAL_ADVANTAGE");
  assert.equal(analysis.exact_test.two_sided_p_value, 0.03125);
  assert.equal(analysis.pass_counts.gpui, 6);
  assert.equal(analysis.pass_counts.electron, 0);
  assert.equal(
    analysis.descriptive_statistics.baseline_adjusted_gpu_peak_memory_mib.gpui
      .median,
    500,
  );
  assert.match(analysis.claim_limit, /not overall migration value/);
});

test("reports the symmetric Electron directional advantage", async () => {
  const analysis = await analyzeDirectionalSafetyManifestV7(
    completeManifest({ reverse: true }),
  );
  assert.equal(analysis.decision, "ELECTRON_DIRECTIONAL_ADVANTAGE");
});

test("makes any structural abort or incomplete schedule inconclusive", async () => {
  const aborted = await analyzeDirectionalSafetyManifestV7(
    completeManifest({ abort: true }),
  );
  assert.equal(aborted.decision, "INCONCLUSIVE");
  assert(
    aborted.structural_blockers.some((failure) => failure.includes("ABORT")),
  );

  const incomplete = completeManifest();
  incomplete.launches.pop();
  incomplete.trials.pop();
  const analysis = await analyzeDirectionalSafetyManifestV7(incomplete);
  assert.equal(analysis.decision, "INCONCLUSIVE");
  assert(
    analysis.structural_blockers.some((failure) => failure.includes("12")),
  );
});
