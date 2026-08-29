import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import test from "node:test";

import sharp from "sharp";

import {
  comparisonWorkloadArtifactHashV5,
  comparisonWorkloadByteHashV5,
  cropRegistrationHashV5,
  loadMaterializedComparisonWorkloadV5,
} from "./comparison-workload-v5.mjs";
import { loadMaterializedComparisonWorkloadV4 } from "./comparison-workload-v4.mjs";
import {
  buildNvidiaBaselineResult,
  qualifyNvidiaEvidence,
  summarizeNvidiaIterations,
} from "./nvidia-sampler.mjs";
import { registerAndComparePresentedCropV2 } from "./registered-crop-v5.mjs";
import {
  optimizedCandidatePathsV4,
  prepareOptimizedCandidatesV4,
  repositoryDirectoryV4,
  validateOptimizedCandidatesV4,
} from "./optimized-candidates-v4.mjs";
import {
  assessV5Launch,
  buildV5ComparisonPlan,
  buildV5ExecutionSchedule,
} from "./run-paired-v5.mjs";
import { canonicalSha256 } from "./run-paired-v4.mjs";
import {
  buildScenarioContractV4,
  representativeScenarioDefinitionsV4,
  scenarioContractVersionV4,
} from "./scenario-contract-v4.mjs";
import {
  buildScenarioContractV5,
  protocolVersionV5,
  scenarioContractVersionV5,
} from "./scenario-contract-v5.mjs";
import {
  crossEngineScanFidelityAlgorithmV2,
  crossEngineScanFidelityParametersV2,
} from "./scan-fidelity-v2.mjs";
import { hardComponentEvidenceContractV5 } from "./summarize-paired-v5.mjs";

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function writeJson(path, value) {
  const bytes = Buffer.from(`${JSON.stringify(value)}\n`);
  await writeFile(path, bytes);
  return { path, bytes: bytes.length, sha256: sha256(bytes) };
}

async function artifact(path) {
  const bytes = await readFile(path);
  return { path, bytes: bytes.length, sha256: sha256(bytes) };
}

function strictGpuEnvelope() {
  const sample = (elapsed, utilization, memory, power) => ({
    elapsed_ms: elapsed,
    timestamp: `2026/08/23 10:00:${String(elapsed / 200).padStart(2, "0")}`,
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
  const iteration = { gpu };
  return {
    requested_iterations: 1,
    provenance: {
      host: {
        hostname: "synthetic-gpu-host",
        os_type: "Linux",
        platform: "linux",
        os_release: "6.8.0-test",
        architecture: "x64",
        display_mode:
          "Screen 0: current 1920 x 1080\nHDMI-0 connected primary 1920x1080+0+0",
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

function gpuiAdapterReceipt() {
  return {
    event: "gpu-adapter-selected",
    available: true,
    is_software_emulated: false,
    device_name: "NVIDIA RTX 4000 Ada Generation",
    driver_name: "NVIDIA",
    driver_info: "580.173.02",
  };
}

function electronBrowserGpuInfo() {
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
    },
  };
}

function performanceSummary(implementation) {
  const gpui = implementation === "gpui";
  return {
    successful_iterations: 1,
    failed_iterations: 0,
    wall_duration_ms: { median: 100 },
    product_latency_ms: { p95: gpui ? 7 : 10 },
    process_tree: {
      cpu_seconds: { median: gpui ? 0.7 : 1 },
      cgroup_memory_peak_bytes: { median: gpui ? 700_000 : 1_000_000 },
    },
    application_frame_intervals_ms: { p95: gpui ? 9 : 10 },
    native_input_to_application_frame_ack_ms: { p95: gpui ? 18 : 20 },
    native_application_frame_acknowledgement_proxy: {
      receipt_scope:
        "trusted-dom-native-event-receipt-to-next-request-animation-frame-callback-not-physical-scanout",
      physical_scanout_observed: false,
      sample_count: 1,
    },
    gpu_whole_device_baseline_adjusted: {
      memory_used_mib: { max: gpui ? 7 : 10 },
      utilization_percent: { p95: gpui ? 7 : 10 },
    },
  };
}

function dynamicSamples() {
  return Array.from({ length: 1921 }, (_, index) => ({
    sample_index: index,
    scheduled_offset_ms: (index * 1000) / 60,
    observed_monotonic_ms: (index * 1000) / 60,
    visible_page_ready_fraction: 1,
    visible_raster_ready_area_fraction: 1,
    visible_raster_pixel_density: 1,
  }));
}

function commandReceiptsV5(workload, component) {
  const evidence = hardComponentEvidenceContractV5[component];
  const scenario = buildScenarioContractV5(workload, evidence.scenario);
  return evidence.command_ids.map((commandId) => {
    const command = scenario.commands.find(({ id }) => id === commandId);
    const payload = {
      command_id: command.id,
      live: true,
      passed: true,
      proven_milestones: [...command.expected_milestones],
    };
    return { ...payload, evidence_sha256: canonicalSha256(payload) };
  });
}

function hardReportV5(workload, component, implementation, dynamicEvidence) {
  const evidence = hardComponentEvidenceContractV5[component];
  const scenario = buildScenarioContractV5(workload, evidence.scenario);
  const summary = performanceSummary(implementation);
  if (component === "multi-document-session") {
    summary.multi_document_session = {
      opened_fixture_ids: [...evidence.fixture_ids],
      switch_sequence: [
        "nasa-apollo-summary-526-v1",
        "bp-single-page-v1",
        "bp-engineering-sheet-v1",
        "bp-annotation-density-v1",
      ],
      close_sequence: [
        "bp-single-page-v1",
        "bp-engineering-sheet-v1",
        "nasa-apollo-summary-526-v1",
      ],
      stable_process_id: 4401,
      observed_process_ids: [4401, 4401, 4401, 4401],
      process_restart_count: 0,
      per_document_state_isolated: true,
      current_raster_receipt_count: 8,
      dense_rectangle_property_user_gesture_count: 1,
      dense_rectangle_property_history_revision_delta:
        implementation === "electron" ? 2 : 1,
      dense_rectangle_stroke_width_points: 4,
      closed_document_resources_released: true,
      remaining_document_count: 1,
      remaining_fixture_id: "bp-annotation-density-v1",
      dense_document_active: true,
      aggregate_resource_observations_complete: true,
      interactive_document_shell: true,
    };
  } else if (component === "native-property-edit-undo") {
    summary.native_property_edit_undo = {
      trusted_native_input: true,
      property: "stroke_width_points",
      before: 1.5,
      committed: 4,
      after_undo: 1.5,
      effective_history_revision_delta: 1,
      application_undo_count: 1,
      canonical_state_restored: true,
      known_baseline_defect_id: null,
      native_presentation_acknowledged: true,
      thumbnail_current: true,
    };
  } else if (component === "native-snap-transform-120hz") {
    summary.native_snap_transform_120hz = {
      trusted_native_input: true,
      input_rate_hz: 120,
      expected_sample_count: 361,
      observed_sample_count: 361,
      snap_enabled: true,
      sensitivity_css_px: 8,
      observed_pixels_per_point: 0.76,
      derived_threshold_points: 8 / 0.76,
      observed_raw_delta_points: { x: 97, y: 83 },
      observed_snap_correction_points: { x: -7, y: 7 },
      snap_target_acquired_count: 1,
      snap_guide_presented_count: 4,
      observed_final_rectangle: { x1: 162, y1: 234, x2: 342, y2: 450 },
      maximum_geometry_deviation_points: 0.005,
      gesture_commit_count: 1,
      undo_redo_exact: true,
      thumbnail_current: true,
    };
  } else {
    summary.viewer_dynamic_fidelity = {
      trusted_native_input: true,
      trajectory_sample_count: 3841,
      native_phase_receipts: ["forward", "pause", "reverse"],
      registered_crops: dynamicEvidence.crops,
      samples: dynamicSamples(),
    };
  }
  return {
    implementation,
    protocol_version: protocolVersionV5,
    decision_contract_version: "bp-perf-v5-decision-1",
    scenario_contract_version: scenarioContractVersionV5,
    manifest_id: workload.manifest_id,
    scenario: evidence.scenario,
    component,
    candidate_artifact_sha256:
      implementation === "electron"
        ? dynamicEvidence.candidateHashes.electron
        : dynamicEvidence.candidateHashes.gpui,
    workload_artifact_sha256: comparisonWorkloadArtifactHashV5(workload),
    workload_byte_sha256: comparisonWorkloadByteHashV5(workload),
    fixture_ids: [...evidence.fixture_ids],
    fixture_sha256_by_id: Object.fromEntries(
      evidence.fixture_ids.map((fixtureId) => [
        fixtureId,
        scenario.fixture_sha256_by_id[fixtureId],
      ]),
    ),
    command_receipts: commandReceiptsV5(workload, component),
    summary,
  };
}

function rawHardReport(workload, report) {
  const gpu = strictGpuEnvelope();
  const gpui = report.implementation === "gpui";
  const frame = gpui ? 9 : 10;
  const acknowledgement = gpui ? 18 : 20;
  const iteration = {
    ...gpu.iterations[0],
    iteration: 1,
    success: true,
    wall_duration_ms: 100,
    cgroup: {
      cpu_seconds: gpui ? 0.7 : 1,
      memory_peak_bytes: gpui ? 700_000 : 1_000_000,
    },
    events: [
      { event: "operation-visible", duration_ms: gpui ? 7 : 10 },
      ...(gpui
        ? [
            gpuiAdapterReceipt(),
            { event: "frame", interval_ms: frame },
            {
              event: "native-application-draw-acknowledgement",
              physical_scanout_observed: false,
              gpui_platform_draw_submitted: true,
              input_latency_samples_before: 0,
              input_latency_samples_after: 1,
              input_to_application_draw_ack_p95_ns: acknowledgement * 1_000_000,
            },
          ]
        : []),
    ],
    ...(gpui
      ? {}
      : {
          renderer: {
            browser_gpu_info: electronBrowserGpuInfo(),
            frame_intervals_ms: [frame],
            native_input_to_application_frame_ack: {
              receipt_scope:
                "trusted-dom-native-event-receipt-to-next-request-animation-frame-callback-not-physical-scanout",
              physical_scanout_observed: false,
              input_event_count: 1,
              acknowledged_event_count: 1,
              samples_ms: [acknowledgement],
            },
          },
        }),
  };
  return {
    ...gpu,
    iterations: [iteration],
    implementation: report.implementation,
    comparison_workload: {
      manifest_id: workload.manifest_id,
      artifact_sha256: comparisonWorkloadArtifactHashV5(workload),
      byte_sha256: comparisonWorkloadByteHashV5(workload),
    },
    comparison_v5: {
      manifest_id: workload.manifest_id,
      workload_artifact_sha256: comparisonWorkloadArtifactHashV5(workload),
      workload_byte_sha256: comparisonWorkloadByteHashV5(workload),
      iterations: [
        {
          command_receipts: report.command_receipts,
          semantic_summary:
            report.summary[
              hardComponentEvidenceContractV5[report.component].summary_key
            ],
        },
      ],
    },
  };
}

function legacyReceipt(workload, run, implementation) {
  const contract = buildScenarioContractV4(workload, run.journey);
  return contract.component_command_ids[run.component].map((commandId) => {
    const command = contract.commands.find(({ id }) => id === commandId);
    if (implementation === "electron") {
      const payload = {
        parent_scenario: run.journey,
        component_scenario: run.component,
        command_id: command.id,
        source_command_id: command.id,
        mapping_status: "exact-semantic-map",
        component_execution_passed: true,
        proven_milestones: [...command.expected_milestones],
        missing_milestones: [],
      };
      return {
        ...payload,
        live: true,
        passed: true,
        evidence_sha256: canonicalSha256(payload),
      };
    }
    const payload = {
      parent_scenario: run.journey,
      component_scenario: run.component,
      command_id: command.id,
      live: true,
      passed: true,
      milestone_ids: [...command.expected_milestones],
    };
    return { ...payload, evidence_sha256: canonicalSha256(payload) };
  });
}

function rawLegacyReport(workload, run) {
  const gpu = strictGpuEnvelope();
  const receipts = legacyReceipt(workload, run, run.implementation);
  const context = {
    manifest_id: workload.manifest_id,
    scenario_contract_version: scenarioContractVersionV4,
    parent_scenario: run.journey,
    scenario: run.journey,
    component_scenario: run.component,
    component_receipts_passed: true,
    command_receipts_by_iteration: [{ iteration: 1, receipts }],
  };
  return {
    ...gpu,
    implementation: run.implementation,
    scenario: run.component,
    cache_class: "app-cold",
    pdf: {
      sha256: representativeScenarioDefinitionsV4[run.journey].fixture_sha256,
    },
    iterations: [
      {
        ...gpu.iterations[0],
        iteration: 1,
        success: true,
        events: [
          ...(run.implementation === "gpui" ? [gpuiAdapterReceipt()] : []),
          { event: "operation-visible", duration_ms: 10 },
          { event: "comparison-memory-recovery", released_render_bytes: 1000 },
        ],
        ...(run.implementation === "electron"
          ? { renderer: { browser_gpu_info: electronBrowserGpuInfo() } }
          : {}),
      },
    ],
    summary: { ...performanceSummary(run.implementation), ...gpu.summary },
    ...(run.implementation === "electron"
      ? { v4_parent_execution: context }
      : { comparison_v4: context }),
  };
}

async function makeDynamicEvidence(
  workload,
  runRoot,
  referenceRoot,
  candidates,
) {
  const command = workload.journeys
    .flatMap(({ commands }) => commands)
    .find(({ id }) => id === "viewer:dynamic-fidelity-scroll");
  const sourceRoot = resolve(
    import.meta.dirname,
    "fixtures/reference-crops-v5",
  );
  const references = {};
  const composites = [];
  for (const crop of command.registered_crops) {
    const sourcePath = resolve(sourceRoot, `${crop.crop_id}.png`);
    const destination = resolve(referenceRoot, `${crop.crop_id}.png`);
    await writeFile(destination, await readFile(sourcePath));
    references[crop.crop_id] = await artifact(destination);
    composites.push({
      input: await sharp(destination)
        .resize(crop.pdf_rect.width, crop.pdf_rect.height, {
          kernel: sharp.kernel.lanczos3,
        })
        .png()
        .toBuffer(),
      left: crop.pdf_rect.x,
      top: 792 - crop.pdf_rect.y - crop.pdf_rect.height,
    });
  }
  const screenshotPath = resolve(runRoot, "dynamic-page-screenshot.png");
  await sharp({
    create: {
      width: 612,
      height: 792,
      channels: 3,
      background: "white",
    },
  })
    .composite(composites)
    .png()
    .toFile(screenshotPath);
  const screenshot = await artifact(screenshotPath);
  const crops = [];
  const declarations = [];
  for (const [index, crop] of command.registered_crops.entries()) {
    const candidatePath = resolve(runRoot, `${crop.crop_id}-candidate.png`);
    const registeredPath = resolve(runRoot, `${crop.crop_id}-registered.png`);
    const comparison = await registerAndComparePresentedCropV2({
      screenshotPath,
      pageBoundsPx: { x: 0, y: 0, width: 612, height: 792 },
      pageSizePt: { width: 612, height: 792 },
      pdfRect: crop.pdf_rect,
      referencePath: references[crop.crop_id].path,
      outputCandidatePath: candidatePath,
      outputRegisteredReferencePath: registeredPath,
    });
    const state = {
      state_sequence: index + 10,
      render_generation: index + 20,
      scroll_offset_css_px: index * 10_000,
      painted_page_bounds_device_px: {
        x: 0,
        y: 0,
        width: 612,
        height: 792,
      },
      raster_ready: true,
    };
    crops.push({
      crop_id: crop.crop_id,
      registration_sha256: cropRegistrationHashV5(crop),
      screenshot_path: screenshotPath,
      candidate_crop_path: candidatePath,
      registered_reference_path: registeredPath,
      ...comparison,
      presentation: {
        zoom_mode: "fixed-percent",
        zoom_percent: 100,
        client_device_scale: 1,
        page_size_points: { width: 612, height: 792 },
        painted_page_bounds_device_px: {
          x: 0,
          y: 0,
          width: 612,
          height: 792,
        },
        x_device_pixels_per_pdf_point: 1,
        y_device_pixels_per_pdf_point: 1,
        presentation_scale_comparable: true,
      },
      stability: {
        hold_ms: 250,
        zero_input_interval_count: 30,
        zero_input_sample_count: 31,
        before: state,
        capture_monotonic_interval: {
          start_ms: 1000 + index * 10_000,
          end_ms: 1001 + index * 10_000,
        },
        after: { ...state },
        stable: true,
      },
    });
    declarations.push({
      crop_id: crop.crop_id,
      screenshot,
      candidate_crop: await artifact(candidatePath),
      registered_reference_crop: await artifact(registeredPath),
      source_reference_crop: references[crop.crop_id],
    });
  }
  return {
    references,
    crops,
    declarations,
    candidateHashes: {
      electron: candidates.electron.sha256,
      gpui: candidates.gpui.sha256,
    },
  };
}

test("v5 execution fails closed before semantic/direct-model evidence can reach benefit analysis", async () => {
  const testResultsRoot = resolve(repositoryDirectoryV4, "test-results");
  await mkdir(testResultsRoot, { recursive: true });
  const directory = await mkdtemp(
    resolve(testResultsRoot, "bp-v5-full-positive-"),
  );
  const runRoot = resolve(directory, "run");
  const referenceRoot = resolve(directory, "references");
  const reportRoot = resolve(runRoot, "reports");
  await Promise.all(
    [runRoot, referenceRoot, reportRoot].map((path) =>
      mkdir(path, { recursive: true }),
    ),
  );
  try {
    const [workload, workloadV4] = await Promise.all([
      loadMaterializedComparisonWorkloadV5(),
      loadMaterializedComparisonWorkloadV4(),
    ]);
    const sourceCandidatePaths = optimizedCandidatePathsV4();
    const candidatePaths = {
      ...sourceCandidatePaths,
      electron_bundle_directory: resolve(directory, "apps/desktop/.vite"),
      desktop_dev_provenance: resolve(
        directory,
        "test-results/desktop-dev-provenance.json",
      ),
    };
    const commandRunner = async (executable, args) => {
      if (executable === "pnpm" && args.includes("@butter-paper/desktop")) {
        await Promise.all([
          mkdir(resolve(candidatePaths.electron_bundle_directory, "build"), {
            recursive: true,
          }),
          mkdir(
            resolve(
              candidatePaths.electron_bundle_directory,
              "renderer/main_window/assets",
            ),
            { recursive: true },
          ),
        ]);
        await Promise.all([
          writeFile(
            resolve(candidatePaths.electron_bundle_directory, "build/main.js"),
            "export const main = true;\n",
          ),
          writeFile(
            resolve(
              candidatePaths.electron_bundle_directory,
              "build/preload.cjs",
            ),
            "exports.preload = true;\n",
          ),
          writeFile(
            resolve(
              candidatePaths.electron_bundle_directory,
              "renderer/main_window/index.html",
            ),
            '<script type="module" src="./assets/index.js"></script>\n',
          ),
          writeFile(
            resolve(
              candidatePaths.electron_bundle_directory,
              "renderer/main_window/assets/index.js",
            ),
            "export const renderer = true;\n",
          ),
        ]);
      }
    };
    const preparedCandidates = await prepareOptimizedCandidatesV4({
      output: resolve(directory, "candidates"),
      electronExecutable: candidatePaths.electron_executable,
      devProvenancePath: sourceCandidatePaths.desktop_dev_provenance,
      runBuilds: true,
      candidatePaths,
      commandRunner,
      pdfiumFetcher: async () => candidatePaths.pdfium_library,
      testOnlyCandidateRoot: directory,
    });
    const candidates = await validateOptimizedCandidatesV4({
      electronManifestPath: preparedCandidates.electronManifestPath,
      gpuiManifestPath: preparedCandidates.gpuiManifestPath,
      electronExecutable: candidatePaths.electron_executable,
      gpuiBinary: candidatePaths.gpui_binary,
      candidatePaths,
      testOnlyCandidateRoot: directory,
    });
    const dynamic = await makeDynamicEvidence(
      workload,
      runRoot,
      referenceRoot,
      candidates,
    );
    const plan = buildV5ComparisonPlan(workload);
    const seed = 0x4250_5635;
    const schedule = buildV5ExecutionSchedule(plan, { seed });
    const fixtureHashes = Object.fromEntries(
      workload.fixtures.map((fixture) => [fixture.id, fixture.sha256]),
    );
    function launchIdentity(run) {
      return run.phase === "correctness"
        ? `correctness-${run.implementation}-${run.component}`
        : `${run.bundle_id}-component${run.component_index + 1}-${run.component}`;
    }
    function launchBinding(run, rawPath) {
      const start = 1_700_000_000_000 + run.schedule_index * 2_000;
      return {
        schema_version: 1,
        launch_id: launchIdentity(run),
        schedule_index: run.schedule_index,
        phase: run.phase,
        inference_eligible: run.inference_eligible,
        journey: run.journey,
        pair: run.pair,
        pair_position: run.pair_position,
        implementation: run.implementation,
        component: run.component,
        component_index: run.component_index,
        input_lane: run.input_lane,
        candidate_manifest_sha256: candidates[run.implementation].sha256,
        fixture_sha256_by_id: Object.fromEntries(
          run.fixture_ids.map((fixtureId) => [
            fixtureId,
            fixtureHashes[fixtureId],
          ]),
        ),
        raw_report_path: rawPath,
        started_at: new Date(start).toISOString(),
        ended_at: new Date(start + 1_000).toISOString(),
        started_monotonic_ms: run.schedule_index * 2_000,
        ended_monotonic_ms: run.schedule_index * 2_000 + 1_000,
      };
    }
    async function uniqueDynamicEvidence(run, raw, binding) {
      if (run.component !== "viewer-dynamic-fidelity") return [];
      const semantic = raw.comparison_v5.iterations[0].semantic_summary;
      const declarations = [];
      for (const [index, declaration] of dynamic.declarations.entries()) {
        const crop = semantic.registered_crops[index];
        const prefix = `${binding.launch_id}-${crop.crop_id}`;
        const paths = {
          screenshot: resolve(reportRoot, `${prefix}-screenshot.png`),
          candidate_crop: resolve(reportRoot, `${prefix}-candidate.png`),
          registered_reference_crop: resolve(
            reportRoot,
            `${prefix}-registered.png`,
          ),
        };
        await Promise.all([
          writeFile(
            paths.screenshot,
            await readFile(declaration.screenshot.path),
          ),
          writeFile(
            paths.candidate_crop,
            await readFile(declaration.candidate_crop.path),
          ),
          writeFile(
            paths.registered_reference_crop,
            await readFile(declaration.registered_reference_crop.path),
          ),
        ]);
        crop.screenshot_path = paths.screenshot;
        crop.candidate_crop_path = paths.candidate_crop;
        crop.registered_reference_path = paths.registered_reference_crop;
        crop.stability.capture_monotonic_interval = {
          start_ms: binding.started_monotonic_ms + 100 + index * 10,
          end_ms: binding.started_monotonic_ms + 101 + index * 10,
        };
        declarations.push({
          crop_id: crop.crop_id,
          launch_id: binding.launch_id,
          capture_id: `${binding.launch_id}:${crop.crop_id}`,
          capture_started_monotonic_ms:
            crop.stability.capture_monotonic_interval.start_ms,
          capture_ended_monotonic_ms:
            crop.stability.capture_monotonic_interval.end_ms,
          screenshot: await artifact(paths.screenshot),
          candidate_crop: await artifact(paths.candidate_crop),
          registered_reference_crop: await artifact(
            paths.registered_reference_crop,
          ),
          source_reference_crop: declaration.source_reference_crop,
        });
      }
      return declarations;
    }
    async function materializeRun(run) {
      const hard = run.hard_component
        ? hardReportV5(workload, run.component, run.implementation, dynamic)
        : null;
      const raw = hard
        ? rawHardReport(workload, hard)
        : rawLegacyReport(workloadV4, run);
      const identity = launchIdentity(run);
      const rawPath = resolve(reportRoot, `${identity}.raw.json`);
      const binding = launchBinding(run, rawPath);
      raw.launch_binding_v5 = binding;
      const dynamicCropArtifacts = await uniqueDynamicEvidence(
        run,
        raw,
        binding,
      );
      const assessment = assessV5Launch({
        workload,
        v4Workload: workloadV4,
        rawReport: raw,
        run,
        candidateArtifactSha256: candidates[run.implementation].sha256,
      });
      assert.equal(
        assessment.passed,
        true,
        `${identity}: ${assessment.failures.join("; ")}`,
      );
      const rawArtifact = await writeJson(rawPath, raw);
      const hardArtifact = assessment.hard_report
        ? await writeJson(
            resolve(reportRoot, `${identity}.hard.json`),
            assessment.hard_report,
          )
        : null;
      return {
        passed: true,
        raw_report_path: rawArtifact.path,
        raw_report_sha256: rawArtifact.sha256,
        hard_report_path: hardArtifact?.path ?? null,
        hard_report_sha256: hardArtifact?.sha256 ?? null,
        receipts: assessment.receipts,
        measurements: assessment.measurements,
        quality_measurements: assessment.quality_measurements,
        correctness_passed: assessment.correctness_passed,
        benefit_metrics_eligible: assessment.benefit_metrics_eligible,
        known_baseline_defect_id: assessment.known_baseline_defect_id,
        dynamic_crop_artifacts: dynamicCropArtifacts,
        launch_binding_v5: binding,
      };
    }

    const launches = [];
    const correctnessReports = [];
    for (const run of schedule.filter(({ phase }) => phase === "correctness")) {
      const result = await materializeRun(run);
      launches.push({ ...run, ...result });
      correctnessReports.push({
        implementation: run.implementation,
        component: run.component,
        raw_report_path: result.raw_report_path,
        raw_report_sha256: result.raw_report_sha256,
        hard_report_path: result.hard_report_path,
        hard_report_sha256: result.hard_report_sha256,
        passed: true,
        correctness_passed: result.correctness_passed,
        known_baseline_defect_id: result.known_baseline_defect_id,
        launch_binding_v5: result.launch_binding_v5,
      });
    }

    const timed = schedule.filter(({ phase }) => phase !== "correctness");
    const semanticRun = timed.find(
      ({ input_lane: inputLane }) => inputLane === "semantic-diagnostic",
    );
    await assert.rejects(
      () => materializeRun(semanticRun),
      /semantic\/direct-model input lanes are correctness-only/,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
