import assert from "node:assert/strict";
import test from "node:test";

import {
  assessDirectionalSafetyTrialV7,
  buildDirectionalSafetyScheduleV7,
  classifyDirectionalSafetyV7,
  directionalSafetyExpectedPlanSha256V7,
  directionalSafetyPairCountV7,
  directionalSafetyPlanSha256V7,
  exactTwoSidedSignTestV7,
} from "./directional-safety-v7.mjs";

function passingRawReport(overrides = {}) {
  return {
    requested_iterations: 1,
    summary: {
      successful_iterations: 1,
      failed_iterations: 0,
      application_frame_intervals_ms: { p95: 16.7 },
      process_tree: {
        cgroup_memory_peak_bytes: { median: 448_065_536 },
      },
      gpu_whole_device_baseline_adjusted: {
        qualification_passed: true,
        sample_count: 1,
        memory_used_mib: { max: 499 },
        utilization_percent: { p95: 14 },
      },
    },
    iterations: [
      {
        success: true,
        timed_out: false,
        cgroup: {
          memory_events: { oom: 0, oom_kill: 0, oom_group_kill: 0 },
        },
        gpu: {
          qualification: { required: true, passed: true },
          baseline: {
            sample_count: 5,
            samples: Array.from({ length: 5 }, () => ({ index: 0 })),
          },
          run: { sample_count: 1, samples: [{ index: 0 }] },
          baseline_adjusted: { sample_count: 1, samples: [{ index: 0 }] },
        },
      },
    ],
    ...overrides,
  };
}

function launch(implementation, pair) {
  return {
    schedule_index: (pair - 1) * 2,
    pair,
    implementation,
    component: "high-zoom-pan",
    journey: "engineering-sheet",
    exit_code: 0,
    signal: null,
    timed_out: false,
    spawn_error: null,
  };
}

function passedSemanticAssessment() {
  return {
    passed: true,
    correctness_passed: true,
    failures: [],
    receipts: [
      {
        command_id: "engineering:pan",
        live: true,
        passed: true,
        milestone_ids: [
          "timestamped-input-complete",
          "visible-tiles-bounded",
          "stale-generations-presented-zero",
          "settled-density-at-least-1",
        ],
        expected_milestone_ids: [
          "timestamped-input-complete",
          "visible-tiles-bounded",
          "stale-generations-presented-zero",
          "settled-density-at-least-1",
        ],
        missing_milestone_ids: [],
        proof_class: "live-app-semantic-exact",
      },
    ],
  };
}

test("freezes six balanced fresh high-zoom pairs", () => {
  const schedule = buildDirectionalSafetyScheduleV7();
  assert.equal(directionalSafetyPairCountV7, 6);
  assert.equal(schedule.length, 12);
  assert.deepEqual(
    schedule.map(({ pair, implementation }) => [pair, implementation]),
    [
      [1, "electron"],
      [1, "gpui"],
      [2, "gpui"],
      [2, "electron"],
      [3, "electron"],
      [3, "gpui"],
      [4, "gpui"],
      [4, "electron"],
      [5, "gpui"],
      [5, "electron"],
      [6, "electron"],
      [6, "gpui"],
    ],
  );
  assert(
    schedule.every(
      (run) =>
        run.phase === "directional-safety" &&
        run.inference_eligible === false &&
        run.benefit_metrics_eligible === false &&
        run.journey === "engineering-sheet" &&
        run.component === "high-zoom-pan" &&
        run.input_lane === "semantic-diagnostic" &&
        run.fixture_ids.join() === "bp-engineering-sheet-v1",
    ),
  );
});

test("freezes the reviewed directional-safety plan hash", () => {
  assert.match(directionalSafetyExpectedPlanSha256V7, /^[0-9a-f]{64}$/);
  assert.equal(
    directionalSafetyPlanSha256V7,
    directionalSafetyExpectedPlanSha256V7,
  );
});

test("assesses exact correctness and absolute resource safety without a defect exception", () => {
  const passed = assessDirectionalSafetyTrialV7({
    launch: launch("gpui", 1),
    rawReport: passingRawReport(),
    semanticAssessment: passedSemanticAssessment(),
  });
  assert.equal(passed.outcome, "PASS");
  assert.equal(passed.structural_failures.length, 0);
  assert.equal(passed.product_failures.length, 0);

  const failed = assessDirectionalSafetyTrialV7({
    launch: { ...launch("electron", 1), exit_code: 1 },
    rawReport: passingRawReport({
      summary: {
        ...passingRawReport().summary,
        successful_iterations: 0,
        failed_iterations: 1,
        gpu_whole_device_baseline_adjusted: {
          qualification_passed: true,
          memory_used_mib: { max: 3_645 },
          utilization_percent: { p95: 100 },
        },
      },
      iterations: [
        {
          ...passingRawReport().iterations[0],
          success: false,
          failure: "GPU process is not usable",
        },
      ],
    }),
    semanticAssessment: {
      passed: false,
      correctness_passed: false,
      failures: ["high zoom milestones did not pass"],
    },
  });
  assert.equal(failed.outcome, "FAIL");
  assert.equal(failed.structural_failures.length, 0);
  assert(
    failed.product_failures.some((failure) => failure.includes("exit code 1")),
  );
  assert(
    failed.product_failures.some((failure) =>
      failure.includes("baseline_adjusted_gpu_peak_memory_mib"),
    ),
  );
  assert.equal(failed.known_baseline_defect_id, null);
});

test("keeps absent authenticated evidence structural instead of counting it as a product loss", () => {
  const assessment = assessDirectionalSafetyTrialV7({
    launch: launch("electron", 1),
    rawReport: null,
    semanticAssessment: null,
  });
  assert.equal(assessment.outcome, "ABORT");
  assert(assessment.structural_failures.includes("raw report is absent"));
});

test("requires six all-discordant pairs for a two-sided directional decision", () => {
  assert.deepEqual(exactTwoSidedSignTestV7(6, 0), {
    discordant_pairs: 6,
    gpui_wins: 6,
    electron_wins: 0,
    two_sided_p_value: 0.03125,
  });
  const yes = classifyDirectionalSafetyV7(
    Array.from({ length: 6 }, (_, index) => ({
      pair: index + 1,
      electron: "FAIL",
      gpui: "PASS",
    })),
  );
  assert.equal(yes.decision, "GPUI_DIRECTIONAL_ADVANTAGE");
  assert.equal(yes.exact_test.two_sided_p_value, 0.03125);

  const no = classifyDirectionalSafetyV7(
    Array.from({ length: 6 }, (_, index) => ({
      pair: index + 1,
      electron: "PASS",
      gpui: "FAIL",
    })),
  );
  assert.equal(no.decision, "ELECTRON_DIRECTIONAL_ADVANTAGE");

  const mixed = classifyDirectionalSafetyV7([
    ...Array.from({ length: 5 }, (_, index) => ({
      pair: index + 1,
      electron: "FAIL",
      gpui: "PASS",
    })),
    { pair: 6, electron: "PASS", gpui: "PASS" },
  ]);
  assert.equal(mixed.decision, "INCONCLUSIVE");
  assert.equal(mixed.exact_test.two_sided_p_value, 0.0625);
});
