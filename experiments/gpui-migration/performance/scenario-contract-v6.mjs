import {
  representativeScenarioDefinitionsV5,
  representativeTimedScenarioIdsV5,
} from "./scenario-contract-v5.mjs";

export const protocolVersionV6 = "bp-perf-v6";
export const scenarioContractVersionV6 = "bp-perf-v6-representative-1";

export const benefitEligibleComponentIdsV6 = Object.freeze([
  "open-pdf",
  "continuous-scroll",
  "viewer-dynamic-fidelity",
  "annotation-create",
  "annotation-transform",
  "editor-create",
  "native-snap-transform-120hz",
  "multi-document-session",
]);

export const semanticCorrectnessOnlyComponentIdsV6 = Object.freeze([
  "viewer-layout",
  "page-navigation",
  "cache-pressure",
  "close-reopen",
  "fit-modes",
  "zoom",
  "high-zoom-pan",
  "cache-pressure-recovery",
  "annotation-properties-history",
  "editor-workload",
  "persistence-workload",
]);

export const propertyCorrectnessOnlyComponentIdV6 = "native-property-edit-undo";

const benefitSet = new Set(benefitEligibleComponentIdsV6);
const semanticSet = new Set(semanticCorrectnessOnlyComponentIdsV6);

export const representativeScenarioDefinitionsV6 = Object.freeze(
  Object.fromEntries(
    representativeTimedScenarioIdsV5.map((scenario) => {
      const source = representativeScenarioDefinitionsV5[scenario];
      const benefitComponents = source.current_runner_components.filter(
        (component) => benefitSet.has(component),
      );
      const correctnessOnlyComponents = source.current_runner_components.filter(
        (component) =>
          semanticSet.has(component) ||
          component === propertyCorrectnessOnlyComponentIdV6,
      );
      return [
        scenario,
        Object.freeze({
          ...structuredClone(source),
          benefit_components: Object.freeze(benefitComponents),
          correctness_only_components: Object.freeze(correctnessOnlyComponents),
          component_benefit_metrics_eligible: Object.freeze(
            Object.fromEntries(
              source.current_runner_components.map((component) => [
                component,
                benefitSet.has(component),
              ]),
            ),
          ),
          benefit_component_weights: Object.freeze(
            benefitComponents.map(() => 1 / benefitComponents.length),
          ),
        }),
      ];
    }),
  ),
);

export function validateScenarioContractV6() {
  const failures = [];
  const observedBenefit = new Set();
  const observedSemantic = new Set();
  let benefitOccurrences = 0;
  for (const scenario of representativeTimedScenarioIdsV5) {
    const definition = representativeScenarioDefinitionsV6[scenario];
    benefitOccurrences += definition.benefit_components.length;
    for (const component of definition.benefit_components) {
      observedBenefit.add(component);
      if (definition.component_benefit_metrics_eligible[component] !== true) {
        failures.push(`${scenario}:${component} is not benefit-eligible`);
      }
    }
    for (const component of definition.correctness_only_components) {
      if (component !== propertyCorrectnessOnlyComponentIdV6) {
        observedSemantic.add(component);
      }
      if (definition.component_benefit_metrics_eligible[component] !== false) {
        failures.push(`${scenario}:${component} is not correctness-only`);
      }
    }
  }
  if (
    [...observedBenefit].sort().join("\0") !==
    [...benefitEligibleComponentIdsV6].sort().join("\0")
  ) {
    failures.push("benefit component set is not exact");
  }
  if (
    [...observedSemantic].sort().join("\0") !==
    [...semanticCorrectnessOnlyComponentIdsV6].sort().join("\0")
  ) {
    failures.push("semantic correctness-only component set is not exact");
  }
  if (benefitOccurrences !== 10) {
    failures.push(
      `benefit component occurrence count is ${benefitOccurrences}`,
    );
  }
  return failures;
}
