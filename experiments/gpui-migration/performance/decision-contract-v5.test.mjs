import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  assessDecisionExecutionV5,
  decisionContractArtifactHashV5,
  decisionContractV5,
  decisionContractVersionV5,
  electronMultiDocumentMissingBenefitMetricsV5,
  gpuiMultiDocumentAbsoluteSafetyBudgetsV5,
  hardComponentIdsV5,
  representativeJourneyIdsV5,
  requiredLiveEvidenceGateIdsV5,
  sourceDecisionContractArtifactSha256V5,
  validateDecisionContractV5,
} from "./decision-contract-v5.mjs";

const digest = "a".repeat(64);

function passingEvidence() {
  return {
    contract_version: decisionContractVersionV5,
    implementations: Object.fromEntries(
      ["electron", "gpui"].map((implementation) => [
        implementation,
        {
          candidate_artifact_sha256: digest,
          journeys: Object.fromEntries(
            representativeJourneyIdsV5.map((journeyId) => [
              journeyId,
              { live: true, passed: true, evidence_sha256: digest },
            ]),
          ),
          hard_components: Object.fromEntries(
            hardComponentIdsV5.map((componentId) => [
              componentId,
              { live: true, passed: true, evidence_sha256: digest },
            ]),
          ),
        },
      ]),
    ),
    live_gates: Object.fromEntries(
      requiredLiveEvidenceGateIdsV5.map((gateId) => [
        gateId,
        { live: true, passed: true, evidence_sha256: digest },
      ]),
    ),
  };
}

test("pins v4 and declares six representative journeys plus four conjunctive hard components", () => {
  assert.deepEqual(validateDecisionContractV5(decisionContractV5), []);
  assert.equal(
    decisionContractV5.source_contract.artifact_sha256,
    sourceDecisionContractArtifactSha256V5,
  );
  assert.deepEqual(
    decisionContractV5.journeys.map(({ id }) => id),
    representativeJourneyIdsV5,
  );
  assert.deepEqual(
    decisionContractV5.hard_components.map(({ id }) => id),
    hardComponentIdsV5,
  );
  for (const component of decisionContractV5.hard_components) {
    assert.equal(component.inference_eligible, true);
    assert.equal(component.non_inferiority, "conjunctive");
  }
  assert.equal(
    decisionContractV5.hard_components.find(
      ({ id }) => id === "native-property-edit-undo",
    ).benefit_metrics_eligible,
    false,
  );
  for (const component of decisionContractV5.hard_components.filter(
    ({ id }) => id !== "native-property-edit-undo",
  )) {
    assert.equal(component.benefit_metrics_eligible, true);
  }
  assert.equal(
    decisionContractArtifactHashV5(),
    "2acdab1dc3f62c1eed82f5d9af9f50c525617cac49c3c4b60fd885116563cfb1",
  );
});

test("freezes the exact multi-document fixtures and dynamic quality rules", () => {
  const session = decisionContractV5.journeys.find(
    ({ id }) => id === "multi-document-session-v1",
  );
  assert.deepEqual(session.fixtures, [
    "bp-single-page-v1",
    "nasa-apollo-summary-526-v1",
    "bp-engineering-sheet-v1",
    "bp-annotation-density-v1",
  ]);
  assert.deepEqual(decisionContractV5.quality_observation, {
    registered_crop_count: 3,
    registered_crop_algorithm: "bp-cross-engine-binary-scan-fidelity-v2",
    registered_crop_minimum_filtered_ssim_each: 0.97,
    registered_crop_minimum_dark_precision_each: 0.99,
    registered_crop_minimum_dark_recall_each: 0.99,
    registered_crop_minimum_dark_f1_each: 0.99,
    presented_zoom_percent: 100,
    presented_device_pixels_per_pdf_point: 1,
    presented_pixels_per_point_tolerance: 0.01,
    paired_candidate_scale_tolerance: 0.01,
    checkpoint_hold_count: 3,
    checkpoint_hold_ms_each: 250,
    candidate_resampling: "forbidden",
    reference_resampling: "downsample-only-lanczos3",
    presented_screenshot_stability: "painted-before-capture-after",
    dynamic_sample_cadence: "fixed-observer-60hz",
    dynamic_sample_count: 1921,
    required_sample_fields: [
      "visible_page_ready_fraction",
      "visible_raster_ready_area_fraction",
      "visible_raster_pixel_density",
    ],
    higher_is_better_lower_95_ratio_thresholds: {
      visible_page_ready_fraction: 0.95,
      visible_raster_ready_area_fraction: 0.95,
      visible_raster_pixel_density: 0.95,
    },
  });
});

test("freezes conjunctive GPUI multi-document absolute safety budgets", () => {
  assert(
    requiredLiveEvidenceGateIdsV5.includes(
      "gpui-multi-document-absolute-safety-budgets-resolved",
    ),
  );
  assert.deepEqual(
    decisionContractV5.absolute_safety_budgets.gpui_multi_document,
    gpuiMultiDocumentAbsoluteSafetyBudgetsV5,
  );
  assert.deepEqual(gpuiMultiDocumentAbsoluteSafetyBudgetsV5.metrics, {
    cpu_seconds: {
      maximum: 120,
      unit: "process-cpu-seconds",
      provenance: "new-v5-safety-cap-no-v4-absolute",
    },
    cgroup_peak_memory_bytes: {
      maximum: 1_610_612_736,
      unit: "bytes",
      provenance: "summarize-paired-v4 process_memory absoluteBudgetFor",
    },
    product_wall_or_latency_ms: {
      maximum: 60_000,
      unit: "milliseconds",
      allowed_source: ["product-latency", "component-wall"],
      provenance: "new-v5-safety-cap-no-v4-multi-document-map",
    },
    application_frame_interval_p95_ms: {
      maximum: 25,
      unit: "milliseconds",
      provenance: "v4 native frame absoluteBudgetFor",
    },
    native_input_to_application_frame_ack_p95_ms: {
      maximum: 1000 / 30,
      unit: "milliseconds",
      provenance: "v4 1000/30 native ack absoluteBudgetFor",
    },
    baseline_adjusted_gpu_peak_memory_mib: {
      maximum: 2_048,
      unit: "MiB",
      provenance: "v4 GPU memory absoluteBudgetFor",
    },
    baseline_adjusted_gpu_utilization_p95_percent: {
      maximum: 100,
      unit: "percent",
      provenance: "new-v5 physical percentage ceiling",
    },
  });
  const broken = structuredClone(decisionContractV5);
  broken.absolute_safety_budgets.gpui_multi_document.metrics.cgroup_peak_memory_bytes.maximum = 4_294_967_296;
  assert(
    validateDecisionContractV5(broken).includes(
      "GPUI multi-document absolute safety budgets are not exact",
    ),
  );
});

test("retains only the exact Electron second-NASA baseline defect and excludes its metrics", () => {
  const evidence = passingEvidence();
  const electronSession =
    evidence.implementations.electron.hard_components["multi-document-session"];
  Object.assign(electronSession, {
    passed: false,
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
  });
  assert.equal(assessDecisionExecutionV5(evidence).executable, true);
  electronSession.inflight_raster_count = 1;
  assert.equal(assessDecisionExecutionV5(evidence).executable, false);

  const gpuiSession =
    evidence.implementations.gpui.hard_components["multi-document-session"];
  Object.assign(gpuiSession, structuredClone(electronSession), {
    inflight_raster_count: 0,
  });
  assert.equal(assessDecisionExecutionV5(evidence).executable, false);
});

test("stays blocked until every v5 journey, hard component, and live gate passes", () => {
  const evidence = passingEvidence();
  assert.deepEqual(assessDecisionExecutionV5(evidence), {
    contract_version: decisionContractVersionV5,
    executable: true,
    status: "ready-final-execution",
    blockers: [],
  });

  evidence.implementations.gpui.hard_components[
    "native-snap-transform-120hz"
  ].passed = false;
  const blocked = assessDecisionExecutionV5(evidence);
  assert.equal(blocked.executable, false);
  assert(
    blocked.blockers.includes(
      "gpui:native-snap-transform-120hz: hard component evidence did not pass",
    ),
  );
});

test("retains the exact Electron property history defect without relaxing GPUI correctness", () => {
  const evidence = passingEvidence();
  const electronProperty =
    evidence.implementations.electron.hard_components[
      "native-property-edit-undo"
    ];
  Object.assign(electronProperty, {
    passed: false,
    known_baseline_defect_id:
      "electron-numeric-property-input-blur-duplicate-history-v1",
    effective_history_revision_delta: 2,
    application_undo_count: 1,
    final_stroke_width_points: 4,
  });
  assert.equal(assessDecisionExecutionV5(evidence).executable, true);

  electronProperty.effective_history_revision_delta = 3;
  assert.equal(assessDecisionExecutionV5(evidence).executable, false);

  const gpuiProperty =
    evidence.implementations.gpui.hard_components["native-property-edit-undo"];
  gpuiProperty.passed = false;
  assert.equal(assessDecisionExecutionV5(evidence).executable, false);
});

test("publishes a strict v5 decision schema", async () => {
  const schema = JSON.parse(
    await readFile(
      new URL("./decision-contract-v5.schema.json", import.meta.url),
      "utf8",
    ),
  );
  assert.equal(
    schema.properties.contract_version.const,
    decisionContractVersionV5,
  );
  assert.equal(schema.properties.journeys.minItems, 6);
  assert.equal(schema.properties.journeys.maxItems, 6);
  assert.equal(schema.properties.hard_components.minItems, 4);
  assert.equal(
    schema.properties.execution.properties.required_live_gate_ids.minItems,
    9,
  );
  assert(schema.required.includes("absolute_safety_budgets"));
  assert.equal(
    schema.$defs.gpuiMultiDocumentAbsoluteSafetyBudgets.properties.metrics.const
      .application_frame_interval_p95_ms.maximum,
    25,
  );
  assert.equal(
    schema.properties.quality_observation.properties.dynamic_sample_count.const,
    1921,
  );
});

test("hard-report schema requires GPUI multi-document safety measurement sources", async () => {
  const schema = JSON.parse(
    await readFile(
      new URL("./hard-component-report-v5.schema.json", import.meta.url),
      "utf8",
    ),
  );
  const gpuiMulti = schema.allOf.find(
    (condition) =>
      condition.if?.properties?.implementation?.const === "gpui" &&
      condition.if?.properties?.component?.const === "multi-document-session",
  );
  const summary = gpuiMulti.then.properties.summary;
  assert(summary.required.includes("process_tree"));
  assert(summary.required.includes("application_frame_intervals_ms"));
  assert(summary.required.includes("gpu_whole_device_baseline_adjusted"));
  assert.equal(summary.anyOf.length, 2);
});
