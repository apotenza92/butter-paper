export const protocolVersionV4 = "bp-perf-v4";
export const scenarioContractVersionV4 = "bp-perf-v4-representative-1";
const sha256Pattern = /^[0-9a-f]{64}$/;

export const representativeTimedScenarioIdsV4 = Object.freeze([
  "small-shell-open",
  "nasa-long-document",
  "engineering-sheet",
  "dense-mixed-editing",
  "persistence",
]);

export const engineeringZoomSequenceV4 = Object.freeze([
  100, 200, 400, 800, 1600, 400, 100, 800, 200, 100, 1200, 100,
]);

export function normalizedPageSequenceV4(pageCount) {
  if (!Number.isInteger(pageCount) || pageCount < 1) {
    throw new Error("page count must be a positive integer");
  }
  return [...new Set([
    pageCount,
    Math.ceil(pageCount * 0.08),
    Math.ceil(pageCount * 0.72),
    Math.ceil(pageCount * 0.25),
    Math.ceil(pageCount * 0.90),
    Math.ceil(pageCount * 0.50),
    11,
    Math.ceil(pageCount * 0.958),
    Math.ceil(pageCount * 0.33),
    1,
  ].map((page) => Math.max(1, Math.min(pageCount, page))))];
}

export const representativeScenarioDefinitionsV4 = Object.freeze({
  "small-shell-open": Object.freeze({
    journey_id: "small-shell-open-v1",
    fixture_id: "bp-single-page-v1",
    fixture_sha256: "f31adeeb3f17ef180012fe707cb2f2650854305dab4b16bba34d73652b6d8fdc",
    command_ids: Object.freeze(["small:launch-cold", "small:open-settle"]),
    current_runner_components: Object.freeze(["open-pdf"]),
    component_command_ids: Object.freeze({
      "open-pdf": Object.freeze(["small:launch-cold", "small:open-settle"]),
    }),
    blocked_commands: Object.freeze([]),
    component_weights: Object.freeze([1]),
    inference_eligible: true,
  }),
  "nasa-long-document": Object.freeze({
    journey_id: "nasa-long-document-v1",
    fixture_id: "nasa-apollo-summary-526-v1",
    fixture_sha256: "68d0f3bb93fd1c4e4f3adf99d483e9161f14293a311e25aa6c31241bbbb84049",
    command_ids: Object.freeze(["nasa:open-settle", "viewer:layout-single", "viewer:layout-continuous", "viewer:navigate-normalized", "viewer:continuous-scroll", "nasa:cache-pressure", "viewer:close-recover-reopen"]),
    current_runner_components: Object.freeze(["open-pdf", "viewer-layout", "page-navigation", "continuous-scroll", "cache-pressure", "close-reopen"]),
    component_command_ids: Object.freeze({
      "open-pdf": Object.freeze(["nasa:open-settle"]),
      "viewer-layout": Object.freeze(["viewer:layout-single", "viewer:layout-continuous"]),
      "page-navigation": Object.freeze(["viewer:navigate-normalized"]),
      "continuous-scroll": Object.freeze(["viewer:continuous-scroll"]),
      "cache-pressure": Object.freeze(["nasa:cache-pressure"]),
      "close-reopen": Object.freeze(["viewer:close-recover-reopen"]),
    }),
    blocked_commands: Object.freeze([]),
    component_weights: Object.freeze([1 / 6, 1 / 6, 1 / 6, 1 / 6, 1 / 6, 1 / 6]),
    inference_eligible: true,
  }),
  "engineering-sheet": Object.freeze({
    journey_id: "engineering-sheet-v1",
    fixture_id: "bp-engineering-sheet-v1",
    fixture_sha256: "49b417e4652a5fc0efb3b59b1f482b443bf3133f810f652559931a08b68a2b91",
    command_ids: Object.freeze(["engineering:open-settle", "engineering:fit-modes", "engineering:zoom-sequence", "engineering:pan", "engineering:cache-recovery"]),
    current_runner_components: Object.freeze([
      "open-pdf",
      "fit-modes",
      "zoom",
      "high-zoom-pan",
      "cache-pressure-recovery",
    ]),
    component_command_ids: Object.freeze({
      "open-pdf": Object.freeze(["engineering:open-settle"]),
      "fit-modes": Object.freeze(["engineering:fit-modes"]),
      zoom: Object.freeze(["engineering:zoom-sequence"]),
      "high-zoom-pan": Object.freeze(["engineering:pan"]),
      "cache-pressure-recovery": Object.freeze(["engineering:cache-recovery"]),
    }),
    blocked_commands: Object.freeze([]),
    component_weights: Object.freeze([0.2, 0.2, 0.2, 0.2, 0.2]),
    inference_eligible: true,
  }),
  "dense-mixed-editing": Object.freeze({
    journey_id: "dense-mixed-editing-v1",
    fixture_id: "bp-annotation-density-v1",
    fixture_sha256: "1dad337b97d573ff971e886eb6509e80e5e1cd07f6ac610ae4f2f9419c666682",
    command_ids: Object.freeze(["rectangle:create-sparse", "rectangle:select-move-resize", "rectangle:properties-history", "rectangle:repeat-dense", "highlight:create", "highlight:edit-history", "text:create", "text:edit-resize-history", "length:set-scale", "length:create", "length:edit-endpoint-history", "image:create", "image:resize-history"]),
    current_runner_components: Object.freeze(["annotation-create", "annotation-transform", "annotation-properties-history", "editor-create", "editor-workload"]),
    component_command_ids: Object.freeze({
      "annotation-create": Object.freeze(["rectangle:create-sparse", "highlight:create"]),
      "annotation-transform": Object.freeze(["rectangle:select-move-resize"]),
      "annotation-properties-history": Object.freeze(["rectangle:properties-history"]),
      "editor-create": Object.freeze(["text:create", "length:set-scale", "length:create", "image:create"]),
      "editor-workload": Object.freeze(["rectangle:repeat-dense", "highlight:edit-history", "text:edit-resize-history", "length:edit-endpoint-history", "image:resize-history"]),
    }),
    blocked_commands: Object.freeze([]),
    component_weights: Object.freeze([0.2, 0.2, 0.2, 0.2, 0.2]),
    inference_eligible: true,
  }),
  persistence: Object.freeze({
    journey_id: "persistence-v1",
    fixture_id: "bp-annotation-all-v1",
    fixture_sha256: "4a0a94cdbcc08e7ee06504914e5b84d218f2aeb01035b42d62f2275e38d02cbd",
    command_ids: Object.freeze(["unknown:import", "unknown:assert-cycle-1", "unknown:assert-cycle-2", "persistence:apply-fixed-state", "persistence:save-1", "persistence:reopen-1", "persistence:save-2", "persistence:reopen-2"]),
    current_runner_components: Object.freeze(["persistence-workload"]),
    component_command_ids: Object.freeze({
      "persistence-workload": Object.freeze(["unknown:import", "unknown:assert-cycle-1", "unknown:assert-cycle-2", "persistence:apply-fixed-state", "persistence:save-1", "persistence:reopen-1", "persistence:save-2", "persistence:reopen-2"]),
    }),
    blocked_commands: Object.freeze([]),
    component_weights: Object.freeze([1]),
    inference_eligible: true,
  }),
});

export const stressScenarioDefinitionsV4 = Object.freeze({
  "usgs-large-sheet-stress": Object.freeze({
    stress_lane_id: "usgs-large-sheet-stress-v1",
    fixture_id: "usgs-usa-geology-sheet-v1",
    fixture_sha256: "f058179e193ccbc15ca662feff3554102f64ff2114436a5ce6116d6fa5d2a6e2",
    command_ids: Object.freeze(["stress:usgs-open", "stress:usgs-zoom", "stress:usgs-pan"]),
    current_runner_components: Object.freeze(["open-pdf", "zoom", "high-zoom-pan"]),
    component_command_ids: Object.freeze({
      "open-pdf": Object.freeze(["stress:usgs-open"]),
      zoom: Object.freeze(["stress:usgs-zoom"]),
      "high-zoom-pan": Object.freeze(["stress:usgs-pan"]),
    }),
    blocked_commands: Object.freeze([]),
    component_weights: Object.freeze([1 / 3, 1 / 3, 1 / 3]),
    inference_eligible: false,
  }),
});

export function validateRepresentativeScenarioDefinitionsV4() {
  const errors = [];
  for (const scenario of representativeTimedScenarioIdsV4) {
    const definition = representativeScenarioDefinitionsV4[scenario];
    const expected = new Set(definition.command_ids);
    const observed = new Map();
    const components = new Set(definition.current_runner_components);
    for (const component of Object.keys(definition.component_command_ids)) {
      if (!components.has(component)) {
        errors.push(`${scenario}: command mapping names unknown component ${component}`);
      }
      for (const commandId of definition.component_command_ids[component]) {
        if (!expected.has(commandId)) {
          errors.push(`${scenario}: component ${component} maps unknown command ${commandId}`);
        }
        if (observed.has(commandId)) {
          errors.push(`${scenario}: command ${commandId} is mapped more than once`);
        }
        observed.set(commandId, `component:${component}`);
      }
    }
    for (const component of components) {
      if (!Object.hasOwn(definition.component_command_ids, component)) {
        errors.push(`${scenario}: component ${component} has no command mapping`);
      }
    }
    for (const blocker of definition.blocked_commands) {
      if (!expected.has(blocker.command_id)) {
        errors.push(`${scenario}: blocker names unknown command ${blocker.command_id}`);
      }
      if (observed.has(blocker.command_id)) {
        errors.push(`${scenario}: command ${blocker.command_id} is both mapped and blocked`);
      }
      if (typeof blocker.reason !== "string" || blocker.reason.length === 0) {
        errors.push(`${scenario}: blocked command ${blocker.command_id} has no reason`);
      }
      observed.set(blocker.command_id, "blocked");
    }
    for (const commandId of expected) {
      if (!observed.has(commandId)) {
        errors.push(`${scenario}: command ${commandId} is neither mapped nor blocked`);
      }
    }
  }
  return errors;
}

export const representativeScenarioBlockersV4 = Object.freeze(
  Object.fromEntries(representativeTimedScenarioIdsV4.map((scenario) => [
    scenario,
    representativeScenarioDefinitionsV4[scenario].blocked_commands,
  ])),
);

export function buildScenarioContractV4(workload, scenario) {
  const representative = representativeScenarioDefinitionsV4[scenario];
  const stress = stressScenarioDefinitionsV4[scenario];
  const definition = representative ?? stress;
  if (!definition) throw new Error(`unknown v4 scenario ${scenario}`);
  if (representative) {
    const definitionErrors = validateRepresentativeScenarioDefinitionsV4();
    if (definitionErrors.length > 0) {
      throw new Error(`invalid v4 representative scenario definitions: ${definitionErrors.join("; ")}`);
    }
  }

  const source = representative
    ? workload.journeys.find(({ id }) => id === definition.journey_id)
    : workload.stress_lanes.find(({ id }) => id === definition.stress_lane_id);
  if (!source) throw new Error(`v4 scenario ${scenario} source is missing`);
  const commands = definition.command_ids.map((commandId) => {
    const command = source.commands.find(({ id }) => id === commandId);
    if (!command) throw new Error(`v4 scenario ${scenario} is missing command ${commandId}`);
    return command;
  });
  return {
    scenario_contract_version: scenarioContractVersionV4,
    manifest_id: workload.manifest_id,
    scenario,
    lane: representative ? "representative-inference" : "stress-diagnostic",
    inference_eligible: definition.inference_eligible,
    fixture_id: definition.fixture_id,
    fixture_sha256: definition.fixture_sha256,
    command_ids: [...definition.command_ids],
    current_runner_components: [...definition.current_runner_components],
    component_command_ids: Object.fromEntries(
      Object.entries(definition.component_command_ids)
        .map(([component, commandIds]) => [component, [...commandIds]]),
    ),
    blocked_commands: definition.blocked_commands.map((blocker) => ({ ...blocker })),
    component_weights: [...definition.component_weights],
    component_aggregation: {
      order: [...definition.current_runner_components],
      weights: [...definition.component_weights],
      benefit_metric_method: "weighted geometric mean",
      non_inferiority_method: "conjunctive every component",
      compensating_regressions_allowed: false,
    },
    commands,
    execution_eligible: false,
    execution_blocker: "live runner support and exact receipts have not been supplied",
  };
}

export function assessScenarioExecutionV4(workload, scenario, evidence) {
  const contract = buildScenarioContractV4(workload, scenario);
  const blockers = contract.blocked_commands.map(({ command_id: commandId, reason }) =>
    `${scenario}:${commandId}: ${reason}`);
  for (const implementation of ["electron", "gpui"]) {
    const candidate = evidence?.implementations?.[implementation];
    if (!sha256Pattern.test(candidate?.candidate_artifact_sha256 ?? "")) {
      blockers.push(`${implementation}: frozen candidate artifact hash is missing`);
    }
    const receipts = new Map(
      (candidate?.command_receipts ?? []).map((receipt) => [receipt.command_id, receipt]),
    );
    for (const commandId of contract.command_ids) {
      const receipt = receipts.get(commandId);
      if (receipt?.live !== true || receipt?.passed !== true
        || !sha256Pattern.test(receipt?.evidence_sha256 ?? "")) {
        blockers.push(`${implementation}:${commandId}: live command receipt did not pass`);
      }
    }
  }
  return {
    ...contract,
    execution_eligible: blockers.length === 0,
    execution_blocker: blockers.length === 0 ? null : "live runner support is incomplete",
    execution_blockers: blockers,
  };
}
