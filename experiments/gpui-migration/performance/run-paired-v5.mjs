#!/usr/bin/env node

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

import {
  comparisonWorkloadArtifactHashV5,
  comparisonWorkloadByteHashV5,
  loadMaterializedComparisonWorkloadV5,
  validateComparisonWorkloadV5,
} from "./comparison-workload-v5.mjs";
import {
  decisionContractArtifactHashV5,
  decisionContractV5,
  decisionContractVersionV5,
  hardComponentIdsV5,
  validateDecisionContractV5,
} from "./decision-contract-v5.mjs";
import { createBalancedPairOrders } from "./decision-statistics.mjs";
import {
  buildViewStateReceiptV5,
  compareBundleViewStatesV5,
} from "./matched-view-state-v5.mjs";
import { summarizeNvidiaIterations } from "./nvidia-sampler.mjs";
import { validateOptimizedCandidatesV4 } from "./optimized-candidates-v4.mjs";
import {
  buildV4ComparisonPlan,
  canonicalSha256,
  componentInputLaneV4,
  runnerInputLaneV4,
  validateV4ComponentReport,
} from "./run-paired-v4.mjs";
import { buildScenarioContractV4 } from "./scenario-contract-v4.mjs";
import {
  protocolVersionV5,
  representativeScenarioDefinitionsV5,
  representativeTimedScenarioIdsV5,
  scenarioContractVersionV5,
  validateRepresentativeScenarioDefinitionsV5,
} from "./scenario-contract-v5.mjs";
import { extractComponentMeasurementsV4 } from "./summarize-paired-v4.mjs";
import {
  commonBenefitTimingBoundaryPassedV5,
  hardComponentEvidenceContractV5,
  validateHardComponentReportV5,
} from "./summarize-paired-v5.mjs";

const performanceDirectory = dirname(fileURLToPath(import.meta.url));
const materializedWorkloadPath = resolve(
  performanceDirectory,
  "comparison-workload-v5.materialized.json",
);
const referenceCropDirectory = resolve(
  performanceDirectory,
  "fixtures/reference-crops-v5",
);
const implementations = Object.freeze(["electron", "gpui"]);
const hardComponents = new Set(hardComponentIdsV5);
const propertyComponent = "native-property-edit-undo";
const exactElectronMultiDocumentDefectV5 =
  "electron-multi-document-second-nasa-visible-pages-empty-v1";
const sha256Pattern = /^[0-9a-f]{64}$/;
const outputLimitBytes = 1_000_000;

export const calibrationPairCountV5 = 6;
export const finalPairCountV5 = 24;
export const finalPairBlockSizeV5 = 4;
export const expectedWorkloadArtifactSha256V5 =
  "cc4f8b8940556390b8d16a6baae43e8a5a022541fba90beea08869e692ee920e";
export const expectedWorkloadByteSha256V5 =
  "e7b2540c7d455a30e52ee64a6819745fe0ad49a6512f887df4631aac72054f6d";
export const expectedDecisionContractSha256V5 =
  "2acdab1dc3f62c1eed82f5d9af9f50c525617cac49c3c4b60fd885116563cfb1";

export const expectedComponentDurationMsV5 = Object.freeze({
  "open-pdf": 12_000,
  "viewer-layout": 10_000,
  "page-navigation": 12_000,
  "continuous-scroll": 25_000,
  "cache-pressure": 18_000,
  "close-reopen": 15_000,
  "viewer-dynamic-fidelity": 45_000,
  "fit-modes": 10_000,
  zoom: 12_000,
  "high-zoom-pan": 15_000,
  "cache-pressure-recovery": 20_000,
  "annotation-create": 15_000,
  "annotation-transform": 15_000,
  "annotation-properties-history": 12_000,
  "editor-create": 15_000,
  "editor-workload": 20_000,
  "native-property-edit-undo": 15_000,
  "native-snap-transform-120hz": 15_000,
  "persistence-workload": 25_000,
  "multi-document-session": 30_000,
});

function parsePositiveInteger(value, option) {
  if (!/^\d+$/.test(value ?? "") || Number(value) < 1) {
    throw new Error(`${option} must be a positive integer`);
  }
  return Number(value);
}

function parseNonnegativeInteger(value, option) {
  if (!/^\d+$/.test(value ?? "")) {
    throw new Error(`${option} must be a nonnegative integer`);
  }
  return Number(value);
}

function parsePositiveNumber(value, option) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`${option} must be a positive number`);
  }
  return parsed;
}

function parseAssignment(value, option) {
  const separator = value.indexOf("=");
  if (separator <= 0 || separator === value.length - 1) {
    throw new Error(`${option} must be <name>=<value>`);
  }
  return [value.slice(0, separator), value.slice(separator + 1)];
}

export function parseV5Arguments(argv) {
  const options = {
    mode: "plan",
    calibrationPairs: calibrationPairCountV5,
    finalPairs: finalPairCountV5,
    timeoutMs: 120_000,
    cooldownMs: 2_000,
    cleanupGraceMs: 15 * 60_000,
    taskHeadroomPercent: 35,
    seed: 0x4250_5635,
    fixtures: new Map(),
    expectedDurations: new Map(),
    referenceCropDirectory,
  };
  const valueOptions = new Set([
    "--output",
    "--plan-output",
    "--fixture",
    "--reference-crop-directory",
    "--electron",
    "--gpui-binary",
    "--electron-candidate-artifact",
    "--gpui-candidate-artifact",
    "--electron-candidate-sha256",
    "--gpui-candidate-sha256",
    "--hourly-usd",
    "--calibration-pairs",
    "--final-pairs",
    "--timeout-ms",
    "--cooldown-ms",
    "--cleanup-grace-ms",
    "--task-headroom-percent",
    "--task-limit-ms",
    "--lease-ttl-ms",
    "--seed",
    "--expected-component-ms",
  ]);
  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index];
    if (option === "-h" || option === "--help") return { help: true };
    if (option === "--execute") {
      options.mode = "execute";
      continue;
    }
    if (option === "--plan") {
      options.mode = "plan";
      continue;
    }
    if (!valueOptions.has(option)) throw new Error(`unknown option: ${option}`);
    const value = argv[++index];
    if (value === undefined || value.startsWith("--")) {
      throw new Error(`${option} requires a value`);
    }
    if (option === "--output") options.output = resolve(value);
    if (option === "--plan-output") options.planOutput = resolve(value);
    if (option === "--reference-crop-directory") {
      options.referenceCropDirectory = resolve(value);
    }
    if (option === "--electron") options.electron = resolve(value);
    if (option === "--gpui-binary") options.gpuiBinary = resolve(value);
    if (option === "--electron-candidate-artifact") {
      options.electronCandidateArtifact = resolve(value);
    }
    if (option === "--gpui-candidate-artifact") {
      options.gpuiCandidateArtifact = resolve(value);
    }
    if (option === "--electron-candidate-sha256") {
      options.electronCandidateSha256 = value;
    }
    if (option === "--gpui-candidate-sha256") {
      options.gpuiCandidateSha256 = value;
    }
    if (option === "--hourly-usd") {
      options.hourlyUsd = parsePositiveNumber(value, option);
    }
    if (option === "--calibration-pairs") {
      options.calibrationPairs = parsePositiveInteger(value, option);
    }
    if (option === "--final-pairs") {
      options.finalPairs = parsePositiveInteger(value, option);
    }
    if (option === "--timeout-ms") {
      options.timeoutMs = parsePositiveInteger(value, option);
    }
    if (option === "--cooldown-ms") {
      options.cooldownMs = parseNonnegativeInteger(value, option);
    }
    if (option === "--cleanup-grace-ms") {
      options.cleanupGraceMs = parsePositiveInteger(value, option);
    }
    if (option === "--task-headroom-percent") {
      options.taskHeadroomPercent = parsePositiveNumber(value, option);
    }
    if (option === "--task-limit-ms") {
      options.taskLimitMs = parsePositiveInteger(value, option);
    }
    if (option === "--lease-ttl-ms") {
      options.leaseTtlMs = parsePositiveInteger(value, option);
    }
    if (option === "--seed") options.seed = Number(value);
    if (option === "--fixture") {
      const [fixtureId, path] = parseAssignment(value, option);
      if (options.fixtures.has(fixtureId)) {
        throw new Error(`duplicate --fixture ${fixtureId}`);
      }
      options.fixtures.set(fixtureId, resolve(path));
    }
    if (option === "--expected-component-ms") {
      const [component, milliseconds] = parseAssignment(value, option);
      if (options.expectedDurations.has(component)) {
        throw new Error(`duplicate expected duration for ${component}`);
      }
      options.expectedDurations.set(
        component,
        parsePositiveInteger(milliseconds, option),
      );
    }
  }

  if (!options.output) throw new Error("--output is required");
  if (!options.electron) throw new Error("--electron is required");
  if (!options.gpuiBinary) throw new Error("--gpui-binary is required");
  if (!options.electronCandidateArtifact) {
    throw new Error("--electron-candidate-artifact is required");
  }
  if (!options.gpuiCandidateArtifact) {
    throw new Error("--gpui-candidate-artifact is required");
  }
  for (const implementation of implementations) {
    const hash = options[`${implementation}CandidateSha256`];
    if (!sha256Pattern.test(hash ?? "")) {
      throw new Error(`--${implementation}-candidate-sha256 is required`);
    }
  }
  if (!Number.isFinite(options.hourlyUsd)) {
    throw new Error("--hourly-usd is required for paid lease estimates");
  }
  if (options.calibrationPairs !== calibrationPairCountV5) {
    throw new Error(
      `--calibration-pairs is frozen at ${calibrationPairCountV5}`,
    );
  }
  if (options.finalPairs !== finalPairCountV5) {
    throw new Error(`--final-pairs is frozen at ${finalPairCountV5}`);
  }
  if (options.finalPairs % finalPairBlockSizeV5 !== 0) {
    throw new Error("--final-pairs must be a balanced block of four");
  }
  if (!Number.isInteger(options.seed))
    throw new Error("--seed must be an integer");
  if (options.mode === "execute") {
    if (!options.taskLimitMs) {
      throw new Error(
        "--execute requires --task-limit-ms from a reviewed plan",
      );
    }
    if (!options.leaseTtlMs) {
      throw new Error("--execute requires --lease-ttl-ms from a reviewed plan");
    }
    if (options.leaseTtlMs < options.taskLimitMs + options.cleanupGraceMs) {
      throw new Error(
        "lease TTL must include the task limit and cleanup grace",
      );
    }
  }
  return options;
}

export function createCalibrationPairOrdersV5(seed) {
  if (!Number.isInteger(seed)) throw new Error("seed must be an integer");
  const orders = [
    ["electron", "gpui"],
    ["electron", "gpui"],
    ["electron", "gpui"],
    ["gpui", "electron"],
    ["gpui", "electron"],
    ["gpui", "electron"],
  ];
  let state = seed >>> 0;
  const random = () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return (state >>> 0) / 0x1_0000_0000;
  };
  for (let index = orders.length - 1; index > 0; index -= 1) {
    const swap = Math.floor(random() * (index + 1));
    [orders[index], orders[swap]] = [orders[swap], orders[index]];
  }
  return orders;
}

function exactV5Hashes(workload) {
  return (
    comparisonWorkloadArtifactHashV5(workload) ===
      expectedWorkloadArtifactSha256V5 &&
    comparisonWorkloadByteHashV5(workload) === expectedWorkloadByteSha256V5 &&
    decisionContractArtifactHashV5(decisionContractV5) ===
      expectedDecisionContractSha256V5
  );
}

export function buildV5ComparisonPlan(workload) {
  const blockers = [
    ...validateDecisionContractV5(decisionContractV5),
    ...validateComparisonWorkloadV5(workload),
    ...validateRepresentativeScenarioDefinitionsV5(),
  ];
  if (!exactV5Hashes(workload)) {
    blockers.push("final v5 workload or decision contract hash changed");
  }
  const journeys = representativeTimedScenarioIdsV5.map((scenario) => {
    const definition = representativeScenarioDefinitionsV5[scenario];
    const benefitComponents = definition.current_runner_components.filter(
      (component) => component !== propertyComponent,
    );
    const benefitCommandIds = benefitComponents.flatMap(
      (component) => definition.component_command_ids[component],
    );
    return {
      scenario,
      journey_id: definition.journey_id,
      fixture_ids: [...definition.fixture_ids],
      fixture_sha256_by_id: { ...definition.fixture_sha256_by_id },
      component_order: benefitComponents,
      component_weights: benefitComponents.map(
        () => 1 / benefitComponents.length,
      ),
      component_command_ids: Object.fromEntries(
        benefitComponents.map((component) => [
          component,
          [...definition.component_command_ids[component]],
        ]),
      ),
      component_fixture_ids: Object.fromEntries(
        benefitComponents.map((component) => [
          component,
          [...definition.component_fixture_ids[component]],
        ]),
      ),
      command_ids: benefitCommandIds,
      excluded_correctness_components: definition.current_runner_components
        .filter((component) => component === propertyComponent)
        .map((component) => ({
          component,
          command_ids: [...definition.component_command_ids[component]],
          reason:
            "one native correctness run per candidate; benefit metrics ineligible",
        })),
    };
  });
  if (journeys.length !== 6)
    blockers.push("v5 must contain exactly six journeys");
  const observedHard = new Set(
    Object.values(representativeScenarioDefinitionsV5).flatMap((definition) =>
      definition.current_runner_components.filter((component) =>
        hardComponents.has(component),
      ),
    ),
  );
  for (const component of hardComponentIdsV5) {
    if (!observedHard.has(component))
      blockers.push(`missing hard component ${component}`);
  }
  return {
    protocol_version: protocolVersionV5,
    decision_contract_version: decisionContractVersionV5,
    scenario_contract_version: scenarioContractVersionV5,
    manifest_id: workload?.manifest_id ?? null,
    workload_artifact_sha256: comparisonWorkloadArtifactHashV5(workload),
    workload_byte_sha256: comparisonWorkloadByteHashV5(workload),
    decision_contract_sha256:
      decisionContractArtifactHashV5(decisionContractV5),
    ready: blockers.length === 0,
    journeys,
    hard_components: [...hardComponentIdsV5],
    property_correctness: {
      component: propertyComponent,
      scenario: "dense-mixed-editing",
      executions_per_candidate: 1,
      benefit_metrics_eligible: false,
      electron_required_outcome:
        "normal-one-undo-pass-or-electron-numeric-property-input-blur-duplicate-history-v1",
      gpui_required_outcome: "one-effective-revision-and-one-exact-undo",
    },
    calibration: { pairs_per_journey: 6, inference_eligible: false },
    final: { pairs_per_journey: 24, block_size: 4, inference_eligible: true },
    excluded_lanes: {
      private_hibbeler_935: "blocked-not-transferred",
      usgs_large_sheet_stress: "not-scheduled-noninferential",
      macos_visual_capture: "blocked-not-run-on-linux-gpu-lane",
    },
    blockers,
  };
}

export function buildV5ExecutionSchedule(plan, { seed }) {
  if (plan?.ready !== true || plan.blockers?.length > 0) {
    throw new Error("BLOCKED v5 paired comparison plan");
  }
  const schedule = [];
  const correctnessOrder = createBalancedPairOrders({ pairCount: 4, seed })[0];
  for (const [position, implementation] of correctnessOrder.entries()) {
    schedule.push({
      phase: "correctness",
      inference_eligible: false,
      journey: "dense-mixed-editing",
      journey_id: "dense-mixed-editing-v1",
      pair: 0,
      pair_position: position === 0 ? "first" : "second",
      implementation,
      bundle_id: null,
      component: propertyComponent,
      component_index: 0,
      component_weight: 0,
      hard_component: true,
      benefit_metrics_eligible: false,
      input_lane: "native-x11-xtest",
      fixture_ids: ["bp-annotation-density-v1"],
    });
  }
  const phases = [
    {
      phase: "calibration",
      inferenceEligible: false,
      orders: createCalibrationPairOrdersV5(seed ^ 0x4341_4c35),
    },
    {
      phase: "final",
      inferenceEligible: true,
      orders: createBalancedPairOrders({
        pairCount: finalPairCountV5,
        seed,
      }),
    },
  ];
  for (const { phase, inferenceEligible, orders } of phases) {
    for (const journey of plan.journeys) {
      for (const [pairIndex, implementationOrder] of orders.entries()) {
        const pair = pairIndex + 1;
        for (const [
          position,
          implementation,
        ] of implementationOrder.entries()) {
          const bundleId = `${phase}-${journey.scenario}-pair${pair}-${implementation}`;
          for (const [
            componentIndex,
            component,
          ] of journey.component_order.entries()) {
            schedule.push({
              phase,
              inference_eligible: inferenceEligible,
              journey: journey.scenario,
              journey_id: journey.journey_id,
              pair,
              pair_position: position === 0 ? "first" : "second",
              implementation,
              bundle_id: bundleId,
              component,
              component_index: componentIndex,
              component_weight: journey.component_weights[componentIndex],
              hard_component: hardComponents.has(component),
              benefit_metrics_eligible: true,
              input_lane: hardComponents.has(component)
                ? "native-x11-xtest"
                : componentInputLaneV4(component),
              fixture_ids: [...journey.component_fixture_ids[component]],
            });
          }
        }
      }
    }
  }
  return schedule.map((run, scheduleIndex) => ({
    ...run,
    schedule_index: scheduleIndex,
  }));
}

function safeName(value) {
  return value.replace(/[^a-zA-Z0-9._-]+/g, "-");
}

export function buildRunnerInvocationV5(run, options) {
  const identity =
    run.phase === "correctness"
      ? `correctness-${run.implementation}-${run.component}`
      : `${run.bundle_id}-component${run.component_index + 1}-${run.component}`;
  const rawReportPath = resolve(options.output, `${safeName(identity)}.json`);
  const runner =
    run.implementation === "electron"
      ? "electron-runner.mjs"
      : "gpui-runner.mjs";
  const argv = [
    process.execPath,
    resolve(performanceDirectory, runner),
    "--scenario",
    run.component,
  ];
  for (const fixtureId of run.fixture_ids) {
    const path = options.fixtures.get(fixtureId);
    if (!path) throw new Error(`--fixture ${fixtureId}=<file> is required`);
    argv.push("--pdf", path);
  }
  argv.push(
    "--iterations",
    "1",
    "--timeout-ms",
    String(options.timeoutMs),
    "--output",
    rawReportPath,
    "--input-lane",
    run.implementation === "electron"
      ? runnerInputLaneV4(run.implementation, run.input_lane)
      : run.input_lane,
  );
  if (run.implementation === "electron") {
    argv.push("--electron", options.electron);
    if (run.hard_component) argv.push("--v5-scenario", run.journey);
    else argv.push("--v4-scenario", run.journey);
  } else {
    argv.push("--binary", options.gpuiBinary);
    if (!run.hard_component) argv.push("--v4-scenario", run.journey);
    if (run.component === "persistence-workload") {
      argv.push(
        "--evidence-directory",
        resolve(options.output, `${safeName(identity)}-persistence-evidence`),
      );
    }
  }
  return {
    ...run,
    identity,
    raw_report_path: rawReportPath,
    hard_report_path: run.hard_component
      ? resolve(options.output, `${safeName(identity)}-hard-report-v5.json`)
      : null,
    environment: {
      BP_PERF_REQUIRE_NVIDIA: "1",
      BP_PERF_V5_REFERENCE_CROP_DIR: options.referenceCropDirectory,
    },
    argv,
    command: argv.map(shellQuote).join(" "),
  };
}

export function buildLaunchBindingV5({
  invocation,
  launched,
  candidate,
  fixtureArtifacts,
}) {
  const binding = {
    schema_version: 1,
    launch_id: invocation.identity,
    schedule_index: invocation.schedule_index,
    phase: invocation.phase,
    inference_eligible: invocation.inference_eligible,
    journey: invocation.journey,
    pair: invocation.pair,
    pair_position: invocation.pair_position,
    implementation: invocation.implementation,
    component: invocation.component,
    component_index: invocation.component_index,
    input_lane: invocation.input_lane,
    candidate_manifest_sha256: candidate.sha256,
    fixture_sha256_by_id: Object.fromEntries(
      invocation.fixture_ids.map((fixtureId) => [
        fixtureId,
        fixtureArtifacts[fixtureId].sha256,
      ]),
    ),
    raw_report_path: invocation.raw_report_path,
    started_at: launched.started_at,
    ended_at: launched.ended_at,
    started_monotonic_ms: launched.started_monotonic_ms,
    ended_monotonic_ms: launched.ended_monotonic_ms,
  };
  if (
    !Number.isInteger(binding.schedule_index) ||
    !Number.isFinite(binding.started_monotonic_ms) ||
    !Number.isFinite(binding.ended_monotonic_ms) ||
    binding.ended_monotonic_ms <= binding.started_monotonic_ms ||
    !Number.isFinite(Date.parse(binding.started_at)) ||
    !Number.isFinite(Date.parse(binding.ended_at)) ||
    Date.parse(binding.ended_at) < Date.parse(binding.started_at)
  ) {
    throw new Error("launch binding has invalid schedule or time boundaries");
  }
  return binding;
}

function shellQuote(value) {
  return /^[a-zA-Z0-9_./:=+-]+$/.test(value)
    ? value
    : `'${value.replaceAll("'", `'\\''`)}'`;
}

function roundCost(value) {
  return Math.ceil(value * 10_000) / 10_000;
}

export function estimateV5PaidLease(schedule, options) {
  const durations = {
    ...expectedComponentDurationMsV5,
    ...Object.fromEntries(options.expectedDurations),
  };
  const missing = [
    ...new Set(schedule.map(({ component }) => component)),
  ].filter((component) => !Number.isFinite(durations[component]));
  if (missing.length > 0) {
    throw new Error(`missing expected durations for ${missing.join(", ")}`);
  }
  const expectedRunMs = schedule.reduce(
    (sum, run) => sum + durations[run.component],
    0,
  );
  const cooldownTotalMs = Math.max(0, schedule.length - 1) * options.cooldownMs;
  const expectedTaskMs = expectedRunMs + cooldownTotalMs;
  const recommendedTaskLimitMs = Math.ceil(
    expectedTaskMs * (1 + options.taskHeadroomPercent / 100),
  );
  const recommendedLeaseTtlMs = recommendedTaskLimitMs + options.cleanupGraceMs;
  const theoreticalComponentTimeoutCeilingMs =
    schedule.length * options.timeoutMs + cooldownTotalMs;
  const selectedTaskLimitMs = options.taskLimitMs ?? recommendedTaskLimitMs;
  const selectedLeaseTtlMs = options.leaseTtlMs ?? recommendedLeaseTtlMs;
  if (selectedTaskLimitMs < expectedTaskMs) {
    throw new Error(
      "selected task limit is shorter than expected task duration",
    );
  }
  if (selectedLeaseTtlMs < selectedTaskLimitMs + options.cleanupGraceMs) {
    throw new Error("selected lease TTL omits cleanup grace");
  }
  return {
    launch_count: schedule.length,
    measured_launch_count: schedule.filter(
      ({ benefit_metrics_eligible: eligible }) => eligible,
    ).length,
    correctness_only_launch_count: schedule.filter(
      ({ phase }) => phase === "correctness",
    ).length,
    expected_component_duration_ms: durations,
    expected_run_ms: expectedRunMs,
    cooldown_total_ms: cooldownTotalMs,
    expected_task_ms: expectedTaskMs,
    task_headroom_percent: options.taskHeadroomPercent,
    recommended_task_limit_ms: recommendedTaskLimitMs,
    cleanup_grace_ms: options.cleanupGraceMs,
    recommended_absolute_lease_ttl_ms: recommendedLeaseTtlMs,
    theoretical_component_timeout_ceiling_ms:
      theoreticalComponentTimeoutCeilingMs,
    selected_task_limit_ms: selectedTaskLimitMs,
    selected_absolute_lease_ttl_ms: selectedLeaseTtlMs,
    hourly_price_usd: options.hourlyUsd,
    expected_cost_usd: roundCost(
      (expectedTaskMs / 3_600_000) * options.hourlyUsd,
    ),
    maximum_cost_usd: roundCost(
      (selectedLeaseTtlMs / 3_600_000) * options.hourlyUsd,
    ),
    concurrency: 1,
    cleanup_owner: "root orchestrator",
    cleanup_mechanism:
      "destroy the paid GPU resource and verify billable compute is absent",
    independent_expiry_required: true,
  };
}

export function buildV5DryRun(workload, options) {
  const plan = buildV5ComparisonPlan(workload);
  const schedule = buildV5ExecutionSchedule(plan, { seed: options.seed });
  const launches = schedule.map((run) => buildRunnerInvocationV5(run, options));
  const lease = estimateV5PaidLease(schedule, options);
  return {
    schema_version: 1,
    mode: options.mode,
    purpose: "same-host Electron versus GPUI v5 paired comparison",
    evidence_boundary:
      "optimized unpackaged candidates on one hardware-accelerated Linux host; not packaged or cross-platform release qualification",
    plan,
    hashes: {
      verification_status:
        options.mode === "plan"
          ? "declared-plan-inputs; execute mode rehashes every byte before launch"
          : "execute-mode-preflight-required",
      workload_artifact_sha256: expectedWorkloadArtifactSha256V5,
      workload_byte_sha256: expectedWorkloadByteSha256V5,
      decision_contract_sha256: expectedDecisionContractSha256V5,
      candidate_manifest_sha256: {
        electron: options.electronCandidateSha256,
        gpui: options.gpuiCandidateSha256,
      },
      reference_crop_sha256_by_id: referenceHashesFromWorkload(workload),
    },
    matrix: {
      schedule_seed: options.seed,
      journey_count: plan.journeys.length,
      hard_components: [...hardComponentIdsV5],
      calibration_pairs_per_journey: calibrationPairCountV5,
      calibration_inference_eligible: false,
      final_pairs_per_journey: finalPairCountV5,
      final_pair_block_size: finalPairBlockSizeV5,
      property_correctness_runs_per_candidate: 1,
      property_benefit_metrics_eligible: false,
      launch_count: launches.length,
    },
    lease,
    output: {
      directory: options.output,
      run_manifest: resolve(options.output, "run-manifest-v5.json"),
      analyzer_input: resolve(options.output, "analyzer-input-v5.json"),
    },
    environment: {
      BP_PERF_REQUIRE_NVIDIA: "1",
      BP_PERF_V5_REFERENCE_CROP_DIR: options.referenceCropDirectory,
    },
    launches: launches.map(({ argv: _argv, ...launch }) => launch),
    acceptance_checks: [
      "all exact fixtures, candidate manifests, workload bytes, and reference crops verified before launch",
      "every runner emits one successful iteration and exact command receipts",
      "every benefit component emits two live view-state checkpoints and all 180 Electron/GPUI pair assessments match",
      "every benefit component uses trusted native input and the shared implementation-neutral X11 Present completion boundary; semantic/direct-model and application-clock-only evidence is correctness-only",
      "every launch retains required NVIDIA baseline, run, and adjusted samples",
      "Electron property result is a normal one-undo pass or the exact allowed known baseline defect",
      "GPUI property result restores canonical state with one undo",
      "an exact Electron second-NASA empty-page defect is retained as correctness-failing with explicit ineligible benefit metrics; GPUI still passes",
      "all 6 calibration and 24 final pairs retain balanced order and complete bundles",
      "dynamic-fidelity reports retain 1,921 observer samples and three exact registered crops",
    ],
  };
}

function referenceHashesFromWorkload(workload) {
  const command = workload.journeys
    .flatMap(({ commands }) => commands)
    .find(({ id }) => id === "viewer:dynamic-fidelity-scroll");
  return Object.fromEntries(
    command.registered_crops.map((crop) => [
      crop.crop_id,
      crop.reference_raster.reference_crop_sha256,
    ]),
  );
}

async function sha256File(path) {
  const hash = createHash("sha256");
  await new Promise((resolvePromise, rejectPromise) => {
    const stream = createReadStream(path);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("error", rejectPromise);
    stream.on("end", resolvePromise);
  });
  return hash.digest("hex");
}

async function fileArtifact(path, label) {
  const metadata = await stat(path);
  if (!metadata.isFile()) throw new Error(`${label} is not a file: ${path}`);
  return { path, bytes: metadata.size, sha256: await sha256File(path) };
}

function pathWithin(root, path) {
  if (typeof root !== "string" || typeof path !== "string") return false;
  const relation = relative(resolve(root), resolve(path));
  return (
    relation !== "" &&
    relation !== ".." &&
    !relation.startsWith(`..${sep}`) &&
    !isAbsolute(relation)
  );
}

export async function retainDynamicCropArtifactsV5(
  hardReport,
  { runArtifactRoot, referenceArtifacts },
) {
  if (hardReport?.component !== "viewer-dynamic-fidelity") return [];
  const crops = hardReport?.summary?.viewer_dynamic_fidelity?.registered_crops;
  if (!Array.isArray(crops) || crops.length !== 3) {
    throw new Error(
      "dynamic fidelity must retain exactly three PNG artifact sets",
    );
  }
  return Promise.all(
    crops.map(async (crop) => {
      const sourceReference = referenceArtifacts?.[crop.crop_id];
      const declarations = {
        screenshot: {
          path: crop.screenshot_path,
          sha256: crop.screenshot_sha256,
        },
        candidate_crop: {
          path: crop.candidate_crop_path,
          sha256: crop.candidate_crop_sha256,
        },
        registered_reference_crop: {
          path: crop.registered_reference_path,
          sha256: crop.registered_reference_crop_sha256,
        },
      };
      if (!sourceReference) {
        throw new Error(
          `${crop.crop_id}: source reference artifact is missing`,
        );
      }
      const retained = {};
      for (const [name, declaration] of Object.entries(declarations)) {
        if (!pathWithin(runArtifactRoot, declaration.path)) {
          throw new Error(
            `${crop.crop_id}:${name}: path is outside the run artifact tree`,
          );
        }
        const artifact = await fileArtifact(
          declaration.path,
          `${crop.crop_id}:${name}`,
        );
        if (artifact.sha256 !== declaration.sha256) {
          throw new Error(
            `${crop.crop_id}:${name}: retained PNG SHA-256 changed`,
          );
        }
        retained[name] = artifact;
      }
      return {
        crop_id: crop.crop_id,
        launch_id: crop.launch_id,
        capture_id: crop.capture_id,
        capture_started_monotonic_ms:
          crop.stability?.capture_monotonic_interval?.start_ms,
        capture_ended_monotonic_ms:
          crop.stability?.capture_monotonic_interval?.end_ms,
        ...retained,
        source_reference_crop: structuredClone(sourceReference),
      };
    }),
  );
}

export async function verifyV5FixturesAndReferences(workload, options) {
  const fixtures = {};
  for (const fixture of workload.fixtures) {
    const path = options.fixtures.get(fixture.id);
    if (!path) throw new Error(`--fixture ${fixture.id}=<file> is required`);
    const artifact = await fileArtifact(path, fixture.id);
    if (artifact.sha256 !== fixture.sha256) {
      throw new Error(
        `${fixture.id}: SHA-256 mismatch: expected ${fixture.sha256}, got ${artifact.sha256}`,
      );
    }
    fixtures[fixture.id] = artifact;
  }
  const references = {};
  for (const [cropId, expected] of Object.entries(
    referenceHashesFromWorkload(workload),
  )) {
    const path = resolve(options.referenceCropDirectory, `${cropId}.png`);
    const artifact = await fileArtifact(path, cropId);
    if (artifact.sha256 !== expected) {
      throw new Error(
        `${cropId}: reference SHA-256 mismatch: expected ${expected}, got ${artifact.sha256}`,
      );
    }
    references[cropId] = artifact;
  }
  return { fixtures, references };
}

export function validateExactCandidateHashesV5(candidates, expected) {
  for (const implementation of implementations) {
    if (
      candidates?.[implementation]?.sha256 !== expected?.[implementation] ||
      !sha256Pattern.test(expected?.[implementation] ?? "")
    ) {
      throw new Error(
        `${implementation} candidate manifest does not match its declared exact SHA-256`,
      );
    }
  }
  return candidates;
}

export function validateGpuSamplesV5(report) {
  const failures = [];
  if (
    !Array.isArray(report?.iterations) ||
    report.iterations.length !== 1 ||
    report.requested_iterations !== 1
  ) {
    failures.push("exactly one runner iteration is required");
  }
  const iteration = report?.iterations?.[0];
  if (
    iteration?.gpu?.qualification?.required !== true ||
    iteration?.gpu?.qualification?.passed !== true
  ) {
    failures.push("required NVIDIA evidence did not pass");
  }
  for (const phase of ["baseline", "run", "baseline_adjusted"]) {
    const minimumSamples = phase === "baseline" ? 3 : 1;
    if (
      !Number.isInteger(iteration?.gpu?.[phase]?.sample_count) ||
      iteration.gpu[phase].sample_count < minimumSamples ||
      !Array.isArray(iteration.gpu[phase].samples) ||
      iteration.gpu[phase].samples.length !== iteration.gpu[phase].sample_count
    ) {
      failures.push(`${phase} GPU samples are missing or inconsistent`);
    }
  }
  if (
    report?.summary?.gpu_whole_device_baseline_adjusted
      ?.qualification_passed !== true ||
    report?.summary?.gpu_whole_device_baseline_adjusted?.sample_count < 1
  ) {
    failures.push("summarized adjusted GPU samples are missing");
  }
  return { passed: failures.length === 0, failures };
}

export function assessRawBenefitEvidenceV5(
  rawReport,
  run,
  { commonBoundaryValidator = commonBenefitTimingBoundaryPassedV5 } = {},
) {
  if (run?.benefit_metrics_eligible !== true) {
    return {
      eligible: false,
      blockers: ["component is declared correctness-only"],
    };
  }
  const blockers = [];
  if (run?.input_lane !== "native-x11-xtest") {
    blockers.push(
      "semantic/direct-model input lanes are correctness-only and cannot contribute benefit metrics",
    );
  }
  const nativeInput = rawReport?.iterations?.[0]?.native_input;
  if (
    nativeInput?.input_lane !== "native-x11-xtest" ||
    nativeInput?.execution_status !== "passed" ||
    nativeInput?.real_gui_run !== true ||
    nativeInput?.decision_timing_eligible !== true
  ) {
    blockers.push(
      "trusted native GUI execution is not decision-timing eligible",
    );
  }
  const commonBoundary =
    nativeInput?.evidence?.common_benefit_timing_boundary ?? null;
  if (!commonBoundaryValidator(commonBoundary)) {
    blockers.push(
      "a common implementation-neutral X11 timing boundary is absent or invalid",
    );
  }
  return { eligible: blockers.length === 0, blockers };
}

export function electronPropertyOutcomeAcceptedV5(assessment) {
  return (
    assessment?.passed === true &&
    (assessment.correctness_passed === true ||
      assessment.known_baseline_defect_id ===
        "electron-numeric-property-input-blur-duplicate-history-v1")
  );
}

function receiptPayloadHashMatches(receipt) {
  if (!sha256Pattern.test(receipt?.evidence_sha256 ?? "")) return false;
  const { evidence_sha256: _hash, ...payload } = receipt;
  return canonicalSha256(payload) === receipt.evidence_sha256;
}

function percentileV5(values, quantile) {
  const sorted = values
    .filter((value) => Number.isFinite(value))
    .sort((left, right) => left - right);
  if (sorted.length === 0) return null;
  return sorted[Math.ceil(quantile * sorted.length) - 1];
}

function numericSummaryV5(values) {
  const finite = values.filter((value) => Number.isFinite(value));
  if (finite.length === 0) return null;
  return {
    min: Math.min(...finite),
    max: Math.max(...finite),
    median: percentileV5(finite, 0.5),
    p95: percentileV5(finite, 0.95),
  };
}

function authoritativeHardPerformanceSummaryV5(rawReport) {
  const iterations = rawReport?.iterations ?? [];
  const commonBenefitTimingBoundary = structuredClone(
    iterations[0]?.native_input?.evidence?.common_benefit_timing_boundary ??
      null,
  );
  const events = iterations.flatMap((iteration) => iteration.events ?? []);
  const frames =
    rawReport?.implementation === "electron"
      ? iterations.flatMap(
          (iteration) => iteration.renderer?.frame_intervals_ms ?? [],
        )
      : events
          .filter(
            (event) =>
              event.event === "frame" && Number.isFinite(event.interval_ms),
          )
          .map((event) => event.interval_ms);
  const acknowledgements =
    rawReport?.implementation === "electron"
      ? iterations.flatMap((iteration) => {
          const observation =
            iteration.renderer?.native_input_to_application_frame_ack;
          return observation?.physical_scanout_observed === false &&
            observation?.receipt_scope ===
              "trusted-dom-native-event-receipt-to-next-request-animation-frame-callback-not-physical-scanout" &&
            Number.isInteger(observation?.input_event_count) &&
            observation.input_event_count > 0 &&
            observation.acknowledged_event_count ===
              observation.input_event_count
            ? (observation.samples_ms ?? [])
            : [];
        })
      : events
          .filter(
            (event) =>
              (event.event === "native-application-draw-acknowledgement" ||
                event.event === "viewer-native-launch-evidence" ||
                event.event === "multi-document-native-frame-evidence") &&
              event.physical_scanout_observed === false &&
              event.gpui_platform_draw_submitted === true &&
              Number.isFinite(event.input_latency_samples_before) &&
              Number.isFinite(event.input_latency_samples_after) &&
              event.input_latency_samples_after >
                event.input_latency_samples_before &&
              Number.isFinite(event.input_to_application_draw_ack_p95_ns),
          )
          .map((event) => event.input_to_application_draw_ack_p95_ns / 1e6);
  const durations = events
    .map((event) => event.duration_ms)
    .filter((value) => Number.isFinite(value) && value > 0);
  const gpu = summarizeNvidiaIterations(iterations);
  return {
    successful_iterations: iterations.filter(
      (iteration) => iteration.success === true,
    ).length,
    failed_iterations: iterations.filter(
      (iteration) => iteration.success !== true,
    ).length,
    wall_duration_ms: numericSummaryV5(
      iterations.map((iteration) => iteration.wall_duration_ms),
    ),
    product_latency_ms: numericSummaryV5(durations),
    process_tree: {
      cpu_seconds: numericSummaryV5(
        iterations.map((iteration) => iteration.cgroup?.cpu_seconds),
      ),
      cgroup_memory_peak_bytes: numericSummaryV5(
        iterations.map((iteration) => iteration.cgroup?.memory_peak_bytes),
      ),
    },
    application_frame_intervals_ms: numericSummaryV5(frames),
    native_input_to_application_frame_ack_ms:
      numericSummaryV5(acknowledgements),
    native_application_frame_acknowledgement_proxy: {
      receipt_scope:
        rawReport?.implementation === "electron"
          ? "trusted-dom-native-event-receipt-to-next-request-animation-frame-callback-not-physical-scanout"
          : "gpui-input-latency-histogram-to-platform-draw-submission-not-physical-scanout",
      physical_scanout_observed: false,
      sample_count: acknowledgements.length,
    },
    common_benefit_timing_boundary: commonBenefitTimingBoundary,
    ...gpu,
  };
}

export function buildHardComponentReportV5({
  workload,
  rawReport,
  run,
  candidateArtifactSha256,
}) {
  const contract = hardComponentEvidenceContractV5[run.component];
  if (!contract) throw new Error(`${run.component} is not a v5 hard component`);
  const context = rawReport?.comparison_v5;
  if (
    rawReport?.comparison_workload?.manifest_id !== workload.manifest_id ||
    rawReport?.comparison_workload?.artifact_sha256 !==
      expectedWorkloadArtifactSha256V5 ||
    rawReport?.comparison_workload?.byte_sha256 !==
      expectedWorkloadByteSha256V5 ||
    context?.manifest_id !== workload.manifest_id ||
    context?.workload_artifact_sha256 !== expectedWorkloadArtifactSha256V5 ||
    context?.workload_byte_sha256 !== expectedWorkloadByteSha256V5
  ) {
    throw new Error(
      "raw hard-component report did not retain exact v5 workload bytes",
    );
  }
  const evidence = context?.iterations?.[0];
  const fixtureDefinition = representativeScenarioDefinitionsV5[run.journey];
  const fixtureSha256ById = Object.fromEntries(
    contract.fixture_ids.map((fixtureId) => [
      fixtureId,
      fixtureDefinition.fixture_sha256_by_id[fixtureId],
    ]),
  );
  const semanticSummary = structuredClone(evidence?.semantic_summary ?? null);
  if (
    run.component === "viewer-dynamic-fidelity" &&
    Array.isArray(semanticSummary?.registered_crops)
  ) {
    semanticSummary.registered_crops = semanticSummary.registered_crops.map(
      (crop) => ({
        ...crop,
        launch_id: rawReport?.launch_binding_v5?.launch_id,
        capture_id: `${rawReport?.launch_binding_v5?.launch_id}:${crop.crop_id}`,
      }),
    );
  }
  const report = {
    implementation: rawReport.implementation,
    protocol_version: protocolVersionV5,
    decision_contract_version: decisionContractVersionV5,
    scenario_contract_version: scenarioContractVersionV5,
    manifest_id: workload.manifest_id,
    scenario: run.journey,
    component: run.component,
    candidate_artifact_sha256: candidateArtifactSha256,
    workload_artifact_sha256: expectedWorkloadArtifactSha256V5,
    workload_byte_sha256: expectedWorkloadByteSha256V5,
    fixture_ids: [...contract.fixture_ids],
    fixture_sha256_by_id: fixtureSha256ById,
    launch_binding_v5: structuredClone(rawReport.launch_binding_v5),
    command_receipts: structuredClone(evidence?.command_receipts ?? []),
    summary: {
      ...authoritativeHardPerformanceSummaryV5(rawReport),
      [contract.summary_key]: semanticSummary,
    },
  };
  if (
    rawReport?.hard_component_report_v5 &&
    canonicalSha256(rawReport.hard_component_report_v5) !==
      canonicalSha256(report)
  ) {
    throw new Error(
      "embedded hard-component projection does not match authoritative raw observations",
    );
  }
  if (report.implementation !== run.implementation) {
    throw new Error("hard report implementation does not match its launch");
  }
  if (report.candidate_artifact_sha256 !== candidateArtifactSha256) {
    throw new Error("hard report candidate artifact SHA-256 does not match");
  }
  if (report.workload_byte_sha256 !== expectedWorkloadByteSha256V5) {
    throw new Error("hard report workload byte SHA-256 does not match");
  }
  if (!report.command_receipts.every(receiptPayloadHashMatches)) {
    throw new Error(
      "hard report contains a missing or forged command receipt hash",
    );
  }
  return report;
}

export function assessV5Launch({
  workload,
  v4Workload,
  rawReport,
  run,
  candidateArtifactSha256,
  commonBoundaryValidator = commonBenefitTimingBoundaryPassedV5,
}) {
  const gpu = validateGpuSamplesV5(rawReport);
  if (run.hard_component) {
    const hardReport = buildHardComponentReportV5({
      workload,
      rawReport,
      run,
      candidateArtifactSha256,
    });
    const hard = validateHardComponentReportV5(workload, hardReport);
    const rawBenefitEvidence = assessRawBenefitEvidenceV5(rawReport, run, {
      commonBoundaryValidator,
    });
    const benefitEligible =
      hard.benefit_metrics_eligible === true && rawBenefitEvidence.eligible;
    const allowedMissingBenefitEvidence =
      hard.known_baseline_defect_id === exactElectronMultiDocumentDefectV5;
    const viewState = benefitEligible
      ? buildViewStateReceiptV5(rawReport, run)
      : { passed: true, failures: [], receipt: null };
    const benefitFailures =
      run.benefit_metrics_eligible === true &&
      !benefitEligible &&
      !allowedMissingBenefitEvidence
        ? rawBenefitEvidence.blockers.length > 0
          ? rawBenefitEvidence.blockers.map(
              (blocker) =>
                `${run.component}: benefit evidence blocked: ${blocker}`,
            )
          : [
              `${run.component}: benefit evidence is correctness-only or lacks the common X11 timing boundary`,
            ]
        : [];
    return {
      passed:
        gpu.passed &&
        hard.passed &&
        benefitFailures.length === 0 &&
        viewState.passed,
      failures: [
        ...gpu.failures,
        ...hard.failures,
        ...benefitFailures,
        ...viewState.failures,
      ],
      receipts: hardReport.command_receipts,
      measurements: benefitEligible ? hard.measurements : {},
      quality_measurements: hard.quality_measurements,
      correctness_passed: hard.correctness_passed,
      benefit_metrics_eligible: benefitEligible,
      known_baseline_defect_id: hard.known_baseline_defect_id,
      view_state_receipt: viewState.receipt,
      hard_report: hardReport,
    };
  }
  const v4Plan = buildV4ComparisonPlan(v4Workload);
  const journey = v4Plan.journeys.find(
    ({ scenario }) => scenario === run.journey,
  );
  const fixtureId = run.fixture_ids[0];
  const scenarioContract = buildScenarioContractV4(v4Workload, run.journey);
  const receiptAssessment = validateV4ComponentReport({
    report: rawReport,
    implementation: run.implementation,
    journey: run.journey,
    component: run.component,
    fixture: { sha256: journey.fixture_sha256 },
    scenarioContract,
  });
  const measurements = extractComponentMeasurementsV4(rawReport, {
    nativeComponent: run.input_lane === "native-x11-xtest",
  });
  const benefitEvidence = assessRawBenefitEvidenceV5(rawReport, run, {
    commonBoundaryValidator,
  });
  const viewState = buildViewStateReceiptV5(rawReport, run);
  const failures = [
    ...gpu.failures,
    ...receiptAssessment.errors,
    ...measurements.missing.map((missing) => `measurement missing: ${missing}`),
    ...benefitEvidence.blockers.map(
      (blocker) => `${run.component}: benefit evidence blocked: ${blocker}`,
    ),
    ...viewState.failures,
  ];
  if (scenarioContract.fixture_id !== fixtureId) {
    failures.push("legacy component fixture does not match its v5 mapping");
  }
  return {
    passed: failures.length === 0,
    failures,
    receipts: receiptAssessment.receipts,
    measurements: benefitEvidence.eligible ? measurements.values : {},
    quality_measurements: {},
    correctness_passed: failures.length === 0,
    benefit_metrics_eligible: benefitEvidence.eligible,
    known_baseline_defect_id: null,
    view_state_receipt: viewState.receipt,
    hard_report: null,
  };
}

export function buildBundleManifestV5({
  runs,
  results,
  journeyPlan,
  candidate,
  workload,
}) {
  if (
    runs.length !== journeyPlan.component_order.length ||
    results.length !== runs.length ||
    results.some(({ passed }) => passed !== true)
  ) {
    throw new Error("v5 bundle has a missing or failed component");
  }
  if (
    runs.map(({ component }) => component).join("\0") !==
    journeyPlan.component_order.join("\0")
  ) {
    throw new Error("v5 bundle component order is not exact");
  }
  const commandIds = results.flatMap(({ receipts }) =>
    receipts.map(({ command_id: commandId }) => commandId),
  );
  if (
    commandIds.length !== journeyPlan.command_ids.length ||
    new Set(commandIds).size !== commandIds.length ||
    [...commandIds].sort().join("\0") !==
      [...journeyPlan.command_ids].sort().join("\0")
  ) {
    throw new Error("v5 bundle receipts do not cover benefit commands exactly");
  }
  const first = runs[0];
  return {
    schema_version: 1,
    protocol_version: protocolVersionV5,
    decision_contract_version: decisionContractVersionV5,
    scenario_contract_version: scenarioContractVersionV5,
    workload_artifact_sha256: expectedWorkloadArtifactSha256V5,
    workload_byte_sha256: expectedWorkloadByteSha256V5,
    phase: first.phase,
    inference_eligible: first.inference_eligible,
    journey: first.journey,
    journey_id: first.journey_id,
    pair: first.pair,
    pair_position: first.pair_position,
    implementation: first.implementation,
    candidate_artifact: candidate,
    fixture_ids: [...journeyPlan.fixture_ids],
    fixture_sha256_by_id: { ...journeyPlan.fixture_sha256_by_id },
    component_aggregation: {
      order: [...journeyPlan.component_order],
      weights: [...journeyPlan.component_weights],
      benefit_metric_method: "equal-weight geometric mean",
      non_inferiority_method: "conjunctive every component",
      compensating_regressions_allowed: false,
      property_correctness_excluded: true,
    },
    components: results.map((result, index) => ({
      component: runs[index].component,
      component_index: index,
      component_weight: runs[index].component_weight,
      input_lane: runs[index].input_lane,
      hard_component: runs[index].hard_component,
      raw_report_path: result.raw_report_path,
      raw_report_sha256: result.raw_report_sha256,
      hard_report_path: result.hard_report_path,
      hard_report_sha256: result.hard_report_sha256,
      command_receipts: result.receipts.map((receipt) => ({
        command_id: receipt.command_id,
        evidence_sha256: receipt.evidence_sha256,
      })),
      measurements: result.measurements,
      quality_measurements: result.quality_measurements,
      benefit_metrics_eligible: result.benefit_metrics_eligible,
      correctness_passed: result.correctness_passed,
      known_baseline_defect_id: result.known_baseline_defect_id,
      view_state_receipt: result.view_state_receipt,
      launch_binding_v5: result.launch_binding_v5,
      dynamic_crop_artifacts: result.dynamic_crop_artifacts ?? [],
    })),
    command_ids: commandIds,
    passed: true,
    manifest_id: workload.manifest_id,
  };
}

function delay(milliseconds) {
  return new Promise((resolvePromise) =>
    setTimeout(resolvePromise, milliseconds),
  );
}

async function runRunner(invocation, timeoutMs) {
  await rm(invocation.raw_report_path, { force: true });
  if (invocation.hard_report_path) {
    await rm(invocation.hard_report_path, { force: true });
  }
  const startedAt = new Date().toISOString();
  const startedMonotonicMs = Number(process.hrtime.bigint()) / 1e6;
  let stdout = "";
  let stderr = "";
  const [executable, ...args] = invocation.argv;
  const child = spawn(executable, args, {
    cwd: performanceDirectory,
    env: { ...process.env, ...invocation.environment },
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    if (stdout.length < outputLimitBytes)
      stdout += chunk.slice(0, outputLimitBytes - stdout.length);
  });
  child.stderr.on("data", (chunk) => {
    if (stderr.length < outputLimitBytes)
      stderr += chunk.slice(0, outputLimitBytes - stderr.length);
    process.stderr.write(chunk);
  });
  const timeout = setTimeout(() => child.kill("SIGTERM"), timeoutMs);
  const outcome = await new Promise((resolvePromise) => {
    child.once("error", (error) =>
      resolvePromise({
        exit_code: null,
        signal: null,
        spawn_error: error.message,
      }),
    );
    child.once("close", (code, signal) =>
      resolvePromise({ exit_code: code, signal, spawn_error: null }),
    );
  });
  clearTimeout(timeout);
  let report = null;
  try {
    report = JSON.parse(await readFile(invocation.raw_report_path, "utf8"));
  } catch {
    // The launch record retains the absent or invalid report as a failure.
  }
  return {
    started_at: startedAt,
    ended_at: new Date().toISOString(),
    started_monotonic_ms: startedMonotonicMs,
    ended_monotonic_ms: Number(process.hrtime.bigint()) / 1e6,
    ...outcome,
    stdout,
    stderr,
    report,
  };
}

export function buildAnalyzerInputV5(manifest, runManifestEvidence) {
  const finalHard = manifest.launches.filter(
    (launch) =>
      launch.phase === "final" &&
      launch.hard_component === true &&
      launch.passed === true,
  );
  const dynamicPairs = [];
  for (let pair = 1; pair <= finalPairCountV5; pair += 1) {
    const pairReports = finalHard.filter(
      (launch) =>
        launch.component === "viewer-dynamic-fidelity" && launch.pair === pair,
    );
    if (
      pairReports.length !== 2 ||
      implementations.some(
        (implementation) =>
          !pairReports.some(
            (report) => report.implementation === implementation,
          ),
      )
    ) {
      throw new Error(`dynamic fidelity pair ${pair} is incomplete`);
    }
    dynamicPairs.push({
      pair,
      electron:
        pairReports.find(({ implementation }) => implementation === "electron")
          ?.quality_measurements ?? null,
      gpui:
        pairReports.find(({ implementation }) => implementation === "gpui")
          ?.quality_measurements ?? null,
    });
  }
  if (
    manifest.correctness_reports.length !== 2 ||
    finalHard.length !== finalPairCountV5 * 2 * 3
  ) {
    throw new Error(
      "v5 analyzer input is missing correctness or final hard reports",
    );
  }
  return {
    schema_version: 1,
    protocol_version: protocolVersionV5,
    decision_contract_version: decisionContractVersionV5,
    workload_artifact_sha256: expectedWorkloadArtifactSha256V5,
    workload_byte_sha256: expectedWorkloadByteSha256V5,
    schedule_seed: manifest.settings.schedule_seed,
    candidate_manifest_sha256: Object.fromEntries(
      implementations.map((implementation) => [
        implementation,
        manifest.candidates[implementation].sha256,
      ]),
    ),
    reference_crop_sha256_by_id: Object.fromEntries(
      Object.entries(manifest.references).map(([id, artifact]) => [
        id,
        artifact.sha256,
      ]),
    ),
    artifact_tree: structuredClone(manifest.artifact_tree),
    run_manifest: structuredClone(runManifestEvidence),
    candidate_artifacts: structuredClone(manifest.candidates),
    property_correctness: manifest.correctness_reports,
    view_state_pairs: structuredClone(manifest.view_state_pairs),
    bundles: manifest.bundles,
    hard_component_reports: finalHard.map((launch) => ({
      phase: launch.phase,
      inference_eligible: launch.inference_eligible,
      journey: launch.journey,
      pair: launch.pair,
      pair_position: launch.pair_position,
      implementation: launch.implementation,
      component: launch.component,
      component_index: launch.component_index,
      input_lane: launch.input_lane,
      path: launch.hard_report_path,
      sha256: launch.hard_report_sha256,
      measurements: launch.measurements,
      quality_measurements: launch.quality_measurements,
      benefit_metrics_eligible: launch.benefit_metrics_eligible,
      correctness_passed: launch.correctness_passed,
      known_baseline_defect_id: launch.known_baseline_defect_id,
      launch_binding_v5: structuredClone(launch.launch_binding_v5),
      dynamic_crop_artifacts: launch.dynamic_crop_artifacts ?? [],
    })),
    dynamic_fidelity_pairs: dynamicPairs,
  };
}

async function executeV5(workload, dryRun, options) {
  const executionStartedMs = Date.now();
  const v4Workload = await import("./comparison-workload-v4.mjs").then(
    ({ loadMaterializedComparisonWorkloadV4 }) =>
      loadMaterializedComparisonWorkloadV4(),
  );
  const verified = await verifyV5FixturesAndReferences(workload, options);
  const candidates = validateExactCandidateHashesV5(
    await validateOptimizedCandidatesV4({
      electronManifestPath: options.electronCandidateArtifact,
      gpuiManifestPath: options.gpuiCandidateArtifact,
      electronExecutable: options.electron,
      gpuiBinary: options.gpuiBinary,
    }),
    {
      electron: options.electronCandidateSha256,
      gpui: options.gpuiCandidateSha256,
    },
  );
  await mkdir(options.output, { recursive: true });
  const manifestPath = resolve(options.output, "run-manifest-v5.json");
  const manifest = {
    schema_version: 1,
    purpose: dryRun.purpose,
    evidence_boundary: dryRun.evidence_boundary,
    started_at: new Date(executionStartedMs).toISOString(),
    plan: dryRun.plan,
    lease: dryRun.lease,
    workload: {
      manifest_id: workload.manifest_id,
      artifact_sha256: expectedWorkloadArtifactSha256V5,
      byte_sha256: expectedWorkloadByteSha256V5,
      file: await fileArtifact(
        materializedWorkloadPath,
        "materialized v5 workload",
      ),
    },
    candidates,
    fixtures: verified.fixtures,
    references: verified.references,
    artifact_tree: {
      run_root: options.output,
      reference_root: options.referenceCropDirectory,
      references: structuredClone(verified.references),
    },
    settings: dryRun.matrix,
    launches: [],
    bundles: [],
    view_state_pairs: [],
    correctness_reports: [],
    complete: false,
  };
  const persist = () =>
    writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
  const persistChecksum = async () => {
    const checksum = await sha256File(manifestPath);
    await writeFile(
      resolve(options.output, "run-manifest-v5.sha256"),
      `${checksum}  run-manifest-v5.json\n`,
      "utf8",
    );
  };
  await persist();
  const deadline = executionStartedMs + options.taskLimitMs;
  const journeyPlans = new Map(
    dryRun.plan.journeys.map((journey) => [journey.scenario, journey]),
  );
  const launches = dryRun.launches.map((launch, index) => ({
    ...launch,
    argv: buildRunnerInvocationV5(
      dryRun.plan.ready
        ? buildV5ExecutionSchedule(dryRun.plan, { seed: options.seed })[index]
        : null,
      options,
    ).argv,
  }));
  try {
    for (let cursor = 0; cursor < launches.length; ) {
      const first = launches[cursor];
      const grouped = [];
      if (first.phase === "correctness") grouped.push(launches[cursor++]);
      else {
        while (launches[cursor]?.bundle_id === first.bundle_id)
          grouped.push(launches[cursor++]);
      }
      const results = [];
      for (const invocation of grouped) {
        const remainingMs = deadline - Date.now();
        if (remainingMs < options.timeoutMs + 15_000)
          throw new Error("TIMED OUT v5 absolute task limit reached");
        const launched = await runRunner(
          invocation,
          Math.min(options.timeoutMs + 15_000, remainingMs),
        );
        let launchBinding = null;
        if (launched.report) {
          launchBinding = buildLaunchBindingV5({
            invocation,
            launched,
            candidate: candidates[invocation.implementation],
            fixtureArtifacts: verified.fixtures,
          });
          launched.report.launch_binding_v5 = launchBinding;
          await writeFile(
            invocation.raw_report_path,
            `${JSON.stringify(launched.report, null, 2)}\n`,
            "utf8",
          );
        }
        const artifact = launched.report
          ? await fileArtifact(
              invocation.raw_report_path,
              `${invocation.identity} raw report`,
            )
          : null;
        const assessment = launched.report
          ? assessV5Launch({
              workload,
              v4Workload,
              rawReport: launched.report,
              run: invocation,
              candidateArtifactSha256:
                candidates[invocation.implementation].sha256,
            })
          : {
              passed: false,
              failures: ["raw runner report is absent or invalid JSON"],
              receipts: [],
              measurements: {},
              quality_measurements: {},
              benefit_metrics_eligible: false,
              view_state_receipt: null,
            };
        let hardArtifact = null;
        let dynamicCropArtifacts = [];
        if (assessment.hard_report) {
          await writeFile(
            invocation.hard_report_path,
            `${JSON.stringify(assessment.hard_report, null, 2)}\n`,
            "utf8",
          );
          hardArtifact = await fileArtifact(
            invocation.hard_report_path,
            `${invocation.identity} hard report`,
          );
          dynamicCropArtifacts = await retainDynamicCropArtifactsV5(
            assessment.hard_report,
            {
              runArtifactRoot: options.output,
              referenceArtifacts: verified.references,
            },
          );
        }
        const passed = launched.exit_code === 0 && assessment.passed;
        const result = {
          passed,
          failures: [
            ...(launched.exit_code === 0
              ? []
              : [`runner exit code ${launched.exit_code}`]),
            ...assessment.failures,
          ],
          raw_report_path: invocation.raw_report_path,
          raw_report_sha256: artifact?.sha256 ?? null,
          hard_report_path: hardArtifact?.path ?? null,
          hard_report_sha256: hardArtifact?.sha256 ?? null,
          receipts: assessment.receipts,
          measurements: assessment.measurements,
          quality_measurements: assessment.quality_measurements,
          correctness_passed: assessment.correctness_passed,
          benefit_metrics_eligible: assessment.benefit_metrics_eligible,
          known_baseline_defect_id: assessment.known_baseline_defect_id,
          view_state_receipt: assessment.view_state_receipt,
          dynamic_crop_artifacts: dynamicCropArtifacts,
          launch_binding_v5: launchBinding,
        };
        results.push(result);
        manifest.launches.push({
          ...invocation,
          command: invocation.command,
          started_at: launched.started_at,
          ended_at: launched.ended_at,
          exit_code: launched.exit_code,
          signal: launched.signal,
          spawn_error: launched.spawn_error,
          stdout: launched.stdout,
          stderr: launched.stderr,
          ...result,
        });
        if (invocation.phase === "correctness") {
          manifest.correctness_reports.push({
            implementation: invocation.implementation,
            component: invocation.component,
            raw_report_path: result.raw_report_path,
            raw_report_sha256: result.raw_report_sha256,
            hard_report_path: result.hard_report_path,
            hard_report_sha256: result.hard_report_sha256,
            passed: result.passed,
            correctness_passed: result.correctness_passed,
            known_baseline_defect_id: result.known_baseline_defect_id,
            launch_binding_v5: result.launch_binding_v5,
          });
        }
        await persist();
        if (!passed) {
          throw new Error(
            `BLOCKED ${invocation.identity}: ${result.failures.join("; ")}`,
          );
        }
        if (
          invocation.phase === "correctness" &&
          invocation.implementation === "electron" &&
          !electronPropertyOutcomeAcceptedV5({
            passed: result.passed,
            correctness_passed: result.correctness_passed,
            known_baseline_defect_id: result.known_baseline_defect_id,
          })
        ) {
          throw new Error(
            "BLOCKED Electron property result is neither an exact pass nor the allowed known baseline defect",
          );
        }
        if (
          invocation.phase === "correctness" &&
          invocation.implementation === "gpui" &&
          result.correctness_passed !== true
        ) {
          throw new Error("BLOCKED GPUI property correctness did not pass");
        }
        if (cursor < launches.length) await delay(options.cooldownMs);
      }
      if (first.phase !== "correctness") {
        const journeyPlan = journeyPlans.get(first.journey);
        const bundle = buildBundleManifestV5({
          runs: grouped,
          results,
          journeyPlan,
          candidate: candidates[first.implementation],
          workload,
        });
        const counterpart = manifest.bundles.find(
          (candidate) =>
            candidate.phase === bundle.phase &&
            candidate.journey === bundle.journey &&
            candidate.pair === bundle.pair &&
            candidate.implementation !== bundle.implementation,
        );
        let viewStatePair = null;
        if (counterpart) {
          const electronBundle =
            bundle.implementation === "electron" ? bundle : counterpart;
          const gpuiBundle =
            bundle.implementation === "gpui" ? bundle : counterpart;
          viewStatePair = compareBundleViewStatesV5(electronBundle, gpuiBundle);
          if (viewStatePair.passed !== true) {
            throw new Error(
              `BLOCKED ${bundle.phase} ${bundle.journey} pair ${bundle.pair} view state differs: ${viewStatePair.failures.join("; ")}`,
            );
          }
        }
        const bundlePath = resolve(
          options.output,
          `${first.bundle_id}-bundle-manifest-v5.json`,
        );
        await writeFile(
          bundlePath,
          `${JSON.stringify(bundle, null, 2)}\n`,
          "utf8",
        );
        manifest.bundles.push({
          bundle_id: first.bundle_id,
          phase: first.phase,
          inference_eligible: first.inference_eligible,
          journey: first.journey,
          pair: first.pair,
          pair_position: first.pair_position,
          implementation: first.implementation,
          path: bundlePath,
          sha256: await sha256File(bundlePath),
          passed: true,
          components: bundle.components.map((component) => ({
            component: component.component,
            benefit_metrics_eligible: component.benefit_metrics_eligible,
            view_state_receipt: component.view_state_receipt,
          })),
        });
        if (viewStatePair) manifest.view_state_pairs.push(viewStatePair);
        await persist();
      }
    }
    const expectedBundles =
      dryRun.plan.journeys.length *
      (calibrationPairCountV5 + finalPairCountV5) *
      implementations.length;
    const expectedViewStatePairs =
      dryRun.plan.journeys.length * (calibrationPairCountV5 + finalPairCountV5);
    if (
      manifest.bundles.length !== expectedBundles ||
      manifest.view_state_pairs.length !== expectedViewStatePairs ||
      manifest.correctness_reports.length !== 2
    ) {
      throw new Error(
        "BLOCKED retained v5 bundle or correctness count is incomplete",
      );
    }
    manifest.complete = true;
    manifest.outcome = "passed";
    manifest.lease_outcome = "completed-early";
    manifest.ended_at = new Date().toISOString();
    manifest.actual_duration_ms = Date.now() - Date.parse(manifest.started_at);
    manifest.timing_assessment = {
      expected_task_ms: dryRun.lease.expected_task_ms,
      actual_duration_ms: manifest.actual_duration_ms,
      variance_ms: manifest.actual_duration_ms - dryRun.lease.expected_task_ms,
      classification:
        manifest.actual_duration_ms < dryRun.lease.expected_task_ms
          ? "completed-early-acceptance-proven"
          : "completed-at-or-after-estimate-acceptance-proven",
    };
    manifest.estimated_actual_cost_usd = roundCost(
      (manifest.actual_duration_ms / 3_600_000) * options.hourlyUsd,
    );
    await persist();
    await persistChecksum();
    const checksumPath = resolve(options.output, "run-manifest-v5.sha256");
    const analyzerInput = buildAnalyzerInputV5(manifest, {
      manifest: await fileArtifact(manifestPath, "completed v5 run manifest"),
      checksum: await fileArtifact(checksumPath, "v5 run manifest checksum"),
    });
    await writeFile(
      resolve(options.output, "analyzer-input-v5.json"),
      `${JSON.stringify(analyzerInput, null, 2)}\n`,
      "utf8",
    );
  } catch (error) {
    manifest.complete = false;
    manifest.outcome = String(error?.message ?? error).startsWith("TIMED OUT")
      ? "timed-out"
      : "failed-closed";
    manifest.lease_outcome =
      manifest.outcome === "timed-out" ? "timed-out" : "failed-early";
    manifest.failure = error?.message ?? String(error);
    manifest.ended_at = new Date().toISOString();
    manifest.actual_duration_ms = Date.now() - Date.parse(manifest.started_at);
    manifest.estimated_actual_cost_usd = roundCost(
      (manifest.actual_duration_ms / 3_600_000) * options.hourlyUsd,
    );
    await persist();
    await persistChecksum();
    throw error;
  }
}

function usage() {
  return `Usage: node run-paired-v5.mjs --plan|--execute --output <directory> [options]

Plan mode is the default. It performs no candidate launch and prints the exact
1,262-launch matrix, runner commands, report paths, expected duration, global
task limit, cleanup grace, absolute paid lease TTL, and cost estimate.

Required in both modes:
  --output <directory>
  --fixture <id>=<pdf>                  Repeat for all five frozen fixtures
  --electron <file>
  --gpui-binary <file>
  --electron-candidate-artifact <file>
  --gpui-candidate-artifact <file>
  --electron-candidate-sha256 <sha256>
  --gpui-candidate-sha256 <sha256>
  --hourly-usd <price>                  Current paid GPU host hourly price

Execution-only:
  --task-limit-ms <ms>                  Reviewed absolute benchmark task limit
  --lease-ttl-ms <ms>                   Independent lease TTL including grace

Options:
  --plan-output <file>                  Also write the dry-run JSON
  --reference-crop-directory <dir>      Exact transferred PNGs
  --timeout-ms <ms>                     Per runner process (default 120000)
  --cooldown-ms <ms>                    Between launches (default 2000)
  --cleanup-grace-ms <ms>               Reserved lease cleanup time (default 900000)
  --task-headroom-percent <percent>      Recommended task-limit headroom (default 35)
  --expected-component-ms <name>=<ms>   Override a documented duration estimate
  --seed <integer>                      Frozen balanced-order seed
`;
}

async function main() {
  const options = parseV5Arguments(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(usage());
    return;
  }
  const workload = await loadMaterializedComparisonWorkloadV5(
    materializedWorkloadPath,
  );
  const dryRun = buildV5DryRun(workload, options);
  if (options.planOutput) {
    await mkdir(dirname(options.planOutput), { recursive: true });
    await writeFile(options.planOutput, `${JSON.stringify(dryRun, null, 2)}\n`);
  }
  if (options.mode === "plan") {
    process.stdout.write(`${JSON.stringify(dryRun, null, 2)}\n`);
    return;
  }
  await executeV5(workload, dryRun, options);
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
) {
  await main();
}
