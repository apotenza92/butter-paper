import assert from "node:assert/strict";
import test from "node:test";

import {
  buildDevelopmentScenarioContract,
  normalizedPageSequence,
  zoomSequence,
} from "./scenario-contract.mjs";
import { loadComparisonWorkload } from "./comparison-workload.mjs";

test("normalizes the navigation sequence for the public NASA document", () => {
  assert.deepEqual(
    normalizedPageSequence(526),
    [526, 43, 379, 132, 474, 263, 11, 504, 174, 1],
  );
});

test("clamps and removes duplicates for the generated six-page fixture", () => {
  assert.deepEqual(normalizedPageSequence(6), [6, 1, 5, 2, 3]);
});

test("keeps the fixed zoom sequence", () => {
  assert.deepEqual(
    zoomSequence,
    [100, 200, 400, 800, 1600, 400, 100, 800, 200, 100, 1200, 100],
  );
});

test("derives annotation-create from the frozen comparison workload", async () => {
  const contract = buildDevelopmentScenarioContract(
    await loadComparisonWorkload(),
    "annotation-create",
  );

  assert.equal(contract.fixture_id, "bp-annotation-density-v1");
  assert.equal(contract.input_lane, "semantic-diagnostic");
  assert.equal(
    contract.fixture_sha256,
    "1dad337b97d573ff971e886eb6509e80e5e1cd07f6ac610ae4f2f9419c666682",
  );
  assert.deepEqual(contract.command_ids, [
    "rectangle:create-sparse",
    "highlight:create",
  ]);
  assert.equal(contract.commands[0].pointer_path.expected_sample_count, 361);
  assert.equal(contract.commands[1].pointer_path.interpolation, "catmull-rom-inclusive");
});

test("derives continuous-scroll from the frozen comparison workload", async () => {
  const contract = buildDevelopmentScenarioContract(
    await loadComparisonWorkload(),
    "continuous-scroll",
  );

  assert.equal(contract.fixture_id, "nasa-apollo-summary-526-v1");
  assert.equal(contract.input_lane, "semantic-diagnostic");
  assert.equal(
    contract.fixture_sha256,
    "68d0f3bb93fd1c4e4f3adf99d483e9161f14293a311e25aa6c31241bbbb84049",
  );
  assert.deepEqual(contract.command_ids, ["viewer:continuous-scroll"]);
  assert.equal(contract.commands[0].path.forward_viewport_heights, 50);
});

test("derives the focused editor-create lane without changing the frozen commands", async () => {
  const contract = buildDevelopmentScenarioContract(
    await loadComparisonWorkload(),
    "editor-create",
  );

  assert.equal(contract.fixture_id, "bp-annotation-density-v1");
  assert.equal(contract.semantic_command_only, false);
  assert.deepEqual(contract.command_ids, [
    "text:create",
    "length:set-scale",
    "length:create",
    "image:create",
  ]);
});

test("derives the focused exact rectangle transform lane", async () => {
  const contract = buildDevelopmentScenarioContract(
    await loadComparisonWorkload(),
    "annotation-transform",
  );
  assert.equal(contract.fixture_id, "bp-annotation-density-v1");
  assert.equal(contract.require_exact_fields, true);
  assert.deepEqual(contract.command_ids, ["rectangle:select-move-resize"]);
  assert.deepEqual(contract.commands[0].expected_milestones, [
    "hit-test-selected",
    "move-committed-once",
    "resize-committed-once",
  ]);
});

test("derives the focused exact rectangle properties and history lane", async () => {
  const contract = buildDevelopmentScenarioContract(
    await loadComparisonWorkload(),
    "annotation-properties-history",
  );
  assert.equal(contract.fixture_id, "bp-annotation-density-v1");
  assert.equal(contract.require_exact_fields, true);
  assert.deepEqual(contract.command_ids, ["rectangle:properties-history"]);
  assert.deepEqual(contract.commands[0].expected_milestones, [
    "properties-current",
    "locked-edit-rejected",
    "undo-redo-exact",
    "dirty-current",
  ]);
});

test("derives every viewer diagnostic from one exact manifest command", async () => {
  const workload = await loadComparisonWorkload();
  const expected = [
    ["viewer-layout", "bp-multi-page-v1", ["viewer:layout-single", "viewer:layout-continuous"]],
    ["page-navigation", "nasa-apollo-summary-526-v1", ["viewer:navigate-normalized"]],
    ["zoom", "usgs-usa-geology-sheet-v1", ["viewer:zoom-sequence"]],
    ["high-zoom-pan", "usgs-usa-geology-sheet-v1", ["viewer:pan-usgs"]],
    ["cache-pressure", "bp-multi-page-v1", ["viewer:cache-pressure"]],
    ["close-reopen", "bp-multi-page-v1", ["viewer:close-recover-reopen"]],
  ];

  for (const [scenario, fixtureId, commandIds] of expected) {
    const contract = buildDevelopmentScenarioContract(workload, scenario);
    assert.equal(contract.fixture_id, fixtureId);
    assert.deepEqual(contract.command_ids, commandIds);
    assert.deepEqual(contract.commands.map(({ id }) => id), commandIds);
  }

  assert.equal(
    buildDevelopmentScenarioContract(workload, "viewer-layout").fixture_sha256,
    "517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b",
  );
  assert.equal(
    buildDevelopmentScenarioContract(workload, "zoom").fixture_sha256,
    "f058179e193ccbc15ca662feff3554102f64ff2114436a5ce6116d6fa5d2a6e2",
  );
  assert.equal(
    buildDevelopmentScenarioContract(workload, "cache-pressure").fixture_sha256,
    "517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b",
  );
});

test("rejects a missing exact viewer command rather than silently using generic timing", async () => {
  const workload = structuredClone(await loadComparisonWorkload());
  const viewer = workload.journeys.find(({ id }) => id === "viewer-v1");
  viewer.commands = viewer.commands.filter(({ id }) => id !== "viewer:pan-usgs");

  assert.throws(
    () => buildDevelopmentScenarioContract(workload, "high-zoom-pan"),
    /missing comparison command viewer:pan-usgs/,
  );
});

test("freezes the requested native lane without changing the manifest commands", async () => {
  const contract = buildDevelopmentScenarioContract(
    await loadComparisonWorkload(),
    "annotation-create",
    "native-x11-xtest",
  );

  assert.equal(contract.input_lane, "native-x11-xtest");
  assert.deepEqual(contract.command_ids, ["rectangle:create-sparse", "highlight:create"]);
  assert.throws(
    () => buildDevelopmentScenarioContract({}, "annotation-create", "browser-evaluate"),
    /unsupported input lane/,
  );
});

test("rejects a scenario when its source command is missing", async () => {
  const workload = structuredClone(await loadComparisonWorkload());
  const viewer = workload.journeys.find(({ id }) => id === "viewer-v1");
  viewer.commands = viewer.commands.filter(({ id }) => id !== "viewer:continuous-scroll");

  assert.throws(
    () => buildDevelopmentScenarioContract(workload, "continuous-scroll"),
    /missing comparison command viewer:continuous-scroll/,
  );
});
