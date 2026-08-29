import {
  hardComponentIdsV5,
  representativeJourneyIdsV5,
} from "./decision-contract-v5.mjs";
import {
  engineeringZoomSequenceV4,
  normalizedPageSequenceV4,
  representativeScenarioDefinitionsV4,
  representativeTimedScenarioIdsV4,
  stressScenarioDefinitionsV4,
} from "./scenario-contract-v4.mjs";

export const protocolVersionV5 = "bp-perf-v5";
export const scenarioContractVersionV5 = "bp-perf-v5-representative-1";
const sha256Pattern = /^[0-9a-f]{64}$/;

export const representativeTimedScenarioIdsV5 = Object.freeze([
  ...representativeTimedScenarioIdsV4,
  "multi-document-session",
]);

export const engineeringZoomSequenceV5 = engineeringZoomSequenceV4;
export const normalizedPageSequenceV5 = normalizedPageSequenceV4;

function equalWeights(count) {
  return Array.from({ length: count }, () => 1 / count);
}

function singleFixtureDefinition(source) {
  return {
    ...structuredClone(source),
    fixture_ids: [source.fixture_id],
    fixture_sha256_by_id: {
      [source.fixture_id]: source.fixture_sha256,
    },
    component_fixture_ids: Object.fromEntries(
      source.current_runner_components.map((component) => [
        component,
        [source.fixture_id],
      ]),
    ),
    component_benefit_metrics_eligible: Object.fromEntries(
      source.current_runner_components.map((component) => [component, true]),
    ),
  };
}

const small = singleFixtureDefinition(
  representativeScenarioDefinitionsV4["small-shell-open"],
);
const nasa = singleFixtureDefinition(
  representativeScenarioDefinitionsV4["nasa-long-document"],
);
nasa.command_ids.push("viewer:dynamic-fidelity-scroll");
nasa.current_runner_components.push("viewer-dynamic-fidelity");
nasa.component_command_ids["viewer-dynamic-fidelity"] = [
  "viewer:dynamic-fidelity-scroll",
];
nasa.component_fixture_ids["viewer-dynamic-fidelity"] = [
  "nasa-apollo-summary-526-v1",
];
nasa.component_benefit_metrics_eligible["viewer-dynamic-fidelity"] = true;
nasa.component_weights = equalWeights(nasa.current_runner_components.length);

const engineering = singleFixtureDefinition(
  representativeScenarioDefinitionsV4["engineering-sheet"],
);
const dense = singleFixtureDefinition(
  representativeScenarioDefinitionsV4["dense-mixed-editing"],
);
dense.command_ids.push(
  "annotation:native-property-edit-undo",
  "annotation:native-snap-transform-120hz",
);
dense.current_runner_components.push(
  "native-property-edit-undo",
  "native-snap-transform-120hz",
);
dense.component_command_ids["native-property-edit-undo"] = [
  "annotation:native-property-edit-undo",
];
dense.component_command_ids["native-snap-transform-120hz"] = [
  "annotation:native-snap-transform-120hz",
];
dense.component_fixture_ids["native-property-edit-undo"] = [
  "bp-annotation-density-v1",
];
dense.component_fixture_ids["native-snap-transform-120hz"] = [
  "bp-annotation-density-v1",
];
dense.component_benefit_metrics_eligible["native-property-edit-undo"] = false;
dense.component_benefit_metrics_eligible["native-snap-transform-120hz"] = true;
dense.component_weights = equalWeights(dense.current_runner_components.length);

const persistence = singleFixtureDefinition(
  representativeScenarioDefinitionsV4.persistence,
);

const multiDocumentFixtureIds = Object.freeze([
  "bp-single-page-v1",
  "nasa-apollo-summary-526-v1",
  "bp-engineering-sheet-v1",
  "bp-annotation-density-v1",
]);

export const representativeScenarioDefinitionsV5 = Object.freeze({
  "small-shell-open": Object.freeze(small),
  "nasa-long-document": Object.freeze(nasa),
  "engineering-sheet": Object.freeze(engineering),
  "dense-mixed-editing": Object.freeze(dense),
  persistence: Object.freeze(persistence),
  "multi-document-session": Object.freeze({
    journey_id: "multi-document-session-v1",
    fixture_id: null,
    fixture_sha256: null,
    fixture_ids: multiDocumentFixtureIds,
    fixture_sha256_by_id: Object.freeze({
      "bp-single-page-v1":
        "f31adeeb3f17ef180012fe707cb2f2650854305dab4b16bba34d73652b6d8fdc",
      "nasa-apollo-summary-526-v1":
        "68d0f3bb93fd1c4e4f3adf99d483e9161f14293a311e25aa6c31241bbbb84049",
      "bp-engineering-sheet-v1":
        "49b417e4652a5fc0efb3b59b1f482b443bf3133f810f652559931a08b68a2b91",
      "bp-annotation-density-v1":
        "1dad337b97d573ff971e886eb6509e80e5e1cd07f6ac610ae4f2f9419c666682",
    }),
    command_ids: Object.freeze([
      "session:open-four-fixtures",
      "session:switch-four-fixtures",
      "session:edit-dense-rectangle",
      "session:close-three-and-recover",
    ]),
    current_runner_components: Object.freeze(["multi-document-session"]),
    component_command_ids: Object.freeze({
      "multi-document-session": Object.freeze([
        "session:open-four-fixtures",
        "session:switch-four-fixtures",
        "session:edit-dense-rectangle",
        "session:close-three-and-recover",
      ]),
    }),
    component_fixture_ids: Object.freeze({
      "multi-document-session": multiDocumentFixtureIds,
    }),
    component_benefit_metrics_eligible: Object.freeze({
      "multi-document-session": true,
    }),
    blocked_commands: Object.freeze([]),
    component_weights: Object.freeze([1]),
    inference_eligible: true,
  }),
});

export const stressScenarioDefinitionsV5 = Object.freeze(
  Object.fromEntries(
    Object.entries(stressScenarioDefinitionsV4).map(
      ([scenario, definition]) => [
        scenario,
        Object.freeze(singleFixtureDefinition(definition)),
      ],
    ),
  ),
);

export function validateRepresentativeScenarioDefinitionsV5() {
  const errors = [];
  const journeyIds = [];
  const hardComponents = new Set();
  for (const scenario of representativeTimedScenarioIdsV5) {
    const definition = representativeScenarioDefinitionsV5[scenario];
    if (!definition) {
      errors.push(`${scenario}: scenario definition is missing`);
      continue;
    }
    journeyIds.push(definition.journey_id);
    const expected = new Set(definition.command_ids);
    const observed = new Map();
    const components = new Set(definition.current_runner_components);
    if (
      definition.component_weights.length !==
        definition.current_runner_components.length ||
      Math.abs(
        definition.component_weights.reduce((sum, weight) => sum + weight, 0) -
          1,
      ) > 1e-12
    ) {
      errors.push(`${scenario}: component weights are not normalized`);
    }
    for (const component of Object.keys(definition.component_command_ids)) {
      if (!components.has(component)) {
        errors.push(
          `${scenario}: command mapping names unknown component ${component}`,
        );
      }
      if (hardComponentIdsV5.includes(component)) hardComponents.add(component);
      for (const commandId of definition.component_command_ids[component]) {
        if (!expected.has(commandId)) {
          errors.push(
            `${scenario}: component ${component} maps unknown command ${commandId}`,
          );
        }
        if (observed.has(commandId)) {
          errors.push(
            `${scenario}: command ${commandId} is mapped more than once`,
          );
        }
        observed.set(commandId, `component:${component}`);
      }
      const fixtures = definition.component_fixture_ids?.[component];
      if (!Array.isArray(fixtures) || fixtures.length === 0) {
        errors.push(
          `${scenario}: component ${component} has no fixture mapping`,
        );
      }
      for (const fixtureId of fixtures ?? []) {
        if (!definition.fixture_ids.includes(fixtureId)) {
          errors.push(
            `${scenario}: component ${component} maps unknown fixture ${fixtureId}`,
          );
        }
      }
    }
    for (const component of components) {
      if (!Object.hasOwn(definition.component_command_ids, component)) {
        errors.push(
          `${scenario}: component ${component} has no command mapping`,
        );
      }
      if (!Object.hasOwn(definition.component_fixture_ids, component)) {
        errors.push(
          `${scenario}: component ${component} has no fixture mapping`,
        );
      }
      if (
        typeof definition.component_benefit_metrics_eligible?.[component] !==
        "boolean"
      ) {
        errors.push(
          `${scenario}: component ${component} has no benefit metric eligibility`,
        );
      }
    }
    for (const blocker of definition.blocked_commands) {
      if (!expected.has(blocker.command_id)) {
        errors.push(
          `${scenario}: blocker names unknown command ${blocker.command_id}`,
        );
      }
      if (observed.has(blocker.command_id)) {
        errors.push(
          `${scenario}: command ${blocker.command_id} is both mapped and blocked`,
        );
      }
      if (typeof blocker.reason !== "string" || blocker.reason.length === 0) {
        errors.push(
          `${scenario}: blocked command ${blocker.command_id} has no reason`,
        );
      }
      observed.set(blocker.command_id, "blocked");
    }
    for (const commandId of expected) {
      if (!observed.has(commandId)) {
        errors.push(
          `${scenario}: command ${commandId} is neither mapped nor blocked`,
        );
      }
    }
  }
  if (
    JSON.stringify(journeyIds) !== JSON.stringify(representativeJourneyIdsV5)
  ) {
    errors.push("representative scenario-to-journey mapping is not exact");
  }
  for (const componentId of hardComponentIdsV5) {
    if (!hardComponents.has(componentId)) {
      errors.push(`hard component ${componentId} is not mapped`);
    }
  }
  const hardBenefitEligibility = Object.fromEntries(
    Object.values(representativeScenarioDefinitionsV5).flatMap((definition) =>
      definition.current_runner_components
        .filter((component) => hardComponentIdsV5.includes(component))
        .map((component) => [
          component,
          definition.component_benefit_metrics_eligible[component],
        ]),
    ),
  );
  for (const componentId of hardComponentIdsV5) {
    const expected = componentId !== "native-property-edit-undo";
    if (hardBenefitEligibility[componentId] !== expected) {
      errors.push(
        `${componentId}: benefit metric eligibility must be ${expected}`,
      );
    }
  }
  return errors;
}

export const representativeScenarioBlockersV5 = Object.freeze(
  Object.fromEntries(
    representativeTimedScenarioIdsV5.map((scenario) => [
      scenario,
      representativeScenarioDefinitionsV5[scenario].blocked_commands,
    ]),
  ),
);

export function buildScenarioContractV5(workload, scenario) {
  const representative = representativeScenarioDefinitionsV5[scenario];
  const stress = stressScenarioDefinitionsV5[scenario];
  const definition = representative ?? stress;
  if (!definition) throw new Error(`unknown v5 scenario ${scenario}`);
  if (representative) {
    const definitionErrors = validateRepresentativeScenarioDefinitionsV5();
    if (definitionErrors.length > 0) {
      throw new Error(
        `invalid v5 representative scenario definitions: ${definitionErrors.join("; ")}`,
      );
    }
  }
  const source = representative
    ? workload.journeys.find(({ id }) => id === definition.journey_id)
    : workload.stress_lanes.find(({ id }) => id === definition.stress_lane_id);
  if (!source) throw new Error(`v5 scenario ${scenario} source is missing`);
  const commands = definition.command_ids.map((commandId) => {
    const command = source.commands.find(({ id }) => id === commandId);
    if (!command) {
      throw new Error(
        `v5 scenario ${scenario} is missing command ${commandId}`,
      );
    }
    return command;
  });
  return {
    scenario_contract_version: scenarioContractVersionV5,
    manifest_id: workload.manifest_id,
    scenario,
    lane: representative ? "representative-inference" : "stress-diagnostic",
    inference_eligible: definition.inference_eligible,
    fixture_id: definition.fixture_id,
    fixture_sha256: definition.fixture_sha256,
    fixture_ids: [...definition.fixture_ids],
    fixture_sha256_by_id: { ...definition.fixture_sha256_by_id },
    command_ids: [...definition.command_ids],
    current_runner_components: [...definition.current_runner_components],
    component_command_ids: Object.fromEntries(
      Object.entries(definition.component_command_ids).map(
        ([component, commandIds]) => [component, [...commandIds]],
      ),
    ),
    component_fixture_ids: Object.fromEntries(
      Object.entries(definition.component_fixture_ids).map(
        ([component, fixtureIds]) => [component, [...fixtureIds]],
      ),
    ),
    component_benefit_metrics_eligible: {
      ...definition.component_benefit_metrics_eligible,
    },
    hard_components: definition.current_runner_components.filter((component) =>
      hardComponentIdsV5.includes(component),
    ),
    blocked_commands: definition.blocked_commands.map((blocker) => ({
      ...blocker,
    })),
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
    execution_blocker:
      "live v5 runner support and exact receipts have not been supplied",
  };
}

export function assessScenarioExecutionV5(workload, scenario, evidence) {
  const contract = buildScenarioContractV5(workload, scenario);
  const blockers = contract.blocked_commands.map(
    ({ command_id: commandId, reason }) =>
      `${scenario}:${commandId}: ${reason}`,
  );
  for (const implementation of ["electron", "gpui"]) {
    const candidate = evidence?.implementations?.[implementation];
    if (!sha256Pattern.test(candidate?.candidate_artifact_sha256 ?? "")) {
      blockers.push(
        `${implementation}: frozen candidate artifact hash is missing`,
      );
    }
    const receipts = new Map(
      (candidate?.command_receipts ?? []).map((receipt) => [
        receipt.command_id,
        receipt,
      ]),
    );
    for (const commandId of contract.command_ids) {
      const receipt = receipts.get(commandId);
      if (
        receipt?.live !== true ||
        receipt?.passed !== true ||
        !sha256Pattern.test(receipt?.evidence_sha256 ?? "")
      ) {
        blockers.push(
          `${implementation}:${commandId}: live command receipt did not pass`,
        );
      }
    }
  }
  return {
    ...contract,
    execution_eligible: blockers.length === 0,
    execution_blocker:
      blockers.length === 0 ? null : "live v5 runner support is incomplete",
    execution_blockers: blockers,
  };
}
