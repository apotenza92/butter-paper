#!/usr/bin/env node

import { createHash } from "node:crypto";
import { execFile, spawn } from "node:child_process";
import { mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { hostname, release } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import { loadMaterializedComparisonWorkloadV5 } from "./comparison-workload-v5.mjs";
import { createBalancedPairOrders } from "./decision-statistics.mjs";
import {
  assessV5Launch,
  buildLaunchBindingV5,
  buildRunnerInvocationV5,
  createCalibrationPairOrdersV5,
  electronPropertyOutcomeAcceptedV5,
  estimateV5PaidLease,
  parseV5Arguments,
  retainDynamicCropArtifactsV5,
  validateExactCandidateHashesV5,
  validateGpuSamplesV5,
  verifyV5FixturesAndReferences,
} from "./run-paired-v5.mjs";
import {
  buildV4ComparisonPlan,
  canonicalSha256,
  validateV4ComponentReport,
} from "./run-paired-v4.mjs";
import { buildScenarioContractV4 } from "./scenario-contract-v4.mjs";
import { loadMaterializedComparisonWorkloadV4 } from "./comparison-workload-v4.mjs";
import {
  revalidateOptimizedCandidateLaunchV4,
  validateOptimizedCandidatesV4,
} from "./optimized-candidates-v4.mjs";
import {
  compareBundleViewStatesV5,
  compareViewStateReceiptsV5,
} from "./matched-view-state-v5.mjs";
import {
  benefitEligibleComponentIdsV6,
  propertyCorrectnessOnlyComponentIdV6,
  protocolVersionV6,
  representativeScenarioDefinitionsV6,
  scenarioContractVersionV6,
  semanticCorrectnessOnlyComponentIdsV6,
  validateScenarioContractV6,
} from "./scenario-contract-v6.mjs";
import {
  x11DamageObserverIntegrationV6,
  commonX11DamageTimingBoundaryPassedV6,
} from "./x11-damage-observer.mjs";
import { preflightRequiredCgroupV2Accounting } from "./linux-cgroup.mjs";
import {
  classifyElectronEngineeringZoomBaselineDefectV6,
  electronEngineeringZoomBaselineDefectIdV6,
} from "./electron-v6-baseline-defect.mjs";

const performanceDirectory = dirname(fileURLToPath(import.meta.url));
const workloadPath = resolve(
  performanceDirectory,
  "comparison-workload-v6.json",
);
const implementations = Object.freeze(["electron", "gpui"]);
const outputLimitBytes = 1_000_000;
const execFileAsync = promisify(execFile);

export const qualificationReceiptTypeV6 =
  "bp-perf-v6-paid-gpu-qualification-v1";
export const qualificationJourneyV6 = "small-shell-open";
export const qualificationComponentV6 = "open-pdf";
export const qualificationTaskLimitMaximumMsV6 = 8 * 60_000;
export const qualificationLeaseTtlMaximumMsV6 = 30 * 60_000;
export const runnerTerminationGraceMsV6 = 2_000;

export const calibrationPairCountV6 = 6;
export const finalPairCountV6 = 24;
export const benefitLaunchCountV6 = 600;
export const semanticCorrectnessLaunchCountV6 = 22;
export const propertyCorrectnessLaunchCountV6 = 2;
export const totalLaunchCountV6 = 624;
export const expectedWorkloadByteSha256V6 =
  "fc7e3cb6f09b74e004a24b01a9f5ccbb444d98feb9ae6489885e27329a442147";

export function parseV6Arguments(argv) {
  for (const frozenOption of ["--calibration-pairs", "--final-pairs"]) {
    if (argv.includes(frozenOption)) {
      throw new Error(`${frozenOption} is frozen by the v6 workload`);
    }
  }
  const resume = argv.includes("--resume");
  const qualify = argv.includes("--qualify");
  let qualificationReceipt;
  const inheritedArguments = [];
  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index];
    if (option === "--resume" || option === "--qualify") continue;
    if (option === "--qualification-receipt") {
      const value = argv[++index];
      if (value === undefined || value.startsWith("--")) {
        throw new Error("--qualification-receipt requires a value");
      }
      qualificationReceipt = resolve(value);
      continue;
    }
    inheritedArguments.push(option);
  }
  const options = parseV5Arguments(inheritedArguments);
  if (options.help) return options;
  if (qualify && options.mode === "execute") {
    throw new Error("--qualify and --execute are mutually exclusive");
  }
  const mode = qualify ? "qualify" : options.mode;
  qualificationReceipt ??= resolve(
    options.output,
    "qualification-receipt-v6.json",
  );
  if (mode === "execute" && !argv.includes("--qualification-receipt")) {
    throw new Error(
      "--execute requires --qualification-receipt from a passed paid-GPU qualification",
    );
  }
  if (mode === "qualify") {
    if (!Number.isInteger(options.taskLimitMs)) {
      throw new Error(
        "--qualify requires --task-limit-ms from a reviewed lease plan",
      );
    }
    if (!Number.isInteger(options.leaseTtlMs)) {
      throw new Error(
        "--qualify requires --lease-ttl-ms from a reviewed lease plan",
      );
    }
    if (options.taskLimitMs > qualificationTaskLimitMaximumMsV6) {
      throw new Error(
        "--qualify task limit exceeds the frozen 8-minute maximum",
      );
    }
    if (options.leaseTtlMs > qualificationLeaseTtlMaximumMsV6) {
      throw new Error(
        "--qualify lease TTL exceeds the frozen 30-minute maximum",
      );
    }
    if (options.leaseTtlMs < options.taskLimitMs + options.cleanupGraceMs) {
      throw new Error(
        "qualification lease TTL must include the task limit and cleanup grace",
      );
    }
  }
  return {
    ...options,
    calibrationPairs: calibrationPairCountV6,
    finalPairs: finalPairCountV6,
    seed: argv.includes("--seed") ? options.seed : 0x4250_5636,
    resume,
    mode,
    qualificationReceipt,
  };
}

export async function loadComparisonWorkloadV6(path = workloadPath) {
  const bytes = await readFile(path);
  return {
    workload: JSON.parse(bytes),
    byte_sha256: createHash("sha256").update(bytes).digest("hex"),
  };
}

export function validateComparisonWorkloadV6(workload, byteSha256) {
  const failures = [...validateScenarioContractV6()];
  if (workload?.manifest_id !== "bp-perf-v6-decision-2") {
    failures.push("v6 manifest ID is not exact");
  }
  if (workload?.protocol_version !== protocolVersionV6) {
    failures.push("v6 protocol version is not exact");
  }
  if (byteSha256 !== expectedWorkloadByteSha256V6) {
    failures.push("v6 workload byte SHA-256 changed");
  }
  const exact = (left, right) => JSON.stringify(left) === JSON.stringify(right);
  if (
    !exact(workload?.common_benefit_timing_boundary, {
      schema_version: 2,
      boundary_id: "x11-damage-notify-after-xtest-v1",
      observer: "native-x11-damage-observer-v1",
      input_clock: "CLOCK_MONOTONIC",
      completion_clock: "CLOCK_MONOTONIC",
      completion_signal: "X11-DamageNotify",
      observation_scope:
        "x11-server-drawable-damage-not-presentation-completion",
      server_observed_drawable_damage: true,
      presentation_completion_observed: false,
      observer_process_independent: true,
      physical_scanout_observed: false,
    })
  ) {
    failures.push("v6 common X11 Damage boundary is not exact");
  }
  if (
    !exact(workload?.benefit_eligible_components, benefitEligibleComponentIdsV6)
  ) {
    failures.push("v6 benefit component list is not exact");
  }
  if (
    !exact(
      workload?.semantic_correctness_only_components,
      semanticCorrectnessOnlyComponentIdsV6,
    )
  ) {
    failures.push("v6 semantic correctness list is not exact");
  }
  if (
    workload?.property_correctness_only_component !==
    propertyCorrectnessOnlyComponentIdV6
  ) {
    failures.push("v6 property correctness component is not exact");
  }
  if (
    workload?.schedule?.benefit_launches !== benefitLaunchCountV6 ||
    workload?.schedule?.semantic_correctness_launches !==
      semanticCorrectnessLaunchCountV6 ||
    workload?.schedule?.property_correctness_launches !==
      propertyCorrectnessLaunchCountV6 ||
    workload?.schedule?.total_launches !== totalLaunchCountV6
  ) {
    failures.push("v6 launch counts are not exact");
  }
  return failures;
}

export function buildV6ComparisonPlan(workload, byteSha256) {
  const blockers = validateComparisonWorkloadV6(workload, byteSha256);
  const journeys = Object.entries(representativeScenarioDefinitionsV6).map(
    ([scenario, definition]) => ({
      scenario,
      journey_id: definition.journey_id,
      fixture_ids: [...definition.fixture_ids],
      benefit_components: [...definition.benefit_components],
      correctness_only_components: [...definition.correctness_only_components],
      benefit_component_weights: [...definition.benefit_component_weights],
    }),
  );
  return {
    protocol_version: protocolVersionV6,
    scenario_contract_version: scenarioContractVersionV6,
    manifest_id: workload?.manifest_id ?? null,
    workload_byte_sha256: byteSha256,
    ready: blockers.length === 0,
    journeys,
    blockers,
  };
}

function correctnessRuns(plan, seed) {
  const components = plan.journeys.flatMap((journey) =>
    journey.correctness_only_components.map((component) => ({
      journey: journey.scenario,
      journey_id: journey.journey_id,
      component,
      fixture_ids: [...journey.fixture_ids],
      input_lane:
        component === propertyCorrectnessOnlyComponentIdV6
          ? "native-x11-xtest"
          : "semantic-diagnostic",
      hard_component: component === propertyCorrectnessOnlyComponentIdV6,
    })),
  );
  if (components.length !== 12) {
    throw new Error(`v6 correctness component count is ${components.length}`);
  }
  const orders = createBalancedPairOrders({
    pairCount: components.length,
    seed: seed ^ 0x4336,
  });
  return components.flatMap((component, index) =>
    orders[index].map((implementation, position) => ({
      phase: "correctness",
      inference_eligible: false,
      benefit_metrics_eligible: false,
      correctness_index: index,
      pair: 0,
      pair_position: position === 0 ? "first" : "second",
      implementation,
      bundle_id: null,
      component_index: 0,
      component_weight: 0,
      ...component,
    })),
  );
}

function benefitRuns(plan, seed) {
  const phases = [
    {
      phase: "calibration",
      inferenceEligible: false,
      orders: createCalibrationPairOrdersV5(seed ^ 0x5636),
    },
    {
      phase: "final",
      inferenceEligible: true,
      orders: createBalancedPairOrders({ pairCount: finalPairCountV6, seed }),
    },
  ];
  return phases.flatMap(({ phase, inferenceEligible, orders }) =>
    plan.journeys.flatMap((journey) =>
      orders.flatMap((order, pairIndex) =>
        order.flatMap((implementation, position) => {
          const pair = pairIndex + 1;
          const bundleId = `${phase}-${journey.scenario}-pair${pair}-${implementation}`;
          return journey.benefit_components.map((component, index) => ({
            phase,
            inference_eligible: inferenceEligible,
            benefit_metrics_eligible: true,
            journey: journey.scenario,
            journey_id: journey.journey_id,
            pair,
            pair_position: position === 0 ? "first" : "second",
            implementation,
            bundle_id: bundleId,
            component,
            component_index: index,
            component_weight: journey.benefit_component_weights[index],
            hard_component: [
              "viewer-dynamic-fidelity",
              "native-snap-transform-120hz",
              "multi-document-session",
            ].includes(component),
            input_lane: "native-x11-xtest",
            fixture_ids: [...journey.fixture_ids],
          }));
        }),
      ),
    ),
  );
}

export function buildV6ExecutionSchedule(plan, { seed = 0x4250_5636 } = {}) {
  if (plan?.ready !== true) throw new Error("BLOCKED invalid v6 plan");
  const schedule = [
    ...correctnessRuns(plan, seed),
    ...benefitRuns(plan, seed),
  ].map((run, scheduleIndex) => ({ ...run, schedule_index: scheduleIndex }));
  if (schedule.length !== totalLaunchCountV6) {
    throw new Error(`v6 schedule has ${schedule.length} launches`);
  }
  return schedule;
}

export function v6NativeObserverIntegrationPreflight({
  requireRuntimeEnvironment = false,
  environment = process.env,
  qualificationAuthenticated = false,
} = {}) {
  const blockers = [];
  if (
    x11DamageObserverIntegrationV6.ready !== true ||
    x11DamageObserverIntegrationV6.activation_environment !==
      "BP_PERF_COMMON_DAMAGE_OBSERVER=1" ||
    x11DamageObserverIntegrationV6.raw_evidence_path !==
      "iterations[0].native_input.evidence.common_benefit_timing_boundary" ||
    JSON.stringify(x11DamageObserverIntegrationV6.implementations) !==
      JSON.stringify(implementations) ||
    JSON.stringify(x11DamageObserverIntegrationV6.benefit_components) !==
      JSON.stringify(benefitEligibleComponentIdsV6)
  ) {
    blockers.push("X11 Damage observer integration is not exact");
  }
  if (!qualificationAuthenticated) {
    blockers.push(
      "an authenticated paid-GPU qualification receipt is required before the 624-launch executor",
    );
  }
  if (requireRuntimeEnvironment) {
    if (!environment.DISPLAY) blockers.push("DISPLAY is required");
    if (!environment.DBUS_SESSION_BUS_ADDRESS) {
      blockers.push("one shared D-Bus desktop session is required");
    }
  }
  return {
    ready: blockers.length === 0,
    blockers,
  };
}

export function buildV6QualificationRuns(plan) {
  if (plan?.ready !== true) throw new Error("BLOCKED invalid v6 plan");
  const journey = plan.journeys.find(
    ({ scenario }) => scenario === qualificationJourneyV6,
  );
  if (
    !journey ||
    journey.benefit_components.length !== 1 ||
    journey.benefit_components[0] !== qualificationComponentV6
  ) {
    throw new Error("BLOCKED v6 qualification journey is not exact");
  }
  return implementations.map((implementation, index) => ({
    phase: "qualification",
    inference_eligible: false,
    benefit_metrics_eligible: true,
    journey: qualificationJourneyV6,
    journey_id: journey.journey_id,
    pair: 0,
    pair_position: index === 0 ? "first" : "second",
    implementation,
    bundle_id: "qualification-small-shell-open-pair0",
    component: qualificationComponentV6,
    component_index: 0,
    component_weight: 1,
    hard_component: false,
    input_lane: "native-x11-xtest",
    fixture_ids: [...journey.fixture_ids],
    schedule_index: index,
  }));
}

export function buildV6MainSchedule(plan, options) {
  return options.mode === "qualify"
    ? buildV6QualificationRuns(plan)
    : buildV6ExecutionSchedule(plan, { seed: options.seed });
}

export function buildRunnerInvocationV6(run, options) {
  const invocation = buildRunnerInvocationV5(run, {
    timeoutMs: 120_000,
    ...options,
  });
  if (run.phase === "qualification") {
    invocation.identity = `${invocation.identity}-${run.implementation}`;
    invocation.raw_report_path = resolve(
      options.output,
      `${invocation.identity}.json`,
    );
    const outputIndex = invocation.argv.indexOf("--output");
    if (outputIndex < 0 || outputIndex + 1 >= invocation.argv.length) {
      throw new Error("qualification runner invocation has no output path");
    }
    invocation.argv[outputIndex + 1] = invocation.raw_report_path;
  }
  if (run.benefit_metrics_eligible === true && run.hard_component !== true) {
    invocation.argv.push("--v6-scenario", run.journey);
    invocation.command = invocation.argv
      .map((value) => JSON.stringify(value))
      .join(" ");
  } else if (run.phase === "qualification") {
    invocation.command = invocation.argv
      .map((value) => JSON.stringify(value))
      .join(" ");
  }
  if (run.benefit_metrics_eligible === true) {
    invocation.environment.BP_PERF_COMMON_DAMAGE_OBSERVER = "1";
  }
  return invocation;
}

export function buildV6DryRun(plan, schedule, options) {
  const v5WorkloadOptions = {
    timeoutMs: 120_000,
    ...options,
  };
  const launches = schedule.map((run) =>
    buildRunnerInvocationV6(run, v5WorkloadOptions),
  );
  const dryRun = {
    schema_version: 1,
    mode: options.mode ?? "plan",
    plan,
    schedule_summary: {
      total_launches: schedule.length,
      benefit_launches: schedule.filter(
        ({ benefit_metrics_eligible }) => benefit_metrics_eligible,
      ).length,
      semantic_correctness_launches: schedule.filter(
        ({ phase, input_lane }) =>
          phase === "correctness" && input_lane === "semantic-diagnostic",
      ).length,
      property_correctness_launches: schedule.filter(
        ({ component }) => component === propertyCorrectnessOnlyComponentIdV6,
      ).length,
    },
    execution_preflight: v6NativeObserverIntegrationPreflight(),
    launches,
  };
  if (
    Number.isFinite(options.hourlyUsd) &&
    options.expectedDurations instanceof Map
  ) {
    dryRun.lease = estimateV5PaidLease(schedule, options);
  }
  return dryRun;
}

export function buildImmutableLeaseBindingV6({
  mode,
  startedAtMs,
  lease,
  options,
}) {
  if (
    !["qualify", "execute"].includes(mode) ||
    !Number.isFinite(startedAtMs) ||
    !Number.isInteger(options?.taskLimitMs) ||
    !Number.isInteger(options?.leaseTtlMs) ||
    !lease
  ) {
    throw new Error("immutable lease binding inputs are incomplete");
  }
  const payload = {
    schema_version: 1,
    mode,
    started_at: new Date(startedAtMs).toISOString(),
    absolute_task_deadline_at: new Date(
      startedAtMs + options.taskLimitMs,
    ).toISOString(),
    absolute_lease_expiry_at: new Date(
      startedAtMs + options.leaseTtlMs,
    ).toISOString(),
    settings: {
      task_limit_ms: options.taskLimitMs,
      cleanup_grace_ms: options.cleanupGraceMs,
      absolute_lease_ttl_ms: options.leaseTtlMs,
      hourly_price_usd: options.hourlyUsd,
      maximum_cost_usd: lease.maximum_cost_usd,
      launch_count: lease.launch_count,
    },
  };
  return { ...payload, lease_digest_sha256: canonicalSha256(payload) };
}

export function validateImmutableLeaseBindingV6(binding, { mode, options }) {
  const { lease_digest_sha256: digest, ...payload } = binding ?? {};
  if (
    digest !== canonicalSha256(payload) ||
    payload.mode !== mode ||
    payload.settings?.task_limit_ms !== options.taskLimitMs ||
    payload.settings?.cleanup_grace_ms !== options.cleanupGraceMs ||
    payload.settings?.absolute_lease_ttl_ms !== options.leaseTtlMs ||
    payload.settings?.hourly_price_usd !== options.hourlyUsd ||
    Date.parse(payload.absolute_task_deadline_at) !==
      Date.parse(payload.started_at) + options.taskLimitMs ||
    Date.parse(payload.absolute_lease_expiry_at) !==
      Date.parse(payload.started_at) + options.leaseTtlMs
  ) {
    throw new Error("immutable lease binding is invalid or changed");
  }
  return binding;
}

function delay(milliseconds) {
  return new Promise((resolvePromise) =>
    setTimeout(resolvePromise, milliseconds),
  );
}

async function sha256File(path) {
  return createHash("sha256")
    .update(await readFile(path))
    .digest("hex");
}

async function fileArtifact(path) {
  return { path, sha256: await sha256File(path) };
}

function sha256Text(value) {
  return createHash("sha256").update(String(value)).digest("hex");
}

export function parseNvidiaAdapterIdentityV6(line, { withIndex = false } = {}) {
  const rows = String(line ?? "")
    .trim()
    .split("\n")
    .filter(Boolean);
  if (rows.length !== 1) {
    throw new Error("paid-GPU qualification requires exactly one NVIDIA GPU");
  }
  const fields = rows[0].split(",").map((value) => value.trim());
  const expected = withIndex ? 5 : 4;
  if (fields.length !== expected) {
    throw new Error("NVIDIA adapter identity has an unexpected field count");
  }
  const [index, name, uuid, driverVersion, memoryTotalMib] = withIndex
    ? fields
    : [null, ...fields];
  if (
    (withIndex && !/^\d+$/.test(index)) ||
    name.length === 0 ||
    !/^GPU-[A-Za-z0-9-]+$/.test(uuid) ||
    driverVersion.length === 0 ||
    !/^\d+(?:\.\d+)?$/.test(memoryTotalMib)
  ) {
    throw new Error("NVIDIA adapter identity is invalid");
  }
  return {
    index: withIndex ? Number(index) : null,
    name,
    uuid,
    driver_version: driverVersion,
    memory_total_mib: Number(memoryTotalMib),
  };
}

function sampleGpuIndexes(report) {
  const samples = ["baseline", "run", "baseline_adjusted"].flatMap(
    (phase) => report?.iterations?.[0]?.gpu?.[phase]?.samples ?? [],
  );
  const indexes = new Set(samples.map(({ index }) => index));
  if (
    samples.length === 0 ||
    indexes.size !== 1 ||
    !Number.isInteger([...indexes][0])
  ) {
    throw new Error("NVIDIA samples do not bind to exactly one adapter index");
  }
  return [...indexes][0];
}

function electronUsesNvidiaAdapter(report) {
  const info = report?.iterations?.[0]?.renderer?.browser_gpu_info;
  const devices = info?.gpu?.devices ?? [];
  return (
    devices.some(
      (device) =>
        device?.vendorId === 0x10de ||
        String(device?.vendorId).toLowerCase() === "0x10de" ||
        /nvidia/i.test(
          `${device?.vendorString ?? ""} ${device?.deviceString ?? ""}`,
        ),
    ) ||
    /nvidia/i.test(
      `${info?.gpu?.auxAttributes?.glRenderer ?? ""} ${info?.gpu?.auxAttributes?.glVendor ?? ""}`,
    )
  );
}

function vulkanUsesNvidiaAdapter(summary) {
  return (
    /vendorID\s*=\s*0x10de/i.test(summary ?? "") &&
    /deviceName\s*=\s*.*NVIDIA/i.test(summary ?? "") &&
    !/deviceName\s*=\s*llvmpipe/i.test(summary ?? "")
  );
}

function requireActiveRendererAdapterV6(report, implementation, deviceUuid) {
  const activeAdapter = report?.iterations?.[0]?.active_gpu_adapter;
  if (
    activeAdapter?.receipt_type !== "bp-active-renderer-adapter-v1" ||
    activeAdapter?.passed !== true ||
    activeAdapter?.implementation !== implementation ||
    activeAdapter?.device_uuid !== deviceUuid ||
    activeAdapter?.selection_source !==
      (implementation === "electron"
        ? "chromium-system-info-active-gl-renderer"
        : "gpui-window-gpu-specs")
  ) {
    throw new Error(
      `${implementation} active renderer adapter is not the qualified NVIDIA UUID`,
    );
  }
  return activeAdapter;
}

async function evidenceCommandV6(execute, environment, executable, args) {
  const { stdout } = await execute(executable, args, {
    encoding: "utf8",
    timeout: 10_000,
    maxBuffer: 1_000_000,
    env: environment,
  });
  if (!stdout.trim()) throw new Error(`${executable} returned no evidence`);
  return stdout.trim();
}

function x11SocketPath(display) {
  const match = String(display ?? "").match(/^:(\d+)(?:\.\d+)?$/);
  if (!match) {
    throw new Error("paid-GPU qualification requires a local X11 DISPLAY");
  }
  return `/tmp/.X11-unix/X${match[1]}`;
}

export function parseDbusBusIdV6(output) {
  const match = String(output ?? "").match(/string\s+"([A-Za-z0-9-]+)"/);
  if (!match) throw new Error("D-Bus GetId returned no bus ID");
  return match[1];
}

export async function collectSessionIdentityV6({
  environment = process.env,
  execute = execFileAsync,
} = {}) {
  if (!environment.DISPLAY || !environment.DBUS_SESSION_BUS_ADDRESS) {
    throw new Error("X11 and shared D-Bus session identity are required");
  }
  const socketPath = x11SocketPath(environment.DISPLAY);
  const [pidOutput, dbusOutput] = await Promise.all([
    evidenceCommandV6(execute, environment, "fuser", [socketPath]),
    evidenceCommandV6(execute, environment, "dbus-send", [
      "--session",
      "--dest=org.freedesktop.DBus",
      "--type=method_call",
      "--print-reply",
      "/",
      "org.freedesktop.DBus.GetId",
    ]),
  ]);
  const pids = pidOutput.match(/\b\d+\b/g)?.map(Number) ?? [];
  if (pids.length !== 1 || pids[0] < 1) {
    throw new Error("X11 socket must have exactly one server process owner");
  }
  const pid = pids[0];
  const [startedAt, commandLine] = await Promise.all([
    evidenceCommandV6(execute, environment, "ps", [
      "-o",
      "lstart=",
      "-p",
      String(pid),
    ]),
    evidenceCommandV6(execute, environment, "ps", [
      "-o",
      "args=",
      "-p",
      String(pid),
    ]),
  ]);
  return {
    x_server: {
      pid,
      started_at: startedAt,
      command_sha256: sha256Text(commandLine),
      socket_path: socketPath,
    },
    dbus_bus_id: parseDbusBusIdV6(dbusOutput),
  };
}

export function assertSessionIdentityV6(expected, actual) {
  if (JSON.stringify(expected) !== JSON.stringify(actual)) {
    throw new Error("BLOCKED X server or D-Bus session identity changed");
  }
  return true;
}

export async function collectPaidGpuEnvironmentV6({
  environment = process.env,
  execute = execFileAsync,
} = {}) {
  if (!environment.DISPLAY || !environment.DBUS_SESSION_BUS_ADDRESS) {
    throw new Error(
      "paid-GPU qualification requires DISPLAY and one shared D-Bus session",
    );
  }
  const [nvidiaIdentity, vulkanSummary, displayMode, sessionIdentity] =
    await Promise.all([
      evidenceCommandV6(execute, environment, "nvidia-smi", [
        "--query-gpu=index,name,uuid,driver_version,memory.total",
        "--format=csv,noheader,nounits",
      ]),
      evidenceCommandV6(execute, environment, "vulkaninfo", ["--summary"]),
      evidenceCommandV6(execute, environment, "xrandr", ["--current"]),
      collectSessionIdentityV6({ environment, execute }),
    ]);
  const adapter = parseNvidiaAdapterIdentityV6(nvidiaIdentity, {
    withIndex: true,
  });
  if (!vulkanUsesNvidiaAdapter(vulkanSummary)) {
    throw new Error("the X11/Vulkan environment is not bound to NVIDIA");
  }
  return {
    hostname: hostname(),
    os_release: release(),
    display: environment.DISPLAY,
    dbus_session_address_sha256: sha256Text(
      environment.DBUS_SESSION_BUS_ADDRESS,
    ),
    adapter,
    vulkan_summary_sha256: sha256Text(vulkanSummary),
    display_mode_sha256: sha256Text(displayMode),
    session_identity: sessionIdentity,
    raw: { vulkan_summary: vulkanSummary, display_mode: displayMode },
  };
}

export function buildQualificationEnvironmentBindingV6({
  environmentEvidence,
  electronReport,
  gpuiReport,
}) {
  const reports = { electron: electronReport, gpui: gpuiReport };
  const expectedAdapter = environmentEvidence?.adapter;
  for (const [implementation, report] of Object.entries(reports)) {
    const host = report?.provenance?.host;
    if (
      host?.hostname !== environmentEvidence.hostname ||
      host?.os_release !== environmentEvidence.os_release
    ) {
      throw new Error(`${implementation} report came from another host`);
    }
    const reportedAdapter = parseNvidiaAdapterIdentityV6(host?.nvidia_gpu);
    for (const field of [
      "name",
      "uuid",
      "driver_version",
      "memory_total_mib",
    ]) {
      if (reportedAdapter[field] !== expectedAdapter[field]) {
        throw new Error(
          `${implementation} report NVIDIA ${field} differs from qualification host`,
        );
      }
    }
    if (
      sha256Text(host?.vulkan_summary) !==
        environmentEvidence.vulkan_summary_sha256 ||
      sha256Text(host?.display_mode) !== environmentEvidence.display_mode_sha256
    ) {
      throw new Error(`${implementation} graphics environment differs`);
    }
    if (sampleGpuIndexes(report) !== expectedAdapter.index) {
      throw new Error(
        `${implementation} NVIDIA samples came from another adapter`,
      );
    }
    requireActiveRendererAdapterV6(
      report,
      implementation,
      expectedAdapter.uuid,
    );
  }
  if (!vulkanUsesNvidiaAdapter(gpuiReport?.provenance?.host?.vulkan_summary)) {
    throw new Error("GPUI Vulkan provenance is not NVIDIA-backed");
  }
  if (!electronUsesNvidiaAdapter(electronReport)) {
    throw new Error("Electron Chromium GPU device is not NVIDIA-backed");
  }
  const { raw: _raw, ...binding } = environmentEvidence;
  return binding;
}

function exactArtifactMap(artifacts) {
  return Object.fromEntries(
    Object.entries(artifacts).map(([id, artifact]) => [id, artifact.sha256]),
  );
}

function requireCgroupAccountingPreflightV6(receipt) {
  if (
    receipt?.preflight_id !== "bp-linux-cgroup-v2-accounting-v1" ||
    receipt?.ready !== true ||
    receipt?.accounting_scope !== "cgroup-v2-child-process-tree" ||
    receipt?.substitution_policy !== "no-rss-substitution" ||
    JSON.stringify(receipt?.required_metrics) !==
      JSON.stringify(["cpu.stat:usage_usec", "memory.peak"]) ||
    !Array.isArray(receipt?.blockers) ||
    receipt.blockers.length !== 0 ||
    receipt?.probe?.passed !== true ||
    !Number.isFinite(receipt?.metrics?.cpu_seconds) ||
    receipt.metrics.cpu_seconds <= 0 ||
    receipt?.metrics?.memory_peak_supported !== true ||
    !Number.isFinite(receipt?.metrics?.memory_peak_bytes) ||
    receipt.metrics.memory_peak_bytes <= 0 ||
    receipt?.cleanup?.removed !== true
  ) {
    throw new Error(
      `BLOCKED required cgroup v2 accounting preflight failed: ${JSON.stringify(receipt)}`,
    );
  }
  return receipt;
}

function candidateValidationOptionsV6(options) {
  return {
    electronManifestPath: options.electronCandidateArtifact,
    gpuiManifestPath: options.gpuiCandidateArtifact,
    electronExecutable: options.electron,
    gpuiBinary: options.gpuiBinary,
  };
}

export function validateCandidateLaunchSealV6({ seal, launchId, candidates }) {
  const { evidence_sha256: evidenceSha256, ...payload } = seal ?? {};
  if (
    payload.launch_id !== launchId ||
    payload.revalidated_immediately_before_launch !== true ||
    !Number.isFinite(payload.started_monotonic_ms) ||
    !Number.isFinite(payload.ended_monotonic_ms) ||
    payload.ended_monotonic_ms < payload.started_monotonic_ms ||
    payload.candidate_profile !== candidates.electron.candidate_profile ||
    payload.candidate_profile !== candidates.gpui.candidate_profile ||
    payload.electron_manifest_sha256 !== candidates.electron.sha256 ||
    payload.electron_executable_sha256 !==
      candidates.electron.executable.sha256 ||
    payload.electron_bundle_tree_sha256 !==
      candidates.electron.bundle_tree_sha256 ||
    payload.electron_runtime_dependency_closure_tree_sha256 !==
      candidates.electron.runtime_dependency_closure_tree_sha256 ||
    payload.gpui_manifest_sha256 !== candidates.gpui.sha256 ||
    payload.gpui_executable_sha256 !== candidates.gpui.executable.sha256 ||
    payload.gpui_pdf_worker_sha256 !== candidates.gpui.pdf_worker.sha256 ||
    evidenceSha256 !== canonicalSha256(payload)
  ) {
    throw new Error(`candidate launch seal is invalid: ${launchId}`);
  }
  return seal;
}

async function sealCandidateLaunchV6(invocation, options, candidates) {
  return validateCandidateLaunchSealV6({
    seal: await revalidateOptimizedCandidateLaunchV4({
      launchId: invocation.identity,
      ...candidateValidationOptionsV6(options),
    }),
    launchId: invocation.identity,
    candidates,
  });
}

export function buildQualificationReceiptV6({
  workload,
  candidates,
  verified,
  environmentBinding,
  launches,
  matchedViewState,
  leaseBinding,
  cgroupAccountingPreflight,
  createdAt = new Date().toISOString(),
}) {
  if (
    launches?.length !== 2 ||
    launches[0]?.implementation !== "electron" ||
    launches[1]?.implementation !== "gpui" ||
    launches.some(
      (launch) =>
        launch.passed !== true ||
        launch.benefit_metrics_eligible !== true ||
        !commonX11DamageTimingBoundaryPassedV6(
          launch.common_benefit_timing_boundary,
        ),
    ) ||
    matchedViewState?.passed !== true
  ) {
    throw new Error("paid-GPU qualification evidence did not pass exactly");
  }
  const payload = {
    schema_version: 1,
    receipt_type: qualificationReceiptTypeV6,
    status: "passed",
    protocol_version: protocolVersionV6,
    scenario_contract_version: scenarioContractVersionV6,
    manifest_id: workload.manifest_id,
    workload_byte_sha256: expectedWorkloadByteSha256V6,
    source_v5: structuredClone(workload.source_v5),
    created_at: createdAt,
    qualification_pair: {
      journey: qualificationJourneyV6,
      component: qualificationComponentV6,
      launch_count: 2,
      implementation_order: [...implementations],
    },
    candidate_manifest_sha256: Object.fromEntries(
      implementations.map((implementation) => [
        implementation,
        candidates[implementation].sha256,
      ]),
    ),
    fixture_sha256_by_id: exactArtifactMap(verified.fixtures),
    reference_crop_sha256_by_id: exactArtifactMap(verified.references),
    lease_binding: structuredClone(leaseBinding),
    cgroup_accounting_preflight: structuredClone(cgroupAccountingPreflight),
    environment_binding: structuredClone(environmentBinding),
    launches: launches.map((launch) => structuredClone(launch)),
    matched_view_state_receipt: structuredClone(matchedViewState),
  };
  return {
    ...payload,
    authentication: {
      algorithm: "SHA-256-canonical-JSON",
      payload_sha256: canonicalSha256(payload),
    },
  };
}

function assertExactHashMap(label, actual, expected) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`qualification receipt ${label} differs`);
  }
}

export function validateQualificationReceiptV6({
  receipt,
  workload,
  candidates,
  verified,
  environmentBinding,
}) {
  const { authentication, ...payload } = receipt ?? {};
  if (
    authentication?.algorithm !== "SHA-256-canonical-JSON" ||
    authentication?.payload_sha256 !== canonicalSha256(payload)
  ) {
    throw new Error("qualification receipt authentication is invalid");
  }
  const qualificationLeaseOptions = {
    taskLimitMs: payload.lease_binding?.settings?.task_limit_ms,
    cleanupGraceMs: payload.lease_binding?.settings?.cleanup_grace_ms,
    leaseTtlMs: payload.lease_binding?.settings?.absolute_lease_ttl_ms,
    hourlyUsd: payload.lease_binding?.settings?.hourly_price_usd,
  };
  validateImmutableLeaseBindingV6(payload.lease_binding, {
    mode: "qualify",
    options: qualificationLeaseOptions,
  });
  requireCgroupAccountingPreflightV6(payload.cgroup_accounting_preflight);
  if (
    payload.receipt_type !== qualificationReceiptTypeV6 ||
    payload.status !== "passed" ||
    payload.protocol_version !== protocolVersionV6 ||
    payload.scenario_contract_version !== scenarioContractVersionV6 ||
    payload.manifest_id !== workload.manifest_id ||
    payload.workload_byte_sha256 !== expectedWorkloadByteSha256V6 ||
    JSON.stringify(payload.source_v5) !== JSON.stringify(workload.source_v5)
  ) {
    throw new Error("qualification receipt workload identity is invalid");
  }
  assertExactHashMap(
    "candidate identity",
    payload.candidate_manifest_sha256,
    Object.fromEntries(
      implementations.map((implementation) => [
        implementation,
        candidates[implementation].sha256,
      ]),
    ),
  );
  assertExactHashMap(
    "fixture identity",
    payload.fixture_sha256_by_id,
    exactArtifactMap(verified.fixtures),
  );
  assertExactHashMap(
    "reference identity",
    payload.reference_crop_sha256_by_id,
    exactArtifactMap(verified.references),
  );
  if (
    JSON.stringify(payload.environment_binding) !==
    JSON.stringify(environmentBinding)
  ) {
    throw new Error("qualification receipt environment binding differs");
  }
  if (
    payload.qualification_pair?.journey !== qualificationJourneyV6 ||
    payload.qualification_pair?.component !== qualificationComponentV6 ||
    payload.launches?.length !== 2 ||
    payload.launches[0]?.implementation !== "electron" ||
    payload.launches[1]?.implementation !== "gpui" ||
    payload.launches.some(
      (launch) =>
        launch.passed !== true ||
        launch.benefit_metrics_eligible !== true ||
        !commonX11DamageTimingBoundaryPassedV6(
          launch.common_benefit_timing_boundary,
        ),
    ) ||
    payload.matched_view_state_receipt?.passed !== true
  ) {
    throw new Error("qualification receipt evidence is incomplete");
  }
  for (const launch of payload.launches) {
    const implementation = launch.implementation;
    if (
      launch.candidate_manifest_sha256 !== candidates[implementation].sha256 ||
      launch.launch_binding?.candidate_manifest_sha256 !==
        candidates[implementation].sha256 ||
      launch.launch_binding?.protocol_version !== protocolVersionV6 ||
      launch.launch_binding?.workload_byte_sha256 !==
        expectedWorkloadByteSha256V6 ||
      launch.launch_binding?.dbus_session_address_sha256 !==
        environmentBinding.dbus_session_address_sha256 ||
      launch.launch_binding?.session_identity_sha256 !==
        canonicalSha256(environmentBinding.session_identity) ||
      launch.gpu_evidence?.qualification?.required !== true ||
      launch.gpu_evidence?.qualification?.passed !== true ||
      launch.gpu_evidence?.adapter_index !== environmentBinding.adapter.index ||
      launch.active_gpu_adapter?.receipt_type !==
        "bp-active-renderer-adapter-v1" ||
      launch.active_gpu_adapter?.passed !== true ||
      launch.active_gpu_adapter?.implementation !== implementation ||
      launch.active_gpu_adapter?.device_uuid !==
        environmentBinding.adapter.uuid ||
      launch.active_gpu_adapter?.selection_source !==
        (implementation === "electron"
          ? "chromium-system-info-active-gl-renderer"
          : "gpui-window-gpu-specs") ||
      !/^[0-9a-f]{64}$/.test(launch.raw_report_sha256 ?? "")
    ) {
      throw new Error(
        `qualification receipt ${implementation} launch binding is invalid`,
      );
    }
    validateCandidateLaunchSealV6({
      seal: launch.launch_binding.candidate_prelaunch_seal,
      launchId: launch.launch_binding.launch_id,
      candidates,
    });
  }
  const matched = compareViewStateReceiptsV5(
    payload.launches[0].view_state_receipt,
    payload.launches[1].view_state_receipt,
  );
  if (
    matched.passed !== true ||
    matched.evidence_sha256 !==
      payload.matched_view_state_receipt.evidence_sha256
  ) {
    throw new Error("qualification receipt matched view state is invalid");
  }
  return { passed: true, payload_sha256: authentication.payload_sha256 };
}

async function atomicWrite(path, bytes) {
  const temporaryPath = `${path}.tmp-${process.pid}`;
  await writeFile(temporaryPath, bytes);
  await rename(temporaryPath, path);
}

function signalProcessGroup(pid, signal) {
  if (!Number.isInteger(pid) || pid < 1) return false;
  try {
    process.kill(-pid, signal);
    return true;
  } catch (error) {
    if (error?.code === "ESRCH") return false;
    throw error;
  }
}

export async function runRunnerV6(
  invocation,
  timeoutMs,
  {
    terminationGraceMs = runnerTerminationGraceMsV6,
    spawnProcess = spawn,
    terminateGroup = signalProcessGroup,
  } = {},
) {
  await rm(invocation.raw_report_path, { force: true });
  if (invocation.hard_report_path) {
    await rm(invocation.hard_report_path, { force: true });
  }
  const startedAt = new Date().toISOString();
  const startedMonotonicMs = Number(process.hrtime.bigint()) / 1e6;
  let stdout = "";
  let stderr = "";
  const [executable, ...args] = invocation.argv;
  const child = spawnProcess(executable, args, {
    cwd: performanceDirectory,
    env: { ...process.env, ...invocation.environment },
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    if (stdout.length < outputLimitBytes) {
      stdout += chunk.slice(0, outputLimitBytes - stdout.length);
    }
  });
  child.stderr.on("data", (chunk) => {
    if (stderr.length < outputLimitBytes) {
      stderr += chunk.slice(0, outputLimitBytes - stderr.length);
    }
    process.stderr.write(chunk);
  });
  let settled = false;
  let timedOut = false;
  let termSent = false;
  let killSent = false;
  let killTimer;
  let hardDeadlineTimer;
  const outcomePromise = new Promise((resolvePromise) => {
    child.once("error", (error) => {
      settled = true;
      resolvePromise({
        exit_code: null,
        signal: null,
        spawn_error: error.message,
      });
    });
    child.once("close", (code, signal) => {
      settled = true;
      resolvePromise({ exit_code: code, signal, spawn_error: null });
    });
  });
  let resolveHardDeadline;
  const hardDeadlinePromise = new Promise((resolvePromise) => {
    resolveHardDeadline = resolvePromise;
  });
  const timeout = setTimeout(() => {
    timedOut = true;
    termSent = terminateGroup(child.pid, "SIGTERM");
    killTimer = setTimeout(() => {
      if (!settled) killSent = terminateGroup(child.pid, "SIGKILL");
      hardDeadlineTimer = setTimeout(
        () => {
          if (!settled) {
            resolveHardDeadline({
              exit_code: null,
              signal: null,
              spawn_error:
                "runner process group did not exit after bounded SIGTERM/SIGKILL",
            });
          }
        },
        Math.max(25, terminationGraceMs),
      );
    }, terminationGraceMs);
  }, timeoutMs);
  const outcome = await Promise.race([outcomePromise, hardDeadlinePromise]);
  clearTimeout(timeout);
  clearTimeout(killTimer);
  clearTimeout(hardDeadlineTimer);
  if (!settled && child.unref) child.unref();
  let report = null;
  try {
    report = JSON.parse(await readFile(invocation.raw_report_path, "utf8"));
  } catch {
    // The retained launch record reports an absent or invalid raw report.
  }
  return {
    started_at: startedAt,
    ended_at: new Date().toISOString(),
    started_monotonic_ms: startedMonotonicMs,
    ended_monotonic_ms: Number(process.hrtime.bigint()) / 1e6,
    ...outcome,
    timed_out: timedOut,
    termination: {
      process_group: true,
      term_sent: termSent,
      kill_sent: killSent,
      grace_ms: terminationGraceMs,
      bounded_wait_complete: settled,
    },
    stdout,
    stderr,
    report,
  };
}

export function exactElectronEngineeringZoomBaselineDefectV6({
  knownBaselineDefectId,
  retainedReportDefectId,
  retainedIterationDefectId,
  gpuFailures,
  fixtureFailures,
  receiptErrors,
}) {
  return (
    knownBaselineDefectId === electronEngineeringZoomBaselineDefectIdV6 &&
    retainedReportDefectId === electronEngineeringZoomBaselineDefectIdV6 &&
    retainedIterationDefectId === electronEngineeringZoomBaselineDefectIdV6 &&
    JSON.stringify(gpuFailures) === "[]" &&
    JSON.stringify(fixtureFailures) === "[]" &&
    JSON.stringify(receiptErrors) ===
      JSON.stringify([
        "component receipt summary did not pass",
        "engineering:zoom-sequence: command receipt is not live and passed",
        "engineering:zoom-sequence: Electron component execution did not pass",
        "engineering:zoom-sequence: Electron milestone proof is not exact",
      ])
  );
}

export function assessSemanticCorrectnessV6({
  rawReport,
  run,
  v4Workload,
  v4Plan,
}) {
  const gpu = validateGpuSamplesV5(rawReport);
  const journey = v4Plan.journeys.find(
    ({ scenario }) => scenario === run.journey,
  );
  if (!journey) {
    return {
      passed: false,
      failures: [`unknown v4 journey ${run.journey}`],
      receipts: [],
      correctness_passed: false,
    };
  }
  const scenarioContract = buildScenarioContractV4(v4Workload, run.journey);
  const receiptAssessment = validateV4ComponentReport({
    report: rawReport,
    implementation: run.implementation,
    journey: run.journey,
    component: run.component,
    fixture: { sha256: journey.fixture_sha256 },
    scenarioContract,
  });
  const fixtureMismatch =
    scenarioContract.fixture_id === run.fixture_ids[0]
      ? []
      : ["semantic correctness fixture does not match its v6 mapping"];
  const knownBaselineDefectId =
    classifyElectronEngineeringZoomBaselineDefectV6({
      implementation: run.implementation,
      journey: run.journey,
      component: run.component,
      receipts: receiptAssessment.receipts,
      source_command_results:
        rawReport?.iterations?.[0]?.renderer?.expanded_comparison
          ?.command_results,
    });
  const exactKnownBaselineDefect =
    exactElectronEngineeringZoomBaselineDefectV6({
      knownBaselineDefectId,
      retainedReportDefectId:
        rawReport?.v4_parent_execution?.known_baseline_defect_id,
      retainedIterationDefectId:
        rawReport?.iterations?.[0]?.renderer?.v4_parent_component_evidence
          ?.known_baseline_defect_id,
      gpuFailures: gpu.failures,
      fixtureFailures: fixtureMismatch,
      receiptErrors: receiptAssessment.errors,
    });
  const failures = [
    ...gpu.failures,
    ...receiptAssessment.errors,
    ...fixtureMismatch,
  ];
  if (exactKnownBaselineDefect) {
    return {
      passed: true,
      failures: [],
      receipts: receiptAssessment.receipts,
      measurements: {},
      quality_measurements: {},
      correctness_passed: false,
      benefit_metrics_eligible: false,
      known_baseline_defect_id: electronEngineeringZoomBaselineDefectIdV6,
      view_state_receipt: null,
      hard_report: null,
    };
  }
  return {
    passed: failures.length === 0,
    failures,
    receipts: receiptAssessment.receipts,
    measurements: {},
    quality_measurements: {},
    correctness_passed: failures.length === 0,
    benefit_metrics_eligible: false,
    known_baseline_defect_id: null,
    view_state_receipt: null,
    hard_report: null,
  };
}

function assessRunV6({
  rawReport,
  run,
  v4Workload,
  v4Plan,
  v5Workload,
  candidateArtifactSha256,
}) {
  return run.phase === "correctness" && !run.hard_component
    ? assessSemanticCorrectnessV6({ rawReport, run, v4Workload, v4Plan })
    : assessV5Launch({
        workload: v5Workload,
        v4Workload,
        rawReport,
        run,
        candidateArtifactSha256,
        commonBoundaryValidator: commonX11DamageTimingBoundaryPassedV6,
      });
}

export function buildBundleManifestV6({
  runs,
  results,
  journeyPlan,
  candidate,
}) {
  if (
    runs.length !== journeyPlan.benefit_components.length ||
    results.length !== runs.length ||
    results.some(({ passed }) => passed !== true)
  ) {
    throw new Error("v6 benefit bundle has a missing or failed component");
  }
  if (
    runs.map(({ component }) => component).join("\0") !==
    journeyPlan.benefit_components.join("\0")
  ) {
    throw new Error("v6 benefit bundle component order is not exact");
  }
  const expectedCommandIds = journeyPlan.benefit_components.flatMap(
    (component) => journeyPlan.component_command_ids[component],
  );
  const commandIds = results.flatMap(({ receipts }) =>
    receipts.map(({ command_id: commandId }) => commandId),
  );
  if (
    commandIds.length !== expectedCommandIds.length ||
    new Set(commandIds).size !== commandIds.length ||
    [...commandIds].sort().join("\0") !==
      [...expectedCommandIds].sort().join("\0")
  ) {
    throw new Error("v6 benefit bundle command receipts are not exact");
  }
  const first = runs[0];
  return {
    schema_version: 1,
    protocol_version: protocolVersionV6,
    scenario_contract_version: scenarioContractVersionV6,
    workload_byte_sha256: expectedWorkloadByteSha256V6,
    bundle_id: first.bundle_id,
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
      order: [...journeyPlan.benefit_components],
      weights: [...journeyPlan.benefit_component_weights],
      benefit_metric_method: "equal-weight geometric mean",
      non_inferiority_method: "conjunctive every component",
      compensating_regressions_allowed: false,
      correctness_only_components_excluded: true,
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
    manifest_id: "bp-perf-v6-decision-2",
  };
}

function exactRunIdentity(actual, expected) {
  return [
    "schedule_index",
    "phase",
    "inference_eligible",
    "benefit_metrics_eligible",
    "journey",
    "pair",
    "pair_position",
    "implementation",
    "bundle_id",
    "component",
    "component_index",
    "input_lane",
    "hard_component",
  ].every((field) => actual?.[field] === expected?.[field]);
}

export function reusableSchedulePrefixV6(retained, launches) {
  const retainedBundles = new Set(
    (retained?.bundles ?? []).map(({ bundle_id: bundleId }) => bundleId),
  );
  let reusableLaunchCount = 0;
  for (let cursor = 0; cursor < launches.length; ) {
    const first = launches[cursor];
    if (first.phase === "correctness") {
      const prior = retained?.launches?.[cursor];
      const correctness = retained?.correctness_reports?.find(
        (report) =>
          report.implementation === first.implementation &&
          report.journey === first.journey &&
          report.component === first.component,
      );
      if (prior?.passed !== true || correctness?.passed !== true) break;
      reusableLaunchCount = ++cursor;
      continue;
    }
    let groupEnd = cursor;
    while (launches[groupEnd]?.bundle_id === first.bundle_id) groupEnd += 1;
    const groupPassed = Array.from(
      { length: groupEnd - cursor },
      (_, offset) => retained?.launches?.[cursor + offset]?.passed === true,
    ).every(Boolean);
    if (!groupPassed || !retainedBundles.has(first.bundle_id)) break;
    reusableLaunchCount = groupEnd;
    cursor = groupEnd;
  }
  return reusableLaunchCount;
}

async function loadAuthenticatedResumeV6({
  manifestPath,
  expectedManifest,
  launches,
  candidates,
  v4Workload,
  v4Plan,
  v5Workload,
  options,
}) {
  const checksumPath = resolve(dirname(manifestPath), "run-manifest-v6.sha256");
  const checkpointPath = resolve(
    dirname(manifestPath),
    "run-manifest-v6.checkpoint.json",
  );
  let bytes;
  let checksumBytes;
  try {
    const checkpoint = JSON.parse(await readFile(checkpointPath, "utf8"));
    if (checkpoint?.manifest_file !== "run-manifest-v6.json") {
      throw new Error("checkpoint manifest path differs");
    }
    bytes = await readFile(manifestPath);
    const checkpointSha256 = createHash("sha256").update(bytes).digest("hex");
    if (checkpoint?.manifest_sha256 !== checkpointSha256) {
      throw new Error("checkpoint hash differs");
    }
    checksumBytes = `${checkpointSha256}  run-manifest-v6.json\n`;
  } catch {
    [bytes, checksumBytes] = await Promise.all([
      readFile(manifestPath),
      readFile(checksumPath, "utf8"),
    ]);
  }
  const observedManifestSha256 = createHash("sha256")
    .update(bytes)
    .digest("hex");
  if (
    checksumBytes.trim() !== `${observedManifestSha256}  run-manifest-v6.json`
  ) {
    throw new Error("BLOCKED v6 resume manifest checksum is absent or invalid");
  }
  const retained = JSON.parse(bytes);
  validateImmutableLeaseBindingV6(retained?.lease_binding, {
    mode: "execute",
    options,
  });
  requireCgroupAccountingPreflightV6(retained?.cgroup_accounting_preflight);
  for (const field of [
    "protocol_version",
    "scenario_contract_version",
    "manifest_id",
    "workload_byte_sha256",
  ]) {
    if (retained?.[field] !== expectedManifest[field]) {
      throw new Error(`BLOCKED v6 resume ${field} is not exact`);
    }
  }
  if (
    retained?.paid_gpu_qualification?.sha256 !==
      expectedManifest.paid_gpu_qualification.sha256 ||
    retained?.paid_gpu_qualification?.payload_sha256 !==
      expectedManifest.paid_gpu_qualification.payload_sha256
  ) {
    throw new Error("BLOCKED v6 resume qualification receipt differs");
  }
  if (
    retained?.settings?.schedule_seed !==
      expectedManifest.settings.schedule_seed ||
    retained?.settings?.timeout_ms !== expectedManifest.settings.timeout_ms ||
    retained?.settings?.cooldown_ms !== expectedManifest.settings.cooldown_ms
  ) {
    throw new Error(
      "BLOCKED v6 resume settings differ from the frozen invocation",
    );
  }
  for (const implementation of implementations) {
    if (
      retained?.candidates?.[implementation]?.sha256 !==
      candidates[implementation].sha256
    ) {
      throw new Error(`BLOCKED v6 resume ${implementation} candidate differs`);
    }
  }

  const retainedBundles = new Map(
    (retained.bundles ?? []).map((bundle) => [bundle.bundle_id, bundle]),
  );
  const reusableLaunchCount = reusableSchedulePrefixV6(retained, launches);

  const reusableLaunches = (retained.launches ?? []).slice(
    0,
    reusableLaunchCount,
  );
  for (let index = 0; index < reusableLaunches.length; index += 1) {
    const prior = reusableLaunches[index];
    const expected = launches[index];
    if (!exactRunIdentity(prior, expected) || prior.exit_code !== 0) {
      throw new Error(`BLOCKED v6 resume launch ${index} identity is invalid`);
    }
    if (
      prior.raw_report_path !== expected.raw_report_path ||
      (await sha256File(prior.raw_report_path)) !== prior.raw_report_sha256
    ) {
      throw new Error(`BLOCKED v6 resume launch ${index} raw artifact differs`);
    }
    const rawReport = JSON.parse(await readFile(prior.raw_report_path, "utf8"));
    requireActiveRendererAdapterV6(
      rawReport,
      expected.implementation,
      retained.paid_gpu_qualification.environment_binding.adapter.uuid,
    );
    const binding = rawReport?.launch_binding_v6;
    if (
      binding?.protocol_version !== protocolVersionV6 ||
      binding?.workload_byte_sha256 !== expectedWorkloadByteSha256V6 ||
      binding?.schedule_index !== index ||
      binding?.dbus_session_address_sha256 !==
        retained.paid_gpu_qualification.environment_binding
          .dbus_session_address_sha256 ||
      binding?.session_identity_sha256 !==
        canonicalSha256(
          retained.paid_gpu_qualification.environment_binding.session_identity,
        ) ||
      binding?.candidate_manifest_sha256 !==
        candidates[expected.implementation].sha256
    ) {
      throw new Error(`BLOCKED v6 resume launch ${index} binding is invalid`);
    }
    validateCandidateLaunchSealV6({
      seal: binding.candidate_prelaunch_seal,
      launchId: expected.identity,
      candidates,
    });
    const assessment = assessRunV6({
      rawReport,
      run: expected,
      v4Workload,
      v4Plan,
      v5Workload,
      candidateArtifactSha256: candidates[expected.implementation].sha256,
    });
    if (
      assessment.passed !== true ||
      (expected.benefit_metrics_eligible === true &&
        assessment.benefit_metrics_eligible !== true)
    ) {
      throw new Error(`BLOCKED v6 resume launch ${index} no longer validates`);
    }
    if (
      prior.hard_report_path &&
      (await sha256File(prior.hard_report_path)) !== prior.hard_report_sha256
    ) {
      throw new Error(
        `BLOCKED v6 resume launch ${index} hard artifact differs`,
      );
    }
  }

  const reusableBundleIds = new Set(
    launches
      .slice(0, reusableLaunchCount)
      .map(({ bundle_id: bundleId }) => bundleId)
      .filter(Boolean),
  );
  const reusableBundles = (retained.bundles ?? []).filter((bundle) =>
    reusableBundleIds.has(bundle.bundle_id),
  );
  for (const bundle of reusableBundles) {
    if ((await sha256File(bundle.path)) !== bundle.sha256) {
      throw new Error(
        `BLOCKED v6 resume bundle artifact differs: ${bundle.bundle_id}`,
      );
    }
    const parsed = JSON.parse(await readFile(bundle.path, "utf8"));
    const { path: _path, sha256: _sha256, ...payload } = bundle;
    if (JSON.stringify(parsed) !== JSON.stringify(payload)) {
      throw new Error(
        `BLOCKED v6 resume bundle payload differs: ${bundle.bundle_id}`,
      );
    }
  }
  const reusablePairKeys = new Set();
  for (const bundle of reusableBundles) {
    const key = `${bundle.phase}\0${bundle.journey}\0${bundle.pair}`;
    if (
      reusableBundles.filter(
        (candidate) =>
          `${candidate.phase}\0${candidate.journey}\0${candidate.pair}` === key,
      ).length === 2
    ) {
      reusablePairKeys.add(key);
    }
  }
  return {
    ...retained,
    complete: false,
    outcome: "running",
    failure: null,
    resumed_at: new Date().toISOString(),
    resume: {
      authenticated_manifest_sha256: observedManifestSha256,
      reused_launch_count: reusableLaunchCount,
      discarded_partial_or_failed_launch_count: Math.max(
        0,
        (retained.launches?.length ?? 0) - reusableLaunchCount,
      ),
    },
    launches: reusableLaunches,
    bundles: reusableBundles,
    correctness_reports: (retained.correctness_reports ?? []).filter((report) =>
      reusableLaunches.some(
        (launch) =>
          launch.phase === "correctness" &&
          launch.implementation === report.implementation &&
          launch.journey === report.journey &&
          launch.component === report.component,
      ),
    ),
    view_state_pairs: (retained.view_state_pairs ?? []).filter((pair) =>
      reusablePairKeys.has(`${pair.phase}\0${pair.journey}\0${pair.pair}`),
    ),
  };
}

function qualificationEnvironmentBinding(environmentEvidence) {
  const { raw: _raw, ...binding } = environmentEvidence;
  return binding;
}

async function validateV6Inputs(options) {
  const v5Workload = await loadMaterializedComparisonWorkloadV5();
  const v4Workload = await loadMaterializedComparisonWorkloadV4();
  const v4Plan = buildV4ComparisonPlan(v4Workload);
  const verified = await verifyV5FixturesAndReferences(v5Workload, options);
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
  return { v5Workload, v4Workload, v4Plan, verified, candidates };
}

function qualificationGpuEvidence(report) {
  const gpu = report.iterations[0].gpu;
  return {
    qualification: structuredClone(gpu.qualification),
    adapter_index: sampleGpuIndexes(report),
    baseline_sample_count: gpu.baseline.sample_count,
    run_sample_count: gpu.run.sample_count,
    baseline_adjusted_sample_count: gpu.baseline_adjusted.sample_count,
  };
}

export async function executeV6Qualification(workload, plan, dryRun, options) {
  const preflight = v6NativeObserverIntegrationPreflight({
    requireRuntimeEnvironment: true,
    qualificationAuthenticated: true,
  });
  if (preflight.ready !== true) {
    throw new Error(
      `BLOCKED v6 qualification preflight: ${preflight.blockers.join("; ")}`,
    );
  }
  const startedMs = Date.now();
  const leaseBinding = buildImmutableLeaseBindingV6({
    mode: "qualify",
    startedAtMs: startedMs,
    lease: dryRun.lease,
    options,
  });
  const deadline = Date.parse(leaseBinding.absolute_task_deadline_at);
  const cgroupAccountingPreflight = requireCgroupAccountingPreflightV6(
    await preflightRequiredCgroupV2Accounting(),
  );
  const inputs = await validateV6Inputs(options);
  const environmentEvidence = await collectPaidGpuEnvironmentV6();
  const receiptPath = options.qualificationReceipt;
  const checksumPath = `${receiptPath}.sha256`;
  await mkdir(options.output, { recursive: true });
  await mkdir(dirname(receiptPath), { recursive: true });
  for (const path of [receiptPath, checksumPath]) {
    try {
      await readFile(path);
      throw new Error(`BLOCKED qualification output already exists: ${path}`);
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }
  }
  const runs = buildV6QualificationRuns(plan);
  const invocations = runs.map((run) =>
    buildRunnerInvocationV6(run, {
      ...options,
      output: options.output,
      timeoutMs: Math.min(options.timeoutMs, 120_000),
    }),
  );
  const retained = [];
  const reports = {};
  for (const invocation of invocations) {
    assertSessionIdentityV6(
      environmentEvidence.session_identity,
      await collectSessionIdentityV6(),
    );
    const remainingMs = deadline - Date.now();
    if (remainingMs < options.timeoutMs + 15_000) {
      throw new Error("TIMED OUT v6 qualification task limit reached");
    }
    const candidatePrelaunchSeal = await sealCandidateLaunchV6(
      invocation,
      options,
      inputs.candidates,
    );
    const launched = await runRunnerV6(
      invocation,
      Math.min(options.timeoutMs + 15_000, remainingMs),
    );
    if (!launched.report) {
      throw new Error(
        `BLOCKED ${invocation.identity}: raw runner report is absent`,
      );
    }
    requireActiveRendererAdapterV6(
      launched.report,
      invocation.implementation,
      environmentEvidence.adapter.uuid,
    );
    const binding = buildLaunchBindingV5({
      invocation,
      launched,
      candidate: inputs.candidates[invocation.implementation],
      fixtureArtifacts: inputs.verified.fixtures,
    });
    launched.report.qualification_launch_binding_v6 = {
      ...structuredClone(binding),
      protocol_version: protocolVersionV6,
      workload_byte_sha256: expectedWorkloadByteSha256V6,
      dbus_session_address_sha256:
        environmentEvidence.dbus_session_address_sha256,
      session_identity_sha256: canonicalSha256(
        environmentEvidence.session_identity,
      ),
      candidate_prelaunch_seal: candidatePrelaunchSeal,
    };
    await writeFile(
      invocation.raw_report_path,
      `${JSON.stringify(launched.report, null, 2)}\n`,
    );
    const assessment = assessRunV6({
      rawReport: launched.report,
      run: invocation,
      v4Workload: inputs.v4Workload,
      v4Plan: inputs.v4Plan,
      v5Workload: inputs.v5Workload,
      candidateArtifactSha256:
        inputs.candidates[invocation.implementation].sha256,
    });
    const passed =
      launched.exit_code === 0 &&
      launched.timed_out !== true &&
      assessment.passed === true;
    if (!passed || assessment.benefit_metrics_eligible !== true) {
      throw new Error(
        `BLOCKED ${invocation.identity}: ${[
          ...(launched.exit_code === 0
            ? []
            : [`runner exit code ${launched.exit_code}`]),
          ...(launched.timed_out ? ["runner timed out"] : []),
          ...assessment.failures,
        ].join("; ")}`,
      );
    }
    const rawArtifact = await fileArtifact(invocation.raw_report_path);
    const commonBoundary =
      launched.report.iterations[0].native_input.evidence
        .common_benefit_timing_boundary;
    const launch = {
      implementation: invocation.implementation,
      journey: invocation.journey,
      component: invocation.component,
      passed: true,
      benefit_metrics_eligible: true,
      raw_report_path: rawArtifact.path,
      raw_report_sha256: rawArtifact.sha256,
      candidate_manifest_sha256:
        inputs.candidates[invocation.implementation].sha256,
      launch_binding: launched.report.qualification_launch_binding_v6,
      common_benefit_timing_boundary: structuredClone(commonBoundary),
      gpu_evidence: qualificationGpuEvidence(launched.report),
      active_gpu_adapter: structuredClone(
        launched.report.iterations[0].active_gpu_adapter,
      ),
      view_state_receipt: structuredClone(assessment.view_state_receipt),
    };
    retained.push(launch);
    reports[invocation.implementation] = launched.report;
    if (retained.length < invocations.length && options.cooldownMs > 0) {
      await delay(options.cooldownMs);
    }
  }
  const matchedViewState = compareViewStateReceiptsV5(
    retained[0].view_state_receipt,
    retained[1].view_state_receipt,
  );
  if (matchedViewState.passed !== true) {
    throw new Error(
      `BLOCKED paid-GPU qualification view state differs: ${matchedViewState.failures.join("; ")}`,
    );
  }
  const environmentBinding = buildQualificationEnvironmentBindingV6({
    environmentEvidence,
    electronReport: reports.electron,
    gpuiReport: reports.gpui,
  });
  const receipt = buildQualificationReceiptV6({
    workload,
    candidates: inputs.candidates,
    verified: inputs.verified,
    environmentBinding,
    launches: retained,
    matchedViewState,
    leaseBinding,
    cgroupAccountingPreflight,
  });
  const bytes = `${JSON.stringify(receipt, null, 2)}\n`;
  const fileSha256 = sha256Text(bytes);
  await atomicWrite(receiptPath, bytes);
  await atomicWrite(
    checksumPath,
    `${fileSha256}  ${receiptPath.split("/").at(-1)}\n`,
  );
  return {
    receipt,
    path: receiptPath,
    sha256: fileSha256,
    actual_duration_ms: Date.now() - startedMs,
    lease_binding: leaseBinding,
  };
}

async function loadAuthenticatedQualificationV6({
  workload,
  options,
  candidates,
  verified,
  v4Workload,
  v4Plan,
  v5Workload,
}) {
  const receiptPath = options.qualificationReceipt;
  const [bytes, checksum] = await Promise.all([
    readFile(receiptPath),
    readFile(`${receiptPath}.sha256`, "utf8"),
  ]);
  const fileSha256 = createHash("sha256").update(bytes).digest("hex");
  if (checksum.trim() !== `${fileSha256}  ${receiptPath.split("/").at(-1)}`) {
    throw new Error("BLOCKED qualification receipt checksum is invalid");
  }
  const receipt = JSON.parse(bytes);
  const currentEnvironment = await collectPaidGpuEnvironmentV6();
  const validation = validateQualificationReceiptV6({
    receipt,
    workload,
    candidates,
    verified,
    environmentBinding: qualificationEnvironmentBinding(currentEnvironment),
  });
  const qualificationRuns = buildV6QualificationRuns(
    buildV6ComparisonPlan(workload, expectedWorkloadByteSha256V6),
  );
  for (const [index, launch] of receipt.launches.entries()) {
    const rawBytes = await readFile(launch.raw_report_path);
    if (
      createHash("sha256").update(rawBytes).digest("hex") !==
      launch.raw_report_sha256
    ) {
      throw new Error(
        `BLOCKED qualification raw report differs: ${launch.implementation}`,
      );
    }
    const rawReport = JSON.parse(rawBytes);
    const assessment = assessRunV6({
      rawReport,
      run: qualificationRuns[index],
      v4Workload,
      v4Plan,
      v5Workload,
      candidateArtifactSha256: candidates[launch.implementation].sha256,
    });
    if (
      assessment.passed !== true ||
      assessment.benefit_metrics_eligible !== true ||
      JSON.stringify(assessment.view_state_receipt) !==
        JSON.stringify(launch.view_state_receipt) ||
      JSON.stringify(
        rawReport?.iterations?.[0]?.native_input?.evidence
          ?.common_benefit_timing_boundary,
      ) !== JSON.stringify(launch.common_benefit_timing_boundary) ||
      JSON.stringify(rawReport?.qualification_launch_binding_v6) !==
        JSON.stringify(launch.launch_binding)
    ) {
      throw new Error(
        `BLOCKED qualification raw report no longer validates: ${launch.implementation}`,
      );
    }
  }
  return {
    path: receiptPath,
    sha256: fileSha256,
    payload_sha256: validation.payload_sha256,
    environment_binding: receipt.environment_binding,
  };
}

export async function executeV6(workload, dryRun, options) {
  const executionStartedMs = Date.now();
  const { v5Workload, v4Workload, v4Plan, verified, candidates } =
    await validateV6Inputs(options);
  const qualification = await loadAuthenticatedQualificationV6({
    workload,
    options,
    candidates,
    verified,
    v4Workload,
    v4Plan,
    v5Workload,
  });
  const preflight = v6NativeObserverIntegrationPreflight({
    requireRuntimeEnvironment: true,
    qualificationAuthenticated: true,
  });
  if (preflight.ready !== true) {
    throw new Error(
      `BLOCKED v6 execution preflight: ${preflight.blockers.join("; ")}`,
    );
  }
  const cgroupAccountingPreflight = requireCgroupAccountingPreflightV6(
    await preflightRequiredCgroupV2Accounting(),
  );
  await mkdir(options.output, { recursive: true });
  const manifestPath = resolve(options.output, "run-manifest-v6.json");
  const initialLeaseBinding = buildImmutableLeaseBindingV6({
    mode: "execute",
    startedAtMs: executionStartedMs,
    lease: dryRun.lease,
    options,
  });
  let manifest = {
    schema_version: 1,
    protocol_version: protocolVersionV6,
    scenario_contract_version: scenarioContractVersionV6,
    manifest_id: workload.manifest_id,
    workload_byte_sha256: expectedWorkloadByteSha256V6,
    source_v5: structuredClone(workload.source_v5),
    started_at: new Date(executionStartedMs).toISOString(),
    plan: dryRun.plan,
    schedule_summary: dryRun.schedule_summary,
    lease: dryRun.lease,
    lease_binding: initialLeaseBinding,
    candidates,
    fixtures: verified.fixtures,
    references: verified.references,
    paid_gpu_qualification: qualification,
    cgroup_accounting_preflight: cgroupAccountingPreflight,
    settings: {
      schedule_seed: options.seed,
      timeout_ms: options.timeoutMs,
      cooldown_ms: options.cooldownMs,
      task_limit_ms: options.taskLimitMs,
      cleanup_grace_ms: options.cleanupGraceMs,
      absolute_lease_ttl_ms: options.leaseTtlMs,
      hourly_price_usd: options.hourlyUsd,
      shared_dbus_session: true,
      common_damage_observer_required: true,
    },
    launches: [],
    bundles: [],
    view_state_pairs: [],
    correctness_reports: [],
    complete: false,
    outcome: "running",
  };
  const launches = dryRun.launches.map((invocation) => ({ ...invocation }));
  if (options.resume) {
    manifest = await loadAuthenticatedResumeV6({
      manifestPath,
      expectedManifest: manifest,
      launches,
      candidates,
      v4Workload,
      v4Plan,
      v5Workload,
      options,
    });
  } else {
    try {
      await readFile(manifestPath);
      throw new Error(
        "BLOCKED v6 output already contains a run manifest; use --resume with the exact same candidates and settings or choose a new output directory",
      );
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }
  }
  const immutableRunStartedMs = Date.parse(manifest.lease_binding.started_at);
  const persist = async () => {
    const manifestBytes = `${JSON.stringify(manifest, null, 2)}\n`;
    const manifestSha256 = createHash("sha256")
      .update(manifestBytes)
      .digest("hex");
    await atomicWrite(
      resolve(options.output, "run-manifest-v6.checkpoint.json"),
      `${JSON.stringify({
        schema_version: 1,
        manifest_sha256: manifestSha256,
        manifest_file: "run-manifest-v6.json",
      })}\n`,
    );
    await atomicWrite(manifestPath, manifestBytes);
    await atomicWrite(
      resolve(options.output, "run-manifest-v6.sha256"),
      `${manifestSha256}  run-manifest-v6.json\n`,
    );
  };
  await persist();
  const deadline = Date.parse(manifest.lease_binding.absolute_task_deadline_at);
  if (!Number.isFinite(deadline) || Date.now() >= deadline) {
    throw new Error("TIMED OUT v6 immutable absolute task deadline reached");
  }
  const journeyPlans = new Map(
    dryRun.plan.journeys.map((journey) => [journey.scenario, journey]),
  );
  try {
    for (let cursor = manifest.launches.length; cursor < launches.length; ) {
      const first = launches[cursor];
      const grouped = [];
      if (first.phase === "correctness") grouped.push(launches[cursor++]);
      else {
        while (launches[cursor]?.bundle_id === first.bundle_id) {
          grouped.push(launches[cursor++]);
        }
      }
      const results = [];
      assertSessionIdentityV6(
        qualification.environment_binding.session_identity,
        await collectSessionIdentityV6(),
      );
      for (const invocation of grouped) {
        const remainingMs = deadline - Date.now();
        if (remainingMs < options.timeoutMs + 15_000) {
          throw new Error("TIMED OUT v6 absolute task limit reached");
        }
        const candidatePrelaunchSeal = await sealCandidateLaunchV6(
          invocation,
          options,
          candidates,
        );
        const launched = await runRunnerV6(
          invocation,
          Math.min(options.timeoutMs + 15_000, remainingMs),
        );
        let launchBinding = null;
        if (launched.report) {
          requireActiveRendererAdapterV6(
            launched.report,
            invocation.implementation,
            qualification.environment_binding.adapter.uuid,
          );
          launchBinding = buildLaunchBindingV5({
            invocation,
            launched,
            candidate: candidates[invocation.implementation],
            fixtureArtifacts: verified.fixtures,
          });
          launched.report.launch_binding_v5 = launchBinding;
          launched.report.launch_binding_v6 = {
            ...structuredClone(launchBinding),
            protocol_version: protocolVersionV6,
            workload_byte_sha256: expectedWorkloadByteSha256V6,
            dbus_session_address_sha256:
              qualification.environment_binding.dbus_session_address_sha256,
            session_identity_sha256: canonicalSha256(
              qualification.environment_binding.session_identity,
            ),
            candidate_prelaunch_seal: candidatePrelaunchSeal,
          };
          await writeFile(
            invocation.raw_report_path,
            `${JSON.stringify(launched.report, null, 2)}\n`,
          );
        }
        const artifact = launched.report
          ? await fileArtifact(invocation.raw_report_path)
          : null;
        const assessment = !launched.report
          ? {
              passed: false,
              failures: ["raw runner report is absent or invalid JSON"],
              receipts: [],
              measurements: {},
              quality_measurements: {},
              benefit_metrics_eligible: false,
              view_state_receipt: null,
            }
          : assessRunV6({
              rawReport: launched.report,
              run: invocation,
              v4Workload,
              v4Plan,
              v5Workload,
              candidateArtifactSha256:
                candidates[invocation.implementation].sha256,
            });
        let hardArtifact = null;
        let dynamicCropArtifacts = [];
        if (assessment.hard_report) {
          await writeFile(
            invocation.hard_report_path,
            `${JSON.stringify(assessment.hard_report, null, 2)}\n`,
          );
          hardArtifact = await fileArtifact(invocation.hard_report_path);
          dynamicCropArtifacts = await retainDynamicCropArtifactsV5(
            assessment.hard_report,
            {
              runArtifactRoot: options.output,
              referenceArtifacts: verified.references,
            },
          );
        }
        const passed =
          launched.exit_code === 0 &&
          launched.timed_out !== true &&
          assessment.passed === true;
        const result = {
          passed,
          failures: [
            ...(launched.exit_code === 0
              ? []
              : [`runner exit code ${launched.exit_code}`]),
            ...(launched.timed_out ? ["runner timed out"] : []),
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
        const { report: _embeddedRawReport, ...retainedLaunchOutcome } =
          launched;
        manifest.launches.push({
          ...invocation,
          ...retainedLaunchOutcome,
          ...result,
        });
        if (invocation.phase === "correctness") {
          manifest.correctness_reports.push({
            implementation: invocation.implementation,
            journey: invocation.journey,
            component: invocation.component,
            input_lane: invocation.input_lane,
            passed,
            correctness_passed: result.correctness_passed,
            known_baseline_defect_id: result.known_baseline_defect_id,
            raw_report_path: result.raw_report_path,
            raw_report_sha256: result.raw_report_sha256,
          });
        }
        await persist();
        if (!passed) {
          throw new Error(
            `BLOCKED ${invocation.identity}: ${result.failures.join("; ")}`,
          );
        }
        if (
          invocation.component === propertyCorrectnessOnlyComponentIdV6 &&
          invocation.implementation === "electron" &&
          !electronPropertyOutcomeAcceptedV5(result)
        ) {
          throw new Error(
            "BLOCKED Electron property result is neither an exact pass nor the frozen known baseline defect",
          );
        }
        if (
          invocation.component === propertyCorrectnessOnlyComponentIdV6 &&
          invocation.implementation === "gpui" &&
          result.correctness_passed !== true
        ) {
          throw new Error("BLOCKED GPUI property correctness did not pass");
        }
        if (
          invocation.benefit_metrics_eligible === true &&
          result.benefit_metrics_eligible !== true
        ) {
          throw new Error(
            `BLOCKED ${invocation.identity} lacks native benefit evidence`,
          );
        }
        if (manifest.launches.length < launches.length) {
          await delay(options.cooldownMs);
        }
      }
      if (first.phase !== "correctness") {
        const bundle = buildBundleManifestV6({
          runs: grouped,
          results,
          journeyPlan: journeyPlans.get(first.journey),
          candidate: candidates[first.implementation],
        });
        const bundlePath = resolve(
          options.output,
          `${first.bundle_id}-bundle-manifest-v6.json`,
        );
        await writeFile(bundlePath, `${JSON.stringify(bundle, null, 2)}\n`);
        const retained = {
          ...bundle,
          path: bundlePath,
          sha256: await sha256File(bundlePath),
        };
        const counterpart = manifest.bundles.find(
          (candidate) =>
            candidate.phase === bundle.phase &&
            candidate.journey === bundle.journey &&
            candidate.pair === bundle.pair &&
            candidate.implementation !== bundle.implementation,
        );
        manifest.bundles.push(retained);
        if (counterpart) {
          const electronBundle =
            bundle.implementation === "electron" ? bundle : counterpart;
          const gpuiBundle =
            bundle.implementation === "gpui" ? bundle : counterpart;
          const viewStatePair = compareBundleViewStatesV5(
            electronBundle,
            gpuiBundle,
          );
          manifest.view_state_pairs.push(viewStatePair);
          if (viewStatePair.passed !== true) {
            throw new Error(
              `BLOCKED ${bundle.phase} ${bundle.journey} pair ${bundle.pair} view state differs: ${viewStatePair.failures.join("; ")}`,
            );
          }
        }
        await persist();
      }
    }
    const expectedBundles =
      dryRun.plan.journeys.filter(
        ({ benefit_components: components }) => components.length > 0,
      ).length *
      (calibrationPairCountV6 + finalPairCountV6) *
      implementations.length;
    const expectedViewStatePairs = expectedBundles / 2;
    if (
      manifest.launches.length !== totalLaunchCountV6 ||
      manifest.bundles.length !== expectedBundles ||
      manifest.view_state_pairs.length !== expectedViewStatePairs ||
      manifest.correctness_reports.length !==
        semanticCorrectnessLaunchCountV6 + propertyCorrectnessLaunchCountV6
    ) {
      throw new Error("BLOCKED retained v6 evidence counts are incomplete");
    }
    manifest.complete = true;
    manifest.outcome = "passed";
    manifest.ended_at = new Date().toISOString();
    manifest.actual_duration_ms = Date.now() - immutableRunStartedMs;
    await persist();
    return manifest;
  } catch (error) {
    manifest.complete = false;
    manifest.outcome = String(error?.message ?? error).startsWith("TIMED OUT")
      ? "timed-out"
      : "failed-closed";
    manifest.failure = error?.message ?? String(error);
    manifest.ended_at = new Date().toISOString();
    manifest.actual_duration_ms = Date.now() - immutableRunStartedMs;
    await persist();
    throw error;
  }
}

function usage() {
  return `Usage: node run-paired-v6.mjs --plan|--qualify|--execute --output <directory> [v5 runner options]

V6 freezes 624 launches: 600 native benefit launches, 22 semantic
correctness launches, and two native property correctness launches. Pair counts
cannot be overridden. Execution must run once inside one shared D-Bus desktop
session, for example: dbus-run-session -- node run-paired-v6.mjs --execute ...
Run --qualify first on the paid GPU with reviewed task and lease TTL limits.
Then pass its exact --qualification-receipt to --execute under the same single
enclosing dbus-run-session; separate D-Bus wrappers fail identity validation.
Use --resume to authenticate and reuse only a complete passed schedule prefix.
`;
}

async function main() {
  const options = parseV6Arguments(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(usage());
    return;
  }
  const { workload, byte_sha256: byteSha256 } =
    await loadComparisonWorkloadV6();
  const plan = buildV6ComparisonPlan(workload, byteSha256);
  const schedule = buildV6MainSchedule(plan, options);
  const dryRun = buildV6DryRun(plan, schedule, options);
  if (options.planOutput) {
    await mkdir(dirname(options.planOutput), { recursive: true });
    await writeFile(options.planOutput, `${JSON.stringify(dryRun, null, 2)}\n`);
  }
  if (options.mode === "plan") {
    process.stdout.write(`${JSON.stringify(dryRun, null, 2)}\n`);
    return;
  }
  if (options.mode === "qualify") {
    const qualification = await executeV6Qualification(
      workload,
      plan,
      dryRun,
      options,
    );
    process.stdout.write(`${JSON.stringify(qualification, null, 2)}\n`);
    return;
  }
  await executeV6(workload, dryRun, options);
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
) {
  await main();
}
