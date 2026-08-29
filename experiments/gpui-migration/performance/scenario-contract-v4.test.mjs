import assert from "node:assert/strict";
import test from "node:test";

import { loadComparisonWorkloadV4 } from "./comparison-workload-v4.mjs";
import {
  assessScenarioExecutionV4,
  buildScenarioContractV4,
  engineeringZoomSequenceV4,
  normalizedPageSequenceV4,
  representativeScenarioDefinitionsV4,
  representativeScenarioBlockersV4,
  representativeTimedScenarioIdsV4,
  scenarioContractVersionV4,
  stressScenarioDefinitionsV4,
  validateRepresentativeScenarioDefinitionsV4,
} from "./scenario-contract-v4.mjs";

const digest = "c".repeat(64);

test("exports exactly five machine-readable representative timed scenarios", () => {
  assert.deepEqual(representativeTimedScenarioIdsV4, [
    "small-shell-open",
    "nasa-long-document",
    "engineering-sheet",
    "dense-mixed-editing",
    "persistence",
  ]);
  assert.deepEqual(Object.keys(representativeScenarioDefinitionsV4), representativeTimedScenarioIdsV4);
  assert.deepEqual(validateRepresentativeScenarioDefinitionsV4(), []);
  for (const definition of Object.values(representativeScenarioDefinitionsV4)) {
    assert.equal(definition.inference_eligible, true);
    assert.notEqual(definition.fixture_id, "usgs-usa-geology-sheet-v1");
    assert.equal(definition.component_weights.length, definition.current_runner_components.length);
    assert(Math.abs(definition.component_weights.reduce((sum, weight) => sum + weight, 0) - 1) < 1e-12);
  }
  assert.deepEqual(representativeScenarioBlockersV4["engineering-sheet"], []);
});

test("maps the five representative scenarios to exact materialized commands", async () => {
  const workload = await loadComparisonWorkloadV4();
  for (const scenario of representativeTimedScenarioIdsV4) {
    const contract = buildScenarioContractV4(workload, scenario);
    const definition = representativeScenarioDefinitionsV4[scenario];
    assert.equal(contract.scenario_contract_version, scenarioContractVersionV4);
    assert.equal(contract.lane, "representative-inference");
    assert.equal(contract.fixture_id, definition.fixture_id);
    assert.deepEqual(contract.command_ids, definition.command_ids);
    assert.deepEqual(contract.commands.map(({ id }) => id), definition.command_ids);
    assert.equal(contract.execution_eligible, false);
    assert.deepEqual(contract.component_aggregation.order, contract.current_runner_components);
    assert.deepEqual(contract.component_aggregation.weights, definition.component_weights);
    assert.equal(contract.component_aggregation.compensating_regressions_allowed, false);
    assert.deepEqual(contract.component_command_ids, definition.component_command_ids);
  }
});

test("maps every engineering component only to the generated moderate sheet", async () => {
  const workload = await loadComparisonWorkloadV4();
  const contract = buildScenarioContractV4(workload, "engineering-sheet");
  assert.equal(contract.fixture_id, "bp-engineering-sheet-v1");
  assert.deepEqual(
    contract.commands.find(({ id }) => id === "engineering:zoom-sequence").percent,
    engineeringZoomSequenceV4,
  );
  assert.equal(contract.commands.find(({ id }) => id === "engineering:pan").zoom_percent, 1600);
  assert.deepEqual(contract.current_runner_components, [
    "open-pdf", "fit-modes", "zoom", "high-zoom-pan", "cache-pressure-recovery",
  ]);
  assert.deepEqual(contract.component_weights, [0.2, 0.2, 0.2, 0.2, 0.2]);
  assert.deepEqual(contract.component_command_ids, {
    "open-pdf": ["engineering:open-settle"],
    "fit-modes": ["engineering:fit-modes"],
    zoom: ["engineering:zoom-sequence"],
    "high-zoom-pan": ["engineering:pan"],
    "cache-pressure-recovery": ["engineering:cache-recovery"],
  });
  assert.deepEqual(contract.blocked_commands, []);
});

test("keeps USGS in a separate non-inferential stress mapping", async () => {
  const workload = await loadComparisonWorkloadV4();
  const contract = buildScenarioContractV4(workload, "usgs-large-sheet-stress");
  assert.equal(contract.lane, "stress-diagnostic");
  assert.equal(contract.inference_eligible, false);
  assert.equal(contract.fixture_id, "usgs-usa-geology-sheet-v1");
  assert.equal(stressScenarioDefinitionsV4["usgs-large-sheet-stress"].inference_eligible, false);
});

test("retains normalized NASA navigation and the fixed engineering zoom sequence", () => {
  assert.deepEqual(
    normalizedPageSequenceV4(526),
    [526, 43, 379, 132, 474, 263, 11, 504, 174, 1],
  );
  assert.deepEqual(engineeringZoomSequenceV4, [
    100, 200, 400, 800, 1600, 400, 100, 800, 200, 100, 1200, 100,
  ]);
});

test("fails closed when a mapped command has no materialized live implementation", async () => {
  const workload = structuredClone(await loadComparisonWorkloadV4());
  const engineering = workload.journeys.find(({ id }) => id === "engineering-sheet-v1");
  engineering.commands = engineering.commands.filter(({ id }) => id !== "engineering:pan");
  assert.throws(
    () => buildScenarioContractV4(workload, "engineering-sheet"),
    /missing command engineering:pan/,
  );
});

test("scenario execution becomes eligible only with exact live receipts from both implementations", async () => {
  const workload = await loadComparisonWorkloadV4();
  const contract = buildScenarioContractV4(workload, "small-shell-open");
  const evidence = {
    implementations: Object.fromEntries(["electron", "gpui"].map((implementation) => [
      implementation,
      {
        candidate_artifact_sha256: digest,
        command_receipts: contract.command_ids.map((commandId) => ({
          command_id: commandId,
          live: true,
          passed: true,
          evidence_sha256: digest,
        })),
      },
    ])),
  };
  assert.equal(assessScenarioExecutionV4(workload, "small-shell-open", {}).execution_eligible, false);
  assert.equal(
    assessScenarioExecutionV4(workload, "small-shell-open", evidence).execution_eligible,
    true,
  );
  evidence.implementations.electron.command_receipts[0].passed = false;
  assert.equal(
    assessScenarioExecutionV4(workload, "small-shell-open", evidence).execution_eligible,
    false,
  );
});

test("engineering execution becomes eligible only when every live receipt passes", async () => {
  const workload = await loadComparisonWorkloadV4();
  const contract = buildScenarioContractV4(workload, "engineering-sheet");
  const evidence = {
    implementations: Object.fromEntries(["electron", "gpui"].map((implementation) => [
      implementation,
      {
        candidate_artifact_sha256: digest,
        command_receipts: contract.command_ids.map((commandId) => ({
          command_id: commandId,
          live: true,
          passed: true,
          evidence_sha256: digest,
        })),
      },
    ])),
  };
  const assessed = assessScenarioExecutionV4(workload, "engineering-sheet", evidence);
  assert.equal(assessed.execution_eligible, true);
  assert.deepEqual(assessed.execution_blockers, []);
});
