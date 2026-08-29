#!/usr/bin/env node

import { createHash } from "node:crypto";
import {
  mkdir,
  mkdtemp,
  readFile,
  realpath,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

import {
  comparisonWorkloadArtifactHashV5,
  comparisonWorkloadByteHashV5,
  loadMaterializedComparisonWorkloadV5,
} from "./comparison-workload-v5.mjs";
import { loadMaterializedComparisonWorkloadV4 } from "./comparison-workload-v4.mjs";
import {
  createBalancedPairOrders,
  pairedLogRatioBootstrap,
} from "./decision-statistics.mjs";
import {
  canonicalSha256V5,
  decisionContractV5,
  decisionContractVersionV5,
  electronMultiDocumentMissingBenefitMetricsV5,
  gpuiMultiDocumentAbsoluteSafetyBudgetsV5,
} from "./decision-contract-v5.mjs";
import { evaluateMigrationDecision } from "./decision-evaluator.mjs";
import {
  buildViewStateReceiptV5,
  compareBundleViewStatesV5,
} from "./matched-view-state-v5.mjs";
import {
  buildNvidiaBaselineResult,
  qualifyNvidiaEvidence,
  summarizeNvidiaIterations,
} from "./nvidia-sampler.mjs";
import {
  repositoryDirectoryV4,
  validateOptimizedCandidatesV4,
} from "./optimized-candidates-v4.mjs";
import {
  assessRawBenefitEvidenceV5,
  buildHardComponentReportV5,
  buildV5ComparisonPlan,
  buildV5ExecutionSchedule,
  createCalibrationPairOrdersV5,
  expectedDecisionContractSha256V5,
  expectedWorkloadArtifactSha256V5,
  expectedWorkloadByteSha256V5,
} from "./run-paired-v5.mjs";
import {
  canonicalSha256,
  componentInputLaneV4,
  validateV4ComponentReport,
} from "./run-paired-v4.mjs";
import {
  buildScenarioContractV4,
  representativeScenarioDefinitionsV4,
  representativeTimedScenarioIdsV4,
  scenarioContractVersionV4,
} from "./scenario-contract-v4.mjs";
import {
  protocolVersionV5,
  representativeScenarioDefinitionsV5,
  representativeTimedScenarioIdsV5,
  scenarioContractVersionV5,
} from "./scenario-contract-v5.mjs";
import {
  inspectPngRasterArtifact,
  registerAndComparePresentedCropV2,
} from "./registered-crop-v5.mjs";
import {
  analyzeDynamicFidelityPairsV5,
  defaultBootstrapSamplesV5,
  validateHardComponentReportV5,
} from "./summarize-paired-v5.mjs";
import {
  buildDecisionEvidenceV4,
  extractComponentMeasurementsV4,
  summarizeVerifiedV4Run,
} from "./summarize-paired-v4.mjs";

export const finalDecisionV5SchemaVersion = 1;

const implementations = Object.freeze(["electron", "gpui"]);
const phases = Object.freeze(["calibration", "final"]);
const sha256Pattern = /^[0-9a-f]{64}$/;
const finalPairCount = 24;
const calibrationPairCount = 6;
const exactElectronPropertyDefect =
  "electron-numeric-property-input-blur-duplicate-history-v1";
const exactElectronMultiDefect =
  "electron-multi-document-second-nasa-visible-pages-empty-v1";
const benefitComponents = Object.freeze([
  "multi-document-session",
  "native-snap-transform-120hz",
  "viewer-dynamic-fidelity",
]);
const familyMetrics = Object.freeze({
  sustained_cpu_work: ["cpu_seconds"],
  process_memory: ["cgroup_peak_memory_bytes"],
  native_interaction_and_frame_pacing: [
    "application_frame_interval_p95_ms",
    "native_input_to_application_frame_ack_p95_ms",
  ],
  product_latency: ["product_wall_or_latency_ms"],
  gpu_resource_pressure: [
    "baseline_adjusted_gpu_peak_memory_mib",
    "baseline_adjusted_gpu_utilization_p95_percent",
  ],
});
const gpuFloors = Object.freeze({
  baseline_adjusted_gpu_peak_memory_mib: 1,
  baseline_adjusted_gpu_utilization_p95_percent: 0.1,
});

function sha256Bytes(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function same(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function geometricMean(values) {
  if (
    values.length === 0 ||
    values.some((value) => !Number.isFinite(value) || value <= 0)
  ) {
    return null;
  }
  return Math.exp(
    values.reduce((sum, value) => sum + Math.log(value), 0) / values.length,
  );
}

function localPath(baseDirectory, path) {
  if (typeof path !== "string" || path.length === 0) return null;
  return isAbsolute(path) ? path : resolve(baseDirectory, path);
}

async function pathWithinRealTree(root, path) {
  if (typeof root !== "string" || typeof path !== "string") return false;
  try {
    const [actualRoot, actualPath] = await Promise.all([
      realpath(root),
      realpath(path),
    ]);
    const relation = relative(actualRoot, actualPath);
    return (
      relation !== "" &&
      relation !== ".." &&
      !relation.startsWith(`..${sep}`) &&
      !isAbsolute(relation)
    );
  } catch {
    return false;
  }
}

async function loadHashedJson(baseDirectory, reference, label) {
  const path = localPath(baseDirectory, reference?.path);
  if (!path || !sha256Pattern.test(reference?.sha256 ?? "")) {
    return {
      value: null,
      path,
      failure: `${label}: path or SHA-256 is invalid`,
    };
  }
  let bytes;
  try {
    bytes = await readFile(path);
  } catch (error) {
    return {
      value: null,
      path,
      failure: `${label}: cannot read report: ${error.message}`,
    };
  }
  const observed = sha256Bytes(bytes);
  if (observed !== reference.sha256) {
    return {
      value: null,
      path,
      failure: `${label}: SHA-256 mismatch; expected ${reference.sha256}, got ${observed}`,
    };
  }
  try {
    return { value: JSON.parse(bytes), path, failure: null };
  } catch (error) {
    return {
      value: null,
      path,
      failure: `${label}: invalid JSON: ${error.message}`,
    };
  }
}

export async function validateAnalyzerRootOfTrustV5({
  input,
  baseDirectory,
  candidateValidator = validateOptimizedCandidatesV4,
}) {
  const failures = [];
  const runRoot = input?.artifact_tree?.run_root;
  const manifestReference = input?.run_manifest?.manifest;
  const checksumReference = input?.run_manifest?.checksum;
  for (const [label, reference] of [
    ["run manifest", manifestReference],
    ["run manifest checksum", checksumReference],
  ]) {
    if (!(await pathWithinRealTree(runRoot, reference?.path))) {
      failures.push(`${label} is outside the declared run artifact tree`);
    }
  }
  const loadedManifest = await loadHashedJson(
    baseDirectory,
    manifestReference,
    "completed v5 run manifest",
  );
  if (loadedManifest.failure) failures.push(loadedManifest.failure);
  let checksumBytes = null;
  try {
    checksumBytes = await readFile(
      localPath(baseDirectory, checksumReference?.path),
    );
    if (sha256Bytes(checksumBytes) !== checksumReference?.sha256) {
      failures.push("run manifest checksum artifact SHA-256 mismatch");
    }
  } catch (error) {
    failures.push(`cannot read run manifest checksum: ${error.message}`);
  }
  if (
    loadedManifest.value &&
    checksumBytes?.toString("utf8") !==
      `${manifestReference.sha256}  run-manifest-v5.json\n`
  ) {
    failures.push(
      "run manifest checksum does not authenticate the exact manifest bytes",
    );
  }
  const manifest = loadedManifest.value;
  if (
    manifest &&
    (manifest.complete !== true ||
      manifest.outcome !== "passed" ||
      manifest.plan?.protocol_version !== protocolVersionV5 ||
      manifest.plan?.decision_contract_version !== decisionContractVersionV5 ||
      manifest.plan?.workload_artifact_sha256 !==
        input.workload_artifact_sha256 ||
      manifest.plan?.workload_byte_sha256 !== input.workload_byte_sha256 ||
      manifest.settings?.schedule_seed !== input.schedule_seed ||
      !same(manifest.bundles, input.bundles) ||
      !same(manifest.correctness_reports, input.property_correctness) ||
      !same(manifest.candidates, input.candidate_artifacts) ||
      !same(manifest.artifact_tree, input.artifact_tree) ||
      !same(
        Object.fromEntries(
          Object.entries(manifest.references ?? {}).map(([id, artifact]) => [
            id,
            artifact.sha256,
          ]),
        ),
        input.reference_crop_sha256_by_id,
      ))
  ) {
    failures.push(
      "analyzer input is not an exact projection of the completed run manifest",
    );
  }
  let candidates = null;
  try {
    const declared = input?.candidate_artifacts;
    candidates = await candidateValidator({
      electronManifestPath: declared?.electron?.path,
      gpuiManifestPath: declared?.gpui?.path,
      electronExecutable: resolve(
        repositoryDirectoryV4,
        declared?.electron?.executable?.path ?? "missing",
      ),
      gpuiBinary: resolve(
        repositoryDirectoryV4,
        declared?.gpui?.executable?.path ?? "missing",
      ),
    });
    if (
      !same(candidates, declared) ||
      implementations.some(
        (implementation) =>
          candidates[implementation].sha256 !==
          input.candidate_manifest_sha256?.[implementation],
      )
    ) {
      failures.push(
        "candidate manifests or executable artifacts changed after the run",
      );
    }
  } catch (error) {
    failures.push(
      `candidate root-of-trust validation failed: ${error.message}`,
    );
  }
  return {
    passed: failures.length === 0,
    failures,
    manifest,
    candidates,
  };
}

function pushMismatch(failures, condition, message) {
  if (!condition) failures.push(message);
}

function receiptsAreAuthentic(report) {
  return (report?.command_receipts ?? []).every((receipt) => {
    if (!sha256Pattern.test(receipt?.evidence_sha256 ?? "")) return false;
    const { evidence_sha256: expected, ...payload } = receipt;
    return canonicalSha256(payload) === expected;
  });
}

export async function collectIndependentFileIdentitiesV5(records) {
  const failures = [];

  async function collectFileIdentity(path, label) {
    try {
      const metadata = await stat(path);
      if (!metadata.isFile()) {
        failures.push(`${label}: ${path} is not a file`);
        return null;
      }
      if ((metadata.mode & 0o444) === 0) {
        failures.push(`${label}: ${path} is not readable`);
        return null;
      }
      return `${metadata.dev}:${metadata.ino}`;
    } catch (error) {
      failures.push(`${label}: ${path} cannot stat: ${error.message}`);
      return null;
    }
  }

  const enrichedRecords = [];
  for (const record of records ?? []) {
    const launchLabel = record.binding?.launch_id ?? "unknown launch";
    const captures = [];
    for (const capture of record.captures ?? []) {
      const captureLabel = capture.capture_id ?? "unknown capture";
      const fileIdentities = [];
      for (const [artifactLabel, artifact] of [
        ["screenshot", capture.screenshot],
        ["candidate crop", capture.candidate_crop],
        ["registered reference", capture.registered_reference_crop],
      ]) {
        const identity = await collectFileIdentity(
          artifact?.path,
          `${captureLabel} ${artifactLabel}`,
        );
        if (identity !== null) fileIdentities.push(identity);
      }
      captures.push({ ...capture, file_identities: fileIdentities });
    }
    enrichedRecords.push({
      ...record,
      raw_file_identity: await collectFileIdentity(
        record.raw_path,
        `${launchLabel} raw report`,
      ),
      hard_file_identity: record.hard_path
        ? await collectFileIdentity(
            record.hard_path,
            `${launchLabel} hard report`,
          )
        : null,
      captures,
    });
  }

  return {
    passed: failures.length === 0,
    failures,
    records: enrichedRecords,
  };
}

export function validateIndependentLaunchBindingsV5(records) {
  const failures = [];
  const launchIds = new Set();
  const scheduleIndices = new Set();
  const rawHashes = new Set();
  const hardHashes = new Set();
  const rawPaths = new Set();
  const hardPaths = new Set();
  const rawFileIdentities = new Set();
  const hardFileIdentities = new Set();
  const captureIds = new Set();
  const capturePaths = new Set();
  const captureFileIdentities = new Set();
  const captureIntervals = new Set();
  const ordered = [];
  const exactFields = [
    "phase",
    "inference_eligible",
    "journey",
    "pair",
    "pair_position",
    "implementation",
    "component",
    "component_index",
    "input_lane",
    "schedule_index",
  ];
  for (const record of records ?? []) {
    const binding = record.binding;
    const label = record.expected?.launch_id ?? binding?.launch_id ?? "unknown";
    if (
      binding?.schema_version !== 1 ||
      typeof binding.launch_id !== "string" ||
      binding.launch_id.length === 0 ||
      !Number.isInteger(binding.schedule_index) ||
      !sha256Pattern.test(binding.candidate_manifest_sha256 ?? "") ||
      binding.raw_report_path !== record.raw_path ||
      !same(binding.fixture_sha256_by_id, record.expected?.fixture_sha256_by_id)
    ) {
      failures.push(
        `${label}: launch binding identity or artifact inputs are invalid`,
      );
    }
    for (const field of exactFields) {
      if (binding?.[field] !== record.expected?.[field]) {
        failures.push(
          `${label}: launch binding ${field} does not match schedule`,
        );
      }
    }
    if (binding?.launch_id !== record.expected?.launch_id) {
      failures.push(
        `${label}: launch_id does not match the scheduled invocation`,
      );
    }
    if (
      binding?.candidate_manifest_sha256 !==
      record.expected?.candidate_manifest_sha256
    ) {
      failures.push(`${label}: candidate identity does not match the launch`);
    }
    const startWall = Date.parse(binding?.started_at);
    const endWall = Date.parse(binding?.ended_at);
    if (
      !Number.isFinite(startWall) ||
      !Number.isFinite(endWall) ||
      endWall < startWall ||
      !Number.isFinite(binding?.started_monotonic_ms) ||
      !Number.isFinite(binding?.ended_monotonic_ms) ||
      binding.ended_monotonic_ms <= binding.started_monotonic_ms
    ) {
      failures.push(`${label}: launch time boundaries are invalid`);
    }
    for (const [kind, value, seen] of [
      ["launch_id", binding?.launch_id, launchIds],
      ["schedule_index", binding?.schedule_index, scheduleIndices],
      ["raw report SHA-256", record.raw_sha256, rawHashes],
      ["raw report path", record.raw_path, rawPaths],
      ["raw report inode", record.raw_file_identity, rawFileIdentities],
    ]) {
      if (value == null) continue;
      if (seen.has(value)) failures.push(`${label}: duplicate ${kind}`);
      seen.add(value);
    }
    if (record.hard_path) {
      for (const [kind, value, seen] of [
        ["hard report SHA-256", record.hard_sha256, hardHashes],
        ["hard report path", record.hard_path, hardPaths],
        ["hard report inode", record.hard_file_identity, hardFileIdentities],
      ]) {
        if (value == null) continue;
        if (seen.has(value)) failures.push(`${label}: duplicate ${kind}`);
        seen.add(value);
      }
      if (!same(record.hard_binding, binding)) {
        failures.push(
          `${label}: hard report launch binding does not match raw`,
        );
      }
    }
    for (const capture of record.captures ?? []) {
      if (
        capture.launch_id !== binding?.launch_id ||
        capture.capture_id !== `${binding?.launch_id}:${capture.crop_id}` ||
        !Number.isFinite(capture.capture_started_monotonic_ms) ||
        !Number.isFinite(capture.capture_ended_monotonic_ms) ||
        capture.capture_started_monotonic_ms < binding.started_monotonic_ms ||
        capture.capture_ended_monotonic_ms > binding.ended_monotonic_ms ||
        capture.capture_ended_monotonic_ms <=
          capture.capture_started_monotonic_ms
      ) {
        failures.push(
          `${label}: capture identity or monotonic interval does not match its launch`,
        );
      }
      if (captureIds.has(capture.capture_id))
        failures.push(`${label}: duplicate capture_id`);
      captureIds.add(capture.capture_id);
      const captureInterval = `${capture.capture_started_monotonic_ms}:${capture.capture_ended_monotonic_ms}`;
      if (captureIntervals.has(captureInterval))
        failures.push(`${label}: dynamic capture interval was reused`);
      captureIntervals.add(captureInterval);
      for (const artifact of [
        capture.screenshot,
        capture.candidate_crop,
        capture.registered_reference_crop,
      ]) {
        if (capturePaths.has(artifact?.path))
          failures.push(`${label}: dynamic capture path was reused`);
        capturePaths.add(artifact?.path);
      }
      for (const identity of capture.file_identities ?? []) {
        if (identity == null) continue;
        if (captureFileIdentities.has(identity))
          failures.push(`${label}: dynamic capture inode was reused`);
        captureFileIdentities.add(identity);
      }
    }
    ordered.push(binding);
  }
  ordered.sort((left, right) => left.schedule_index - right.schedule_index);
  for (let index = 0; index < ordered.length; index += 1) {
    const current = ordered[index];
    if (current.schedule_index !== index)
      failures.push(
        `launch schedule index ${index} is missing or out of order`,
      );
    const previous = ordered[index - 1];
    if (
      previous &&
      (current.started_monotonic_ms < previous.ended_monotonic_ms ||
        Date.parse(current.started_at) < Date.parse(previous.ended_at))
    ) {
      failures.push(
        `${current.launch_id}: launch interval overlaps or rewinds`,
      );
    }
  }
  return { passed: failures.length === 0, failures };
}

function exactNvidiaSample(sample, { adjusted = false } = {}) {
  const expectedKeys = [
    "elapsed_ms",
    "gpu_utilization_percent",
    "index",
    "memory_used_mib",
    "power_draw_watts",
    "timestamp",
    ...(adjusted ? [] : ["memory_total_mib"]),
  ];
  return (
    sample &&
    Number.isFinite(sample.elapsed_ms) &&
    sample.elapsed_ms >= 0 &&
    typeof sample.timestamp === "string" &&
    sample.timestamp.length > 0 &&
    Number.isInteger(sample.index) &&
    sample.index >= 0 &&
    Number.isFinite(sample.gpu_utilization_percent) &&
    sample.gpu_utilization_percent >= 0 &&
    sample.gpu_utilization_percent <= 100 &&
    Number.isFinite(sample.memory_used_mib) &&
    sample.memory_used_mib >= 0 &&
    (adjusted ||
      (Number.isFinite(sample.memory_total_mib) &&
        sample.memory_total_mib > 0 &&
        sample.memory_used_mib <= sample.memory_total_mib)) &&
    Number.isFinite(sample.power_draw_watts) &&
    sample.power_draw_watts >= 0 &&
    Object.keys(sample).sort().join("\0") === expectedKeys.sort().join("\0")
  );
}

function matchVulkanValue(summary, name) {
  return new RegExp(`(?:^|\\n)\\s*${name}\\s*=\\s*([^\\n]+)`, "m")
    .exec(summary ?? "")?.[1]
    ?.trim();
}

function hostGpuIdentity(host, failures) {
  const nvidia = host?.nvidia_gpu?.split(",").map((value) => value.trim());
  const vulkan = host?.vulkan_summary;
  const vulkanUuid = matchVulkanValue(vulkan, "deviceUUID");
  const nvidiaUuid = nvidia?.[1]?.replace(/^GPU-/, "");
  const requiredStrings = [
    host?.hostname,
    host?.os_type,
    host?.platform,
    host?.os_release,
    host?.architecture,
    host?.display_mode,
    ...(nvidia ?? []),
    vulkan,
  ];
  if (
    requiredStrings.some(
      (value) => typeof value !== "string" || value.trim().length === 0,
    ) ||
    host.platform !== "linux" ||
    nvidia?.length !== 4 ||
    !/^GPU-[0-9a-f-]{36}$/i.test(nvidia[1] ?? "") ||
    !/^\d+(?:\.\d+)+$/.test(nvidia[2] ?? "") ||
    !Number.isFinite(Number(nvidia[3])) ||
    Number(nvidia[3]) <= 0 ||
    matchVulkanValue(vulkan, "vendorID")?.toLowerCase() !== "0x10de" ||
    !/^0x[0-9a-f]+$/i.test(matchVulkanValue(vulkan, "deviceID") ?? "") ||
    matchVulkanValue(vulkan, "deviceType") !==
      "PHYSICAL_DEVICE_TYPE_DISCRETE_GPU" ||
    matchVulkanValue(vulkan, "deviceName") !== nvidia[0] ||
    matchVulkanValue(vulkan, "driverInfo") !== nvidia[2] ||
    vulkanUuid?.toLowerCase() !== nvidiaUuid?.toLowerCase()
  ) {
    failures.push(
      "provenance does not identify one exact NVIDIA discrete GPU and display",
    );
    return null;
  }
  return {
    hostname: host.hostname,
    os_type: host.os_type,
    platform: host.platform,
    os_release: host.os_release,
    architecture: host.architecture,
    display_mode_sha256: sha256Bytes(Buffer.from(host.display_mode)),
    nvidia_name: nvidia[0],
    nvidia_uuid: nvidia[1],
    nvidia_driver: nvidia[2],
    nvidia_memory_total_mib: Number(nvidia[3]),
    vulkan_vendor_id: matchVulkanValue(vulkan, "vendorID"),
    vulkan_device_id: matchVulkanValue(vulkan, "deviceID"),
    vulkan_device_type: matchVulkanValue(vulkan, "deviceType"),
    vulkan_device_name: matchVulkanValue(vulkan, "deviceName"),
    vulkan_driver_info: matchVulkanValue(vulkan, "driverInfo"),
    vulkan_device_uuid: vulkanUuid,
  };
}

function normalizeGpuName(value) {
  return String(value ?? "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

function normalizeDriverVersion(value) {
  const parts = String(value ?? "").match(/\d+/g);
  return parts?.map((part) => String(Number(part))).join(".") ?? "";
}

function gpuNameMatches(candidate, expected) {
  const candidateName = normalizeGpuName(candidate);
  const expectedName = normalizeGpuName(expected);
  return (
    candidateName.length > 0 &&
    expectedName.length > 0 &&
    (candidateName.includes(expectedName) ||
      expectedName.includes(candidateName))
  );
}

function exactGpuiAdapterBinding(report, identity, failures) {
  const events = report?.iterations?.[0]?.events;
  const receipts = Array.isArray(events)
    ? events.filter((event) => event?.event === "gpu-adapter-selected")
    : [];
  if (receipts.length !== 1) {
    failures.push("GPUI must retain exactly one selected GPU adapter receipt");
    return;
  }
  const receipt = receipts[0];
  if (
    receipt.available !== true ||
    receipt.is_software_emulated !== false ||
    !gpuNameMatches(receipt.device_name, identity.nvidia_name) ||
    !/nvidia/i.test(receipt.driver_name ?? "") ||
    normalizeDriverVersion(receipt.driver_info) !==
      normalizeDriverVersion(identity.nvidia_driver)
  ) {
    failures.push(
      "GPUI selected adapter is software-emulated or does not match the frozen NVIDIA device",
    );
  }
}

function exactElectronAdapterBinding(report, identity, failures) {
  const gpuInfo = report?.iterations?.[0]?.renderer?.browser_gpu_info;
  const devices = gpuInfo?.gpu?.devices;
  const active = Array.isArray(devices)
    ? devices.filter((device) => device?.active === true)
    : [];
  const device = active[0];
  const renderer = gpuInfo?.gpu?.auxAttributes?.glRenderer ?? "";
  const compositor = gpuInfo?.gpu?.featureStatus?.gpu_compositing;
  const vendorId =
    typeof device?.vendorId === "string"
      ? Number.parseInt(device.vendorId, 0)
      : device?.vendorId;
  if (
    active.length !== 1 ||
    vendorId !== 0x10de ||
    !gpuNameMatches(device?.deviceString, identity.nvidia_name) ||
    normalizeDriverVersion(device?.driverVersion) !==
      normalizeDriverVersion(identity.nvidia_driver) ||
    !gpuNameMatches(renderer, identity.nvidia_name) ||
    /swiftshader|llvmpipe|software rasterizer/i.test(
      `${device?.deviceString ?? ""} ${renderer}`,
    ) ||
    typeof compositor !== "string" ||
    !compositor.startsWith("enabled")
  ) {
    failures.push(
      "Electron active Chromium adapter is software-rendered or does not match the frozen NVIDIA device",
    );
  }
}

export function validateCandidateGpuAdapterBindingV5(
  report,
  identity,
  failures = [],
) {
  if (report?.implementation === "gpui") {
    exactGpuiAdapterBinding(report, identity, failures);
  } else if (report?.implementation === "electron") {
    exactElectronAdapterBinding(report, identity, failures);
  } else {
    failures.push("report implementation is not electron or gpui");
  }
  return { passed: failures.length === 0, failures };
}

export function validateGpuAndHostEvidenceV5(report) {
  const failures = [];
  if (report?.requested_iterations !== 1 || report?.iterations?.length !== 1) {
    failures.push("exactly one GPU iteration is required");
    return { passed: false, failures, identity: null };
  }
  const iteration = report.iterations[0];
  const gpu = iteration?.gpu;
  for (const phase of ["baseline", "run", "baseline_adjusted"]) {
    const samples = gpu?.[phase]?.samples;
    if (
      !Array.isArray(samples) ||
      samples.length !== gpu?.[phase]?.sample_count ||
      samples.some(
        (sample) =>
          !exactNvidiaSample(sample, {
            adjusted: phase === "baseline_adjusted",
          }),
      )
    ) {
      failures.push(`${phase} must contain finite exact NVIDIA samples`);
    }
  }
  if ((gpu?.invalid_lines?.length ?? 0) !== 0) {
    failures.push("NVIDIA evidence contains invalid sampler lines");
  }
  if (failures.length === 0) {
    const recomputed = buildNvidiaBaselineResult({
      baselineSamples: gpu.baseline.samples,
      transitionSamples: gpu.transition_samples ?? [],
      runSamples: gpu.run.samples,
      invalidLines: gpu.invalid_lines ?? [],
      spawnError: null,
      baselineDurationMs: gpu.baseline_duration_ms,
      intervalMs: gpu.requested_interval_ms,
    });
    recomputed.qualification = qualifyNvidiaEvidence(recomputed, {
      required: true,
    });
    for (const key of [
      "supported",
      "command_available",
      "reason",
      "scope",
      "requested_interval_ms",
      "baseline_duration_ms",
      "invalid_lines",
      "transition_samples",
      "samples",
      "summary",
      "baseline",
      "run",
      "baseline_adjusted",
      "qualification",
    ]) {
      if (!same(gpu?.[key], recomputed[key])) {
        failures.push(
          `NVIDIA ${key} receipt does not match recomputed samples`,
        );
      }
    }
    const recomputedSummary = summarizeNvidiaIterations([{ gpu }]);
    for (const key of [
      "gpu_whole_device_baseline",
      "gpu_whole_device",
      "gpu_whole_device_baseline_adjusted",
    ]) {
      if (!same(report?.summary?.[key], recomputedSummary[key])) {
        failures.push(`${key} does not match recomputed iteration samples`);
      }
    }
  }
  const identity = hostGpuIdentity(report?.provenance?.host, failures);
  if (identity)
    validateCandidateGpuAdapterBindingV5(report, identity, failures);
  return { passed: failures.length === 0, failures, identity };
}

export function validateFrozenHostGpuIdentityV5(assessments) {
  const failures = [];
  const identities = (assessments ?? []).map(({ identity }) => identity);
  if (
    identities.length === 0 ||
    identities.some((identity) => !identity) ||
    identities.some((identity) => !same(identity, identities[0]))
  ) {
    failures.push("all reports must bind to one frozen host/GPU identity");
  }
  return {
    passed: failures.length === 0,
    failures,
    identity: failures.length === 0 ? identities[0] : null,
  };
}

function referenceCropHashes(workload) {
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

function classifyEvidenceFailure(evidence) {
  const validation = evidence.validation_failures ?? [];
  const v4Blocked =
    evidence.v4_decision?.eligibility !== "decision-ready"
      ? [
          ...(evidence.v4_decision?.eligibility_failures ?? [
            "v4 resource and statistics evidence is not decision-ready",
          ]),
        ]
      : [];
  const correctnessBlocked =
    evidence.correctness?.executable === false
      ? (evidence.correctness.failures ?? [
          "v5 correctness evidence is incomplete",
        ])
      : [];
  const dynamicBlocked =
    evidence.dynamic_fidelity?.executable === false
      ? (evidence.dynamic_fidelity.failures ?? [
          "dynamic fidelity evidence is incomplete",
        ])
      : [];
  const benefitBlocked =
    evidence.v5_benefit_metrics?.executable === false
      ? (evidence.v5_benefit_metrics.failures ?? [
          "v5 benefit evidence is incomplete",
        ])
      : [];
  return [
    ...validation,
    ...v4Blocked,
    ...correctnessBlocked,
    ...dynamicBlocked,
    ...benefitBlocked,
  ];
}

export function composeDecisionV5(evidence) {
  const blocked = classifyEvidenceFailure(evidence);
  const notRun = [
    "macOS visual capture and platform qualification were not run on the Linux GPU lane",
    "Windows platform qualification was not run on the Linux GPU lane",
    "packaged-candidate and public-release qualification were not run",
    "private Hibbeler corpus was blocked-not-transferred and was not run",
  ];
  if (blocked.length > 0) {
    return {
      schema_version: finalDecisionV5SchemaVersion,
      status: "blocked",
      decision: null,
      worth_funding_migration: null,
      comparison: "Electron/PDF.js versus GPUI/PDFium whole application stacks",
      checks: { passed: [], failed: [], blocked, not_run: notRun },
      rule: "invalid or incomplete evidence is BLOCKED and is never guessed as NO",
    };
  }

  const failed = [];
  if (evidence.v4_decision?.decision !== "yes") {
    failed.push(
      ...(evidence.v4_decision?.metric_failures ?? [
        "shared v4 resource and statistics decision did not pass",
      ]),
    );
  }
  if (evidence.correctness?.passed !== true) {
    failed.push(
      ...(evidence.correctness?.failures ?? ["v5 correctness did not pass"]),
    );
  }
  if (evidence.dynamic_fidelity?.decision_ready !== true) {
    failed.push(
      ...(evidence.dynamic_fidelity?.failures ?? [
        "dynamic fidelity did not pass",
      ]),
    );
  }
  if (evidence.v5_benefit_metrics?.decision_ready !== true) {
    failed.push(
      ...(evidence.v5_benefit_metrics?.failures ?? [
        "v5 benefit metrics did not pass",
      ]),
    );
  }
  const yes = failed.length === 0;
  const passed = [
    "all analyzer input identities, hashes, reports, hardware receipts, and pair balance are valid",
    "shared five-journey v4 resource and statistical evidence is decision-ready",
    "GPUI hard-component correctness and exact allowed Electron baseline outcomes were evaluated",
    "registered crops, presented scale, and dynamic fidelity observations were evaluated",
    "eligible v5 benefit metrics were evaluated without imputing missing Electron data",
  ];
  return {
    schema_version: finalDecisionV5SchemaVersion,
    status: "decision-ready",
    decision: yes ? "YES" : "NO",
    worth_funding_migration: yes,
    comparison: "Electron/PDF.js versus GPUI/PDFium whole application stacks",
    checks: { passed, failed, blocked: [], not_run: notRun },
    rule: "YES requires all shared resource families, exact hard correctness, dynamic fidelity, and every eligible v5 benefit family to pass; measured regression after eligibility is NO",
  };
}

function reportReferenceMatches(reference, assessment) {
  return (
    same(reference.measurements ?? {}, assessment.measurements ?? {}) &&
    same(
      reference.quality_measurements ?? {},
      assessment.quality_measurements ?? {},
    ) &&
    reference.benefit_metrics_eligible ===
      assessment.benefit_metrics_eligible &&
    reference.correctness_passed === assessment.correctness_passed &&
    (reference.known_baseline_defect_id ?? null) ===
      (assessment.known_baseline_defect_id ?? null)
  );
}

function expectedHardComponentIndex(journey, component) {
  const definition = representativeScenarioDefinitionsV5[journey];
  return definition?.current_runner_components
    ?.filter((item) => item !== "native-property-edit-undo")
    .indexOf(component);
}

export function validateFinalHardReferencesV5(references, retainedReports) {
  const failures = [];
  const retained = [];
  const identities = new Set();
  const paths = new Set();
  for (const reference of references ?? []) {
    const label = `${reference?.journey ?? "unknown"}:pair-${reference?.pair ?? "unknown"}:${reference?.implementation ?? "unknown"}:${reference?.component ?? "unknown"}`;
    const identity = `${reference?.journey}\0${reference?.pair}\0${reference?.implementation}\0${reference?.component}`;
    if (identities.has(identity))
      failures.push(`${label}: duplicate final hard reference identity`);
    identities.add(identity);
    if (paths.has(reference?.path))
      failures.push(`${label}: final hard report path was reused`);
    paths.add(reference?.path);
    const expectedIndex = expectedHardComponentIndex(
      reference?.journey,
      reference?.component,
    );
    if (
      reference?.phase !== "final" ||
      reference?.inference_eligible !== true ||
      !Number.isInteger(reference?.pair) ||
      reference.pair < 1 ||
      reference.pair > finalPairCount ||
      reference?.input_lane !== "native-x11-xtest" ||
      reference?.component_index !== expectedIndex ||
      expectedIndex < 0
    ) {
      failures.push(
        `${label}: reference is not an exact final inference hard-component record`,
      );
    }
    const matches = (retainedReports ?? []).filter(
      ({ entry, componentEntry }) =>
        componentEntry?.hard_report_path === reference?.path &&
        componentEntry?.hard_report_sha256 === reference?.sha256 &&
        entry?.journey === reference?.journey &&
        entry?.pair === reference?.pair &&
        entry?.pair_position === reference?.pair_position &&
        entry?.implementation === reference?.implementation &&
        componentEntry?.component === reference?.component,
    );
    if (matches.length !== 1) {
      failures.push(
        `${label}: expected exactly one retained bundle binding, got ${matches.length}`,
      );
      continue;
    }
    const match = matches[0];
    if (
      match.entry.phase !== "final" ||
      match.entry.inference_eligible !== true ||
      match.componentEntry.hard_component !== true ||
      match.componentEntry.input_lane !== "native-x11-xtest" ||
      match.componentEntry.component_index !== expectedIndex
    ) {
      failures.push(
        `${label}: retained report is not in the exact final inference bundle`,
      );
      continue;
    }
    if (!reportReferenceMatches(reference, match.assessment)) {
      failures.push(
        `${label}: analyzer metadata does not match the exact hard report`,
      );
      continue;
    }
    retained.push({ reference, retained: match });
  }
  return { passed: failures.length === 0, failures, retained };
}

async function hashedPngArtifact(path, expectedSha256, label, failures) {
  let bytes;
  try {
    bytes = await readFile(path);
    await inspectPngRasterArtifact(path);
  } catch (error) {
    failures.push(`${label}: PNG is missing or undecodable: ${error.message}`);
    return null;
  }
  const observed = sha256Bytes(bytes);
  if (observed !== expectedSha256) {
    failures.push(
      `${label}: PNG SHA-256 mismatch; expected ${expectedSha256}, got ${observed}`,
    );
    return null;
  }
  return { path, sha256: observed };
}

export async function validateDynamicCropArtifactsV5({
  hardReports,
  workload,
  artifactTree,
}) {
  const failures = [];
  const verified = [];
  const command = workload?.journeys
    ?.flatMap(({ commands }) => commands)
    .find(({ id }) => id === "viewer:dynamic-fidelity-scroll");
  for (const hard of (hardReports ?? []).filter(
    ({ component }) => component === "viewer-dynamic-fidelity",
  )) {
    const crops =
      hard.report?.summary?.viewer_dynamic_fidelity?.registered_crops;
    const declarations = hard.dynamic_crop_artifacts;
    const expectedCropCount = command?.registered_crops?.length;
    if (
      !Number.isInteger(expectedCropCount) ||
      expectedCropCount < 1 ||
      !Array.isArray(crops) ||
      crops.length !== expectedCropCount ||
      !Array.isArray(declarations) ||
      declarations.length !== expectedCropCount
    ) {
      failures.push(
        `${hard.implementation}:pair-${hard.pair}: exactly three declared dynamic PNG artifact sets are required`,
      );
      continue;
    }
    for (const crop of crops) {
      const label = `${hard.implementation}:pair-${hard.pair}:${crop.crop_id}`;
      const declarationMatches = declarations.filter(
        ({ crop_id: cropId }) => cropId === crop.crop_id,
      );
      const expected = command?.registered_crops?.find(
        ({ crop_id: cropId }) => cropId === crop.crop_id,
      );
      const source = artifactTree?.references?.[crop.crop_id];
      if (declarationMatches.length !== 1 || !expected || !source) {
        failures.push(
          `${label}: artifact declaration or crop contract is not unique`,
        );
        continue;
      }
      const declaration = declarationMatches[0];
      const expectedDeclarations = {
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
        source_reference_crop: source,
      };
      if (
        typeof declaration.launch_id !== "string" ||
        typeof declaration.capture_id !== "string" ||
        !Number.isFinite(declaration.capture_started_monotonic_ms) ||
        !Number.isFinite(declaration.capture_ended_monotonic_ms) ||
        declaration.launch_id !== crop.launch_id ||
        declaration.capture_id !== crop.capture_id ||
        declaration.capture_started_monotonic_ms !==
          crop.stability?.capture_monotonic_interval?.start_ms ||
        declaration.capture_ended_monotonic_ms !==
          crop.stability?.capture_monotonic_interval?.end_ms ||
        Object.entries(expectedDeclarations).some(
          ([name, expectedArtifact]) =>
            declaration?.[name]?.path !== expectedArtifact.path ||
            declaration?.[name]?.sha256 !== expectedArtifact.sha256,
        )
      ) {
        failures.push(
          `${label}: declared PNG artifacts do not match the hard report`,
        );
        continue;
      }
      const runArtifacts = [
        ["screenshot", declaration.screenshot],
        ["candidate crop", declaration.candidate_crop],
        ["registered reference", declaration.registered_reference_crop],
      ];
      let pathsValid = true;
      for (const [name, artifact] of runArtifacts) {
        if (
          !(await pathWithinRealTree(artifactTree?.run_root, artifact.path))
        ) {
          failures.push(
            `${label}:${name}: path is outside the declared run artifact tree`,
          );
          pathsValid = false;
        }
      }
      if (
        !(await pathWithinRealTree(artifactTree?.reference_root, source.path))
      ) {
        failures.push(
          `${label}: source reference path is outside the declared reference tree`,
        );
        pathsValid = false;
      }
      if (!pathsValid) continue;
      const artifacts = await Promise.all([
        hashedPngArtifact(
          declaration.screenshot.path,
          declaration.screenshot.sha256,
          `${label}:screenshot`,
          failures,
        ),
        hashedPngArtifact(
          declaration.candidate_crop.path,
          declaration.candidate_crop.sha256,
          `${label}:candidate crop`,
          failures,
        ),
        hashedPngArtifact(
          declaration.registered_reference_crop.path,
          declaration.registered_reference_crop.sha256,
          `${label}:registered reference`,
          failures,
        ),
        hashedPngArtifact(
          source.path,
          source.sha256,
          `${label}:source reference`,
          failures,
        ),
      ]);
      if (artifacts.some((artifact) => !artifact)) continue;
      const temporary = await mkdtemp(resolve(tmpdir(), "bp-v5-crop-audit-"));
      try {
        const outputCandidatePath = resolve(temporary, "candidate.png");
        const outputReferencePath = resolve(temporary, "reference.png");
        const recomputed = await registerAndComparePresentedCropV2({
          screenshotPath: declaration.screenshot.path,
          pageBoundsPx: crop.presentation?.painted_page_bounds_device_px,
          pageSizePt: crop.presentation?.page_size_points,
          pdfRect: expected.pdf_rect,
          referencePath: source.path,
          outputCandidatePath,
          outputRegisteredReferencePath: outputReferencePath,
        });
        const receipt = {
          mapped_bounds_pixels: crop.mapped_bounds_pixels,
          extracted_bounds_pixels: crop.extracted_bounds_pixels,
          candidate_dimensions: crop.candidate_dimensions,
          reference_original_dimensions: crop.reference_original_dimensions,
          registered_reference_dimensions: crop.registered_reference_dimensions,
          candidate_resampled: crop.candidate_resampled,
          reference_resampled: crop.reference_resampled,
          reference_resampling: crop.reference_resampling,
          screenshot_sha256: crop.screenshot_sha256,
          candidate_crop_sha256: crop.candidate_crop_sha256,
          reference_crop_sha256: crop.reference_crop_sha256,
          registered_reference_crop_sha256:
            crop.registered_reference_crop_sha256,
          metric: crop.metric,
        };
        if (!same(receipt, recomputed)) {
          failures.push(
            `${label}: registered v2 metric receipt does not match recomputed PNG bytes`,
          );
          continue;
        }
        const [candidateBytes, referenceBytes] = await Promise.all([
          readFile(outputCandidatePath),
          readFile(outputReferencePath),
        ]);
        if (
          sha256Bytes(candidateBytes) !== declaration.candidate_crop.sha256 ||
          sha256Bytes(referenceBytes) !==
            declaration.registered_reference_crop.sha256
        ) {
          failures.push(
            `${label}: retained derived PNGs are not the recomputed native artifacts`,
          );
          continue;
        }
        verified.push({
          pair: hard.pair,
          implementation: hard.implementation,
          crop_id: crop.crop_id,
        });
      } catch (error) {
        failures.push(`${label}: PNG recomputation failed: ${error.message}`);
      } finally {
        await rm(temporary, { recursive: true, force: true });
      }
    }
  }
  return { passed: failures.length === 0, failures, verified };
}

function exactPairOrders(bundleEntries, failures, seed) {
  const orders = {};
  for (const phase of phases) {
    const pairCount =
      phase === "calibration" ? calibrationPairCount : finalPairCount;
    orders[phase] = [];
    for (let pair = 1; pair <= pairCount; pair += 1) {
      const perJourney = representativeTimedScenarioIdsV5.map((journey) => {
        const matching = bundleEntries.filter(
          (entry) =>
            entry.phase === phase &&
            entry.journey === journey &&
            entry.pair === pair,
        );
        if (matching.length !== 2) {
          failures.push(
            `${phase}:${journey}:pair-${pair}: expected two implementation bundles`,
          );
          return null;
        }
        const first = matching.filter(
          ({ pair_position: position }) => position === "first",
        );
        const second = matching.filter(
          ({ pair_position: position }) => position === "second",
        );
        if (
          first.length !== 1 ||
          second.length !== 1 ||
          first[0].implementation === second[0].implementation ||
          !implementations.includes(first[0].implementation) ||
          !implementations.includes(second[0].implementation)
        ) {
          failures.push(
            `${phase}:${journey}:pair-${pair}: first/second balance is invalid`,
          );
          return null;
        }
        return [first[0].implementation, second[0].implementation];
      });
      const expected = perJourney.find(Boolean) ?? null;
      if (!expected || perJourney.some((order) => !same(order, expected))) {
        failures.push(
          `${phase}:pair-${pair}: order is not identical across journeys`,
        );
      }
      orders[phase].push(expected);
    }
  }
  for (let block = 0; block < finalPairCount / 4; block += 1) {
    const starts = orders.final
      .slice(block * 4, block * 4 + 4)
      .map((order) => order?.[0]);
    if (
      starts.filter((item) => item === "electron").length !== 2 ||
      starts.filter((item) => item === "gpui").length !== 2
    ) {
      failures.push(
        `final pair-order block ${block + 1} is not balanced two-and-two`,
      );
    }
  }
  const calibrationStarts = orders.calibration.map((order) => order?.[0]);
  if (
    calibrationStarts.filter((item) => item === "electron").length !== 3 ||
    calibrationStarts.filter((item) => item === "gpui").length !== 3
  ) {
    failures.push("calibration pair order is not balanced three-and-three");
  }
  if (!Number.isInteger(seed)) {
    failures.push("reviewed schedule seed is missing or invalid");
  } else {
    const expectedOrders = {
      calibration: createCalibrationPairOrdersV5(seed ^ 0x4341_4c35),
      final: createBalancedPairOrders({ pairCount: finalPairCount, seed }),
    };
    for (const phase of phases) {
      if (!same(orders[phase], expectedOrders[phase])) {
        failures.push(`${phase} pair order does not match the reviewed seed`);
      }
    }
  }
  return orders;
}

export function validatePairOrdersV5(bundleEntries, seed) {
  const failures = [];
  const orders = exactPairOrders(bundleEntries, failures, seed);
  return { passed: failures.length === 0, failures, orders };
}

function ratioForMetric(name, electron, gpui) {
  const floor = gpuFloors[name];
  if (floor) return Math.max(gpui, floor) / Math.max(electron, floor);
  if (![electron, gpui].every((value) => Number.isFinite(value) && value > 0))
    return null;
  return gpui / electron;
}

function absoluteBudget(component, family, metric) {
  if (family === "process_memory") return 1.5 * 1024 * 1024 * 1024;
  if (
    family === "gpu_resource_pressure" &&
    metric === "baseline_adjusted_gpu_peak_memory_mib"
  )
    return 2048;
  if (family === "native_interaction_and_frame_pacing") {
    return metric === "application_frame_interval_p95_ms" ? 25 : 1000 / 30;
  }
  return null;
}

export function analyzeV5BenefitMetrics(
  hardReports,
  { bootstrapSamples = defaultBootstrapSamplesV5 } = {},
) {
  const blockingFailures = [];
  const metricFailures = [];
  const gpuiMultiDocumentAbsoluteSafety =
    analyzeGpuiMultiDocumentAbsoluteSafetyV5(hardReports);
  blockingFailures.push(...gpuiMultiDocumentAbsoluteSafety.blocking_failures);
  metricFailures.push(...gpuiMultiDocumentAbsoluteSafety.metric_failures);
  const components = {};
  const explicitlyIneligible = [];
  for (const component of benefitComponents) {
    const reports = hardReports.filter((item) => item.component === component);
    const byPair = Array.from({ length: finalPairCount }, (_, index) => {
      const pair = index + 1;
      return Object.fromEntries(
        implementations.map((implementation) => [
          implementation,
          reports.find(
            (item) =>
              item.pair === pair && item.implementation === implementation,
          ),
        ]),
      );
    });
    const electronIneligible = byPair.every(
      (pair) => pair.electron?.assessment?.benefit_metrics_eligible === false,
    );
    if (component === "multi-document-session" && electronIneligible) {
      const exact = byPair.every(
        (pair) =>
          pair.electron?.assessment?.known_baseline_defect_id ===
          exactElectronMultiDefect,
      );
      if (!exact)
        blockingFailures.push(
          `${component}: Electron benefit metrics are missing without the exact allowed defect`,
        );
      explicitlyIneligible.push({
        component,
        implementation: "electron",
        reason: exactElectronMultiDefect,
        missing_metrics: [...electronMultiDocumentMissingBenefitMetricsV5],
      });
      components[component] = {
        status: exact
          ? "explicitly-ineligible-known-electron-defect"
          : "invalid-missing",
        families: {},
      };
      continue;
    }
    for (const [index, pair] of byPair.entries()) {
      for (const implementation of implementations) {
        if (
          pair[implementation]?.assessment?.benefit_metrics_eligible !== true
        ) {
          blockingFailures.push(
            `${component}:pair-${index + 1}:${implementation}: benefit evidence is ineligible`,
          );
        }
      }
    }
    const families = {};
    for (const [family, metrics] of Object.entries(familyMetrics)) {
      const ratios = [];
      const budgetChecks = [];
      for (const [index, pair] of byPair.entries()) {
        const metricRatios = metrics.map((metric) => {
          const electron = pair.electron?.assessment?.measurements?.[metric];
          const gpui = pair.gpui?.assessment?.measurements?.[metric];
          const ratio = ratioForMetric(metric, electron, gpui);
          const budget = absoluteBudget(component, family, metric);
          if (Number.isFinite(budget)) {
            for (const [implementation, value] of [
              ["electron", electron],
              ["gpui", gpui],
            ]) {
              budgetChecks.push({
                pair: index + 1,
                implementation,
                metric,
                value,
                budget,
                passed: Number.isFinite(value) && value <= budget,
              });
            }
          }
          return ratio;
        });
        const ratio = geometricMean(metricRatios);
        if (!ratio)
          blockingFailures.push(
            `${component}:${family}:pair-${index + 1}: measurements are missing`,
          );
        else ratios.push(ratio);
      }
      const bootstrap =
        ratios.length === finalPairCount
          ? pairedLogRatioBootstrap(ratios, {
              samples: bootstrapSamples,
              seed: Number.parseInt(
                canonicalSha256V5(`${component}:${family}`).slice(0, 8),
                16,
              ),
            })
          : null;
      const threshold =
        decisionContractV5.decision.primary_metric_upper_95_thresholds[family];
      const passed =
        bootstrap?.upper_95 <= threshold &&
        budgetChecks.every((check) => check.passed);
      if (bootstrap && bootstrap.upper_95 > threshold)
        metricFailures.push(
          `${component}:${family} upper_95 ${bootstrap.upper_95} exceeds ${threshold}`,
        );
      for (const check of budgetChecks.filter(
        ({ passed: itemPassed }) => !itemPassed,
      )) {
        metricFailures.push(
          `${component}:${family}:${check.implementation}:${check.metric} ${check.value} exceeds ${check.budget}`,
        );
      }
      families[family] = {
        status: bootstrap ? "complete" : "missing-measurements",
        direction: "lower-is-better",
        ratio: "gpui/electron",
        threshold_upper_95: threshold,
        paired_ratio: bootstrap,
        absolute_budget: {
          passed: budgetChecks.every((check) => check.passed),
          checks: budgetChecks,
        },
        passed,
      };
    }
    components[component] = { status: "complete", families };
  }
  if (bootstrapSamples !== defaultBootstrapSamplesV5) {
    blockingFailures.push(
      `decision bootstrap requires ${defaultBootstrapSamplesV5} resamples; got ${bootstrapSamples}`,
    );
  }
  const failures = [...blockingFailures, ...metricFailures];
  return {
    executable: blockingFailures.length === 0,
    bootstrap_samples: bootstrapSamples,
    components,
    explicitly_ineligible: explicitlyIneligible,
    gpui_multi_document_absolute_safety: gpuiMultiDocumentAbsoluteSafety,
    decision_ready: failures.length === 0,
    blocking_failures: blockingFailures,
    metric_failures: metricFailures,
    failures,
  };
}

export function analyzeGpuiMultiDocumentAbsoluteSafetyV5(hardReports) {
  const blockingFailures = [];
  const metricFailures = [];
  const reports = (hardReports ?? []).filter(
    (item) =>
      item.component === "multi-document-session" &&
      item.implementation === "gpui",
  );
  const expectedMetrics = gpuiMultiDocumentAbsoluteSafetyBudgetsV5.metrics;
  if (
    reports.length !== finalPairCount ||
    new Set(reports.map(({ pair }) => pair)).size !== finalPairCount
  ) {
    blockingFailures.push(
      `GPUI multi-document safety requires exactly ${finalPairCount} unique final reports`,
    );
  }
  const valuesByMetric = Object.fromEntries(
    Object.keys(expectedMetrics).map((metric) => [metric, []]),
  );
  for (const report of reports) {
    if (report.assessment?.benefit_metrics_eligible !== true) {
      blockingFailures.push(
        `GPUI multi-document pair ${report.pair}: benefit evidence is ineligible`,
      );
      continue;
    }
    const measurements = report.assessment?.measurements;
    if (
      !gpuiMultiDocumentAbsoluteSafetyBudgetsV5.metrics.product_wall_or_latency_ms.allowed_source.includes(
        measurements?.product_wall_or_latency_source,
      )
    ) {
      blockingFailures.push(
        `GPUI multi-document pair ${report.pair}: product latency source is invalid`,
      );
    }
    for (const metric of Object.keys(expectedMetrics)) {
      const value = measurements?.[metric];
      if (!Number.isFinite(value) || value < 0) {
        blockingFailures.push(
          `GPUI multi-document pair ${report.pair}: ${metric} is missing or nonfinite`,
        );
      } else {
        valuesByMetric[metric].push(value);
      }
    }
  }
  const checks = Object.fromEntries(
    Object.entries(expectedMetrics).map(([metric, contract]) => {
      const values = valuesByMetric[metric];
      const maximum =
        values.length === finalPairCount ? Math.max(...values) : null;
      const passed = Number.isFinite(maximum) && maximum <= contract.maximum;
      if (Number.isFinite(maximum) && !passed) {
        metricFailures.push(
          `GPUI multi-document ${metric} maximum ${maximum} exceeds ${contract.maximum}`,
        );
      }
      return [
        metric,
        {
          maximum,
          budget: contract.maximum,
          unit: contract.unit,
          passed,
        },
      ];
    }),
  );
  return {
    resolved: blockingFailures.length === 0,
    passed: blockingFailures.length === 0 && metricFailures.length === 0,
    aggregation: gpuiMultiDocumentAbsoluteSafetyBudgetsV5.aggregation,
    checks,
    blocking_failures: blockingFailures,
    metric_failures: metricFailures,
  };
}

function syntheticV4Manifest(input, workloadV4, orders, v5Plan) {
  const journeys = representativeTimedScenarioIdsV4.map((scenario) => {
    const definition = representativeScenarioDefinitionsV4[scenario];
    return {
      scenario,
      journey_id: definition.journey_id,
      fixture_id: definition.fixture_id,
      fixture_sha256: definition.fixture_sha256,
      component_order: [...definition.current_runner_components],
      component_weights: [...definition.component_weights],
      component_command_ids: structuredClone(definition.component_command_ids),
      command_ids: [...definition.command_ids],
      inference_eligible: true,
    };
  });
  return {
    schema_version: 1,
    complete: true,
    outcome: "passed",
    plan: {
      protocol_version: "bp-perf-v4",
      decision_contract_version: "bp-perf-v4-decision-1",
      scenario_contract_version: scenarioContractVersionV4,
      manifest_id: workloadV4.manifest_id,
      ready: true,
      blockers: [],
      journeys,
    },
    workload: {
      manifest_id: workloadV4.manifest_id,
      canonical_artifact_sha256: input.workload_artifact_sha256,
    },
    candidates: Object.fromEntries(
      implementations.map((implementation) => [
        implementation,
        { sha256: input.candidate_manifest_sha256[implementation] },
      ]),
    ),
    fixtures: Object.fromEntries(
      journeys.map((journey) => [
        journey.fixture_id,
        { sha256: journey.fixture_sha256 },
      ]),
    ),
    settings: {
      calibration_pairs_per_journey: calibrationPairCount,
      calibration_inference_eligible: false,
      final_pairs_per_journey: finalPairCount,
      final_pair_orders: orders.final,
    },
    excluded_lanes: {
      usgs_large_sheet_stress: "non-inferential and not scheduled",
      private_hibbeler_935: "blocked-not-transferred and not scheduled",
    },
    source_v5_plan: v5Plan,
  };
}

export async function loadVerifiedV5AnalyzerInput(
  inputPath,
  { candidateValidator = validateOptimizedCandidatesV4 } = {},
) {
  const absoluteInput = resolve(inputPath);
  const baseDirectory = dirname(absoluteInput);
  const failures = [];
  let input;
  try {
    input = JSON.parse(await readFile(absoluteInput, "utf8"));
  } catch (error) {
    return {
      input: null,
      failures: [`cannot read analyzer-input-v5.json: ${error.message}`],
    };
  }
  const workload = await loadMaterializedComparisonWorkloadV5();
  const workloadV4 = await loadMaterializedComparisonWorkloadV4();
  const plan = buildV5ComparisonPlan(workload);
  const expectedSchedule = Number.isInteger(input?.schedule_seed)
    ? buildV5ExecutionSchedule(plan, { seed: input.schedule_seed })
    : [];
  if (!Number.isInteger(input?.schedule_seed)) {
    failures.push("schedule_seed must be an integer");
  }
  const expectedLaunchById = new Map(
    expectedSchedule.map((run) => {
      const launchId =
        run.phase === "correctness"
          ? `correctness-${run.implementation}-${run.component}`
          : `${run.bundle_id}-component${run.component_index + 1}-${run.component}`;
      return [launchId, run];
    }),
  );
  const rootOfTrust = await validateAnalyzerRootOfTrustV5({
    input,
    baseDirectory,
    candidateValidator,
  });
  failures.push(...rootOfTrust.failures);
  pushMismatch(
    failures,
    input.schema_version === 1,
    "analyzer input schema_version must be 1",
  );
  pushMismatch(
    failures,
    input.protocol_version === protocolVersionV5,
    `protocol_version must be ${protocolVersionV5}`,
  );
  pushMismatch(
    failures,
    input.decision_contract_version === decisionContractVersionV5,
    `decision contract must be ${decisionContractVersionV5}`,
  );
  pushMismatch(
    failures,
    input.workload_artifact_sha256 === expectedWorkloadArtifactSha256V5 &&
      input.workload_artifact_sha256 ===
        comparisonWorkloadArtifactHashV5(workload),
    "workload artifact SHA-256 is not the frozen v5 value",
  );
  pushMismatch(
    failures,
    input.workload_byte_sha256 === expectedWorkloadByteSha256V5 &&
      input.workload_byte_sha256 === comparisonWorkloadByteHashV5(workload),
    "workload byte SHA-256 is not the frozen v5 value",
  );
  pushMismatch(
    failures,
    plan.ready === true &&
      plan.blockers.length === 0 &&
      plan.decision_contract_sha256 === expectedDecisionContractSha256V5 &&
      expectedDecisionContractSha256V5 ===
        "2acdab1dc3f62c1eed82f5d9af9f50c525617cac49c3c4b60fd885116563cfb1",
    "analyzer decision-contract artifact identity changed",
  );
  for (const implementation of implementations) {
    pushMismatch(
      failures,
      sha256Pattern.test(
        input.candidate_manifest_sha256?.[implementation] ?? "",
      ),
      `${implementation} candidate manifest SHA-256 is invalid`,
    );
  }
  pushMismatch(
    failures,
    same(input.reference_crop_sha256_by_id, referenceCropHashes(workload)),
    "reference crop hashes do not match the frozen workload",
  );

  const property = [];
  const gpuHostAssessments = [];
  const independentLaunchRecords = [];
  for (const implementation of implementations) {
    const refs = (input.property_correctness ?? []).filter(
      (item) => item.implementation === implementation,
    );
    if (refs.length !== 1) {
      failures.push(
        `${implementation} property correctness requires exactly one report`,
      );
      continue;
    }
    const raw = await loadHashedJson(
      baseDirectory,
      {
        path: refs[0].raw_report_path,
        sha256: refs[0].raw_report_sha256,
      },
      `${implementation} property correctness raw report`,
    );
    if (raw.failure) failures.push(raw.failure);
    if (raw.value) {
      const gpu = validateGpuAndHostEvidenceV5(raw.value);
      gpuHostAssessments.push(gpu);
      failures.push(
        ...gpu.failures.map(
          (failure) => `${implementation} property correctness: ${failure}`,
        ),
      );
    }
    const loaded = await loadHashedJson(
      baseDirectory,
      {
        path: refs[0].hard_report_path,
        sha256: refs[0].hard_report_sha256,
      },
      `${implementation} property correctness`,
    );
    if (loaded.failure) failures.push(loaded.failure);
    if (!loaded.value || !raw.value) continue;
    try {
      const reconstructed = buildHardComponentReportV5({
        workload,
        rawReport: raw.value,
        run: {
          implementation,
          journey: "dense-mixed-editing",
          component: "native-property-edit-undo",
        },
        candidateArtifactSha256:
          input.candidate_manifest_sha256[implementation],
      });
      if (!same(reconstructed, loaded.value))
        failures.push(
          `${implementation} property hard report is not bound to its raw report`,
        );
    } catch (error) {
      failures.push(
        `${implementation} property hard report cannot be reconstructed from raw evidence: ${error.message}`,
      );
    }
    const assessment = validateHardComponentReportV5(workload, loaded.value);
    failures.push(
      ...assessment.failures.map(
        (failure) => `${implementation} property correctness: ${failure}`,
      ),
    );
    if (
      refs[0].component !== "native-property-edit-undo" ||
      refs[0].passed !== true ||
      refs[0].correctness_passed !== assessment.correctness_passed ||
      (refs[0].known_baseline_defect_id ?? null) !==
        (assessment.known_baseline_defect_id ?? null)
    )
      failures.push(
        `${implementation} property correctness metadata does not match the report`,
      );
    if (
      loaded.value.candidate_artifact_sha256 !==
      input.candidate_manifest_sha256[implementation]
    )
      failures.push(
        `${implementation} property correctness candidate hash does not match`,
      );
    if (
      loaded.value.workload_byte_sha256 !== expectedWorkloadByteSha256V5 ||
      !receiptsAreAuthentic(loaded.value)
    )
      failures.push(
        `${implementation} property correctness workload bytes or command receipt hashes are invalid`,
      );
    property.push({
      implementation,
      reference: refs[0],
      report: loaded.value,
      assessment,
    });
    independentLaunchRecords.push({
      expected: {
        ...expectedLaunchById.get(
          `correctness-${implementation}-native-property-edit-undo`,
        ),
        launch_id: `correctness-${implementation}-native-property-edit-undo`,
        candidate_manifest_sha256:
          input.candidate_manifest_sha256[implementation],
        fixture_sha256_by_id: loaded.value.fixture_sha256_by_id,
      },
      binding: raw.value.launch_binding_v5,
      hard_binding: loaded.value.launch_binding_v5,
      raw_path: raw.path,
      raw_sha256: refs[0].raw_report_sha256,
      hard_path: loaded.path,
      hard_sha256: refs[0].hard_report_sha256,
      captures: [],
    });
  }

  const bundleRecords = [];
  const identities = new Set();
  const rawPaths = new Set();
  const allHardPaths = new Set();
  const retainedHardReports = [];
  for (const entry of input.bundles ?? []) {
    const identity = `${entry.phase}:${entry.journey}:${entry.pair}:${entry.implementation}`;
    if (identities.has(identity))
      failures.push(`${identity}: duplicate bundle identity`);
    identities.add(identity);
    const loaded = await loadHashedJson(
      baseDirectory,
      entry,
      `${identity} bundle`,
    );
    const recordFailures = [];
    if (loaded.failure) recordFailures.push(loaded.failure);
    const bundle = loaded.value;
    const planJourney = plan.journeys.find(
      ({ scenario }) => scenario === entry.journey,
    );
    if (bundle && planJourney) {
      for (const key of [
        "phase",
        "journey",
        "pair",
        "pair_position",
        "implementation",
      ]) {
        if (bundle[key] !== entry[key])
          recordFailures.push(`bundle ${key} does not match analyzer input`);
      }
      if (
        bundle.schema_version !== 1 ||
        bundle.protocol_version !== protocolVersionV5 ||
        bundle.decision_contract_version !== decisionContractVersionV5 ||
        bundle.scenario_contract_version !== scenarioContractVersionV5 ||
        bundle.workload_artifact_sha256 !== input.workload_artifact_sha256 ||
        bundle.workload_byte_sha256 !== input.workload_byte_sha256 ||
        bundle.passed !== true ||
        bundle.inference_eligible !== (bundle.phase === "final")
      )
        recordFailures.push(
          "bundle v5 identity, pass state, or inference phase is invalid",
        );
      if (
        bundle.candidate_artifact?.sha256 !==
        input.candidate_manifest_sha256[bundle.implementation]
      )
        recordFailures.push(
          "bundle candidate SHA-256 does not match analyzer input",
        );
      if (
        !same(bundle.fixture_ids, planJourney.fixture_ids) ||
        !same(bundle.fixture_sha256_by_id, planJourney.fixture_sha256_by_id)
      )
        recordFailures.push("bundle fixtures do not match the frozen v5 plan");
      if (
        !same(
          bundle.component_aggregation?.order,
          planJourney.component_order,
        ) ||
        !same(
          bundle.component_aggregation?.weights,
          planJourney.component_weights,
        )
      )
        recordFailures.push(
          "bundle component order or weights do not match the frozen v5 plan",
        );
      if (
        !same(
          bundle.components?.map(({ component }) => component),
          planJourney.component_order,
        )
      )
        recordFailures.push("bundle component list is not exact");
      const components = [];
      for (const [index, componentEntry] of (
        bundle.components ?? []
      ).entries()) {
        const expectedHard = benefitComponents.includes(
          componentEntry.component,
        );
        if (
          componentEntry.component_index !== index ||
          componentEntry.component_weight !==
            planJourney.component_weights[index] ||
          componentEntry.input_lane !==
            (expectedHard
              ? "native-x11-xtest"
              : componentInputLaneV4(componentEntry.component)) ||
          componentEntry.hard_component !== expectedHard
        ) {
          recordFailures.push(
            `${componentEntry.component}: component index, weight, lane, or hard-component identity is invalid`,
          );
        }
        const rawRef = {
          path: componentEntry.raw_report_path,
          sha256: componentEntry.raw_report_sha256,
        };
        const raw = await loadHashedJson(
          baseDirectory,
          rawRef,
          `${identity}:${componentEntry.component} raw report`,
        );
        if (raw.path && rawPaths.has(raw.path))
          recordFailures.push(
            `${componentEntry.component}: raw report path was reused`,
          );
        if (raw.path) rawPaths.add(raw.path);
        if (raw.failure) recordFailures.push(raw.failure);
        let assessment = null;
        if (raw.value) {
          const gpu = validateGpuAndHostEvidenceV5(raw.value);
          gpuHostAssessments.push(gpu);
          recordFailures.push(
            ...gpu.failures.map(
              (failure) => `${componentEntry.component}: ${failure}`,
            ),
          );
          if (componentEntry.hard_component === true) {
            const hardRef = {
              path: componentEntry.hard_report_path,
              sha256: componentEntry.hard_report_sha256,
            };
            const hard = await loadHashedJson(
              baseDirectory,
              hardRef,
              `${identity}:${componentEntry.component} hard report`,
            );
            if (hard.path && allHardPaths.has(hard.path))
              recordFailures.push(
                `${componentEntry.component}: hard report path was reused`,
              );
            if (hard.path) allHardPaths.add(hard.path);
            if (hard.failure) recordFailures.push(hard.failure);
            if (hard.value) {
              try {
                const reconstructed = buildHardComponentReportV5({
                  workload,
                  rawReport: raw.value,
                  run: {
                    implementation: bundle.implementation,
                    journey: bundle.journey,
                    component: componentEntry.component,
                  },
                  candidateArtifactSha256:
                    input.candidate_manifest_sha256[bundle.implementation],
                });
                if (!same(reconstructed, hard.value))
                  recordFailures.push(
                    `${componentEntry.component}: normalized hard report is not bound to its raw report`,
                  );
              } catch (error) {
                recordFailures.push(
                  `${componentEntry.component}: cannot reconstruct hard report from raw evidence: ${error.message}`,
                );
              }
              const hardAssessment = validateHardComponentReportV5(
                workload,
                hard.value,
              );
              const rawBenefitEvidence = assessRawBenefitEvidenceV5(raw.value, {
                benefit_metrics_eligible: true,
                input_lane: componentEntry.input_lane,
              });
              const benefitEligible =
                hardAssessment.benefit_metrics_eligible === true &&
                rawBenefitEvidence.eligible;
              const allowedMissingBenefitEvidence =
                hardAssessment.known_baseline_defect_id ===
                exactElectronMultiDefect;
              assessment = {
                ...hardAssessment,
                benefit_metrics_eligible: benefitEligible,
                measurements: benefitEligible
                  ? hardAssessment.measurements
                  : {},
              };
              recordFailures.push(
                ...assessment.failures.map(
                  (failure) => `${componentEntry.component}: ${failure}`,
                ),
                ...(allowedMissingBenefitEvidence
                  ? []
                  : rawBenefitEvidence.blockers.map(
                      (blocker) =>
                        `${componentEntry.component}: benefit evidence blocked: ${blocker}`,
                    )),
              );
              if (
                hard.value.candidate_artifact_sha256 !==
                input.candidate_manifest_sha256[bundle.implementation]
              )
                recordFailures.push(
                  `${componentEntry.component}: hard report candidate hash does not match`,
                );
              if (!reportReferenceMatches(componentEntry, assessment))
                recordFailures.push(
                  `${componentEntry.component}: bundle assessment does not match hard report`,
                );
              retainedHardReports.push({
                entry,
                componentEntry,
                report: hard.value,
                assessment,
              });
            }
          } else {
            const scenarioContract = buildScenarioContractV4(
              workloadV4,
              bundle.journey,
            );
            const fixtureSha =
              representativeScenarioDefinitionsV4[bundle.journey]
                ?.fixture_sha256;
            const exactReport = validateV4ComponentReport({
              report: raw.value,
              implementation: bundle.implementation,
              journey: bundle.journey,
              component: componentEntry.component,
              fixture: { sha256: fixtureSha },
              scenarioContract,
            });
            recordFailures.push(
              ...exactReport.errors.map(
                (failure) => `${componentEntry.component}: ${failure}`,
              ),
            );
            const measurements = extractComponentMeasurementsV4(raw.value, {
              nativeComponent:
                componentInputLaneV4(componentEntry.component) ===
                "native-x11-xtest",
            });
            recordFailures.push(
              ...measurements.missing.map(
                (missing) =>
                  `${componentEntry.component}: measurement missing: ${missing}`,
              ),
            );
            const benefitEvidence = assessRawBenefitEvidenceV5(raw.value, {
              benefit_metrics_eligible: true,
              input_lane: componentInputLaneV4(componentEntry.component),
            });
            recordFailures.push(
              ...benefitEvidence.blockers.map(
                (blocker) =>
                  `${componentEntry.component}: benefit evidence blocked: ${blocker}`,
              ),
            );
            assessment = {
              measurements: benefitEvidence.eligible ? measurements.values : {},
              benefit_metrics_eligible: benefitEvidence.eligible,
            };
            if (!same(componentEntry.measurements, assessment.measurements))
              recordFailures.push(
                `${componentEntry.component}: bundle measurements do not match raw report`,
              );
          }
        }
        if (raw.value && componentEntry.benefit_metrics_eligible === true) {
          const viewState = buildViewStateReceiptV5(raw.value, {
            implementation: bundle.implementation,
            journey: bundle.journey,
            component: componentEntry.component,
            fixture_ids:
              planJourney.component_fixture_ids?.[componentEntry.component] ??
              [],
          });
          recordFailures.push(
            ...viewState.failures.map(
              (failure) => `${componentEntry.component}: ${failure}`,
            ),
          );
          if (!same(componentEntry.view_state_receipt, viewState.receipt)) {
            recordFailures.push(
              `${componentEntry.component}: bundle view-state receipt does not match raw live events`,
            );
          }
        } else if (componentEntry.view_state_receipt != null) {
          recordFailures.push(
            `${componentEntry.component}: benefit-ineligible component retained a view-state receipt`,
          );
        }
        components.push({
          component: componentEntry.component,
          component_index: index,
          component_weight: componentEntry.component_weight,
          input_lane: componentEntry.input_lane,
          raw_report_sha256: componentEntry.raw_report_sha256,
          command_receipts: componentEntry.command_receipts ?? [],
          report_valid: assessment != null,
          measurements: assessment
            ? { values: assessment.measurements ?? {}, missing: [] }
            : { values: {}, missing: ["invalid raw or hard report"] },
        });
        if (raw.value) {
          independentLaunchRecords.push({
            expected: {
              ...expectedLaunchById.get(
                `${entry.bundle_id}-component${index + 1}-${componentEntry.component}`,
              ),
              launch_id: `${entry.bundle_id}-component${index + 1}-${componentEntry.component}`,
              candidate_manifest_sha256:
                input.candidate_manifest_sha256[entry.implementation],
              fixture_sha256_by_id: Object.fromEntries(
                (
                  planJourney.component_fixture_ids?.[
                    componentEntry.component
                  ] ?? []
                ).map((fixtureId) => [
                  fixtureId,
                  planJourney.fixture_sha256_by_id[fixtureId],
                ]),
              ),
            },
            binding: raw.value.launch_binding_v5,
            hard_binding:
              componentEntry.hard_component === true
                ? retainedHardReports.at(-1)?.report?.launch_binding_v5
                : null,
            raw_path: raw.path,
            raw_sha256: componentEntry.raw_report_sha256,
            hard_path:
              componentEntry.hard_component === true
                ? localPath(baseDirectory, componentEntry.hard_report_path)
                : null,
            hard_sha256: componentEntry.hard_report_sha256,
            captures: componentEntry.dynamic_crop_artifacts ?? [],
          });
        }
      }
      const commandIds = (bundle.components ?? []).flatMap(
        (component) =>
          component.command_receipts?.map((receipt) => receipt.command_id) ??
          [],
      );
      if (
        !same(bundle.command_ids, commandIds) ||
        !same([...commandIds].sort(), [...planJourney.command_ids].sort())
      )
        recordFailures.push(
          "bundle command receipts do not cover the journey exactly once",
        );
      bundleRecords.push({
        entry,
        bundle,
        components,
        failures: recordFailures,
      });
    } else {
      if (!planJourney)
        recordFailures.push("bundle journey is not representative v5");
      bundleRecords.push({
        entry,
        bundle: bundle ?? null,
        components: [],
        failures: recordFailures,
      });
    }
    failures.push(
      ...recordFailures.map((failure) => `${identity}: ${failure}`),
    );
  }
  const expectedBundleCount =
    representativeTimedScenarioIdsV5.length *
    (calibrationPairCount + finalPairCount) *
    implementations.length;
  if ((input.bundles ?? []).length !== expectedBundleCount)
    failures.push(
      `retained bundle count must be exactly ${expectedBundleCount}`,
    );
  const pairOrderValidation = validatePairOrdersV5(
    input.bundles ?? [],
    input.schedule_seed,
  );
  failures.push(...pairOrderValidation.failures);
  const orders = pairOrderValidation.orders;
  const derivedViewStatePairs = [];
  for (const electronRecord of bundleRecords.filter(
    ({ bundle }) => bundle?.implementation === "electron",
  )) {
    const gpuiRecord = bundleRecords.find(
      ({ bundle }) =>
        bundle?.implementation === "gpui" &&
        bundle.phase === electronRecord.bundle.phase &&
        bundle.journey === electronRecord.bundle.journey &&
        bundle.pair === electronRecord.bundle.pair,
    );
    if (!gpuiRecord) continue;
    derivedViewStatePairs.push(
      compareBundleViewStatesV5(electronRecord.bundle, gpuiRecord.bundle),
    );
  }
  const expectedViewStatePairCount =
    representativeTimedScenarioIdsV5.length *
    (calibrationPairCount + finalPairCount);
  if (derivedViewStatePairs.length !== expectedViewStatePairCount) {
    failures.push(
      `matched view-state pair count must be exactly ${expectedViewStatePairCount}`,
    );
  }
  for (const pair of derivedViewStatePairs) {
    failures.push(
      ...pair.failures.map(
        (failure) =>
          `${pair.phase} ${pair.journey} pair ${pair.pair} view state: ${failure}`,
      ),
    );
  }
  if (!same(input.view_state_pairs, derivedViewStatePairs)) {
    failures.push(
      "view_state_pairs do not match the retained Electron and GPUI bundle receipts",
    );
  }
  const frozenHost = validateFrozenHostGpuIdentityV5(gpuHostAssessments);
  failures.push(...frozenHost.failures);
  const identityCollection = await collectIndependentFileIdentitiesV5(
    independentLaunchRecords,
  );
  failures.push(...identityCollection.failures);
  const independentLaunches = validateIndependentLaunchBindingsV5(
    identityCollection.records,
  );
  failures.push(...independentLaunches.failures);

  const normalizedHardReferences = (input.hard_component_reports ?? []).map(
    (reference) => ({
      ...reference,
      path: localPath(baseDirectory, reference.path),
    }),
  );
  const hardReferenceValidation = validateFinalHardReferencesV5(
    normalizedHardReferences,
    retainedHardReports,
  );
  failures.push(...hardReferenceValidation.failures);
  const hardReports = hardReferenceValidation.retained.map(
    ({ reference, retained }) => ({
      ...reference,
      report: retained.report,
      assessment: retained.assessment,
    }),
  );
  const cropArtifactValidation = await validateDynamicCropArtifactsV5({
    hardReports,
    workload,
    artifactTree: input.artifact_tree,
  });
  failures.push(...cropArtifactValidation.failures);
  if (
    (input.hard_component_reports ?? []).length !==
    finalPairCount * implementations.length * benefitComponents.length
  )
    failures.push("final hard report count must be exactly 144");

  const derivedDynamicPairs = Array.from(
    { length: finalPairCount },
    (_, index) => {
      const pair = index + 1;
      return {
        pair,
        electron:
          hardReports.find(
            (item) =>
              item.pair === pair &&
              item.implementation === "electron" &&
              item.component === "viewer-dynamic-fidelity",
          )?.assessment.quality_measurements ?? null,
        gpui:
          hardReports.find(
            (item) =>
              item.pair === pair &&
              item.implementation === "gpui" &&
              item.component === "viewer-dynamic-fidelity",
          )?.assessment.quality_measurements ?? null,
      };
    },
  );
  if (!same(input.dynamic_fidelity_pairs, derivedDynamicPairs))
    failures.push("dynamic_fidelity_pairs do not match exact hard reports");

  const v4PlanComponents = new Map(
    representativeTimedScenarioIdsV4.map((scenario) => [
      scenario,
      new Set(
        representativeScenarioDefinitionsV4[scenario].current_runner_components,
      ),
    ]),
  );
  const v4Bundles = bundleRecords
    .filter((record) => v4PlanComponents.has(record.entry.journey))
    .map((record) => ({
      ...record,
      components: record.components.filter((component) =>
        v4PlanComponents.get(record.entry.journey).has(component.component),
      ),
    }));
  const v4Verified = {
    input: baseDirectory,
    manifest: syntheticV4Manifest(input, workloadV4, orders, plan),
    bundles: v4Bundles,
    failures: [...failures],
  };
  return {
    input,
    workload,
    workloadV4,
    plan,
    property,
    bundleRecords,
    hardReports,
    cropArtifactValidation,
    dynamicPairs: derivedDynamicPairs,
    frozenHostGpuIdentity: frozenHost.identity,
    rootOfTrust,
    v4Verified,
    failures,
  };
}

function buildCorrectness(verified) {
  const failures = [];
  const electronProperty = verified.property.find(
    ({ implementation }) => implementation === "electron",
  )?.assessment;
  const gpuiProperty = verified.property.find(
    ({ implementation }) => implementation === "gpui",
  )?.assessment;
  if (
    electronProperty?.correctness_passed !== true &&
    electronProperty?.known_baseline_defect_id !== exactElectronPropertyDefect
  )
    failures.push(
      "Electron property outcome is neither an exact pass nor the allowed known baseline defect",
    );
  if (gpuiProperty?.correctness_passed !== true)
    failures.push("GPUI property edit and one exact undo did not pass");
  for (const component of benefitComponents) {
    const componentReports = verified.hardReports.filter(
      (item) => item.component === component,
    );
    if (componentReports.length !== finalPairCount * 2)
      failures.push(`${component}: final correctness evidence is incomplete`);
    for (const item of componentReports) {
      if (
        item.implementation === "gpui" &&
        item.assessment.correctness_passed !== true
      )
        failures.push(
          `${component}:pair-${item.pair}: GPUI correctness did not pass`,
        );
      if (
        item.implementation === "electron" &&
        item.assessment.correctness_passed !== true &&
        !(
          component === "multi-document-session" &&
          item.assessment.known_baseline_defect_id === exactElectronMultiDefect
        )
      )
        failures.push(
          `${component}:pair-${item.pair}: Electron outcome is neither pass nor the exact allowance`,
        );
    }
  }
  return {
    executable: true,
    passed: failures.length === 0,
    gpui_all_passed: failures.every((failure) => !failure.includes("GPUI")),
    electron_allowed_defects: [
      ...(electronProperty?.known_baseline_defect_id ===
      exactElectronPropertyDefect
        ? [exactElectronPropertyDefect]
        : []),
      ...(verified.hardReports.some(
        (item) =>
          item.assessment.known_baseline_defect_id === exactElectronMultiDefect,
      )
        ? [exactElectronMultiDefect]
        : []),
    ],
    failures,
  };
}

export async function analyzePairedV5(inputPath, options = {}) {
  let verified;
  try {
    verified = await loadVerifiedV5AnalyzerInput(inputPath, options);
  } catch (error) {
    const evidence = {
      validation_failures: [
        `analyzer evidence validation failed closed: ${error.message}`,
      ],
      v4_decision: { eligibility: "not-decision-ready" },
      correctness: { executable: false },
      dynamic_fidelity: { executable: false },
      v5_benefit_metrics: { executable: false },
    };
    return { ...composeDecisionV5(evidence), evidence };
  }
  if (!verified.input) {
    const evidence = {
      validation_failures: verified.failures,
      v4_decision: { eligibility: "not-decision-ready" },
      correctness: { executable: false },
      dynamic_fidelity: { executable: false },
      v5_benefit_metrics: { executable: false },
    };
    return { ...composeDecisionV5(evidence), evidence };
  }
  const v4Summary = summarizeVerifiedV4Run(verified.v4Verified);
  const v4Decision = evaluateMigrationDecision(
    buildDecisionEvidenceV4(v4Summary),
  );
  const correctness = buildCorrectness(verified);
  const dynamicFidelity = analyzeDynamicFidelityPairsV5(verified.dynamicPairs);
  const v5BenefitMetrics = analyzeV5BenefitMetrics(verified.hardReports);
  const evidence = {
    validation_failures: verified.failures,
    v4_summary: v4Summary,
    v4_decision: v4Decision,
    correctness,
    dynamic_fidelity: dynamicFidelity,
    v5_benefit_metrics: v5BenefitMetrics,
    evidence_boundary: {
      compared:
        "optimized unpackaged Electron/PDF.js and GPUI/PDFium candidates on the same NVIDIA Linux host",
      macos_visual_capture: "not-run",
      windows_platform_testing: "not-run",
      hibbeler_private_corpus: "blocked-not-transferred",
      packaged_release_qualification: "not-run",
    },
  };
  return { ...composeDecisionV5(evidence), evidence };
}

export function renderDecisionMarkdownV5(result) {
  const headline = result.decision ?? "BLOCKED";
  const sections = [
    "# Butter Paper GPUI migration funding decision",
    "",
    `Decision: **${headline}**`,
    "",
    `Comparison: ${result.comparison}.`,
    "",
  ];
  for (const [name, items] of Object.entries(result.checks)) {
    sections.push(`## ${name.replace("_", " ")}`, "");
    sections.push(
      ...(items.length > 0 ? items.map((item) => `- ${item}`) : ["- None"]),
      "",
    );
  }
  sections.push("## Interpretation", "", result.rule, "");
  return sections.join("\n");
}

export function parseAnalyzeV5Arguments(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index];
    if (option === "-h" || option === "--help") return { help: true };
    if (!["--input", "--output"].includes(option))
      throw new Error(`unknown option: ${option}`);
    const value = argv[++index];
    if (!value || value.startsWith("--"))
      throw new Error(`${option} requires a value`);
    options[option.slice(2)] = resolve(value);
  }
  if (!options.input) throw new Error("--input is required");
  options.output ??= dirname(options.input);
  return options;
}

function usage() {
  return `Usage: node analyze-paired-v5.mjs --input <analyzer-input-v5.json> [--output <directory>]\n\nVerifies all frozen v5 identities, hashes, raw and hard reports, NVIDIA receipts,\npair balance, crop/scale evidence, and exact Electron allowances. Writes\npaired-decision-v5.json and paired-decision-v5.md. Invalid or incomplete evidence\nis BLOCKED and never converted into a guessed NO.\n`;
}

async function main() {
  const options = parseAnalyzeV5Arguments(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(usage());
    return;
  }
  const result = await analyzePairedV5(options.input);
  await mkdir(options.output, { recursive: true });
  const jsonPath = resolve(options.output, "paired-decision-v5.json");
  const markdownPath = resolve(options.output, "paired-decision-v5.md");
  await writeFile(jsonPath, `${JSON.stringify(result, null, 2)}\n`);
  await writeFile(markdownPath, renderDecisionMarkdownV5(result));
  process.stdout.write(
    `${JSON.stringify({ status: result.status, decision: result.decision, json: jsonPath, markdown: markdownPath }, null, 2)}\n`,
  );
  if (result.status === "blocked") process.exitCode = 2;
  else if (result.decision === "NO") process.exitCode = 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`${error.stack ?? error.message}\n`);
    process.exitCode = 2;
  });
}
