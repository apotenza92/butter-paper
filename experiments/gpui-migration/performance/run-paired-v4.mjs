#!/usr/bin/env node

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  decisionContractV4,
  decisionContractVersionV4,
  representativeJourneyIdsV4,
  validateDecisionContractV4,
} from "./decision-contract-v4.mjs";
import { createBalancedPairOrders } from "./decision-statistics.mjs";
import {
  comparisonWorkloadArtifactHashV4,
  loadMaterializedComparisonWorkloadV4,
  validateComparisonWorkloadV4,
} from "./comparison-workload-v4.mjs";
import {
  buildScenarioContractV4,
  representativeScenarioDefinitionsV4,
  representativeTimedScenarioIdsV4,
  scenarioContractVersionV4,
  validateRepresentativeScenarioDefinitionsV4,
} from "./scenario-contract-v4.mjs";
import { validateOptimizedCandidatesV4 } from "./optimized-candidates-v4.mjs";

const performanceDirectory = dirname(fileURLToPath(import.meta.url));
const materializedWorkloadPath = resolve(
  performanceDirectory,
  "comparison-workload-v4.materialized.json",
);
const implementations = Object.freeze(["electron", "gpui"]);
const sha256Pattern = /^[0-9a-f]{64}$/;
const nativeX11Components = new Set([
  "open-pdf",
  "annotation-create",
  "annotation-transform",
  "editor-create",
  "continuous-scroll",
]);

export const calibrationPairCountV4 = 6;
export const minimumFinalPairCountV4 = 24;
export const maximumFinalPairCountV4 = 40;
export const finalPairBlockSizeV4 = 4;

function canonicalize(value) {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonicalize(value[key])]),
    );
  }
  return value;
}

export function canonicalSha256(value) {
  return createHash("sha256")
    .update(JSON.stringify(canonicalize(value)))
    .digest("hex");
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
  return {
    path,
    bytes: metadata.size,
    sha256: await sha256File(path),
  };
}

function parsePositiveInteger(value, option) {
  if (!/^\d+$/.test(value ?? "") || Number(value) < 1) {
    throw new Error(`${option} must be a positive integer`);
  }
  return Number(value);
}

export function parseV4Arguments(argv) {
  const options = {
    calibrationPairs: calibrationPairCountV4,
    finalPairs: minimumFinalPairCountV4,
    timeoutMs: 120_000,
    cooldownMs: 2_000,
    seed: 0x4250_5634,
    fixtures: new Map(),
  };
  const valueOptions = new Set([
    "--fixture",
    "--output",
    "--calibration-pairs",
    "--final-pairs",
    "--timeout-ms",
    "--cooldown-ms",
    "--seed",
    "--electron",
    "--gpui-binary",
    "--electron-candidate-artifact",
    "--gpui-candidate-artifact",
  ]);
  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index];
    if (option === "-h" || option === "--help") return { help: true };
    if (!valueOptions.has(option)) throw new Error(`unknown option: ${option}`);
    const value = argv[++index];
    if (value === undefined || value.startsWith("--")) {
      throw new Error(`${option} requires a value`);
    }
    if (option === "--fixture") {
      const separator = value.indexOf("=");
      if (separator <= 0 || separator === value.length - 1) {
        throw new Error("--fixture must be <fixture-id>=<file>");
      }
      const fixtureId = value.slice(0, separator);
      if (options.fixtures.has(fixtureId))
        throw new Error(`duplicate --fixture ${fixtureId}`);
      options.fixtures.set(fixtureId, resolve(value.slice(separator + 1)));
    }
    if (option === "--output") options.output = resolve(value);
    if (option === "--calibration-pairs") {
      options.calibrationPairs = parsePositiveInteger(value, option);
    }
    if (option === "--final-pairs")
      options.finalPairs = parsePositiveInteger(value, option);
    if (option === "--timeout-ms")
      options.timeoutMs = parsePositiveInteger(value, option);
    if (option === "--cooldown-ms") options.cooldownMs = Number(value);
    if (option === "--seed") options.seed = Number(value);
    if (option === "--electron") options.electron = resolve(value);
    if (option === "--gpui-binary") options.gpuiBinary = resolve(value);
    if (option === "--electron-candidate-artifact") {
      options.electronCandidateArtifact = resolve(value);
    }
    if (option === "--gpui-candidate-artifact") {
      options.gpuiCandidateArtifact = resolve(value);
    }
  }
  if (!options.output) throw new Error("--output is required");
  if (options.calibrationPairs !== calibrationPairCountV4) {
    throw new Error(
      `--calibration-pairs is frozen at ${calibrationPairCountV4}`,
    );
  }
  if (
    options.finalPairs < minimumFinalPairCountV4 ||
    options.finalPairs > maximumFinalPairCountV4 ||
    options.finalPairs % finalPairBlockSizeV4 !== 0
  ) {
    throw new Error(
      `--final-pairs must be a multiple of ${finalPairBlockSizeV4} from ` +
        `${minimumFinalPairCountV4} through ${maximumFinalPairCountV4}`,
    );
  }
  if (!Number.isInteger(options.cooldownMs) || options.cooldownMs < 0) {
    throw new Error("--cooldown-ms must be a nonnegative integer");
  }
  if (!Number.isInteger(options.seed))
    throw new Error("--seed must be an integer");
  if (!options.electron)
    throw new Error("--electron is required to freeze the executable");
  if (!options.gpuiBinary) throw new Error("--gpui-binary is required");
  if (!options.electronCandidateArtifact) {
    throw new Error("--electron-candidate-artifact is required");
  }
  if (!options.gpuiCandidateArtifact) {
    throw new Error("--gpui-candidate-artifact is required");
  }
  return options;
}

function usage() {
  return `Usage: node run-paired-v4.mjs --output <directory> --fixture <id>=<pdf> ... [options]

Runs the complete frozen v4 representative set. Each journey sample is an
ordered bundle of fresh one-process component runs. Six calibration pairs are
retained but excluded from inference. Final pairs use randomized balanced
blocks of four.

Required:
  --output <directory>
  --fixture <id>=<pdf>                 Repeat for every representative fixture
  --electron <file>                    Frozen Electron executable
  --gpui-binary <file>                 Frozen GPUI executable
  --electron-candidate-artifact <file> Prepared Electron optimized-candidate manifest
  --gpui-candidate-artifact <file>     Prepared GPUI optimized-candidate manifest

Options:
  --calibration-pairs <count>          Frozen at 6
  --final-pairs <count>                Multiple of 4 from 24 through 40 (default 24)
  --timeout-ms <ms>                    Per component process (default 120000)
  --cooldown-ms <ms>                   Gap between component processes (default 2000)
  --seed <integer>                     Recorded order seed
`;
}

function seededRandom(seed) {
  let state = seed >>> 0;
  return () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return (state >>> 0) / 0x1_0000_0000;
  };
}

export function createCalibrationPairOrdersV4(seed) {
  if (!Number.isInteger(seed)) throw new Error("seed must be an integer");
  const orders = [
    ["electron", "gpui"],
    ["electron", "gpui"],
    ["electron", "gpui"],
    ["gpui", "electron"],
    ["gpui", "electron"],
    ["gpui", "electron"],
  ];
  const random = seededRandom(seed);
  for (let index = orders.length - 1; index > 0; index -= 1) {
    const swap = Math.floor(random() * (index + 1));
    [orders[index], orders[swap]] = [orders[swap], orders[index]];
  }
  return orders;
}

export function componentInputLaneV4(component) {
  return nativeX11Components.has(component)
    ? "native-x11-xtest"
    : "semantic-diagnostic";
}

export function runnerInputLaneV4(implementation, matchedLane) {
  if (!implementations.includes(implementation)) {
    throw new Error(`unknown implementation ${implementation}`);
  }
  if (!["native-x11-xtest", "semantic-diagnostic"].includes(matchedLane)) {
    throw new Error(`unknown matched input lane ${matchedLane}`);
  }
  return implementation === "electron" && matchedLane === "semantic-diagnostic"
    ? "cdp-input-diagnostic"
    : matchedLane;
}

export function buildV4ComparisonPlan(
  workload,
  definitions = representativeScenarioDefinitionsV4,
) {
  const blockers = [
    ...validateDecisionContractV4(decisionContractV4),
    ...validateComparisonWorkloadV4(workload),
    ...(definitions === representativeScenarioDefinitionsV4
      ? validateRepresentativeScenarioDefinitionsV4()
      : []),
  ];
  const workloadJourneyIds = new Set(
    workload?.journeys?.map(({ id }) => id) ?? [],
  );
  const journeys = [];
  for (const scenario of representativeTimedScenarioIdsV4) {
    const definition = definitions[scenario];
    if (!definition) {
      blockers.push(
        `${scenario}: representative scenario definition is missing`,
      );
      continue;
    }
    if (!representativeJourneyIdsV4.includes(definition.journey_id)) {
      blockers.push(
        `${scenario}: unknown representative journey ${definition.journey_id}`,
      );
    }
    if (!workloadJourneyIds.has(definition.journey_id)) {
      blockers.push(
        `${scenario}: workload journey ${definition.journey_id} is missing`,
      );
    }
    for (const {
      command_id: commandId,
      reason,
    } of definition.blocked_commands ?? []) {
      blockers.push(`${scenario}:${commandId}: ${reason}`);
    }
    const components = [...(definition.current_runner_components ?? [])];
    const weights = [...(definition.component_weights ?? [])];
    if (components.length === 0 || weights.length !== components.length) {
      blockers.push(
        `${scenario}: component order and weights must have equal nonzero length`,
      );
    }
    const equalWeight = components.length === 0 ? null : 1 / components.length;
    if (weights.some((weight) => Math.abs(weight - equalWeight) > 1e-12)) {
      blockers.push(`${scenario}: every component must have equal weight`);
    }
    const mappedCommandIds = components.flatMap(
      (component) => definition.component_command_ids?.[component] ?? [],
    );
    if (
      mappedCommandIds.length !== definition.command_ids?.length ||
      new Set(mappedCommandIds).size !== mappedCommandIds.length ||
      definition.command_ids.some(
        (commandId) => !mappedCommandIds.includes(commandId),
      )
    ) {
      blockers.push(
        `${scenario}: components must map every command exactly once`,
      );
    }
    journeys.push({
      scenario,
      journey_id: definition.journey_id,
      fixture_id: definition.fixture_id,
      fixture_sha256: definition.fixture_sha256,
      component_order: components,
      component_weights: weights,
      component_command_ids: Object.fromEntries(
        components.map((component) => [
          component,
          [...(definition.component_command_ids?.[component] ?? [])],
        ]),
      ),
      command_ids: [...(definition.command_ids ?? [])],
      inference_eligible: definition.inference_eligible === true,
    });
  }
  return {
    protocol_version: "bp-perf-v4",
    decision_contract_version: decisionContractVersionV4,
    scenario_contract_version: scenarioContractVersionV4,
    manifest_id: workload?.manifest_id ?? null,
    ready: blockers.length === 0,
    journeys,
    stress_lanes: [
      {
        id: "usgs-large-sheet-stress-v1",
        inference_eligible: false,
        included: false,
      },
    ],
    supplementary_lanes: [
      {
        id: "private-hibbeler-935-v1",
        status: "blocked-not-transferred",
        included: false,
      },
    ],
    component_aggregation: {
      benefit_metric_method: "equal-weight geometric mean",
      non_inferiority_method: "conjunctive every component",
      compensating_regressions_allowed: false,
    },
    blockers,
  };
}

export function buildV4ExecutionSchedule(plan, { finalPairs, seed }) {
  if (plan?.ready !== true || plan.blockers?.length > 0) {
    throw new Error("BLOCKED v4 paired comparison plan");
  }
  if (
    !Number.isInteger(finalPairs) ||
    finalPairs < minimumFinalPairCountV4 ||
    finalPairs > maximumFinalPairCountV4 ||
    finalPairs % finalPairBlockSizeV4 !== 0
  ) {
    throw new Error("finalPairs must be a valid 24-40 balanced-block count");
  }
  const phases = [
    {
      phase: "calibration",
      inference_eligible: false,
      orders: createCalibrationPairOrdersV4(seed ^ 0x4341_4c34),
    },
    {
      phase: "final",
      inference_eligible: true,
      orders: createBalancedPairOrders({ pairCount: finalPairs, seed }),
    },
  ];
  const schedule = [];
  for (const {
    phase,
    inference_eligible: inferenceEligible,
    orders,
  } of phases) {
    for (const journey of plan.journeys) {
      for (const [pairIndex, implementationOrder] of orders.entries()) {
        const pair = pairIndex + 1;
        for (const [
          positionIndex,
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
              pair_position: positionIndex === 0 ? "first" : "second",
              implementation,
              bundle_id: bundleId,
              component,
              component_index: componentIndex,
              component_weight: journey.component_weights[componentIndex],
              input_lane: componentInputLaneV4(component),
              cache_class: "app-cold",
              one_process_per_component: true,
            });
          }
        }
      }
    }
  }
  return schedule;
}

function sortedStrings(values) {
  return [...values].sort((left, right) => left.localeCompare(right));
}

function receiptHashPayload(receipt, implementation) {
  if (implementation === "electron") {
    const fields = [
      "parent_scenario",
      "component_scenario",
      "command_id",
      "source_command_id",
      "mapping_status",
      "component_execution_passed",
      "proven_milestones",
      "missing_milestones",
    ];
    return Object.fromEntries(fields.map((field) => [field, receipt[field]]));
  }
  const { evidence_sha256: _evidenceSha256, ...payload } = receipt;
  return payload;
}

export function commandReceiptsFromReportV4(report, implementation) {
  const iterations =
    implementation === "electron"
      ? report?.v4_parent_execution?.command_receipts_by_iteration
      : report?.comparison_v4?.command_receipts_by_iteration;
  if (
    !Array.isArray(iterations) ||
    iterations.length !== 1 ||
    iterations[0]?.iteration !== 1
  ) {
    return null;
  }
  return Array.isArray(iterations[0].receipts) ? iterations[0].receipts : null;
}

export function validateV4ComponentReport({
  report,
  implementation,
  journey,
  component,
  fixture,
  scenarioContract,
}) {
  const errors = [];
  if (!implementations.includes(implementation))
    errors.push(`unknown implementation ${implementation}`);
  if (report?.implementation !== implementation)
    errors.push("implementation does not match");
  if (report?.scenario !== component)
    errors.push("component scenario does not match");
  if (report?.requested_iterations !== 1)
    errors.push("runner must contain exactly one process iteration");
  if (report?.cache_class !== "app-cold")
    errors.push("runner cache class must be app-cold");
  if (
    report?.summary?.successful_iterations !== 1 ||
    report?.summary?.failed_iterations !== 0
  ) {
    errors.push("component runner summary did not pass exactly one iteration");
  }
  if (report?.pdf?.sha256 !== fixture.sha256)
    errors.push("component fixture hash does not match");
  const v4 =
    implementation === "electron"
      ? report?.v4_parent_execution
      : report?.comparison_v4;
  if (v4?.manifest_id !== scenarioContract.manifest_id)
    errors.push("v4 manifest id does not match");
  if (v4?.scenario_contract_version !== scenarioContractVersionV4) {
    errors.push("v4 scenario contract version does not match");
  }
  if ((v4?.parent_scenario ?? v4?.scenario) !== journey)
    errors.push("v4 parent journey does not match");
  if (v4?.component_scenario !== component)
    errors.push("v4 component provenance does not match");
  if (v4?.component_receipts_passed !== true)
    errors.push("component receipt summary did not pass");

  const receipts = commandReceiptsFromReportV4(report, implementation);
  const expectedCommandIds =
    scenarioContract.component_command_ids[component] ?? [];
  if (!receipts) {
    errors.push("exactly one retained command receipt set is required");
    return { passed: false, errors, receipts: [] };
  }
  const receivedIds = receipts.map(({ command_id: commandId }) => commandId);
  if (
    receivedIds.length !== expectedCommandIds.length ||
    new Set(receivedIds).size !== receivedIds.length ||
    sortedStrings(receivedIds).join("\0") !==
      sortedStrings(expectedCommandIds).join("\0")
  ) {
    errors.push(
      "receipt command ids do not exactly match the component contract",
    );
  }
  const commandDefinitions = new Map(
    scenarioContract.commands.map((command) => [command.id, command]),
  );
  for (const receipt of receipts) {
    const command = commandDefinitions.get(receipt.command_id);
    const label = receipt.command_id ?? "unknown-command";
    if (receipt.live !== true || receipt.passed !== true) {
      errors.push(`${label}: command receipt is not live and passed`);
    }
    if (!sha256Pattern.test(receipt.evidence_sha256 ?? "")) {
      errors.push(`${label}: command evidence hash is invalid`);
    } else if (
      canonicalSha256(receiptHashPayload(receipt, implementation)) !==
      receipt.evidence_sha256
    ) {
      errors.push(
        `${label}: command evidence hash does not match its exact payload`,
      );
    }
    if (
      receipt.parent_scenario !== journey ||
      receipt.component_scenario !== component
    ) {
      errors.push(
        `${label}: command provenance does not match the component bundle`,
      );
    }
    const expectedMilestones = command?.expected_milestones ?? [];
    if (implementation === "electron") {
      if (receipt.mapping_status !== "exact-semantic-map") {
        errors.push(`${label}: Electron command mapping is not exact`);
      }
      if (receipt.component_execution_passed !== true) {
        errors.push(`${label}: Electron component execution did not pass`);
      }
      if (
        sortedStrings(receipt.proven_milestones ?? []).join("\0") !==
          sortedStrings(expectedMilestones).join("\0") ||
        (receipt.missing_milestones?.length ?? 0) !== 0
      ) {
        errors.push(`${label}: Electron milestone proof is not exact`);
      }
    } else if (
      sortedStrings(receipt.milestone_ids ?? []).join("\0") !==
      sortedStrings(expectedMilestones).join("\0")
    ) {
      errors.push(`${label}: GPUI milestone proof is not exact`);
    }
  }
  return { passed: errors.length === 0, errors, receipts };
}

export function buildBundleManifestV4({
  plannedRuns,
  componentResults,
  journeyPlan,
  candidateArtifact,
  fixture,
  workloadArtifactSha256,
}) {
  if (plannedRuns.length !== journeyPlan.component_order.length) {
    throw new Error("bundle does not contain every planned component");
  }
  const observedOrder = plannedRuns.map(({ component }) => component);
  if (observedOrder.join("\0") !== journeyPlan.component_order.join("\0")) {
    throw new Error(
      "bundle component order does not match the frozen contract",
    );
  }
  if (
    componentResults.length !== plannedRuns.length ||
    componentResults.some(({ passed }) => passed !== true)
  ) {
    throw new Error("bundle contains a failed or missing component result");
  }
  const commandIds = componentResults.flatMap(({ receipts }) =>
    receipts.map(({ command_id: commandId }) => commandId),
  );
  if (
    commandIds.length !== journeyPlan.command_ids.length ||
    new Set(commandIds).size !== commandIds.length ||
    sortedStrings(commandIds).join("\0") !==
      sortedStrings(journeyPlan.command_ids).join("\0")
  ) {
    throw new Error(
      "bundle command receipts do not cover the journey exactly once",
    );
  }
  const first = plannedRuns[0];
  return {
    schema_version: 1,
    protocol_version: "bp-perf-v4",
    decision_contract_version: decisionContractVersionV4,
    scenario_contract_version: scenarioContractVersionV4,
    workload_artifact_sha256: workloadArtifactSha256,
    phase: first.phase,
    inference_eligible: first.inference_eligible,
    journey: first.journey,
    journey_id: first.journey_id,
    pair: first.pair,
    pair_position: first.pair_position,
    implementation: first.implementation,
    candidate_artifact: candidateArtifact,
    fixture,
    cache_class: "app-cold",
    process_model: "one fresh process per component",
    component_aggregation: {
      order: [...journeyPlan.component_order],
      weights: [...journeyPlan.component_weights],
      benefit_metric_method: "equal-weight geometric mean",
      non_inferiority_method: "conjunctive every component",
      compensating_regressions_allowed: false,
    },
    components: componentResults.map((result, index) => ({
      component: plannedRuns[index].component,
      component_index: index,
      component_weight: plannedRuns[index].component_weight,
      input_lane: plannedRuns[index].input_lane,
      raw_report_path: result.raw_report_path,
      raw_report_sha256: result.raw_report_sha256,
      command_receipts: result.receipts.map((receipt) => ({
        command_id: receipt.command_id,
        evidence_sha256: receipt.evidence_sha256,
      })),
    })),
    command_ids: [...commandIds],
    passed: true,
  };
}

export async function verifyV4Fixtures(plan, fixtureAssignments) {
  const fixtures = {};
  for (const journey of plan.journeys) {
    const path = fixtureAssignments.get(journey.fixture_id);
    if (!path)
      throw new Error(`--fixture ${journey.fixture_id}=<file> is required`);
    if (fixtures[journey.fixture_id]) continue;
    const artifact = await fileArtifact(path, journey.fixture_id);
    if (artifact.sha256 !== journey.fixture_sha256) {
      throw new Error(
        `${journey.fixture_id}: SHA-256 mismatch: expected ${journey.fixture_sha256}, got ${artifact.sha256}`,
      );
    }
    fixtures[journey.fixture_id] = artifact;
  }
  return fixtures;
}

function delay(milliseconds) {
  return new Promise((resolvePromise) =>
    setTimeout(resolvePromise, milliseconds),
  );
}

async function runRunner(args, outputPath) {
  const startedAt = new Date().toISOString();
  let stdout = "";
  let stderr = "";
  const child = spawn(process.execPath, args, {
    cwd: performanceDirectory,
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    stdout += chunk;
  });
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
    process.stderr.write(chunk);
  });
  const outcome = await new Promise((resolvePromise) => {
    child.once("error", (error) =>
      resolvePromise({
        exit_code: null,
        signal: null,
        spawn_error: error.message,
      }),
    );
    child.once("close", (code, signal) =>
      resolvePromise({
        exit_code: code,
        signal,
        spawn_error: null,
      }),
    );
  });
  let report = null;
  try {
    report = JSON.parse(await readFile(outputPath, "utf8"));
  } catch {
    // The retained launch record reports the invalid or absent raw report.
  }
  return {
    started_at: startedAt,
    ended_at: new Date().toISOString(),
    ...outcome,
    stdout,
    stderr,
    report,
  };
}

async function main() {
  const options = parseV4Arguments(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(usage());
    return;
  }
  const workload = await loadMaterializedComparisonWorkloadV4(
    materializedWorkloadPath,
  );
  const plan = buildV4ComparisonPlan(workload);
  if (!plan.ready) {
    throw new Error(
      `BLOCKED v4 paired comparison plan: ${plan.blockers.join("; ")}`,
    );
  }
  const fixtures = await verifyV4Fixtures(plan, options.fixtures);
  const candidates = await validateOptimizedCandidatesV4({
    electronManifestPath: options.electronCandidateArtifact,
    gpuiManifestPath: options.gpuiCandidateArtifact,
    electronExecutable: options.electron,
    gpuiBinary: options.gpuiBinary,
  });
  const executables = {
    electron: candidates.electron.executable,
    gpui: candidates.gpui.executable,
    gpui_pdf_worker: candidates.gpui.pdf_worker,
  };
  const workloadArtifactSha256 = comparisonWorkloadArtifactHashV4(workload);
  const materializedWorkloadArtifact = await fileArtifact(
    materializedWorkloadPath,
    "materialized v4 workload",
  );
  const schedule = buildV4ExecutionSchedule(plan, {
    finalPairs: options.finalPairs,
    seed: options.seed,
  });

  await mkdir(options.output, { recursive: true });
  const manifestPath = resolve(options.output, "run-manifest-v4.json");
  const manifest = {
    schema_version: 1,
    purpose:
      "same-host Electron versus GPUI full representative v4 decision comparison",
    evidence_boundary:
      "optimized production-bundle/release-binary comparison; unpackaged, unsigned, and not installed-candidate qualification",
    started_at: new Date().toISOString(),
    plan,
    workload: {
      manifest_id: workload.manifest_id,
      canonical_artifact_sha256: workloadArtifactSha256,
      materialized_file: materializedWorkloadArtifact,
    },
    candidates,
    executables,
    fixtures,
    settings: {
      calibration_pairs_per_journey: calibrationPairCountV4,
      calibration_inference_eligible: false,
      final_pairs_per_journey: options.finalPairs,
      final_pair_block_size: finalPairBlockSizeV4,
      final_pair_orders: createBalancedPairOrders({
        pairCount: options.finalPairs,
        seed: options.seed,
      }),
      calibration_pair_orders: createCalibrationPairOrdersV4(
        options.seed ^ 0x4341_4c34,
      ),
      order_seed: options.seed,
      timeout_ms_per_component: options.timeoutMs,
      cooldown_ms: options.cooldownMs,
      concurrency: 1,
      cache_class: "app-cold",
      process_model: "one fresh process per component",
    },
    excluded_lanes: {
      usgs_large_sheet_stress: "non-inferential and not scheduled",
      private_hibbeler_935: "blocked-not-transferred and not scheduled",
    },
    launches: [],
    bundles: [],
    complete: false,
  };
  const persist = async () => {
    await writeFile(
      manifestPath,
      `${JSON.stringify(manifest, null, 2)}\n`,
      "utf8",
    );
  };
  const persistChecksum = async () => {
    const checksum = await sha256File(manifestPath);
    await writeFile(
      resolve(options.output, "run-manifest-v4.sha256"),
      `${checksum}  run-manifest-v4.json\n`,
      "utf8",
    );
  };
  await persist();

  const journeyPlans = new Map(
    plan.journeys.map((journey) => [journey.scenario, journey]),
  );
  try {
    for (let cursor = 0; cursor < schedule.length; ) {
      const bundleId = schedule[cursor].bundle_id;
      const plannedRuns = [];
      while (schedule[cursor]?.bundle_id === bundleId)
        plannedRuns.push(schedule[cursor++]);
      const journeyPlan = journeyPlans.get(plannedRuns[0].journey);
      const scenarioContract = buildScenarioContractV4(
        workload,
        plannedRuns[0].journey,
      );
      const fixture = fixtures[journeyPlan.fixture_id];
      const componentResults = [];
      for (const planned of plannedRuns) {
        const safeName = planned.bundle_id.replace(/[^a-zA-Z0-9._-]+/g, "-");
        const rawReportPath = resolve(
          options.output,
          `${safeName}-component${planned.component_index + 1}-${planned.component}.json`,
        );
        const runner =
          planned.implementation === "electron"
            ? "electron-runner.mjs"
            : "gpui-runner.mjs";
        const runnerInputLane = runnerInputLaneV4(
          planned.implementation,
          planned.input_lane,
        );
        const args = [
          resolve(performanceDirectory, runner),
          "--scenario",
          planned.component,
          "--v4-scenario",
          planned.journey,
          "--pdf",
          fixture.path,
          "--iterations",
          "1",
          "--timeout-ms",
          String(options.timeoutMs),
          "--output",
          rawReportPath,
          "--input-lane",
          runnerInputLane,
        ];
        if (planned.implementation === "electron") {
          args.push("--electron", options.electron);
        } else {
          args.push("--binary", options.gpuiBinary);
          if (planned.component === "persistence-workload") {
            args.push(
              "--evidence-directory",
              resolve(options.output, `${safeName}-persistence-evidence`),
            );
          }
        }
        const launched = await runRunner(args, rawReportPath);
        const rawReportArtifact = launched.report
          ? await fileArtifact(
              rawReportPath,
              `${planned.bundle_id}:${planned.component} report`,
            )
          : null;
        const assessment = validateV4ComponentReport({
          report: launched.report,
          implementation: planned.implementation,
          journey: planned.journey,
          component: planned.component,
          fixture,
          scenarioContract,
        });
        const componentResult = {
          passed: launched.exit_code === 0 && assessment.passed,
          errors: [
            ...(launched.exit_code === 0
              ? []
              : [`runner exit code ${launched.exit_code}`]),
            ...assessment.errors,
          ],
          raw_report_path: rawReportPath,
          raw_report_sha256: rawReportArtifact?.sha256 ?? null,
          receipts: assessment.receipts,
        };
        componentResults.push(componentResult);
        manifest.launches.push({
          ...planned,
          runner_input_lane: runnerInputLane,
          output: rawReportPath,
          raw_report_sha256: componentResult.raw_report_sha256,
          started_at: launched.started_at,
          ended_at: launched.ended_at,
          exit_code: launched.exit_code,
          signal: launched.signal,
          spawn_error: launched.spawn_error,
          stdout: launched.stdout,
          stderr: launched.stderr,
          passed: componentResult.passed,
          errors: componentResult.errors,
        });
        await persist();
        if (!componentResult.passed) {
          throw new Error(
            `BLOCKED ${planned.bundle_id}:${planned.component}: ${componentResult.errors.join("; ")}`,
          );
        }
        await delay(options.cooldownMs);
      }
      const bundle = buildBundleManifestV4({
        plannedRuns,
        componentResults,
        journeyPlan,
        candidateArtifact: candidates[plannedRuns[0].implementation],
        fixture,
        workloadArtifactSha256,
      });
      const bundlePath = resolve(
        options.output,
        `${bundleId}-bundle-manifest.json`,
      );
      await writeFile(
        bundlePath,
        `${JSON.stringify(bundle, null, 2)}\n`,
        "utf8",
      );
      manifest.bundles.push({
        bundle_id: bundleId,
        phase: bundle.phase,
        inference_eligible: bundle.inference_eligible,
        journey: bundle.journey,
        pair: bundle.pair,
        pair_position: bundle.pair_position,
        implementation: bundle.implementation,
        path: bundlePath,
        sha256: await sha256File(bundlePath),
        passed: true,
      });
      await persist();
    }
    manifest.ended_at = new Date().toISOString();
    manifest.complete = true;
    manifest.outcome = "passed";
    manifest.expected_bundle_count =
      plan.journeys.length *
      (calibrationPairCountV4 + options.finalPairs) *
      implementations.length;
    manifest.observed_bundle_count = manifest.bundles.length;
    if (manifest.observed_bundle_count !== manifest.expected_bundle_count) {
      manifest.complete = false;
      await persist();
      throw new Error(
        "BLOCKED retained bundle count does not match the complete schedule",
      );
    }
    await persist();
    await persistChecksum();
  } catch (error) {
    manifest.ended_at = new Date().toISOString();
    manifest.complete = false;
    manifest.outcome = "failed-closed";
    manifest.failure = error?.message ?? String(error);
    await persist();
    await persistChecksum();
    throw error;
  }
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
) {
  await main();
}
