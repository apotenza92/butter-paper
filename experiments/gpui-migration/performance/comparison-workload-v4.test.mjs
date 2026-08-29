import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  assessLiveCommandReceiptsV4,
  buildDeclaredCapabilityReportV4,
  comparisonWorkloadArtifactHashV4,
  comparisonWorkloadHashesV4,
  loadComparisonWorkloadV4,
  loadMaterializedComparisonWorkloadV4,
  validateComparisonWorkloadV4,
} from "./comparison-workload-v4.mjs";
import {
  comparisonWorkloadArtifactHash as v3ArtifactHash,
  loadComparisonWorkload as loadV3Workload,
} from "./comparison-workload.mjs";
import { representativeJourneyIdsV4 } from "./decision-contract-v4.mjs";

const digest = "b".repeat(64);

function representativeCommands(workload) {
  return workload.journeys.flatMap(({ commands }) => commands);
}

test("materializes five representative v4 journeys without changing frozen v3", async () => {
  const before = await loadV3Workload();
  const beforeHash = v3ArtifactHash(before);
  const workload = await loadComparisonWorkloadV4();
  const afterHash = v3ArtifactHash(await loadV3Workload());

  assert.deepEqual(validateComparisonWorkloadV4(workload), []);
  assert.equal(beforeHash, "1926113f60f434b383aed89c34f157197f0b5680a4ad71d14f25c539f14cbd2f");
  assert.equal(afterHash, beforeHash);
  const continuousScroll = representativeCommands(workload).find(
    ({ id }) => id === "viewer:continuous-scroll",
  );
  assert(continuousScroll.expected_milestones.includes("visible-raster-readiness-observed"));
  assert.equal(
    continuousScroll.expected_milestones.includes("blank-current-generation-frames-zero"),
    false,
  );
  assert.equal(
    workload.expected.milestone_rules.visible_raster_readiness_observations_minimum,
    1,
  );
  assert.equal(
    Object.hasOwn(workload.expected.milestone_rules, "blank_current_generation_frames"),
    false,
  );
  assert.deepEqual(workload.journeys.map(({ id }) => id), representativeJourneyIdsV4);
  assert.deepEqual(comparisonWorkloadHashesV4(workload), {
    representative_command_stream_sha256: "e73410f5e765268527df99b26cbee6c72557ca7f584582b5358320eb0731b3b5",
    representative_milestone_stream_sha256: "f0079a662d7902f8bf833db6a1475b1a9e305a9d2f0b0c3854c1b51046a83a30",
    stress_command_stream_sha256: "a7e63ced8f1379758d42c56f3a608100270f0131393f65ca7841bd9e3fb0f3e6",
    expected_state_sha256: "8868578316a2e5fb59fec8f82396fd5793dfd305bf6ca516ea5db3532084c465",
  });
  assert.equal(
    comparisonWorkloadArtifactHashV4(workload),
    "4a826bd3c19f3c7693128961f43064e5b7414e799a0f791b7f3381ed59e897b1",
  );
});

test("checked-in materialized workload is byte-independent but structurally and cryptographically identical", async () => {
  const fromDescriptor = await loadComparisonWorkloadV4();
  const checkedIn = await loadMaterializedComparisonWorkloadV4();

  assert.deepEqual(checkedIn, fromDescriptor);
  assert.deepEqual(comparisonWorkloadHashesV4(checkedIn), comparisonWorkloadHashesV4(fromDescriptor));
  assert.equal(
    comparisonWorkloadArtifactHashV4(checkedIn),
    comparisonWorkloadArtifactHashV4(fromDescriptor),
  );
  assert.deepEqual(validateComparisonWorkloadV4(checkedIn), []);
});

test("keeps USGS stress and Hibbeler supplementary evidence outside inference", async () => {
  const workload = await loadComparisonWorkloadV4();
  assert.equal(workload.fixtures.some(({ id }) => id === "usgs-usa-geology-sheet-v1"), false);
  assert.equal(
    workload.journeys.some(({ fixtures: fixtureIds }) =>
      fixtureIds.includes("usgs-usa-geology-sheet-v1")),
    false,
  );
  assert.equal(workload.stress_lanes[0].fixture_id, "usgs-usa-geology-sheet-v1");
  assert.equal(workload.stress_lanes[0].inference_eligible, false);
  assert.deepEqual(workload.supplementary_lanes[0], {
    id: "private-hibbeler-935-v1",
    status: "blocked-not-transferred",
    inference_eligible: false,
    allowed_lane: "owner-authorized-local-macos",
  });
});

test("uses symmetric decoded, presentation, and renderer-submission image evidence", async () => {
  const workload = await loadComparisonWorkloadV4();
  const commands = new Map(representativeCommands(workload).map((command) => [command.id, command]));
  const create = commands.get("image:create");
  const resize = commands.get("image:resize-history");

  assert.deepEqual(create.resource_observation, {
    decoded_payload_bytes: 786_432,
    renderer_resource_submission_bytes: 786_432,
    physical_bus_upload_bytes: null,
  });
  for (const command of [create, resize]) {
    assert(command.expected_milestones.includes("renderer-resource-submission-bytes-exact"));
    assert(command.expected_milestones.includes("bitmap-presented-from-decoded-payload"));
    assert.equal(command.expected_milestones.includes("bitmap-upload-recorded"), false);
    assert.equal(command.expected_milestones.includes("upload-byte-count-recorded"), false);
  }
  assert(create.expected_milestones.includes("decoded-payload-bytes-exact"));
});

test("static declarations never make v4 execution eligible", async () => {
  const workload = await loadComparisonWorkloadV4();
  const commandIds = representativeCommands(workload).map(({ id }) => id);
  const declaration = buildDeclaredCapabilityReportV4(workload, {
    implementation: "gpui",
    command_ids: commandIds,
  });
  assert.equal(declaration.missing_command_ids.length, 0);
  assert.equal(declaration.execution_eligible, false);
  assert.equal(declaration.evidence_class, "planning-declaration-only");
});

test("live command coverage passes only with exact retained receipts", async () => {
  const workload = await loadComparisonWorkloadV4();
  const receipts = representativeCommands(workload).map(({ id }) => ({
    command_id: id,
    live: true,
    passed: true,
    evidence_sha256: digest,
  }));
  const passed = assessLiveCommandReceiptsV4(workload, "gpui", receipts);
  assert.equal(passed.ready, true);
  assert.equal(passed.expected_command_count, receipts.length);

  receipts.find(({ command_id: commandId }) => commandId === "engineering:pan").passed = false;
  const blocked = assessLiveCommandReceiptsV4(workload, "gpui", receipts);
  assert.equal(blocked.ready, false);
  assert(blocked.failures.includes("gpui:engineering:pan: live command receipt did not pass"));
});

test("publishes a v4 workload schema that separates representative and stress lanes", async () => {
  const schema = JSON.parse(
    await readFile(new URL("./comparison-workload-v4.schema.json", import.meta.url), "utf8"),
  );
  assert.equal(schema.properties.manifest_id.const, "bp-perf-v4-decision-1");
  assert.equal(schema.properties.representative_journeys.minItems, 5);
  assert.equal(schema.properties.representative_journeys.maxItems, 5);
  assert.equal(schema.properties.stress_lanes.minItems, 1);
});
