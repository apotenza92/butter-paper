import assert from "node:assert/strict";
import test from "node:test";

import {
  buildFeatureCoverageReport,
  comparisonWorkloadArtifactHash,
  comparisonWorkloadHashes,
  currentRunnerCoverageReport,
  loadComparisonWorkload,
  runnerComparisonMetadata,
  validateComparisonWorkload,
} from "./comparison-workload.mjs";
import { assertBenchmarkStyleContract } from "./benchmark-canonical.mjs";

const expectedJourneyIds = [
  "viewer-v1",
  "rectangle-v1",
  "highlight-v1",
  "text-v1",
  "length-v1",
  "image-v1",
  "unknown-preservation-v1",
  "save-reopen-v1",
];

test("the frozen decision workload validates and has independent known hashes", async () => {
  const workload = await loadComparisonWorkload();

  assert.deepEqual(validateComparisonWorkload(workload), []);
  assert.equal(workload.manifest_id, "bp-perf-v3-decision-3");
  assert.deepEqual(workload.journeys.map(({ id }) => id), expectedJourneyIds);
  assert.deepEqual(comparisonWorkloadHashes(workload), {
    command_stream_sha256: "73877cf258d75dd76fd5e379db05b0b7114bd6df38a6ebdc3f70dae1902808cc",
    milestone_stream_sha256: "560bb1848a61a91e67331f10f4d7fb0789b5afc5214a0f8a488294883093e5b5",
    expected_state_sha256: "e9b746ca9ba717129c10c6e485e5f4ecf021e7276ed99d0f9799d567e570f887",
  });
  assert.equal(
    comparisonWorkloadArtifactHash(workload),
    "1926113f60f434b383aed89c34f157197f0b5680a4ad71d14f25c539f14cbd2f",
  );
});

test("v3 decision revision 3 keeps corrected native placement and no-fill selection semantics", async () => {
  const workload = await loadComparisonWorkload();
  const commands = new Map(workload.journeys.flatMap(({ commands }) => commands)
    .map((command) => [command.id, command]));

  assert.deepEqual(commands.get("rectangle:select-move-resize").select_point, {
    x: 117,
    y: 240,
  });

  assert.equal(workload.manifest_id, "bp-perf-v3-decision-3");
  assert.deepEqual(commands.get("text:create").placement, {
    point: { x: 210, y: 426 },
    sizing: "shaped-text-autosize-nonblank",
  });
  assert.equal("bounds" in commands.get("text:create"), false);
  assert.deepEqual(commands.get("text:edit-resize-history").resize_bounds, {
    x: 90, y: 390, width: 300, height: 84,
  });
  assert.deepEqual(commands.get("image:create").placement, {
    point: { x: 432, y: 444 },
    sizing: "natural-size-page-contained",
    max_page_fraction: 0.45,
    fixture_page_size_points: { width: 612, height: 792 },
  });
  assert.equal("bounds" in commands.get("image:create"), false);
  assert.deepEqual(commands.get("image:resize-history").replacement_bounds, {
    x: 360, y: 390, width: 180, height: 135,
  });
});

test("the workload freezes exact interaction and persistence obligations", async () => {
  const workload = await loadComparisonWorkload();
  const command = (id) => workload.journeys.flatMap(({ commands }) => commands)
    .find((candidate) => candidate.id === id);

  assert.deepEqual(command("viewer:zoom-sequence").percent, [
    100, 200, 400, 800, 1600, 400, 100, 800, 200, 100, 1200, 100,
  ]);
  assert.deepEqual(command("viewer:continuous-scroll").path, {
    forward_duration_ms: 20_000,
    forward_viewport_heights: 50,
    pause_duration_ms: 2_000,
    reverse_duration_ms: 10_000,
    finish_page: 1,
  });
  assert.deepEqual(assertBenchmarkStyleContract(command("rectangle:create-sparse")), {
    stroke: "#ff0000ff", fill: null, width_pt: 1, dash: "solid", opacity: 1,
  });
  assert.deepEqual(assertBenchmarkStyleContract(command("highlight:create")), {
    color: "#ffff00ff", width_pt: 12, opacity: 1, blend: "multiply",
  });
  assert.deepEqual(command("rectangle:create-sparse").pointer_path, {
    rate_hz: 120,
    duration_ms: 3_000,
    expected_sample_count: 361,
    coordinate_space: "pdf-points-bottom-left",
    start: { x: 72, y: 144 },
    finish: { x: 252, y: 240 },
    interpolation: "linear-inclusive",
  });
  assert.equal(command("highlight:create").pointer_path.rate_hz, 120);
  assert.equal(command("text:create").text, "Beam B-12 / revision 3");
  assert.deepEqual(command("length:set-scale").scale, {
    paper_points: 72,
    real_world_value: 1,
    unit: "m",
    precision: 2,
  });
  assert.equal(command("image:create").asset_id, "bp-image-checker-v1");
  assert.equal(command("unknown:assert-cycle-2").comparison, "byte-exact-dictionary-and-appearance-stream");
  assert.equal(command("persistence:reopen-2").cycle, 2);
});

test("validation rejects hash drift and missing required journeys", async () => {
  const workload = structuredClone(await loadComparisonWorkload());
  workload.journeys.find(({ id }) => id === "text-v1").commands[0].text = "drift";
  workload.journeys = workload.journeys.filter(({ id }) => id !== "image-v1");

  assert.deepEqual(validateComparisonWorkload(workload), [
    "missing required journey image-v1",
    "command_stream_sha256 does not match the canonical command stream",
    "milestone_stream_sha256 does not match the canonical milestone stream",
  ]);
});

test("coverage is blocked unless every command and milestone is explicitly supported", async () => {
  const workload = await loadComparisonWorkload();
  const report = buildFeatureCoverageReport(workload, {
    implementation: "gpui",
    supported_operations: ["app.launch-cold", "document.open", "render.wait-preview"],
    supported_milestones: ["native-window-presented", "document-opened", "preview-current-generation"],
  });

  assert.equal(report.status, "blocked");
  assert.equal(report.ready, false);
  assert.ok(report.recognized_operation_count > 0);
  assert.ok(report.blocked_command_count > 0);
  assert.equal(report.journeys.find(({ id }) => id === "image-v1").status, "blocked");
  assert.deepEqual(report.blocked_commands.find(({ command_id }) => command_id === "image:create"), {
    journey_id: "image-v1",
    command_id: "image:create",
    operation: "annotation.image.create",
    reason: "unsupported-operation",
  });
});

test("current runners expose their exact frozen command coverage", async () => {
  const workload = await loadComparisonWorkload();
  for (const implementation of ["electron", "gpui"]) {
    const metadata = runnerComparisonMetadata(workload, implementation, "zoom");
    assert.equal(metadata.manifest_id, "bp-perf-v3-decision-3");
    assert.equal(metadata.execution_lane, "development-subset");
    assert.equal(metadata.scenario_status, "supported-diagnostic");
    assert.equal(metadata.diagnostic_timing_eligible, true);
    assert.equal(metadata.decision_timing_eligible, false);
    assert.equal(metadata.feature_coverage.status, "blocked");
    assert.equal(metadata.feature_coverage.ready, false);
  }

  const annotation = runnerComparisonMetadata(workload, "gpui", "annotation-create");
  assert.equal(annotation.scenario_status, "supported-diagnostic");
  assert.equal(annotation.diagnostic_timing_eligible, true);
  assert.equal(annotation.execution_lane, "semantic-diagnostic");
  assert.equal(
    annotation.feature_coverage.journeys.find(({ id }) => id === "highlight-v1")
      .commands.find(({ command_id }) => command_id === "highlight:create").status,
    "supported",
  );
  assert.equal(annotation.feature_coverage.ready, false);

  const editorCreate = runnerComparisonMetadata(workload, "gpui", "editor-create");
  assert.equal(editorCreate.scenario_status, "supported-diagnostic");
  assert.equal(editorCreate.execution_lane, "semantic-diagnostic");
  assert.equal(editorCreate.diagnostic_timing_eligible, true);
  assert.equal(editorCreate.decision_timing_eligible, false);
  assert.equal(editorCreate.feature_coverage.ready, false);

  const gpuiEditor = runnerComparisonMetadata(workload, "gpui", "editor-workload");
  for (const [journeyId, commandId] of [
    ["rectangle-v1", "rectangle:repeat-dense"],
    ["highlight-v1", "highlight:edit-history"],
    ["text-v1", "text:edit-resize-history"],
    ["length-v1", "length:set-scale"],
    ["length-v1", "length:create"],
    ["length-v1", "length:edit-endpoint-history"],
  ]) {
    assert.equal(
      gpuiEditor.feature_coverage.journeys.find(({ id }) => id === journeyId)
        .commands.find(({ command_id: candidate }) => candidate === commandId).status,
      "supported",
    );
  }

  for (const commandId of ["text:create", "image:create", "image:resize-history"]) {
    assert.equal(
      gpuiEditor.feature_coverage.journeys.flatMap(({ commands }) => commands)
        .find(({ command_id: candidate }) => candidate === commandId).status,
      "supported",
    );
  }

  const cachePressure = runnerComparisonMetadata(workload, "gpui", "cache-pressure");
  const cachePressureCommand = cachePressure.feature_coverage.journeys
    .find(({ id }) => id === "viewer-v1")
    .commands.find(({ command_id }) => command_id === "viewer:cache-pressure");
  assert.equal(cachePressureCommand.status, "supported");

  const electronCachePressure = runnerComparisonMetadata(workload, "electron", "cache-pressure");
  assert.equal(electronCachePressure.scenario_status, "blocked-unsupported");
  assert.equal(
    electronCachePressure.blocked_reason,
    "electron-cache-pressure-live-semantic-and-gpu-upload-proof-missing",
  );

  const unsupported = runnerComparisonMetadata(workload, "gpui", "annotation");
  assert.equal(unsupported.scenario_status, "blocked-unsupported");
  assert.equal(unsupported.diagnostic_timing_eligible, false);
  assert.equal(unsupported.decision_timing_eligible, false);
  assert.equal(unsupported.blocked_reason, "runner-does-not-implement-scenario");

  const gpuiProperties = runnerComparisonMetadata(
    workload,
    "gpui",
    "annotation-properties-history",
  );
  assert.equal(gpuiProperties.scenario_status, "supported-diagnostic");
  assert.equal(
    gpuiProperties.feature_coverage.journeys
      .find(({ id }) => id === "rectangle-v1")
      .commands.find(({ command_id }) => command_id === "rectangle:properties-history").status,
    "supported",
  );

  const electronProperties = runnerComparisonMetadata(
    workload,
    "electron",
    "annotation-properties-history",
  );
  assert.equal(electronProperties.scenario_status, "supported-diagnostic");
  assert.equal(electronProperties.diagnostic_timing_eligible, true);
  assert.equal(electronProperties.decision_timing_eligible, false);
  assert.equal(electronProperties.blocked_reason, null);
  assert.equal(
    electronProperties.feature_coverage.journeys
      .find(({ id }) => id === "rectangle-v1")
      .commands.find(({ command_id }) => command_id === "rectangle:properties-history").status,
    "supported",
  );
  assert.equal(electronProperties.feature_coverage.ready, false);

  const electronEditor = runnerComparisonMetadata(workload, "electron", "editor-workload");
  assert.equal(electronEditor.scenario_status, "supported-diagnostic");
  for (const [journeyId, commandId] of [
    ["rectangle-v1", "rectangle:repeat-dense"],
    ["highlight-v1", "highlight:edit-history"],
    ["text-v1", "text:create"],
    ["text-v1", "text:edit-resize-history"],
    ["length-v1", "length:set-scale"],
    ["length-v1", "length:create"],
    ["length-v1", "length:edit-endpoint-history"],
  ]) {
    assert.equal(
      electronEditor.feature_coverage.journeys.find(({ id }) => id === journeyId)
        .commands.find(({ command_id: candidate }) => candidate === commandId).status,
      "supported",
    );
  }
  const electronImages = electronEditor.feature_coverage.journeys
    .find(({ id }) => id === "image-v1").commands;
  const electronImageCreate = electronImages.find(({ command_id }) => command_id === "image:create");
  assert.equal(electronImageCreate.status, "blocked");
  assert.deepEqual(electronImageCreate.missing_milestones, ["bitmap-upload-recorded"]);
  const electronImageResize = electronImages
    .find(({ command_id }) => command_id === "image:resize-history");
  assert.equal(electronImageResize.status, "blocked");
  assert.deepEqual(electronImageResize.missing_milestones, ["upload-byte-count-recorded"]);

  const electronPersistence = runnerComparisonMetadata(workload, "electron", "persistence-workload");
  assert.equal(electronPersistence.scenario_status, "supported-diagnostic");
  for (const commandId of [
    "unknown:import",
    "unknown:assert-cycle-1",
    "unknown:assert-cycle-2",
    "persistence:apply-fixed-state",
    "persistence:save-1",
    "persistence:reopen-1",
    "persistence:save-2",
    "persistence:reopen-2",
  ]) {
    assert.equal(
      electronPersistence.feature_coverage.journeys
        .flatMap(({ commands }) => commands)
        .find(({ command_id: candidate }) => candidate === commandId).status,
      "supported",
    );
  }

  const gpuiPersistence = runnerComparisonMetadata(workload, "gpui", "persistence-workload");
  for (const commandId of [
    "unknown:import",
    "unknown:assert-cycle-1",
    "unknown:assert-cycle-2",
    "persistence:apply-fixed-state",
    "persistence:save-1",
    "persistence:reopen-1",
    "persistence:save-2",
    "persistence:reopen-2",
  ]) {
    assert.equal(
      gpuiPersistence.feature_coverage.journeys
        .flatMap(({ commands }) => commands)
        .find(({ command_id: candidate }) => candidate === commandId).status,
      "supported",
    );
  }

  const currentCoverage = Object.fromEntries(
    currentRunnerCoverageReport(workload).implementations
      .map((report) => [report.implementation, {
        ready: report.ready_command_count,
        blocked: report.blocked_command_count,
        total: report.command_count,
      }]),
  );
  assert.deepEqual(currentCoverage, {
    electron: { ready: 27, blocked: 4, total: 31 },
    gpui: { ready: 30, blocked: 1, total: 31 },
  });
});

test("Electron recognizes exact native launch and open but blocks unproven high-zoom pan", async () => {
  const workload = await loadComparisonWorkload();
  const open = runnerComparisonMetadata(workload, "electron", "open-pdf");
  const viewer = open.feature_coverage.journeys.find(({ id }) => id === "viewer-v1");
  for (const commandId of ["viewer:launch-cold", "viewer:open-each"]) {
    assert.equal(
      viewer.commands.find(({ command_id: candidate }) => candidate === commandId).status,
      "supported",
    );
  }

  const pan = runnerComparisonMetadata(workload, "electron", "high-zoom-pan");
  assert.equal(pan.scenario_status, "blocked-unsupported");
  assert.equal(pan.blocked_reason, "electron-high-zoom-pan-live-proof-missing");
  assert.equal(
    pan.feature_coverage.journeys.find(({ id }) => id === "viewer-v1")
      .commands.find(({ command_id: commandId }) => commandId === "viewer:pan-usgs").status,
    "blocked",
  );
});

test("GPUI recognizes native launch and open evidence without hiding the USGS pan blocker", async () => {
  const workload = await loadComparisonWorkload();
  const metadata = runnerComparisonMetadata(workload, "gpui", "open-pdf");
  const viewer = metadata.feature_coverage.journeys.find(({ id }) => id === "viewer-v1");
  assert.equal(
    viewer.commands.find(({ command_id: commandId }) => commandId === "viewer:launch-cold").status,
    "supported",
  );
  assert.equal(
    viewer.commands.find(({ command_id: commandId }) => commandId === "viewer:open-each").status,
    "supported",
  );
  assert.equal(metadata.feature_coverage.ready, false);
  assert.equal(metadata.decision_timing_eligible, false);
});
