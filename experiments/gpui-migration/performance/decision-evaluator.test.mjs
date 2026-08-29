import assert from "node:assert/strict";
import test from "node:test";

import {
  decisionEvidenceSchemaVersion,
  evaluateMigrationDecision,
  requiredDecisionHardGates,
  requiredPrimaryMetrics,
  requiredPrimaryMetricsV4,
} from "./decision-evaluator.mjs";
import {
  decisionContractVersionV4,
  requiredLiveEvidenceGateIdsV4,
} from "./decision-contract-v4.mjs";
import { fullTimedRepresentativeScenarios } from "./run-paired.mjs";
import { representativeTimedScenarioIdsV4 } from "./scenario-contract-v4.mjs";

function passingEvidence() {
  return {
    contract_version: "bp-perf-v3-decision-3",
    execution_phase: "final",
    candidate_frozen: true,
    hard_gates: Object.fromEntries(requiredDecisionHardGates.map((gate) => [gate, true])),
    sampling: {
      calibration_pairs: 6,
      calibration_included_in_inference: false,
      planned_final_pairs: 24,
      completed_final_pairs: 24,
      calculation: { raw_pairs: 20, final_pairs: 24, clamped: "minimum" },
    },
    preflight: {
      gpui_shell_starts: { attempts: 30, failures: 0, maximum_native_presentation_ms: 4_999 },
      electron_zoom_promotions: { attempts: 10, failures: 0 },
      gpui_high_zoom_sequences: { attempts: 10, failures: 0 },
    },
    reliability: { app_cold_attempts: 100, failures: 0 },
    paired_comparison_summary: {
      complete: true,
      decision_ready: true,
      comparison_readiness: { ready: true },
      scenarios: Object.fromEntries(fullTimedRepresentativeScenarios.map((scenario) => [
        scenario,
        { valid_pair_count: 24 },
      ])),
    },
    primary_metrics: Object.fromEntries(
      Object.entries(requiredPrimaryMetrics).map(([metric, threshold]) => [
        metric,
        { upper_95: threshold - 0.01, absolute_budget_pass: true },
      ]),
    ),
  };
}

test("returns yes only when every predeclared decision gate passes", () => {
  const result = evaluateMigrationDecision(passingEvidence());

  assert.equal(result.decision, "yes");
  assert.equal(result.worth_migrating, true);
  assert.deepEqual(result.hard_gate_failures, []);
  assert.deepEqual(result.metric_failures, []);
  assert.equal(result.reliability.passed, true);
  assert.ok(result.reliability.upper_failure_rate_95 < 0.03);
});

test("returns no when a primary confidence bound exceeds its threshold", () => {
  const evidence = passingEvidence();
  evidence.primary_metrics.sustained_cpu_work.upper_95 = 0.81;

  const result = evaluateMigrationDecision(evidence);

  assert.equal(result.decision, "no");
  assert.equal(result.worth_migrating, false);
  assert.deepEqual(result.metric_failures, [
    "sustained_cpu_work upper_95 0.81 exceeds 0.8",
  ]);
});

test("returns no for incomplete parity, missing evidence, or any reliability failure", () => {
  const evidence = passingEvidence();
  evidence.hard_gates.representative_candidate_parity = false;
  delete evidence.primary_metrics.gpu_memory;
  evidence.reliability = { app_cold_attempts: 100, failures: 1 };

  const result = evaluateMigrationDecision(evidence);

  assert.equal(result.decision, "no");
  assert.deepEqual(result.hard_gate_failures, [
    "representative_candidate_parity did not pass",
    "reliability requires zero failures; got 1",
  ]);
  assert.deepEqual(result.metric_failures, ["gpu_memory is missing"]);
});

test("returns no when an absolute user-experience budget fails", () => {
  const evidence = passingEvidence();
  evidence.primary_metrics.product_latency.absolute_budget_pass = false;

  assert.deepEqual(evaluateMigrationDecision(evidence).metric_failures, [
    "product_latency failed an absolute budget",
  ]);
});

test("returns no for incomplete or underpowered final sampling", () => {
  const evidence = passingEvidence();
  evidence.sampling.planned_final_pairs = 40;
  evidence.sampling.completed_final_pairs = 39;
  evidence.sampling.calculation = { raw_pairs: 41, final_pairs: 40, clamped: "maximum" };

  assert.deepEqual(evaluateMigrationDecision(evidence).hard_gate_failures, [
    "completed final pairs 39 do not match planned final pairs 40",
    "calibration requires more than the maximum 40 final pairs",
  ]);
});

test("returns no when a preflight reliability proof is absent", () => {
  const evidence = passingEvidence();
  delete evidence.preflight;

  assert.deepEqual(evaluateMigrationDecision(evidence).hard_gate_failures, [
    "GPUI preflight requires 30 successful shell starts below 5000 ms",
    "Electron preflight requires 10 successful zoom quality promotions",
    "GPUI preflight requires 10 successful high-zoom tile sequences",
  ]);
});

test("returns no when the paired summary omits preflights or any representative timed scenario", () => {
  const evidence = passingEvidence();
  evidence.paired_comparison_summary.decision_ready = false;
  evidence.paired_comparison_summary.comparison_readiness.ready = false;
  delete evidence.paired_comparison_summary.scenarios["annotation-transform"];

  assert.deepEqual(evaluateMigrationDecision(evidence).hard_gate_failures, [
    "paired comparison summary did not pass full preflight and timing readiness",
    "paired comparison summary is missing timed scenario annotation-transform",
  ]);
});

function passingV4Evidence() {
  const digest = "a".repeat(64);
  const scenarios = Object.fromEntries(representativeTimedScenarioIdsV4.map((scenario) => [
    scenario,
    {
      expected_pair_count: 24,
      valid_pair_count: 24,
      metrics: { retained: { source: scenario } },
      pairs: Array.from({ length: 24 }, () => ({
        valid: true,
        runs: {
          electron: { candidate_artifact_sha256: digest },
          gpui: { candidate_artifact_sha256: digest },
        },
      })),
    },
  ]));
  const metricFamilies = Object.fromEntries(
    Object.entries(requiredPrimaryMetricsV4).map(([metric, threshold]) => [
      metric,
      {
        status: "complete",
        evidence_refs: ["#/scenarios/small-shell-open/metrics/retained"],
        paired_ratio: { upper_95: threshold - 0.01 },
        absolute_budget: { passed: true },
      },
    ]),
  );
  return {
    evidence_schema_version: decisionEvidenceSchemaVersion,
    contract_version: decisionContractVersionV4,
    execution_phase: "final",
    candidate_frozen: true,
    sampling: { calibration_pairs: 6, calibration_included_in_inference: false },
    paired_comparison_summary: {
      complete: true,
      decision_ready: true,
      comparison_readiness: { ready: true },
      corpora: { representative: { sha256: digest } },
      scenarios,
      analysis: {
        schema_version: decisionEvidenceSchemaVersion,
        contract_version: decisionContractVersionV4,
        eligibility: "decision-ready",
        complete: true,
        reliability: {
          electron: {
            attempts: 120,
            failures: 0,
            attempt_refs: Array(120).fill("#/scenarios/small-shell-open/pairs/0"),
          },
          gpui: {
            attempts: 120,
            failures: 0,
            attempt_refs: Array(120).fill("#/scenarios/small-shell-open/pairs/0"),
          },
        },
        journeys: {
          "small-shell-open-v1": {
            status: "complete",
            required_scenarios: ["small-shell-open"],
            evidence_refs: ["#/scenarios/small-shell-open"],
          },
          "nasa-long-document-v1": {
            status: "complete",
            required_scenarios: ["nasa-long-document"],
            evidence_refs: ["#/scenarios/nasa-long-document"],
          },
          "engineering-sheet-v1": {
            status: "complete",
            required_scenarios: ["engineering-sheet"],
            evidence_refs: ["#/scenarios/engineering-sheet"],
          },
          "dense-mixed-editing-v1": {
            status: "complete",
            required_scenarios: ["dense-mixed-editing"],
            evidence_refs: ["#/scenarios/dense-mixed-editing"],
          },
          "persistence-v1": {
            status: "complete",
            required_scenarios: ["persistence"],
            evidence_refs: ["#/scenarios/persistence"],
          },
        },
        metric_families: metricFamilies,
        hard_evidence_refs: Object.fromEntries(requiredLiveEvidenceGateIdsV4.map((gate) => [
          gate,
          gate === "candidate-artifacts-frozen"
            ? [
                "#/scenarios/small-shell-open/pairs/0/runs/electron/candidate_artifact_sha256",
                "#/scenarios/small-shell-open/pairs/0/runs/gpui/candidate_artifact_sha256",
              ]
            : gate === "fixture-bundle-verified"
              ? ["#/corpora/representative/sha256"]
              : ["#/scenarios/small-shell-open"],
        ])),
      },
    },
  };
}

test("v4 classifies incomplete evidence as not decision ready instead of a technical no", () => {
  const evidence = passingV4Evidence();
  evidence.paired_comparison_summary.complete = false;
  evidence.paired_comparison_summary.decision_ready = false;
  evidence.paired_comparison_summary.analysis.complete = false;
  evidence.paired_comparison_summary.analysis.eligibility = "not-decision-ready";

  const result = evaluateMigrationDecision(evidence);

  assert.equal(result.eligibility, "not-decision-ready");
  assert.equal(result.decision, "not-decision-ready");
  assert.equal(result.worth_migrating, null);
  assert.deepEqual(result.metric_failures, []);
  assert(result.eligibility_failures.includes(
    "paired comparison summary did not pass full preflight and timing readiness",
  ));
});

test("v4 reaches yes from retained analysis and tracks reliability per implementation", () => {
  const evidence = passingV4Evidence();
  evidence.hard_gates = Object.fromEntries(requiredDecisionHardGates.map((gate) => [gate, false]));
  evidence.primary_metrics = Object.fromEntries(
    Object.keys(requiredPrimaryMetrics).map((metric) => [metric, { upper_95: 99 }]),
  );

  const result = evaluateMigrationDecision(evidence);

  assert.equal(result.eligibility, "decision-ready");
  assert.equal(result.decision, "yes");
  assert.equal(result.worth_migrating, true);
  assert.equal(result.reliability.electron.attempts, 120);
  assert.equal(result.reliability.gpui.attempts, 120);
  assert.equal(result.reliability.electron.passed, true);
  assert.equal(result.reliability.gpui.passed, true);
});

test("v4 returns a technical no only after eligible derived metrics exceed the threshold", () => {
  const evidence = passingV4Evidence();
  evidence.paired_comparison_summary.analysis.metric_families.sustained_cpu_work
    .paired_ratio.upper_95 = 0.81;

  const result = evaluateMigrationDecision(evidence);

  assert.equal(result.eligibility, "decision-ready");
  assert.equal(result.decision, "no");
  assert.equal(result.worth_migrating, false);
  assert.deepEqual(result.metric_failures, [
    "sustained_cpu_work upper_95 0.81 exceeds 0.8",
  ]);
});

test("v4 fails closed when a retained evidence reference is invalid", () => {
  const evidence = passingV4Evidence();
  evidence.paired_comparison_summary.analysis.metric_families.gpu_resource_pressure.evidence_refs = [
    "#/scenarios/small-shell-open/metrics/not-recorded",
  ];

  const result = evaluateMigrationDecision(evidence);

  assert.equal(result.decision, "not-decision-ready");
  assert(result.eligibility_failures.includes(
    "gpu_resource_pressure contains an invalid retained metric reference",
  ));
});
