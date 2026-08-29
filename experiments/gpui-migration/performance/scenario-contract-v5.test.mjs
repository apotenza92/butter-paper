import assert from "node:assert/strict";
import test from "node:test";

import { loadComparisonWorkloadV5 } from "./comparison-workload-v5.mjs";
import { hardComponentIdsV5 } from "./decision-contract-v5.mjs";
import {
  assessScenarioExecutionV5,
  buildScenarioContractV5,
  representativeScenarioDefinitionsV5,
  representativeTimedScenarioIdsV5,
  scenarioContractVersionV5,
  validateRepresentativeScenarioDefinitionsV5,
} from "./scenario-contract-v5.mjs";

const digest = "c".repeat(64);

test("maps six representative scenarios and all four hard components exactly once", () => {
  assert.deepEqual(representativeTimedScenarioIdsV5, [
    "small-shell-open",
    "nasa-long-document",
    "engineering-sheet",
    "dense-mixed-editing",
    "persistence",
    "multi-document-session",
  ]);
  assert.deepEqual(validateRepresentativeScenarioDefinitionsV5(), []);
  const mappedHardComponents = Object.values(
    representativeScenarioDefinitionsV5,
  )
    .flatMap(({ current_runner_components: components }) => components)
    .filter((component) => hardComponentIdsV5.includes(component));
  assert.deepEqual(mappedHardComponents.sort(), [...hardComponentIdsV5].sort());
});

test("adds dynamic fidelity and native editing as conjunctive components", async () => {
  const workload = await loadComparisonWorkloadV5();
  const nasa = buildScenarioContractV5(workload, "nasa-long-document");
  assert.equal(nasa.scenario_contract_version, scenarioContractVersionV5);
  assert.equal(
    nasa.current_runner_components.at(-1),
    "viewer-dynamic-fidelity",
  );
  assert.deepEqual(nasa.component_command_ids["viewer-dynamic-fidelity"], [
    "viewer:dynamic-fidelity-scroll",
  ]);
  assert.deepEqual(nasa.component_fixture_ids["viewer-dynamic-fidelity"], [
    "nasa-apollo-summary-526-v1",
  ]);

  const dense = buildScenarioContractV5(workload, "dense-mixed-editing");
  assert.deepEqual(dense.current_runner_components.slice(-2), [
    "native-property-edit-undo",
    "native-snap-transform-120hz",
  ]);
  assert.deepEqual(dense.hard_components, [
    "native-property-edit-undo",
    "native-snap-transform-120hz",
  ]);
  assert.equal(
    dense.component_benefit_metrics_eligible["native-property-edit-undo"],
    false,
  );
  assert.equal(
    dense.component_benefit_metrics_eligible["native-snap-transform-120hz"],
    true,
  );
  assert.equal(
    dense.component_aggregation.compensating_regressions_allowed,
    false,
  );
});

test("maps one multi-document component to four commands and four fixtures", async () => {
  const workload = await loadComparisonWorkloadV5();
  const contract = buildScenarioContractV5(workload, "multi-document-session");
  assert.equal(contract.fixture_id, null);
  assert.deepEqual(contract.fixture_ids, [
    "bp-single-page-v1",
    "nasa-apollo-summary-526-v1",
    "bp-engineering-sheet-v1",
    "bp-annotation-density-v1",
  ]);
  assert.deepEqual(contract.current_runner_components, [
    "multi-document-session",
  ]);
  assert.deepEqual(contract.component_command_ids, {
    "multi-document-session": [
      "session:open-four-fixtures",
      "session:switch-four-fixtures",
      "session:edit-dense-rectangle",
      "session:close-three-and-recover",
    ],
  });
  assert.deepEqual(
    contract.component_fixture_ids["multi-document-session"],
    contract.fixture_ids,
  );
  assert.equal(contract.execution_eligible, false);
});

test("becomes eligible only after exact live receipts from both implementations", async () => {
  const workload = await loadComparisonWorkloadV5();
  const contract = buildScenarioContractV5(workload, "multi-document-session");
  const evidence = {
    implementations: Object.fromEntries(
      ["electron", "gpui"].map((implementation) => [
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
      ]),
    ),
  };
  assert.equal(
    assessScenarioExecutionV5(workload, "multi-document-session", {})
      .execution_eligible,
    false,
  );
  assert.equal(
    assessScenarioExecutionV5(workload, "multi-document-session", evidence)
      .execution_eligible,
    true,
  );
  evidence.implementations.electron.command_receipts[1].passed = false;
  assert.equal(
    assessScenarioExecutionV5(workload, "multi-document-session", evidence)
      .execution_eligible,
    false,
  );
});
