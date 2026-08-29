export const decisionContractVersion = "bp-perf-v3-decision-3";

export const requiredDecisionFixtureIds = Object.freeze([
  "bp-single-page-v1",
  "bp-multi-page-v1",
  "bp-annotation-density-v1",
  "nasa-apollo-summary-526-v1",
  "usgs-usa-geology-sheet-v1",
  "bp-annotation-all-v1",
]);

export const requiredDecisionJourneyIds = Object.freeze([
  "viewer-journey-v1",
  "annotation-journey-v1",
  "persistence-journey-v1",
]);

export const decisionContract = Object.freeze({
  schema_version: 1,
  contract_version: decisionContractVersion,
  evidence_boundary:
    "fund-or-stop decision for completing the Butter Paper GPUI migration; not release qualification",
  execution: {
    status: "blocked-candidate-parity",
    executable: false,
    unblock_requires: [
      "bounded visible-tile rendering through 1600 percent zoom",
      "representative rectangle, pen/highlight, text, measurement, and image annotations",
      "selection, transform, properties, history, and annotation-aware thumbnails",
      "annotation import, safe save, semantic validation, and two reopen cycles",
      "native presentation and fixed visual-crop instrumentation",
    ],
  },
  fixtures: [
    { id: "bp-single-page-v1", storage: "generated", purpose: "shell and small cold-open control" },
    { id: "bp-multi-page-v1", storage: "generated", purpose: "navigation and cache control" },
    { id: "bp-annotation-density-v1", storage: "generated", purpose: "dense editing and resource pressure" },
    { id: "nasa-apollo-summary-526-v1", storage: "public", purpose: "long-document navigation and scroll" },
    { id: "usgs-usa-geology-sheet-v1", storage: "public", purpose: "large-sheet zoom, pan, and tiling" },
    { id: "bp-annotation-all-v1", storage: "generated", purpose: "representative and unknown annotation compatibility" },
  ],
  journeys: [
    {
      id: "viewer-journey-v1",
      fixtures: ["bp-single-page-v1", "bp-multi-page-v1", "nasa-apollo-summary-526-v1", "usgs-usa-geology-sheet-v1"],
      required_capabilities: [
        "cold shell and open",
        "normalized page navigation",
        "thirty-second timestamped continuous scroll",
        "zoom and fixed-position pan through 1600 percent",
        "cache-pressure cycles and post-close memory recovery",
      ],
    },
    {
      id: "annotation-journey-v1",
      fixtures: ["bp-annotation-density-v1"],
      required_capabilities: [
        "120 Hz draw, move, and resize replay",
        "rectangle, pen/highlight, text, measurement, and image editing",
        "hit test, selection, properties, copy, paste, delete, undo, and redo",
        "exact final semantic model and one history entry per gesture",
      ],
    },
    {
      id: "persistence-journey-v1",
      fixtures: ["bp-annotation-all-v1"],
      required_capabilities: [
        "annotation import and unknown-annotation preservation",
        "safe save followed by two reopen cycles",
        "independent PDF validation and semantic round-trip oracle",
        "matched post-presentation visual crops and annotation-aware thumbnails",
      ],
    },
  ],
  phases: {
    preflight: {
      gpui_shell_starts: 30,
      gpui_shell_native_presentation_budget_ms: 5_000,
      electron_zoom_quality_promotions: 10,
      gpui_high_zoom_tile_sequences: 10,
      allowed_failures: 0,
    },
    calibration: {
      pairs: 6,
      include_in_inference: false,
      permits_candidate_changes_after_phase: false,
    },
    final: {
      minimum_pairs: 24,
      maximum_pairs: 40,
      pair_block_size: 4,
      electron_first_per_block: 2,
      gpui_first_per_block: 2,
      bootstrap_resamples: 100_000,
      required_app_cold_attempts_per_implementation: 100,
      include_warmups_in_inference: false,
    },
  },
  statistics: {
    sampling_unit: "paired independent application process",
    pair_order: "seeded randomized blocks of four with two Electron-first and two GPUI-first pairs",
    sample_size: {
      alpha_two_sided: 0.05,
      power: 0.80,
      minimum_detectable_ratio: 1.10,
      variance_basis: "sample variance of paired log GPUI/Electron ratios from excluded calibration pairs",
      round_up_to_pair_block: 4,
    },
    bootstrap: {
      method: "paired percentile bootstrap of the geometric mean ratio",
      resamples: 100_000,
      confidence_level: 0.95,
    },
  },
  decision: {
    disposition: "no-until-all-conjunctive-gates-pass",
    primary_metric_upper_95_thresholds: {
      sustained_cpu_work: 0.80,
      process_memory: 0.75,
      native_interaction_and_frame_pacing: 1.05,
      product_latency: 1.10,
      gpu_memory: 1.15,
    },
  },
});

export function validateDecisionContract(contract) {
  const errors = [];
  if (contract?.contract_version !== decisionContractVersion) {
    errors.push(`contract_version must be ${decisionContractVersion}`);
  }
  if (contract?.execution?.executable !== false || contract?.execution?.status !== "blocked-candidate-parity") {
    errors.push("decision contract must remain blocked until candidate parity");
  }
  const fixtureIds = new Set(contract?.fixtures?.map(({ id }) => id) ?? []);
  for (const fixtureId of requiredDecisionFixtureIds) {
    if (!fixtureIds.has(fixtureId)) errors.push(`missing required fixture ${fixtureId}`);
  }
  const journeys = new Map(contract?.journeys?.map((journey) => [journey.id, journey]) ?? []);
  for (const journeyId of requiredDecisionJourneyIds) {
    const journey = journeys.get(journeyId);
    if (!journey) {
      errors.push(`missing required journey ${journeyId}`);
    } else if (!Array.isArray(journey.required_capabilities) || journey.required_capabilities.length === 0) {
      errors.push(`${journeyId} must declare required capabilities`);
    }
  }
  return errors;
}
