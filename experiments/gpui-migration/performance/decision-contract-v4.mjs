export const decisionContractVersionV4 = "bp-perf-v4-decision-1";

export const representativeJourneyIdsV4 = Object.freeze([
  "small-shell-open-v1",
  "nasa-long-document-v1",
  "engineering-sheet-v1",
  "dense-mixed-editing-v1",
  "persistence-v1",
]);

export const representativeFixtureIdsV4 = Object.freeze([
  "bp-single-page-v1",
  "nasa-apollo-summary-526-v1",
  "bp-engineering-sheet-v1",
  "bp-annotation-density-v1",
  "bp-annotation-all-v1",
]);

export const requiredLiveEvidenceGateIdsV4 = Object.freeze([
  "candidate-artifacts-frozen",
  "fixture-bundle-verified",
  "representative-command-receipts-exact",
  "semantic-visual-persistence-oracles-passed",
  "native-application-frame-traces-passed",
  "resource-observations-complete",
]);

const implementations = Object.freeze(["electron", "gpui"]);
const sha256Pattern = /^[0-9a-f]{64}$/;

export const decisionContractV4 = Object.freeze({
  schema_version: 2,
  contract_version: decisionContractVersionV4,
  supersedes: "bp-perf-v3-decision-3",
  evidence_boundary:
    "fund-or-stop decision for completing the Butter Paper GPUI migration; not packaged release qualification",
  execution: {
    readiness_model: "derived-from-live-evidence",
    static_capability_declarations_are_evidence: false,
    required_live_gate_ids: [...requiredLiveEvidenceGateIdsV4],
    required_implementations: [...implementations],
    required_representative_journey_ids: [...representativeJourneyIdsV4],
  },
  fixtures: [
    {
      id: "bp-single-page-v1",
      storage: "generated",
      purpose: "small shell and cold-open control",
    },
    {
      id: "nasa-apollo-summary-526-v1",
      storage: "public",
      purpose: "long-document navigation, scroll, cache, and memory",
    },
    {
      id: "bp-engineering-sheet-v1",
      storage: "generated",
      purpose: "moderate-sheet zoom, pan, tiling, and cancellation",
    },
    {
      id: "bp-annotation-density-v1",
      storage: "generated",
      purpose: "sparse and dense mixed editing",
    },
    {
      id: "bp-annotation-all-v1",
      storage: "generated",
      purpose: "native annotation persistence and unknown preservation",
    },
  ],
  journeys: [
    {
      id: "small-shell-open-v1",
      fixtures: ["bp-single-page-v1"],
      required_capabilities: [
        "app-cold launch",
        "native window presentation",
        "interactive shell",
        "small PDF preview and settled raster",
      ],
    },
    {
      id: "nasa-long-document-v1",
      fixtures: ["nasa-apollo-summary-526-v1"],
      required_capabilities: [
        "single and continuous layout",
        "normalized navigation",
        "timestamped continuous scroll",
        "bounded virtualization and resource recovery",
      ],
    },
    {
      id: "engineering-sheet-v1",
      fixtures: ["bp-engineering-sheet-v1"],
      required_capabilities: [
        "fit page and fit width",
        "visible tiling through 1600 percent",
        "fixed pan path",
        "cancellation and stale-generation rejection",
      ],
    },
    {
      id: "dense-mixed-editing-v1",
      fixtures: ["bp-annotation-density-v1"],
      required_capabilities: [
        "sparse and dense editing",
        "rectangle and highlight native replay",
        "text, measurement, and image editing",
        "selection, properties, history, thumbnails, and exact final state",
      ],
    },
    {
      id: "persistence-v1",
      fixtures: ["bp-annotation-all-v1"],
      required_capabilities: [
        "native annotation import",
        "safe save and two reopen cycles",
        "independent PDF validation",
        "unknown annotation and original content preservation",
      ],
    },
  ],
  stress_lanes: [
    {
      id: "usgs-large-sheet-stress-v1",
      fixture_id: "usgs-usa-geology-sheet-v1",
      inference_eligible: false,
      purpose:
        "pathological 180 MB large-sheet robustness, tiling, cancellation, and memory limits",
      disposition_rule:
        "report independently; GPUI failure blocks migration only when Electron passes the same frozen stress lane",
    },
  ],
  supplementary_lanes: [
    {
      id: "private-hibbeler-935-v1",
      status: "blocked-not-transferred",
      inference_eligible: false,
      allowed_lane: "owner-authorized-local-macos",
    },
  ],
  resource_observation: {
    decoded_payload_bytes: "exact-required",
    renderer_resource_submission_bytes: "exact-required",
    physical_bus_upload_bytes: "optional-nullable",
    whole_device_gpu_samples: "baseline-adjusted-diagnostic",
  },
  phases: {
    calibration: { pairs_per_journey: 6, include_in_inference: false },
    final: {
      minimum_pairs_per_journey: 24,
      maximum_pairs_per_journey: 40,
      pair_block_size: 4,
    },
  },
  statistics: {
    sampling_unit: "paired isolated journey execution bundle",
    component_process_model:
      "one fresh process per component scenario in the frozen scenario-contract-v4 order",
    pair_order:
      "seeded randomized blocks of four with two Electron-first and two GPUI-first pairs",
    component_aggregation: {
      order_source: "scenario-contract-v4 current_runner_components",
      benefit_metric_method:
        "equal-weight geometric mean of component paired ratios within each journey",
      non_inferiority_method: "every component must pass its family threshold",
      compensating_regressions_allowed: false,
    },
    bootstrap: {
      method: "paired percentile bootstrap of the geometric mean ratio",
      resamples: 100_000,
      confidence_level: 0.95,
    },
  },
  decision: {
    result_states: ["not-decision-ready", "yes", "no"],
    primary_metric_upper_95_thresholds: {
      sustained_cpu_work: 0.8,
      process_memory: 0.75,
      native_interaction_and_frame_pacing: 1.05,
      product_latency: 1.1,
      gpu_resource_pressure: 1.15,
    },
  },
});

export function validateDecisionContractV4(contract) {
  const errors = [];
  if (contract?.contract_version !== decisionContractVersionV4) {
    errors.push(`contract_version must be ${decisionContractVersionV4}`);
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
  for (const fixtureId of representativeFixtureIdsV4) {
    if (!fixtureIds.has(fixtureId))
      errors.push(`missing representative fixture ${fixtureId}`);
  }
  if (fixtureIds.has("usgs-usa-geology-sheet-v1")) {
    errors.push("USGS must not be a representative fixture");
  }
  const journeys = new Map(
    contract?.journeys?.map((journey) => [journey.id, journey]) ?? [],
  );
  for (const journeyId of representativeJourneyIdsV4) {
    if (!journeys.has(journeyId))
      errors.push(`missing representative journey ${journeyId}`);
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

export function assessDecisionExecutionV4(evidence) {
  const blockers = [];
  if (evidence?.contract_version !== decisionContractVersionV4) {
    blockers.push(`contract_version must be ${decisionContractVersionV4}`);
  }
  for (const implementation of implementations) {
    const candidate = evidence?.implementations?.[implementation];
    if (!sha256Pattern.test(candidate?.candidate_artifact_sha256 ?? "")) {
      blockers.push(
        `${implementation}: frozen candidate artifact hash is missing`,
      );
    }
    for (const journeyId of representativeJourneyIdsV4) {
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
  }
  for (const gateId of requiredLiveEvidenceGateIdsV4) {
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
    contract_version: decisionContractVersionV4,
    executable: blockers.length === 0,
    status:
      blockers.length === 0 ? "ready-final-execution" : "blocked-live-evidence",
    blockers,
  };
}
