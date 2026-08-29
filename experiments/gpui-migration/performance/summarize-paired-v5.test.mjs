import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  comparisonWorkloadArtifactHashV5,
  cropRegistrationHashV5,
  loadComparisonWorkloadV5,
} from "./comparison-workload-v5.mjs";
import {
  buildScenarioContractV5,
  protocolVersionV5,
  scenarioContractVersionV5,
} from "./scenario-contract-v5.mjs";
import { electronMultiDocumentMissingBenefitMetricsV5 } from "./decision-contract-v5.mjs";
import {
  analyzeDynamicFidelityPairsV5,
  buildAnalyzerScaffoldV5,
  extractDynamicFidelityMeasurementsV5,
  hardComponentEvidenceContractV5,
  validateHardComponentReportV5,
} from "./summarize-paired-v5.mjs";
import {
  crossEngineScanFidelityAlgorithmV2,
  crossEngineScanFidelityParametersV2,
} from "./scan-fidelity-v2.mjs";

const digest = "d".repeat(64);

function commonPerformanceSummary() {
  return {
    wall_duration_ms: { median: 100 },
    product_latency_ms: { p95: 10 },
    process_tree: {
      cpu_seconds: { median: 1 },
      cgroup_memory_peak_bytes: { median: 100_000 },
    },
    application_frame_intervals_ms: { p95: 16 },
    native_input_to_application_frame_ack_ms: { p95: 20 },
    native_application_frame_acknowledgement_proxy: {
      receipt_scope:
        "trusted-dom-native-event-receipt-to-next-request-animation-frame-callback-not-physical-scanout",
      physical_scanout_observed: false,
      sample_count: 1,
    },
    common_benefit_timing_boundary: {
      schema_version: 1,
      boundary_id: "x11-present-complete-after-xtest-v1",
      input_clock: "CLOCK_MONOTONIC",
      completion_clock: "CLOCK_MONOTONIC",
      completion_signal: "X11-PresentCompleteNotify",
      observer_process_independent: true,
      physical_scanout_observed: false,
      passed: true,
      decision_timing_eligible: true,
      sample_count: 60,
      input_to_present_complete_p95_ms: 14.5,
    },
    gpu_whole_device_baseline_adjusted: {
      memory_used_mib: { max: 10 },
      utilization_percent: { p95: 20 },
    },
  };
}

test("hard component keeps correctness but excludes application-clock-only benefit metrics", async () => {
  const workload = await loadComparisonWorkloadV5();
  const report = await hardReport(
    workload,
    "native-snap-transform-120hz",
    "gpui",
  );
  delete report.summary.common_benefit_timing_boundary;
  report.summary.native_application_frame_acknowledgement_proxy = {
    receipt_scope:
      "gpui-input-latency-histogram-to-platform-draw-submission-not-physical-scanout",
    physical_scanout_observed: false,
    sample_count: 60,
  };
  const assessment = validateHardComponentReportV5(workload, report);
  assert.equal(assessment.passed, true);
  assert.equal(assessment.correctness_passed, true);
  assert.equal(assessment.benefit_metrics_eligible, false);
  assert.deepEqual(assessment.measurements, {});
});

function dynamicSamples(value = 1) {
  return Array.from({ length: 1921 }, (_, sampleIndex) => ({
    sample_index: sampleIndex,
    scheduled_offset_ms: (sampleIndex * 1000) / 60,
    observed_monotonic_ms: (sampleIndex * 1000) / 60,
    visible_page_ready_fraction: value,
    visible_raster_ready_area_fraction: value,
    visible_raster_pixel_density: value,
  }));
}

async function hardReport(workload, component, implementation = "electron") {
  const evidence = hardComponentEvidenceContractV5[component];
  const scenario = buildScenarioContractV5(workload, evidence.scenario);
  const commands = evidence.command_ids.map((commandId) =>
    scenario.commands.find(({ id }) => id === commandId),
  );
  const summary =
    evidence.benefit_metrics_eligible === true
      ? commonPerformanceSummary()
      : {};

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
    const electron = implementation === "electron";
    summary.native_property_edit_undo = {
      trusted_native_input: true,
      property: "stroke_width_points",
      before: 1.5,
      committed: 4,
      after_undo: electron ? 4 : 1.5,
      effective_history_revision_delta: electron ? 2 : 1,
      application_undo_count: 1,
      canonical_state_restored: !electron,
      known_baseline_defect_id: electron
        ? "electron-numeric-property-input-blur-duplicate-history-v1"
        : null,
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
    const dynamicCommand = commands[0];
    summary.viewer_dynamic_fidelity = {
      trusted_native_input: true,
      trajectory_sample_count: 3841,
      native_phase_receipts: ["forward", "pause", "reverse"],
      registered_crops: dynamicCommand.registered_crops.map((crop, index) => {
        const candidateDimensions = {
          width: crop.pdf_rect.width,
          height: crop.pdf_rect.height,
        };
        const pageSize = { width: 612, height: 792 };
        const pageBounds = { x: 300, y: 20, ...pageSize };
        const paintedState = {
          state_sequence: index + 10,
          render_generation: index + 20,
          scroll_offset_css_px: index * 10_000,
          painted_page_bounds_device_px: pageBounds,
          raster_ready: true,
        };
        return {
          crop_id: crop.crop_id,
          registration_sha256: cropRegistrationHashV5(crop),
          screenshot_sha256: digest,
          candidate_crop_sha256: digest,
          reference_crop_sha256: crop.reference_raster.reference_crop_sha256,
          registered_reference_crop_sha256: digest,
          candidate_dimensions: candidateDimensions,
          reference_original_dimensions: {
            width: candidateDimensions.width * 2,
            height: candidateDimensions.height * 2,
          },
          registered_reference_dimensions: candidateDimensions,
          candidate_resampled: false,
          reference_resampling: "downsample-only-lanczos3",
          metric: {
            algorithm: crossEngineScanFidelityAlgorithmV2,
            parameters: { ...crossEngineScanFidelityParametersV2 },
            dimensions: candidateDimensions,
            phase_offset_px: { dx: 0, dy: 0 },
            filtered_ssim_luma: 0.99,
            dark_content: { precision: 1, recall: 1, f1: 1 },
            passed: true,
          },
          presentation: {
            zoom_mode: "fixed-percent",
            zoom_percent: 100,
            client_device_scale: 1,
            page_size_points: pageSize,
            painted_page_bounds_device_px: pageBounds,
            x_device_pixels_per_pdf_point: 1,
            y_device_pixels_per_pdf_point: 1,
            presentation_scale_comparable: true,
          },
          stability: {
            hold_ms: 250,
            zero_input_interval_count: 30,
            zero_input_sample_count: 31,
            before: paintedState,
            capture_monotonic_interval: {
              start_ms: 1000 + index * 10_000,
              end_ms: 1001 + index * 10_000,
            },
            after: { ...paintedState },
            stable: true,
          },
        };
      }),
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
    candidate_artifact_sha256: digest,
    workload_artifact_sha256: comparisonWorkloadArtifactHashV5(workload),
    fixture_ids: [...evidence.fixture_ids],
    fixture_sha256_by_id: Object.fromEntries(
      evidence.fixture_ids.map((fixtureId) => [
        fixtureId,
        scenario.fixture_sha256_by_id[fixtureId],
      ]),
    ),
    launch_binding_v5: {
      schema_version: 1,
      launch_id: `test-${implementation}-${component}`,
      schedule_index: 0,
      phase: "final",
      inference_eligible: true,
      journey: evidence.scenario,
      pair: 1,
      pair_position: "first",
      implementation,
      component,
      component_index: 0,
      input_lane: "native-x11-xtest",
      candidate_manifest_sha256: digest,
      fixture_sha256_by_id: Object.fromEntries(
        evidence.fixture_ids.map((fixtureId) => [
          fixtureId,
          scenario.fixture_sha256_by_id[fixtureId],
        ]),
      ),
      raw_report_path: `/run/test-${implementation}-${component}.json`,
      started_at: "2026-08-23T00:00:00.000Z",
      ended_at: "2026-08-23T00:00:01.000Z",
      started_monotonic_ms: 1,
      ended_monotonic_ms: 2,
    },
    command_receipts: commands.map((command) => ({
      command_id: command.id,
      live: true,
      passed: true,
      evidence_sha256: digest,
      proven_milestones: [...command.expected_milestones],
    })),
    summary,
  };
}

test("publishes an execution-ready v5 runner and analyzer scaffold", async () => {
  const workload = await loadComparisonWorkloadV5();
  const scaffold = buildAnalyzerScaffoldV5(workload);
  assert.equal(scaffold.protocol_version, protocolVersionV5);
  assert.equal(
    scaffold.runner_support,
    "implemented-v5-runner-and-final-analyzer",
  );
  assert.equal(scaffold.final_analyzer, "./analyze-paired-v5.mjs");
  assert.equal(scaffold.execution_ready, true);
  assert.equal(scaffold.blocker, null);
  assert.deepEqual(scaffold.dynamic_fidelity_quality_family.metrics, [
    "visible_page_ready_fraction",
    "visible_raster_ready_area_fraction",
    "visible_raster_pixel_density",
  ]);
});

test("validates all four exact hard-component report contracts", async () => {
  const workload = await loadComparisonWorkloadV5();
  for (const component of Object.keys(hardComponentEvidenceContractV5)) {
    const report = await hardReport(workload, component);
    const result = validateHardComponentReportV5(workload, report);
    assert.deepEqual(
      result.failures,
      [],
      `${component}: ${result.failures.join("; ")}`,
    );
    assert.equal(result.passed, true);
  }
});

test("retains the Electron property defect while requiring exact GPUI history and native presentation", async () => {
  const workload = await loadComparisonWorkloadV5();
  const report = await hardReport(workload, "native-property-edit-undo");
  assert.deepEqual(report.summary.process_tree, undefined);
  const passed = validateHardComponentReportV5(workload, report);
  assert.equal(passed.passed, true);
  assert.equal(passed.correctness_passed, false);
  assert.equal(
    passed.known_baseline_defect_id,
    "electron-numeric-property-input-blur-duplicate-history-v1",
  );
  assert.deepEqual(passed.measurements, {});

  const gpuiReport = await hardReport(
    workload,
    "native-property-edit-undo",
    "gpui",
  );
  const gpuiPassed = validateHardComponentReportV5(workload, gpuiReport);
  assert.equal(gpuiPassed.passed, true);
  assert.equal(gpuiPassed.correctness_passed, true);
  assert.equal(gpuiPassed.known_baseline_defect_id, null);

  report.summary.native_property_edit_undo.native_presentation_acknowledged = false;
  const failed = validateHardComponentReportV5(workload, report);
  assert.equal(failed.passed, false);
  assert(
    failed.failures.includes(
      "native property edit and undo evidence is not exact",
    ),
  );
});

test("retains only the exact Electron second-NASA defect without benefit metrics", async () => {
  const workload = await loadComparisonWorkloadV5();
  const report = await hardReport(workload, "multi-document-session");
  report.summary = {
    multi_document_session: {
      known_baseline_defect_id:
        "electron-multi-document-second-nasa-visible-pages-empty-v1",
      activated_fixture_id: "nasa-apollo-summary-526-v1",
      activation_ordinal: 2,
      visible_page_indices: [],
      queued_raster_count: 0,
      inflight_raster_count: 0,
      visible_raster_presented: false,
      error_presented: false,
      benchmark_metrics_eligible: false,
      benchmark_metrics_missing: [
        ...electronMultiDocumentMissingBenefitMetricsV5,
      ],
    },
  };
  report.command_receipts = report.command_receipts.map((receipt, index) => ({
    ...receipt,
    live: index === 0,
    passed: false,
    proven_milestones: index === 0 ? receipt.proven_milestones.slice(0, 1) : [],
  }));
  const retained = validateHardComponentReportV5(workload, report);
  assert.equal(retained.passed, true);
  assert.equal(retained.correctness_passed, false);
  assert.equal(retained.benefit_metrics_eligible, false);
  assert.deepEqual(retained.measurements, {});
  assert.equal(
    retained.known_baseline_defect_id,
    "electron-multi-document-second-nasa-visible-pages-empty-v1",
  );

  report.summary.multi_document_session.error_presented = true;
  assert.equal(validateHardComponentReportV5(workload, report).passed, false);

  report.implementation = "gpui";
  report.summary.multi_document_session.error_presented = false;
  assert.equal(validateHardComponentReportV5(workload, report).passed, false);
});

test("extracts complete 32-second dynamic fidelity evidence and fails closed on a missing sample", async () => {
  const workload = await loadComparisonWorkloadV5();
  const report = await hardReport(workload, "viewer-dynamic-fidelity");
  const summary = report.summary.viewer_dynamic_fidelity;
  const extracted = extractDynamicFidelityMeasurementsV5(summary);
  assert.deepEqual(extracted.failures, []);
  assert.deepEqual(extracted.measurements, {
    visible_page_ready_fraction: 1,
    visible_raster_ready_area_fraction: 1,
    visible_raster_pixel_density: 1,
  });

  summary.samples.pop();
  const failed = validateHardComponentReportV5(workload, report);
  assert.equal(failed.passed, false);
  assert(
    failed.failures.includes(
      "dynamic fidelity must retain exactly 1921 observer samples",
    ),
  );

  const timestampReport = await hardReport(workload, "viewer-dynamic-fidelity");
  timestampReport.summary.viewer_dynamic_fidelity.samples[10].observed_monotonic_ms =
    timestampReport.summary.viewer_dynamic_fidelity.samples[9].observed_monotonic_ms;
  const timestampFailure = validateHardComponentReportV5(
    workload,
    timestampReport,
  );
  assert.equal(timestampFailure.passed, false);
  assert(
    timestampFailure.failures.some((failure) =>
      failure.includes("invalid actual observation timestamp"),
    ),
  );
});

test("enforces every v2 crop gate, stable hold, and presentation scale", async () => {
  const workload = await loadComparisonWorkloadV5();
  const report = await hardReport(workload, "viewer-dynamic-fidelity");
  report.summary.viewer_dynamic_fidelity.registered_crops[1].metric.dark_content.recall = 0.98;
  const result = validateHardComponentReportV5(workload, report);
  assert.equal(result.passed, false);
  assert(
    result.failures.some((failure) =>
      failure.includes("nasa-scroll-middle-page-15-body"),
    ),
  );

  const algorithmReport = await hardReport(workload, "viewer-dynamic-fidelity");
  algorithmReport.summary.viewer_dynamic_fidelity.registered_crops[0].metric.algorithm =
    "ambiguous-ssim";
  assert.equal(
    validateHardComponentReportV5(workload, algorithmReport).passed,
    false,
  );

  const stabilityReport = await hardReport(workload, "viewer-dynamic-fidelity");
  stabilityReport.summary.viewer_dynamic_fidelity.registered_crops[2].stability.after.render_generation += 1;
  assert.equal(
    validateHardComponentReportV5(workload, stabilityReport).passed,
    false,
  );

  const scaleReport = await hardReport(workload, "viewer-dynamic-fidelity");
  scaleReport.summary.viewer_dynamic_fidelity.registered_crops[0].presentation.x_device_pixels_per_pdf_point = 1.02;
  assert.equal(
    validateHardComponentReportV5(workload, scaleReport).passed,
    false,
  );
});

test("uses higher-is-better paired quality ratios with a conjunctive lower-95 gate", () => {
  const presentationScales = ["start", "middle", "apex"].map((cropId) => ({
    crop_id: cropId,
    x_device_pixels_per_pdf_point: 1,
    y_device_pixels_per_pdf_point: 1,
  }));
  const passingPairs = Array.from({ length: 24 }, (_, index) => ({
    pair: index + 1,
    electron: {
      visible_page_ready_fraction: 0.9,
      visible_raster_ready_area_fraction: 0.9,
      visible_raster_pixel_density: 0.9,
      presentation_scales: structuredClone(presentationScales),
    },
    gpui: {
      visible_page_ready_fraction: 0.9 * 0.96,
      visible_raster_ready_area_fraction: 0.9 * 0.96,
      visible_raster_pixel_density: 0.9 * 0.96,
      presentation_scales: structuredClone(presentationScales),
    },
  }));
  const passed = analyzeDynamicFidelityPairsV5(passingPairs);
  assert.equal(passed.decision_ready, true);
  for (const metric of Object.values(passed.metrics)) {
    assert.equal(metric.ratio, "gpui/electron");
    assert.equal(metric.passed, true);
  }

  const failingPairs = structuredClone(passingPairs);
  for (const pair of failingPairs) {
    pair.gpui.visible_raster_pixel_density =
      pair.electron.visible_raster_pixel_density * 0.9;
  }
  const failed = analyzeDynamicFidelityPairsV5(failingPairs);
  assert.equal(failed.decision_ready, false);
  assert.equal(failed.executable, true);
  assert.equal(failed.metrics.visible_raster_pixel_density.passed, false);

  const mismatchedScalePairs = structuredClone(passingPairs);
  mismatchedScalePairs[0].gpui.presentation_scales[1].x_device_pixels_per_pdf_point = 1.02;
  const scaleFailure = analyzeDynamicFidelityPairsV5(mismatchedScalePairs);
  assert.equal(scaleFailure.decision_ready, false);
  assert.equal(scaleFailure.executable, false);
  assert(
    scaleFailure.failures.some((failure) =>
      failure.includes("presented scale is not comparable"),
    ),
  );
});

test("publishes analyzer scaffold and hard-report schemas", async () => {
  const [summarySchema, reportSchema] = await Promise.all([
    readFile(
      new URL("./paired-summary-v5.schema.json", import.meta.url),
      "utf8",
    ),
    readFile(
      new URL("./hard-component-report-v5.schema.json", import.meta.url),
      "utf8",
    ),
  ]).then((values) => values.map(JSON.parse));
  assert.equal(
    summarySchema.properties.protocol_version.const,
    protocolVersionV5,
  );
  assert.equal(summarySchema.properties.execution_ready.const, true);
  assert.deepEqual(reportSchema.properties.component.enum, [
    "multi-document-session",
    "native-property-edit-undo",
    "native-snap-transform-120hz",
    "viewer-dynamic-fidelity",
  ]);
  assert(reportSchema.required.includes("launch_binding_v5"));
  assert(reportSchema.$defs.registeredCropV2.required.includes("capture_id"));
  assert.equal(reportSchema.properties.command_receipts.maxItems, 4);
});
