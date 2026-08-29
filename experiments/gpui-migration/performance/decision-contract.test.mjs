import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  decisionContract,
  decisionContractVersion,
  validateDecisionContract,
} from "./decision-contract.mjs";

test("declares the blocked v3 decision evidence contract", () => {
  assert.equal(decisionContractVersion, "bp-perf-v3-decision-3");
  assert.deepEqual(validateDecisionContract(decisionContract), []);
  assert.equal(decisionContract.execution.status, "blocked-candidate-parity");
  assert.equal(decisionContract.execution.executable, false);
  assert.equal(decisionContract.phases.calibration.include_in_inference, false);
  assert.deepEqual(
    decisionContract.fixtures.map(({ id }) => id),
    [
      "bp-single-page-v1",
      "bp-multi-page-v1",
      "bp-annotation-density-v1",
      "nasa-apollo-summary-526-v1",
      "usgs-usa-geology-sheet-v1",
      "bp-annotation-all-v1",
    ],
  );
  assert.deepEqual(
    decisionContract.journeys.map(({ id }) => id),
    ["viewer-journey-v1", "annotation-journey-v1", "persistence-journey-v1"],
  );
});

test("rejects an incomplete decision contract", () => {
  const incomplete = structuredClone(decisionContract);
  incomplete.fixtures.pop();
  incomplete.journeys[1].required_capabilities = [];

  assert.deepEqual(validateDecisionContract(incomplete), [
    "missing required fixture bp-annotation-all-v1",
    "annotation-journey-v1 must declare required capabilities",
  ]);
});

test("publishes a JSON Schema pinned to the decision contract version", async () => {
  const schema = JSON.parse(
    await readFile(new URL("./decision-contract.schema.json", import.meta.url), "utf8"),
  );

  assert.equal(schema.$schema, "https://json-schema.org/draft/2020-12/schema");
  assert.equal(schema.properties.contract_version.const, decisionContractVersion);
  assert.equal(schema.properties.execution.properties.executable.const, false);
});
