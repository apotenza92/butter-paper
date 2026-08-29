#!/usr/bin/env node

import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import {
  mkdir,
  readdir,
  readFile,
  readlink,
  realpath,
  rename,
  stat,
  writeFile,
} from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import { loadMaterializedComparisonWorkloadV4 } from "./comparison-workload-v4.mjs";
import { loadMaterializedComparisonWorkloadV5 } from "./comparison-workload-v5.mjs";
import {
  assessDirectionalSafetyTrialV7,
  buildDirectionalSafetyScheduleV7,
  directionalSafetyCooldownMsV7,
  directionalSafetyExpectedPlanSha256V7,
  directionalSafetyLaunchCountV7,
  directionalSafetyPlanSha256V7,
  directionalSafetyPlanV7,
  directionalSafetyProtocolVersionV7,
} from "./directional-safety-v7.mjs";
import {
  revalidateOptimizedCandidateLaunchV4,
  validateOptimizedCandidatesV4,
} from "./optimized-candidates-v4.mjs";
import {
  buildLaunchBindingV5,
  validateExactCandidateHashesV5,
  verifyV5FixturesAndReferences,
} from "./run-paired-v5.mjs";
import { buildV4ComparisonPlan, canonicalSha256 } from "./run-paired-v4.mjs";
import {
  assertSessionIdentityV6,
  assessSemanticCorrectnessV6,
  buildRunnerInvocationV6,
  collectPaidGpuEnvironmentV6,
  collectSessionIdentityV6,
  expectedWorkloadByteSha256V6,
  loadComparisonWorkloadV6,
  parseV6Arguments,
  runRunnerV6,
  validateCandidateLaunchSealV6,
  validateQualificationReceiptV6,
} from "./run-paired-v6.mjs";

export const directionalSafetyTaskLimitMaximumMsV7 = 15 * 60_000;
export const directionalSafetyLeaseTtlMaximumMsV7 = 30 * 60_000;
const execFileAsync = promisify(execFile);
const gpuRecoveryAllowanceMibV7 = 128;

function safeName(value) {
  return value.replace(/[^a-zA-Z0-9._-]+/g, "-");
}

export function buildDirectionalSafetyInvocationV7(run, options) {
  const invocation = buildRunnerInvocationV6(run, options);
  invocation.identity = `directional-safety-pair${run.pair}-${run.implementation}-high-zoom-pan`;
  invocation.raw_report_path = resolve(
    options.output,
    `${safeName(invocation.identity)}.json`,
  );
  const outputIndex = invocation.argv.indexOf("--output");
  if (outputIndex < 0 || outputIndex + 1 >= invocation.argv.length) {
    throw new Error("directional safety invocation has no output path");
  }
  invocation.argv[outputIndex + 1] = invocation.raw_report_path;
  invocation.command = invocation.argv
    .map((value) => JSON.stringify(value))
    .join(" ");
  return invocation;
}

export async function executeDirectionalSafetyLaunchesV7(
  schedule,
  { runLaunch, onResult = async () => {} },
) {
  const results = [];
  for (const run of schedule) {
    const result = await runLaunch(run);
    results.push(result);
    await onResult(result, results);
    if (result?.outcome === "ABORT") {
      throw new Error(
        `ABORTED directional safety structural evidence failure: ${result.structural_failures?.join("; ") || "unknown"}`,
      );
    }
  }
  return results;
}

async function atomicWrite(path, bytes) {
  const temporary = `${path}.tmp-${process.pid}`;
  await writeFile(temporary, bytes);
  await rename(temporary, path);
}

async function fileArtifact(path) {
  const bytes = await readFile(path);
  const metadata = await stat(path);
  return {
    path,
    bytes: metadata.size,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  };
}

function qualificationEnvironmentBinding(environmentEvidence) {
  const { raw: _raw, ...binding } = environmentEvidence;
  return binding;
}

async function candidateSurvivorProcesses(candidatePaths) {
  const canonicalCandidates = new Set(
    await Promise.all(candidatePaths.map((path) => realpath(path))),
  );
  const survivors = Object.fromEntries(
    [...canonicalCandidates].map((path) => [path, []]),
  );
  for (const entry of await readdir("/proc", { withFileTypes: true })) {
    if (!entry.isDirectory() || !/^\d+$/.test(entry.name)) continue;
    try {
      const executable = (await readlink(`/proc/${entry.name}/exe`)).replace(
        / \(deleted\)$/,
        "",
      );
      if (canonicalCandidates.has(executable)) {
        survivors[executable].push(Number(entry.name));
      }
    } catch {
      // The process can exit between listing /proc and reading its executable.
    }
  }
  return survivors;
}

async function collectHostHealthV7({
  expectedEnvironmentBinding,
  qualificationBaselineMemoryMaxMib,
  candidatePaths,
}) {
  const failures = [];
  let environmentBinding = null;
  let gpuMemoryUsedMib = null;
  const survivorProcesses = {};
  try {
    environmentBinding = qualificationEnvironmentBinding(
      await collectPaidGpuEnvironmentV6(),
    );
    assertSessionIdentityV6(
      expectedEnvironmentBinding.session_identity,
      environmentBinding.session_identity,
    );
    if (
      JSON.stringify(environmentBinding) !==
      JSON.stringify(expectedEnvironmentBinding)
    ) {
      failures.push(
        "qualified host, driver, Vulkan, X11, or D-Bus binding changed",
      );
    }
  } catch (error) {
    failures.push(
      `host/session health probe failed: ${error?.message ?? error}`,
    );
  }
  try {
    const { stdout } = await execFileAsync("nvidia-smi", [
      "--query-gpu=memory.used",
      "--format=csv,noheader,nounits",
    ]);
    const values = stdout
      .trim()
      .split(/\s+/)
      .map(Number)
      .filter(Number.isFinite);
    if (values.length !== 1) {
      failures.push("GPU recovery probe did not return exactly one adapter");
    } else if (
      values[0] >
      qualificationBaselineMemoryMaxMib + gpuRecoveryAllowanceMibV7
    ) {
      failures.push(
        `GPU baseline did not recover: ${values[0]} MiB exceeds ${qualificationBaselineMemoryMaxMib + gpuRecoveryAllowanceMibV7} MiB`,
      );
    } else {
      [gpuMemoryUsedMib] = values;
    }
  } catch (error) {
    failures.push(`GPU recovery probe failed: ${error?.message ?? error}`);
  }
  try {
    Object.assign(
      survivorProcesses,
      await candidateSurvivorProcesses(candidatePaths),
    );
    for (const [path, matches] of Object.entries(survivorProcesses)) {
      if (matches.length > 0) {
        failures.push(`candidate survivor process remains: ${path}`);
      }
    }
  } catch (error) {
    failures.push(
      `candidate survivor probe failed: ${error?.message ?? error}`,
    );
  }
  return {
    passed: failures.length === 0,
    failures,
    environment_binding: environmentBinding,
    gpu_memory_used_mib: gpuMemoryUsedMib,
    survivor_processes: survivorProcesses,
  };
}

async function loadAuthenticatedQualificationV7({
  workload,
  options,
  candidates,
  verified,
  environmentBinding,
}) {
  const receiptPath = options.qualificationReceipt;
  const [bytes, checksum] = await Promise.all([
    readFile(receiptPath),
    readFile(`${receiptPath}.sha256`, "utf8"),
  ]);
  const sha256 = createHash("sha256").update(bytes).digest("hex");
  if (checksum.trim() !== `${sha256}  ${receiptPath.split("/").at(-1)}`) {
    throw new Error("BLOCKED qualification receipt checksum is invalid");
  }
  const receipt = JSON.parse(bytes);
  const validation = validateQualificationReceiptV6({
    receipt,
    workload,
    candidates,
    verified,
    environmentBinding,
  });
  let baselineMemoryMaxMib = 0;
  for (const launch of receipt.launches ?? []) {
    const rawBytes = await readFile(launch.raw_report_path);
    const observed = createHash("sha256").update(rawBytes).digest("hex");
    if (observed !== launch.raw_report_sha256) {
      throw new Error(
        `BLOCKED qualification raw report differs: ${launch.implementation}`,
      );
    }
    const rawReport = JSON.parse(rawBytes);
    const baselineValues =
      rawReport?.iterations?.[0]?.gpu?.baseline?.samples?.map(
        ({ memory_used_mib: value }) => value,
      ) ?? [];
    if (
      baselineValues.length < 3 ||
      baselineValues.some((value) => !Number.isFinite(value))
    ) {
      throw new Error(
        `BLOCKED qualification baseline memory evidence is invalid: ${launch.implementation}`,
      );
    }
    baselineMemoryMaxMib = Math.max(baselineMemoryMaxMib, ...baselineValues);
  }
  return {
    path: receiptPath,
    sha256,
    payload_sha256: validation.payload_sha256,
    environment_binding: receipt.environment_binding,
    baseline_memory_max_mib: baselineMemoryMaxMib,
  };
}

function candidateOptions(options) {
  return {
    electronManifestPath: options.electronCandidateArtifact,
    gpuiManifestPath: options.gpuiCandidateArtifact,
    electronExecutable: options.electron,
    gpuiBinary: options.gpuiBinary,
  };
}

async function sealCandidateLaunchV7(invocation, options, candidates) {
  return validateCandidateLaunchSealV6({
    seal: await revalidateOptimizedCandidateLaunchV4({
      launchId: invocation.identity,
      ...candidateOptions(options),
    }),
    launchId: invocation.identity,
    candidates,
  });
}

function validateCurrentAdapterV7(report, implementation, environmentBinding) {
  const failures = [];
  const adapter = report?.iterations?.[0]?.active_gpu_adapter;
  const expectedSource =
    implementation === "electron"
      ? "chromium-system-info-active-gl-renderer"
      : "gpui-window-gpu-specs";
  if (
    adapter?.receipt_type !== "bp-active-renderer-adapter-v1" ||
    adapter?.active !== true ||
    adapter?.passed !== true ||
    adapter?.implementation !== implementation ||
    adapter?.device_uuid !== environmentBinding.adapter.uuid ||
    adapter?.selection_source !== expectedSource
  ) {
    failures.push("active renderer is not the qualified NVIDIA adapter");
  }
  const indexes = new Set(
    ["baseline", "run", "baseline_adjusted"].flatMap(
      (phase) =>
        report?.iterations?.[0]?.gpu?.[phase]?.samples?.map(
          ({ index }) => index,
        ) ?? [],
    ),
  );
  if (indexes.size !== 1 || !indexes.has(environmentBinding.adapter.index)) {
    failures.push("GPU samples do not use the qualified NVIDIA adapter index");
  }
  return failures;
}

function buildLaunchBindingV7({
  launchBindingV5,
  qualification,
  environmentBinding,
  candidateSeal,
}) {
  const payload = {
    ...structuredClone(launchBindingV5),
    protocol_version: directionalSafetyProtocolVersionV7,
    source_v6_workload_byte_sha256: expectedWorkloadByteSha256V6,
    qualification_receipt_sha256: qualification.sha256,
    qualification_payload_sha256: qualification.payload_sha256,
    dbus_session_address_sha256: environmentBinding.dbus_session_address_sha256,
    session_identity_sha256: canonicalSha256(
      environmentBinding.session_identity,
    ),
    candidate_prelaunch_seal: candidateSeal,
  };
  return { ...payload, evidence_sha256: canonicalSha256(payload) };
}

function validateLeaseOptionsV7(options) {
  if (
    !Number.isInteger(options.taskLimitMs) ||
    options.taskLimitMs > directionalSafetyTaskLimitMaximumMsV7 ||
    !Number.isInteger(options.leaseTtlMs) ||
    options.leaseTtlMs > directionalSafetyLeaseTtlMaximumMsV7 ||
    options.leaseTtlMs < options.taskLimitMs + options.cleanupGraceMs ||
    options.cooldownMs !== directionalSafetyCooldownMsV7 ||
    directionalSafetyPlanSha256V7 !== directionalSafetyExpectedPlanSha256V7
  ) {
    throw new Error(
      "BLOCKED V7 requires the frozen plan hash, 2-second cooldown, task <=15 minutes, and lease TTL <=30 minutes covering task plus cleanup grace",
    );
  }
}

export async function executeDirectionalSafetyV7(options) {
  validateLeaseOptionsV7(options);
  const startedMs = Date.now();
  const deadline = startedMs + options.taskLimitMs;
  const { workload } = await loadComparisonWorkloadV6();
  const [v5Workload, v4Workload] = await Promise.all([
    loadMaterializedComparisonWorkloadV5(),
    loadMaterializedComparisonWorkloadV4(),
  ]);
  const v4Plan = buildV4ComparisonPlan(v4Workload);
  const verified = await verifyV5FixturesAndReferences(v5Workload, options);
  const candidates = validateExactCandidateHashesV5(
    await validateOptimizedCandidatesV4(candidateOptions(options)),
    {
      electron: options.electronCandidateSha256,
      gpui: options.gpuiCandidateSha256,
    },
  );
  const currentEnvironment = await collectPaidGpuEnvironmentV6();
  const environmentBinding =
    qualificationEnvironmentBinding(currentEnvironment);
  const qualification = await loadAuthenticatedQualificationV7({
    workload,
    options,
    candidates,
    verified,
    environmentBinding,
  });
  await mkdir(options.output, { recursive: true });
  const manifestPath = resolve(
    options.output,
    "directional-safety-manifest-v7.json",
  );
  try {
    await readFile(manifestPath);
    throw new Error(
      "BLOCKED V7 output already contains a manifest; choose a fresh output directory",
    );
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  const schedule = buildDirectionalSafetyScheduleV7();
  const invocations = schedule.map((run) =>
    buildDirectionalSafetyInvocationV7(run, options),
  );
  let manifest = {
    schema_version: 1,
    protocol_version: directionalSafetyProtocolVersionV7,
    source_v6: {
      protocol_version: workload.protocol_version,
      scenario_contract_version: "bp-perf-v6-representative-1",
      manifest_id: workload.manifest_id,
      workload_byte_sha256: expectedWorkloadByteSha256V6,
      disposition: "failed-closed-and-not-reclassified",
    },
    directional_safety_plan: structuredClone(directionalSafetyPlanV7),
    directional_safety_plan_sha256: directionalSafetyPlanSha256V7,
    started_at: new Date(startedMs).toISOString(),
    settings: {
      task_limit_ms: options.taskLimitMs,
      cleanup_grace_ms: options.cleanupGraceMs,
      absolute_lease_ttl_ms: options.leaseTtlMs,
      absolute_task_deadline_at: new Date(deadline).toISOString(),
      hourly_price_usd: options.hourlyUsd,
      maximum_estimated_cost_usd:
        Math.ceil(
          (options.hourlyUsd * options.leaseTtlMs * 10_000) / 3_600_000,
        ) / 10_000,
    },
    candidates: structuredClone(candidates),
    qualification,
    environment_binding: environmentBinding,
    schedule,
    launches: [],
    trials: [],
    complete: false,
    outcome: "running",
    failure: null,
  };
  const candidatePaths = [options.electron, options.gpuiBinary];
  const persist = async () => {
    const bytes = `${JSON.stringify(manifest, null, 2)}\n`;
    const sha256 = createHash("sha256").update(bytes).digest("hex");
    await atomicWrite(manifestPath, bytes);
    await atomicWrite(
      `${manifestPath}.sha256`,
      `${sha256}  ${manifestPath.split("/").at(-1)}\n`,
    );
  };
  await persist();
  try {
    await executeDirectionalSafetyLaunchesV7(invocations, {
      runLaunch: async (invocation) => {
        const remainingMs = deadline - Date.now();
        if (remainingMs < options.timeoutMs + 15_000) {
          const trial = {
            pair: invocation.pair,
            implementation: invocation.implementation,
            outcome: "ABORT",
            structural_failures: ["V7 immutable task deadline reached"],
            product_failures: [],
            measurements: {},
            known_baseline_defect_id: null,
          };
          manifest.trials.push(trial);
          return trial;
        }
        assertSessionIdentityV6(
          environmentBinding.session_identity,
          await collectSessionIdentityV6(),
        );
        const preHealth = await collectHostHealthV7({
          expectedEnvironmentBinding: environmentBinding,
          qualificationBaselineMemoryMaxMib:
            qualification.baseline_memory_max_mib,
          candidatePaths,
        });
        if (!preHealth.passed) {
          const trial = {
            pair: invocation.pair,
            implementation: invocation.implementation,
            outcome: "ABORT",
            structural_failures: preHealth.failures.map(
              (failure) => `pre-launch health: ${failure}`,
            ),
            product_failures: [],
            measurements: {},
            known_baseline_defect_id: null,
          };
          manifest.trials.push(trial);
          return trial;
        }
        const candidateSeal = await sealCandidateLaunchV7(
          invocation,
          options,
          candidates,
        );
        const launched = await runRunnerV6(
          invocation,
          Math.min(options.timeoutMs + 15_000, remainingMs),
        );
        let launchBindingV5 = null;
        let launchBindingV7 = null;
        let semanticAssessment = null;
        const adapterFailures = launched.report
          ? validateCurrentAdapterV7(
              launched.report,
              invocation.implementation,
              environmentBinding,
            )
          : [];
        if (launched.report) {
          launchBindingV5 = buildLaunchBindingV5({
            invocation,
            launched,
            candidate: candidates[invocation.implementation],
            fixtureArtifacts: verified.fixtures,
          });
          launchBindingV7 = buildLaunchBindingV7({
            launchBindingV5,
            qualification,
            environmentBinding,
            candidateSeal,
          });
          launched.report.launch_binding_v5 = launchBindingV5;
          launched.report.launch_binding_v7 = launchBindingV7;
          await writeFile(
            invocation.raw_report_path,
            `${JSON.stringify(launched.report, null, 2)}\n`,
          );
          semanticAssessment = assessSemanticCorrectnessV6({
            rawReport: launched.report,
            run: invocation,
            v4Workload,
            v4Plan,
          });
        }
        const rawArtifact = launched.report
          ? await fileArtifact(invocation.raw_report_path)
          : null;
        await new Promise((resolvePromise) =>
          setTimeout(resolvePromise, directionalSafetyCooldownMsV7),
        );
        const postHealth = await collectHostHealthV7({
          expectedEnvironmentBinding: environmentBinding,
          qualificationBaselineMemoryMaxMib:
            qualification.baseline_memory_max_mib,
          candidatePaths,
        });
        const retainedLaunch = {
          ...invocation,
          started_at: launched.started_at,
          ended_at: launched.ended_at,
          started_monotonic_ms: launched.started_monotonic_ms,
          ended_monotonic_ms: launched.ended_monotonic_ms,
          exit_code: launched.exit_code,
          signal: launched.signal,
          spawn_error: launched.spawn_error,
          timed_out: launched.timed_out,
          termination: launched.termination,
          stdout: launched.stdout,
          stderr: launched.stderr,
          raw_report_sha256: rawArtifact?.sha256 ?? null,
          raw_report_bytes: rawArtifact?.bytes ?? null,
          launch_binding_v5: launchBindingV5,
          launch_binding_v7: launchBindingV7,
          candidate_prelaunch_seal: candidateSeal,
          pre_launch_health: preHealth,
          post_launch_health: postHealth,
        };
        manifest.launches.push(retainedLaunch);
        const trial = assessDirectionalSafetyTrialV7({
          launch: retainedLaunch,
          rawReport: launched.report,
          semanticAssessment,
        });
        trial.structural_failures = [
          ...new Set([
            ...trial.structural_failures,
            ...adapterFailures,
            ...(launched.termination?.bounded_wait_complete === true
              ? []
              : ["runner process group teardown was not bounded"]),
            ...(launched.report && rawArtifact?.sha256
              ? []
              : launched.timed_out ||
                  launched.signal != null ||
                  (Number.isInteger(launched.exit_code) &&
                    launched.exit_code !== 0)
                ? []
                : ["authenticated raw report artifact is absent"]),
            ...postHealth.failures.map(
              (failure) => `post-launch health: ${failure}`,
            ),
          ]),
        ];
        if (trial.structural_failures.length > 0) trial.outcome = "ABORT";
        manifest.trials.push(trial);
        return trial;
      },
      onResult: async () => {
        await persist();
      },
    });
    manifest.complete = true;
    manifest.outcome = "completed";
    manifest.ended_at = new Date().toISOString();
    manifest.actual_duration_ms = Date.now() - startedMs;
    await persist();
    return manifest;
  } catch (error) {
    manifest.complete = false;
    manifest.outcome = String(error?.message ?? error).startsWith("TIMED OUT")
      ? "timed-out"
      : "failed-closed";
    manifest.failure = error?.message ?? String(error);
    manifest.ended_at = new Date().toISOString();
    manifest.actual_duration_ms = Date.now() - startedMs;
    await persist();
    throw error;
  }
}

function usage() {
  return `Usage: node run-directional-safety-v7.mjs --execute --output <fresh-directory> [V6 candidate, qualification, fixture, lease options]\n\nRuns exactly six balanced fresh engineering high-zoom pairs. Product failures are retained and execution continues; structural evidence failures abort.\n`;
}

async function main() {
  if (process.argv.includes("--help") || process.argv.includes("-h")) {
    process.stdout.write(usage());
    return;
  }
  const options = parseV6Arguments(process.argv.slice(2));
  if (options.mode !== "execute") {
    throw new Error("--execute is required for V7 directional safety");
  }
  await executeDirectionalSafetyV7(options);
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
) {
  await main();
}
