import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  assessDecisionExecutionV4,
  decisionContractV4,
  decisionContractVersionV4,
  representativeJourneyIdsV4,
  requiredLiveEvidenceGateIdsV4,
  validateDecisionContractV4,
} from "./decision-contract-v4.mjs";

const digest = "a".repeat(64);

function passingLiveEvidence() {
  return {
    contract_version: decisionContractVersionV4,
    implementations: Object.fromEntries(["electron", "gpui"].map((implementation) => [
      implementation,
      {
        candidate_artifact_sha256: digest,
        journeys: Object.fromEntries(representativeJourneyIdsV4.map((journeyId) => [
          journeyId,
          { live: true, passed: true, evidence_sha256: digest },
        ])),
      },
    ])),
    live_gates: Object.fromEntries(requiredLiveEvidenceGateIdsV4.map((gateId) => [
      gateId,
      { live: true, passed: true, evidence_sha256: digest },
    ])),
  };
}

test("declares five representative journeys and keeps private or pathological lanes non-inferential", () => {
  assert.deepEqual(validateDecisionContractV4(decisionContractV4), []);
  assert.deepEqual(decisionContractV4.journeys.map(({ id }) => id), representativeJourneyIdsV4);
  assert.equal(decisionContractV4.fixtures.some(({ id }) => id.includes("usgs")), false);
  assert.equal(decisionContractV4.stress_lanes[0].inference_eligible, false);
  assert.equal(decisionContractV4.supplementary_lanes[0].status, "blocked-not-transferred");
  assert.deepEqual(decisionContractV4.resource_observation, {
    decoded_payload_bytes: "exact-required",
    renderer_resource_submission_bytes: "exact-required",
    physical_bus_upload_bytes: "optional-nullable",
    whole_device_gpu_samples: "baseline-adjusted-diagnostic",
  });
  assert.equal(
    decisionContractV4.statistics.sampling_unit,
    "paired isolated journey execution bundle",
  );
  assert.equal(
    decisionContractV4.statistics.component_aggregation.compensating_regressions_allowed,
    false,
  );
  assert.match(
    decisionContractV4.statistics.component_aggregation.benefit_metric_method,
    /equal-weight geometric mean/,
  );
});

test("execution remains blocked when only declarations exist", () => {
  const result = assessDecisionExecutionV4({
    contract_version: decisionContractVersionV4,
    declared_capabilities: { electron: "complete", gpui: "complete" },
  });
  assert.equal(result.contract_version, decisionContractVersionV4);
  assert.equal(result.executable, false);
  assert.equal(result.status, "blocked-live-evidence");
  assert(result.blockers.length > requiredLiveEvidenceGateIdsV4.length);
});

test("execution becomes ready only after every live evidence receipt passes", () => {
  const evidence = passingLiveEvidence();
  assert.deepEqual(assessDecisionExecutionV4(evidence), {
    contract_version: decisionContractVersionV4,
    executable: true,
    status: "ready-final-execution",
    blockers: [],
  });

  evidence.implementations.gpui.journeys["engineering-sheet-v1"].passed = false;
  const blocked = assessDecisionExecutionV4(evidence);
  assert.equal(blocked.executable, false);
  assert(blocked.blockers.includes(
    "gpui:engineering-sheet-v1: live journey evidence did not pass",
  ));
});

test("publishes a v4 schema with live-derived execution and nullable physical upload evidence", async () => {
  const schema = JSON.parse(
    await readFile(new URL("./decision-contract-v4.schema.json", import.meta.url), "utf8"),
  );
  assert.equal(schema.properties.contract_version.const, decisionContractVersionV4);
  assert.equal(
    schema.properties.execution.properties.readiness_model.const,
    "derived-from-live-evidence",
  );
  assert.equal(
    schema.properties.resource_observation.properties.physical_bus_upload_bytes.const,
    "optional-nullable",
  );
});
