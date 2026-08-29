import { createHash } from "node:crypto";

import {
  decisionContractV4,
  representativeFixtureIdsV4,
  representativeJourneyIdsV4,
} from "./decision-contract-v4.mjs";

export const decisionContractVersionV5 = "bp-perf-v5-decision-1";
export const sourceDecisionContractArtifactSha256V5 =
  "89be6a20bc2e3b151ab94575a8d48c3d6ef83ab911dec52c8f3ab88af2c2ea52";

export const representativeFixtureIdsV5 = Object.freeze([
  ...representativeFixtureIdsV4,
]);

export const representativeJourneyIdsV5 = Object.freeze([
  ...representativeJourneyIdsV4,
  "multi-document-session-v1",
]);

export const hardComponentIdsV5 = Object.freeze([
  "multi-document-session",
  "native-property-edit-undo",
  "native-snap-transform-120hz",
  "viewer-dynamic-fidelity",
]);

export const electronMultiDocumentMissingBenefitMetricsV5 = Object.freeze([
  "cpu_seconds",
  "cgroup_peak_memory_bytes",
  "product_wall_or_latency_ms",
  "application_frame_interval_p95_ms",
  "native_input_to_application_frame_ack_p95_ms",
  "baseline_adjusted_gpu_peak_memory_mib",
  "baseline_adjusted_gpu_utilization_p95_percent",
]);

export const gpuiMultiDocumentAbsoluteSafetyBudgetsV5 = Object.freeze({
  applicability:
    "every-live-gpui-multi-document-session-report; mandatory-when-electron-benefit-metrics-ineligible",
  aggregation: "maximum-across-retained-final-gpui-multi-document-reports",
  conjunction: "all-seven",
  outcomes: Object.freeze({
    exceeded_finite_valid: "measured-no",
    missing_nonfinite_or_structurally_invalid: "blocked-not-decision-ready",
  }),
  metrics: Object.freeze({
    cpu_seconds: Object.freeze({
      maximum: 120,
      unit: "process-cpu-seconds",
      provenance: "new-v5-safety-cap-no-v4-absolute",
    }),
    cgroup_peak_memory_bytes: Object.freeze({
      maximum: 1_610_612_736,
      unit: "bytes",
      provenance: "summarize-paired-v4 process_memory absoluteBudgetFor",
    }),
    product_wall_or_latency_ms: Object.freeze({
      maximum: 60_000,
      unit: "milliseconds",
      allowed_source: Object.freeze(["product-latency", "component-wall"]),
      provenance: "new-v5-safety-cap-no-v4-multi-document-map",
    }),
    application_frame_interval_p95_ms: Object.freeze({
      maximum: 25,
      unit: "milliseconds",
      provenance: "v4 native frame absoluteBudgetFor",
    }),
    native_input_to_application_frame_ack_p95_ms: Object.freeze({
      maximum: 1000 / 30,
      unit: "milliseconds",
      provenance: "v4 1000/30 native ack absoluteBudgetFor",
    }),
    baseline_adjusted_gpu_peak_memory_mib: Object.freeze({
      maximum: 2_048,
      unit: "MiB",
      provenance: "v4 GPU memory absoluteBudgetFor",
    }),
    baseline_adjusted_gpu_utilization_p95_percent: Object.freeze({
      maximum: 100,
      unit: "percent",
      provenance: "new-v5 physical percentage ceiling",
    }),
  }),
});

export const requiredLiveEvidenceGateIdsV5 = Object.freeze([
  ...decisionContractV4.execution.required_live_gate_ids,
  "v5-hard-component-evidence-passed",
  "dynamic-fidelity-observations-complete",
  "gpui-multi-document-absolute-safety-budgets-resolved",
]);

const implementations = Object.freeze(["electron", "gpui"]);
const sha256Pattern = /^[0-9a-f]{64}$/;

function canonicalize(value) {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonicalize(value[key])]),
    );
  }
  return value;
}

export function canonicalSha256V5(value) {
  return createHash("sha256")
    .update(JSON.stringify(canonicalize(value)))
    .digest("hex");
}

function sourceV4ContractHash() {
  return canonicalSha256V5(decisionContractV4);
}

const sourceJourneys = decisionContractV4.journeys.map((journey) => ({
  ...structuredClone(journey),
  required_capabilities:
    journey.id === "nasa-long-document-v1"
      ? [
          ...journey.required_capabilities,
          "dynamic fidelity across native continuous scroll with three registered crops",
        ]
      : journey.id === "dense-mixed-editing-v1"
        ? [
            ...journey.required_capabilities,
            "native property edit and undo",
            "snap-enabled native transform replay at 120 Hz",
          ]
        : [...journey.required_capabilities],
}));

export const decisionContractV5 = Object.freeze({
  ...structuredClone(decisionContractV4),
  schema_version: 3,
  contract_version: decisionContractVersionV5,
  supersedes: decisionContractV4.contract_version,
  source_contract: {
    id: decisionContractV4.contract_version,
    module: "./decision-contract-v4.mjs",
    artifact_sha256: sourceDecisionContractArtifactSha256V5,
  },
  evidence_boundary:
    "fund-or-stop decision for completing the Butter Paper GPUI migration with multi-document, native editing, snapping, and dynamic viewer fidelity; not packaged release qualification",
  execution: {
    ...structuredClone(decisionContractV4.execution),
    required_live_gate_ids: [...requiredLiveEvidenceGateIdsV5],
    required_representative_journey_ids: [...representativeJourneyIdsV5],
    required_hard_component_ids: [...hardComponentIdsV5],
  },
  journeys: [
    ...sourceJourneys,
    {
      id: "multi-document-session-v1",
      fixtures: [
        "bp-single-page-v1",
        "nasa-apollo-summary-526-v1",
        "bp-engineering-sheet-v1",
        "bp-annotation-density-v1",
      ],
      required_capabilities: [
        "open four frozen fixtures in one application process",
        "switch documents in a frozen tab order",
        "edit a Rectangle property in the dense document",
        "preserve isolated per-document view and edit state",
        "present the current raster after every switch",
        "close three documents and release their resources while the edited dense document and application process remain active",
      ],
    },
  ],
  hard_components: [
    {
      id: "multi-document-session",
      journey_id: "multi-document-session-v1",
      input_lane: "native-replay",
      inference_eligible: true,
      non_inferiority: "conjunctive",
      benefit_metrics_eligible: true,
      gpui_candidate_correctness_required: true,
      electron_baseline_outcome:
        "pass-or-exact-electron-multi-document-second-nasa-visible-pages-empty-v1",
    },
    {
      id: "native-property-edit-undo",
      journey_id: "dense-mixed-editing-v1",
      input_lane: "native-replay",
      inference_eligible: true,
      non_inferiority: "conjunctive",
      benefit_metrics_eligible: false,
      gpui_candidate_correctness_required: true,
      electron_baseline_outcome:
        "pass-or-exact-electron-numeric-property-input-blur-duplicate-history-v1",
    },
    {
      id: "native-snap-transform-120hz",
      journey_id: "dense-mixed-editing-v1",
      input_lane: "native-replay-120hz",
      inference_eligible: true,
      non_inferiority: "conjunctive",
      benefit_metrics_eligible: true,
    },
    {
      id: "viewer-dynamic-fidelity",
      journey_id: "nasa-long-document-v1",
      input_lane: "native-replay-120hz",
      inference_eligible: true,
      non_inferiority: "conjunctive",
      benefit_metrics_eligible: true,
    },
  ],
  absolute_safety_budgets: {
    gpui_multi_document: structuredClone(
      gpuiMultiDocumentAbsoluteSafetyBudgetsV5,
    ),
  },
  quality_observation: {
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
  },
  statistics: {
    ...structuredClone(decisionContractV4.statistics),
    component_process_model:
      "one fresh process per component scenario in frozen scenario-contract-v5 order; the multi-document-session component alone opens four fixtures in that one process",
    component_aggregation: {
      ...structuredClone(decisionContractV4.statistics.component_aggregation),
      order_source: "scenario-contract-v5 current_runner_components",
    },
  },
  decision: {
    ...structuredClone(decisionContractV4.decision),
    hard_quality_rules: {
      gpui_multi_document_absolute_safety_budget: {
        contract_path: "#/absolute_safety_budgets/gpui_multi_document",
        applicability:
          "every-live-gpui-multi-document-session-report; mandatory-when-electron-benefit-metrics-ineligible",
        conjunction: "all-seven",
        exceeded_finite_valid: "measured-no",
        missing_nonfinite_or_structurally_invalid: "blocked-not-decision-ready",
      },
      registered_crop_algorithm: "bp-cross-engine-binary-scan-fidelity-v2",
      registered_crop_minimum_filtered_ssim_each: 0.97,
      registered_crop_minimum_dark_precision_each: 0.99,
      registered_crop_minimum_dark_recall_each: 0.99,
      registered_crop_minimum_dark_f1_each: 0.99,
      presented_scale_comparable_each: true,
      checkpoint_hold_stable_each: true,
      candidate_resampling: "forbidden",
      reference_resampling: "downsample-only-lanczos3",
      dynamic_fidelity_higher_is_better_lower_95_ratio: 0.95,
      missing_dynamic_observations: "not-decision-ready",
    },
  },
});

export function decisionContractArtifactHashV5(contract = decisionContractV5) {
  return canonicalSha256V5(contract);
}

export function validateDecisionContractV5(contract) {
  const errors = [];
  if (sourceV4ContractHash() !== sourceDecisionContractArtifactSha256V5) {
    errors.push("frozen v4 decision contract artifact hash changed");
  }
  if (contract?.contract_version !== decisionContractVersionV5) {
    errors.push(`contract_version must be ${decisionContractVersionV5}`);
  }
  if (
    canonicalSha256V5(
      contract?.absolute_safety_budgets?.gpui_multi_document,
    ) !== canonicalSha256V5(gpuiMultiDocumentAbsoluteSafetyBudgetsV5)
  ) {
    errors.push("GPUI multi-document absolute safety budgets are not exact");
  }
  if (
    contract?.source_contract?.id !== decisionContractV4.contract_version ||
    contract?.source_contract?.artifact_sha256 !==
      sourceDecisionContractArtifactSha256V5
  ) {
    errors.push("v5 must pin the immutable v4 source contract");
  }
  if (contract?.execution?.readiness_model !== "derived-from-live-evidence") {
    errors.push("execution readiness must be derived from live evidence");
  }
  if (
    contract?.execution?.static_capability_declarations_are_evidence !== false
  ) {
    errors.push(
      "static capability declarations must not be execution evidence",
    );
  }
  const fixtureIds = new Set(contract?.fixtures?.map(({ id }) => id) ?? []);
  for (const fixtureId of representativeFixtureIdsV5) {
    if (!fixtureIds.has(fixtureId)) {
      errors.push(`missing representative fixture ${fixtureId}`);
    }
  }
  if (fixtureIds.has("usgs-usa-geology-sheet-v1")) {
    errors.push("USGS must not be a representative fixture");
  }
  const journeys = new Map(
    contract?.journeys?.map((journey) => [journey.id, journey]) ?? [],
  );
  for (const journeyId of representativeJourneyIdsV5) {
    if (!journeys.has(journeyId)) {
      errors.push(`missing representative journey ${journeyId}`);
    }
  }
  const sessionFixtures = journeys.get("multi-document-session-v1")?.fixtures;
  if (
    JSON.stringify(sessionFixtures) !==
    JSON.stringify([
      "bp-single-page-v1",
      "nasa-apollo-summary-526-v1",
      "bp-engineering-sheet-v1",
      "bp-annotation-density-v1",
    ])
  ) {
    errors.push(
      "multi-document session must freeze the exact four-fixture order",
    );
  }
  const hardComponents = new Map(
    contract?.hard_components?.map((component) => [component.id, component]) ??
      [],
  );
  for (const componentId of hardComponentIdsV5) {
    const component = hardComponents.get(componentId);
    if (
      component?.inference_eligible !== true ||
      component?.non_inferiority !== "conjunctive"
    ) {
      errors.push(`${componentId} must be hard, inferential, and conjunctive`);
    }
    const expectedBenefitEligibility =
      componentId !== "native-property-edit-undo";
    if (component?.benefit_metrics_eligible !== expectedBenefitEligibility) {
      errors.push(
        `${componentId} benefit metric eligibility must be ${expectedBenefitEligibility}`,
      );
    }
    if (
      componentId === "multi-document-session" &&
      (component?.gpui_candidate_correctness_required !== true ||
        component?.electron_baseline_outcome !==
          "pass-or-exact-electron-multi-document-second-nasa-visible-pages-empty-v1")
    ) {
      errors.push(
        "multi-document-session must require GPUI correctness and retain the exact Electron second-NASA baseline outcome",
      );
    }
    if (
      componentId === "native-property-edit-undo" &&
      (component?.gpui_candidate_correctness_required !== true ||
        component?.electron_baseline_outcome !==
          "pass-or-exact-electron-numeric-property-input-blur-duplicate-history-v1")
    ) {
      errors.push(
        "native-property-edit-undo must require GPUI correctness and retain the exact Electron baseline outcome",
      );
    }
  }
  if (
    contract?.quality_observation?.registered_crop_count !== 3 ||
    contract?.quality_observation?.registered_crop_algorithm !==
      "bp-cross-engine-binary-scan-fidelity-v2" ||
    contract?.quality_observation
      ?.registered_crop_minimum_filtered_ssim_each !== 0.97 ||
    contract?.quality_observation
      ?.registered_crop_minimum_dark_precision_each !== 0.99 ||
    contract?.quality_observation?.registered_crop_minimum_dark_recall_each !==
      0.99 ||
    contract?.quality_observation?.registered_crop_minimum_dark_f1_each !==
      0.99 ||
    contract?.quality_observation?.presented_zoom_percent !== 100 ||
    contract?.quality_observation?.presented_device_pixels_per_pdf_point !==
      1 ||
    contract?.quality_observation?.presented_pixels_per_point_tolerance !==
      0.01 ||
    contract?.quality_observation?.paired_candidate_scale_tolerance !== 0.01 ||
    contract?.quality_observation?.checkpoint_hold_count !== 3 ||
    contract?.quality_observation?.checkpoint_hold_ms_each !== 250 ||
    contract?.quality_observation?.candidate_resampling !== "forbidden" ||
    contract?.quality_observation?.reference_resampling !==
      "downsample-only-lanczos3" ||
    contract?.quality_observation?.presented_screenshot_stability !==
      "painted-before-capture-after"
  ) {
    errors.push(
      "dynamic fidelity presented crop quality contract is not exact",
    );
  }
  if (
    contract?.quality_observation?.dynamic_sample_cadence !==
      "fixed-observer-60hz" ||
    contract?.quality_observation?.dynamic_sample_count !== 1921
  ) {
    errors.push("dynamic fidelity must freeze 1921 observations at 60 Hz");
  }
  const requiredFields = new Set(
    contract?.quality_observation?.required_sample_fields ?? [],
  );
  for (const field of [
    "visible_page_ready_fraction",
    "visible_raster_ready_area_fraction",
    "visible_raster_pixel_density",
  ]) {
    if (!requiredFields.has(field)) {
      errors.push(`dynamic fidelity is missing required field ${field}`);
    }
  }
  const usgs = contract?.stress_lanes?.find(
    ({ id }) => id === "usgs-large-sheet-stress-v1",
  );
  if (
    usgs?.fixture_id !== "usgs-usa-geology-sheet-v1" ||
    usgs?.inference_eligible !== false
  ) {
    errors.push("USGS must remain a non-inferential stress lane");
  }
  const hibbeler = contract?.supplementary_lanes?.find(
    ({ id }) => id === "private-hibbeler-935-v1",
  );
  if (
    hibbeler?.status !== "blocked-not-transferred" ||
    hibbeler?.inference_eligible !== false
  ) {
    errors.push(
      "Hibbeler must remain supplementary and blocked-not-transferred",
    );
  }
  return errors;
}

export function assessDecisionExecutionV5(evidence) {
  const blockers = [];
  if (evidence?.contract_version !== decisionContractVersionV5) {
    blockers.push(`contract_version must be ${decisionContractVersionV5}`);
  }
  for (const implementation of implementations) {
    const candidate = evidence?.implementations?.[implementation];
    if (!sha256Pattern.test(candidate?.candidate_artifact_sha256 ?? "")) {
      blockers.push(
        `${implementation}: frozen candidate artifact hash is missing`,
      );
    }
    for (const journeyId of representativeJourneyIdsV5) {
      const journey = candidate?.journeys?.[journeyId];
      if (
        journey?.live !== true ||
        journey?.passed !== true ||
        !sha256Pattern.test(journey?.evidence_sha256 ?? "")
      ) {
        blockers.push(
          `${implementation}:${journeyId}: live journey evidence did not pass`,
        );
      }
    }
    for (const componentId of hardComponentIdsV5) {
      const component = candidate?.hard_components?.[componentId];
      const exactElectronMultiDocumentDefect =
        implementation === "electron" &&
        componentId === "multi-document-session" &&
        component?.known_baseline_defect_id ===
          "electron-multi-document-second-nasa-visible-pages-empty-v1" &&
        component?.activated_fixture_id === "nasa-apollo-summary-526-v1" &&
        component?.activation_ordinal === 2 &&
        JSON.stringify(component?.visible_page_indices) === "[]" &&
        component?.queued_raster_count === 0 &&
        component?.inflight_raster_count === 0 &&
        component?.visible_raster_presented === false &&
        component?.error_presented === false &&
        component?.benchmark_metrics_eligible === false &&
        JSON.stringify(component?.benchmark_metrics_missing) ===
          JSON.stringify(electronMultiDocumentMissingBenefitMetricsV5);
      const exactElectronPropertyDefect =
        implementation === "electron" &&
        componentId === "native-property-edit-undo" &&
        component?.known_baseline_defect_id ===
          "electron-numeric-property-input-blur-duplicate-history-v1" &&
        component?.effective_history_revision_delta === 2 &&
        component?.application_undo_count === 1 &&
        component?.final_stroke_width_points === 4;
      if (
        component?.live !== true ||
        (!exactElectronMultiDocumentDefect &&
          !exactElectronPropertyDefect &&
          component?.passed !== true) ||
        !sha256Pattern.test(component?.evidence_sha256 ?? "")
      ) {
        blockers.push(
          `${implementation}:${componentId}: hard component evidence did not pass`,
        );
      }
    }
  }
  for (const gateId of requiredLiveEvidenceGateIdsV5) {
    const gate = evidence?.live_gates?.[gateId];
    if (
      gate?.live !== true ||
      gate?.passed !== true ||
      !sha256Pattern.test(gate?.evidence_sha256 ?? "")
    ) {
      blockers.push(`${gateId}: live evidence gate did not pass`);
    }
  }
  return {
    contract_version: decisionContractVersionV5,
    executable: blockers.length === 0,
    status:
      blockers.length === 0 ? "ready-final-execution" : "blocked-live-evidence",
    blockers,
  };
}
