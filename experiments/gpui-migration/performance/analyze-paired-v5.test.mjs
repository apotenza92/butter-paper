import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  chmod,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import test from "node:test";

import sharp from "sharp";

import {
  buildNvidiaBaselineResult,
  qualifyNvidiaEvidence,
  summarizeNvidiaIterations,
} from "./nvidia-sampler.mjs";
import {
  analyzeV5BenefitMetrics,
  analyzePairedV5,
  analyzeGpuiMultiDocumentAbsoluteSafetyV5,
  composeDecisionV5,
  collectIndependentFileIdentitiesV5,
  renderDecisionMarkdownV5,
  validateFinalHardReferencesV5,
  validateDynamicCropArtifactsV5,
  validateFrozenHostGpuIdentityV5,
  validateGpuAndHostEvidenceV5,
  validateIndependentLaunchBindingsV5,
  validatePairOrdersV5,
  validateAnalyzerRootOfTrustV5,
} from "./analyze-paired-v5.mjs";
import { createCalibrationPairOrdersV5 } from "./run-paired-v5.mjs";
import { createBalancedPairOrders } from "./decision-statistics.mjs";
import { registerAndComparePresentedCropV2 } from "./registered-crop-v5.mjs";

async function artifact(path) {
  const bytes = await readFile(path);
  return {
    path,
    bytes: bytes.length,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  };
}

function gpuiAdapterReceipt(overrides = {}) {
  return {
    event: "gpu-adapter-selected",
    available: true,
    is_software_emulated: false,
    device_name: "NVIDIA RTX 4000 Ada Generation",
    driver_name: "NVIDIA",
    driver_info: "580.173.02",
    ...overrides,
  };
}

function electronBrowserGpuInfo(overrides = {}) {
  return {
    gpu: {
      devices: [
        {
          active: true,
          vendorId: 0x10de,
          deviceString: "NVIDIA RTX 4000 Ada Generation",
          driverVersion: "580.173.02",
        },
      ],
      auxAttributes: {
        glRenderer: "NVIDIA RTX 4000 Ada Generation",
      },
      featureStatus: { gpu_compositing: "enabled_on" },
      ...overrides,
    },
  };
}

function strictGpuReport(hostname = "gpu-host-1", implementation = "gpui") {
  const sample = (elapsed, utilization, memory, power) => ({
    elapsed_ms: elapsed,
    timestamp: `2026/08/23 10:00:0${elapsed}`,
    index: 0,
    gpu_utilization_percent: utilization,
    memory_used_mib: memory,
    memory_total_mib: 20475,
    power_draw_watts: power,
  });
  const gpu = buildNvidiaBaselineResult({
    baselineSamples: [
      sample(0, 1, 100, 20),
      sample(200, 2, 101, 21),
      sample(400, 1, 100, 20),
    ],
    runSamples: [sample(600, 31, 300, 70), sample(800, 41, 400, 80)],
  });
  gpu.qualification = qualifyNvidiaEvidence(gpu, { required: true });
  const iteration = {
    gpu,
    ...(implementation === "gpui"
      ? { events: [gpuiAdapterReceipt()] }
      : { renderer: { browser_gpu_info: electronBrowserGpuInfo() } }),
  };
  return {
    implementation,
    requested_iterations: 1,
    provenance: {
      host: {
        hostname,
        os_type: "Linux",
        platform: "linux",
        os_release: "6.8.0-test",
        architecture: "x64",
        display_mode:
          "Screen 0: minimum 8 x 8, current 1920 x 1080\nHDMI-0 connected primary 1920x1080+0+0",
        nvidia_gpu:
          "NVIDIA RTX 4000 Ada Generation, GPU-87203edb-af3c-bf52-7395-ee3ad8f7389f, 580.173.02, 20475",
        vulkan_summary:
          "vendorID = 0x10de\ndeviceID = 0x27b2\ndeviceType = PHYSICAL_DEVICE_TYPE_DISCRETE_GPU\ndeviceName = NVIDIA RTX 4000 Ada Generation\ndriverInfo = 580.173.02\ndeviceUUID = 87203edb-af3c-bf52-7395-ee3ad8f7389f",
      },
    },
    iterations: [iteration],
    summary: summarizeNvidiaIterations([iteration]),
  };
}

function syntheticHardBenefitReports() {
  const reports = [];
  for (let pair = 1; pair <= 24; pair += 1) {
    reports.push({
      pair,
      component: "multi-document-session",
      implementation: "electron",
      assessment: {
        benefit_metrics_eligible: false,
        known_baseline_defect_id:
          "electron-multi-document-second-nasa-visible-pages-empty-v1",
        measurements: {},
      },
    });
    reports.push({
      pair,
      component: "multi-document-session",
      implementation: "gpui",
      assessment: {
        benefit_metrics_eligible: true,
        measurements: {
          cpu_seconds: 10,
          cgroup_peak_memory_bytes: 500_000_000,
          product_wall_or_latency_ms: 10_000,
          product_wall_or_latency_source: "product-latency",
          application_frame_interval_p95_ms: 16,
          native_input_to_application_frame_ack_p95_ms: 20,
          baseline_adjusted_gpu_peak_memory_mib: 500,
          baseline_adjusted_gpu_utilization_p95_percent: 50,
        },
      },
    });
    for (const component of [
      "native-snap-transform-120hz",
      "viewer-dynamic-fidelity",
    ]) {
      for (const implementation of ["electron", "gpui"]) {
        const gpui = implementation === "gpui";
        reports.push({
          pair,
          component,
          implementation,
          assessment: {
            benefit_metrics_eligible: true,
            measurements: {
              cpu_seconds: gpui ? 0.7 : 1,
              cgroup_peak_memory_bytes: gpui ? 700 : 1_000,
              application_frame_interval_p95_ms: gpui ? 9 : 10,
              native_input_to_application_frame_ack_p95_ms: gpui ? 18 : 20,
              product_wall_or_latency_ms: gpui ? 8 : 10,
              baseline_adjusted_gpu_peak_memory_mib: gpui ? 7 : 10,
              baseline_adjusted_gpu_utilization_p95_percent: gpui ? 7 : 10,
            },
          },
        });
      }
    }
  }
  return reports;
}

function decisionReadyEvidence() {
  return {
    validation_failures: [],
    v4_decision: {
      eligibility: "decision-ready",
      decision: "yes",
      metric_failures: [],
    },
    correctness: {
      passed: true,
      gpui_all_passed: true,
      electron_allowed_defects: [
        "electron-numeric-property-input-blur-duplicate-history-v1",
        "electron-multi-document-second-nasa-visible-pages-empty-v1",
      ],
      failures: [],
    },
    dynamic_fidelity: { decision_ready: true, failures: [] },
    v5_benefit_metrics: {
      decision_ready: true,
      failures: [],
      explicitly_ineligible: [
        {
          component: "multi-document-session",
          implementation: "electron",
          reason: "electron-multi-document-second-nasa-visible-pages-empty-v1",
        },
      ],
    },
  };
}

test("composeDecisionV5 returns a clean YES only for complete passing evidence", () => {
  const result = composeDecisionV5(decisionReadyEvidence());
  assert.equal(result.status, "decision-ready");
  assert.equal(result.decision, "YES");
  assert.equal(result.worth_funding_migration, true);
  assert.deepEqual(result.checks.failed, []);
  assert.deepEqual(result.checks.blocked, []);
  assert.ok(result.checks.not_run.some((item) => item.includes("Hibbeler")));
});

test("composeDecisionV5 returns NO for a measured regression after eligibility passes", () => {
  const evidence = decisionReadyEvidence();
  evidence.v5_benefit_metrics.decision_ready = false;
  evidence.v5_benefit_metrics.failures = [
    "product_latency upper_95 1.3 exceeds 1.15",
  ];
  const result = composeDecisionV5(evidence);
  assert.equal(result.status, "decision-ready");
  assert.equal(result.decision, "NO");
  assert.equal(result.worth_funding_migration, false);
  assert.deepEqual(result.checks.blocked, []);
  assert.match(result.checks.failed[0], /product_latency/);
});

test("composeDecisionV5 returns BLOCKED, never guessed NO, for invalid evidence", () => {
  const evidence = decisionReadyEvidence();
  evidence.validation_failures = ["bundle SHA-256 mismatch"];
  const result = composeDecisionV5(evidence);
  assert.equal(result.status, "blocked");
  assert.equal(result.decision, null);
  assert.equal(result.worth_funding_migration, null);
  assert.deepEqual(result.checks.failed, []);
  assert.match(result.checks.blocked[0], /SHA-256/);
});

test("analyzeV5BenefitMetrics passes eligible additions and retains exact missing data", () => {
  const result = analyzeV5BenefitMetrics(syntheticHardBenefitReports());
  assert.equal(result.executable, true);
  assert.equal(result.decision_ready, true);
  assert.equal(result.explicitly_ineligible.length, 1);
  assert.deepEqual(result.components["multi-document-session"].families, {});
});

test("analyzeV5BenefitMetrics distinguishes measured NO from incomplete BLOCKED evidence", () => {
  const regression = syntheticHardBenefitReports();
  for (const report of regression) {
    if (
      report.component === "native-snap-transform-120hz" &&
      report.implementation === "gpui"
    ) {
      report.assessment.measurements.product_wall_or_latency_ms = 20;
    }
  }
  const measured = analyzeV5BenefitMetrics(regression);
  assert.equal(measured.executable, true);
  assert.equal(measured.decision_ready, false);
  assert.match(measured.metric_failures[0], /product_latency/);

  const incomplete = syntheticHardBenefitReports();
  delete incomplete.find(
    (report) =>
      report.component === "viewer-dynamic-fidelity" && report.pair === 1,
  ).assessment.measurements.cpu_seconds;
  const blocked = analyzeV5BenefitMetrics(incomplete);
  assert.equal(blocked.executable, false);
  assert.match(blocked.blocking_failures[0], /measurements are missing/);

  const correctnessOnly = syntheticHardBenefitReports();
  correctnessOnly.find(
    (report) =>
      report.component === "native-snap-transform-120hz" &&
      report.implementation === "gpui" &&
      report.pair === 1,
  ).assessment.benefit_metrics_eligible = false;
  const rejected = analyzeV5BenefitMetrics(correctnessOnly);
  assert.equal(rejected.executable, false);
  assert.match(rejected.blocking_failures[0], /benefit evidence is ineligible/);
});

test("GPUI-only multi-document safety distinguishes measured excess from missing evidence", () => {
  const reports = syntheticHardBenefitReports();
  const passing = analyzeGpuiMultiDocumentAbsoluteSafetyV5(reports);
  assert.equal(passing.resolved, true);
  assert.equal(passing.passed, true);

  const excessive = structuredClone(reports);
  excessive.find(
    (report) =>
      report.component === "multi-document-session" &&
      report.implementation === "gpui",
  ).assessment.measurements.cgroup_peak_memory_bytes = 99_000_000_000;
  const measuredNo = analyzeGpuiMultiDocumentAbsoluteSafetyV5(excessive);
  assert.equal(measuredNo.resolved, true);
  assert.equal(measuredNo.passed, false);
  assert.match(measuredNo.metric_failures[0], /exceeds/);

  const incomplete = structuredClone(reports);
  delete incomplete.find(
    (report) =>
      report.component === "multi-document-session" &&
      report.implementation === "gpui",
  ).assessment.measurements.cpu_seconds;
  const blocked = analyzeGpuiMultiDocumentAbsoluteSafetyV5(incomplete);
  assert.equal(blocked.resolved, false);
  assert.match(blocked.blocking_failures[0], /missing or nonfinite/);

  const correctnessOnly = structuredClone(reports);
  correctnessOnly.find(
    (report) =>
      report.component === "multi-document-session" &&
      report.implementation === "gpui",
  ).assessment.benefit_metrics_eligible = false;
  const rejected = analyzeGpuiMultiDocumentAbsoluteSafetyV5(correctnessOnly);
  assert.equal(rejected.resolved, false);
  assert.match(rejected.blocking_failures[0], /benefit evidence is ineligible/);
});

test("analyzePairedV5 fails closed on a tampered analyzer input", async () => {
  const directory = await mkdtemp(resolve(tmpdir(), "bp-analyze-v5-"));
  try {
    const inputPath = resolve(directory, "analyzer-input-v5.json");
    await writeFile(
      inputPath,
      `${JSON.stringify({
        schema_version: 1,
        protocol_version: "bp-perf-v5",
        decision_contract_version: "bp-perf-v5-decision-1",
        workload_artifact_sha256: "0".repeat(64),
        workload_byte_sha256: "0".repeat(64),
        candidate_manifest_sha256: {
          electron: "e".repeat(64),
          gpui: "g".repeat(64),
        },
        reference_crop_sha256_by_id: {},
        property_correctness: [],
        bundles: [],
        hard_component_reports: [],
        dynamic_fidelity_pairs: [],
      })}\n`,
    );
    const result = await analyzePairedV5(inputPath);
    assert.equal(result.status, "blocked");
    assert.equal(result.decision, null);
    assert.equal(result.worth_funding_migration, null);
    assert.deepEqual(result.checks.failed, []);
    assert.ok(
      result.checks.blocked.some((item) =>
        item.includes("workload artifact SHA-256"),
      ),
    );
    assert.match(renderDecisionMarkdownV5(result), /Decision: \*\*BLOCKED\*\*/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("final hard references reject calibration substitution and ambiguous bundle matches", () => {
  const digest = "a".repeat(64);
  const hardPath = "/run/final-hard.json";
  const retained = {
    entry: {
      phase: "calibration",
      inference_eligible: false,
      journey: "nasa-long-document",
      pair: 1,
      pair_position: "first",
      implementation: "electron",
    },
    componentEntry: {
      component: "viewer-dynamic-fidelity",
      component_index: 6,
      input_lane: "native-x11-xtest",
      hard_component: true,
      hard_report_path: hardPath,
      hard_report_sha256: digest,
    },
    assessment: {
      measurements: {},
      quality_measurements: {},
      benefit_metrics_eligible: true,
      correctness_passed: true,
      known_baseline_defect_id: null,
    },
  };
  const reference = {
    phase: "final",
    inference_eligible: true,
    journey: "nasa-long-document",
    pair: 1,
    pair_position: "first",
    implementation: "electron",
    component: "viewer-dynamic-fidelity",
    component_index: 6,
    input_lane: "native-x11-xtest",
    path: hardPath,
    sha256: digest,
    measurements: {},
    quality_measurements: {},
    benefit_metrics_eligible: true,
    correctness_passed: true,
    known_baseline_defect_id: null,
  };
  const substituted = validateFinalHardReferencesV5([reference], [retained]);
  assert.equal(substituted.passed, false);
  assert.match(substituted.failures.join("\n"), /final inference bundle/);

  retained.entry.phase = "final";
  retained.entry.inference_eligible = true;
  const ambiguous = validateFinalHardReferencesV5(
    [reference],
    [retained, structuredClone(retained)],
  );
  assert.equal(ambiguous.passed, false);
  assert.match(ambiguous.failures.join("\n"), /exactly one retained bundle/);
});

test("strict GPU evidence rejects placeholder samples and cross-host mixing", () => {
  const valid = strictGpuReport();
  const accepted = validateGpuAndHostEvidenceV5(valid);
  assert.equal(accepted.passed, true);

  const placeholder = structuredClone(valid);
  placeholder.iterations[0].gpu.baseline.samples[0] = {};
  const rejected = validateGpuAndHostEvidenceV5(placeholder);
  assert.equal(rejected.passed, false);
  assert.match(rejected.failures.join("\n"), /finite exact NVIDIA sample/);

  const otherHost = validateGpuAndHostEvidenceV5(strictGpuReport("gpu-host-2"));
  const mixed = validateFrozenHostGpuIdentityV5([accepted, otherHost]);
  assert.equal(mixed.passed, false);
  assert.match(mixed.failures.join("\n"), /one frozen host\/GPU identity/);
});

test("strict GPU evidence proves the candidate is bound to the frozen NVIDIA adapter", () => {
  assert.equal(
    validateGpuAndHostEvidenceV5(strictGpuReport("gpu-host-1", "electron"))
      .passed,
    true,
  );

  const softwareGpui = strictGpuReport();
  softwareGpui.iterations[0].events[0].is_software_emulated = true;
  const rejectedGpui = validateGpuAndHostEvidenceV5(softwareGpui);
  assert.equal(rejectedGpui.passed, false);
  assert.match(rejectedGpui.failures.join("\n"), /selected adapter/);

  const softwareElectron = strictGpuReport("gpu-host-1", "electron");
  const browserGpu =
    softwareElectron.iterations[0].renderer.browser_gpu_info.gpu;
  browserGpu.devices[0].deviceString = "Google SwiftShader";
  browserGpu.auxAttributes.glRenderer = "Google SwiftShader";
  const rejectedElectron = validateGpuAndHostEvidenceV5(softwareElectron);
  assert.equal(rejectedElectron.passed, false);
  assert.match(rejectedElectron.failures.join("\n"), /Chromium adapter/);

  const missingReceipt = strictGpuReport();
  missingReceipt.iterations[0].events = [];
  const rejectedMissing = validateGpuAndHostEvidenceV5(missingReceipt);
  assert.equal(rejectedMissing.passed, false);
  assert.match(rejectedMissing.failures.join("\n"), /exactly one selected/);
});

test("dynamic crop evidence rehashes and recomputes retained PNG artifacts", async () => {
  const directory = await mkdtemp(resolve(tmpdir(), "bp-v5-crops-"));
  const runRoot = resolve(directory, "run");
  const referenceRoot = resolve(directory, "reference");
  await Promise.all([
    mkdir(runRoot, { recursive: true }),
    mkdir(referenceRoot, { recursive: true }),
  ]);
  try {
    const screenshotPath = resolve(runRoot, "screenshot.png");
    const sourcePath = resolve(referenceRoot, "crop-1.png");
    const candidatePath = resolve(runRoot, "candidate.png");
    const registeredPath = resolve(runRoot, "registered.png");
    const pixels = Buffer.alloc(16 * 16 * 3, 255);
    for (let y = 0; y < 16; y += 1) {
      const offset = (y * 16 + 8) * 3;
      pixels.fill(0, offset, offset + 3);
    }
    await Promise.all([
      sharp(pixels, { raw: { width: 16, height: 16, channels: 3 } })
        .png()
        .toFile(screenshotPath),
      sharp(pixels, { raw: { width: 16, height: 16, channels: 3 } })
        .png()
        .toFile(sourcePath),
    ]);
    const comparison = await registerAndComparePresentedCropV2({
      screenshotPath,
      pageBoundsPx: { x: 0, y: 0, width: 16, height: 16 },
      pageSizePt: { width: 16, height: 16 },
      pdfRect: { x: 0, y: 0, width: 16, height: 16 },
      referencePath: sourcePath,
      outputCandidatePath: candidatePath,
      outputRegisteredReferencePath: registeredPath,
    });
    const source = await artifact(sourcePath);
    const declaration = {
      crop_id: "crop-1",
      launch_id: "dynamic-test-launch",
      capture_id: "dynamic-test-launch:crop-1",
      capture_started_monotonic_ms: 10,
      capture_ended_monotonic_ms: 11,
      screenshot: await artifact(screenshotPath),
      candidate_crop: await artifact(candidatePath),
      registered_reference_crop: await artifact(registeredPath),
      source_reference_crop: source,
    };
    const crop = {
      crop_id: "crop-1",
      launch_id: "dynamic-test-launch",
      capture_id: "dynamic-test-launch:crop-1",
      screenshot_path: screenshotPath,
      candidate_crop_path: candidatePath,
      registered_reference_path: registeredPath,
      presentation: {
        page_size_points: { width: 16, height: 16 },
        painted_page_bounds_device_px: {
          x: 0,
          y: 0,
          width: 16,
          height: 16,
        },
      },
      stability: {
        capture_monotonic_interval: { start_ms: 10, end_ms: 11 },
      },
      ...comparison,
    };
    const options = {
      workload: {
        journeys: [
          {
            commands: [
              {
                id: "viewer:dynamic-fidelity-scroll",
                registered_crops: [
                  {
                    crop_id: "crop-1",
                    pdf_rect: { x: 0, y: 0, width: 16, height: 16 },
                  },
                ],
              },
            ],
          },
        ],
      },
      artifactTree: {
        run_root: runRoot,
        reference_root: referenceRoot,
        references: { "crop-1": source },
      },
      hardReports: [
        {
          pair: 1,
          implementation: "gpui",
          component: "viewer-dynamic-fidelity",
          dynamic_crop_artifacts: [declaration],
          report: {
            summary: {
              viewer_dynamic_fidelity: { registered_crops: [crop] },
            },
          },
        },
      ],
    };
    const valid = await validateDynamicCropArtifactsV5(options);
    assert.equal(valid.passed, true, valid.failures.join("\n"));

    await sharp({
      create: {
        width: 16,
        height: 16,
        channels: 3,
        background: "black",
      },
    })
      .png()
      .toFile(candidatePath);
    const tampered = await validateDynamicCropArtifactsV5(options);
    assert.equal(tampered.passed, false);
    assert.match(tampered.failures.join("\n"), /PNG SHA-256 mismatch/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("pair order must match the reviewed seed, not only remain balanced", () => {
  const seed = 0x4250_5635;
  const journeys = [
    "small-shell-open",
    "nasa-long-document",
    "engineering-sheet",
    "dense-mixed-editing",
    "persistence",
    "multi-document-session",
  ];
  const entries = [];
  for (const [phase, orders] of [
    ["calibration", createCalibrationPairOrdersV5(seed ^ 0x4341_4c35)],
    ["final", createBalancedPairOrders({ pairCount: 24, seed })],
  ]) {
    for (const journey of journeys) {
      for (const [index, order] of orders.entries()) {
        for (const [position, implementation] of order.entries()) {
          entries.push({
            phase,
            inference_eligible: phase === "final",
            journey,
            pair: index + 1,
            pair_position: position === 0 ? "first" : "second",
            implementation,
          });
        }
      }
    }
  }
  assert.equal(validatePairOrdersV5(entries, seed).passed, true);
  for (const entry of entries.filter(
    ({ phase, pair }) => phase === "final" && pair === 1,
  )) {
    entry.pair_position = entry.pair_position === "first" ? "second" : "first";
  }
  const wrongSeedOrder = validatePairOrdersV5(entries, seed);
  assert.equal(wrongSeedOrder.passed, false);
  assert.match(wrongSeedOrder.failures.join("\n"), /reviewed seed/);
});

test("analyzer root of trust rejects a substituted run-manifest checksum", async () => {
  const directory = await mkdtemp(resolve(tmpdir(), "bp-v5-root-"));
  try {
    const candidates = {
      electron: {
        path: "/candidate/electron.json",
        sha256: "e".repeat(64),
        executable: { path: "electron", sha256: "1".repeat(64) },
      },
      gpui: {
        path: "/candidate/gpui.json",
        sha256: "f".repeat(64),
        executable: { path: "gpui", sha256: "2".repeat(64) },
      },
    };
    const artifactTree = {
      run_root: directory,
      reference_root: directory,
      references: {},
    };
    const manifest = {
      complete: true,
      outcome: "passed",
      plan: {
        protocol_version: "bp-perf-v5",
        decision_contract_version: "bp-perf-v5-decision-1",
        workload_artifact_sha256: "a".repeat(64),
        workload_byte_sha256: "b".repeat(64),
      },
      settings: { schedule_seed: 7 },
      bundles: [],
      correctness_reports: [],
      candidates,
      references: {},
      artifact_tree: artifactTree,
    };
    const manifestPath = resolve(directory, "run-manifest-v5.json");
    await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
    const manifestArtifact = await artifact(manifestPath);
    const checksumPath = resolve(directory, "run-manifest-v5.sha256");
    await writeFile(
      checksumPath,
      `${manifestArtifact.sha256}  run-manifest-v5.json\n`,
    );
    const input = {
      workload_artifact_sha256: "a".repeat(64),
      workload_byte_sha256: "b".repeat(64),
      schedule_seed: 7,
      bundles: [],
      property_correctness: [],
      candidate_artifacts: candidates,
      candidate_manifest_sha256: {
        electron: candidates.electron.sha256,
        gpui: candidates.gpui.sha256,
      },
      reference_crop_sha256_by_id: {},
      artifact_tree: artifactTree,
      run_manifest: {
        manifest: manifestArtifact,
        checksum: await artifact(checksumPath),
      },
    };
    const candidateValidator = async () => structuredClone(candidates);
    const valid = await validateAnalyzerRootOfTrustV5({
      input,
      baseDirectory: directory,
      candidateValidator,
    });
    assert.equal(valid.passed, true, valid.failures.join("\n"));

    await writeFile(checksumPath, `${"0".repeat(64)}  run-manifest-v5.json\n`);
    input.run_manifest.checksum = await artifact(checksumPath);
    const substituted = await validateAnalyzerRootOfTrustV5({
      input,
      baseDirectory: directory,
      candidateValidator,
    });
    assert.equal(substituted.passed, false);
    assert.match(substituted.failures.join("\n"), /does not authenticate/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("publishes exact schemas for runner input and final decision output", async () => {
  const [inputSchema, decisionSchema] = await Promise.all([
    readFile(
      new URL("./analyzer-input-v5.schema.json", import.meta.url),
      "utf8",
    ),
    readFile(
      new URL("./final-decision-v5.schema.json", import.meta.url),
      "utf8",
    ),
  ]).then((sources) => sources.map(JSON.parse));

  assert.equal(inputSchema.properties.protocol_version.const, "bp-perf-v5");
  assert.equal(inputSchema.additionalProperties, false);
  assert.equal(inputSchema.properties.property_correctness.minItems, 2);
  assert.equal(inputSchema.properties.property_correctness.maxItems, 2);
  assert.equal(inputSchema.properties.bundles.minItems, 360);
  assert.equal(inputSchema.properties.bundles.maxItems, 360);
  assert.equal(inputSchema.properties.view_state_pairs.minItems, 180);
  assert.equal(inputSchema.properties.view_state_pairs.maxItems, 180);
  assert.equal(inputSchema.properties.hard_component_reports.minItems, 144);
  assert.equal(inputSchema.properties.hard_component_reports.maxItems, 144);
  assert.equal(inputSchema.properties.dynamic_fidelity_pairs.minItems, 24);
  assert.equal(inputSchema.properties.dynamic_fidelity_pairs.maxItems, 24);
  assert(inputSchema.required.includes("run_manifest"));
  assert(inputSchema.required.includes("candidate_artifacts"));
  assert(inputSchema.required.includes("view_state_pairs"));
  assert(
    inputSchema.properties.property_correctness.items.required.includes(
      "launch_binding_v5",
    ),
  );
  assert(inputSchema.$defs.dynamicArtifactSet.required.includes("capture_id"));
  assert(
    inputSchema.$defs.dynamicArtifactSet.required.includes(
      "capture_started_monotonic_ms",
    ),
  );
  assert.equal(inputSchema.$defs.launchBinding.additionalProperties, false);

  assert.equal(decisionSchema.additionalProperties, false);
  assert(decisionSchema.required.includes("evidence"));
  assert.deepEqual(decisionSchema.properties.status.enum, [
    "blocked",
    "decision-ready",
  ]);
  assert.equal(decisionSchema.allOf[0].then.properties.decision.type, "null");
  assert.deepEqual(decisionSchema.allOf[1].then.properties.decision.enum, [
    "YES",
    "NO",
  ]);
});

function independentLaunchRecord(index) {
  const launchId = `final-nasa-long-document-pair${index + 1}-gpui-component1-open-pdf`;
  const binding = {
    schema_version: 1,
    launch_id: launchId,
    schedule_index: index,
    phase: "final",
    inference_eligible: true,
    journey: "nasa-long-document",
    pair: index + 1,
    pair_position: index % 2 === 0 ? "first" : "second",
    implementation: "gpui",
    component: "open-pdf",
    component_index: 0,
    input_lane: "native-x11-xtest",
    candidate_manifest_sha256: `${index + 1}`.repeat(64).slice(0, 64),
    fixture_sha256_by_id: { nasa: "a".repeat(64) },
    raw_report_path: `/run/raw-${index}.json`,
    started_at: new Date(index * 2_000).toISOString(),
    ended_at: new Date(index * 2_000 + 1_000).toISOString(),
    started_monotonic_ms: index * 2_000,
    ended_monotonic_ms: index * 2_000 + 1_000,
  };
  return {
    expected: { ...binding },
    binding,
    hard_binding: structuredClone(binding),
    raw_path: binding.raw_report_path,
    raw_sha256: `${index + 3}`.repeat(64).slice(0, 64),
    raw_file_identity: `1:${100 + index}`,
    hard_path: `/run/hard-${index}.json`,
    hard_sha256: `${index + 5}`.repeat(64).slice(0, 64),
    hard_file_identity: `1:${200 + index}`,
    captures: [
      {
        crop_id: "crop-1",
        launch_id: launchId,
        capture_id: `${launchId}:crop-1`,
        capture_started_monotonic_ms: index * 2_000 + 100,
        capture_ended_monotonic_ms: index * 2_000 + 101,
        screenshot: {
          path: `/run/screenshot-${index}.png`,
          sha256: `${index + 7}`.repeat(64).slice(0, 64),
        },
        candidate_crop: { path: `/run/candidate-${index}.png` },
        registered_reference_crop: { path: `/run/reference-${index}.png` },
        file_identities: [
          `1:${index * 3}`,
          `1:${index * 3 + 1}`,
          `1:${index * 3 + 2}`,
        ],
      },
    ],
  };
}

test("independent launch binding rejects hard-links, copied content, reused launch IDs, and crops", () => {
  const valid = [independentLaunchRecord(0), independentLaunchRecord(1)];
  assert.equal(validateIndependentLaunchBindingsV5(valid).passed, true);

  const hardLinked = structuredClone(valid);
  hardLinked[1].raw_sha256 = hardLinked[0].raw_sha256;
  hardLinked[1].hard_sha256 = hardLinked[0].hard_sha256;
  const duplicateContent = validateIndependentLaunchBindingsV5(hardLinked);
  assert.equal(duplicateContent.passed, false);
  assert.match(
    duplicateContent.failures.join("\n"),
    /duplicate raw report SHA-256/,
  );
  assert.match(
    duplicateContent.failures.join("\n"),
    /duplicate hard report SHA-256/,
  );

  const sameLaunch = structuredClone(valid);
  sameLaunch[1].binding.launch_id = sameLaunch[0].binding.launch_id;
  const repeatedIdentity = validateIndependentLaunchBindingsV5(sameLaunch);
  assert.equal(repeatedIdentity.passed, false);
  assert.match(repeatedIdentity.failures.join("\n"), /duplicate launch_id/);

  const reusedCrop = structuredClone(valid);
  reusedCrop[1].captures = structuredClone(reusedCrop[0].captures);
  const repeatedCapture = validateIndependentLaunchBindingsV5(reusedCrop);
  assert.equal(repeatedCapture.passed, false);
  assert.match(repeatedCapture.failures.join("\n"), /duplicate capture_id/);
  assert.match(repeatedCapture.failures.join("\n"), /capture path was reused/);
  assert.match(repeatedCapture.failures.join("\n"), /capture inode was reused/);

  const equalPixels = structuredClone(valid);
  equalPixels[1].captures[0].screenshot.sha256 =
    equalPixels[0].captures[0].screenshot.sha256;
  assert.equal(validateIndependentLaunchBindingsV5(equalPixels).passed, true);
});

test("file identity collection blocks missing, directory, and unreadable evidence without throwing", async () => {
  const directory = await mkdtemp(resolve(tmpdir(), "bp-v5-file-identity-"));
  try {
    const rawPath = resolve(directory, "raw.json");
    const unreadablePath = resolve(directory, "unreadable.png");
    const captureDirectory = resolve(directory, "capture-directory");
    await Promise.all([
      writeFile(rawPath, "{}\n"),
      writeFile(unreadablePath, "png"),
      mkdir(captureDirectory),
    ]);
    await chmod(unreadablePath, 0o000);
    const collected = await collectIndependentFileIdentitiesV5([
      {
        raw_path: rawPath,
        hard_path: resolve(directory, "missing-hard.json"),
        captures: [
          {
            capture_id: "launch:crop",
            screenshot: { path: captureDirectory },
            candidate_crop: { path: unreadablePath },
            registered_reference_crop: {
              path: resolve(directory, "missing.png"),
            },
          },
        ],
      },
    ]);
    assert.equal(collected.passed, false);
    assert.match(collected.failures.join("\n"), /missing-hard.*cannot stat/);
    assert.match(
      collected.failures.join("\n"),
      /capture-directory.*not a file/,
    );
    assert.match(collected.failures.join("\n"), /unreadable.*not readable/);
    assert.match(collected.failures.join("\n"), /missing\.png.*cannot stat/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
