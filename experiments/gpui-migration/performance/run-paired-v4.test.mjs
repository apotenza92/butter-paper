import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import test from "node:test";

import { loadMaterializedComparisonWorkloadV4 } from "./comparison-workload-v4.mjs";
import {
  buildScenarioContractV4,
  representativeScenarioDefinitionsV4,
} from "./scenario-contract-v4.mjs";
import {
  buildBundleManifestV4,
  buildV4ComparisonPlan,
  buildV4ExecutionSchedule,
  calibrationPairCountV4,
  canonicalSha256,
  commandReceiptsFromReportV4,
  componentInputLaneV4,
  createCalibrationPairOrdersV4,
  parseV4Arguments,
  runnerInputLaneV4,
  validateV4ComponentReport,
  verifyV4Fixtures,
} from "./run-paired-v4.mjs";

function readyDefinitions() {
  return {
    ...representativeScenarioDefinitionsV4,
    "engineering-sheet": {
      ...representativeScenarioDefinitionsV4["engineering-sheet"],
      current_runner_components: [
        "open-pdf",
        "fit-modes",
        "zoom",
        "high-zoom-pan",
        "cache-pressure-recovery",
      ],
      component_command_ids: {
        "open-pdf": ["engineering:open-settle"],
        "fit-modes": ["engineering:fit-modes"],
        zoom: ["engineering:zoom-sequence"],
        "high-zoom-pan": ["engineering:pan"],
        "cache-pressure-recovery": ["engineering:cache-recovery"],
      },
      blocked_commands: [],
      component_weights: [0.2, 0.2, 0.2, 0.2, 0.2],
    },
  };
}

function validArguments(overrides = []) {
  return [
    "--output",
    "/tmp/bp-v4-output",
    "--electron",
    "/tmp/electron",
    "--gpui-binary",
    "/tmp/gpui",
    "--electron-candidate-artifact",
    "/tmp/electron-candidate",
    "--gpui-candidate-artifact",
    "/tmp/gpui-candidate",
    ...overrides,
  ];
}

test("freezes six excluded calibration pairs and accepts only 24-40 final block counts", () => {
  const options = parseV4Arguments(validArguments(["--final-pairs", "32"]));
  assert.equal(options.calibrationPairs, calibrationPairCountV4);
  assert.equal(options.finalPairs, 32);
  assert.throws(
    () => parseV4Arguments(validArguments(["--calibration-pairs", "8"])),
    /frozen at 6/,
  );
  for (const count of [20, 25, 44]) {
    assert.throws(
      () => parseV4Arguments(validArguments(["--final-pairs", String(count)])),
      /multiple of 4 from 24 through 40/,
    );
  }
});

test("requires independently hashable candidate artifacts and explicit executables", () => {
  assert.throws(
    () =>
      parseV4Arguments([
        "--output",
        "/tmp/out",
        "--electron",
        "/tmp/electron",
        "--gpui-binary",
        "/tmp/gpui",
      ]),
    /electron-candidate-artifact is required/,
  );
  assert.throws(
    () =>
      parseV4Arguments([
        "--output",
        "/tmp/out",
        "--gpui-binary",
        "/tmp/gpui",
        "--electron-candidate-artifact",
        "/tmp/electron-candidate",
        "--gpui-candidate-artifact",
        "/tmp/gpui-candidate",
      ]),
    /--electron is required/,
  );
});

test("calibration order is deterministic and balanced three-three", () => {
  const orders = createCalibrationPairOrdersV4(91);
  assert.deepEqual(orders, createCalibrationPairOrdersV4(91));
  assert.equal(orders.length, 6);
  assert.equal(orders.filter(([first]) => first === "electron").length, 3);
  assert.equal(orders.filter(([first]) => first === "gpui").length, 3);
});

test("accepts the complete central representative set and keeps USGS outside inference", async () => {
  const workload = await loadMaterializedComparisonWorkloadV4();
  const plan = buildV4ComparisonPlan(workload);
  assert.equal(plan.ready, true);
  assert.deepEqual(plan.blockers, []);
  assert.deepEqual(plan.stress_lanes, [
    {
      id: "usgs-large-sheet-stress-v1",
      inference_eligible: false,
      included: false,
    },
  ]);
  assert(
    buildV4ExecutionSchedule(plan, { finalPairs: 24, seed: 1 }).length > 0,
  );
});

test("schedules each journey sample as an ordered fresh-process component bundle", async () => {
  const workload = await loadMaterializedComparisonWorkloadV4();
  const plan = buildV4ComparisonPlan(workload, readyDefinitions());
  assert.deepEqual(plan.blockers, []);
  assert.equal(plan.ready, true);
  const schedule = buildV4ExecutionSchedule(plan, { finalPairs: 24, seed: 7 });
  const expectedRuns = plan.journeys.reduce(
    (count, journey) => count + journey.component_order.length * (6 + 24) * 2,
    0,
  );
  assert.equal(schedule.length, expectedRuns);

  const firstBundleId = schedule[0].bundle_id;
  const firstBundle = schedule.filter(
    ({ bundle_id: bundleId }) => bundleId === firstBundleId,
  );
  assert.deepEqual(
    firstBundle.map(({ component }) => component),
    plan.journeys[0].component_order,
  );
  assert(
    firstBundle.every(({ one_process_per_component: one }) => one === true),
  );
  assert(
    firstBundle.every(
      ({ cache_class: cacheClass }) => cacheClass === "app-cold",
    ),
  );
  assert(
    firstBundle.every(({ inference_eligible: eligible }) => eligible === false),
  );

  const finalSmall = schedule.filter(
    (run) => run.phase === "final" && run.journey === "small-shell-open",
  );
  const finalBundles = [
    ...new Map(finalSmall.map((run) => [run.bundle_id, run])).values(),
  ];
  assert.equal(finalBundles.length, 48);
  for (let block = 0; block < 24; block += 4) {
    const pairFirsts = finalBundles
      .filter(({ pair_position: position }) => position === "first")
      .slice(block, block + 4);
    assert.equal(
      pairFirsts.filter(({ implementation }) => implementation === "electron")
        .length,
      2,
    );
  }
});

test("assigns the shared native lane only to components with matched native replay", () => {
  assert.equal(componentInputLaneV4("open-pdf"), "native-x11-xtest");
  assert.equal(componentInputLaneV4("editor-create"), "native-x11-xtest");
  assert.equal(componentInputLaneV4("zoom"), "semantic-diagnostic");
  assert.equal(componentInputLaneV4("fit-modes"), "semantic-diagnostic");
  assert.equal(componentInputLaneV4("editor-workload"), "semantic-diagnostic");
  assert.equal(
    componentInputLaneV4("persistence-workload"),
    "semantic-diagnostic",
  );
  assert.equal(
    componentInputLaneV4("cache-pressure-recovery"),
    "semantic-diagnostic",
  );
  assert.equal(
    runnerInputLaneV4("electron", "semantic-diagnostic"),
    "cdp-input-diagnostic",
  );
  assert.equal(
    runnerInputLaneV4("gpui", "semantic-diagnostic"),
    "semantic-diagnostic",
  );
  assert.equal(
    runnerInputLaneV4("electron", "native-x11-xtest"),
    "native-x11-xtest",
  );
});

test("verifies every locked representative fixture before scheduling", async () => {
  const directory = await mkdtemp(resolve(tmpdir(), "bp-v4-fixture-"));
  try {
    const path = resolve(directory, "fixture.pdf");
    const bytes = "fixture bytes";
    await writeFile(path, bytes);
    const sha256 = createHash("sha256").update(bytes).digest("hex");
    const plan = {
      journeys: [{ fixture_id: "fixture-v1", fixture_sha256: sha256 }],
    };
    const verified = await verifyV4Fixtures(
      plan,
      new Map([["fixture-v1", path]]),
    );
    assert.equal(verified["fixture-v1"].sha256, sha256);
    await assert.rejects(
      verifyV4Fixtures(
        {
          journeys: [
            { fixture_id: "fixture-v1", fixture_sha256: "0".repeat(64) },
          ],
        },
        new Map([["fixture-v1", path]]),
      ),
      /SHA-256 mismatch/,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

function electronReceipt(command, component, journey) {
  const payload = {
    parent_scenario: journey,
    component_scenario: component,
    command_id: command.id,
    source_command_id: command.id,
    mapping_status: "exact-semantic-map",
    component_execution_passed: true,
    proven_milestones: [...command.expected_milestones],
    missing_milestones: [],
  };
  return {
    command_id: command.id,
    live: true,
    passed: true,
    evidence_sha256: canonicalSha256(payload),
    ...payload,
  };
}

function gpuiReceipt(command, component, journey) {
  const payload = {
    command_id: command.id,
    live: true,
    passed: true,
    component_scenario: component,
    parent_scenario: journey,
    milestone_ids: [...command.expected_milestones],
    evidence_event_indexes: command.expected_milestones.map(
      (_, index) => index,
    ),
  };
  return { ...payload, evidence_sha256: canonicalSha256(payload) };
}

function componentReport({
  implementation,
  component,
  journey,
  fixtureSha256,
  receipts,
}) {
  const v4 = {
    manifest_id: "bp-perf-v4-decision-1",
    scenario_contract_version: "bp-perf-v4-representative-1",
    parent_scenario: journey,
    component_scenario: component,
    component_receipts_passed: true,
    command_receipts_by_iteration: [{ iteration: 1, receipts }],
  };
  return {
    implementation,
    scenario: component,
    requested_iterations: 1,
    cache_class: "app-cold",
    summary: { successful_iterations: 1, failed_iterations: 0 },
    pdf: { sha256: fixtureSha256 },
    ...(implementation === "electron"
      ? { v4_parent_execution: v4 }
      : { comparison_v4: v4 }),
  };
}

test("accepts independently hashed exact Electron and GPUI command receipts", async () => {
  const workload = await loadMaterializedComparisonWorkloadV4();
  const contract = buildScenarioContractV4(workload, "dense-mixed-editing");
  const component = "annotation-transform";
  const command = contract.commands.find(
    ({ id }) => id === "rectangle:select-move-resize",
  );
  const fixture = { sha256: contract.fixture_sha256 };
  for (const implementation of ["electron", "gpui"]) {
    const receipt =
      implementation === "electron"
        ? electronReceipt(command, component, "dense-mixed-editing")
        : gpuiReceipt(command, component, "dense-mixed-editing");
    const report = componentReport({
      implementation,
      component,
      journey: "dense-mixed-editing",
      fixtureSha256: fixture.sha256,
      receipts: [receipt],
    });
    const assessment = validateV4ComponentReport({
      report,
      implementation,
      journey: "dense-mixed-editing",
      component,
      fixture,
      scenarioContract: contract,
    });
    assert.deepEqual(assessment.errors, []);
    assert.equal(assessment.passed, true);
    assert.deepEqual(commandReceiptsFromReportV4(report, implementation), [
      receipt,
    ]);
  }
});

test("fails closed on a forged hash, missing milestone, or unretained receipt set", async () => {
  const workload = await loadMaterializedComparisonWorkloadV4();
  const contract = buildScenarioContractV4(workload, "dense-mixed-editing");
  const component = "annotation-transform";
  const command = contract.commands.find(
    ({ id }) => id === "rectangle:select-move-resize",
  );
  const fixture = { sha256: contract.fixture_sha256 };
  const receipt = electronReceipt(command, component, "dense-mixed-editing");
  receipt.evidence_sha256 = "0".repeat(64);
  receipt.proven_milestones = [];
  const report = componentReport({
    implementation: "electron",
    component,
    journey: "dense-mixed-editing",
    fixtureSha256: fixture.sha256,
    receipts: [receipt],
  });
  const failed = validateV4ComponentReport({
    report,
    implementation: "electron",
    journey: "dense-mixed-editing",
    component,
    fixture,
    scenarioContract: contract,
  });
  assert.equal(failed.passed, false);
  assert(failed.errors.some((error) => error.includes("hash does not match")));
  assert(
    failed.errors.some((error) =>
      error.includes("milestone proof is not exact"),
    ),
  );

  report.v4_parent_execution.command_receipts_by_iteration = [];
  assert.equal(
    validateV4ComponentReport({
      report,
      implementation: "electron",
      journey: "dense-mixed-editing",
      component,
      fixture,
      scenarioContract: contract,
    }).passed,
    false,
  );
});

test("bundle manifests retain ordered raw report hashes and equal-weight metadata", () => {
  const journeyPlan = {
    component_order: ["first", "second"],
    component_weights: [0.5, 0.5],
    command_ids: ["command:first", "command:second"],
  };
  const plannedRuns = ["first", "second"].map((component, index) => ({
    phase: "final",
    inference_eligible: true,
    journey: "journey",
    journey_id: "journey-v1",
    pair: 1,
    pair_position: "first",
    implementation: "electron",
    component,
    component_weight: 0.5,
    input_lane: "semantic-diagnostic",
    component_index: index,
  }));
  const componentResults = ["first", "second"].map((component) => ({
    passed: true,
    raw_report_path: `/reports/${component}.json`,
    raw_report_sha256: component === "first" ? "1".repeat(64) : "2".repeat(64),
    receipts: [
      {
        command_id: `command:${component}`,
        evidence_sha256:
          component === "first" ? "3".repeat(64) : "4".repeat(64),
      },
    ],
  }));
  const bundle = buildBundleManifestV4({
    plannedRuns,
    componentResults,
    journeyPlan,
    candidateArtifact: { sha256: "5".repeat(64) },
    fixture: { sha256: "6".repeat(64) },
    workloadArtifactSha256: "7".repeat(64),
  });
  assert.deepEqual(bundle.component_aggregation.order, ["first", "second"]);
  assert.deepEqual(bundle.component_aggregation.weights, [0.5, 0.5]);
  assert.deepEqual(
    bundle.components.map(({ raw_report_sha256: sha256 }) => sha256),
    ["1".repeat(64), "2".repeat(64)],
  );
  assert.equal(
    bundle.component_aggregation.compensating_regressions_allowed,
    false,
  );
  assert.equal(bundle.passed, true);
  assert.throws(
    () =>
      buildBundleManifestV4({
        plannedRuns: [...plannedRuns].reverse(),
        componentResults,
        journeyPlan,
        candidateArtifact: {},
        fixture: {},
        workloadArtifactSha256: "7".repeat(64),
      }),
    /component order/,
  );
});
