#!/usr/bin/env node

import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { mkdir, readFile, rename, stat, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { totalmem } from "node:os";
import {
  buildRunnerInvocationV6,
  buildV6ComparisonPlan,
  buildV6ExecutionSchedule,
  exactElectronEngineeringZoomBaselineDefectV6,
  loadComparisonWorkloadV6,
  runRunnerV6,
  validateCandidateLaunchSealV6,
} from "./run-paired-v6.mjs";
import {
  buildHardComponentReportV5,
  validateExactCandidateHashesV5,
} from "./run-paired-v5.mjs";
import { loadMaterializedComparisonWorkloadV5 } from "./comparison-workload-v5.mjs";
import { loadMaterializedComparisonWorkloadV4 } from "./comparison-workload-v4.mjs";
import {
  buildV4ComparisonPlan,
  validateV4ComponentReport,
} from "./run-paired-v4.mjs";
import { buildScenarioContractV4 } from "./scenario-contract-v4.mjs";
import { validateHardComponentReportV5 } from "./summarize-paired-v5.mjs";
import {
  revalidateOptimizedCandidateLaunchV4,
  validateOptimizedCandidatesV4,
} from "./optimized-candidates-v4.mjs";
import {
  classifyElectronEngineeringZoomBaselineDefectV6,
  electronEngineeringZoomBaselineDefectIdV6,
} from "./electron-v6-baseline-defect.mjs";

export const localCorrectnessReceiptTypeV6 =
  "bp-perf-v6-local-correctness-preflight-v1";
export const localCorrectnessEvidenceScopeV6 =
  "local-development-host-correctness-only-non-decision";
const manifestFilename = "local-correctness-manifest-v6.json";
const defaultSeed = 0x4250_5636;
const runnerCleanupMarginMs = 15_000;
const exactFixtureIds = Object.freeze([
  "nasa-apollo-summary-526-v1",
  "bp-engineering-sheet-v1",
  "bp-annotation-density-v1",
  "bp-annotation-all-v1",
]);

function parseInteger(value, option, { minimum = 0 } = {}) {
  if (!/^\d+$/.test(value ?? "") || Number(value) < minimum) {
    throw new Error(`${option} must be an integer at least ${minimum}`);
  }
  return Number(value);
}

export function parseLocalCorrectnessArgumentsV6(argv) {
  const options = {
    timeoutMs: 120_000,
    cooldownMs: 0,
    seed: defaultSeed,
    fixtures: new Map(),
  };
  const values = new Map([
    ["--output", "output"],
    ["--electron", "electron"],
    ["--gpui-binary", "gpuiBinary"],
    ["--electron-candidate-artifact", "electronCandidateArtifact"],
    ["--gpui-candidate-artifact", "gpuiCandidateArtifact"],
    ["--electron-candidate-sha256", "electronCandidateSha256"],
    ["--gpui-candidate-sha256", "gpuiCandidateSha256"],
  ]);
  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index];
    if (option === "--help" || option === "-h") {
      options.help = true;
      continue;
    }
    const value = argv[++index];
    if (value === undefined || value.startsWith("--")) {
      throw new Error(`${option} requires a value`);
    }
    if (option === "--fixture") {
      const separator = value.indexOf("=");
      if (separator < 1 || separator === value.length - 1) {
        throw new Error("--fixture requires <id>=<path>");
      }
      const id = value.slice(0, separator);
      if (options.fixtures.has(id)) throw new Error(`duplicate fixture ${id}`);
      options.fixtures.set(id, resolve(value.slice(separator + 1)));
    } else if (option === "--timeout-ms") {
      options.timeoutMs = parseInteger(value, option, { minimum: 1 });
    } else if (option === "--cooldown-ms") {
      options.cooldownMs = parseInteger(value, option);
    } else if (option === "--seed") {
      options.seed = parseInteger(value, option);
    } else if (values.has(option)) {
      const field = values.get(option);
      options[field] = field.endsWith("Sha256") ? value : resolve(value);
    } else {
      throw new Error(`unknown local correctness option ${option}`);
    }
  }
  if (options.help) return options;
  for (const field of values.values()) {
    if (!options[field]) throw new Error(`${field} is required`);
  }
  for (const field of ["electronCandidateSha256", "gpuiCandidateSha256"]) {
    if (!/^[0-9a-f]{64}$/.test(options[field])) {
      throw new Error(`${field} must be a lowercase SHA-256`);
    }
  }
  if (
    options.fixtures.size !== exactFixtureIds.length ||
    exactFixtureIds.some((id) => !options.fixtures.has(id))
  ) {
    throw new Error(
      `local correctness requires exactly these four fixtures: ${exactFixtureIds.join(", ")}`,
    );
  }
  return options;
}

export function buildLocalCorrectnessScheduleV6(
  plan,
  { seed = defaultSeed } = {},
) {
  const schedule = buildV6ExecutionSchedule(plan, { seed }).filter(
    ({ phase }) => phase === "correctness",
  );
  const semantic = schedule.filter(
    ({ input_lane: lane }) => lane === "semantic-diagnostic",
  ).length;
  const property = schedule.filter(
    ({ component }) => component === "native-property-edit-undo",
  ).length;
  if (
    schedule.length !== 24 ||
    semantic !== 22 ||
    property !== 2 ||
    schedule.some(
      ({ inference_eligible: inference, benefit_metrics_eligible: benefit }) =>
        inference !== false || benefit !== false,
    )
  ) {
    throw new Error("BLOCKED local v6 correctness schedule is not exact");
  }
  const fixtures = [
    ...new Set(schedule.flatMap(({ fixture_ids: ids }) => ids)),
  ];
  if (
    fixtures.length !== exactFixtureIds.length ||
    exactFixtureIds.some((id) => !fixtures.includes(id))
  ) {
    throw new Error("BLOCKED local v6 correctness fixture subset is not exact");
  }
  return schedule;
}

export function buildLocalCorrectnessInvocationV6(run, options) {
  const invocation = buildRunnerInvocationV6(run, options);
  const environment = {
    ...invocation.environment,
    BP_PERF_REQUIRE_NVIDIA: "0",
    BP_PERF_COMMON_DAMAGE_OBSERVER: "0",
  };
  if (run.implementation === "gpui") {
    environment.GPUI_X11_SCALE_FACTOR = "1";
  }
  delete environment.BP_PERF_V5_REFERENCE_CROP_DIR;
  return {
    ...invocation,
    environment,
    inference_eligible: false,
    benefit_metrics_eligible: false,
  };
}

async function fileArtifact(path) {
  const metadata = await stat(path);
  if (!metadata.isFile()) throw new Error(`artifact is not a file: ${path}`);
  const hash = createHash("sha256");
  await new Promise((resolvePromise, rejectPromise) => {
    const stream = createReadStream(path);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("error", rejectPromise);
    stream.on("end", resolvePromise);
  });
  return { path, bytes: metadata.size, sha256: hash.digest("hex") };
}

export async function verifyLocalCorrectnessFixturesV6({
  workload,
  schedule,
  fixtures,
  readArtifact = fileArtifact,
}) {
  const requiredIds = [
    ...new Set(schedule.flatMap(({ fixture_ids: ids }) => ids)),
  ];
  if (
    requiredIds.length !== exactFixtureIds.length ||
    fixtures.size !== exactFixtureIds.length ||
    exactFixtureIds.some((id) => !requiredIds.includes(id) || !fixtures.has(id))
  ) {
    throw new Error(
      "local correctness fixture assignment is not the exact four-fixture subset",
    );
  }
  const definitions = new Map(
    (workload?.fixtures ?? []).map((fixture) => [fixture.id, fixture]),
  );
  const verified = {};
  for (const id of exactFixtureIds) {
    const definition = definitions.get(id);
    if (!definition)
      throw new Error(`frozen fixture definition is missing: ${id}`);
    const artifact = await readArtifact(fixtures.get(id));
    if (artifact.sha256 !== definition.sha256) {
      throw new Error(
        `${id}: fixture SHA-256 mismatch: expected ${definition.sha256}, got ${artifact.sha256}`,
      );
    }
    verified[id] = artifact;
  }
  return verified;
}

function assessLocalSemanticCorrectnessV6({
  rawReport,
  run,
  v4Workload,
  v4Plan,
}) {
  const journey = v4Plan.journeys.find(
    ({ scenario }) => scenario === run.journey,
  );
  if (!journey) {
    return {
      passed: false,
      failures: [`unknown v4 journey ${run.journey}`],
      receipts: [],
      correctness_passed: false,
      known_baseline_defect_id: null,
      hard_report: null,
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
  const fixtureFailures =
    scenarioContract.fixture_id === run.fixture_ids[0]
      ? []
      : ["semantic correctness fixture does not match its v6 mapping"];
  const knownBaselineDefectId = classifyElectronEngineeringZoomBaselineDefectV6(
    {
      implementation: run.implementation,
      journey: run.journey,
      component: run.component,
      receipts: receiptAssessment.receipts,
      source_command_results:
        rawReport?.iterations?.[0]?.renderer?.expanded_comparison
          ?.command_results,
    },
  );
  const exactKnownDefect = exactElectronEngineeringZoomBaselineDefectV6({
    knownBaselineDefectId,
    retainedReportDefectId:
      rawReport?.v4_parent_execution?.known_baseline_defect_id,
    retainedIterationDefectId:
      rawReport?.iterations?.[0]?.renderer?.v4_parent_component_evidence
        ?.known_baseline_defect_id,
    gpuFailures: [],
    fixtureFailures,
    receiptErrors: receiptAssessment.errors,
  });
  if (exactKnownDefect) {
    return {
      passed: true,
      failures: [],
      receipts: receiptAssessment.receipts,
      correctness_passed: false,
      known_baseline_defect_id: electronEngineeringZoomBaselineDefectIdV6,
      hard_report: null,
    };
  }
  const failures = [...receiptAssessment.errors, ...fixtureFailures];
  return {
    passed: failures.length === 0,
    failures,
    receipts: receiptAssessment.receipts,
    correctness_passed: failures.length === 0,
    known_baseline_defect_id: null,
    hard_report: null,
  };
}

export function assessLocalCorrectnessRunV6({
  rawReport,
  run,
  v4Workload,
  v4Plan,
  v5Workload,
  candidateArtifactSha256,
}) {
  if (run.phase !== "correctness" || run.benefit_metrics_eligible !== false) {
    return {
      passed: false,
      failures: ["local assessor accepts only frozen correctness-only runs"],
      receipts: [],
      correctness_passed: false,
      known_baseline_defect_id: null,
      hard_report: null,
    };
  }
  if (!run.hard_component) {
    return assessLocalSemanticCorrectnessV6({
      rawReport,
      run,
      v4Workload,
      v4Plan,
    });
  }
  const hardReport = buildHardComponentReportV5({
    workload: v5Workload,
    rawReport,
    run,
    candidateArtifactSha256,
  });
  const assessment = validateHardComponentReportV5(v5Workload, hardReport);
  return {
    passed: assessment.passed,
    failures: assessment.failures,
    receipts: hardReport.command_receipts,
    correctness_passed: assessment.correctness_passed,
    known_baseline_defect_id: assessment.known_baseline_defect_id,
    hard_report: hardReport,
  };
}

function validateRuntimeEnvironment(environment) {
  const failures = [];
  if (!environment.DISPLAY)
    failures.push("DISPLAY is required for real local application runners");
  if (!environment.DBUS_SESSION_BUS_ADDRESS) {
    failures.push(
      "DBUS_SESSION_BUS_ADDRESS is required for one local desktop session",
    );
  }
  if (failures.length > 0) throw new Error(`BLOCKED ${failures.join("; ")}`);
}

async function atomicWrite(path, bytes) {
  const temporary = `${path}.tmp-${process.pid}`;
  await writeFile(temporary, bytes);
  await rename(temporary, path);
}

function delay(milliseconds) {
  return new Promise((resolvePromise) =>
    setTimeout(resolvePromise, milliseconds),
  );
}

async function defaultSealCandidate(invocation, options, candidates) {
  return validateCandidateLaunchSealV6({
    seal: await revalidateOptimizedCandidateLaunchV4({
      launchId: invocation.identity,
      electronManifestPath: options.electronCandidateArtifact,
      gpuiManifestPath: options.gpuiCandidateArtifact,
      electronExecutable: options.electron,
      gpuiBinary: options.gpuiBinary,
    }),
    launchId: invocation.identity,
    candidates,
  });
}

export async function executeLocalCorrectnessV6(
  {
    plan,
    schedule,
    options,
    candidates,
    fixtures,
    v4Workload,
    v4Plan,
    v5Workload,
  },
  {
    environment = process.env,
    sealCandidate = defaultSealCandidate,
    runRunner = runRunnerV6,
    assessRun = assessLocalCorrectnessRunV6,
  } = {},
) {
  validateRuntimeEnvironment(environment);
  if (
    schedule.length !== 24 ||
    schedule.some(
      ({ phase, benefit_metrics_eligible: benefit }) =>
        phase !== "correctness" || benefit !== false,
    )
  ) {
    throw new Error(
      "BLOCKED executor received a non-exact local correctness schedule",
    );
  }
  await mkdir(options.output);
  const manifestPath = resolve(options.output, manifestFilename);
  const manifest = {
    schema_version: 1,
    receipt_type: localCorrectnessReceiptTypeV6,
    evidence_scope: localCorrectnessEvidenceScopeV6,
    status: "running",
    complete: false,
    decision_eligible: false,
    paid_qualification: false,
    nvidia_required: false,
    benefit_metrics_eligible: false,
    correctness_passed: false,
    protocol_version: plan.protocol_version,
    scenario_contract_version: plan.scenario_contract_version,
    manifest_id: plan.manifest_id,
    workload_byte_sha256: plan.workload_byte_sha256,
    schedule: {
      launch_count: 24,
      semantic_launch_count: 22,
      property_launch_count: 2,
      seed: options.seed,
      serial_concurrency: 1,
    },
    local_host_diagnostics: {
      physical_memory_bytes: totalmem(),
      display: environment.DISPLAY,
      dbus_session_present: Boolean(environment.DBUS_SESSION_BUS_ADDRESS),
      xdg_current_desktop: environment.XDG_CURRENT_DESKTOP ?? null,
      gtk_use_portal: environment.GTK_USE_PORTAL ?? null,
      portal_chooser_compatibility:
        "not-preapproved; runner failures remain fail-closed and retained",
    },
    candidates: Object.fromEntries(
      ["electron", "gpui"].map((implementation) => [
        implementation,
        { sha256: candidates[implementation].sha256 },
      ]),
    ),
    fixtures,
    known_baseline_defects: [],
    launches: [],
    started_at: new Date().toISOString(),
  };
  const persist = () =>
    atomicWrite(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  await persist();
  try {
    for (const run of schedule) {
      const invocation = buildLocalCorrectnessInvocationV6(run, options);
      const candidateSeal = await sealCandidate(
        invocation,
        options,
        candidates,
      );
      // The child runner owns application cleanup and report persistence at
      // its timeout. Keep the outer process-group deadline behind that work.
      const launched = await runRunner(
        invocation,
        options.timeoutMs + runnerCleanupMarginMs,
      );
      let rawArtifact = null;
      try {
        rawArtifact = await fileArtifact(invocation.raw_report_path);
      } catch {
        // The retained launch record below reports the missing raw artifact.
      }
      let assessment;
      try {
        assessment = launched.report
          ? assessRun({
              rawReport: launched.report,
              run,
              v4Workload,
              v4Plan,
              v5Workload,
              candidateArtifactSha256: candidates[run.implementation].sha256,
            })
          : {
              passed: false,
              failures: ["runner did not retain a parseable raw report"],
              receipts: [],
              correctness_passed: false,
              known_baseline_defect_id: null,
              hard_report: null,
            };
      } catch (error) {
        assessment = {
          passed: false,
          failures: [
            `local correctness assessment failed: ${error?.message ?? error}`,
          ],
          receipts: [],
          correctness_passed: false,
          known_baseline_defect_id: null,
          hard_report: null,
        };
      }
      let hardArtifact = null;
      if (assessment.hard_report) {
        await writeFile(
          invocation.hard_report_path,
          `${JSON.stringify(assessment.hard_report, null, 2)}\n`,
        );
        hardArtifact = await fileArtifact(invocation.hard_report_path);
      }
      const passed =
        launched.exit_code === 0 &&
        launched.timed_out !== true &&
        rawArtifact !== null &&
        assessment.passed === true;
      const failures = [
        ...(launched.exit_code === 0
          ? []
          : [`runner exit code ${launched.exit_code}`]),
        ...(launched.timed_out ? ["runner timed out"] : []),
        ...(rawArtifact ? [] : ["raw report is missing"]),
        ...assessment.failures,
      ];
      const { report: _report, ...retainedOutcome } = launched;
      const retained = {
        ...invocation,
        ...retainedOutcome,
        candidate_prelaunch_seal: candidateSeal,
        passed,
        failures,
        correctness_passed: assessment.correctness_passed,
        known_baseline_defect_id: assessment.known_baseline_defect_id,
        command_receipts: assessment.receipts,
        raw_report_sha256: rawArtifact?.sha256 ?? null,
        raw_report_bytes: rawArtifact?.bytes ?? null,
        hard_report_sha256: hardArtifact?.sha256 ?? null,
        hard_report_bytes: hardArtifact?.bytes ?? null,
      };
      manifest.launches.push(retained);
      if (assessment.known_baseline_defect_id) {
        manifest.known_baseline_defects.push({
          implementation: run.implementation,
          journey: run.journey,
          component: run.component,
          known_baseline_defect_id: assessment.known_baseline_defect_id,
        });
      }
      await persist();
      if (!passed) {
        throw new Error(
          `BLOCKED ${invocation.identity}: ${failures.join("; ")}`,
        );
      }
      if (
        options.cooldownMs > 0 &&
        manifest.launches.length < schedule.length
      ) {
        await delay(options.cooldownMs);
      }
    }
    manifest.complete = true;
    manifest.correctness_passed = manifest.launches.every(
      ({ correctness_passed: passed }) => passed === true,
    );
    manifest.status = manifest.correctness_passed
      ? "completed-all-correctness-passed"
      : "completed-with-known-baseline-defects";
    manifest.ended_at = new Date().toISOString();
    await persist();
    return manifest;
  } catch (error) {
    manifest.complete = false;
    manifest.status = "failed-closed";
    manifest.failure = error?.message ?? String(error);
    manifest.ended_at = new Date().toISOString();
    await persist();
    throw error;
  }
}

async function prepareLocalCorrectnessV6(options, plan, schedule) {
  const [v4Workload, v5Workload] = await Promise.all([
    loadMaterializedComparisonWorkloadV4(),
    loadMaterializedComparisonWorkloadV5(),
  ]);
  const [fixtures, candidates] = await Promise.all([
    verifyLocalCorrectnessFixturesV6({
      workload: v5Workload,
      schedule,
      fixtures: options.fixtures,
    }),
    (async () =>
      validateExactCandidateHashesV5(
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
      ))(),
  ]);
  return {
    plan,
    schedule,
    options,
    candidates,
    fixtures,
    v4Workload,
    v4Plan: buildV4ComparisonPlan(v4Workload),
    v5Workload,
  };
}

function usage() {
  return `Usage: node run-local-correctness-v6.mjs \\
  --output <new-directory> \\
  --electron <candidate-executable> \\
  --gpui-binary <candidate-executable> \\
  --electron-candidate-artifact <manifest.json> \\
  --gpui-candidate-artifact <manifest.json> \\
  --electron-candidate-sha256 <sha256> \\
  --gpui-candidate-sha256 <sha256> \\
  --fixture <id>=<path>  # repeat for the exact four correctness fixtures

This launches only the 24 frozen V6 correctness runs. It is local,
non-decision evidence and never replaces paid-GPU qualification.
`;
}

async function main() {
  const options = parseLocalCorrectnessArgumentsV6(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(usage());
    return;
  }
  const { workload, byte_sha256: byteSha256 } =
    await loadComparisonWorkloadV6();
  const plan = buildV6ComparisonPlan(workload, byteSha256);
  const schedule = buildLocalCorrectnessScheduleV6(plan, {
    seed: options.seed,
  });
  const result = await executeLocalCorrectnessV6(
    await prepareLocalCorrectnessV6(options, plan, schedule),
  );
  process.stdout.write(
    `${JSON.stringify(
      {
        receipt_type: result.receipt_type,
        status: result.status,
        correctness_passed: result.correctness_passed,
        known_baseline_defects: result.known_baseline_defects,
        manifest_path: resolve(options.output, manifestFilename),
      },
      null,
      2,
    )}\n`,
  );
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
) {
  try {
    await main();
  } catch (error) {
    process.stderr.write(
      `run-local-correctness-v6: ${error?.message ?? error}\n`,
    );
    process.exitCode = 1;
  }
}
