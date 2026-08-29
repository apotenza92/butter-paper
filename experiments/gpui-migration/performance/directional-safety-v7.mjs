import { validateGpuSamplesV5 } from "./run-paired-v5.mjs";
import { canonicalSha256 } from "./run-paired-v4.mjs";

export const directionalSafetyProtocolVersionV7 =
  "bp-perf-v7-directional-safety-1";
export const directionalSafetyPairCountV7 = 6;
export const directionalSafetyLaunchCountV7 = 12;
export const directionalSafetyScheduleSeedV7 = 0x4250_5637;
export const directionalSafetyCooldownMsV7 = 2_000;

export const directionalSafetyMilestonesV7 = Object.freeze([
  "timestamped-input-complete",
  "visible-tiles-bounded",
  "stale-generations-presented-zero",
  "settled-density-at-least-1",
]);

export const directionalSafetyThresholdsV7 = Object.freeze({
  cgroup_peak_memory_bytes: 1.5 * 1024 * 1024 * 1024,
  application_frame_interval_p95_ms: 25,
  baseline_adjusted_gpu_peak_memory_mib: 2_048,
  baseline_adjusted_gpu_utilization_p95_percent: 100,
});

const pairOrders = Object.freeze([
  Object.freeze(["electron", "gpui"]),
  Object.freeze(["gpui", "electron"]),
  Object.freeze(["electron", "gpui"]),
  Object.freeze(["gpui", "electron"]),
  Object.freeze(["gpui", "electron"]),
  Object.freeze(["electron", "gpui"]),
]);

export const directionalSafetyPlanV7 = Object.freeze({
  schema_version: 1,
  protocol_version: directionalSafetyProtocolVersionV7,
  source_v6: Object.freeze({
    protocol_version: "bp-perf-v6",
    scenario_contract_version: "bp-perf-v6-representative-1",
    manifest_id: "bp-perf-v6-decision-2",
    workload_byte_sha256:
      "fc7e3cb6f09b74e004a24b01a9f5ccbb444d98feb9ae6489885e27329a442147",
    disposition: "failed-closed-and-not-reclassified",
  }),
  lane: Object.freeze({
    journey: "engineering-sheet",
    component: "high-zoom-pan",
    fixture_id: "bp-engineering-sheet-v1",
    fixture_sha256:
      "49b417e4652a5fc0efb3b59b1f482b443bf3133f810f652559931a08b68a2b91",
    input_lane: "semantic-diagnostic",
    zoom_percent: 1600,
    duration_ms: 5000,
    rate_hz: 120,
    required_milestones: directionalSafetyMilestonesV7,
  }),
  schedule: Object.freeze({
    pair_count: directionalSafetyPairCountV7,
    launch_count: directionalSafetyLaunchCountV7,
    pair_orders: pairOrders,
    cold_application_launch_per_trial: true,
    adjacent_pairs: true,
    replacement_launches: false,
    early_stop: false,
    cooldown_ms: directionalSafetyCooldownMsV7,
  }),
  thresholds: directionalSafetyThresholdsV7,
  decision: Object.freeze({
    test: "exact-two-sided-paired-sign",
    alpha: 0.05,
    gpui_directional_advantage: "gpui-pass-6-of-6-and-electron-pass-0-of-6",
    electron_directional_advantage: "electron-pass-6-of-6-and-gpui-pass-0-of-6",
    otherwise: "inconclusive",
  }),
  claim_limit:
    "directional Linux NVIDIA high-zoom rendering safety only; not overall migration value, generalized benefit, release, macOS, or Windows qualification",
});

export const directionalSafetyExpectedPlanSha256V7 =
  "74bab9b2380b44e4caba1c721f7a4a9384765caf9eebee500109e5648479aba7";
export const directionalSafetyPlanSha256V7 = canonicalSha256(
  directionalSafetyPlanV7,
);

export function buildDirectionalSafetyScheduleV7() {
  return pairOrders
    .flatMap((order, pairIndex) =>
      order.map((implementation, position) => ({
        phase: "directional-safety",
        lane_id: directionalSafetyProtocolVersionV7,
        inference_eligible: false,
        benefit_metrics_eligible: false,
        pair: pairIndex + 1,
        pair_position: position === 0 ? "first" : "second",
        implementation,
        bundle_id: null,
        component_index: 0,
        component_weight: 0,
        journey: "engineering-sheet",
        journey_id: "engineering-sheet-v1",
        component: "high-zoom-pan",
        input_lane: "semantic-diagnostic",
        hard_component: false,
        fixture_ids: ["bp-engineering-sheet-v1"],
      })),
    )
    .map((run, scheduleIndex) => ({ ...run, schedule_index: scheduleIndex }));
}

function finiteMetric(structuralFailures, measurements, metric, value) {
  if (!Number.isFinite(value)) {
    structuralFailures.push(`${metric} is missing or non-finite`);
    return;
  }
  measurements[metric] = value;
}

export function assessDirectionalSafetyTrialV7({
  launch,
  rawReport,
  semanticAssessment,
}) {
  const structuralFailures = [];
  const productFailures = [];
  const measurements = {};
  const boundedProductTermination =
    launch?.timed_out === true ||
    launch?.signal != null ||
    (Number.isInteger(launch?.exit_code) && launch.exit_code !== 0);
  if (!rawReport) {
    if (!boundedProductTermination)
      structuralFailures.push("raw report is absent");
  }
  if (!semanticAssessment) {
    if (!boundedProductTermination) {
      structuralFailures.push("semantic assessment is absent");
    }
  }
  if (launch?.spawn_error) {
    structuralFailures.push(`runner spawn failed: ${launch.spawn_error}`);
  }
  if (rawReport) {
    const gpu = validateGpuSamplesV5(rawReport);
    structuralFailures.push(...gpu.failures);
    const iteration = rawReport.iterations?.[0];
    if (
      rawReport.requested_iterations !== 1 ||
      rawReport.iterations?.length !== 1
    ) {
      structuralFailures.push("exactly one retained iteration is required");
    }
    finiteMetric(
      structuralFailures,
      measurements,
      "cgroup_peak_memory_bytes",
      rawReport.summary?.process_tree?.cgroup_memory_peak_bytes?.median,
    );
    finiteMetric(
      structuralFailures,
      measurements,
      "application_frame_interval_p95_ms",
      rawReport.summary?.application_frame_intervals_ms?.p95,
    );
    finiteMetric(
      structuralFailures,
      measurements,
      "baseline_adjusted_gpu_peak_memory_mib",
      rawReport.summary?.gpu_whole_device_baseline_adjusted?.memory_used_mib
        ?.max,
    );
    finiteMetric(
      structuralFailures,
      measurements,
      "baseline_adjusted_gpu_utilization_p95_percent",
      rawReport.summary?.gpu_whole_device_baseline_adjusted?.utilization_percent
        ?.p95,
    );
    const memoryEvents = iteration?.cgroup?.memory_events;
    if (!memoryEvents) {
      structuralFailures.push("cgroup memory events are absent");
    } else if (
      ["oom", "oom_kill", "oom_group_kill"].some(
        (event) => !Number.isInteger(memoryEvents[event]),
      )
    ) {
      structuralFailures.push("cgroup OOM counters are invalid");
    } else if (
      memoryEvents.oom > 0 ||
      memoryEvents.oom_kill > 0 ||
      memoryEvents.oom_group_kill > 0
    ) {
      productFailures.push("candidate caused a cgroup out-of-memory event");
    }
    if (
      rawReport.summary?.successful_iterations !== 1 ||
      rawReport.summary?.failed_iterations !== 0 ||
      iteration?.success !== true
    ) {
      productFailures.push("candidate iteration did not complete successfully");
    }
  }
  if (launch?.exit_code !== 0) {
    productFailures.push(`runner exit code ${launch?.exit_code}`);
  }
  if (launch?.signal != null) {
    productFailures.push(`runner exited on signal ${launch.signal}`);
  }
  if (launch?.timed_out === true) {
    productFailures.push("runner timed out");
  }
  if (
    semanticAssessment &&
    (semanticAssessment.passed !== true ||
      semanticAssessment.correctness_passed !== true)
  ) {
    productFailures.push(
      ...(semanticAssessment.failures?.length > 0
        ? semanticAssessment.failures
        : ["semantic correctness did not pass"]
      ).map((failure) => `semantic correctness: ${failure}`),
    );
  }
  if (semanticAssessment?.passed === true) {
    const receipts = semanticAssessment.receipts;
    const receipt =
      Array.isArray(receipts) && receipts.length === 1 ? receipts[0] : null;
    if (
      receipt?.command_id !== "engineering:pan" ||
      receipt?.live !== true ||
      receipt?.passed !== true ||
      receipt?.proof_class !== "live-app-semantic-exact" ||
      JSON.stringify(receipt?.milestone_ids) !==
        JSON.stringify(directionalSafetyMilestonesV7) ||
      JSON.stringify(receipt?.expected_milestone_ids) !==
        JSON.stringify(directionalSafetyMilestonesV7) ||
      JSON.stringify(receipt?.missing_milestone_ids) !== "[]"
    ) {
      structuralFailures.push(
        "live passed engineering:pan receipt with all four milestones is absent",
      );
    }
  }
  for (const [metric, maximum] of Object.entries(
    directionalSafetyThresholdsV7,
  )) {
    const value = measurements[metric];
    if (Number.isFinite(value) && value > maximum) {
      productFailures.push(`${metric} ${value} exceeds ${maximum}`);
    }
  }
  return {
    schema_version: 1,
    protocol_version: directionalSafetyProtocolVersionV7,
    pair: launch?.pair ?? null,
    implementation: launch?.implementation ?? null,
    outcome:
      structuralFailures.length > 0
        ? "ABORT"
        : productFailures.length > 0
          ? "FAIL"
          : "PASS",
    structural_failures: [...new Set(structuralFailures)],
    product_failures: [...new Set(productFailures)],
    measurements,
    known_baseline_defect_id: null,
  };
}

function combinations(n, k) {
  if (k < 0 || k > n) return 0;
  let result = 1;
  for (let index = 1; index <= Math.min(k, n - k); index += 1) {
    result = (result * (n - index + 1)) / index;
  }
  return result;
}

export function exactTwoSidedSignTestV7(gpuiWins, electronWins) {
  if (
    !Number.isInteger(gpuiWins) ||
    !Number.isInteger(electronWins) ||
    gpuiWins < 0 ||
    electronWins < 0
  ) {
    throw new Error("sign-test win counts must be nonnegative integers");
  }
  const discordantPairs = gpuiWins + electronWins;
  const tail = Math.min(gpuiWins, electronWins);
  let cumulative = 0;
  for (let wins = 0; wins <= tail; wins += 1) {
    cumulative += combinations(discordantPairs, wins);
  }
  const pValue =
    discordantPairs === 0
      ? 1
      : Math.min(1, (2 * cumulative) / 2 ** discordantPairs);
  return {
    discordant_pairs: discordantPairs,
    gpui_wins: gpuiWins,
    electron_wins: electronWins,
    two_sided_p_value: pValue,
  };
}

export function classifyDirectionalSafetyV7(pairs) {
  const structuralFailures = [];
  if (!Array.isArray(pairs) || pairs.length !== directionalSafetyPairCountV7) {
    structuralFailures.push("exactly six complete pairs are required");
  }
  const pairIds = new Set();
  let gpuiWins = 0;
  let electronWins = 0;
  for (const pair of pairs ?? []) {
    pairIds.add(pair?.pair);
    if (!["PASS", "FAIL"].includes(pair?.electron)) {
      structuralFailures.push(`pair ${pair?.pair} Electron outcome is invalid`);
    }
    if (!["PASS", "FAIL"].includes(pair?.gpui)) {
      structuralFailures.push(`pair ${pair?.pair} GPUI outcome is invalid`);
    }
    if (pair?.electron === "FAIL" && pair?.gpui === "PASS") gpuiWins += 1;
    if (pair?.electron === "PASS" && pair?.gpui === "FAIL") electronWins += 1;
  }
  if (
    pairIds.size !== directionalSafetyPairCountV7 ||
    [...pairIds].some((pair) => !Number.isInteger(pair) || pair < 1 || pair > 6)
  ) {
    structuralFailures.push("pair identities are incomplete or duplicated");
  }
  const exactTest = exactTwoSidedSignTestV7(gpuiWins, electronWins);
  const gpuiPasses = (pairs ?? []).filter(({ gpui }) => gpui === "PASS").length;
  const electronPasses = (pairs ?? []).filter(
    ({ electron }) => electron === "PASS",
  ).length;
  let decision = "INCONCLUSIVE";
  if (
    structuralFailures.length === 0 &&
    gpuiPasses === 6 &&
    electronPasses === 0 &&
    exactTest.two_sided_p_value <= 0.05
  ) {
    decision = "GPUI_DIRECTIONAL_ADVANTAGE";
  } else if (
    structuralFailures.length === 0 &&
    electronPasses === 6 &&
    gpuiPasses === 0 &&
    exactTest.two_sided_p_value <= 0.05
  ) {
    decision = "ELECTRON_DIRECTIONAL_ADVANTAGE";
  }
  return {
    decision,
    decision_scope: directionalSafetyPlanV7.claim_limit,
    pass_counts: { electron: electronPasses, gpui: gpuiPasses },
    exact_test: exactTest,
    structural_failures: [...new Set(structuralFailures)],
  };
}
