import assert from "node:assert/strict";
import test from "node:test";

import { analyzeV6Manifest, classifyV6Decision } from "./analyze-paired-v6.mjs";

test("classifies complete evidence distinctly from metric failure and structural blockage", () => {
  assert.equal(classifyV6Decision({ blockers: [], metricFailures: [] }), "YES");
  assert.equal(
    classifyV6Decision({ blockers: [], metricFailures: ["cpu regression"] }),
    "NO",
  );
  assert.equal(
    classifyV6Decision({ blockers: ["missing launch"], metricFailures: [] }),
    "BLOCKED",
  );
});

test("fails closed instead of deriving a decision from an incomplete manifest", async () => {
  const analysis = await analyzeV6Manifest(
    {
      protocol_version: "bp-perf-v6",
      scenario_contract_version: "bp-perf-v6-representative-1",
      manifest_id: "bp-perf-v6-decision-2",
      workload_byte_sha256:
        "fc7e3cb6f09b74e004a24b01a9f5ccbb444d98feb9ae6489885e27329a442147",
      settings: { schedule_seed: 0x4250_5636 },
      launches: [],
      bundles: [],
      view_state_pairs: [],
      correctness_reports: [],
      complete: false,
      outcome: "failed-closed",
    },
    { bootstrapSamples: 1 },
  );
  assert.equal(analysis.decision, "BLOCKED");
  assert(
    analysis.structural_blockers.some((failure) =>
      failure.includes("not a complete passed execution"),
    ),
  );
  assert.equal(
    analysis.schedule.correctness_excluded_from_benefit_statistics,
    true,
  );
});

test("surfaces the exact retained Electron engineering zoom baseline defect", async () => {
  const defect = {
    implementation: "electron",
    journey: "engineering-sheet",
    component: "zoom",
    passed: true,
    correctness_passed: false,
    known_baseline_defect_id:
      "electron-engineering-zoom-density-and-raster-bound-v1",
  };
  const analysis = await analyzeV6Manifest(
    {
      protocol_version: "bp-perf-v6",
      scenario_contract_version: "bp-perf-v6-representative-1",
      manifest_id: "bp-perf-v6-decision-2",
      workload_byte_sha256:
        "fc7e3cb6f09b74e004a24b01a9f5ccbb444d98feb9ae6489885e27329a442147",
      settings: { schedule_seed: 0x4250_5636 },
      launches: [],
      bundles: [],
      view_state_pairs: [],
      correctness_reports: [defect],
      complete: false,
      outcome: "failed-closed",
    },
    { bootstrapSamples: 1 },
  );

  assert.deepEqual(analysis.correctness.known_baseline_defects, [defect]);
  assert.equal(analysis.decision, "BLOCKED");
});
