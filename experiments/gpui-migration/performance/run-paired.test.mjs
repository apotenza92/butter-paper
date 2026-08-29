import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import test from "node:test";

import { loadComparisonWorkload } from "./comparison-workload.mjs";
import {
  buildPairedComparisonPlan,
  buildPairedExecutionSchedule,
  assessUntimedPreflightRuns,
  fullTimedRepresentativeScenarios,
  matchedScenarioBlockers,
  pairedInputLane,
  pairOrder,
  pairOrders,
  resolveScenarioFixtures,
  scenarioFixtureRequirements,
  untimedCorrectnessPreflights,
  verifyScenarioFixtures,
} from "./run-paired.mjs";

test("declares the full timed decision set and every untimed gate per implementation", () => {
  assert.deepEqual(fullTimedRepresentativeScenarios, [
    "open-pdf",
    "viewer-layout",
    "page-navigation",
    "zoom",
    "high-zoom-pan",
    "continuous-scroll",
    "cache-pressure",
    "close-reopen",
    "annotation-create",
    "annotation-transform",
    "editor-create",
  ]);
  assert.deepEqual(untimedCorrectnessPreflights, [
    { implementation: "electron", scenario: "open-pdf", input_lane: "native-x11-xtest", proof: "native-launch-open" },
    { implementation: "gpui", scenario: "open-pdf", input_lane: "native-x11-xtest", proof: "native-launch-open" },
    { implementation: "electron", scenario: "editor-workload", input_lane: "semantic-diagnostic", proof: "full-editor-correctness" },
    { implementation: "gpui", scenario: "editor-workload", input_lane: "semantic-diagnostic", proof: "full-editor-correctness" },
    { implementation: "electron", scenario: "persistence-workload", input_lane: "semantic-diagnostic", proof: "full-persistence-correctness" },
    { implementation: "gpui", scenario: "persistence-workload", input_lane: "semantic-diagnostic", proof: "full-persistence-correctness" },
  ]);
});

test("alternates implementation order within fixed pairs", () => {
  assert.deepEqual(pairOrder(1), ["electron", "gpui"]);
  assert.deepEqual(pairOrder(2), ["gpui", "electron"]);
  assert.deepEqual(pairOrder(5), ["electron", "gpui"]);
});

test("supports recorded balanced randomized blocks without losing alternating development order", () => {
  assert.deepEqual(pairOrders({ pairCount: 5, mode: "alternating", seed: 99 }), [
    ["electron", "gpui"],
    ["gpui", "electron"],
    ["electron", "gpui"],
    ["gpui", "electron"],
    ["electron", "gpui"],
  ]);
  const randomized = pairOrders({ pairCount: 8, mode: "randomized-blocks", seed: 0x4250_5633 });
  assert.deepEqual(
    randomized,
    pairOrders({ pairCount: 8, mode: "randomized-blocks", seed: 0x4250_5633 }),
  );
  for (let block = 0; block < randomized.length; block += 4) {
    assert.equal(
      randomized.slice(block, block + 4).filter(([first]) => first === "electron").length,
      2,
    );
  }
  assert.throws(
    () => pairOrders({ pairCount: 6, mode: "randomized-blocks", seed: 1 }),
    /multiple of 4/,
  );
});

test("maps every scenario to its locked fixture instead of reusing one PDF", () => {
  const requirements = scenarioFixtureRequirements([
    "open-pdf",
    "viewer-layout",
    "page-navigation",
    "zoom",
    "high-zoom-pan",
    "cache-pressure",
    "close-reopen",
    "annotation-create",
    "continuous-scroll",
    "editor-workload",
    "persistence-workload",
  ]);
  assert.equal(requirements["open-pdf"].fixture_id, "nasa-apollo-summary-526-v1");
  assert.equal(requirements["viewer-layout"].fixture_id, "bp-multi-page-v1");
  assert.equal(requirements["page-navigation"].fixture_id, "nasa-apollo-summary-526-v1");
  assert.equal(requirements.zoom.fixture_id, "usgs-usa-geology-sheet-v1");
  assert.equal(requirements["high-zoom-pan"].fixture_id, "usgs-usa-geology-sheet-v1");
  assert.equal(requirements["cache-pressure"].fixture_id, "bp-multi-page-v1");
  assert.equal(requirements["close-reopen"].fixture_id, "bp-multi-page-v1");
  assert.equal(requirements["annotation-create"].fixture_id, "bp-annotation-density-v1");
  assert.equal(requirements["continuous-scroll"].fixture_id, "nasa-apollo-summary-526-v1");
  assert.equal(requirements["editor-workload"].fixture_id, "bp-annotation-density-v1");
  assert.equal(requirements["persistence-workload"].fixture_id, "bp-annotation-all-v1");
  assert.equal(
    requirements.zoom.fixture_sha256,
    "f058179e193ccbc15ca662feff3554102f64ff2114436a5ce6116d6fa5d2a6e2",
  );
});

test("allows every matched scenario and blocks unmatched or semantic-only pairs", async () => {
  const workload = await loadComparisonWorkload();
  assert.deepEqual(matchedScenarioBlockers(workload, [
    "open-pdf",
    "viewer-layout",
    "page-navigation",
    "zoom",
    "close-reopen",
    "annotation-create",
    "continuous-scroll",
  ]), []);
  assert.deepEqual(matchedScenarioBlockers(workload, ["editor-create"]), []);
  assert.deepEqual(
    matchedScenarioBlockers(workload, ["high-zoom-pan", "cache-pressure", "editor-workload", "persistence-workload"])
      .map(({ scenario, implementation, reason }) => ({ scenario, implementation, reason })),
    [
      {
        scenario: "high-zoom-pan",
        implementation: "electron",
        reason: "electron-high-zoom-pan-live-proof-missing",
      },
      {
        scenario: "cache-pressure",
        implementation: "electron",
      reason: "electron-cache-pressure-live-semantic-and-gpu-upload-proof-missing",
      },
    ],
  );
});

test("fails the decision plan closed on unsupported scenarios and global command coverage", async () => {
  const workload = await loadComparisonWorkload();
  const plan = buildPairedComparisonPlan(workload);

  assert.equal(plan.ready, false);
  assert.deepEqual(
    plan.timed_scenarios.map(({ scenario, input_lane: inputLane }) => [scenario, inputLane]),
    fullTimedRepresentativeScenarios.map((scenario) => [
      scenario,
      pairedInputLane(scenario) ?? "semantic-diagnostic",
    ]),
  );
  assert.deepEqual(plan.untimed_preflights, untimedCorrectnessPreflights.map((preflight) => ({
    ...preflight,
    decision_timing_eligible: false,
  })));
  assert.deepEqual(
    plan.blockers.map(({ phase, scenario, implementation, reason }) => ({
      phase, scenario, implementation, reason,
    })),
    [
      {
        phase: "timed",
        scenario: "high-zoom-pan",
        implementation: "electron",
        reason: "electron-high-zoom-pan-live-proof-missing",
      },
      {
        phase: "timed",
        scenario: "cache-pressure",
        implementation: "electron",
      reason: "electron-cache-pressure-live-semantic-and-gpu-upload-proof-missing",
      },
      {
        phase: "global-feature-coverage",
        scenario: null,
        implementation: "electron",
        reason: "global-command-coverage-incomplete",
      },
      {
        phase: "global-feature-coverage",
        scenario: null,
        implementation: "gpui",
        reason: "global-command-coverage-incomplete",
      },
    ],
  );
  assert.deepEqual(
    plan.global_command_coverage.map(({ implementation, command_count: commandCount, ready }) => ({
      implementation, command_count: commandCount, ready,
    })),
    [
      { implementation: "electron", command_count: 31, ready: false },
      { implementation: "gpui", command_count: 31, ready: false },
    ],
  );
  for (const coverage of plan.global_command_coverage) {
    assert.equal(
      coverage.ready_command_count + coverage.blocked_command_count,
      coverage.command_count,
    );
    assert(coverage.blocked_command_count > 0);
  }

  const partial = buildPairedComparisonPlan(workload, [
    "open-pdf",
    "viewer-layout",
    "page-navigation",
    "zoom",
    "close-reopen",
    "annotation-create",
    "continuous-scroll",
  ]);
  assert.deepEqual(
    partial.blockers
      .filter(({ reason }) => reason === "required-timed-scenario-missing")
      .map(({ scenario }) => scenario),
    ["high-zoom-pan", "cache-pressure", "annotation-transform", "editor-create"],
  );
});

test("schedules every untimed preflight before timed warmups and measured pairs", async () => {
  const workload = await loadComparisonWorkload();
  const plan = buildPairedComparisonPlan(workload);
  const schedule = buildPairedExecutionSchedule(
    { ...plan, ready: true, blockers: [] },
    { pairCount: 2, warmups: 1, orderMode: "alternating", seed: 7 },
  );

  assert.deepEqual(
    schedule.slice(0, untimedCorrectnessPreflights.length).map((run) => ({
      phase: run.phase,
      implementation: run.implementation,
      scenario: run.scenario,
      input_lane: run.input_lane,
      measured: run.measured,
    })),
    untimedCorrectnessPreflights.map(({ implementation, scenario, input_lane: inputLane }) => ({
      phase: "untimed-correctness-preflight",
      implementation,
      scenario,
      input_lane: inputLane,
      measured: false,
    })),
  );
  assert(schedule.slice(untimedCorrectnessPreflights.length).every(
    ({ phase }) => phase === "timed",
  ));
  assert.throws(
    () => buildPairedExecutionSchedule(plan, { pairCount: 2, warmups: 0 }),
    /BLOCKED paired comparison plan/,
  );
});

test("requires an explicit matching proof for every untimed preflight", async () => {
  const workload = await loadComparisonWorkload();
  const plan = buildPairedComparisonPlan(workload);
  const runs = plan.untimed_preflights.map((preflight) => ({
    phase: "untimed-correctness-preflight",
    implementation: preflight.implementation,
    scenario: preflight.scenario,
    input_lane: preflight.input_lane,
    exit_code: 0,
    report: { successful_iterations: 1, failed_iterations: 0 },
    preflight_proof: {
      proof: preflight.proof,
      input_lane: preflight.input_lane,
      passed: true,
    },
  }));

  assert.deepEqual(assessUntimedPreflightRuns(plan, runs), {
    ready: true,
    failures: [],
  });
  runs[0].preflight_proof = null;
  const failed = assessUntimedPreflightRuns(plan, runs);
  assert.equal(failed.ready, false);
  assert.match(failed.failures[0], /electron:open-pdf.*native-launch-open proof did not pass/);
});

test("resolves each scenario independently and selects native input where required", () => {
  const assignments = new Map([
    ["nasa-apollo-summary-526-v1", "/fixtures/nasa.pdf"],
    ["bp-annotation-density-v1", "/fixtures/annotations.pdf"],
  ]);
  const fixtures = resolveScenarioFixtures(
    ["page-navigation", "annotation-create", "continuous-scroll"],
    assignments,
  );
  assert.equal(fixtures["page-navigation"].path, "/fixtures/nasa.pdf");
  assert.equal(fixtures["annotation-create"].path, "/fixtures/annotations.pdf");
  assert.equal(fixtures["continuous-scroll"].path, "/fixtures/nasa.pdf");
  assert.equal(pairedInputLane("annotation-create"), "native-x11-xtest");
  assert.equal(pairedInputLane("annotation-transform"), "native-x11-xtest");
  assert.equal(pairedInputLane("editor-create"), "native-x11-xtest");
  assert.equal(pairedInputLane("continuous-scroll"), "native-x11-xtest");
  assert.equal(pairedInputLane("viewer-layout"), "native-x11-xtest");
  assert.equal(pairedInputLane("page-navigation"), "native-x11-xtest");
  assert.equal(pairedInputLane("zoom"), "native-x11-xtest");
  assert.equal(pairedInputLane("high-zoom-pan"), "native-x11-xtest");
  assert.equal(pairedInputLane("open-pdf"), null);
  assert.equal(pairedInputLane("cache-pressure"), null);
  assert.equal(pairedInputLane("close-reopen"), null);
  assert.throws(
    () => resolveScenarioFixtures(["zoom"], assignments),
    /--fixture usgs-usa-geology-sheet-v1=<file> is required for zoom/,
  );
});

test("verifies every unique fixture hash before a paired launch", async () => {
  const directory = await mkdtemp(resolve(tmpdir(), "bp-paired-fixture-"));
  try {
    const path = resolve(directory, "fixture.pdf");
    const bytes = "locked fixture bytes";
    const expected = createHash("sha256").update(bytes).digest("hex");
    await writeFile(path, bytes);
    const scenarioFixtures = {
      "open-pdf": {
        fixture_id: "fixture-v1",
        fixture_sha256: expected,
        path,
      },
      "page-navigation": {
        fixture_id: "fixture-v1",
        fixture_sha256: expected,
        path,
      },
    };
    assert.deepEqual(await verifyScenarioFixtures(scenarioFixtures), {
      "fixture-v1": { path, expected_sha256: expected, sha256: expected },
    });
    scenarioFixtures["open-pdf"].fixture_sha256 = "0".repeat(64);
    scenarioFixtures["page-navigation"].fixture_sha256 = "0".repeat(64);
    assert.rejects(
      verifyScenarioFixtures(scenarioFixtures),
      /fixture-v1: SHA-256 mismatch/,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
