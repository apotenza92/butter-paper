#!/usr/bin/env node

import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { access, lstat, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { longbridgeCompatProfile } from "./compat-evidence-validator.mjs";
import {
  componentDevelopmentCandidateProfile,
  validateComponentDevelopmentCandidate,
} from "./component-development-candidate.mjs";
import { loadMaterializedComparisonWorkloadV4 } from "./comparison-workload-v4.mjs";
import { loadMaterializedComparisonWorkloadV5 } from "./comparison-workload-v5.mjs";
import { preflightRequiredCgroupV2Accounting } from "./linux-cgroup.mjs";
import { compareViewStateReceiptsV5 } from "./matched-view-state-v5.mjs";
import { assessV5Launch } from "./run-paired-v5.mjs";
import { runRunnerV6 } from "./run-paired-v6.mjs";
import { commonX11DamageTimingBoundaryPassedV6 } from "./x11-damage-observer.mjs";

const performanceDirectory = path.dirname(fileURLToPath(import.meta.url));
const repositoryDirectory = path.resolve(performanceDirectory, "../../..");

export const componentShortQualificationProfile =
  "bp-perf-component-short-qualification-v1";
export const acceptedComponentCandidateFileSha256 =
  "717f258375eee783317389011bcbfa99bf38b14567491c8b17524915b254d570";
export const acceptedComponentManifestSha256 =
  "26174855ac7f11f514c8d948d5a5bdf879c75571f4aac59700d15e35594bca4a";
export const acceptedComponentBinarySha256 =
  "c1f28ef31f3f6da6ce8373d7e78edca34abef15212fbee7c51504b4cb382e26a";
export const acceptedComponentWorkerSha256 =
  "b441c721a01fcfb137289ff86b61b6f3bb9cd3338523178b355d91af3528cb6e";
export const acceptedPdfiumSha256 =
  "f728930966f503652b92acc89b9374a2eeca00ce42e26dccd3e4b5c5161b2d64";
export const acceptedElectronCandidateFileSha256 =
  "5cc380e9d0ede2d6ced785cbb404e114a937d03eadad50bdd383ade8e64c16c0";
export const acceptedElectronExecutableSha256 =
  "84e1078c8f38659f3785fec90b68ff47dcb7b5ed94e97e4e4cc6dd3290758991";

const requiredCommands = Object.freeze([
  "cc",
  "convert",
  "nvidia-smi",
  "vulkaninfo",
  "xdotool",
  "xdpyinfo",
  "xprop",
]);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function resolveSealedPath(repositoryRoot, relativePath) {
  if (
    typeof relativePath !== "string" ||
    relativePath.length === 0 ||
    path.isAbsolute(relativePath) ||
    relativePath.includes("\\")
  ) {
    throw new Error("candidate artifact path is not repository-relative");
  }
  const resolved = path.resolve(repositoryRoot, relativePath);
  const relative = path.relative(repositoryRoot, resolved);
  if (relative === ".." || relative.startsWith(`..${path.sep}`)) {
    throw new Error("candidate artifact path escapes the repository");
  }
  return resolved;
}

async function validateArtifactRecord(record, repositoryRoot, label) {
  if (
    !Number.isInteger(record?.bytes) ||
    !/^[a-f0-9]{64}$/.test(record?.sha256 ?? "")
  ) {
    throw new Error(`${label} record is incomplete`);
  }
  const artifactPath = resolveSealedPath(repositoryRoot, record.path);
  const metadata = await lstat(artifactPath);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`${label} is not a regular file`);
  }
  const bytes = await readFile(artifactPath);
  if (bytes.length !== record.bytes || sha256(bytes) !== record.sha256) {
    throw new Error(`${label} hash or byte count differs`);
  }
}

export async function authenticateAcceptedComponentCandidate({
  candidatePath,
  repositoryRoot = repositoryDirectory,
}) {
  if (path.resolve(repositoryRoot) !== repositoryDirectory) {
    throw new Error("component candidate repository root is not exact");
  }
  const bytes = await readFile(path.resolve(candidatePath));
  const fileSha256 = sha256(bytes);
  if (fileSha256 !== acceptedComponentCandidateFileSha256) {
    throw new Error("accepted component candidate file hash differs");
  }
  const manifest = JSON.parse(bytes.toString("utf8"));
  if (
    manifest.profile !== componentDevelopmentCandidateProfile ||
    manifest.manifestSha256 !== acceptedComponentManifestSha256 ||
    manifest.developmentOnly !== true ||
    manifest.productionApproved !== false ||
    manifest.packaged !== false ||
    manifest.optimized !== false ||
    manifest.timingEligible !== false ||
    manifest.runtime?.binary?.sha256 !== acceptedComponentBinarySha256 ||
    manifest.runtime?.worker?.sha256 !== acceptedComponentWorkerSha256 ||
    manifest.runtime?.pdfium?.sha256 !== acceptedPdfiumSha256
  ) {
    throw new Error("accepted component candidate classification differs");
  }
  await validateComponentDevelopmentCandidate(manifest);
  return { file_sha256: fileSha256, manifest };
}

export async function authenticateAcceptedElectronCandidate({
  candidatePath,
  repositoryRoot = repositoryDirectory,
}) {
  if (path.resolve(repositoryRoot) !== repositoryDirectory) {
    throw new Error("Electron candidate repository root is not exact");
  }
  const bytes = await readFile(path.resolve(candidatePath));
  const fileSha256 = sha256(bytes);
  if (fileSha256 !== acceptedElectronCandidateFileSha256) {
    throw new Error("accepted Electron candidate file hash differs");
  }
  const manifest = JSON.parse(bytes.toString("utf8"));
  if (
    manifest.schema_version !== 1 ||
    manifest.candidate_profile !== "bp-perf-v4-optimized-candidate-1" ||
    manifest.implementation !== "electron" ||
    manifest.runtime_profile !==
      "vite-production-bundles-unpackaged-electron-runtime" ||
    manifest.build?.node_env !== "production" ||
    manifest.build?.packaged !== false ||
    manifest.executable?.sha256 !== acceptedElectronExecutableSha256
  ) {
    throw new Error("accepted Electron candidate classification differs");
  }
  const records = [
    manifest.executable,
    ...manifest.bundle.files,
    ...manifest.runtime_dependency_closure.packages.flatMap(
      ({ files }) => files,
    ),
    manifest.runtime_artifact_closure.desktop_dev_provenance.artifact,
  ];
  const paths = new Set();
  for (const record of records) {
    if (paths.has(record.path)) {
      throw new Error(`duplicate Electron candidate artifact: ${record.path}`);
    }
    paths.add(record.path);
    await validateArtifactRecord(
      record,
      repositoryRoot,
      `Electron candidate artifact ${record.path}`,
    );
  }
  return { file_sha256: fileSha256, manifest };
}

export function parseComponentShortQualificationArguments(argv) {
  if (
    argv.length !== 2 ||
    argv[0] !== "--output" ||
    !path.isAbsolute(argv[1] ?? "") ||
    /[\0-\x1f\x7f;&|`$<>]/.test(argv[1] ?? "")
  ) {
    throw new Error(
      "fixed component qualification usage: component-short-qualification.mjs --output /absolute/fresh/path",
    );
  }
  return { output: path.resolve(argv[1]) };
}

export function componentShortQualificationPreflight({
  platform = process.platform,
  architecture = process.arch,
  environment = process.env,
  availableCommands = new Set(),
} = {}) {
  const blockers = [];
  if (!environment.DISPLAY) blockers.push("DISPLAY is required");
  if (!environment.DBUS_SESSION_BUS_ADDRESS) {
    blockers.push("one shared D-Bus desktop session is required");
  }
  if (platform !== "linux") blockers.push("Linux is required");
  if (architecture !== "x64") blockers.push("x86_64 is required");
  for (const command of requiredCommands) {
    if (!availableCommands.has(command)) {
      blockers.push(`required host command is missing: ${command}`);
    }
  }
  return { ready: blockers.length === 0, blockers, launches: 0 };
}

export function buildComponentShortQualificationPlan({ root, output }) {
  const fixture = path.resolve(
    root,
    "fixtures/public-fixtures-v1/bp-single-page-v1.pdf",
  );
  const runnerDirectory = path.resolve(
    root,
    "experiments/gpui-migration/performance",
  );
  const node = path.resolve(root, "runtime/node");
  const common = [
    "--scenario",
    "open-pdf",
    "--pdf",
    fixture,
    "--iterations",
    "1",
    "--timeout-ms",
    "120000",
    "--input-lane",
    "native-x11-xtest",
  ];
  const environment = Object.freeze({
    BP_PERF_REQUIRE_NVIDIA: "1",
    BP_PERF_COMMON_DAMAGE_OBSERVER: "1",
    BP_PERF_V5_REFERENCE_CROP_DIR: path.resolve(
      runnerDirectory,
      "fixtures/reference-crops-v5",
    ),
  });
  const electronOutput = path.resolve(output, "electron-open-pdf.json");
  const gpuiOutput = path.resolve(output, "gpui-component-open-pdf.json");
  return {
    schema_version: 1,
    profile: componentShortQualificationProfile,
    development_only: true,
    timing_eligible: false,
    v6_acceptance: false,
    implementation_order: ["electron", "gpui"],
    launches: [
      {
        implementation: "electron",
        identity: "component-qualification-electron-open-pdf",
        phase: "qualification",
        journey: "small-shell-open",
        component: "open-pdf",
        fixture_ids: ["bp-single-page-v1"],
        input_lane: "native-x11-xtest",
        hard_component: false,
        benefit_metrics_eligible: true,
        output: electronOutput,
        raw_report_path: electronOutput,
        hard_report_path: null,
        environment,
        argv: [
          node,
          path.resolve(runnerDirectory, "electron-runner.mjs"),
          ...common,
          "--output",
          electronOutput,
          "--electron",
          path.resolve(root, "node_modules/electron/dist/electron"),
          "--v4-scenario",
          "small-shell-open",
          "--v6-scenario",
          "small-shell-open",
        ],
      },
      {
        implementation: "gpui",
        identity: "component-qualification-gpui-open-pdf",
        phase: "qualification",
        journey: "small-shell-open",
        component: "open-pdf",
        fixture_ids: ["bp-single-page-v1"],
        input_lane: "native-x11-xtest",
        hard_component: false,
        benefit_metrics_eligible: true,
        output: gpuiOutput,
        raw_report_path: gpuiOutput,
        hard_report_path: null,
        environment,
        argv: [
          node,
          path.resolve(runnerDirectory, "gpui-runner.mjs"),
          ...common,
          "--output",
          gpuiOutput,
          "--evidence-directory",
          path.resolve(output, "gpui-component-evidence"),
          "--binary",
          path.resolve(
            root,
            "experiments/gpui-migration/.build-targets/gpui-component-portable-u24/debug/component_story",
          ),
          "--v4-scenario",
          "small-shell-open",
          "--v6-scenario",
          "small-shell-open",
          "--compat-profile",
          longbridgeCompatProfile,
        ],
      },
    ],
  };
}

export async function availableComponentQualificationCommands(
  environment = process.env,
) {
  const available = new Set();
  const directories = String(environment.PATH ?? "")
    .split(path.delimiter)
    .filter(Boolean);
  for (const command of requiredCommands) {
    for (const directory of directories) {
      try {
        await access(path.resolve(directory, command), constants.X_OK);
        available.add(command);
        break;
      } catch {
        // Continue through the explicit PATH entries.
      }
    }
  }
  return available;
}

async function requireFreshOutput(output) {
  try {
    await lstat(output);
    throw new Error("component qualification output must be fresh");
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  await mkdir(output, { recursive: false, mode: 0o700 });
}

async function retainQualificationResult(output, result) {
  await writeFile(
    path.resolve(output, "component-short-qualification.json"),
    `${JSON.stringify(result, null, 2)}\n`,
    { mode: 0o600 },
  );
  return result;
}

function runnerStructuralBlockers(launch, outcome) {
  const report = outcome?.report;
  const iteration = report?.iterations?.[0];
  const blockers = [];
  if (outcome?.exit_code !== 0) {
    blockers.push(`runner exit code ${outcome?.exit_code}`);
  }
  if (outcome?.timed_out === true) blockers.push("runner timed out");
  if (
    report?.schema_version !== 1 ||
    report?.implementation !== launch.implementation ||
    report?.scenario !== "open-pdf" ||
    report?.requested_iterations !== 1 ||
    report?.summary?.successful_iterations !== 1 ||
    report?.summary?.failed_iterations !== 0 ||
    report?.iterations?.length !== 1 ||
    iteration?.application_success !== true
  ) {
    blockers.push("runner report did not pass one exact open-pdf iteration");
  }
  if (
    iteration?.gpu?.qualification?.required !== true ||
    iteration?.gpu?.qualification?.passed !== true ||
    iteration?.active_gpu_adapter?.passed !== true ||
    iteration?.active_gpu_adapter?.implementation !== launch.implementation
  ) {
    blockers.push("runner did not prove its active NVIDIA adapter");
  }
  if (
    !commonX11DamageTimingBoundaryPassedV6(
      iteration?.native_input?.evidence?.common_benefit_timing_boundary,
    )
  ) {
    blockers.push("runner did not prove the common X11 Damage boundary");
  }
  if (
    launch.implementation === "gpui" &&
    (report?.compat_profile !== longbridgeCompatProfile ||
      iteration?.compat_evidence_validation?.passed !== true ||
      iteration?.compat_resource_cleanup?.passed !== true)
  ) {
    blockers.push("GPUI Longbridge compatibility evidence did not pass");
  }
  return blockers;
}

async function reportArtifact(reportPath) {
  const bytes = await readFile(reportPath);
  return { path: reportPath, bytes: bytes.length, sha256: sha256(bytes) };
}

export async function runComponentShortQualification({
  root = repositoryDirectory,
  output,
  componentCandidatePath = path.resolve(
    root,
    "candidates/component-development-candidate.json",
  ),
  electronCandidatePath = path.resolve(
    root,
    "candidates/electron-optimized-candidate-v4.json",
  ),
  environment = process.env,
  platform = process.platform,
  architecture = process.arch,
  availableCommands,
  launchRunner = runRunnerV6,
  cgroupPreflight = preflightRequiredCgroupV2Accounting,
}) {
  const [componentCandidate, electronCandidate] = await Promise.all([
    authenticateAcceptedComponentCandidate({
      candidatePath: componentCandidatePath,
      repositoryRoot: root,
    }),
    authenticateAcceptedElectronCandidate({
      candidatePath: electronCandidatePath,
      repositoryRoot: root,
    }),
  ]);
  const observedCommands =
    availableCommands ??
    (await availableComponentQualificationCommands(environment));
  const preflight = componentShortQualificationPreflight({
    platform,
    architecture,
    environment,
    availableCommands: observedCommands,
  });
  await requireFreshOutput(output);
  const authentication = {
    component_candidate_file_sha256: componentCandidate.file_sha256,
    component_manifest_sha256: componentCandidate.manifest.manifestSha256,
    electron_candidate_file_sha256: electronCandidate.file_sha256,
    electron_executable_sha256: electronCandidate.manifest.executable.sha256,
  };
  if (!preflight.ready) {
    return retainQualificationResult(output, {
      schema_version: 1,
      profile: componentShortQualificationProfile,
      status: "BLOCKED",
      development_only: true,
      timing_eligible: false,
      v6_acceptance: false,
      authentication,
      preflight,
      launches: 0,
    });
  }
  let cgroupAccounting;
  try {
    cgroupAccounting = await cgroupPreflight();
  } catch (error) {
    return retainQualificationResult(output, {
      schema_version: 1,
      profile: componentShortQualificationProfile,
      status: "BLOCKED",
      development_only: true,
      timing_eligible: false,
      v6_acceptance: false,
      authentication,
      preflight: {
        ...preflight,
        ready: false,
        blockers: [`cgroup v2 accounting preflight failed: ${error.message}`],
        launches: 0,
      },
      launches: 0,
    });
  }
  if (cgroupAccounting?.ready !== true) {
    return retainQualificationResult(output, {
      schema_version: 1,
      profile: componentShortQualificationProfile,
      status: "BLOCKED",
      development_only: true,
      timing_eligible: false,
      v6_acceptance: false,
      authentication,
      preflight: {
        ...preflight,
        ready: false,
        blockers: [
          ...(cgroupAccounting?.blockers ?? [
            "cgroup v2 accounting preflight did not pass",
          ]),
        ],
        cgroup_accounting: cgroupAccounting,
        launches: 0,
      },
      launches: 0,
    });
  }
  const plan = buildComponentShortQualificationPlan({ root, output });
  const [v4Workload, v5Workload] = await Promise.all([
    loadMaterializedComparisonWorkloadV4(),
    loadMaterializedComparisonWorkloadV5(),
  ]);
  const retained = [];
  for (const launch of plan.launches) {
    const outcome = await launchRunner(launch, 135_000);
    const blockers = runnerStructuralBlockers(launch, outcome);
    let assessment = null;
    if (outcome.report) {
      assessment = assessV5Launch({
        workload: v5Workload,
        v4Workload,
        rawReport: outcome.report,
        run: launch,
        candidateArtifactSha256:
          launch.implementation === "electron"
            ? electronCandidate.file_sha256
            : componentCandidate.file_sha256,
        commonBoundaryValidator: commonX11DamageTimingBoundaryPassedV6,
      });
      blockers.push(...assessment.failures);
    }
    if (blockers.length > 0 || assessment?.passed !== true) {
      return retainQualificationResult(output, {
        schema_version: 1,
        profile: componentShortQualificationProfile,
        status: "BLOCKED",
        development_only: true,
        timing_eligible: false,
        v6_acceptance: false,
        authentication,
        preflight: { ...preflight, cgroup_accounting: cgroupAccounting },
        launches: retained.length + 1,
        retained_launches: retained,
        failed_launch: {
          implementation: launch.implementation,
          blockers: [...new Set(blockers)],
        },
      });
    }
    retained.push({
      implementation: launch.implementation,
      passed: true,
      raw_report: await reportArtifact(launch.raw_report_path),
      view_state_receipt: assessment.view_state_receipt,
      active_gpu_adapter: structuredClone(
        outcome.report.iterations[0].active_gpu_adapter,
      ),
    });
  }
  const matchedViewState = compareViewStateReceiptsV5(
    retained[0].view_state_receipt,
    retained[1].view_state_receipt,
  );
  const adapterMatches =
    retained[0].active_gpu_adapter.device_uuid ===
    retained[1].active_gpu_adapter.device_uuid;
  const passed = matchedViewState.passed === true && adapterMatches;
  return retainQualificationResult(output, {
    schema_version: 1,
    profile: componentShortQualificationProfile,
    status: passed ? "PASSED" : "BLOCKED",
    development_only: true,
    timing_eligible: false,
    v6_acceptance: false,
    authentication,
    preflight: { ...preflight, cgroup_accounting: cgroupAccounting },
    launches: retained.length,
    retained_launches: retained,
    matched_view_state: matchedViewState,
    active_adapter_matches: adapterMatches,
    blockers: [
      ...(matchedViewState.failures ?? []),
      ...(adapterMatches
        ? []
        : ["Electron and GPUI used different GPU adapters"]),
    ],
  });
}

async function main() {
  const options = parseComponentShortQualificationArguments(
    process.argv.slice(2),
  );
  const result = await runComponentShortQualification({
    root: repositoryDirectory,
    output: options.output,
  });
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  if (result.status !== "PASSED") process.exitCode = 2;
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === path.resolve(process.argv[1])
) {
  await main();
}
