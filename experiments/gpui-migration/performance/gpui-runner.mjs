#!/usr/bin/env node

import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import {
  mkdir,
  readFile,
  readdir,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import {
  hostname,
  arch,
  cpus,
  freemem,
  platform,
  release,
  totalmem,
  type,
} from "node:os";
import { basename, dirname, extname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFile, spawn } from "node:child_process";
import { promisify } from "node:util";

import {
  allowedScenarios,
  buildDevelopmentScenarioContract,
  normalizedPageSequence,
  protocolVersion,
  scenarioContractVersion,
  zoomSequence,
} from "./scenario-contract.mjs";
import {
  cgroupLaunch,
  createLinuxCgroup,
  readLinuxCgroup,
  removeLinuxCgroup,
} from "./linux-cgroup.mjs";
import {
  startNvidiaBaselineRunSampler,
  summarizeNvidiaIterations,
} from "./nvidia-sampler.mjs";
import { fetchDevelopmentPdfium } from "../gpui-gallery/scripts/fetch-pdfium-development.mjs";
import {
  loadComparisonWorkload,
  runnerComparisonMetadata,
  validateComparisonWorkload,
} from "./comparison-workload.mjs";
import {
  loadComparisonWorkloadV4,
  validateComparisonWorkloadV4,
} from "./comparison-workload-v4.mjs";
import {
  buildScenarioContractV4,
  representativeScenarioDefinitionsV4,
} from "./scenario-contract-v4.mjs";
import {
  comparisonWorkloadArtifactHashV5,
  comparisonWorkloadByteHashV5,
  loadComparisonWorkloadV5,
  validateComparisonWorkloadV5,
} from "./comparison-workload-v5.mjs";
import { buildScenarioContractV5 } from "./scenario-contract-v5.mjs";
import {
  protocolVersionV6,
  representativeScenarioDefinitionsV6,
  scenarioContractVersionV6,
  validateScenarioContractV6,
} from "./scenario-contract-v6.mjs";
import {
  executeNativeX11Scenario,
  nativeX11InputLane,
  nativeX11LaneMetadata,
  semanticDiagnosticInputLane,
} from "./gpui-native-x11.mjs";
import {
  abortX11DamageObserverCollection,
  beginX11DamageObserverCollection,
  finishX11DamageObserverCollection,
} from "./x11-damage-observer.mjs";
import {
  activeGpuAdapterRequired,
  buildGpuiActiveGpuAdapterReceipt,
} from "./active-gpu-adapter.mjs";
import {
  longbridgeCompatProfile,
  validateCompatEventSequence,
  validateCompatPresentedCrop,
} from "./compat-evidence-validator.mjs";

const execFileAsync = promisify(execFile);
const performanceDirectory = dirname(fileURLToPath(import.meta.url));
const migrationDirectory = dirname(performanceDirectory);
const repositoryDirectory = resolve(performanceDirectory, "../../..");
const defaultBinary = resolve(
  migrationDirectory,
  platform() === "darwin"
    ? "gpui-gallery/target/Butter Paper GPUI.app/Contents/MacOS/ButterPaperGPUI"
    : "gpui-gallery/target/debug/butter-paper-gpui-gallery",
);
const sampleIntervalMs = 100;
const defaultTimeoutMs = 120_000;
const stderrLimitBytes = 1_000_000;
const v6ManifestId = "bp-perf-v6-decision-2";
const v6WorkloadByteSha256 =
  "fc7e3cb6f09b74e004a24b01a9f5ccbb444d98feb9ae6489885e27329a442147";
const v6WorkloadPath = resolve(
  performanceDirectory,
  "comparison-workload-v6.json",
);
const localV4EngineeringComponents = Object.freeze({
  "fit-modes": "engineering:fit-modes",
  "cache-pressure-recovery": "engineering:cache-recovery",
});
const v4NativeInteractionComponents = new Set([
  "open-pdf",
  "annotation-create",
  "annotation-transform",
  "editor-create",
  "continuous-scroll",
]);
const v4SourceCommandIds = Object.freeze({
  "nasa:cache-pressure": "viewer:cache-pressure",
  "engineering:zoom-sequence": "viewer:zoom-sequence",
  "engineering:pan": "viewer:pan-usgs",
});
const v4FixedCropOracleHashes = Object.freeze({
  "bp-single-page-v1":
    "cc231d7d5da2ef403509e58565a19fb1855fea3da6aca1436d56dbc38ce218ef",
  "bp-engineering-sheet-v1":
    "f929bfe59c255cffea617b2d4442180aff6368adde243ec921faa42bc17ade32",
});
const v4VisibleRasterReadinessMilestone = "visible-raster-readiness-observed";
const multiDocumentScenario = "multi-document-session";
const dynamicFidelityScenario = "viewer-dynamic-fidelity";
const nativePropertyScenario = "native-property-edit-undo";
const nativeSnapScenario = "native-snap-transform-120hz";
const nativeEditingV5Scenarios = new Set([
  nativePropertyScenario,
  nativeSnapScenario,
]);

export function nativeEvidenceTimeoutMs(scenario, processTimeoutMs) {
  const evidenceBudgetMs = [
    "annotation-transform",
    dynamicFidelityScenario,
    multiDocumentScenario,
  ].includes(scenario)
    ? 45_000
    : 15_000;
  return Math.min(processTimeoutMs, evidenceBudgetMs);
}

export function compatEvidenceValidationForIteration(
  compatProfile,
  events,
  { scenario, pid },
) {
  if (compatProfile === undefined || compatProfile === null) return null;
  if (compatProfile !== longbridgeCompatProfile) {
    throw new Error(`unknown compatibility evidence profile ${compatProfile}`);
  }
  return validateCompatEventSequence(events, { scenario, pid });
}

export function compatResourceCleanupForIteration(
  compatProfile,
  events,
  cgroupCleanup,
) {
  if (compatProfile === undefined || compatProfile === null) return null;
  if (compatProfile !== longbridgeCompatProfile) {
    throw new Error(`unknown compatibility evidence profile ${compatProfile}`);
  }
  const errors = [];
  const receipts = events.filter(
    (event) => event?.event === "resource-cleanup-complete",
  );
  if (
    receipts.length !== 1 ||
    receipts[0]?.worker_exited !== true ||
    receipts[0]?.mapped_surfaces_released !== true
  ) {
    errors.push(
      "expected one verified worker and mapped-surface cleanup receipt",
    );
  }
  if (cgroupCleanup?.removed !== true) {
    errors.push(
      cgroupCleanup?.reason ?? "the benchmark child cgroup was not removed",
    );
  }
  return { passed: errors.length === 0, errors };
}

export function createGpuiV6ExecutionContext({
  workload,
  workloadByteSha256,
  parentScenario,
  componentScenario,
}) {
  const contractFailures = validateScenarioContractV6();
  if (
    workload?.manifest_id !== v6ManifestId ||
    workload?.protocol_version !== protocolVersionV6 ||
    workloadByteSha256 !== v6WorkloadByteSha256 ||
    contractFailures.length > 0
  ) {
    throw new Error(
      `invalid v6 execution contract: ${[
        workload?.manifest_id !== v6ManifestId
          ? "manifest ID is not exact"
          : null,
        workload?.protocol_version !== protocolVersionV6
          ? "protocol version is not exact"
          : null,
        workloadByteSha256 !== v6WorkloadByteSha256
          ? "workload byte SHA-256 changed"
          : null,
        ...contractFailures,
      ]
        .filter(Boolean)
        .join("; ")}`,
    );
  }
  const parentContract = representativeScenarioDefinitionsV6[parentScenario];
  if (!parentContract) {
    throw new Error(`unknown v6 parent scenario ${parentScenario}`);
  }
  if (!parentContract.benefit_components.includes(componentScenario)) {
    throw new Error(
      `${componentScenario} is not a benefit-eligible component of v6 scenario ${parentScenario}`,
    );
  }
  const fixtureIds = parentContract.component_fixture_ids[componentScenario];
  if (!Array.isArray(fixtureIds) || fixtureIds.length === 0) {
    throw new Error(
      `${componentScenario} has no exact v6 fixture mapping in ${parentScenario}`,
    );
  }
  return {
    protocol_version: protocolVersionV6,
    scenario_contract_version: scenarioContractVersionV6,
    manifest_id: workload.manifest_id,
    workload_byte_sha256: workloadByteSha256,
    parent_scenario: parentScenario,
    component_scenario: componentScenario,
    benefit_metrics_eligible: true,
    execution_contract: {
      scenario: componentScenario,
      parent_scenario: parentScenario,
      fixture_ids: [...fixtureIds],
      fixture_sha256_by_id: Object.fromEntries(
        fixtureIds.map((fixtureId) => [
          fixtureId,
          parentContract.fixture_sha256_by_id[fixtureId],
        ]),
      ),
    },
  };
}

export function dynamicArtifactDirectoryForOutput(output, iteration) {
  const outputName = basename(output, extname(output));
  return resolve(
    dirname(output),
    `${outputName}-artifacts`,
    `iteration-${String(iteration).padStart(3, "0")}`,
  );
}

export async function prepareFreshArtifactDirectory(path) {
  await mkdir(path, { recursive: true });
  const entries = await readdir(path);
  if (entries.length > 0) {
    throw new Error(
      `benchmark evidence directory must be empty before launch: ${path}`,
    );
  }
  return path;
}

function usage() {
  return `Usage:
  node gpui-runner.mjs --scenario <name> --pdf <file> [--pdf <file> ...] [options]

Required:
  --scenario <name>       GPUI deterministic performance scenario
  --pdf <file>            Ordered PDF fixture; repeat four times for multi-document-session

Options:
  --iterations <count>    Independent process runs (default: 3)
  --output <file>         JSON report path (default: beside this runner)
  --timeout-ms <ms>       Timeout for each iteration (default: 120000)
  --binary <file>         Override the built GPUI executable
  --input-lane <lane>     semantic-diagnostic (default) or native-x11-xtest
  --v4-scenario <name>    Parent v4 representative journey for this component
  --v6-scenario <name>    Require the v6 common server-side XDamage boundary
  --compat-profile <name> Enable a reviewed candidate-specific evidence policy
  --evidence-directory <directory>
                          Retain persistence PDFs/crops outside disposable cache
  -h, --help              Show this help
`;
}

function fail(message) {
  process.stderr.write(`gpui-runner: ${message}\n\n${usage()}`);
  process.exitCode = 2;
}

function parsePositiveInteger(value, option) {
  if (!/^\d+$/.test(value ?? "") || Number(value) < 1) {
    throw new Error(`${option} must be a positive integer`);
  }
  return Number(value);
}

export function parseArguments(argv) {
  const options = {
    iterations: 3,
    timeoutMs: defaultTimeoutMs,
    binary: defaultBinary,
    inputLane: semanticDiagnosticInputLane,
    pdfs: [],
  };

  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index];
    if (option === "-h" || option === "--help") {
      options.help = true;
      continue;
    }
    const valueOptions = new Set([
      "--scenario",
      "--pdf",
      "--iterations",
      "--output",
      "--timeout-ms",
      "--binary",
      "--input-lane",
      "--v4-scenario",
      "--v6-scenario",
      "--compat-profile",
      "--evidence-directory",
    ]);
    if (!valueOptions.has(option)) {
      throw new Error(`unknown option: ${option}`);
    }
    const value = argv[index + 1];
    if (value === undefined || value.startsWith("--")) {
      throw new Error(`${option} requires a value`);
    }
    index += 1;
    if (option === "--scenario") options.scenario = value;
    if (option === "--pdf") options.pdfs.push(resolve(value));
    if (option === "--iterations") {
      options.iterations = parsePositiveInteger(value, option);
    }
    if (option === "--output") options.output = resolve(value);
    if (option === "--timeout-ms") {
      options.timeoutMs = parsePositiveInteger(value, option);
    }
    if (option === "--binary") options.binary = resolve(value);
    if (option === "--input-lane") options.inputLane = value;
    if (option === "--v4-scenario") options.v4Scenario = value;
    if (option === "--v6-scenario") options.v6Scenario = value;
    if (option === "--compat-profile") options.compatProfile = value;
    if (option === "--evidence-directory")
      options.evidenceDirectory = resolve(value);
  }

  if (options.help) return options;
  if (
    !allowedScenarios.has(options.scenario) &&
    options.scenario !== multiDocumentScenario &&
    options.scenario !== dynamicFidelityScenario &&
    !nativeEditingV5Scenarios.has(options.scenario) &&
    !Object.hasOwn(localV4EngineeringComponents, options.scenario)
  ) {
    throw new Error(
      `--scenario must be one of ${[...allowedScenarios].join(", ")}`,
    );
  }
  if (options.pdfs.length === 0) throw new Error("--pdf is required");
  if (options.scenario === multiDocumentScenario && options.pdfs.length !== 4) {
    throw new Error(
      "multi-document-session requires exactly four ordered --pdf values",
    );
  }
  if (options.scenario !== multiDocumentScenario && options.pdfs.length !== 1) {
    throw new Error(
      "only multi-document-session accepts repeated --pdf values",
    );
  }
  options.pdf = options.pdfs[0];
  if (
    ![semanticDiagnosticInputLane, nativeX11InputLane].includes(
      options.inputLane,
    )
  ) {
    throw new Error(
      "--input-lane must be semantic-diagnostic or native-x11-xtest",
    );
  }
  if (
    options.inputLane === nativeX11InputLane &&
    ![
      "open-pdf",
      "annotation-create",
      "annotation-transform",
      "editor-create",
      "continuous-scroll",
      "multi-document-session",
      dynamicFidelityScenario,
      nativePropertyScenario,
      nativeSnapScenario,
    ].includes(options.scenario)
  ) {
    throw new Error(
      "native-x11-xtest is implemented only for open-pdf, annotation-create, annotation-transform, editor-create, continuous-scroll, multi-document-session, and the v5 native property/snap components",
    );
  }
  if (
    options.evidenceDirectory &&
    options.scenario !== "persistence-workload" &&
    options.compatProfile !== longbridgeCompatProfile
  ) {
    throw new Error(
      "--evidence-directory is implemented only for persistence-workload",
    );
  }
  if (
    options.compatProfile === longbridgeCompatProfile &&
    (options.inputLane !== nativeX11InputLane ||
      options.v4Scenario !== "small-shell-open" ||
      !options.evidenceDirectory)
  ) {
    throw new Error(
      "Longbridge compatibility evidence requires native-x11-xtest, --v4-scenario small-shell-open, and --evidence-directory",
    );
  }
  if (
    Object.hasOwn(localV4EngineeringComponents, options.scenario) &&
    options.v4Scenario !== "engineering-sheet"
  ) {
    throw new Error(
      `${options.scenario} requires --v4-scenario engineering-sheet`,
    );
  }
  if (options.v4Scenario) {
    const definition = representativeScenarioDefinitionsV4[options.v4Scenario];
    if (!definition)
      throw new Error(`unknown --v4-scenario ${options.v4Scenario}`);
    const locallyImplemented =
      options.v4Scenario === "engineering-sheet" &&
      Object.hasOwn(localV4EngineeringComponents, options.scenario);
    if (
      !definition.current_runner_components.includes(options.scenario) &&
      !locallyImplemented
    ) {
      throw new Error(
        `${options.scenario} is not a component of v4 scenario ${options.v4Scenario}`,
      );
    }
  }
  if (
    options.v6Scenario &&
    options.v4Scenario &&
    options.v6Scenario !== options.v4Scenario
  ) {
    throw new Error(
      "--v6-scenario must match the inherited --v4-scenario parent",
    );
  }
  if (options.v6Scenario && options.inputLane !== nativeX11InputLane) {
    throw new Error("--v6-scenario requires --input-lane native-x11-xtest");
  }
  if (
    options.compatProfile !== undefined &&
    options.compatProfile !== longbridgeCompatProfile
  ) {
    throw new Error(`unknown --compat-profile ${options.compatProfile}`);
  }
  if (!options.output) {
    const safeScenario = options.scenario.replace(/[^a-zA-Z0-9._-]+/g, "-");
    options.output = resolve(performanceDirectory, `gpui-${safeScenario}.json`);
  }
  return options;
}

export function buildDynamicFidelityV5Context(workload) {
  const parent = buildScenarioContractV5(workload, "nasa-long-document");
  const command = parent.commands.find(
    ({ id }) => id === "viewer:dynamic-fidelity-scroll",
  );
  if (!command) throw new Error("v5 dynamic fidelity command is missing");
  return {
    ...parent,
    scenario: dynamicFidelityScenario,
    input_lane: nativeX11InputLane,
    fixture_ids: [command.fixture_id],
    fixture_sha256_by_id: {
      [command.fixture_id]: parent.fixture_sha256_by_id[command.fixture_id],
    },
    command_ids: [command.id],
    current_runner_components: [dynamicFidelityScenario],
    component_command_ids: { [dynamicFidelityScenario]: [command.id] },
    component_fixture_ids: {
      [dynamicFidelityScenario]: [command.fixture_id],
    },
    component_benefit_metrics_eligible: { [dynamicFidelityScenario]: true },
    commands: [command],
  };
}

export function buildNativeEditingV5Context(workload, component) {
  if (!nativeEditingV5Scenarios.has(component)) {
    throw new Error(`unknown native v5 editing component ${component}`);
  }
  const parent = buildScenarioContractV5(workload, "dense-mixed-editing");
  const commandIds = parent.component_command_ids[component];
  const commands = commandIds.map((commandId) =>
    parent.commands.find(({ id }) => id === commandId),
  );
  if (commands.some((command) => !command)) {
    throw new Error(`${component} command is missing from dense-mixed-editing`);
  }
  return {
    ...parent,
    scenario: component,
    input_lane: nativeX11InputLane,
    fixture_ids: [...parent.component_fixture_ids[component]],
    fixture_sha256_by_id: Object.fromEntries(
      parent.component_fixture_ids[component].map((fixtureId) => [
        fixtureId,
        parent.fixture_sha256_by_id[fixtureId],
      ]),
    ),
    command_ids: [...commandIds],
    current_runner_components: [component],
    component_command_ids: { [component]: [...commandIds] },
    component_fixture_ids: {
      [component]: [...parent.component_fixture_ids[component]],
    },
    component_benefit_metrics_eligible: {
      [component]: parent.component_benefit_metrics_eligible[component],
    },
    commands,
  };
}

export function buildV4RunnerContext(
  workload,
  parentScenario,
  componentScenario,
) {
  const contract = buildScenarioContractV4(workload, parentScenario);
  const locallyImplemented =
    parentScenario === "engineering-sheet" &&
    Object.hasOwn(localV4EngineeringComponents, componentScenario);
  if (
    !contract.current_runner_components.includes(componentScenario) &&
    !locallyImplemented
  ) {
    throw new Error(
      `${componentScenario} is not a component of v4 scenario ${parentScenario}`,
    );
  }
  return {
    ...contract,
    component_scenario: componentScenario,
    execution_eligible: false,
    execution_blocker: "exact live v4 command receipts have not been assessed",
  };
}

export function fixtureIdsForLaunch(v5RunnerContext, v4RunnerContext) {
  if (Array.isArray(v5RunnerContext?.fixture_ids)) {
    return [...v5RunnerContext.fixture_ids];
  }
  if (Array.isArray(v4RunnerContext?.fixture_ids)) {
    return [...v4RunnerContext.fixture_ids];
  }
  return typeof v4RunnerContext?.fixture_id === "string"
    ? [v4RunnerContext.fixture_id]
    : null;
}

export function applyV4ComponentExecutionContract(
  developmentContract,
  v4Context,
) {
  if (
    !developmentContract ||
    v4Context?.component_scenario !== "continuous-scroll"
  ) {
    return developmentContract;
  }
  const commandIds =
    v4Context.component_command_ids?.[v4Context.component_scenario] ?? [];
  const commands = developmentContract.commands.map((command) => {
    const target = v4Context.commands.find(
      ({ id }) => id === command.id && commandIds.includes(id),
    );
    return target
      ? { ...command, expected_milestones: [...target.expected_milestones] }
      : command;
  });
  return {
    ...developmentContract,
    command_ids: commands.map(({ id }) => id),
    commands,
  };
}

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

function exactMilestones(actual, expected) {
  return (
    Array.isArray(actual) &&
    actual.length === expected.length &&
    [...actual]
      .sort()
      .every((milestone, index) => milestone === [...expected].sort()[index])
  );
}

export function validateOrderedScenarioFixtures(pdfs, contract) {
  if (!Array.isArray(contract?.fixture_ids)) return null;
  if (pdfs.length !== contract.fixture_ids.length) {
    return `${contract.scenario} requires ${contract.fixture_ids.length} ordered PDF fixtures; received ${pdfs.length}`;
  }
  for (let index = 0; index < contract.fixture_ids.length; index += 1) {
    const fixtureId = contract.fixture_ids[index];
    const expected = contract.fixture_sha256_by_id?.[fixtureId];
    if (pdfs[index]?.sha256 !== expected) {
      return `${fixtureId} at ordered --pdf index ${index} requires PDF SHA-256 ${expected}; received ${pdfs[index]?.sha256}`;
    }
  }
  return null;
}

function sha256Canonical(value) {
  return createHash("sha256")
    .update(JSON.stringify(canonicalize(value)))
    .digest("hex");
}

export function buildMultiDocumentV5Evidence(
  iteration,
  events,
  contract,
  { inputLane, nativeReplay, applicationSuccess },
) {
  const commandReceipts = contract.commands.map((command) => {
    const milestoneEvents = events.filter(
      (event) =>
        event.event === "comparison-milestone" &&
        event.command_id === command.id,
    );
    const provenMilestones = milestoneEvents.map((event) => event.milestone);
    const commandEvidence = events.filter(
      (event) =>
        event.event === "multi-document-command-evidence" &&
        event.command_id === command.id,
    );
    const nativeFrames = events.filter(
      (event) =>
        event.event === "multi-document-native-frame-evidence" &&
        event.command_id === command.id &&
        event.gpui_platform_draw_submitted === true &&
        event.physical_scanout_observed === false &&
        event.input_latency_samples_after > event.input_latency_samples_before,
    );
    const nativeRequired = command.input_lane === "native-replay";
    const expectedNativeFrames =
      command.id === "session:switch-four-fixtures"
        ? 4
        : command.id === "session:edit-dense-rectangle"
          ? 1
          : 0;
    const passed =
      applicationSuccess === true &&
      commandEvidence.length === 1 &&
      exactMilestones(provenMilestones, command.expected_milestones) &&
      (!nativeRequired ||
        (inputLane === nativeX11InputLane &&
          nativeReplay?.success === true &&
          nativeFrames.length === expectedNativeFrames));
    const evidence = {
      command_id: command.id,
      iteration,
      live: true,
      passed,
      proven_milestones: provenMilestones,
      command_evidence: commandEvidence,
      native_frame_evidence: nativeFrames,
      native_replay_verified: nativeReplay?.target_verified === true,
    };
    return { ...evidence, evidence_sha256: sha256Canonical(evidence) };
  });

  const opens = events.filter(
    (event) => event.event === "multi-document-open-raster-observed",
  );
  const switches = events.filter(
    (event) =>
      event.event === "multi-document-native-frame-evidence" &&
      event.command_id === "session:switch-four-fixtures",
  );
  const close = events.find(
    (event) =>
      event.event === "multi-document-command-evidence" &&
      event.command_id === "session:close-three-and-recover",
  );
  const edit = events.find(
    (event) =>
      event.event === "multi-document-command-evidence" &&
      event.command_id === "session:edit-dense-rectangle",
  );
  const observedProcessIds = events
    .filter((event) => Number.isInteger(event.process_id))
    .map((event) => event.process_id);
  const summary = {
    opened_fixture_ids: opens.map((event) => event.fixture_id),
    switch_sequence: switches.map((event) => event.fixture_id),
    close_sequence: (close?.closed_documents ?? []).map(
      (observation) => observation.fixture_id,
    ),
    process_restart_count: 0,
    observed_process_ids: observedProcessIds,
    stable_process_id:
      observedProcessIds.length > 0 ? observedProcessIds[0] : null,
    trusted_native_input:
      inputLane === nativeX11InputLane && nativeReplay?.success === true,
    per_document_state_isolated: edit?.other_document_states_unchanged === true,
    current_raster_receipt_count: opens.length + switches.length,
    dense_rectangle_property_commit_count:
      edit?.stroke_width_points === 4 ? 1 : 0,
    dense_rectangle_stroke_width_points: edit?.stroke_width_points ?? null,
    closed_document_resources_released:
      (close?.closed_documents ?? []).length === 3 &&
      close.closed_documents.every(
        (observation) =>
          observation.document_removed === true &&
          observation.render_requests_removed === true &&
          observation.annotation_state_removed === true &&
          observation.released_decoded_page_bytes > 0,
      ),
    remaining_document_count: close?.remaining_document_count ?? null,
    remaining_fixture_id: close?.remaining_fixture_id ?? null,
    dense_document_active:
      close?.remaining_fixture_id === "bp-annotation-density-v1",
    aggregate_resource_observations_complete:
      opens.length === 4 &&
      opens.every((event) => event.decoded_page_bytes > 0),
    interactive_document_shell: close?.interactive_document_shell === true,
  };
  return {
    component: multiDocumentScenario,
    benefit_metrics_eligible: true,
    passed: commandReceipts.every((receipt) => receipt.passed),
    command_receipts: commandReceipts,
    semantic_summary: summary,
  };
}

export function buildDynamicFidelityV5Evidence(
  iteration,
  events,
  contract,
  { inputLane, nativeReplay, applicationSuccess },
) {
  const command = contract.commands[0];
  const milestones = events
    .filter(
      (event) =>
        event.event === "comparison-milestone" &&
        event.command_id === command.id,
    )
    .map((event) => event.milestone);
  const runnerEvidence = events.filter(
    (event) =>
      event.event === "dynamic-fidelity-runner-evidence" &&
      event.command_id === command.id,
  );
  const applicationEvidence = events.filter(
    (event) =>
      event.event === "dynamic-fidelity-application-evidence" &&
      event.command_id === command.id,
  );
  const passed =
    applicationSuccess === true &&
    inputLane === nativeX11InputLane &&
    nativeReplay?.success === true &&
    runnerEvidence.length === 1 &&
    applicationEvidence.length === 1 &&
    exactMilestones(milestones, command.expected_milestones);
  const evidence = {
    command_id: command.id,
    iteration,
    live: true,
    passed,
    proven_milestones: milestones,
    runner_evidence: runnerEvidence,
    application_evidence: applicationEvidence,
    native_replay_verified: nativeReplay?.target_verified === true,
  };
  return {
    component: dynamicFidelityScenario,
    benefit_metrics_eligible: true,
    passed,
    command_receipts: [
      { ...evidence, evidence_sha256: sha256Canonical(evidence) },
    ],
    semantic_summary: {
      trajectory_sample_count:
        runnerEvidence[0]?.trajectory_samples?.length ?? 0,
      fidelity_sample_count: runnerEvidence[0]?.fidelity_samples?.length ?? 0,
      registered_crop_count: runnerEvidence[0]?.registered_crops?.length ?? 0,
      registered_crop_ssim_luma:
        runnerEvidence[0]?.registered_crops?.map((crop) => crop.ssim_luma) ??
        [],
      all_registered_crops_passed:
        runnerEvidence[0]?.registered_crops?.length === 3 &&
        runnerEvidence[0].registered_crops.every(
          (crop) => crop.passed === true,
        ),
      max_visible_page_count:
        applicationEvidence[0]?.max_visible_page_count ?? null,
      finish_page: applicationEvidence[0]?.current_page ?? null,
      gpui_platform_draw_submitted:
        applicationEvidence[0]?.gpui_platform_draw_submitted === true,
      physical_scanout_observed: false,
    },
  };
}

export function buildNativeEditingV5Evidence(
  iteration,
  events,
  contract,
  { inputLane, nativeReplay, applicationSuccess },
) {
  const command = contract.commands[0];
  const milestones = events
    .filter(
      (event) =>
        event.event === "comparison-milestone" &&
        event.command_id === command.id,
    )
    .map(({ milestone }) => milestone);
  const property = contract.scenario === nativePropertyScenario;
  const applicationEventName = property
    ? "native-v5-property-application-evidence"
    : "native-v5-snap-application-evidence";
  const presentationEventName = property
    ? "native-v5-property-presentation-evidence"
    : "native-v5-snap-presentation-evidence";
  const application = events.filter(
    (event) =>
      event.event === applicationEventName && event.command_id === command.id,
  );
  const presentation = events.filter(
    (event) =>
      event.event === presentationEventName && event.command_id === command.id,
  );
  const nativeCommand = nativeReplay?.commands?.find(
    ({ command_id: commandId }) => commandId === command.id,
  );
  const snapReceiptJoined =
    property ||
    (nativeCommand?.timestamped_injected_samples?.length ===
      command.expected_sample_count &&
      application[0]?.expected_injected_sample_count ===
        command.expected_sample_count &&
      Number.isInteger(application[0]?.observed_application_update_count) &&
      application[0].observed_application_update_count >= 3 &&
      application[0]?.observed_application_update_timestamps_ms?.length ===
        application[0].observed_application_update_count &&
      application[0]?.first_position_observed === true &&
      application[0]?.final_position_observed === true);
  const passed =
    applicationSuccess === true &&
    inputLane === nativeX11InputLane &&
    nativeReplay?.success === true &&
    nativeReplay?.target_verified === true &&
    application.length === 1 &&
    presentation.length === 1 &&
    nativeCommand?.timing?.within_tolerance === true &&
    snapReceiptJoined &&
    exactMilestones(milestones, command.expected_milestones);
  const evidence = {
    command_id: command.id,
    iteration,
    live: true,
    passed,
    proven_milestones: milestones,
    application_evidence: application,
    presentation_evidence: presentation,
    native_replay: nativeCommand ?? null,
    native_replay_verified: nativeReplay?.target_verified === true,
  };
  const semanticSummary = {
    ...application[0],
    trusted_native_input:
      inputLane === nativeX11InputLane && nativeReplay?.success === true,
    native_presentation_acknowledged:
      property && presentation[0]?.gpui_platform_draw_submitted === true,
    snap_guide_presented_count: property
      ? undefined
      : presentation[0]?.snap_guide_presented === true
        ? 1
        : 0,
    expected_sample_count: property ? undefined : command.expected_sample_count,
    observed_sample_count: property
      ? undefined
      : nativeCommand?.timestamped_injected_samples?.length,
  };
  delete semanticSummary.schema_version;
  delete semanticSummary.event;
  delete semanticSummary.t_ms;
  return {
    component: contract.scenario,
    benefit_metrics_eligible:
      contract.component_benefit_metrics_eligible[contract.scenario],
    passed,
    command_receipts: [
      { ...evidence, evidence_sha256: sha256Canonical(evidence) },
    ],
    semantic_summary: semanticSummary,
  };
}

function exactLocalV4Evidence(componentScenario, event) {
  if (componentScenario === "fit-modes") {
    return (
      Array.isArray(event.observations) &&
      event.observations.length === 2 &&
      ["fit-page", "fit-width"].every((mode) =>
        event.observations.some(
          (observation) =>
            observation?.mode === mode &&
            observation.shell_width === 1_200 &&
            observation.shell_height === 800 &&
            observation.current_generation_presented === true,
        ),
      )
    );
  }
  if (componentScenario === "cache-pressure-recovery") {
    const observation = event.observation;
    return (
      observation?.cycles_completed === 5 &&
      observation.before?.document_count === 1 &&
      observation.before?.renderer_resource_submission_bytes > 0 &&
      observation.after?.document_count === 0 &&
      observation.after?.tile_cache_bytes === 0 &&
      observation.after?.decoded_page_bytes === 0 &&
      observation.after?.renderer_resource_submission_bytes === 0 &&
      event.released_render_bytes > 0
    );
  }
  return false;
}

function indexedEvents(events, predicate) {
  return events
    .map((event, index) => ({ event, index }))
    .filter(({ event }) => predicate(event));
}

function evidenceMilestones(events, commandId) {
  const milestones = new Set();
  const indexes = new Set();
  for (const { event, index } of indexedEvents(
    events,
    (candidate) =>
      [
        "comparison-command-evidence",
        "comparison-live-presentation-evidence",
      ].includes(candidate.event) &&
      (candidate.command_id === commandId ||
        candidate.evidence?.command_id === commandId),
  )) {
    for (const milestone of event.evidence?.proven_manifest_milestones ?? []) {
      milestones.add(milestone);
    }
    indexes.add(index);
  }
  for (const { event, index } of indexedEvents(events, (candidate) =>
    [
      "editor-create-presentation-frame-observed",
      "editor-final-presentation-evidence",
    ].includes(candidate.event),
  )) {
    const evidence = event.report?.commands?.find(
      ({ command_id: candidate }) => candidate === commandId,
    );
    if (!evidence) continue;
    for (const milestone of evidence.proven_manifest_milestones ?? []) {
      milestones.add(milestone);
    }
    indexes.add(index);
  }
  const dense = indexedEvents(
    events,
    (candidate) =>
      candidate.event === "dense-rectangle-presentation-evidence" &&
      candidate.report?.evidence?.command_id === commandId,
  );
  for (const { event, index } of dense) {
    for (const milestone of event.report.evidence.proven_manifest_milestones ??
      []) {
      milestones.add(milestone);
    }
    indexes.add(index);
  }
  return { milestones, indexes };
}

function exactNativeReplay(evidence) {
  return (
    evidence.inputLane === nativeX11InputLane &&
    evidence.nativeReplay?.success === true &&
    evidence.nativeReplay?.target_verified === true
  );
}

function exactOpenProof(command, events, context, evidence) {
  const proven = new Set();
  const indexes = new Set();
  const claims = {};
  const launch = indexedEvents(
    events,
    (event) =>
      event.event === "viewer-native-launch-evidence" &&
      event.native_input_observed === true &&
      event.gpui_platform_draw_submitted === true &&
      event.interactive_shell === true,
  );
  const open = indexedEvents(
    events,
    (event) =>
      event.event === "viewer-native-open-evidence" &&
      event.document_opened === true &&
      event.preview_current_generation === true &&
      event.settled_current_generation_ms >= 250 &&
      event.completed_open_generation === event.requested_open_generation &&
      event.preview_generation !== null,
  );
  const pdfOpen = indexedEvents(
    events,
    (event) =>
      event.event === "pdf-open-completed" &&
      Number.isInteger(event.pages) &&
      event.pages > 0,
  );
  const viewport = indexedEvents(
    events,
    (event) =>
      event.event === "viewport-raster-completed" &&
      event.surface_kind === "in-memory-bgra" &&
      event.pixel_width > 0 &&
      event.rendered_device_pixel_ratio > 0,
  );
  if (command.id === "small:launch-cold") {
    const process = indexedEvents(
      events,
      (event) => event.event === "process-main-enter",
    );
    if (process.length === 1) proven.add("process-started");
    if (launch.length === 1 && exactNativeReplay(evidence)) {
      proven.add("native-window-presented");
      proven.add("interactive-shell");
    }
    for (const entry of [...process, ...launch]) indexes.add(entry.index);
    claims.native_input_and_platform_draw_exact =
      exactNativeReplay(evidence) && launch.length === 1;
    return { proven, indexes, claims };
  }
  if (open.length === 1 && pdfOpen.length === 1 && viewport.length === 1) {
    proven.add("document-opened");
    proven.add("preview-current-generation");
    proven.add("settled-current-generation-250ms");
    for (const entry of [...open, ...pdfOpen, ...viewport])
      indexes.add(entry.index);
  }
  if (command.expected_milestones.includes("fixed-crops-matched")) {
    const cropOracleSha256 = v4FixedCropOracleHashes[context.fixture_id];
    if (evidence.compatProfile === longbridgeCompatProfile) {
      const crop = validateCompatPresentedCrop(events, {
        fixtureId: context.fixture_id,
        commandId: command.id,
        driverReceipt:
          evidence.nativeReplay?.driver_evidence?.compat_presented_crop,
      });
      if (
        crop.passed &&
        cropOracleSha256 &&
        context.fixture_sha256 &&
        viewport.length === 1
      ) {
        proven.add("fixed-crops-matched");
        claims.fixed_crop_registration = {
          fixture_id: context.fixture_id,
          fixture_sha256: context.fixture_sha256,
          crop_oracle_sha256: cropOracleSha256,
          live_viewport_raster_event_index: viewport[0].index,
          evidence_source: "independent-native-driver",
          ...crop.receipt,
        };
      } else {
        claims.fixed_crop_registration = {
          fixture_id: context.fixture_id,
          fixture_sha256: context.fixture_sha256 ?? null,
          crop_oracle_sha256: cropOracleSha256 ?? null,
          passed: false,
          errors: crop.errors,
        };
      }
    } else if (
      cropOracleSha256 &&
      context.fixture_sha256 &&
      viewport.length === 1
    ) {
      proven.add("fixed-crops-matched");
      claims.fixed_crop_registration = {
        fixture_id: context.fixture_id,
        fixture_sha256: context.fixture_sha256,
        crop_oracle_sha256: cropOracleSha256,
        live_viewport_raster_event_index: viewport[0].index,
      };
    }
  }
  if (
    command.expected_milestones.includes("virtual-page-window-bounded") &&
    pdfOpen[0]?.event.pages > 1 &&
    viewport.length === 1
  ) {
    proven.add("virtual-page-window-bounded");
    claims.visible_page_window = {
      visible_pages: 1,
      document_pages: pdfOpen[0].event.pages,
    };
  }
  claims.native_open_replay_exact = exactNativeReplay(evidence);
  return { proven, indexes, claims };
}

function exactImageResourceProof(commandId, events) {
  const reports = indexedEvents(events, (event) =>
    [
      "editor-create-presentation-frame-observed",
      "editor-final-presentation-evidence",
    ].includes(event.event),
  );
  for (const { event, index } of reports) {
    const report = event.report;
    const command = report?.commands?.find(
      ({ command_id: candidate }) => candidate === commandId,
    );
    const decodedBytes =
      command?.facts?.decoded_render_image?.decoded_bgra_bytes ??
      command?.facts?.decoded_bgra_bytes;
    const submittedBytes =
      command?.facts?.gpui_atlas_upload_bytes ??
      report?.gpui_atlas_upload_bytes;
    if (
      decodedBytes === 786_432 &&
      submittedBytes === 786_432 &&
      report?.gpui_present_submission_observed === true &&
      command?.blocked_manifest_milestones?.length === 0
    ) {
      return {
        index,
        decoded_payload_bytes: decodedBytes,
        renderer_resource_submission_bytes: submittedBytes,
        physical_bus_upload_bytes: null,
        presented_after_submission: true,
      };
    }
  }
  return null;
}

function exactPersistenceProof(events, persistenceEvidence) {
  const indexed = indexedEvents(
    events,
    (event) => event.event === "persistence-evidence-complete",
  );
  return indexed.length === 1 &&
    exactPersistenceReceiptSucceeded(events, persistenceEvidence)
    ? indexed[0]
    : null;
}

function proveV4Command(command, events, context, evidence) {
  const sourceCommandId = v4SourceCommandIds[command.id] ?? command.id;
  const proven = new Set();
  const indexes = new Set();
  const claims = {};
  for (const { event, index } of indexedEvents(
    events,
    (candidate) =>
      candidate.event === "comparison-milestone" &&
      candidate.command_id === sourceCommandId,
  )) {
    proven.add(event.milestone);
    indexes.add(index);
  }
  const commandEvidence = evidenceMilestones(events, sourceCommandId);
  for (const milestone of commandEvidence.milestones) proven.add(milestone);
  for (const index of commandEvidence.indexes) indexes.add(index);

  if (command.expected_milestones.includes(v4VisibleRasterReadinessMilestone)) {
    const readiness = indexedEvents(
      events,
      (event) =>
        event.event === "comparison-raster-readiness-observed" &&
        event.command_id === sourceCommandId &&
        Number.isInteger(event.raster_observation_count) &&
        event.raster_observation_count > 0 &&
        Number.isInteger(event.missing_raster_observation_count) &&
        event.missing_raster_observation_count >= 0 &&
        event.missing_raster_observation_count <=
          event.raster_observation_count &&
        Number.isFinite(event.readiness_rate) &&
        event.readiness_rate >= 0 &&
        event.readiness_rate <= 1,
    );
    if (readiness.length === 1) {
      proven.add(v4VisibleRasterReadinessMilestone);
      indexes.add(readiness[0].index);
      claims.visible_raster_readiness = {
        raster_observation_count: readiness[0].event.raster_observation_count,
        missing_raster_observation_count:
          readiness[0].event.missing_raster_observation_count,
        readiness_rate: readiness[0].event.readiness_rate,
        acceptance_role: "diagnostic-counts-and-rate",
      };
    } else {
      proven.delete(v4VisibleRasterReadinessMilestone);
    }
  }

  if (
    [
      "small:launch-cold",
      "small:open-settle",
      "nasa:open-settle",
      "engineering:open-settle",
    ].includes(command.id)
  ) {
    const open = exactOpenProof(command, events, context, evidence);
    for (const milestone of open.proven) proven.add(milestone);
    for (const index of open.indexes) indexes.add(index);
    Object.assign(claims, open.claims);
  }

  if (command.id === "nasa:cache-pressure") {
    const uploads = indexedEvents(
      events,
      (event) =>
        event.event === "comparison-tile-atlas-upload-bytes" &&
        event.bytes > 0 &&
        event.evidence_kind === "gpui-wgpu-paint-image-atlas-upload-queued" &&
        event.physical_bus_upload_bytes === null,
    );
    if (uploads.length === 1 && proven.has("upload-byte-count-recorded")) {
      proven.add("renderer-resource-submission-bytes-exact");
      indexes.add(uploads[0].index);
      claims.renderer_resource_submission = {
        bytes: uploads[0].event.bytes,
        scope: uploads[0].event.evidence_kind,
        physical_bus_upload_bytes: null,
      };
    }
  }

  if (command.id === "engineering:zoom-sequence") {
    const observations = indexedEvents(
      events,
      (event) =>
        event.event === "comparison-v4-zoom-generation-evidence" &&
        event.command_id === sourceCommandId &&
        event.stale_visible_generation_count === 0 &&
        event.visible_tile_count <= event.maximum_visible_tiles,
    );
    if (
      observations.length === zoomSequence.length &&
      observations.every(({ event }, index) => event.step_index === index)
    ) {
      proven.add("stale-generations-presented-zero");
      for (const observation of observations) indexes.add(observation.index);
      claims.zoom_generation_observation_count = observations.length;
    }
  }

  if (command.id === "engineering:pan") {
    const input = indexedEvents(
      events,
      (event) =>
        event.event === "comparison-v4-pan-input-evidence" &&
        event.command_id === sourceCommandId &&
        event.timestamped_input_complete === true &&
        event.input_samples === event.expected_input_samples &&
        event.input_rate_hz > 0 &&
        event.duration_ms > 0,
    );
    if (input.length === 1) {
      proven.add("timestamped-input-complete");
      indexes.add(input[0].index);
      claims.timestamped_pan_input = {
        input_samples: input[0].event.input_samples,
        input_rate_hz: input[0].event.input_rate_hz,
        duration_ms: input[0].event.duration_ms,
      };
    }
  }

  const localCommandId =
    localV4EngineeringComponents[context.component_scenario];
  if (command.id === localCommandId) {
    const local = indexedEvents(
      events,
      (event) =>
        event.event === "comparison-v4-command-receipt" &&
        event.command_id === command.id &&
        event.component_scenario === context.component_scenario &&
        event.passed === true &&
        exactMilestones(event.milestone_ids, command.expected_milestones) &&
        exactLocalV4Evidence(context.component_scenario, event),
    );
    if (local.length === 1) {
      for (const milestone of command.expected_milestones)
        proven.add(milestone);
      indexes.add(local[0].index);
      claims.local_v4_exact_receipt = local[0].event;
    }
  }

  if (["image:create", "image:resize-history"].includes(command.id)) {
    const resource = exactImageResourceProof(command.id, events);
    if (resource) {
      proven.add("renderer-resource-submission-bytes-exact");
      proven.add("bitmap-presented-from-decoded-payload");
      if (command.id === "image:create") {
        proven.add("decoded-payload-bytes-exact");
        proven.add("bitmap-decoded");
      }
      indexes.add(resource.index);
      claims.image_resource = resource;
    }
  }

  if (context.component_scenario === "editor-workload") {
    const completed = indexedEvents(
      events,
      (event) =>
        event.event === "comparison-command-complete" &&
        event.command_id === command.id,
    );
    if (completed.length === 1) indexes.add(completed[0].index);
    else claims.command_completion_count = completed.length;
  }

  if (context.component_scenario === "persistence-workload") {
    const persistence = exactPersistenceProof(
      events,
      evidence.persistenceEvidence,
    );
    const completed = indexedEvents(
      events,
      (event) =>
        event.event === "comparison-command-complete" &&
        event.command_id === command.id,
    );
    if (persistence && completed.length === 1) {
      for (const milestone of command.expected_milestones)
        proven.add(milestone);
      indexes.add(persistence.index);
      indexes.add(completed[0].index);
      claims.persistence_exact_receipt = {
        cycle_1_sha256: persistence.event.exact_receipt.cycle_1_sha256,
        cycle_2_sha256: persistence.event.exact_receipt.cycle_2_sha256,
        validator_receipt_count:
          persistence.event.exact_receipt.validator_receipt_count,
        independent_pdf_validation_passed: true,
        independent_visual_validation_passed: true,
      };
    }
  }

  const nativeRequired = v4NativeInteractionComponents.has(
    context.component_scenario,
  );
  const nativeExact = exactNativeReplay(evidence);
  const blockers = [];
  if (evidence.applicationSuccess !== true)
    blockers.push("live-app-scenario-did-not-pass");
  if (nativeRequired && !nativeExact)
    blockers.push("native-xtest-replay-did-not-pass");
  if (!events.some(({ event }) => event === "scenario-complete")) {
    blockers.push("scenario-complete-event-missing");
  }
  const missing = command.expected_milestones.filter(
    (milestone) => !proven.has(milestone),
  );
  blockers.push(
    ...missing.map((milestone) => `missing-milestone:${milestone}`),
  );
  return {
    passed: blockers.length === 0,
    proven: command.expected_milestones.filter((milestone) =>
      proven.has(milestone),
    ),
    missing,
    indexes: [...indexes].sort((left, right) => left - right),
    blockers,
    claims,
    proofClass: nativeRequired
      ? "native-xtest-plus-live-app-state"
      : "live-app-semantic-exact",
    nativeInputEligible: nativeRequired && nativeExact,
  };
}

function hashedV4Receipt(payload) {
  return {
    ...payload,
    evidence_sha256: createHash("sha256")
      .update(JSON.stringify(canonicalize(payload)))
      .digest("hex"),
  };
}

export function buildV4ComponentReceipts(
  iteration,
  events,
  context,
  evidence = {
    applicationSuccess: true,
    inputLane: semanticDiagnosticInputLane,
  },
) {
  const commandIds =
    context?.component_command_ids?.[context.component_scenario] ??
    [localV4EngineeringComponents[context?.component_scenario]].filter(Boolean);
  const commands = commandIds.map((commandId) =>
    context?.commands?.find(({ id }) => id === commandId),
  );
  if (commands.length === 0 || commands.some((command) => !command)) {
    return { iteration, passed: false, receipts: [] };
  }
  const receipts = commands.map((command) => {
    const proof = proveV4Command(command, events, context, evidence);
    const fixedCropCommand = command.expected_milestones.includes(
      "fixed-crops-matched",
    );
    const longbridgeParityEligible = fixedCropCommand
      ? evidence.compatProfile === longbridgeCompatProfile &&
        proof.passed &&
        proof.claims.fixed_crop_registration?.evidence_source ===
          "independent-native-driver"
      : null;
    return hashedV4Receipt({
      command_id: command.id,
      live: true,
      passed: proof.passed,
      component_scenario: context.component_scenario,
      parent_scenario: context.scenario,
      milestone_ids: proof.proven,
      expected_milestone_ids: [...command.expected_milestones],
      missing_milestone_ids: proof.missing,
      evidence_event_indexes: proof.indexes,
      proof_class: proof.proofClass,
      native_input_eligible: proof.nativeInputEligible,
      longbridge_parity_eligible: longbridgeParityEligible,
      claims: proof.claims,
      blockers: proof.blockers,
    });
  });
  return {
    iteration,
    passed:
      receipts.length === commands.length &&
      receipts.every(({ passed }) => passed === true),
    receipts,
  };
}

export function buildV4ComparisonReport(context, iterations) {
  return {
    ...context,
    live_component_passed:
      iterations.length > 0 &&
      iterations.every((iteration) => iteration.success === true),
    command_receipts_by_iteration: iterations.map(
      (iteration) => iteration.v4_component_receipts,
    ),
    component_receipts_passed:
      iterations.length > 0 &&
      iterations.every(
        (iteration) => iteration.v4_component_receipts?.passed === true,
      ),
  };
}

async function fileProvenance(path) {
  const metadata = await stat(path);
  if (!metadata.isFile()) throw new Error(`not a file: ${path}`);
  const hash = createHash("sha256");
  await new Promise((resolvePromise, rejectPromise) => {
    const input = createReadStream(path);
    input.on("data", (chunk) => hash.update(chunk));
    input.on("error", rejectPromise);
    input.on("end", resolvePromise);
  });
  return {
    path,
    bytes: metadata.size,
    sha256: hash.digest("hex"),
    modified_at: metadata.mtime.toISOString(),
  };
}

async function captureValidator(command, args) {
  try {
    const { stdout, stderr } = await execFileAsync(command, args, {
      encoding: "utf8",
      timeout: 15_000,
      maxBuffer: 1_000_000,
    });
    return { command, args, exit_code: 0, stdout, stderr, passed: true };
  } catch (error) {
    return {
      command,
      args,
      exit_code: Number.isInteger(error?.code) ? error.code : null,
      stdout: error?.stdout ?? "",
      stderr: error?.stderr ?? "",
      passed: false,
      error: error?.message ?? String(error),
    };
  }
}

export async function collectPersistenceEvidence(
  evidenceDirectory,
  { artifactsRetained = false } = {},
) {
  const artifacts = [];
  const validators = [];
  for (const cycle of [1, 2]) {
    const path = resolve(evidenceDirectory, `cycle-${cycle}.pdf`);
    try {
      artifacts.push({ cycle, ...(await fileProvenance(path)) });
    } catch (error) {
      return {
        status: "failed",
        evidence_class: "diagnostic-only",
        decision_timing_eligible: false,
        artifacts_retained: artifactsRetained,
        artifacts,
        validators,
        error: `cycle ${cycle} evidence unavailable: ${error.message}`,
      };
    }
    validators.push(
      {
        cycle,
        validator: "qpdf",
        ...(await captureValidator("qpdf", ["--check", path])),
      },
      {
        cycle,
        validator: "pdfinfo",
        ...(await captureValidator("pdfinfo", [path])),
      },
    );
  }
  let visualOracle = null;
  if (artifactsRetained) {
    const cropArtifacts = [
      ["source", "source-crop.ppm"],
      ["cycle-1", "cycle-1-crop.ppm"],
      ["cycle-2", "cycle-2-crop.ppm"],
    ];
    const crops = {};
    for (const [identity, fileName] of cropArtifacts) {
      const path = resolve(evidenceDirectory, fileName);
      try {
        const artifact = await fileProvenance(path);
        crops[identity] = artifact;
        artifacts.push({
          artifact: "fixed-raster-crop",
          identity,
          ...artifact,
        });
      } catch (error) {
        return {
          status: "failed",
          evidence_class: "diagnostic-only",
          decision_timing_eligible: false,
          artifacts_retained: true,
          artifacts,
          validators,
          visual_oracle: null,
          error: `${identity} raster evidence unavailable: ${error.message}`,
        };
      }
    }
    const cropChangedFromSource =
      crops.source.sha256 !== crops["cycle-1"].sha256;
    const cropStableAcrossCycles =
      crops["cycle-1"].sha256 === crops["cycle-2"].sha256;
    visualOracle = {
      status:
        cropChangedFromSource && cropStableAcrossCycles ? "passed" : "failed",
      renderer: "pdftoppm-in-application-persistence-gate",
      page: 1,
      dpi: 72,
      crop_device_pixels: [54, 250, 510, 430],
      source_crop_sha256: crops.source.sha256,
      cycle_1_crop_sha256: crops["cycle-1"].sha256,
      cycle_2_crop_sha256: crops["cycle-2"].sha256,
      changed_from_source: cropChangedFromSource,
      stable_across_cycles: cropStableAcrossCycles,
    };
  }
  const passed =
    validators.every((validator) => validator.passed) &&
    (visualOracle === null || visualOracle.status === "passed");
  return {
    status: passed ? "passed" : "failed",
    evidence_class: "diagnostic-only",
    decision_timing_eligible: false,
    artifacts_retained: artifactsRetained,
    artifact_disposition: artifactsRetained
      ? `PDFs and raster crops retained in ${evidenceDirectory}`
      : "cycle PDFs are removed with the disposable cache after hashes and validator outputs enter this report",
    artifacts,
    validators,
    visual_oracle: visualOracle,
  };
}

const exactPersistenceCommandIds = Object.freeze([
  "unknown:import",
  "unknown:assert-cycle-1",
  "unknown:assert-cycle-2",
  "persistence:apply-fixed-state",
  "persistence:save-1",
  "persistence:reopen-1",
  "persistence:save-2",
  "persistence:reopen-2",
]);

export function exactPersistenceReceiptSucceeded(events, collectedEvidence) {
  const receipts = events.filter(
    ({ event }) => event === "persistence-evidence-complete",
  );
  if (receipts.length !== 1 || collectedEvidence?.status !== "passed")
    return false;
  const event = receipts[0];
  const receipt = event.exact_receipt;
  const sha256 = (value) =>
    typeof value === "string" && /^[0-9a-f]{64}$/i.test(value);
  if (
    event.receipt_status !== "exact-passed" ||
    receipt?.status !== "exact-passed" ||
    JSON.stringify(receipt.completed_command_ids) !==
      JSON.stringify(exactPersistenceCommandIds) ||
    receipt.typed_state_exact !== true ||
    receipt.unknown_probes_exact !== true ||
    !Number.isInteger(receipt.untouched_annotation_count) ||
    receipt.untouched_annotation_count < 1 ||
    receipt.independent_pdf_validation_passed !== true ||
    receipt.independent_visual_validation_passed !== true ||
    receipt.validator_receipt_count !== 4 ||
    !sha256(receipt.cycle_1_sha256) ||
    !sha256(receipt.cycle_2_sha256) ||
    !sha256(receipt.source_crop_sha256) ||
    !sha256(receipt.cycle_1_crop_sha256) ||
    !sha256(receipt.cycle_2_crop_sha256) ||
    receipt.source_crop_sha256 === receipt.cycle_1_crop_sha256 ||
    receipt.cycle_1_crop_sha256 !== receipt.cycle_2_crop_sha256
  ) {
    return false;
  }
  const cycleArtifacts = new Map(
    (collectedEvidence.artifacts ?? [])
      .filter(({ cycle }) => cycle === 1 || cycle === 2)
      .map((artifact) => [artifact.cycle, artifact]),
  );
  if (
    cycleArtifacts.get(1)?.sha256 !== receipt.cycle_1_sha256 ||
    cycleArtifacts.get(2)?.sha256 !== receipt.cycle_2_sha256 ||
    collectedEvidence.validators?.length !== 4 ||
    !collectedEvidence.validators.every(({ passed }) => passed === true)
  ) {
    return false;
  }
  const visual = collectedEvidence.visual_oracle;
  return (
    visual === null ||
    (visual.status === "passed" &&
      visual.source_crop_sha256 === receipt.source_crop_sha256 &&
      visual.cycle_1_crop_sha256 === receipt.cycle_1_crop_sha256 &&
      visual.cycle_2_crop_sha256 === receipt.cycle_2_crop_sha256)
  );
}

export function persistenceScenarioSucceeded({
  timedOut,
  outcome,
  invalidStdout,
  events,
  comparisonContract,
  collectedEvidence,
}) {
  if (
    !scenarioSucceeded({
      timedOut,
      outcome,
      invalidStdout,
      events,
      comparisonContract: null,
    })
  )
    return false;
  if (
    !events.some(
      ({ event, input_lane: inputLane }) =>
        event === "scenario-lane" &&
        inputLane === comparisonContract?.input_lane,
    )
  )
    return false;
  const completed = new Set(
    events
      .filter(({ event }) => event === "comparison-command-complete")
      .map(({ command_id: commandId }) => commandId),
  );
  return (
    exactPersistenceCommandIds.every((commandId) => completed.has(commandId)) &&
    exactPersistenceReceiptSucceeded(events, collectedEvidence)
  );
}

export function gpuiComparisonMetadata(
  workload,
  scenario,
  inputLane,
  iterations = [],
) {
  const metadata = runnerComparisonMetadata(workload, "gpui", scenario);
  if (inputLane !== nativeX11InputLane) return metadata;
  const allExact =
    iterations.length > 0 &&
    iterations.every(
      (iteration) =>
        iteration.success === true &&
        iteration.native_input?.evidence?.success === true &&
        iteration.application_success === true,
    );
  return {
    ...metadata,
    execution_lane: nativeX11InputLane,
    diagnostic_timing_eligible: allExact,
    decision_timing_eligible: allExact && metadata.feature_coverage.ready,
    blocked_reason: allExact
      ? metadata.feature_coverage.ready
        ? null
        : "full-comparison-feature-coverage-incomplete"
      : "native-replay-has-not-passed-exact-milestones-and-timing",
  };
}

export function qualifyNativeLaneMetadata(
  evidence,
  featureCoverageReady = false,
) {
  const metadata = nativeX11LaneMetadata(evidence);
  const diagnosticTimingEligible = metadata.decision_timing_eligible;
  return {
    ...metadata,
    execution_lane: nativeX11InputLane,
    diagnostic_timing_eligible: diagnosticTimingEligible,
    decision_timing_eligible: diagnosticTimingEligible && featureCoverageReady,
    blocked_reason:
      metadata.decision_timing_eligible && !featureCoverageReady
        ? "full-comparison-feature-coverage-incomplete"
        : metadata.decision_timing_eligible
          ? null
          : "native-replay-has-not-passed-exact-milestones-and-timing",
  };
}

async function optionalCommand(command, args, cwd) {
  try {
    const { stdout } = await execFileAsync(command, args, {
      encoding: "utf8",
      cwd,
      timeout: 5_000,
      maxBuffer: 256_000,
    });
    return stdout.trim() || null;
  } catch {
    return null;
  }
}

async function collectProvenance(binary, pdfRuntime) {
  const cpuList = cpus();
  const [
    macosVersion,
    gitRevision,
    gitStatus,
    nvidiaGpu,
    vulkanSummary,
    displayMode,
  ] = await Promise.all([
    optionalCommand("/usr/bin/sw_vers", ["-productVersion"]),
    optionalCommand("/usr/bin/git", ["rev-parse", "HEAD"], repositoryDirectory),
    optionalCommand(
      "/usr/bin/git",
      ["status", "--porcelain=v1", "--untracked-files=all"],
      repositoryDirectory,
    ),
    optionalCommand("nvidia-smi", [
      "--query-gpu=name,uuid,driver_version,memory.total",
      "--format=csv,noheader,nounits",
    ]),
    optionalCommand("vulkaninfo", ["--summary"]),
    optionalCommand("xrandr", ["--current"]),
  ]);
  return {
    captured_at: new Date().toISOString(),
    host: {
      hostname: hostname(),
      os_type: type(),
      platform: platform(),
      os_release: release(),
      macos_version: macosVersion,
      architecture: arch(),
      logical_cpu_count: cpuList.length,
      cpu_model: cpuList[0]?.model ?? null,
      total_memory_bytes: totalmem(),
      free_memory_bytes_at_start: freemem(),
      display_mode: displayMode,
      nvidia_gpu: nvidiaGpu,
      vulkan_summary: vulkanSummary,
    },
    runtime: {
      runner: "gpui-runner.mjs",
      node: process.version,
      node_versions: process.versions,
      sample_interval_ms: sampleIntervalMs,
      gpui_x11_scale_factor_override: process.env.GPUI_X11_SCALE_FACTOR ?? null,
      git_revision: gitRevision,
      git_status_sha256: createHash("sha256")
        .update(gitStatus ?? "")
        .digest("hex"),
      binary: await fileProvenance(binary),
      pdf_worker: await fileProvenance(pdfRuntime.worker),
      pdfium_library: await fileProvenance(pdfRuntime.library),
    },
  };
}

async function resolvePdfRuntime(binary) {
  const workerName =
    platform() === "win32"
      ? "butter-paper-pdf-worker.exe"
      : "butter-paper-pdf-worker";
  const worker = resolve(dirname(binary), workerName);
  const library = await fetchDevelopmentPdfium();
  await Promise.all([fileProvenance(worker), fileProvenance(library)]);
  return { worker, library };
}

function percentile(values, fraction) {
  if (values.length === 0) return null;
  const ordered = [...values].sort((left, right) => left - right);
  const position = (ordered.length - 1) * fraction;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  if (lower === upper) return ordered[lower];
  return (
    ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)
  );
}

function numericSummary(values) {
  const valid = values.filter(Number.isFinite);
  if (valid.length === 0) return null;
  const total = valid.reduce((sum, value) => sum + value, 0);
  return {
    count: valid.length,
    min: Math.min(...valid),
    median: percentile(valid, 0.5),
    mean: total / valid.length,
    p95: percentile(valid, 0.95),
    max: Math.max(...valid),
  };
}

async function directorySummary(path) {
  let fileCount = 0;
  let bytes = 0;
  async function visit(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const entryPath = resolve(directory, entry.name);
      if (entry.isDirectory()) await visit(entryPath);
      if (entry.isFile()) {
        const metadata = await stat(entryPath);
        fileCount += 1;
        bytes += metadata.size;
      }
    }
  }
  await visit(path);
  return { file_count: fileCount, bytes };
}

async function sampleProcessTree(rootPid) {
  const { stdout } = await execFileAsync(
    "/bin/ps",
    ["-axo", "pid=,ppid=,%cpu=,rss="],
    { encoding: "utf8", timeout: 2_000, maxBuffer: 4_000_000 },
  );
  const processes = stdout
    .split("\n")
    .map((line) => line.trim().split(/\s+/).map(Number))
    .filter(
      ([pid, parentPid, cpu, rss]) =>
        Number.isInteger(pid) &&
        Number.isInteger(parentPid) &&
        Number.isFinite(cpu) &&
        Number.isFinite(rss),
    )
    .map(([pid, parentPid, cpuPercent, rssKb]) => ({
      pid,
      parentPid,
      cpuPercent,
      rssKb,
    }));

  const included = new Set([rootPid]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const processEntry of processes) {
      if (
        included.has(processEntry.parentPid) &&
        !included.has(processEntry.pid)
      ) {
        included.add(processEntry.pid);
        changed = true;
      }
    }
  }
  const tree = processes.filter((entry) => included.has(entry.pid));
  if (tree.length === 0) return null;
  return {
    process_count: tree.length,
    cpu_percent: tree.reduce((sum, entry) => sum + entry.cpuPercent, 0),
    rss_kb: tree.reduce((sum, entry) => sum + entry.rssKb, 0),
    pids: tree.map((entry) => entry.pid),
  };
}

function terminateProcessGroup(pid, signal) {
  try {
    process.kill(-pid, signal);
  } catch (error) {
    if (error.code !== "ESRCH") throw error;
  }
}

export function gpuiGpuEvidencePassed(gpuMetrics) {
  return gpuMetrics?.qualification?.passed === true;
}

async function runIteration(options, iteration) {
  const events = [];
  const invalidStdout = [];
  const samples = [];
  let stdoutBuffer = "";
  let stderr = "";
  let sampling = true;
  let sampleInProgress = false;
  let timedOut = false;
  let cgroupMetrics;
  let cgroupCleanup;
  let gpuMetrics;
  const cacheDirectory = resolve(
    dirname(options.output),
    ".gpui-cold-cache",
    `${Date.now()}-${process.pid}-${iteration}`,
  );
  const evidenceDirectory = options.evidenceDirectory
    ? resolve(
        options.evidenceDirectory,
        `iteration-${String(iteration).padStart(3, "0")}`,
      )
    : null;
  const dynamicArtifactDirectory =
    options.scenario === dynamicFidelityScenario
      ? dynamicArtifactDirectoryForOutput(options.output, iteration)
      : null;
  await mkdir(cacheDirectory, { recursive: true });
  if (dynamicArtifactDirectory) {
    await prepareFreshArtifactDirectory(dynamicArtifactDirectory);
  }
  if (evidenceDirectory) {
    await prepareFreshArtifactDirectory(evidenceDirectory);
  }
  const gpuSampler = await startNvidiaBaselineRunSampler();
  const startedAt = new Date();
  const startedMonotonic = process.hrtime.bigint();

  // AppKit can restore a stale crash dialog before GPUI reaches its first
  // window. Keep this benchmark launch deterministic. The GPUI gallery
  // filters the two NSUserDefaults arguments before treating the remaining
  // arguments as document paths.
  const applicationArguments =
    platform() === "darwin"
      ? ["-ApplePersistenceIgnoreState", "YES", ...options.pdfs]
      : [...options.pdfs];
  const cgroup = await createLinuxCgroup(`gpui-${process.pid}-${iteration}`);
  const launch = cgroupLaunch(cgroup, options.binary, applicationArguments);
  gpuSampler.startRun();
  const child = spawn(launch.executable, launch.args, {
    env: {
      ...process.env,
      BP_NATIVE_DEVELOPMENT: "1",
      BP_GPUI_PERF_SCENARIO: options.scenario,
      BP_GPUI_PERF_ITERATION: String(iteration),
      BP_GPUI_INPUT_LANE: options.inputLane,
      ...(options.compatProfile
        ? { BP_GPUI_COMPAT_PROFILE: options.compatProfile }
        : {}),
      BP_GPUI_IMAGE_ASSET_PATH: resolve(
        performanceDirectory,
        "results/public-fixtures-v1/bp-image-checker-v1.png",
      ),
      ...(options.v5RunnerContext || options.v4RunnerContext
        ? {
            BP_GPUI_FIXTURE_IDS: JSON.stringify(
              fixtureIdsForLaunch(
                options.v5RunnerContext,
                options.v4RunnerContext,
              ),
            ),
          }
        : {}),
      ...(options.v4RunnerContext
        ? { BP_GPUI_V4_MANIFEST_ID: options.v4RunnerContext.manifest_id }
        : {}),
      BP_GPUI_CACHE_DIR: cacheDirectory,
      ...(evidenceDirectory || dynamicArtifactDirectory
        ? {
            BP_GPUI_EVIDENCE_DIR: evidenceDirectory ?? dynamicArtifactDirectory,
          }
        : {}),
      BP_PDF_WORKER_EXE: options.pdfRuntime.worker,
      BP_PDFIUM_LIBRARY: options.pdfRuntime.library,
    },
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  const requireCommonDamageObserver =
    options.v6Scenario || process.env.BP_PERF_COMMON_DAMAGE_OBSERVER === "1";
  if (requireCommonDamageObserver) {
    beginX11DamageObserverCollection({ candidatePid: child.pid });
  }
  const outcomePromise = new Promise((resolvePromise) => {
    child.once("error", (error) =>
      resolvePromise({ spawn_error: error.message }),
    );
    child.once("close", (code, signal) =>
      resolvePromise({ exit_code: code, signal }),
    );
  });

  const parseLine = (line) => {
    if (!line.trim()) return;
    try {
      const event = JSON.parse(line);
      if (
        event?.schema_version !== 1 ||
        typeof event.event !== "string" ||
        !Number.isFinite(event.t_ms)
      ) {
        throw new Error(
          "expected schema_version=1, event string, and numeric t_ms",
        );
      }
      event.runner_observed_monotonic_ms =
        Number(process.hrtime.bigint()) / 1e6;
      events.push(event);
    } catch (error) {
      invalidStdout.push({ line, error: error.message });
    }
  };

  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    stdoutBuffer += chunk;
    const lines = stdoutBuffer.split("\n");
    stdoutBuffer = lines.pop() ?? "";
    for (const line of lines) parseLine(line);
  });
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk) => {
    if (stderr.length < stderrLimitBytes) {
      stderr += chunk.slice(0, stderrLimitBytes - stderr.length);
    }
  });

  const nativeReplayPromise =
    options.inputLane === nativeX11InputLane
      ? executeNativeX11Scenario({
          pid: child.pid,
          scenario: options.scenario,
          contract: options.developmentScenarioContract,
          workload: options.comparisonWorkload,
          events,
          artifactDirectory: evidenceDirectory ?? dynamicArtifactDirectory,
          ...(options.compatProfile === longbridgeCompatProfile
            ? {
                compatPresentedCrop: {
                  fixturePath: options.pdfs[0],
                  registrationPath: resolve(
                    performanceDirectory,
                    "results/public-fixtures-v1/bp-single-page-v1.crops.json",
                  ),
                },
              }
            : {}),
          timeoutMs: nativeEvidenceTimeoutMs(
            options.scenario,
            options.timeoutMs,
          ),
        }).catch((error) => {
          terminateProcessGroup(child.pid, "SIGTERM");
          return {
            success: false,
            target_verified: false,
            error: error.message,
          };
        })
      : Promise.resolve(null);

  const sample = async () => {
    if (!sampling || sampleInProgress) return;
    sampleInProgress = true;
    try {
      const snapshot = await sampleProcessTree(child.pid);
      if (snapshot) {
        samples.push({
          elapsed_ms: Number(process.hrtime.bigint() - startedMonotonic) / 1e6,
          ...snapshot,
        });
      }
    } catch (error) {
      samples.push({
        elapsed_ms: Number(process.hrtime.bigint() - startedMonotonic) / 1e6,
        sample_error: error.message,
      });
    } finally {
      sampleInProgress = false;
    }
  };

  await sample();
  const sampleTimer = setInterval(sample, sampleIntervalMs);
  sampleTimer.unref();

  let forceKillTimer;
  const timeoutTimer = setTimeout(() => {
    timedOut = true;
    terminateProcessGroup(child.pid, "SIGTERM");
    forceKillTimer = setTimeout(
      () => terminateProcessGroup(child.pid, "SIGKILL"),
      2_000,
    );
    forceKillTimer.unref();
  }, options.timeoutMs);
  timeoutTimer.unref();

  const [outcome, rawNativeReplay] = await Promise.all([
    outcomePromise,
    nativeReplayPromise,
  ]);
  let nativeReplay = rawNativeReplay;
  if (requireCommonDamageObserver) {
    try {
      nativeReplay = {
        ...rawNativeReplay,
        common_benefit_timing_boundary: finishX11DamageObserverCollection(),
      };
    } catch (error) {
      const primaryError = rawNativeReplay?.error ?? error.message;
      nativeReplay = {
        ...rawNativeReplay,
        success: false,
        common_benefit_timing_boundary: null,
        common_benefit_timing_boundary_error: primaryError,
        ...(rawNativeReplay?.error
          ? {
              common_benefit_timing_boundary_collection_error: error.message,
            }
          : {}),
      };
    } finally {
      abortX11DamageObserverCollection();
    }
  }
  clearTimeout(timeoutTimer);
  if (forceKillTimer) clearTimeout(forceKillTimer);
  clearInterval(sampleTimer);
  sampling = false;
  while (sampleInProgress) {
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 5));
  }
  if (stdoutBuffer.trim()) parseLine(stdoutBuffer);
  const compatEvidenceValidation = compatEvidenceValidationForIteration(
    options.compatProfile,
    events,
    { scenario: options.scenario, pid: child.pid },
  );
  cgroupMetrics = await readLinuxCgroup(cgroup);
  cgroupCleanup = await removeLinuxCgroup(cgroup);
  gpuMetrics = await gpuSampler.stop();

  const endedAt = new Date();
  const elapsedMs = Number(process.hrtime.bigint() - startedMonotonic) / 1e6;
  const validSamples = samples.filter((entry) => !entry.sample_error);
  const cache = await directorySummary(cacheDirectory);
  const persistenceEvidence =
    options.scenario === "persistence-workload"
      ? await collectPersistenceEvidence(evidenceDirectory ?? cacheDirectory, {
          artifactsRetained: evidenceDirectory !== null,
        })
      : null;
  await rm(cacheDirectory, { recursive: true, force: true });
  const persistenceReceiptSuccess =
    options.scenario !== "persistence-workload" ||
    exactPersistenceReceiptSucceeded(events, persistenceEvidence);
  const activeGpuAdapter = buildGpuiActiveGpuAdapterReceipt(
    events,
    options.provenance?.host?.nvidia_gpu,
  );
  const activeGpuPassed =
    !activeGpuAdapterRequired() || activeGpuAdapter.passed === true;
  const baseApplicationSuccess =
    options.scenario === "persistence-workload"
      ? persistenceScenarioSucceeded({
          timedOut,
          outcome,
          invalidStdout,
          events,
          comparisonContract: options.developmentScenarioContract,
          collectedEvidence: persistenceEvidence,
        })
      : scenarioSucceeded({
          timedOut,
          outcome,
          invalidStdout,
          events,
          comparisonContract: options.developmentScenarioContract,
        });
  const compatResourceCleanup = compatResourceCleanupForIteration(
    options.compatProfile,
    events,
    cgroupCleanup,
  );
  const applicationSuccess =
    baseApplicationSuccess &&
    (compatEvidenceValidation === null || compatEvidenceValidation.passed) &&
    (compatResourceCleanup === null || compatResourceCleanup.passed);
  const v4ComponentReceipts = options.v4RunnerContext
    ? buildV4ComponentReceipts(iteration, events, options.v4RunnerContext, {
        applicationSuccess,
        inputLane: options.inputLane,
        nativeReplay,
        persistenceEvidence,
        compatProfile: options.compatProfile,
      })
    : null;
  const v5ComponentEvidence = options.v5RunnerContext
    ? options.scenario === dynamicFidelityScenario
      ? buildDynamicFidelityV5Evidence(
          iteration,
          events,
          options.v5RunnerContext,
          {
            applicationSuccess,
            inputLane: options.inputLane,
            nativeReplay,
          },
        )
      : nativeEditingV5Scenarios.has(options.scenario)
        ? buildNativeEditingV5Evidence(
            iteration,
            events,
            options.v5RunnerContext,
            {
              applicationSuccess,
              inputLane: options.inputLane,
              nativeReplay,
            },
          )
        : buildMultiDocumentV5Evidence(
            iteration,
            events,
            options.v5RunnerContext,
            {
              applicationSuccess,
              inputLane: options.inputLane,
              nativeReplay,
            },
          )
    : null;
  const success =
    applicationSuccess &&
    (options.inputLane !== nativeX11InputLane ||
      nativeReplay?.success === true) &&
    (persistenceEvidence === null || persistenceEvidence.status === "passed") &&
    persistenceReceiptSuccess &&
    gpuiGpuEvidencePassed(gpuMetrics) &&
    activeGpuPassed &&
    (v4ComponentReceipts === null || v4ComponentReceipts.passed === true) &&
    (v5ComponentEvidence === null || v5ComponentEvidence.passed === true);

  return {
    iteration,
    started_at: startedAt.toISOString(),
    ended_at: endedAt.toISOString(),
    wall_duration_ms: elapsedMs,
    success,
    timed_out: timedOut,
    ...outcome,
    events,
    ...(compatEvidenceValidation
      ? { compat_evidence_validation: compatEvidenceValidation }
      : {}),
    ...(compatResourceCleanup
      ? { compat_resource_cleanup: compatResourceCleanup }
      : {}),
    active_gpu_adapter: activeGpuAdapter,
    invalid_stdout: invalidStdout,
    stderr,
    native_input:
      options.inputLane === nativeX11InputLane
        ? {
            ...qualifyNativeLaneMetadata(
              {
                ...nativeReplay,
                application_success: applicationSuccess,
              },
              options.comparisonFeatureCoverageReady === true,
            ),
            evidence: nativeReplay,
          }
        : {
            input_lane: semanticDiagnosticInputLane,
            execution_status: "semantic-diagnostic",
            real_gui_run: false,
            decision_timing_eligible: false,
          },
    cache,
    persistence_evidence: persistenceEvidence,
    v4_component_receipts: v4ComponentReceipts,
    v5_component_evidence: v5ComponentEvidence,
    evidence_directory: evidenceDirectory,
    samples,
    cgroup: cgroupMetrics,
    cgroup_cleanup: cgroupCleanup,
    gpu: gpuMetrics,
    gpu_evidence_blocker: gpuiGpuEvidencePassed(gpuMetrics)
      ? activeGpuPassed
        ? null
        : activeGpuAdapter.blocker
      : (gpuMetrics?.qualification?.blocker ??
        "required NVIDIA evidence is missing"),
    resource_summary: {
      sample_count: validSamples.length,
      cpu_percent: numericSummary(
        validSamples.map((entry) => entry.cpu_percent),
      ),
      rss_kb: numericSummary(validSamples.map((entry) => entry.rss_kb)),
      process_count: numericSummary(
        validSamples.map((entry) => entry.process_count),
      ),
    },
    application_success: applicationSuccess,
  };
}

function summarizeDurationEvents(iterations) {
  const byEvent = new Map();
  for (const iteration of iterations) {
    for (const event of iteration.events) {
      if (!Number.isFinite(event.duration_ms)) continue;
      if (!byEvent.has(event.event)) byEvent.set(event.event, []);
      byEvent.get(event.event).push(event.duration_ms);
    }
  }
  return Object.fromEntries(
    [...byEvent.entries()]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([event, durations]) => [event, numericSummary(durations)]),
  );
}

function summarizeEventTimestamps(iterations) {
  const byEvent = new Map();
  for (const iteration of iterations) {
    for (const event of iteration.events) {
      if (!byEvent.has(event.event)) byEvent.set(event.event, []);
      byEvent.get(event.event).push(event.t_ms);
    }
  }
  return Object.fromEntries(
    [...byEvent.entries()]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([event, timestamps]) => [event, numericSummary(timestamps)]),
  );
}

export function gpuiNativeApplicationAckSamples(iterations) {
  return iterations.flatMap((iteration) =>
    iteration.events
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
      .map((event) => event.input_to_application_draw_ack_p95_ns / 1e6),
  );
}

function summarizeReport(iterations) {
  const allSamples = iterations.flatMap((iteration) =>
    iteration.samples.filter((sample) => !sample.sample_error),
  );
  const eventTimes = iterations.flatMap((iteration) =>
    iteration.events.map((event) => event.t_ms),
  );
  const frameIntervals = iterations.flatMap((iteration) =>
    iteration.events
      .filter(
        (event) =>
          event.event === "frame" && Number.isFinite(event.interval_ms),
      )
      .map((event) => event.interval_ms),
  );
  const nativeApplicationAckSamples =
    gpuiNativeApplicationAckSamples(iterations);
  return {
    successful_iterations: iterations.filter((iteration) => iteration.success)
      .length,
    failed_iterations: iterations.filter((iteration) => !iteration.success)
      .length,
    wall_duration_ms: numericSummary(
      iterations.map((iteration) => iteration.wall_duration_ms),
    ),
    event_t_ms: numericSummary(eventTimes),
    event_timestamps_ms: summarizeEventTimestamps(iterations),
    duration_events_ms: summarizeDurationEvents(iterations),
    frame_intervals_ms: numericSummary(frameIntervals),
    application_frame_intervals_ms: numericSummary(frameIntervals),
    native_input_to_application_frame_ack_ms: numericSummary(
      nativeApplicationAckSamples,
    ),
    native_application_frame_acknowledgement_proxy: {
      receipt_scope:
        "gpui-input-latency-histogram-to-platform-draw-submission-not-physical-scanout",
      physical_scanout_observed: false,
      sample_count: nativeApplicationAckSamples.length,
    },
    frame_interval_thresholds: {
      over_8_33_ms: frameIntervals.filter((value) => value > 8.33).length,
      over_16_67_ms: frameIntervals.filter((value) => value > 16.67).length,
      over_33_33_ms: frameIntervals.filter((value) => value > 33.33).length,
    },
    cache: {
      files: numericSummary(
        iterations.map((iteration) => iteration.cache.file_count),
      ),
      bytes: numericSummary(
        iterations.map((iteration) => iteration.cache.bytes),
      ),
    },
    process_tree: {
      cpu_percent: numericSummary(
        allSamples.map((sample) => sample.cpu_percent),
      ),
      rss_kb: numericSummary(allSamples.map((sample) => sample.rss_kb)),
      peak_cpu_percent:
        allSamples.length > 0
          ? Math.max(...allSamples.map((sample) => sample.cpu_percent))
          : null,
      median_cpu_percent: percentile(
        allSamples.map((sample) => sample.cpu_percent),
        0.5,
      ),
      peak_rss_kb:
        allSamples.length > 0
          ? Math.max(...allSamples.map((sample) => sample.rss_kb))
          : null,
      median_rss_kb: percentile(
        allSamples.map((sample) => sample.rss_kb),
        0.5,
      ),
      cpu_seconds: numericSummary(
        iterations.map((iteration) => iteration.cgroup?.cpu_seconds),
      ),
      cgroup_memory_peak_bytes: numericSummary(
        iterations.map((iteration) => iteration.cgroup?.memory_peak_bytes),
      ),
    },
    ...summarizeNvidiaIterations(iterations),
  };
}

async function main() {
  let options;
  try {
    options = parseArguments(process.argv.slice(2));
  } catch (error) {
    fail(error.message);
    return;
  }
  if (options.help) {
    process.stdout.write(usage());
    return;
  }

  let pdf;
  let provenance;
  let pdfRuntime;
  let comparisonWorkload;
  let developmentScenarioContract;
  let comparisonWorkloadV4;
  let v4RunnerContext;
  let comparisonWorkloadV5;
  let v5RunnerContext;
  let comparisonWorkloadV6;
  let v6RunnerContext;
  try {
    if (options.v6Scenario) {
      const bytes = await readFile(v6WorkloadPath);
      comparisonWorkloadV6 = JSON.parse(bytes);
      v6RunnerContext = createGpuiV6ExecutionContext({
        workload: comparisonWorkloadV6,
        workloadByteSha256: createHash("sha256").update(bytes).digest("hex"),
        parentScenario: options.v6Scenario,
        componentScenario: options.scenario,
      });
    }
    if (
      options.scenario === multiDocumentScenario ||
      options.scenario === dynamicFidelityScenario ||
      nativeEditingV5Scenarios.has(options.scenario)
    ) {
      comparisonWorkloadV5 = await loadComparisonWorkloadV5();
      const v5Errors = validateComparisonWorkloadV5(comparisonWorkloadV5);
      if (v5Errors.length > 0) {
        throw new Error(
          `invalid v5 comparison workload: ${v5Errors.join("; ")}`,
        );
      }
      v5RunnerContext =
        options.scenario === dynamicFidelityScenario
          ? buildDynamicFidelityV5Context(comparisonWorkloadV5)
          : nativeEditingV5Scenarios.has(options.scenario)
            ? buildNativeEditingV5Context(
                comparisonWorkloadV5,
                options.scenario,
              )
            : buildScenarioContractV5(comparisonWorkloadV5, options.scenario);
      v5RunnerContext = {
        ...v5RunnerContext,
        workload_artifact_sha256:
          comparisonWorkloadArtifactHashV5(comparisonWorkloadV5),
        workload_byte_sha256:
          comparisonWorkloadByteHashV5(comparisonWorkloadV5),
      };
      developmentScenarioContract = v5RunnerContext;
      comparisonWorkload = comparisonWorkloadV5;
    } else {
      comparisonWorkload = await loadComparisonWorkload();
      const comparisonErrors = validateComparisonWorkload(comparisonWorkload);
      if (comparisonErrors.length > 0) {
        throw new Error(
          `invalid comparison workload: ${comparisonErrors.join("; ")}`,
        );
      }
      developmentScenarioContract = buildDevelopmentScenarioContract(
        comparisonWorkload,
        options.scenario,
        options.inputLane,
      );
    }
    if (options.v4Scenario) {
      comparisonWorkloadV4 = await loadComparisonWorkloadV4();
      const v4Errors = validateComparisonWorkloadV4(comparisonWorkloadV4);
      if (v4Errors.length > 0) {
        throw new Error(
          `invalid v4 comparison workload: ${v4Errors.join("; ")}`,
        );
      }
      v4RunnerContext = buildV4RunnerContext(
        comparisonWorkloadV4,
        options.v4Scenario,
        options.scenario,
      );
      developmentScenarioContract = applyV4ComponentExecutionContract(
        developmentScenarioContract,
        v4RunnerContext,
      );
    }
    const pdfs = await Promise.all(options.pdfs.map(fileProvenance));
    pdf = pdfs[0];
    pdfRuntime = await resolvePdfRuntime(options.binary);
    provenance = await collectProvenance(options.binary, pdfRuntime);
    options.provenance = provenance;
    const fixtureError = v5RunnerContext
      ? validateOrderedScenarioFixtures(pdfs, v5RunnerContext)
      : validateScenarioFixture(
          pdf,
          v4RunnerContext ?? developmentScenarioContract,
        );
    if (fixtureError) throw new Error(fixtureError);
    options.developmentScenarioContract = developmentScenarioContract;
    options.comparisonWorkload = comparisonWorkload;
    options.v4RunnerContext = v4RunnerContext;
    options.v5RunnerContext = v5RunnerContext;
    options.pdfProvenance = pdfs;
    options.pdfRuntime = pdfRuntime;
    options.comparisonFeatureCoverageReady = v6RunnerContext
      ? true
      : v5RunnerContext
        ? true
        : runnerComparisonMetadata(comparisonWorkload, "gpui", options.scenario)
            .feature_coverage.ready;
  } catch (error) {
    fail(
      formatFixtureAccessError(
        error,
        v4RunnerContext ?? developmentScenarioContract,
        options.pdfs.join(", "),
      ),
    );
    return;
  }

  const iterations = [];
  for (let iteration = 1; iteration <= options.iterations; iteration += 1) {
    process.stderr.write(
      `GPUI ${options.scenario}: iteration ${iteration}/${options.iterations}\n`,
    );
    iterations.push(await runIteration(options, iteration));
  }

  const report = {
    schema_version: 1,
    protocol_version: v6RunnerContext ? protocolVersionV6 : protocolVersion,
    scenario_contract_version: v6RunnerContext
      ? scenarioContractVersionV6
      : scenarioContractVersion,
    implementation: "gpui",
    scenario: options.scenario,
    ...(options.compatProfile ? { compat_profile: options.compatProfile } : {}),
    requested_iterations: options.iterations,
    timeout_ms_per_iteration: options.timeoutMs,
    cache_class: "app-cold",
    input_lane: developmentScenarioContract?.input_lane ?? options.inputLane,
    input_lane_metadata:
      options.inputLane === nativeX11InputLane
        ? qualifyNativeLaneMetadata(
            {
              success:
                iterations.length > 0 &&
                iterations.every(
                  (iteration) =>
                    iteration.native_input?.evidence?.success === true,
                ),
              application_success:
                iterations.length > 0 &&
                iterations.every(
                  (iteration) => iteration.application_success === true,
                ),
              target_verified:
                iterations.length > 0 &&
                iterations.every(
                  (iteration) =>
                    iteration.native_input?.evidence?.target_verified === true,
                ),
            },
            options.comparisonFeatureCoverageReady,
          )
        : {
            input_lane: semanticDiagnosticInputLane,
            execution_status: "semantic-diagnostic",
            real_gui_run: false,
            decision_timing_eligible: false,
          },
    comparison_workload: v6RunnerContext
      ? {
          manifest_id: comparisonWorkloadV6.manifest_id,
          byte_sha256: v6RunnerContext.workload_byte_sha256,
          scenario_contract_version: v6RunnerContext.scenario_contract_version,
          fixture_ids: v6RunnerContext.execution_contract.fixture_ids,
          benefit_metrics_eligible: true,
        }
      : v5RunnerContext
        ? {
            manifest_id: comparisonWorkloadV5.manifest_id,
            artifact_sha256:
              comparisonWorkloadArtifactHashV5(comparisonWorkloadV5),
            byte_sha256: comparisonWorkloadByteHashV5(comparisonWorkloadV5),
            scenario_contract_version:
              v5RunnerContext.scenario_contract_version,
            fixture_ids: v5RunnerContext.fixture_ids,
            benefit_metrics_eligible:
              v5RunnerContext.component_benefit_metrics_eligible[
                options.scenario
              ],
          }
        : gpuiComparisonMetadata(
            comparisonWorkload,
            options.scenario,
            options.inputLane,
            iterations,
          ),
    comparison_v4: v4RunnerContext
      ? buildV4ComparisonReport(v4RunnerContext, iterations)
      : null,
    comparison_v5: v5RunnerContext
      ? {
          ...v5RunnerContext,
          execution_eligible: iterations.every(
            (iteration) => iteration.v5_component_evidence?.passed === true,
          ),
          execution_blocker: iterations.every(
            (iteration) => iteration.v5_component_evidence?.passed === true,
          )
            ? null
            : `exact live GPUI v5 ${options.scenario} receipt failed`,
          iterations: iterations.map(
            (iteration) => iteration.v5_component_evidence,
          ),
        }
      : null,
    comparison_v6: v6RunnerContext
      ? {
          ...v6RunnerContext,
          inherited_v4_execution: true,
          execution_eligible: iterations.every(
            (iteration) => iteration.v4_component_receipts?.passed === true,
          ),
        }
      : null,
    workload: (() => {
      if (developmentScenarioContract) return developmentScenarioContract;
      const events =
        iterations.find((iteration) => iteration.success)?.events ?? [];
      if (options.scenario === "page-navigation") {
        const pageCount = events.find(
          (event) => event.event === "pdf-open-completed",
        )?.pages;
        return {
          page_sequence: Number.isInteger(pageCount)
            ? normalizedPageSequence(pageCount)
            : [],
        };
      }
      return options.scenario === "zoom"
        ? { zoom_sequence_percent: zoomSequence }
        : {};
    })(),
    pdf,
    pdfs: options.pdfProvenance,
    provenance,
    summary: summarizeReport(iterations),
    iterations,
    persistence_evidence:
      options.scenario === "persistence-workload"
        ? {
            evidence_class: "diagnostic-only",
            decision_timing_eligible: false,
            artifacts_retained: Boolean(options.evidenceDirectory),
            evidence_directory: options.evidenceDirectory ?? null,
            iterations: iterations.map(
              (iteration) => iteration.persistence_evidence,
            ),
          }
        : null,
  };
  await mkdir(dirname(options.output), { recursive: true });
  await writeFile(
    options.output,
    `${JSON.stringify(report, null, 2)}\n`,
    "utf8",
  );
  process.stderr.write(`Wrote ${options.output}\n`);
  if (report.summary.failed_iterations > 0) process.exitCode = 1;
}

export function scenarioSucceeded({
  timedOut,
  outcome,
  invalidStdout,
  events,
  comparisonContract = null,
}) {
  return (
    !timedOut &&
    outcome.exit_code === 0 &&
    !outcome.spawn_error &&
    invalidStdout.length === 0 &&
    !events.some((event) => event.event === "scenario-error") &&
    comparisonMilestonesSucceeded(events, comparisonContract) &&
    events.some((event) => event.event === "scenario-complete")
  );
}

export function comparisonMilestonesSucceeded(events, contract) {
  if (!contract) return true;
  const expectedInputLane =
    contract.input_lane ??
    (contract.scenario === multiDocumentScenario
      ? nativeX11InputLane
      : undefined);
  if (
    !events.some(
      ({ event, input_lane: inputLane }) =>
        event === "scenario-lane" && inputLane === expectedInputLane,
    )
  ) {
    return false;
  }
  if (contract.semantic_command_only) {
    const completed = new Set(
      events
        .filter(({ event }) => event === "comparison-command-complete")
        .map(({ command_id: commandId }) => commandId),
    );
    return contract.command_ids.every((commandId) => completed.has(commandId));
  }
  const observed = new Set(
    events
      .filter(({ event }) => event === "comparison-milestone")
      .map(
        ({ command_id: commandId, milestone }) => `${commandId}\0${milestone}`,
      ),
  );
  return contract.commands.every((command) =>
    command.expected_milestones.every((milestone) =>
      observed.has(`${command.id}\0${milestone}`),
    ),
  );
}

export function validateScenarioFixture(pdf, contract) {
  if (!contract) return null;
  if (pdf.sha256 !== contract.fixture_sha256) {
    return `${contract.fixture_id} requires PDF SHA-256 ${contract.fixture_sha256}; received ${pdf.sha256}`;
  }
  return null;
}

export function formatFixtureAccessError(error, contract, path) {
  const absent =
    error?.code === "ENOENT" ||
    /ENOENT|no such file/i.test(error?.message ?? "");
  const lockedPublic = [
    "nasa-apollo-summary-526-v1",
    "usgs-usa-geology-sheet-v1",
  ].includes(contract?.fixture_id);
  if (absent && lockedPublic) {
    return `BLOCKED locked corpus ${contract.fixture_id} is absent at ${path}; fetch and verify the exact byte count and SHA-256 before launch`;
  }
  return error?.message ?? String(error);
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
) {
  await main();
}
