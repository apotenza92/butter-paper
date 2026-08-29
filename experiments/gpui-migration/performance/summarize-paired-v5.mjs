import { pairedLogRatioBootstrap } from "./decision-statistics.mjs";
import {
  decisionContractV5,
  decisionContractVersionV5,
  electronMultiDocumentMissingBenefitMetricsV5,
  hardComponentIdsV5,
} from "./decision-contract-v5.mjs";
import {
  comparisonWorkloadArtifactHashV5,
  cropRegistrationHashV5,
} from "./comparison-workload-v5.mjs";
import {
  protocolVersionV5,
  representativeScenarioDefinitionsV5,
  scenarioContractVersionV5,
} from "./scenario-contract-v5.mjs";
import {
  crossEngineScanFidelityAlgorithmV2,
  crossEngineScanFidelityParametersV2,
} from "./scan-fidelity-v2.mjs";
import { extractComponentMeasurementsV4 } from "./summarize-paired-v4.mjs";

export const pairedSummaryV5SchemaVersion = 5;
export const defaultBootstrapSamplesV5 = 100_000;
export const minimumFinalPairsV5 = 24;
export const maximumFinalPairsV5 = 40;

const implementations = Object.freeze(["electron", "gpui"]);
const sha256Pattern = /^[0-9a-f]{64}$/;
const dynamicMetricNames = Object.freeze([
  "visible_page_ready_fraction",
  "visible_raster_ready_area_fraction",
  "visible_raster_pixel_density",
]);

export const commonBenefitTimingBoundaryV5 = Object.freeze({
  schema_version: 1,
  boundary_id: "x11-present-complete-after-xtest-v1",
  input_clock: "CLOCK_MONOTONIC",
  completion_clock: "CLOCK_MONOTONIC",
  completion_signal: "X11-PresentCompleteNotify",
  observer_process_independent: true,
  physical_scanout_observed: false,
});

export function commonBenefitTimingBoundaryPassedV5(receipt) {
  return (
    receipt?.schema_version === commonBenefitTimingBoundaryV5.schema_version &&
    receipt?.boundary_id === commonBenefitTimingBoundaryV5.boundary_id &&
    receipt?.input_clock === commonBenefitTimingBoundaryV5.input_clock &&
    receipt?.completion_clock ===
      commonBenefitTimingBoundaryV5.completion_clock &&
    receipt?.completion_signal ===
      commonBenefitTimingBoundaryV5.completion_signal &&
    receipt?.observer_process_independent === true &&
    receipt?.physical_scanout_observed === false &&
    receipt?.passed === true &&
    receipt?.decision_timing_eligible === true &&
    Number.isInteger(receipt?.sample_count) &&
    receipt.sample_count > 0 &&
    Number.isFinite(receipt?.input_to_present_complete_p95_ms) &&
    receipt.input_to_present_complete_p95_ms > 0
  );
}

export const hardComponentEvidenceContractV5 = Object.freeze({
  "multi-document-session": Object.freeze({
    scenario: "multi-document-session",
    command_ids: Object.freeze([
      "session:open-four-fixtures",
      "session:switch-four-fixtures",
      "session:edit-dense-rectangle",
      "session:close-three-and-recover",
    ]),
    fixture_ids: Object.freeze([
      "bp-single-page-v1",
      "nasa-apollo-summary-526-v1",
      "bp-engineering-sheet-v1",
      "bp-annotation-density-v1",
    ]),
    summary_key: "multi_document_session",
    benefit_metrics_eligible: true,
  }),
  "native-property-edit-undo": Object.freeze({
    scenario: "dense-mixed-editing",
    command_ids: Object.freeze(["annotation:native-property-edit-undo"]),
    fixture_ids: Object.freeze(["bp-annotation-density-v1"]),
    summary_key: "native_property_edit_undo",
    benefit_metrics_eligible: false,
  }),
  "native-snap-transform-120hz": Object.freeze({
    scenario: "dense-mixed-editing",
    command_ids: Object.freeze(["annotation:native-snap-transform-120hz"]),
    fixture_ids: Object.freeze(["bp-annotation-density-v1"]),
    summary_key: "native_snap_transform_120hz",
    benefit_metrics_eligible: true,
  }),
  "viewer-dynamic-fidelity": Object.freeze({
    scenario: "nasa-long-document",
    command_ids: Object.freeze(["viewer:dynamic-fidelity-scroll"]),
    fixture_ids: Object.freeze(["nasa-apollo-summary-526-v1"]),
    summary_key: "viewer_dynamic_fidelity",
    benefit_metrics_eligible: true,
  }),
});

function finitePositive(value) {
  return Number.isFinite(value) && value > 0;
}

function exact(actual, expected) {
  return JSON.stringify(actual) === JSON.stringify(expected);
}

function percentile(values, fraction) {
  const ordered = [...values].sort((left, right) => left - right);
  const position = (ordered.length - 1) * fraction;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  return lower === upper
    ? ordered[lower]
    : ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower);
}

function average(values) {
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function expectedCommands(workload, contract) {
  const journeyId =
    representativeScenarioDefinitionsV5[contract.scenario].journey_id;
  const journey = workload.journeys.find(({ id }) => id === journeyId);
  return contract.command_ids.map((commandId) =>
    journey.commands.find(({ id }) => id === commandId),
  );
}

function validateGenericReport(workload, report, contract, failures) {
  const exactElectronSessionDefect = exactElectronMultiDocumentDefect(
    report?.summary?.multi_document_session,
    report?.implementation,
  );
  if (!implementations.includes(report?.implementation)) {
    failures.push("implementation must be electron or gpui");
  }
  if (
    report?.protocol_version !== protocolVersionV5 ||
    report?.decision_contract_version !== decisionContractVersionV5 ||
    report?.scenario_contract_version !== scenarioContractVersionV5 ||
    report?.manifest_id !== workload.manifest_id
  ) {
    failures.push("report protocol or contract identity is not v5");
  }
  if (
    report?.scenario !== contract.scenario ||
    report?.component !==
      hardComponentIdsV5.find((componentId) =>
        Object.is(hardComponentEvidenceContractV5[componentId], contract),
      )
  ) {
    failures.push(
      "hard component scenario identity does not match its contract",
    );
  }
  if (!sha256Pattern.test(report?.candidate_artifact_sha256 ?? "")) {
    failures.push("candidate artifact SHA-256 is missing");
  }
  const binding = report?.launch_binding_v5;
  if (
    binding?.schema_version !== 1 ||
    typeof binding.launch_id !== "string" ||
    binding.launch_id.length === 0 ||
    !Number.isInteger(binding.schedule_index) ||
    binding.implementation !== report.implementation ||
    binding.journey !== report.scenario ||
    binding.component !== report.component ||
    binding.candidate_manifest_sha256 !== report.candidate_artifact_sha256 ||
    !Number.isFinite(binding.started_monotonic_ms) ||
    !Number.isFinite(binding.ended_monotonic_ms) ||
    binding.ended_monotonic_ms <= binding.started_monotonic_ms
  ) {
    failures.push("hard report launch binding is missing or inconsistent");
  }
  if (
    report?.workload_artifact_sha256 !==
    comparisonWorkloadArtifactHashV5(workload)
  ) {
    failures.push("workload artifact SHA-256 does not match v5");
  }
  const definition = representativeScenarioDefinitionsV5[contract.scenario];
  if (!exact(report?.fixture_ids, contract.fixture_ids)) {
    failures.push("fixture order does not match the hard component contract");
  }
  for (const fixtureId of contract.fixture_ids) {
    if (
      report?.fixture_sha256_by_id?.[fixtureId] !==
      definition.fixture_sha256_by_id[fixtureId]
    ) {
      failures.push(`${fixtureId}: fixture SHA-256 does not match v5`);
    }
  }
  if (!exact(binding?.fixture_sha256_by_id, report?.fixture_sha256_by_id)) {
    failures.push("launch binding fixture identities do not match the report");
  }
  const commands = expectedCommands(workload, contract);
  const receipts = report?.command_receipts ?? [];
  if (
    !exact(
      receipts.map(({ command_id: commandId }) => commandId),
      contract.command_ids,
    )
  ) {
    failures.push(
      "command receipt order does not match the hard component contract",
    );
  }
  for (const command of commands) {
    const receiptIndex = commands.indexOf(command);
    const receipt = receipts.find(
      ({ command_id: commandId }) => commandId === command.id,
    );
    if (exactElectronSessionDefect) {
      const expectedSubset = command.expected_milestones.filter((milestone) =>
        (receipt?.proven_milestones ?? []).includes(milestone),
      );
      const exactStoppedReceipt =
        receiptIndex === 0
          ? receipt?.live === true &&
            receipt?.passed === false &&
            Array.isArray(receipt?.proven_milestones) &&
            exact(receipt.proven_milestones, expectedSubset) &&
            receipt.proven_milestones.length <
              command.expected_milestones.length
          : receipt?.live === false &&
            receipt?.passed === false &&
            exact(receipt?.proven_milestones, []);
      if (
        !exactStoppedReceipt ||
        !sha256Pattern.test(receipt?.evidence_sha256 ?? "")
      ) {
        failures.push(
          `${command.id}: exact stopped Electron defect receipt is invalid`,
        );
      }
      continue;
    }
    if (
      receipt?.live !== true ||
      receipt?.passed !== true ||
      !sha256Pattern.test(receipt?.evidence_sha256 ?? "") ||
      !exact(receipt?.proven_milestones, command.expected_milestones)
    ) {
      failures.push(`${command.id}: exact live milestone receipt did not pass`);
    }
  }
  if (
    contract.benefit_metrics_eligible === false ||
    exactElectronMultiDocumentDefect(
      report?.summary?.multi_document_session,
      report?.implementation,
    )
  ) {
    return {};
  }
  if (
    !commonBenefitTimingBoundaryPassedV5(
      report?.summary?.common_benefit_timing_boundary,
    )
  ) {
    return {};
  }
  const measurements = extractComponentMeasurementsV4(report, {
    nativeComponent: true,
  });
  for (const missing of measurements.missing) {
    failures.push(`measurement missing: ${missing}`);
  }
  return measurements.values;
}

function validateMultiDocument(summary, failures, implementation) {
  if (exactElectronMultiDocumentDefect(summary, implementation)) return;
  const fixtures =
    hardComponentEvidenceContractV5["multi-document-session"].fixture_ids;
  if (
    !exact(summary?.opened_fixture_ids, fixtures) ||
    !exact(summary?.switch_sequence, [
      "nasa-apollo-summary-526-v1",
      "bp-single-page-v1",
      "bp-engineering-sheet-v1",
      "bp-annotation-density-v1",
    ]) ||
    !exact(summary?.close_sequence, [
      "bp-single-page-v1",
      "bp-engineering-sheet-v1",
      "nasa-apollo-summary-526-v1",
    ])
  ) {
    failures.push(
      "multi-document open, switch, or close sequence is not exact",
    );
  }
  if (
    summary?.process_restart_count !== 0 ||
    !Array.isArray(summary?.observed_process_ids) ||
    new Set(summary.observed_process_ids).size !== 1 ||
    summary.observed_process_ids[0] !== summary.stable_process_id
  ) {
    failures.push("multi-document session did not retain one stable process");
  }
  if (
    summary?.per_document_state_isolated !== true ||
    summary?.current_raster_receipt_count !== 8 ||
    summary?.dense_rectangle_property_user_gesture_count !== 1 ||
    summary?.dense_rectangle_property_history_revision_delta !==
      (implementation === "electron" ? 2 : 1) ||
    summary?.dense_rectangle_stroke_width_points !== 4 ||
    summary?.closed_document_resources_released !== true ||
    summary?.remaining_document_count !== 1 ||
    summary?.remaining_fixture_id !== "bp-annotation-density-v1" ||
    summary?.dense_document_active !== true ||
    summary?.aggregate_resource_observations_complete !== true ||
    summary?.interactive_document_shell !== true
  ) {
    failures.push("multi-document session semantic evidence is incomplete");
  }
}

function exactElectronMultiDocumentDefect(summary, implementation) {
  return (
    implementation === "electron" &&
    summary?.known_baseline_defect_id ===
      "electron-multi-document-second-nasa-visible-pages-empty-v1" &&
    summary?.activated_fixture_id === "nasa-apollo-summary-526-v1" &&
    summary?.activation_ordinal === 2 &&
    exact(summary?.visible_page_indices, []) &&
    summary?.queued_raster_count === 0 &&
    summary?.inflight_raster_count === 0 &&
    summary?.visible_raster_presented === false &&
    summary?.error_presented === false &&
    summary?.benchmark_metrics_eligible === false &&
    exact(
      summary?.benchmark_metrics_missing,
      electronMultiDocumentMissingBenefitMetricsV5,
    )
  );
}

function exactElectronPropertyDefect(summary) {
  return (
    summary?.known_baseline_defect_id ===
      "electron-numeric-property-input-blur-duplicate-history-v1" &&
    summary?.effective_history_revision_delta === 2 &&
    summary?.application_undo_count === 1 &&
    summary?.after_undo === 4 &&
    summary?.canonical_state_restored === false
  );
}

function validateProperty(summary, failures, implementation) {
  const common =
    summary?.trusted_native_input !== true ||
    summary?.property !== "stroke_width_points" ||
    summary?.before !== 1.5 ||
    summary?.committed !== 4 ||
    summary?.native_presentation_acknowledged !== true ||
    summary?.thumbnail_current !== true;
  const gpuiExact =
    summary?.after_undo === 1.5 &&
    summary?.effective_history_revision_delta === 1 &&
    summary?.application_undo_count === 1 &&
    summary?.canonical_state_restored === true &&
    summary?.known_baseline_defect_id == null;
  const outcomeExact =
    implementation === "electron"
      ? gpuiExact || exactElectronPropertyDefect(summary)
      : gpuiExact;
  if (common || !outcomeExact) {
    failures.push("native property edit and undo evidence is not exact");
  }
}

function validateSnap(summary, failures) {
  if (
    summary?.trusted_native_input !== true ||
    summary?.input_rate_hz !== 120 ||
    summary?.expected_sample_count !== 361 ||
    summary?.observed_sample_count !== 361 ||
    summary?.snap_enabled !== true ||
    summary?.sensitivity_css_px !== 8 ||
    !Number.isFinite(summary?.observed_pixels_per_point) ||
    summary.observed_pixels_per_point <= 0 ||
    !Number.isFinite(summary?.derived_threshold_points) ||
    Math.abs(
      summary.derived_threshold_points - 8 / summary.observed_pixels_per_point,
    ) > 1e-9 ||
    !exact(summary?.observed_raw_delta_points, { x: 97, y: 83 }) ||
    !exact(summary?.observed_snap_correction_points, { x: -7, y: 7 }) ||
    7 * summary.observed_pixels_per_point > 8 + 1e-9 ||
    summary?.snap_target_acquired_count < 1 ||
    summary?.snap_guide_presented_count < 1 ||
    !exact(summary?.observed_final_rectangle, {
      x1: 162,
      y1: 234,
      x2: 342,
      y2: 450,
    }) ||
    !Number.isFinite(summary?.maximum_geometry_deviation_points) ||
    summary.maximum_geometry_deviation_points > 0.01 ||
    summary?.gesture_commit_count !== 1 ||
    summary?.undo_redo_exact !== true ||
    summary?.thumbnail_current !== true
  ) {
    failures.push("native snap-enabled 120 Hz transform evidence is not exact");
  }
}

export function extractDynamicFidelityMeasurementsV5(summary) {
  const failures = [];
  if (
    summary?.trusted_native_input !== true ||
    summary?.trajectory_sample_count !== 3841 ||
    !exact(summary?.native_phase_receipts, ["forward", "pause", "reverse"])
  ) {
    failures.push("dynamic fidelity native input sample count is not exact");
  }
  const samples = summary?.samples;
  if (!Array.isArray(samples) || samples.length !== 1921) {
    failures.push("dynamic fidelity must retain exactly 1921 observer samples");
  }
  const values = Object.fromEntries(
    dynamicMetricNames.map((name) => [name, []]),
  );
  for (let index = 0; index < (samples?.length ?? 0); index += 1) {
    const sample = samples[index];
    const scheduled = (index * 1000) / 60;
    if (
      sample?.sample_index !== index ||
      !Number.isFinite(sample?.scheduled_offset_ms) ||
      Math.abs(sample.scheduled_offset_ms - scheduled) > 1e-6
    ) {
      failures.push(
        `dynamic fidelity sample ${index} has an invalid cadence receipt`,
      );
      continue;
    }
    if (
      !Number.isFinite(sample?.observed_monotonic_ms) ||
      sample.observed_monotonic_ms < 0 ||
      (index > 0 &&
        sample.observed_monotonic_ms <=
          samples[index - 1]?.observed_monotonic_ms)
    ) {
      failures.push(
        `dynamic fidelity sample ${index} has an invalid actual observation timestamp`,
      );
      continue;
    }
    for (const name of dynamicMetricNames) {
      const value = sample?.[name];
      const valid =
        Number.isFinite(value) &&
        value >= 0 &&
        (name === "visible_raster_pixel_density" || value <= 1);
      if (!valid) {
        failures.push(`dynamic fidelity sample ${index} has invalid ${name}`);
      } else {
        values[name].push(value);
      }
    }
  }
  const measurements =
    failures.length === 0
      ? {
          visible_page_ready_fraction: average(
            values.visible_page_ready_fraction,
          ),
          visible_raster_ready_area_fraction: average(
            values.visible_raster_ready_area_fraction,
          ),
          visible_raster_pixel_density: percentile(
            values.visible_raster_pixel_density,
            0.1,
          ),
        }
      : {};
  if (
    failures.length === 0 &&
    Object.values(measurements).some((value) => !finitePositive(value))
  ) {
    failures.push("dynamic fidelity aggregate measurements must be positive");
  }
  return { measurements, failures };
}

function validateDynamic(workload, summary, failures) {
  const dynamicCommand = workload.journeys
    .flatMap(({ commands }) => commands)
    .find(({ id }) => id === "viewer:dynamic-fidelity-scroll");
  const expectedCrops = dynamicCommand.registered_crops;
  if (
    !Array.isArray(summary?.registered_crops) ||
    summary.registered_crops.length !== 3
  ) {
    failures.push("dynamic fidelity must retain exactly three crop results");
  }
  for (let index = 0; index < expectedCrops.length; index += 1) {
    const expected = expectedCrops[index];
    const observed = summary?.registered_crops?.[index];
    const metric = observed?.metric;
    const presentation = observed?.presentation;
    const stability = observed?.stability;
    const before = stability?.before;
    const after = stability?.after;
    const expectedParameters = Object.fromEntries(
      Object.keys(crossEngineScanFidelityParametersV2).map((key) => [
        key,
        expected.reference_raster[key],
      ]),
    );
    const scaleTolerance =
      dynamicCommand.presentation.pixels_per_point_tolerance;
    const dimensionsExact =
      exact(metric?.dimensions, observed?.candidate_dimensions) &&
      exact(
        observed?.registered_reference_dimensions,
        observed?.candidate_dimensions,
      );
    const nativeCandidate =
      observed?.candidate_resampled === false &&
      observed?.reference_resampling === "downsample-only-lanczos3" &&
      observed?.reference_original_dimensions?.width >=
        observed?.candidate_dimensions?.width &&
      observed?.reference_original_dimensions?.height >=
        observed?.candidate_dimensions?.height;
    const scaleExact =
      presentation?.zoom_mode === "fixed-percent" &&
      presentation?.zoom_percent === 100 &&
      presentation?.client_device_scale === 1 &&
      Number.isFinite(presentation?.x_device_pixels_per_pdf_point) &&
      Number.isFinite(presentation?.y_device_pixels_per_pdf_point) &&
      Math.abs(presentation.x_device_pixels_per_pdf_point - 1) <=
        scaleTolerance &&
      Math.abs(presentation.y_device_pixels_per_pdf_point - 1) <=
        scaleTolerance &&
      Math.abs(
        presentation.x_device_pixels_per_pdf_point -
          presentation.y_device_pixels_per_pdf_point,
      ) <= scaleTolerance &&
      presentation?.presentation_scale_comparable === true;
    const paintedBoundsExact =
      exact(
        before?.painted_page_bounds_device_px,
        presentation?.painted_page_bounds_device_px,
      ) &&
      exact(
        after?.painted_page_bounds_device_px,
        presentation?.painted_page_bounds_device_px,
      );
    const stableExact =
      stability?.hold_ms === expected.checkpoint.hold_ms &&
      stability?.zero_input_interval_count ===
        expected.checkpoint.zero_input_interval_count &&
      stability?.zero_input_sample_count ===
        expected.checkpoint.zero_input_sample_count &&
      stability?.stable === true &&
      Number.isInteger(before?.state_sequence) &&
      before.state_sequence === after?.state_sequence &&
      Number.isInteger(before?.render_generation) &&
      before.render_generation === after?.render_generation &&
      Number.isFinite(before?.scroll_offset_css_px) &&
      before.scroll_offset_css_px === after?.scroll_offset_css_px &&
      before?.raster_ready === true &&
      after?.raster_ready === true &&
      paintedBoundsExact &&
      Number.isFinite(stability?.capture_monotonic_interval?.start_ms) &&
      Number.isFinite(stability?.capture_monotonic_interval?.end_ms) &&
      stability.capture_monotonic_interval.end_ms >=
        stability.capture_monotonic_interval.start_ms;
    if (
      observed?.crop_id !== expected.crop_id ||
      observed?.registration_sha256 !== cropRegistrationHashV5(expected) ||
      !sha256Pattern.test(observed?.screenshot_sha256 ?? "") ||
      !sha256Pattern.test(observed?.candidate_crop_sha256 ?? "") ||
      !sha256Pattern.test(observed?.registered_reference_crop_sha256 ?? "") ||
      observed?.reference_crop_sha256 !==
        expected.reference_raster.reference_crop_sha256 ||
      metric?.algorithm !== crossEngineScanFidelityAlgorithmV2 ||
      !exact(metric?.parameters, expectedParameters) ||
      !dimensionsExact ||
      !nativeCandidate ||
      !Number.isInteger(metric?.phase_offset_px?.dx) ||
      Math.abs(metric.phase_offset_px.dx) >
        expected.reference_raster.maximum_phase_offset_px ||
      !Number.isInteger(metric?.phase_offset_px?.dy) ||
      Math.abs(metric.phase_offset_px.dy) >
        expected.reference_raster.maximum_phase_offset_px ||
      !Number.isFinite(metric?.filtered_ssim_luma) ||
      metric.filtered_ssim_luma <
        expected.reference_raster.minimum_filtered_ssim ||
      !Number.isFinite(metric?.dark_content?.precision) ||
      metric.dark_content.precision <
        expected.reference_raster.minimum_dark_precision ||
      !Number.isFinite(metric?.dark_content?.recall) ||
      metric.dark_content.recall <
        expected.reference_raster.minimum_dark_recall ||
      !Number.isFinite(metric?.dark_content?.f1) ||
      metric.dark_content.f1 < expected.reference_raster.minimum_dark_f1 ||
      metric?.passed !== true ||
      !scaleExact ||
      !stableExact
    ) {
      failures.push(
        `${expected.crop_id}: registered crop evidence did not pass`,
      );
    }
  }
  const extracted = extractDynamicFidelityMeasurementsV5(summary);
  failures.push(...extracted.failures);
  return {
    ...extracted.measurements,
    presentation_scales: (summary?.registered_crops ?? []).map((crop) => ({
      crop_id: crop.crop_id,
      x_device_pixels_per_pdf_point:
        crop.presentation?.x_device_pixels_per_pdf_point,
      y_device_pixels_per_pdf_point:
        crop.presentation?.y_device_pixels_per_pdf_point,
    })),
  };
}

export function validateHardComponentReportV5(workload, report) {
  const failures = [];
  const contract = hardComponentEvidenceContractV5[report?.component];
  if (!contract) {
    return {
      passed: false,
      component: report?.component ?? null,
      measurements: {},
      quality_measurements: {},
      failures: ["report does not name a v5 hard component"],
    };
  }
  const measurements = validateGenericReport(
    workload,
    report,
    contract,
    failures,
  );
  const summary = report?.summary?.[contract.summary_key];
  let qualityMeasurements = {};
  if (report.component === "multi-document-session") {
    validateMultiDocument(summary, failures, report.implementation);
  } else if (report.component === "native-property-edit-undo") {
    validateProperty(summary, failures, report.implementation);
  } else if (report.component === "native-snap-transform-120hz") {
    validateSnap(summary, failures);
  } else if (report.component === "viewer-dynamic-fidelity") {
    qualityMeasurements = validateDynamic(workload, summary, failures);
  }
  return {
    passed: failures.length === 0,
    correctness_passed:
      failures.length === 0 &&
      !exactElectronMultiDocumentDefect(summary, report.implementation) &&
      !(
        report.implementation === "electron" &&
        report.component === "native-property-edit-undo" &&
        exactElectronPropertyDefect(summary)
      ),
    known_baseline_defect_id:
      failures.length === 0 &&
      exactElectronMultiDocumentDefect(summary, report.implementation)
        ? summary.known_baseline_defect_id
        : failures.length === 0 &&
            report.implementation === "electron" &&
            report.component === "native-property-edit-undo" &&
            exactElectronPropertyDefect(summary)
          ? summary.known_baseline_defect_id
          : null,
    implementation: report?.implementation ?? null,
    component: report?.component ?? null,
    benefit_metrics_eligible:
      contract.benefit_metrics_eligible === true &&
      !exactElectronMultiDocumentDefect(summary, report.implementation) &&
      commonBenefitTimingBoundaryPassedV5(
        report?.summary?.common_benefit_timing_boundary,
      ),
    measurements,
    quality_measurements: qualityMeasurements,
    failures,
  };
}

export function analyzeDynamicFidelityPairsV5(
  pairs,
  { bootstrapSamples = defaultBootstrapSamplesV5 } = {},
) {
  if (!Number.isInteger(bootstrapSamples) || bootstrapSamples < 1) {
    throw new Error("bootstrapSamples must be a positive integer");
  }
  const structuralFailures = [];
  const metricFailures = [];
  if (
    !Array.isArray(pairs) ||
    pairs.length < minimumFinalPairsV5 ||
    pairs.length > maximumFinalPairsV5 ||
    pairs.length % 4 !== 0
  ) {
    structuralFailures.push(
      "dynamic fidelity requires 24 through 40 final pairs in blocks of four",
    );
  }
  const metrics = {};
  const threshold =
    decisionContractV5.quality_observation
      .higher_is_better_lower_95_ratio_thresholds;
  for (const name of dynamicMetricNames) {
    const ratios = [];
    for (const pair of pairs ?? []) {
      const electron = pair?.electron?.[name];
      const gpui = pair?.gpui?.[name];
      if (!finitePositive(electron) || !finitePositive(gpui)) {
        structuralFailures.push(
          `pair ${pair?.pair ?? "unknown"}: ${name} is missing or nonpositive`,
        );
      } else {
        ratios.push(gpui / electron);
      }
    }
    const complete =
      ratios.length === (pairs?.length ?? 0) && ratios.length > 0;
    const bootstrap = complete
      ? pairedLogRatioBootstrap(ratios, {
          samples: bootstrapSamples,
          seed: 0x4250_5635 + dynamicMetricNames.indexOf(name),
        })
      : null;
    metrics[name] = {
      direction: "higher-is-better",
      ratio: "gpui/electron",
      threshold_lower_95: threshold[name],
      paired_ratio: bootstrap,
      passed: bootstrap ? bootstrap.lower_95 >= threshold[name] : false,
    };
    if (bootstrap && bootstrap.lower_95 < threshold[name]) {
      metricFailures.push(
        `${name}: lower 95% GPUI/Electron ratio ${bootstrap.lower_95} is below ${threshold[name]}`,
      );
    }
  }
  for (const pair of pairs ?? []) {
    const electronScales = pair?.electron?.presentation_scales;
    const gpuiScales = pair?.gpui?.presentation_scales;
    if (
      !Array.isArray(electronScales) ||
      electronScales.length !== 3 ||
      !Array.isArray(gpuiScales) ||
      gpuiScales.length !== 3
    ) {
      structuralFailures.push(
        `pair ${pair?.pair ?? "unknown"}: three paired presentation scales are required`,
      );
      continue;
    }
    for (let index = 0; index < 3; index += 1) {
      const electron = electronScales[index];
      const gpui = gpuiScales[index];
      const sameCrop = electron?.crop_id === gpui?.crop_id;
      const values = [
        electron?.x_device_pixels_per_pdf_point,
        electron?.y_device_pixels_per_pdf_point,
        gpui?.x_device_pixels_per_pdf_point,
        gpui?.y_device_pixels_per_pdf_point,
      ];
      const eachAtOne = values.every(
        (value) => Number.isFinite(value) && Math.abs(value - 1) <= 0.01,
      );
      const pairAgrees =
        eachAtOne &&
        Math.abs(values[0] - values[2]) <= 0.01 &&
        Math.abs(values[1] - values[3]) <= 0.01;
      if (!sameCrop || !pairAgrees) {
        structuralFailures.push(
          `pair ${pair?.pair ?? "unknown"}:${electron?.crop_id ?? index}: presented scale is not comparable`,
        );
      }
    }
  }
  if (bootstrapSamples !== defaultBootstrapSamplesV5) {
    structuralFailures.push(
      `decision bootstrap requires ${defaultBootstrapSamplesV5} resamples; got ${bootstrapSamples}`,
    );
  }
  const failures = [...structuralFailures, ...metricFailures];
  return {
    schema_version: pairedSummaryV5SchemaVersion,
    protocol_version: protocolVersionV5,
    decision_contract_version: decisionContractVersionV5,
    analysis: "dynamic-fidelity-hard-quality-family",
    pair_count: pairs?.length ?? 0,
    bootstrap_samples: bootstrapSamples,
    metrics,
    executable: structuralFailures.length === 0,
    decision_ready: failures.length === 0,
    structural_failures: structuralFailures,
    metric_failures: metricFailures,
    failures,
  };
}

export function buildAnalyzerScaffoldV5(workload) {
  return {
    schema_version: pairedSummaryV5SchemaVersion,
    protocol_version: protocolVersionV5,
    decision_contract_version: decisionContractVersionV5,
    workload_artifact_sha256: comparisonWorkloadArtifactHashV5(workload),
    source_analyzer: {
      protocol_version: "bp-perf-v4",
      role: "retain v4 lower-is-better CPU, memory, native timing, product latency, and GPU families",
    },
    hard_component_contracts: structuredClone(hardComponentEvidenceContractV5),
    dynamic_fidelity_quality_family: {
      metrics: [...dynamicMetricNames],
      direction: "higher-is-better",
      ratio: "gpui/electron",
      bootstrap: "paired percentile bootstrap of the geometric mean ratio",
      samples: defaultBootstrapSamplesV5,
      lower_95_thresholds: structuredClone(
        decisionContractV5.quality_observation
          .higher_is_better_lower_95_ratio_thresholds,
      ),
      registered_crop_algorithm: crossEngineScanFidelityAlgorithmV2,
      registered_crop_absolute_floors_each: {
        filtered_ssim_luma: 0.97,
        dark_precision: 0.99,
        dark_recall: 0.99,
        dark_f1: 0.99,
      },
      presentation_contract: {
        zoom_percent: 100,
        device_pixels_per_pdf_point: 1,
        tolerance: 0.01,
        candidate_resampling: "forbidden",
        reference_resampling: "downsample-only-lanczos3",
        checkpoint_hold_count: 3,
        checkpoint_hold_ms_each: 250,
        stability: "painted-before-capture-after",
      },
    },
    runner_support: "implemented-v5-runner-and-final-analyzer",
    final_analyzer: "./analyze-paired-v5.mjs",
    execution_ready: true,
    blocker: null,
  };
}
