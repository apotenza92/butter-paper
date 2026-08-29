import { decisionContract, decisionContractVersion } from "./decision-contract.mjs";
import {
  decisionContractV4,
  decisionContractVersionV4,
  representativeJourneyIdsV4,
  requiredLiveEvidenceGateIdsV4,
} from "./decision-contract-v4.mjs";
import { fullTimedRepresentativeScenarios } from "./run-paired.mjs";

export const requiredDecisionHardGates = Object.freeze([
  "representative_candidate_parity",
  "semantic_and_visual_correctness",
  "save_reopen_integrity",
  "evidence_completeness",
  "distribution_path",
  "migration_budget",
]);

export const requiredPrimaryMetrics = Object.freeze({
  ...decisionContract.decision.primary_metric_upper_95_thresholds,
});

export const decisionEvidenceSchemaVersion = 4;

const requiredV4JourneyIds = Object.freeze([
  ...representativeJourneyIdsV4,
]);

const requiredV4HardEvidence = Object.freeze([
  ...requiredLiveEvidenceGateIdsV4,
]);

export const requiredPrimaryMetricsV4 = Object.freeze({
  ...decisionContractV4.decision.primary_metric_upper_95_thresholds,
});

function binomialCdf(failures, attempts, probability) {
  if (probability <= 0) return 1;
  if (probability >= 1) return failures >= attempts ? 1 : 0;
  let term = (1 - probability) ** attempts;
  let sum = term;
  for (let count = 1; count <= failures; count += 1) {
    term *= ((attempts - count + 1) / count) * (probability / (1 - probability));
    sum += term;
  }
  return sum;
}

export function oneSidedFailureUpper95({ attempts, failures }) {
  if (!Number.isInteger(attempts) || attempts < 1) {
    throw new Error("reliability attempts must be a positive integer");
  }
  if (!Number.isInteger(failures) || failures < 0 || failures > attempts) {
    throw new Error("reliability failures must be an integer from zero through attempts");
  }
  if (failures === attempts) return 1;
  let lower = 0;
  let upper = 1;
  for (let iteration = 0; iteration < 80; iteration += 1) {
    const midpoint = (lower + upper) / 2;
    if (binomialCdf(failures, attempts, midpoint) > 0.05) lower = midpoint;
    else upper = midpoint;
  }
  return upper;
}

function evaluateV3MigrationDecision(evidence) {
  const hardGateFailures = [];
  const metricFailures = [];

  if (evidence?.contract_version !== decisionContractVersion) {
    hardGateFailures.push(`contract_version must be ${decisionContractVersion}`);
  }
  if (evidence?.execution_phase !== "final") {
    hardGateFailures.push("execution_phase must be final");
  }
  if (evidence?.candidate_frozen !== true) {
    hardGateFailures.push("candidate must be frozen before final inference");
  }
  for (const gate of requiredDecisionHardGates) {
    if (evidence?.hard_gates?.[gate] !== true) {
      hardGateFailures.push(`${gate} did not pass`);
    }
  }
  const pairedSummary = evidence?.paired_comparison_summary;
  if (
    pairedSummary?.complete !== true
    || pairedSummary?.decision_ready !== true
    || pairedSummary?.comparison_readiness?.ready !== true
  ) {
    hardGateFailures.push(
      "paired comparison summary did not pass full preflight and timing readiness",
    );
  }
  for (const scenario of fullTimedRepresentativeScenarios) {
    if (!pairedSummary?.scenarios?.[scenario]) {
      hardGateFailures.push(`paired comparison summary is missing timed scenario ${scenario}`);
    }
  }

  const sampling = evidence?.sampling;
  if (sampling?.calibration_pairs !== 6 || sampling?.calibration_included_in_inference !== false) {
    hardGateFailures.push("sampling requires six excluded calibration pairs");
  }
  const plannedFinalPairs = sampling?.planned_final_pairs;
  const completedFinalPairs = sampling?.completed_final_pairs;
  if (
    !Number.isInteger(plannedFinalPairs)
    || plannedFinalPairs < 24
    || plannedFinalPairs > 40
    || plannedFinalPairs % 4 !== 0
  ) {
    hardGateFailures.push("planned final pairs must be a multiple of 4 from 24 through 40");
  }
  if (completedFinalPairs !== plannedFinalPairs) {
    hardGateFailures.push(
      `completed final pairs ${completedFinalPairs ?? "missing"} do not match planned final pairs ${plannedFinalPairs ?? "missing"}`,
    );
  }
  if (sampling?.calculation?.final_pairs !== plannedFinalPairs) {
    hardGateFailures.push("planned final pairs do not match the frozen calibration calculation");
  }
  if (sampling?.calculation?.raw_pairs > 40) {
    hardGateFailures.push("calibration requires more than the maximum 40 final pairs");
  }

  const gpuiShellStarts = evidence?.preflight?.gpui_shell_starts;
  if (
    !Number.isInteger(gpuiShellStarts?.attempts)
    || gpuiShellStarts.attempts < 30
    || gpuiShellStarts.failures !== 0
    || !Number.isFinite(gpuiShellStarts.maximum_native_presentation_ms)
    || gpuiShellStarts.maximum_native_presentation_ms >= 5_000
  ) {
    hardGateFailures.push("GPUI preflight requires 30 successful shell starts below 5000 ms");
  }
  const electronZoomPromotions = evidence?.preflight?.electron_zoom_promotions;
  if (
    !Number.isInteger(electronZoomPromotions?.attempts)
    || electronZoomPromotions.attempts < 10
    || electronZoomPromotions.failures !== 0
  ) {
    hardGateFailures.push("Electron preflight requires 10 successful zoom quality promotions");
  }
  const gpuiHighZoomSequences = evidence?.preflight?.gpui_high_zoom_sequences;
  if (
    !Number.isInteger(gpuiHighZoomSequences?.attempts)
    || gpuiHighZoomSequences.attempts < 10
    || gpuiHighZoomSequences.failures !== 0
  ) {
    hardGateFailures.push("GPUI preflight requires 10 successful high-zoom tile sequences");
  }

  const attempts = evidence?.reliability?.app_cold_attempts;
  const failures = evidence?.reliability?.failures;
  let upperFailureRate95 = null;
  if (!Number.isInteger(attempts) || attempts < 100) {
    hardGateFailures.push(`reliability requires at least 100 app-cold attempts; got ${attempts ?? "missing"}`);
  }
  if (!Number.isInteger(failures) || failures < 0) {
    hardGateFailures.push("reliability failure count is missing or invalid");
  } else if (failures !== 0) {
    hardGateFailures.push(`reliability requires zero failures; got ${failures}`);
  }
  if (Number.isInteger(attempts) && attempts > 0 && Number.isInteger(failures) && failures >= 0 && failures <= attempts) {
    upperFailureRate95 = oneSidedFailureUpper95({ attempts, failures });
  }

  for (const [metric, threshold] of Object.entries(requiredPrimaryMetrics)) {
    const result = evidence?.primary_metrics?.[metric];
    if (!result) {
      metricFailures.push(`${metric} is missing`);
      continue;
    }
    if (result.absolute_budget_pass !== true) {
      metricFailures.push(`${metric} failed an absolute budget`);
    }
    if (!Number.isFinite(result.upper_95)) {
      metricFailures.push(`${metric} upper_95 is missing or invalid`);
    } else if (result.upper_95 > threshold) {
      metricFailures.push(`${metric} upper_95 ${result.upper_95} exceeds ${threshold}`);
    }
  }

  const passed = hardGateFailures.length === 0 && metricFailures.length === 0;
  return {
    contract_version: decisionContractVersion,
    decision: passed ? "yes" : "no",
    worth_migrating: passed,
    hard_gate_failures: hardGateFailures,
    metric_failures: metricFailures,
    reliability: {
      app_cold_attempts: attempts ?? null,
      failures: failures ?? null,
      upper_failure_rate_95: upperFailureRate95,
      passed: Number.isInteger(attempts) && attempts >= 100 && failures === 0,
    },
    rule: "yes only when every hard gate and every primary metric passes",
  };
}

function resolveEvidenceRef(root, ref) {
  if (typeof ref !== "string" || !ref.startsWith("#/")) return undefined;
  return ref.slice(2).split("/").reduce((value, segment) => {
    if (value === null || value === undefined) return undefined;
    const key = segment.replaceAll("~1", "/").replaceAll("~0", "~");
    return value[key];
  }, root);
}

function v4Reliability(summary, eligibilityFailures) {
  return Object.fromEntries(["electron", "gpui"].map((implementation) => {
    const retained = summary?.analysis?.reliability?.[implementation];
    const attempts = retained?.attempts;
    const failures = retained?.failures;
    let upperFailureRate95 = null;
    if (!Number.isInteger(attempts) || attempts < 100) {
      eligibilityFailures.push(
        `${implementation} reliability requires at least 100 retained attempts; got ${attempts ?? "missing"}`,
      );
    }
    if (!Number.isInteger(failures) || failures < 0 || failures > attempts) {
      eligibilityFailures.push(`${implementation} reliability failures are missing or invalid`);
    } else {
      if (failures !== 0) {
        eligibilityFailures.push(`${implementation} reliability requires zero failures; got ${failures}`);
      }
      if (Number.isInteger(attempts) && attempts > 0) {
        upperFailureRate95 = oneSidedFailureUpper95({ attempts, failures });
      }
    }
    if (
      !Array.isArray(retained?.attempt_refs)
      || retained.attempt_refs.length !== attempts
      || retained.attempt_refs.some((ref) => resolveEvidenceRef(summary, ref) === undefined)
    ) {
      eligibilityFailures.push(
        `${implementation} reliability attempts lack valid retained evidence references`,
      );
    }
    return [implementation, {
      attempts: attempts ?? null,
      failures: failures ?? null,
      upper_failure_rate_95: upperFailureRate95,
      passed: Number.isInteger(attempts) && attempts >= 100 && failures === 0,
      evidence_refs: retained?.attempt_refs ?? [],
      failure_refs: retained?.failure_refs ?? [],
    }];
  }));
}

function v4NotReady(evidence, eligibilityFailures, reliability) {
  return {
    contract_version: decisionContractVersionV4,
    evidence_schema_version: decisionEvidenceSchemaVersion,
    eligibility: "not-decision-ready",
    decision: "not-decision-ready",
    worth_migrating: null,
    eligibility_failures: eligibilityFailures,
    hard_gate_failures: [],
    metric_failures: [],
    reliability,
    rule: "incomplete evidence is not a technical no; infer yes or no only after eligibility passes",
  };
}

function evaluateV4MigrationDecision(evidence) {
  const eligibilityFailures = [];
  const summary = evidence?.paired_comparison_summary;
  const analysis = summary?.analysis;
  if (evidence?.execution_phase !== "final") {
    eligibilityFailures.push("execution_phase must be final");
  }
  if (evidence?.contract_version !== decisionContractVersionV4) {
    eligibilityFailures.push(`contract_version must be ${decisionContractVersionV4}`);
  }
  if (
    summary?.complete !== true
    || summary?.decision_ready !== true
    || summary?.comparison_readiness?.ready !== true
  ) {
    eligibilityFailures.push(
      "paired comparison summary did not pass full preflight and timing readiness",
    );
  }
  if (analysis?.schema_version !== decisionEvidenceSchemaVersion) {
    eligibilityFailures.push(
      `paired comparison analysis schema must be ${decisionEvidenceSchemaVersion}`,
    );
  }
  if (analysis?.contract_version !== decisionContractVersionV4) {
    eligibilityFailures.push(
      `paired comparison analysis contract must be ${decisionContractVersionV4}`,
    );
  }
  if (analysis?.complete !== true || analysis?.eligibility !== "decision-ready") {
    eligibilityFailures.push("paired comparison analysis is incomplete");
  }

  for (const journey of requiredV4JourneyIds) {
    const retained = analysis?.journeys?.[journey];
    if (retained?.status !== "complete") {
      eligibilityFailures.push(`${journey} journey evidence is incomplete`);
    }
    if (!Array.isArray(retained?.evidence_refs) || retained.evidence_refs.length === 0) {
      eligibilityFailures.push(`${journey} journey has no retained evidence references`);
    } else if (retained.evidence_refs.some((ref) => resolveEvidenceRef(summary, ref) === undefined)) {
      eligibilityFailures.push(`${journey} journey contains an invalid retained evidence reference`);
    }
  }
  for (const gate of requiredV4HardEvidence) {
    const refs = analysis?.hard_evidence_refs?.[gate];
    if (!Array.isArray(refs) || refs.length === 0) {
      eligibilityFailures.push(`${gate} has no retained hard-evidence references`);
    } else if (refs.some((ref) => resolveEvidenceRef(summary, ref) === undefined)) {
      eligibilityFailures.push(`${gate} contains an invalid retained hard-evidence reference`);
    } else if (
      ["candidate-artifacts-frozen", "fixture-bundle-verified"].includes(gate)
      && refs.some((ref) => !/^[0-9a-f]{64}$/i.test(resolveEvidenceRef(summary, ref)))
    ) {
      eligibilityFailures.push(`${gate} references a value that is not a SHA-256 identity`);
    }
  }
  const representativeScenarios = new Set(requiredV4JourneyIds.flatMap((journey) =>
    analysis?.journeys?.[journey]?.required_scenarios ?? []));
  for (const scenario of representativeScenarios) {
    const retained = summary?.scenarios?.[scenario];
    if (
      !retained
      || retained.expected_pair_count < 24
      || retained.valid_pair_count !== retained.expected_pair_count
    ) {
      eligibilityFailures.push(`${scenario} lacks at least 24 complete retained pairs`);
    }
  }

  const sampling = evidence?.sampling;
  if (sampling?.calibration_pairs !== 6 || sampling?.calibration_included_in_inference !== false) {
    eligibilityFailures.push("sampling requires six excluded calibration pairs");
  }
  const reliability = v4Reliability(summary, eligibilityFailures);

  for (const metric of Object.keys(requiredPrimaryMetricsV4)) {
    const family = analysis?.metric_families?.[metric];
    if (family?.status !== "complete" || !Number.isFinite(family?.paired_ratio?.upper_95)) {
      eligibilityFailures.push(`${metric} has missing retained measurements`);
      continue;
    }
    if (!Array.isArray(family.evidence_refs) || family.evidence_refs.length === 0) {
      eligibilityFailures.push(`${metric} has no retained metric references`);
    } else if (family.evidence_refs.some((ref) => resolveEvidenceRef(summary, ref) === undefined)) {
      eligibilityFailures.push(`${metric} contains an invalid retained metric reference`);
    }
  }

  if (eligibilityFailures.length > 0) {
    return v4NotReady(evidence, eligibilityFailures, reliability);
  }

  const metricFailures = [];
  for (const [metric, threshold] of Object.entries(requiredPrimaryMetricsV4)) {
    const family = analysis.metric_families[metric];
    if (family.absolute_budget?.passed !== true) {
      metricFailures.push(`${metric} failed an absolute budget`);
    }
    if (family.paired_ratio.upper_95 > threshold) {
      metricFailures.push(
        `${metric} upper_95 ${family.paired_ratio.upper_95} exceeds ${threshold}`,
      );
    }
  }
  const passed = metricFailures.length === 0;
  return {
    contract_version: decisionContractVersionV4,
    evidence_schema_version: decisionEvidenceSchemaVersion,
    eligibility: "decision-ready",
    decision: passed ? "yes" : "no",
    worth_migrating: passed,
    eligibility_failures: [],
    hard_gate_failures: [],
    metric_failures: metricFailures,
    reliability,
    evidence_refs: analysis.hard_evidence_refs,
    rule: "after evidence eligibility passes, yes requires every derived metric family and absolute budget to pass",
  };
}

export function evaluateMigrationDecision(evidence) {
  if (
    evidence?.evidence_schema_version === decisionEvidenceSchemaVersion
    || evidence?.paired_comparison_summary?.analysis?.schema_version === decisionEvidenceSchemaVersion
  ) {
    return evaluateV4MigrationDecision(evidence);
  }
  return evaluateV3MigrationDecision(evidence);
}
