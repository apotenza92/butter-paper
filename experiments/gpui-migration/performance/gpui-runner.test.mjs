import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import test from "node:test";

import {
  applyV4ComponentExecutionContract,
  buildDynamicFidelityV5Context,
  buildNativeEditingV5Context,
  buildNativeEditingV5Evidence,
  buildMultiDocumentV5Evidence,
  buildV4RunnerContext,
  buildV4ComponentReceipts,
  buildV4ComparisonReport,
  comparisonMilestonesSucceeded,
  compatEvidenceValidationForIteration,
  compatResourceCleanupForIteration,
  collectPersistenceEvidence,
  createGpuiV6ExecutionContext,
  dynamicArtifactDirectoryForOutput,
  exactPersistenceReceiptSucceeded,
  fixtureIdsForLaunch,
  formatFixtureAccessError,
  gpuiComparisonMetadata,
  gpuiGpuEvidencePassed,
  gpuiNativeApplicationAckSamples,
  nativeEvidenceTimeoutMs,
  parseArguments,
  persistenceScenarioSucceeded,
  prepareFreshArtifactDirectory,
  qualifyNativeLaneMetadata,
  scenarioSucceeded,
  validateScenarioFixture,
  validateOrderedScenarioFixtures,
} from "./gpui-runner.mjs";
import { loadComparisonWorkload } from "./comparison-workload.mjs";
import { loadComparisonWorkloadV4 } from "./comparison-workload-v4.mjs";
import { loadComparisonWorkloadV5 } from "./comparison-workload-v5.mjs";
import { longbridgeCompatProfile } from "./compat-evidence-validator.mjs";

test("binds GPUI benefit execution to the exact frozen v6 context", async () => {
  const bytes = await readFile(
    resolve(import.meta.dirname, "comparison-workload-v6.json"),
  );
  const workload = JSON.parse(bytes);
  const context = createGpuiV6ExecutionContext({
    workload,
    workloadByteSha256: createHash("sha256").update(bytes).digest("hex"),
    parentScenario: "small-shell-open",
    componentScenario: "open-pdf",
  });
  assert.equal(context.protocol_version, "bp-perf-v6");
  assert.equal(context.benefit_metrics_eligible, true);
  assert.deepEqual(context.execution_contract.fixture_ids, [
    "bp-single-page-v1",
  ]);
  assert.throws(
    () =>
      createGpuiV6ExecutionContext({
        workload,
        workloadByteSha256: "0".repeat(64),
        parentScenario: "small-shell-open",
        componentScenario: "open-pdf",
      }),
    /workload byte SHA-256 changed/,
  );
});

test("isolates dynamic artifacts by output and rejects a nonempty attempt directory", async () => {
  const directory = await mkdtemp(resolve(tmpdir(), "bp-gpui-artifacts-"));
  try {
    const artifactDirectory = dynamicArtifactDirectoryForOutput(
      resolve(directory, "dynamic-held-functional-5.json"),
      2,
    );
    assert.match(
      artifactDirectory,
      /dynamic-held-functional-5-artifacts\/iteration-002$/,
    );
    assert.equal(
      await prepareFreshArtifactDirectory(artifactDirectory),
      artifactDirectory,
    );
    await writeFile(resolve(artifactDirectory, "retained.txt"), "evidence");
    await assert.rejects(
      prepareFreshArtifactDirectory(artifactDirectory),
      /must be empty before launch/,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("builds the isolated native v5 dynamic fidelity runner contract", async () => {
  const context = buildDynamicFidelityV5Context(
    await loadComparisonWorkloadV5(),
  );
  assert.equal(context.scenario, "viewer-dynamic-fidelity");
  assert.equal(context.input_lane, "native-x11-xtest");
  assert.deepEqual(context.command_ids, ["viewer:dynamic-fidelity-scroll"]);
  assert.equal(context.commands[0].observer.expected_sample_count, 1_921);
  assert.equal(context.commands[0].expected_trajectory_sample_count, 3_841);
  const options = parseArguments([
    "--scenario",
    "viewer-dynamic-fidelity",
    "--pdf",
    "/tmp/nasa.pdf",
    "--input-lane",
    "native-x11-xtest",
  ]);
  assert.equal(options.scenario, "viewer-dynamic-fidelity");
});

test("builds isolated native v5 property and snap runner contracts", async () => {
  const workload = await loadComparisonWorkloadV5();
  const property = buildNativeEditingV5Context(
    workload,
    "native-property-edit-undo",
  );
  const snap = buildNativeEditingV5Context(
    workload,
    "native-snap-transform-120hz",
  );
  assert.equal(property.input_lane, "native-x11-xtest");
  assert.deepEqual(property.command_ids, [
    "annotation:native-property-edit-undo",
  ]);
  assert.equal(
    property.component_benefit_metrics_eligible[property.scenario],
    false,
  );
  assert.equal(snap.commands[0].expected_sample_count, 361);
  assert.equal(snap.commands[0].rate_hz, 120);
  assert.equal(snap.commands[0].duration_ms, 3_000);
  for (const scenario of [property.scenario, snap.scenario]) {
    assert.equal(
      parseArguments([
        "--scenario",
        scenario,
        "--pdf",
        "/tmp/dense.pdf",
        "--input-lane",
        "native-x11-xtest",
      ]).scenario,
      scenario,
    );
  }
});

test("builds exact native v5 property and snap hard evidence", async () => {
  const workload = await loadComparisonWorkloadV5();
  for (const component of [
    "native-property-edit-undo",
    "native-snap-transform-120hz",
  ]) {
    const context = buildNativeEditingV5Context(workload, component);
    const command = context.commands[0];
    const property = component === "native-property-edit-undo";
    const application = {
      schema_version: 1,
      event: property
        ? "native-v5-property-application-evidence"
        : "native-v5-snap-application-evidence",
      t_ms: 12,
      command_id: command.id,
      ...(property
        ? {
            property: "stroke_width_points",
            before: 1.5,
            committed: 4,
            after_undo: 1.5,
            effective_history_revision_delta: 1,
            application_undo_count: 1,
            canonical_state_restored: true,
            known_baseline_defect_id: null,
            thumbnail_current: true,
          }
        : {
            expected_injected_sample_count: 361,
            observed_application_update_count: 3,
            observed_application_update_timestamps_ms: [1, 2, 3],
            first_position_observed: true,
            final_position_observed: true,
            snap_guide_presented_count: 1,
          }),
    };
    const presentation = {
      schema_version: 1,
      event: property
        ? "native-v5-property-presentation-evidence"
        : "native-v5-snap-presentation-evidence",
      t_ms: 11,
      command_id: command.id,
      gpui_platform_draw_submitted: true,
      snap_guide_presented: !property,
    };
    const events = [
      application,
      presentation,
      ...command.expected_milestones.map((milestone) => ({
        event: "comparison-milestone",
        command_id: command.id,
        milestone,
      })),
    ];
    const result = buildNativeEditingV5Evidence(1, events, context, {
      inputLane: "native-x11-xtest",
      nativeReplay: {
        success: true,
        target_verified: true,
        commands: [
          {
            command_id: command.id,
            timing: { within_tolerance: true },
            ...(property
              ? {}
              : { timestamped_injected_samples: Array.from({ length: 361 }) }),
          },
        ],
      },
      applicationSuccess: true,
    });
    assert.equal(result.passed, true);
    assert.equal(result.command_receipts.length, 1);
    assert.match(result.command_receipts[0].evidence_sha256, /^[0-9a-f]{64}$/);
    assert.equal(result.semantic_summary.trusted_native_input, true);
  }
});

function evidence(events, overrides = {}) {
  return {
    timedOut: false,
    outcome: { exit_code: 0, spawn_error: null },
    invalidStdout: [],
    events,
    ...overrides,
  };
}

function canonicalize(value) {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonicalize(value[key])]),
    );
  }
  return value;
}

function receiptHash(receipt) {
  const { evidence_sha256: _evidenceSha256, ...payload } = receipt;
  return createHash("sha256")
    .update(JSON.stringify(canonicalize(payload)))
    .digest("hex");
}

test("gives the three-stage native transform one bounded evidence budget", () => {
  assert.equal(
    nativeEvidenceTimeoutMs("annotation-transform", 180_000),
    45_000,
  );
  assert.equal(nativeEvidenceTimeoutMs("annotation-transform", 30_000), 30_000);
  assert.equal(nativeEvidenceTimeoutMs("annotation-create", 180_000), 15_000);
});

test("GPUI accepts unavailable local NVIDIA diagnostics but rejects required missing evidence", () => {
  assert.equal(
    gpuiGpuEvidencePassed({
      qualification: { required: false, passed: true },
    }),
    true,
  );
  assert.equal(
    gpuiGpuEvidencePassed({
      qualification: { required: true, passed: false },
    }),
    false,
  );
  assert.equal(gpuiGpuEvidencePassed(null), false);
});

test("GPUI native timing accepts only platform-draw histogram receipts", () => {
  assert.deepEqual(
    gpuiNativeApplicationAckSamples([
      {
        events: [
          {
            event: "native-application-draw-acknowledgement",
            input_latency_samples_before: 2,
            input_latency_samples_after: 3,
            input_to_application_draw_ack_p95_ns: 12_500_000,
            gpui_platform_draw_submitted: true,
            physical_scanout_observed: false,
          },
        ],
      },
    ]),
    [12.5],
  );
  assert.deepEqual(
    gpuiNativeApplicationAckSamples([
      {
        events: [
          {
            event: "native-application-draw-acknowledgement",
            input_latency_samples_before: 2,
            input_latency_samples_after: 2,
            input_to_application_draw_ack_p95_ns: 12_500_000,
            gpui_platform_draw_submitted: true,
            physical_scanout_observed: false,
          },
        ],
      },
    ]),
    [],
  );
});

test("accepts editor-create in the native X11 replay lane", () => {
  const options = parseArguments([
    "--scenario",
    "editor-create",
    "--pdf",
    "/tmp/public-fixture.pdf",
    "--input-lane",
    "native-x11-xtest",
  ]);
  assert.equal(options.scenario, "editor-create");
  assert.equal(options.inputLane, "native-x11-xtest");
});

test("accepts open-pdf in the native X11 viewer evidence lane", () => {
  const options = parseArguments([
    "--scenario",
    "open-pdf",
    "--pdf",
    "/tmp/public-fixture.pdf",
    "--input-lane",
    "native-x11-xtest",
  ]);
  assert.equal(options.scenario, "open-pdf");
  assert.equal(options.inputLane, "native-x11-xtest");
});

test("opts into strict Longbridge compatibility evidence with one reviewed profile", () => {
  const options = parseArguments([
    "--scenario",
    "open-pdf",
    "--pdf",
    "/tmp/public-fixture.pdf",
    "--compat-profile",
    longbridgeCompatProfile,
    "--input-lane",
    "native-x11-xtest",
    "--v4-scenario",
    "small-shell-open",
    "--evidence-directory",
    "/tmp/longbridge-evidence",
  ]);
  assert.equal(options.compatProfile, longbridgeCompatProfile);
  assert.throws(
    () =>
      parseArguments([
        "--scenario",
        "open-pdf",
        "--pdf",
        "/tmp/public-fixture.pdf",
        "--compat-profile",
        "moving-main",
      ]),
    /unknown --compat-profile/,
  );
});

test("strict event provenance is opt-in and fails the Longbridge iteration on reserved-field drift", () => {
  const events = [
    {
      schema_version: 1,
      runtime: "electron",
      scenario: "open-pdf",
      event: "scenario-complete",
      t_ms: 1,
      pid: 4242,
    },
  ];
  assert.equal(
    compatEvidenceValidationForIteration(undefined, events, {
      scenario: "open-pdf",
      pid: 4242,
    }),
    null,
  );
  assert.equal(
    compatEvidenceValidationForIteration(longbridgeCompatProfile, events, {
      scenario: "open-pdf",
      pid: 4242,
    }).passed,
    false,
  );
});

test("Longbridge success requires the app cleanup receipt and an empty removed cgroup", () => {
  const cleanupEvent = {
    event: "resource-cleanup-complete",
    worker_exited: true,
    mapped_surfaces_released: true,
  };
  assert.deepEqual(
    compatResourceCleanupForIteration(longbridgeCompatProfile, [cleanupEvent], {
      removed: true,
    }),
    { passed: true, errors: [] },
  );
  assert.equal(
    compatResourceCleanupForIteration(longbridgeCompatProfile, [], {
      removed: true,
    }).passed,
    false,
  );
  assert.equal(
    compatResourceCleanupForIteration(longbridgeCompatProfile, [cleanupEvent], {
      removed: false,
      reason: "cgroup-v2-cleanup-failed: busy",
    }).passed,
    false,
  );
  assert.equal(
    compatResourceCleanupForIteration(undefined, [], { removed: false }),
    null,
  );
});

test("accepts annotation-transform in the native X11 replay lane", () => {
  const options = parseArguments([
    "--scenario",
    "annotation-transform",
    "--pdf",
    "/tmp/public-fixture.pdf",
    "--input-lane",
    "native-x11-xtest",
  ]);
  assert.equal(options.scenario, "annotation-transform");
  assert.equal(options.inputLane, "native-x11-xtest");
});

test("accepts exactly four ordered PDFs for the native multi-document lane", () => {
  const options = parseArguments([
    "--scenario",
    "multi-document-session",
    "--input-lane",
    "native-x11-xtest",
    ...["small", "nasa", "engineering", "dense"].flatMap((name) => [
      "--pdf",
      `/tmp/${name}.pdf`,
    ]),
  ]);
  assert.deepEqual(
    options.pdfs.map((path) => path.split("/").at(-1)),
    ["small.pdf", "nasa.pdf", "engineering.pdf", "dense.pdf"],
  );
  assert.throws(
    () =>
      parseArguments([
        "--scenario",
        "multi-document-session",
        "--pdf",
        "/tmp/only-one.pdf",
      ]),
    /exactly four ordered --pdf/,
  );
});

test("validates the multi-document fixture hashes in frozen order", () => {
  const contract = {
    scenario: "multi-document-session",
    fixture_ids: ["small", "nasa", "engineering", "dense"],
    fixture_sha256_by_id: {
      small: "a",
      nasa: "b",
      engineering: "c",
      dense: "d",
    },
  };
  assert.equal(
    validateOrderedScenarioFixtures(
      ["a", "b", "c", "d"].map((sha256) => ({ sha256 })),
      contract,
    ),
    null,
  );
  assert.match(
    validateOrderedScenarioFixtures(
      ["b", "a", "c", "d"].map((sha256) => ({ sha256 })),
      contract,
    ),
    /small at ordered --pdf index 0/,
  );
});

test("builds passing v5 multi-document receipts only from exact live and native evidence", () => {
  const commands = [
    {
      id: "session:open-four-fixtures",
      expected_milestones: ["opened"],
    },
    {
      id: "session:switch-four-fixtures",
      input_lane: "native-replay",
      expected_milestones: ["switched"],
    },
    {
      id: "session:edit-dense-rectangle",
      input_lane: "native-replay",
      expected_milestones: ["edited"],
    },
    {
      id: "session:close-three-and-recover",
      expected_milestones: ["closed"],
    },
  ];
  const events = commands.flatMap((command) => [
    {
      event: "comparison-milestone",
      command_id: command.id,
      milestone: command.expected_milestones[0],
    },
    {
      event: "multi-document-command-evidence",
      command_id: command.id,
    },
  ]);
  for (let actionIndex = 0; actionIndex < 4; actionIndex += 1) {
    events.push({
      event: "multi-document-native-frame-evidence",
      command_id: "session:switch-four-fixtures",
      action_index: actionIndex,
      input_latency_samples_before: actionIndex,
      input_latency_samples_after: actionIndex + 1,
      gpui_platform_draw_submitted: true,
      physical_scanout_observed: false,
    });
  }
  events.push({
    event: "multi-document-native-frame-evidence",
    command_id: "session:edit-dense-rectangle",
    action_index: 0,
    input_latency_samples_before: 4,
    input_latency_samples_after: 5,
    gpui_platform_draw_submitted: true,
    physical_scanout_observed: false,
  });
  const report = buildMultiDocumentV5Evidence(
    1,
    events,
    { commands },
    {
      inputLane: "native-x11-xtest",
      nativeReplay: { success: true, target_verified: true },
      applicationSuccess: true,
    },
  );
  assert.equal(report.passed, true);
  assert.equal(report.command_receipts.length, 4);
  assert.ok(
    report.command_receipts.every((receipt) =>
      /^[0-9a-f]{64}$/.test(receipt.evidence_sha256),
    ),
  );
  events.pop();
  assert.equal(
    buildMultiDocumentV5Evidence(
      1,
      events,
      { commands },
      {
        inputLane: "native-x11-xtest",
        nativeReplay: { success: true, target_verified: true },
        applicationSuccess: true,
      },
    ).passed,
    false,
  );
});

test("accepts only a listed component for a v4 representative scenario", async () => {
  const options = parseArguments([
    "--scenario",
    "zoom",
    "--pdf",
    "/tmp/engineering-sheet.pdf",
    "--v4-scenario",
    "engineering-sheet",
  ]);
  assert.equal(options.v4Scenario, "engineering-sheet");

  assert.throws(
    () =>
      parseArguments([
        "--scenario",
        "page-navigation",
        "--pdf",
        "/tmp/engineering-sheet.pdf",
        "--v4-scenario",
        "engineering-sheet",
      ]),
    /page-navigation is not a component/,
  );

  const workload = await loadComparisonWorkloadV4();
  const context = buildV4RunnerContext(workload, "engineering-sheet", "zoom");
  assert.equal(context.fixture_id, "bp-engineering-sheet-v1");
  assert.equal(context.component_scenario, "zoom");
  assert.equal(context.inference_eligible, true);
  assert.equal(context.execution_eligible, false);
  assert.deepEqual(fixtureIdsForLaunch(null, context), [
    "bp-engineering-sheet-v1",
  ]);
});

test("accepts v6 XDamage activation only on the native lane and matching v4 parent", () => {
  const options = parseArguments([
    "--scenario",
    "annotation-create",
    "--pdf",
    "/tmp/engineering-sheet.pdf",
    "--v4-scenario",
    "dense-mixed-editing",
    "--v6-scenario",
    "dense-mixed-editing",
    "--input-lane",
    "native-x11-xtest",
  ]);
  assert.equal(options.v6Scenario, "dense-mixed-editing");
  assert.throws(
    () =>
      parseArguments([
        "--scenario",
        "annotation-create",
        "--pdf",
        "/tmp/engineering-sheet.pdf",
        "--v4-scenario",
        "dense-mixed-editing",
        "--v6-scenario",
        "small-plan-review",
        "--input-lane",
        "native-x11-xtest",
      ]),
    /must match/,
  );
});

test("prefers ordered v5 fixture ids when both comparison contexts exist", () => {
  assert.deepEqual(
    fixtureIdsForLaunch(
      { fixture_ids: ["small", "dense"] },
      { fixture_id: "legacy" },
    ),
    ["small", "dense"],
  );
  assert.equal(fixtureIdsForLaunch(null, null), null);
});

test("accepts the two local engineering v4 components only in their parent scenario", () => {
  for (const scenario of ["fit-modes", "cache-pressure-recovery"]) {
    const options = parseArguments([
      "--scenario",
      scenario,
      "--pdf",
      "/tmp/engineering-sheet.pdf",
      "--v4-scenario",
      "engineering-sheet",
    ]);
    assert.equal(options.scenario, scenario);
    assert.equal(options.v4Scenario, "engineering-sheet");
  }
  assert.throws(
    () =>
      parseArguments([
        "--scenario",
        "fit-modes",
        "--pdf",
        "/tmp/engineering-sheet.pdf",
      ]),
    /requires --v4-scenario engineering-sheet/,
  );
});

test("builds stable live command receipts only from exact GPUI v4 evidence", () => {
  const context = {
    scenario: "engineering-sheet",
    component_scenario: "fit-modes",
    commands: [
      {
        id: "engineering:fit-modes",
        expected_milestones: [
          "fit-state-current",
          "visible-tiles-bounded",
          "settled-density-at-least-1",
        ],
      },
    ],
  };
  const events = [
    {
      event: "comparison-v4-command-receipt",
      command_id: "engineering:fit-modes",
      component_scenario: "fit-modes",
      passed: true,
      observations: [
        {
          mode: "fit-page",
          shell_width: 1200,
          shell_height: 800,
          current_generation_presented: true,
        },
        {
          mode: "fit-width",
          shell_width: 1200,
          shell_height: 800,
          current_generation_presented: true,
        },
      ],
      milestone_ids: [
        "fit-state-current",
        "visible-tiles-bounded",
        "settled-density-at-least-1",
      ],
    },
    { event: "scenario-complete" },
  ];

  const result = buildV4ComponentReceipts(3, events, context);

  assert.equal(result.passed, true);
  assert.equal(result.receipts.length, 1);
  assert.equal(result.receipts[0].command_id, "engineering:fit-modes");
  assert.equal(result.receipts[0].live, true);
  assert.equal(result.receipts[0].passed, true);
  assert.equal(result.receipts[0].proof_class, "live-app-semantic-exact");
  assert.deepEqual(result.receipts[0].milestone_ids, [
    "fit-state-current",
    "visible-tiles-bounded",
    "settled-density-at-least-1",
  ]);
  assert.deepEqual(result.receipts[0].evidence_event_indexes, [0]);
  assert.deepEqual(result.receipts[0].blockers, []);
  assert.match(result.receipts[0].evidence_sha256, /^[0-9a-f]{64}$/);

  const incomplete = buildV4ComponentReceipts(
    3,
    [
      {
        ...events[0],
        observations: events[0].observations.slice(0, 1),
      },
      events[1],
    ],
    context,
  );
  assert.equal(incomplete.passed, false);
  assert.equal(incomplete.receipts.length, 1);
  assert.equal(incomplete.receipts[0].passed, false);
});

test("v4 continuous scroll replaces only the frozen v3 blank-frame acceptance", async () => {
  const v3 = await loadComparisonWorkload();
  const v4 = await loadComparisonWorkloadV4();
  const context = buildV4RunnerContext(
    v4,
    "nasa-long-document",
    "continuous-scroll",
  );
  const development = applyV4ComponentExecutionContract(
    {
      input_lane: "native-x11-xtest",
      commands: [
        v3.journeys
          .find(({ id }) => id === "viewer-v1")
          .commands.find(({ id }) => id === "viewer:continuous-scroll"),
      ],
    },
    context,
  );
  assert.deepEqual(development.commands[0].expected_milestones, [
    "timestamped-input-complete",
    "virtual-page-window-bounded",
    "finish-page-current",
    "visible-raster-readiness-observed",
  ]);

  const baseEvents = [
    ...development.commands[0].expected_milestones.map((milestone) => ({
      event: "comparison-milestone",
      command_id: "viewer:continuous-scroll",
      milestone,
    })),
    { event: "scenario-complete" },
  ];
  const nativeEvidence = {
    applicationSuccess: true,
    inputLane: "native-x11-xtest",
    nativeReplay: { success: true, target_verified: true },
  };
  assert.equal(
    buildV4ComponentReceipts(1, baseEvents, context, nativeEvidence).passed,
    false,
  );

  const readiness = {
    event: "comparison-raster-readiness-observed",
    command_id: "viewer:continuous-scroll",
    raster_observation_count: 120,
    missing_raster_observation_count: 17,
    readiness_rate: 103 / 120,
  };
  const passed = buildV4ComponentReceipts(
    1,
    [...baseEvents.slice(0, -1), readiness, baseEvents.at(-1)],
    context,
    nativeEvidence,
  );
  assert.equal(passed.passed, true);
  assert.deepEqual(passed.receipts[0].claims.visible_raster_readiness, {
    raster_observation_count: 120,
    missing_raster_observation_count: 17,
    readiness_rate: 103 / 120,
    acceptance_role: "diagnostic-counts-and-rate",
  });
});

test("retains exact v4 receipts at the paired-runner report boundary", () => {
  const context = {
    scenario: "engineering-sheet",
    component_scenario: "fit-modes",
  };
  const receipt = {
    iteration: 1,
    passed: true,
    receipts: [
      { command_id: "engineering:fit-modes", evidence_sha256: "a".repeat(64) },
    ],
  };
  const report = buildV4ComparisonReport(context, [
    { success: true, v4_component_receipts: receipt },
  ]);

  assert.deepEqual(report.command_receipts_by_iteration, [receipt]);
  assert.equal(report.component_receipts_passed, true);
  assert.equal(report.live_component_passed, true);
});

test("small open emits exact native shell and fixed-crop registered receipts", async () => {
  const workload = await loadComparisonWorkloadV4();
  const context = buildV4RunnerContext(
    workload,
    "small-shell-open",
    "open-pdf",
  );
  const events = [
    { event: "process-main-enter" },
    {
      event: "viewer-native-launch-evidence",
      native_input_observed: true,
      gpui_platform_draw_submitted: true,
      interactive_shell: true,
    },
    { event: "pdf-open-completed", pages: 1 },
    {
      event: "viewport-raster-completed",
      surface_kind: "in-memory-bgra",
      pixel_width: 612,
      rendered_device_pixel_ratio: 1,
    },
    {
      event: "viewer-native-open-evidence",
      document_opened: true,
      preview_current_generation: true,
      settled_current_generation_ms: 250,
      requested_open_generation: 2,
      completed_open_generation: 2,
      preview_generation: 3,
    },
    { event: "scenario-complete" },
  ];
  const result = buildV4ComponentReceipts(1, events, context, {
    applicationSuccess: true,
    inputLane: "native-x11-xtest",
    nativeReplay: { success: true, target_verified: true },
  });
  assert.equal(result.passed, true);
  assert.deepEqual(
    result.receipts.map(({ command_id: commandId }) => commandId),
    ["small:launch-cold", "small:open-settle"],
  );
  assert(
    result.receipts.every(
      ({ native_input_eligible: eligible }) => eligible === true,
    ),
  );
  assert.equal(
    result.receipts[1].claims.fixed_crop_registration.crop_oracle_sha256,
    "cc231d7d5da2ef403509e58565a19fb1855fea3da6aca1436d56dbc38ce218ef",
  );
  assert.equal(result.receipts[1].longbridge_parity_eligible, false);
  assert(
    result.receipts.every(
      (receipt) => receipt.evidence_sha256 === receiptHash(receipt),
    ),
  );
});

test("historical component story shape remains explicitly ineligible for Longbridge crop parity", async () => {
  const context = buildV4RunnerContext(
    await loadComparisonWorkloadV4(),
    "small-shell-open",
    "open-pdf",
  );
  const common = { schema_version: 1, runtime: "gpui", scenario: "open-pdf" };
  const events = [
    { ...common, event: "process-main-enter" },
    {
      ...common,
      event: "viewer-native-launch-evidence",
      command_id: "small:launch-cold",
      native_input_observed: true,
      gpui_platform_draw_submitted: true,
      interactive_shell: true,
    },
    { ...common, event: "pdf-open-completed", pages: 1, worker_pid: 9001 },
    {
      ...common,
      event: "viewport-raster-completed",
      surface_kind: "in-memory-bgra",
      pixel_width: 800,
      pixel_height: 1035,
      pixel_bytes: 3312000,
      rendered_device_pixel_ratio: 1,
    },
    {
      ...common,
      event: "viewer-native-open-evidence",
      command_id: "small:open-settle",
      document_opened: true,
      preview_current_generation: true,
      settled_current_generation_ms: 250,
      requested_open_generation: 7,
      completed_open_generation: 7,
      preview_generation: 3,
    },
    { ...common, event: "scenario-complete" },
  ];
  const nativeEvidence = {
    applicationSuccess: true,
    inputLane: "native-x11-xtest",
    nativeReplay: { success: true, target_verified: true },
  };

  const historical = buildV4ComponentReceipts(
    1,
    events,
    context,
    nativeEvidence,
  );
  assert.equal(historical.passed, true);
  assert.equal(historical.receipts[1].longbridge_parity_eligible, false);
  assert.equal(
    buildV4ComponentReceipts(
      1,
      events.filter(({ event }) => event !== "viewport-raster-completed"),
      context,
      nativeEvidence,
    ).passed,
    false,
  );
  assert.equal(
    buildV4ComponentReceipts(
      1,
      events.map((event) =>
        event.event === "viewer-native-open-evidence"
          ? { ...event, completed_open_generation: 8 }
          : event,
      ),
      context,
      nativeEvidence,
    ).passed,
    false,
  );
  assert.equal(
    buildV4ComponentReceipts(
      1,
      [...events, events.find(({ event }) => event === "pdf-open-completed")],
      context,
      nativeEvidence,
    ).passed,
    false,
  );
});

test("Longbridge profile refuses fixed-crop parity without a real registered presented crop", async () => {
  const context = buildV4RunnerContext(
    await loadComparisonWorkloadV4(),
    "small-shell-open",
    "open-pdf",
  );
  const common = {
    schema_version: 1,
    runtime: "gpui",
    scenario: "open-pdf",
    pid: 4242,
  };
  const events = [
    { ...common, event: "process-main-enter", t_ms: 0 },
    {
      ...common,
      event: "viewer-native-launch-evidence",
      t_ms: 10,
      native_input_observed: true,
      gpui_platform_draw_submitted: true,
      interactive_shell: true,
    },
    { ...common, event: "pdf-open-completed", t_ms: 20, pages: 1 },
    {
      ...common,
      event: "viewport-raster-completed",
      t_ms: 30,
      surface_kind: "in-memory-bgra",
      pixel_width: 800,
      rendered_device_pixel_ratio: 1,
    },
    {
      ...common,
      event: "viewer-native-open-evidence",
      t_ms: 280,
      document_opened: true,
      preview_current_generation: true,
      settled_current_generation_ms: 250,
      requested_open_generation: 7,
      completed_open_generation: 7,
      preview_generation: 3,
    },
    { ...common, event: "scenario-complete", t_ms: 281 },
  ];
  const evidence = {
    applicationSuccess: true,
    inputLane: "native-x11-xtest",
    nativeReplay: { success: true, target_verified: true },
    compatProfile: longbridgeCompatProfile,
  };

  assert.equal(
    buildV4ComponentReceipts(1, events, context, evidence).passed,
    false,
  );

  const cropReceipt = {
    ...common,
    event: "compat-presented-crop-evidence",
    t_ms: 280.5,
    command_id: "small:open-settle",
    fixture_id: "bp-single-page-v1",
    crop_id: "single-registration",
    page_id: "bp-single-page-v1:page:001",
    registration_sha256:
      "cc231d7d5da2ef403509e58565a19fb1855fea3da6aca1436d56dbc38ce218ef",
    acceptance_source: "XGetImage-presented-client-drawable",
    candidate_resampled: false,
    presented_drawable_artifact_sha256: "c".repeat(64),
    retained_ppm_sha256: "c".repeat(64),
    candidate_crop_sha256: "a".repeat(64),
    registered_reference_crop_sha256: "a".repeat(64),
    candidate_dimensions: { width: 540, height: 720 },
    mapped_bounds_pixels: { x: 100, y: 40, width: 540, height: 720 },
    extracted_bounds_pixels: { x: 100, y: 40, width: 540, height: 720 },
    page_size_points: { width: 612, height: 792 },
    pdf_rect: { x: 36, y: 36, width: 540, height: 720 },
    rendered_device_pixel_ratio: 1,
    display_scale_factor: 1,
    painted_render_generation: 4,
    painted_generation_stable: true,
    exact_pixel_match: true,
  };
  evidence.nativeReplay.driver_evidence = {
    compat_presented_crop: cropReceipt,
  };
  const accepted = buildV4ComponentReceipts(1, events, context, evidence);
  assert.equal(accepted.passed, true);
  assert.equal(
    events.some(({ event }) => event === "compat-presented-crop-evidence"),
    false,
  );
  assert.equal(
    accepted.receipts[1].claims.fixed_crop_registration.candidate_crop_sha256,
    "a".repeat(64),
  );
  assert.equal(accepted.receipts[1].longbridge_parity_eligible, true);
});

test("NASA cache translates the exact retained atlas submission byte observation", async () => {
  const workload = await loadComparisonWorkloadV4();
  const context = buildV4RunnerContext(
    workload,
    "nasa-long-document",
    "cache-pressure",
  );
  const sourceCommand = "viewer:cache-pressure";
  const events = [
    ...[
      "declared-cache-byte-limit-held",
      "decoded-byte-limit-held",
      "upload-byte-count-recorded",
    ].map((milestone) => ({
      event: "comparison-milestone",
      command_id: sourceCommand,
      milestone,
    })),
    {
      event: "comparison-tile-atlas-upload-bytes",
      bytes: 8_388_608,
      evidence_kind: "gpui-wgpu-paint-image-atlas-upload-queued",
      physical_bus_upload_bytes: null,
    },
    { event: "scenario-complete" },
  ];
  const result = buildV4ComponentReceipts(1, events, context);
  assert.equal(result.passed, true);
  assert.deepEqual(result.receipts[0].milestone_ids, [
    "declared-cache-byte-limit-held",
    "decoded-byte-limit-held",
    "renderer-resource-submission-bytes-exact",
  ]);
  assert.equal(
    result.receipts[0].claims.renderer_resource_submission.bytes,
    8_388_608,
  );
  assert.equal(result.receipts[0].native_input_eligible, false);
});

test("engineering zoom and pan fail closed without generation and timing observations", async () => {
  const workload = await loadComparisonWorkloadV4();
  const zoomContext = buildV4RunnerContext(
    workload,
    "engineering-sheet",
    "zoom",
  );
  const zoomMilestones = [
    "zoom-state-current",
    "visible-tiles-bounded",
    "preview-current-generation",
    "settled-density-at-least-1",
  ].map((milestone) => ({
    event: "comparison-milestone",
    command_id: "viewer:zoom-sequence",
    milestone,
  }));
  const zoomEvents = [
    ...zoomMilestones,
    ...[100, 200, 400, 800, 1600, 400, 100, 800, 200, 100, 1200, 100].map(
      (zoom, stepIndex) => ({
        event: "comparison-v4-zoom-generation-evidence",
        command_id: "viewer:zoom-sequence",
        step_index: stepIndex,
        zoom_percent: zoom,
        visible_tile_count: 4,
        maximum_visible_tiles: 32,
        stale_visible_generation_count: 0,
      }),
    ),
    { event: "scenario-complete" },
  ];
  const zoom = buildV4ComponentReceipts(1, zoomEvents, zoomContext);
  assert.equal(zoom.passed, true);
  assert(
    zoom.receipts[0].milestone_ids.includes("stale-generations-presented-zero"),
  );
  const missingZoom = buildV4ComponentReceipts(
    1,
    [...zoomMilestones, { event: "scenario-complete" }],
    zoomContext,
  );
  assert.equal(missingZoom.passed, false);
  assert(
    missingZoom.receipts[0].blockers.includes(
      "missing-milestone:stale-generations-presented-zero",
    ),
  );

  const panContext = buildV4RunnerContext(
    workload,
    "engineering-sheet",
    "high-zoom-pan",
  );
  const panEvents = [
    ...[
      "visible-tiles-bounded",
      "stale-generations-presented-zero",
      "settled-density-at-least-1",
    ].map((milestone) => ({
      event: "comparison-milestone",
      command_id: "viewer:pan-usgs",
      milestone,
    })),
    {
      event: "comparison-v4-pan-input-evidence",
      command_id: "viewer:pan-usgs",
      timestamped_input_complete: true,
      input_samples: 181,
      expected_input_samples: 181,
      input_rate_hz: 60,
      duration_ms: 3000,
    },
    { event: "scenario-complete" },
  ];
  const pan = buildV4ComponentReceipts(1, panEvents, panContext);
  assert.equal(pan.passed, true);
  assert(pan.receipts[0].milestone_ids.includes("timestamped-input-complete"));
});

test("image creation requires exact decoded bytes, atlas submission, and later presentation", () => {
  const command = {
    id: "image:create",
    expected_milestones: [
      "bitmap-decoded",
      "decoded-payload-bytes-exact",
      "renderer-resource-submission-bytes-exact",
      "bitmap-presented-from-decoded-payload",
      "gesture-committed-once",
    ],
  };
  const context = {
    scenario: "dense-mixed-editing",
    component_scenario: "editor-create",
    component_command_ids: { "editor-create": ["image:create"] },
    commands: [command],
  };
  const events = [
    {
      event: "comparison-milestone",
      command_id: "image:create",
      milestone: "bitmap-decoded",
    },
    {
      event: "comparison-milestone",
      command_id: "image:create",
      milestone: "gesture-committed-once",
    },
    {
      event: "editor-create-presentation-frame-observed",
      report: {
        gpui_present_submission_observed: true,
        gpui_atlas_upload_bytes: 786_432,
        commands: [
          {
            command_id: "image:create",
            proven_manifest_milestones: [
              "bitmap-upload-recorded",
              "annotation-painted",
            ],
            blocked_manifest_milestones: [],
            facts: {
              decoded_render_image: {
                annotation_id: "comparison:image:1",
                render_image_id: 1,
                width_px: 512,
                height_px: 384,
                decoded_bgra_bytes: 786_432,
              },
              gpui_atlas_upload_bytes: 786_432,
            },
          },
        ],
      },
    },
    { event: "scenario-complete" },
  ];
  const result = buildV4ComponentReceipts(1, events, context, {
    applicationSuccess: true,
    inputLane: "native-x11-xtest",
    nativeReplay: { success: true, target_verified: true },
  });
  assert.equal(result.passed, true);
  assert.deepEqual(result.receipts[0].claims.image_resource, {
    index: 2,
    decoded_payload_bytes: 786_432,
    renderer_resource_submission_bytes: 786_432,
    physical_bus_upload_bytes: null,
    presented_after_submission: true,
  });
  const missingPresentation = structuredClone(events);
  missingPresentation[2].report.gpui_present_submission_observed = false;
  assert.equal(
    buildV4ComponentReceipts(1, missingPresentation, context, {
      applicationSuccess: true,
      inputLane: "native-x11-xtest",
      nativeReplay: { success: true, target_verified: true },
    }).passed,
    false,
  );
});

test("semantic editor receipts remain native-latency ineligible and require exact completion", () => {
  const command = {
    id: "highlight:edit-history",
    expected_milestones: [
      "hit-test-selected",
      "path-bounds-current",
      "undo-redo-exact",
      "thumbnail-current",
    ],
  };
  const context = {
    scenario: "dense-mixed-editing",
    component_scenario: "editor-workload",
    component_command_ids: { "editor-workload": [command.id] },
    commands: [command],
  };
  const events = [
    {
      event: "comparison-command-evidence",
      command_id: command.id,
      evidence: {
        command_id: command.id,
        proven_manifest_milestones: [...command.expected_milestones],
      },
    },
    { event: "comparison-command-complete", command_id: command.id },
    { event: "scenario-complete" },
  ];
  const result = buildV4ComponentReceipts(1, events, context);
  assert.equal(result.passed, true);
  assert.equal(result.receipts[0].proof_class, "live-app-semantic-exact");
  assert.equal(result.receipts[0].native_input_eligible, false);
  assert.equal(
    buildV4ComponentReceipts(1, events.slice(0, 1), context).passed,
    false,
  );
});

test("persistence v4 receipts require the exact retained two-cycle validator boundary", async () => {
  const workload = await loadComparisonWorkloadV4();
  const context = buildV4RunnerContext(
    workload,
    "persistence",
    "persistence-workload",
  );
  const commandIds = context.component_command_ids["persistence-workload"];
  const cycle = "1".repeat(64);
  const sourceCrop = "2".repeat(64);
  const savedCrop = "3".repeat(64);
  const receiptEvent = {
    event: "persistence-evidence-complete",
    receipt_status: "exact-passed",
    exact_receipt: {
      status: "exact-passed",
      completed_command_ids: [...commandIds],
      typed_state_exact: true,
      unknown_probes_exact: true,
      untouched_annotation_count: 2,
      independent_pdf_validation_passed: true,
      independent_visual_validation_passed: true,
      validator_receipt_count: 4,
      cycle_1_sha256: cycle,
      cycle_2_sha256: cycle,
      source_crop_sha256: sourceCrop,
      cycle_1_crop_sha256: savedCrop,
      cycle_2_crop_sha256: savedCrop,
    },
  };
  const persistenceEvidence = {
    status: "passed",
    validators: [1, 2, 3, 4].map(() => ({ passed: true })),
    artifacts: [
      { cycle: 1, sha256: cycle },
      { cycle: 2, sha256: cycle },
    ],
    visual_oracle: null,
  };
  const events = [
    receiptEvent,
    ...commandIds.map((command_id) => ({
      event: "comparison-command-complete",
      command_id,
    })),
    { event: "scenario-complete" },
  ];
  const result = buildV4ComponentReceipts(1, events, context, {
    applicationSuccess: true,
    inputLane: "semantic-diagnostic",
    persistenceEvidence,
  });
  assert.equal(result.passed, true);
  assert.equal(result.receipts.length, 8);
  assert(result.receipts.every(({ passed }) => passed));
  assert(
    result.receipts.every(
      ({ native_input_eligible: eligible }) => eligible === false,
    ),
  );
  assert(
    result.receipts.every(
      (receipt) => receipt.evidence_sha256 === receiptHash(receipt),
    ),
  );

  persistenceEvidence.validators[0].passed = false;
  const failed = buildV4ComponentReceipts(1, events, context, {
    applicationSuccess: true,
    inputLane: "semantic-diagnostic",
    persistenceEvidence,
  });
  assert.equal(failed.passed, false);
  assert(failed.receipts.every(({ passed }) => passed === false));
});

test("accepts only an error-free successful terminal event", () => {
  assert.equal(
    scenarioSucceeded(evidence([{ event: "scenario-complete" }])),
    true,
  );
});

test("rejects scenario errors even if a later completion event exists", () => {
  assert.equal(
    scenarioSucceeded(
      evidence([{ event: "scenario-error" }, { event: "scenario-complete" }]),
    ),
    false,
  );
});

test("rejects missing completion, process failure, timeout, and malformed output", () => {
  assert.equal(scenarioSucceeded(evidence([])), false);
  assert.equal(
    scenarioSucceeded(
      evidence([{ event: "scenario-complete" }], {
        outcome: { exit_code: 1, spawn_error: null },
      }),
    ),
    false,
  );
  assert.equal(
    scenarioSucceeded(
      evidence([{ event: "scenario-complete" }], { timedOut: true }),
    ),
    false,
  );
  assert.equal(
    scenarioSucceeded(
      evidence([{ event: "scenario-complete" }], {
        invalidStdout: ["not-json"],
      }),
    ),
    false,
  );
});

test("expanded scenarios reject the wrong fixture bytes before launch", () => {
  const contract = {
    fixture_id: "bp-annotation-density-v1",
    fixture_sha256: "expected",
  };
  assert.equal(validateScenarioFixture({ sha256: "expected" }, contract), null);
  assert.match(
    validateScenarioFixture({ sha256: "wrong" }, contract),
    /bp-annotation-density-v1 requires PDF SHA-256 expected; received wrong/,
  );
  assert.equal(validateScenarioFixture({ sha256: "anything" }, null), null);
});

test("names an absent locked public corpus as blocked, not a runner failure", () => {
  assert.match(
    formatFixtureAccessError(
      new Error("ENOENT"),
      { fixture_id: "usgs-usa-geology-sheet-v1" },
      "/cache/usgs.pdf",
    ),
    /^BLOCKED locked corpus usgs-usa-geology-sheet-v1 is absent at \/cache\/usgs\.pdf/,
  );
  assert.equal(
    formatFixtureAccessError(new Error("denied"), null, "/cache/other.pdf"),
    "denied",
  );
});

test("expanded scenarios require every exact command milestone", () => {
  const contract = {
    input_lane: "semantic-diagnostic",
    commands: [
      {
        id: "rectangle:create-sparse",
        expected_milestones: ["pointer-stream-received"],
      },
      { id: "highlight:create", expected_milestones: ["path-smoothed"] },
    ],
  };
  const events = [
    { event: "scenario-lane", input_lane: "semantic-diagnostic" },
    {
      event: "comparison-milestone",
      command_id: "rectangle:create-sparse",
      milestone: "pointer-stream-received",
    },
    {
      event: "comparison-milestone",
      command_id: "highlight:create",
      milestone: "path-smoothed",
    },
  ];
  assert.equal(comparisonMilestonesSucceeded(events, contract), true);
  assert.equal(
    comparisonMilestonesSucceeded(events.slice(0, 2), contract),
    false,
  );
  assert.equal(
    scenarioSucceeded({
      ...evidence([...events, { event: "scenario-complete" }]),
      comparisonContract: contract,
    }),
    true,
  );
  assert.equal(
    scenarioSucceeded({
      ...evidence([events[0], events[1], { event: "scenario-complete" }]),
      comparisonContract: contract,
    }),
    false,
  );
});

test("semantic workload diagnostics require every command without claiming manifest milestones", () => {
  const contract = {
    input_lane: "semantic-diagnostic",
    semantic_command_only: true,
    command_ids: ["text:create", "persistence:save-1"],
    commands: [],
  };
  const events = [
    { event: "scenario-lane", input_lane: "semantic-diagnostic" },
    { event: "comparison-command-complete", command_id: "text:create" },
    { event: "comparison-command-complete", command_id: "persistence:save-1" },
  ];
  assert.equal(comparisonMilestonesSucceeded(events, contract), true);
  assert.equal(
    comparisonMilestonesSucceeded(events.slice(0, 2), contract),
    false,
  );
});

test("GPUI native timing stays ineligible until exact replay and full feature coverage", async () => {
  const workload = await loadComparisonWorkload();
  const exact = {
    success: true,
    application_success: true,
    native_input: { evidence: { success: true, target_verified: true } },
  };
  const metadata = gpuiComparisonMetadata(
    workload,
    "annotation-create",
    "native-x11-xtest",
    [exact],
  );
  assert.equal(metadata.execution_lane, "native-x11-xtest");
  assert.equal(metadata.diagnostic_timing_eligible, true);
  assert.equal(metadata.feature_coverage.ready, false);
  assert.equal(metadata.decision_timing_eligible, false);
  assert.equal(
    metadata.blocked_reason,
    "full-comparison-feature-coverage-incomplete",
  );

  const scrollMetadata = gpuiComparisonMetadata(
    workload,
    "continuous-scroll",
    "native-x11-xtest",
    [exact],
  );
  assert.equal(scrollMetadata.diagnostic_timing_eligible, true);
  assert.equal(scrollMetadata.decision_timing_eligible, false);
  assert.equal(
    scrollMetadata.blocked_reason,
    "full-comparison-feature-coverage-incomplete",
  );

  const lane = qualifyNativeLaneMetadata(
    {
      success: true,
      application_success: true,
      target_verified: true,
    },
    false,
  );
  assert.equal(lane.execution_lane, "native-x11-xtest");
  assert.equal(lane.diagnostic_timing_eligible, true);
  assert.equal(lane.decision_timing_eligible, false);
});

test("persistence diagnostics report both cycle hashes and validator outputs before cleanup", async () => {
  const directory = await mkdtemp(
    resolve(tmpdir(), "bp-persistence-evidence-"),
  );
  try {
    await writeFile(resolve(directory, "cycle-1.pdf"), "cycle one");
    await writeFile(resolve(directory, "cycle-2.pdf"), "cycle two");
    const evidence = await collectPersistenceEvidence(directory);
    assert.equal(evidence.evidence_class, "diagnostic-only");
    assert.equal(evidence.decision_timing_eligible, false);
    assert.equal(evidence.artifacts_retained, false);
    assert.deepEqual(
      evidence.artifacts.map(({ cycle, bytes }) => ({ cycle, bytes })),
      [
        { cycle: 1, bytes: 9 },
        { cycle: 2, bytes: 9 },
      ],
    );
    assert.equal(
      evidence.artifacts.every(({ sha256 }) => /^[0-9a-f]{64}$/.test(sha256)),
      true,
    );
    assert.deepEqual(
      evidence.validators.map(({ cycle, validator }) => ({ cycle, validator })),
      [
        { cycle: 1, validator: "qpdf" },
        { cycle: 1, validator: "pdfinfo" },
        { cycle: 2, validator: "qpdf" },
        { cycle: 2, validator: "pdfinfo" },
      ],
    );
    assert.equal(
      evidence.validators.every(
        (validator) => typeof validator.stdout === "string",
      ),
      true,
    );
    assert.equal(
      evidence.validators.every(
        (validator) => typeof validator.stderr === "string",
      ),
      true,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("persistence evidence directory is opt-in and restricted to its semantic scenario", () => {
  const options = parseArguments([
    "--scenario",
    "persistence-workload",
    "--pdf",
    "/tmp/public-fixture.pdf",
    "--evidence-directory",
    "/tmp/bp-retained-evidence",
  ]);
  assert.equal(options.evidenceDirectory, resolve("/tmp/bp-retained-evidence"));
  assert.throws(
    () =>
      parseArguments([
        "--scenario",
        "open-pdf",
        "--pdf",
        "/tmp/public-fixture.pdf",
        "--evidence-directory",
        "/tmp/bp-retained-evidence",
      ]),
    /--evidence-directory is implemented only for persistence-workload/,
  );
});

test("opt-in persistence collection retains PDF and fixed raster identities", async () => {
  const directory = await mkdtemp(
    resolve(tmpdir(), "bp-retained-persistence-evidence-"),
  );
  try {
    await writeFile(resolve(directory, "cycle-1.pdf"), "cycle one");
    await writeFile(resolve(directory, "cycle-2.pdf"), "cycle two");
    await writeFile(resolve(directory, "source-crop.ppm"), "source crop");
    await writeFile(resolve(directory, "cycle-1-crop.ppm"), "saved crop");
    await writeFile(resolve(directory, "cycle-2-crop.ppm"), "saved crop");

    const evidence = await collectPersistenceEvidence(directory, {
      artifactsRetained: true,
    });
    assert.equal(evidence.evidence_class, "diagnostic-only");
    assert.equal(evidence.decision_timing_eligible, false);
    assert.equal(evidence.artifacts_retained, true);
    assert.equal(evidence.artifacts.length, 5);
    assert.equal(evidence.visual_oracle.status, "passed");
    assert.equal(
      evidence.visual_oracle.cycle_1_crop_sha256,
      evidence.visual_oracle.cycle_2_crop_sha256,
    );
    assert.notEqual(
      evidence.visual_oracle.source_crop_sha256,
      evidence.visual_oracle.cycle_1_crop_sha256,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("exact persistence receipt requires two stable cycles and matching runner artifacts", () => {
  const cycle1 = "1".repeat(64);
  const cycle2 = "2".repeat(64);
  const sourceCrop = "3".repeat(64);
  const savedCrop = "4".repeat(64);
  const event = {
    event: "persistence-evidence-complete",
    receipt_status: "exact-passed",
    exact_receipt: {
      status: "exact-passed",
      completed_command_ids: [
        "unknown:import",
        "unknown:assert-cycle-1",
        "unknown:assert-cycle-2",
        "persistence:apply-fixed-state",
        "persistence:save-1",
        "persistence:reopen-1",
        "persistence:save-2",
        "persistence:reopen-2",
      ],
      typed_state_exact: true,
      unknown_probes_exact: true,
      untouched_annotation_count: 2,
      independent_pdf_validation_passed: true,
      independent_visual_validation_passed: true,
      validator_receipt_count: 4,
      cycle_1_sha256: cycle1,
      cycle_2_sha256: cycle2,
      source_crop_sha256: sourceCrop,
      cycle_1_crop_sha256: savedCrop,
      cycle_2_crop_sha256: savedCrop,
      artifacts_retained: false,
    },
  };
  const collected = {
    status: "passed",
    validators: [1, 2, 3, 4].map(() => ({ passed: true })),
    artifacts: [
      { cycle: 1, sha256: cycle1 },
      { cycle: 2, sha256: cycle2 },
    ],
    visual_oracle: null,
  };

  assert.equal(exactPersistenceReceiptSucceeded([event], collected), true);
  event.exact_receipt.cycle_2_crop_sha256 = "5".repeat(64);
  assert.equal(exactPersistenceReceiptSucceeded([event], collected), false);
});

test("persistence success requires its exact receipt, validators, and every command completion", () => {
  const cycle = "1".repeat(64);
  const sourceCrop = "2".repeat(64);
  const savedCrop = "3".repeat(64);
  const commandIds = [
    "unknown:import",
    "unknown:assert-cycle-1",
    "unknown:assert-cycle-2",
    "persistence:apply-fixed-state",
    "persistence:save-1",
    "persistence:reopen-1",
    "persistence:save-2",
    "persistence:reopen-2",
  ];
  const receipt = {
    event: "persistence-evidence-complete",
    receipt_status: "exact-passed",
    exact_receipt: {
      status: "exact-passed",
      completed_command_ids: commandIds,
      typed_state_exact: true,
      unknown_probes_exact: true,
      untouched_annotation_count: 2,
      independent_pdf_validation_passed: true,
      independent_visual_validation_passed: true,
      validator_receipt_count: 4,
      cycle_1_sha256: cycle,
      cycle_2_sha256: cycle,
      source_crop_sha256: sourceCrop,
      cycle_1_crop_sha256: savedCrop,
      cycle_2_crop_sha256: savedCrop,
    },
  };
  const collectedEvidence = {
    status: "passed",
    validators: [1, 2, 3, 4].map(() => ({ passed: true })),
    artifacts: [
      { cycle: 1, sha256: cycle },
      { cycle: 2, sha256: cycle },
    ],
    visual_oracle: null,
  };
  const events = [
    { event: "scenario-lane", input_lane: "semantic-diagnostic" },
    receipt,
    ...commandIds.map((command_id) => ({
      event: "comparison-command-complete",
      command_id,
    })),
    { event: "scenario-complete" },
  ];
  const input = {
    timedOut: false,
    outcome: { exit_code: 0, spawn_error: null },
    invalidStdout: [],
    events,
    comparisonContract: { input_lane: "semantic-diagnostic" },
    collectedEvidence,
  };
  assert.equal(persistenceScenarioSucceeded(input), true);
  assert.equal(
    persistenceScenarioSucceeded({
      ...input,
      events: events.filter(
        ({ command_id: commandId }) => commandId !== "persistence:reopen-2",
      ),
    }),
    false,
  );
  assert.equal(
    persistenceScenarioSucceeded({
      ...input,
      collectedEvidence: {
        ...collectedEvidence,
        validators: [
          { passed: false },
          ...collectedEvidence.validators.slice(1),
        ],
      },
    }),
    false,
  );
  assert.equal(
    persistenceScenarioSucceeded({
      ...input,
      events: [...events, { event: "scenario-error" }],
    }),
    false,
  );
});
