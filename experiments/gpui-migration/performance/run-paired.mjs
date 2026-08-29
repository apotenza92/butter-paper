#!/usr/bin/env node

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { allowedScenarios, lockedFixtureForScenario } from "./scenario-contract.mjs";
import { createBalancedPairOrders } from "./decision-statistics.mjs";
import {
  currentRunnerCoverageReport,
  loadComparisonWorkload,
  runnerComparisonMetadata,
} from "./comparison-workload.mjs";

const performanceDirectory = dirname(fileURLToPath(import.meta.url));
export const fullTimedRepresentativeScenarios = Object.freeze([
  "open-pdf",
  "viewer-layout",
  "page-navigation",
  "zoom",
  "high-zoom-pan",
  "continuous-scroll",
  "cache-pressure",
  "close-reopen",
  "annotation-create",
  "annotation-transform",
  "editor-create",
]);

export const untimedCorrectnessPreflights = Object.freeze([
  Object.freeze({
    implementation: "electron",
    scenario: "open-pdf",
    input_lane: "native-x11-xtest",
    proof: "native-launch-open",
  }),
  Object.freeze({
    implementation: "gpui",
    scenario: "open-pdf",
    input_lane: "native-x11-xtest",
    proof: "native-launch-open",
  }),
  Object.freeze({
    implementation: "electron",
    scenario: "editor-workload",
    input_lane: "semantic-diagnostic",
    proof: "full-editor-correctness",
  }),
  Object.freeze({
    implementation: "gpui",
    scenario: "editor-workload",
    input_lane: "semantic-diagnostic",
    proof: "full-editor-correctness",
  }),
  Object.freeze({
    implementation: "electron",
    scenario: "persistence-workload",
    input_lane: "semantic-diagnostic",
    proof: "full-persistence-correctness",
  }),
  Object.freeze({
    implementation: "gpui",
    scenario: "persistence-workload",
    input_lane: "semantic-diagnostic",
    proof: "full-persistence-correctness",
  }),
]);

function parsePositiveInteger(value, option) {
  if (!/^\d+$/.test(value ?? "") || Number(value) < 1) {
    throw new Error(`${option} must be a positive integer`);
  }
  return Number(value);
}

function parseArguments(argv) {
  const options = {
    pairs: 5,
    warmups: 1,
    timeoutMs: 120_000,
    cooldownMs: 2_000,
    orderMode: "alternating",
    seed: 0x4250_5633,
    scenarios: fullTimedRepresentativeScenarios,
    fixtures: new Map(),
  };
  const valueOptions = new Set([
    "--fixture", "--output", "--pairs", "--warmups", "--timeout-ms", "--cooldown-ms",
    "--scenarios", "--electron", "--gpui-binary", "--order-mode", "--seed",
  ]);
  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index];
    if (option === "-h" || option === "--help") return { help: true };
    if (!valueOptions.has(option)) throw new Error(`unknown option: ${option}`);
    const value = argv[++index];
    if (value === undefined || value.startsWith("--")) throw new Error(`${option} requires a value`);
    if (option === "--fixture") {
      const separator = value.indexOf("=");
      if (separator <= 0 || separator + 1 >= value.length) {
        throw new Error("--fixture must be <fixture-id>=<file>");
      }
      const fixtureId = value.slice(0, separator);
      if (options.fixtures.has(fixtureId)) throw new Error(`duplicate --fixture ${fixtureId}`);
      options.fixtures.set(fixtureId, resolve(value.slice(separator + 1)));
    }
    if (option === "--output") options.output = resolve(value);
    if (option === "--pairs") options.pairs = parsePositiveInteger(value, option);
    if (option === "--warmups") options.warmups = Number(value);
    if (option === "--timeout-ms") options.timeoutMs = parsePositiveInteger(value, option);
    if (option === "--cooldown-ms") options.cooldownMs = Number(value);
    if (option === "--scenarios") options.scenarios = value.split(",").filter(Boolean);
    if (option === "--electron") options.electron = resolve(value);
    if (option === "--gpui-binary") options.gpuiBinary = resolve(value);
    if (option === "--order-mode") options.orderMode = value;
    if (option === "--seed") options.seed = Number(value);
  }
  if (!options.output) throw new Error("--output is required");
  if (!Number.isInteger(options.warmups) || options.warmups < 0) throw new Error("--warmups must be a nonnegative integer");
  if (!Number.isInteger(options.cooldownMs) || options.cooldownMs < 0) throw new Error("--cooldown-ms must be a nonnegative integer");
  if (!["alternating", "randomized-blocks"].includes(options.orderMode)) {
    throw new Error("--order-mode must be alternating or randomized-blocks");
  }
  if (!Number.isInteger(options.seed)) throw new Error("--seed must be an integer");
  if (options.scenarios.some((scenario) => !allowedScenarios.has(scenario))) {
    throw new Error(`--scenarios may contain only ${[...allowedScenarios].join(",")}`);
  }
  return options;
}

function usage() {
  return `Usage: node run-paired.mjs --fixture <fixture-id>=<file> --output <directory> [options]

Runs one application at a time. Each pair contains one Electron and one GPUI
process, with the first implementation alternating by pair. Warmups are kept
as raw reports but excluded from the paired summary.

Options:
  --pairs <count>         Measured pairs per scenario (default: 5)
  --warmups <count>       Warmups per implementation and scenario (default: 1)
  --fixture <id>=<file>   Repeat for each locked fixture required by selected scenarios
  --scenarios <csv>       Full decision set by default; omitting a required scenario blocks execution
  --order-mode <mode>     alternating or randomized-blocks (default: alternating)
  --seed <integer>        Recorded randomized-block seed (default: 1112561203)
  --timeout-ms <ms>       Per-process runner timeout (default: 120000)
  --cooldown-ms <ms>      Idle gap between processes (default: 2000)
  --electron <file>       Override Electron executable
  --gpui-binary <file>    Override GPUI executable
`;
}

export function pairOrder(pair) {
  return pair % 2 === 1 ? ["electron", "gpui"] : ["gpui", "electron"];
}

export function pairOrders({ pairCount, mode = "alternating", seed = 0x4250_5633 }) {
  if (!Number.isInteger(pairCount) || pairCount < 1) {
    throw new Error("pairCount must be a positive integer");
  }
  if (mode === "alternating") {
    return Array.from({ length: pairCount }, (_, index) => pairOrder(index + 1));
  }
  if (mode === "randomized-blocks") {
    return createBalancedPairOrders({ pairCount, seed });
  }
  throw new Error(`unsupported pair order mode ${mode}`);
}

export function pairedInputLane(scenario) {
  return [
    "viewer-layout",
    "page-navigation",
    "zoom",
    "high-zoom-pan",
    "continuous-scroll",
    "annotation-create",
    "annotation-transform",
    "editor-create",
  ].includes(scenario)
    ? "native-x11-xtest"
    : null;
}

export function scenarioFixtureRequirements(scenarios) {
  return Object.fromEntries(scenarios.map((scenario) => [scenario, lockedFixtureForScenario(scenario)]));
}

export function matchedScenarioBlockers(workload, scenarios) {
  const blockers = [];
  for (const scenario of scenarios) {
    for (const implementation of ["electron", "gpui"]) {
      const metadata = runnerComparisonMetadata(workload, implementation, scenario);
      if (metadata.scenario_status !== "supported-diagnostic") {
        blockers.push({ scenario, implementation, reason: metadata.blocked_reason });
      }
    }
  }
  return blockers;
}

export function buildPairedComparisonPlan(
  workload,
  timedScenarios = fullTimedRepresentativeScenarios,
) {
  const timedSetBlockers = fullTimedRepresentativeScenarios
    .filter((scenario) => !timedScenarios.includes(scenario))
    .map((scenario) => ({
      phase: "timed-set",
      scenario,
      implementation: null,
      reason: "required-timed-scenario-missing",
    }));
  const timedBlockers = matchedScenarioBlockers(workload, timedScenarios).map((blocker) => ({
    phase: "timed",
    ...blocker,
  }));
  const preflightBlockers = untimedCorrectnessPreflights.flatMap((preflight) => {
    const metadata = runnerComparisonMetadata(
      workload,
      preflight.implementation,
      preflight.scenario,
    );
    return metadata.scenario_status === "supported-diagnostic" ? [] : [{
      phase: "untimed-correctness-preflight",
      ...preflight,
      reason: metadata.blocked_reason,
    }];
  });
  const globalCommandCoverage = currentRunnerCoverageReport(workload).implementations.map((report) => ({
    implementation: report.implementation,
    command_count: report.command_count,
    ready_command_count: report.ready_command_count,
    blocked_command_count: report.blocked_command_count,
    ready: report.ready,
  }));
  const coverageBlockers = globalCommandCoverage
    .filter(({ ready }) => !ready)
    .map((coverage) => ({
      phase: "global-feature-coverage",
      scenario: null,
      implementation: coverage.implementation,
      reason: "global-command-coverage-incomplete",
      command_count: coverage.command_count,
      ready_command_count: coverage.ready_command_count,
      blocked_command_count: coverage.blocked_command_count,
    }));
  const blockers = [
    ...timedSetBlockers,
    ...timedBlockers,
    ...preflightBlockers,
    ...coverageBlockers,
  ];
  return {
    manifest_id: workload.manifest_id,
    decision_contract_version: workload.decision_contract_version,
    ready: blockers.length === 0,
    timed_scenarios: timedScenarios.map((scenario) => ({
      scenario,
      input_lane: pairedInputLane(scenario) ?? "semantic-diagnostic",
    })),
    untimed_preflights: untimedCorrectnessPreflights.map((preflight) => ({
      ...preflight,
      decision_timing_eligible: false,
    })),
    global_command_coverage: globalCommandCoverage,
    blockers,
  };
}

export function buildPairedExecutionSchedule(
  plan,
  {
    pairCount,
    warmups,
    orderMode = "alternating",
    seed = 0x4250_5633,
  },
) {
  if (plan?.ready !== true || plan.blockers?.length > 0) {
    throw new Error("BLOCKED paired comparison plan");
  }
  if (!Number.isInteger(warmups) || warmups < 0) {
    throw new Error("warmups must be a nonnegative integer");
  }
  const schedule = plan.untimed_preflights.map((preflight) => ({
    ...preflight,
    phase: "untimed-correctness-preflight",
    name: `preflight-${preflight.proof}-${preflight.implementation}`,
    measured: false,
  }));
  const orders = pairOrders({ pairCount, mode: orderMode, seed });
  for (const { scenario, input_lane: inputLane } of plan.timed_scenarios) {
    for (let warmup = 1; warmup <= warmups; warmup += 1) {
      for (const implementation of ["electron", "gpui"]) {
        schedule.push({
          phase: "timed",
          implementation,
          scenario,
          input_lane: inputLane,
          name: `${scenario}-warmup${warmup}-${implementation}`,
          measured: false,
        });
      }
    }
    for (const [pairIndex, order] of orders.entries()) {
      const pair = pairIndex + 1;
      for (const [positionIndex, implementation] of order.entries()) {
        schedule.push({
          phase: "timed",
          implementation,
          scenario,
          input_lane: inputLane,
          pair,
          position: positionIndex === 0 ? "first" : "second",
          name: `${scenario}-pair${pair}-${positionIndex === 0 ? "first" : "second"}-${implementation}`,
          measured: true,
        });
      }
    }
  }
  return schedule;
}

export function assessUntimedPreflightRuns(plan, runs) {
  const failures = [];
  for (const expected of plan.untimed_preflights) {
    const matching = runs.filter((run) =>
      run.phase === "untimed-correctness-preflight"
      && run.implementation === expected.implementation
      && run.scenario === expected.scenario);
    const label = `${expected.implementation}:${expected.scenario}`;
    if (matching.length !== 1) {
      failures.push(
        `${label}: expected one retained ${expected.proof} preflight run, got ${matching.length}`,
      );
      continue;
    }
    const [run] = matching;
    if (
      run.exit_code !== 0
      || run.report?.successful_iterations !== 1
      || run.report?.failed_iterations !== 0
    ) {
      failures.push(`${label}: untimed preflight runner did not pass`);
      continue;
    }
    if (
      run.input_lane !== expected.input_lane
      || run.preflight_proof?.input_lane !== expected.input_lane
      || run.preflight_proof?.proof !== expected.proof
      || run.preflight_proof?.passed !== true
    ) {
      failures.push(`${label}: ${expected.proof} proof did not pass on ${expected.input_lane}`);
    }
  }
  return { ready: failures.length === 0, failures };
}

export function resolveScenarioFixtures(scenarios, fixtureAssignments) {
  const requirements = scenarioFixtureRequirements(scenarios);
  return Object.fromEntries(Object.entries(requirements).map(([scenario, requirement]) => {
    const path = fixtureAssignments.get(requirement.fixture_id);
    if (!path) throw new Error(`--fixture ${requirement.fixture_id}=<file> is required for ${scenario}`);
    return [scenario, { ...requirement, path }];
  }));
}

export async function verifyScenarioFixtures(scenarioFixtures) {
  const uniqueFixtures = [
    ...new Map(
      Object.values(scenarioFixtures).map((fixture) => [fixture.fixture_id, fixture]),
    ).values(),
  ];
  return Object.fromEntries(await Promise.all(uniqueFixtures.map(async (fixture) => {
    const actualSha256 = await sha256(fixture.path);
    if (actualSha256 !== fixture.fixture_sha256) {
      throw new Error(
        `${fixture.fixture_id}: SHA-256 mismatch: expected ${fixture.fixture_sha256}, got ${actualSha256}`,
      );
    }
    return [fixture.fixture_id, {
      path: fixture.path,
      expected_sha256: fixture.fixture_sha256,
      sha256: actualSha256,
    }];
  })));
}

function delay(ms) {
  return new Promise((resolvePromise) => setTimeout(resolvePromise, ms));
}

async function sha256(path) {
  const hash = createHash("sha256");
  await new Promise((resolvePromise, rejectPromise) => {
    const stream = createReadStream(path);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("error", rejectPromise);
    stream.on("end", resolvePromise);
  });
  return hash.digest("hex");
}

async function runCommand(args, output, preflight = null) {
  const startedAt = new Date().toISOString();
  let stdout = "";
  let stderr = "";
  const child = spawn(process.execPath, args, {
    cwd: performanceDirectory,
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => { stdout += chunk; });
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
    process.stderr.write(chunk);
  });
  const result = await new Promise((resolvePromise) => {
    child.once("error", (error) => resolvePromise({ exit_code: null, signal: null, spawn_error: error.message }));
    child.once("close", (code, signal) => resolvePromise({ exit_code: code, signal, spawn_error: null }));
  });
  let reportStatus = null;
  try {
    const report = JSON.parse(await readFile(output, "utf8"));
    const events = (report.iterations ?? []).flatMap((iteration) => iteration.events ?? []);
    const iterationPassed = report.summary?.successful_iterations === 1
      && report.summary?.failed_iterations === 0;
    const preflightPassed = preflight?.proof === "native-launch-open"
      ? iterationPassed && events.some(({ event }) => event === "viewer-native-launch-open-evidence")
      : preflight ? iterationPassed : null;
    reportStatus = {
      successful_iterations: report.summary?.successful_iterations,
      failed_iterations: report.summary?.failed_iterations,
      protocol_version: report.protocol_version,
      pdf_sha256: report.pdf?.sha256,
      implementation: report.implementation,
      scenario: report.scenario,
      decision_timing_eligible: report.comparison_workload?.decision_timing_eligible,
      feature_coverage_ready: report.comparison_workload?.feature_coverage?.ready,
    };
    if (preflight) {
      reportStatus.preflight_proof = {
        proof: preflight.proof,
        input_lane: preflight.input_lane,
        passed: preflightPassed,
      };
    }
  } catch {
    reportStatus = { report_missing_or_invalid: true };
  }
  return { started_at: startedAt, ended_at: new Date().toISOString(), ...result, stdout, stderr, report: reportStatus };
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(usage());
    return;
  }
  const workload = await loadComparisonWorkload();
  const comparisonPlan = buildPairedComparisonPlan(workload, options.scenarios);
  if (!comparisonPlan.ready) {
    throw new Error(`BLOCKED paired comparison plan: ${comparisonPlan.blockers.map(
      ({ phase, scenario, implementation }) =>
        `${phase}:${scenario ?? "all-commands"}:${implementation ?? "both"}`,
    ).join(", ")}`);
  }
  const fixtureScenarios = [...new Set([
    ...options.scenarios,
    ...comparisonPlan.untimed_preflights.map(({ scenario }) => scenario),
  ])];
  const scenarioFixtures = resolveScenarioFixtures(fixtureScenarios, options.fixtures);
  const verifiedFixtures = await verifyScenarioFixtures(scenarioFixtures);
  await mkdir(options.output, { recursive: true });
  const manifestPath = resolve(options.output, "run-manifest.json");
  const manifest = {
    schema_version: 2,
    purpose: "same-host Electron versus GPUI full representative decision comparison",
    evidence_boundary: "untimed correctness preflights followed by full timed development-runtime pairs; not packaged-candidate qualification",
    started_at: new Date().toISOString(),
    comparison_plan: comparisonPlan,
    fixtures: verifiedFixtures,
    scenario_fixtures: scenarioFixtures,
    settings: {
      pairs: options.pairs,
      warmups: options.warmups,
      timeout_ms: options.timeoutMs,
      cooldown_ms: options.cooldownMs,
      scenarios: options.scenarios,
      order_mode: options.orderMode,
      order_seed: options.seed,
      pair_orders: pairOrders({ pairCount: options.pairs, mode: options.orderMode, seed: options.seed }),
      cache_class: "app-cold",
      concurrency: 1,
    },
    runs: [],
  };
  const persist = async () => writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
  await persist();

  const run = async ({ implementation, scenario, name, measured, phase, input_lane: inputLane, proof }) => {
    const runner = implementation === "electron" ? "electron-runner.mjs" : "gpui-runner.mjs";
    const output = resolve(options.output, `${name}.json`);
    const fixture = scenarioFixtures[scenario];
    const args = [
      resolve(performanceDirectory, runner),
      "--scenario", scenario,
      "--pdf", fixture.path,
      "--iterations", "1",
      "--timeout-ms", String(options.timeoutMs),
      "--output", output,
    ];
    if (inputLane) args.push("--input-lane", inputLane);
    if (implementation === "electron" && options.electron) args.push("--electron", options.electron);
    if (implementation === "gpui" && options.gpuiBinary) args.push("--binary", options.gpuiBinary);
    const result = await runCommand(args, output, proof ? { proof, input_lane: inputLane } : null);
    const retainedRun = {
      phase,
      implementation,
      scenario,
      fixture_id: fixture.fixture_id,
      input_lane: inputLane,
      ...(proof ? { proof } : {}),
      name,
      measured,
      output,
      ...result,
      ...(result.report?.preflight_proof
        ? { preflight_proof: result.report.preflight_proof }
        : {}),
    };
    manifest.runs.push(retainedRun);
    await persist();
    await delay(options.cooldownMs);
    return retainedRun;
  };

  const schedule = buildPairedExecutionSchedule(comparisonPlan, {
    pairCount: options.pairs,
    warmups: options.warmups,
    orderMode: options.orderMode,
    seed: options.seed,
  });
  const preflightSchedule = schedule.filter(({ phase }) => phase === "untimed-correctness-preflight");
  for (const plannedRun of preflightSchedule) await run(plannedRun);
  manifest.untimed_preflight_assessment = assessUntimedPreflightRuns(
    comparisonPlan,
    manifest.runs,
  );
  await persist();
  if (!manifest.untimed_preflight_assessment.ready) {
    throw new Error(`BLOCKED untimed correctness preflights: ${manifest.untimed_preflight_assessment.failures.join("; ")}`);
  }
  for (const plannedRun of schedule.filter(({ phase }) => phase === "timed")) await run(plannedRun);
  manifest.ended_at = new Date().toISOString();
  manifest.complete = manifest.runs.every((run) =>
    run.exit_code === 0 && run.report?.successful_iterations === 1 && run.report?.failed_iterations === 0,
  );
  await persist();
  if (!manifest.complete) process.exitCode = 1;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  await main();
}
