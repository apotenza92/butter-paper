import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { JSDOM } from "jsdom";

import {
  assessCloseReopenEvidence,
  assessElectronCachePressureEvidence,
  assessElectronEditorCreateEvidence,
  assessElectronEngineeringCacheRecoveryEvidence,
  assessElectronEngineeringFitModesEvidence,
  assessElectronV4OpenEvidence,
  assessElectronNativeOpenEvidence,
  assessElectronNativePanEvidence,
  assessElectronNativeWindowFocusRestoration,
  assessElectronNativeSelectPoint,
  assessElectronNativeTransformEvidence,
  assessElectronNativePropertyEditUndoV5,
  assessElectronNativeSnapTransformV5,
  assessElectronMultiDocumentSessionV5,
  assessHighZoomPanEvidence,
  annotationGeometryTolerancePt,
  buildAnnotationPointerSamples,
  buildContinuousScrollPhases,
  buildEditorCreateHighZoomPlan,
  buildEditorCreateLengthInteractionPlan,
  buildElectronComparisonViewStateEvent,
  buildElectronSemanticPanFrames,
  buildElectronRectanglePropertiesUiPlan,
  buildElectronNativePanReplay,
  buildElectronNativePageNavigationReplay,
  buildElectronNativeViewportPanSegment,
  buildElectronNativeFileDialogCommands,
  buildElectronNativeFileDialogOpenClick,
  buildElectronNativeStableHexReplacementCommands,
  buildElectronNativeSnapReplayPlan,
  buildElectronDenseRectangleSourceContract,
  buildElectronDenseFixtureMarkups,
  buildElectronImageRendererResourceSubmissionReceipt,
  buildElectronImageResizeRendererResourceSubmissionReceipt,
  buildElectronV4ComponentEvidence,
  buildElectronV5ComponentEvidence,
  bindElectronDynamicObserverSamples,
  buildElectronSecondNasaBaselineSummary,
  createElectronV4ExecutionContext,
  createElectronComparisonViewStateCheckpointGate,
  createElectronV5ExecutionContext,
  createElectronV6ExecutionContext,
  electronDenseGeometryIndexMatchesSeed,
  buildNormalizedPointerSamples,
  comparisonMilestonesSucceeded,
  electronComparisonMetadata,
  electronHighlightSmoothingEvidence,
  electronGpuEvidencePassed,
  electronNativeApplicationAckSamples,
  electronSettledDensityIsCurrent,
  electronZoomPresetMenuReady,
  electronNativeX11Scenarios,
  electronV4ComponentLaneEligible,
  electronV5ComponentScenarios,
  electronV5MaintainedUiCapability,
  editorCreatePrecisionUiSelection,
  findVisibleElectronSelectOption,
  electronSelectTriggerRetainsValue,
  measureElectronVisibleRasterDensity,
  classifyEditorCreateBoundsContract,
  classifyElectronSecondTabRasterBlocker,
  electronCropCaptureStateStable,
  evaluateVisiblePageGeometry,
  formatFixtureAccessError,
  pdfPointToCss,
  pdfPointToNativeCss,
  parseElectronRunnerArguments,
  optimizedElectronBenchmarkEnvironment,
  rectangleMoveEdgePoint,
  replayHistoryToBoundary,
  remapElectronUntimedAction,
  sameDocumentHistoryPosition,
  resolveLabeledPropertyControl,
  validateExpandedScenarioFixture,
  validateOrderedElectronScenarioFixtures,
} from "./electron-runner.mjs";
import { buildViewStateReceiptV5 } from "./matched-view-state-v5.mjs";
import {
  loadComparisonWorkload,
  runnerComparisonMetadata,
} from "./comparison-workload.mjs";
import { loadComparisonWorkloadV4 } from "./comparison-workload-v4.mjs";
import { loadMaterializedComparisonWorkloadV5 } from "./comparison-workload-v5.mjs";
import { loadComparisonWorkloadV6 } from "./run-paired-v6.mjs";

test("Electron native timing accepts only complete trusted-event frame acknowledgements", () => {
  assert.deepEqual(
    electronNativeApplicationAckSamples([
      {
        renderer: {
          native_input_to_application_frame_ack: {
            receipt_scope:
              "trusted-dom-native-event-receipt-to-next-request-animation-frame-callback-not-physical-scanout",
            physical_scanout_observed: false,
            input_event_count: 2,
            acknowledged_event_count: 2,
            samples_ms: [4, 8],
          },
        },
      },
    ]),
    [4, 8],
  );
  assert.deepEqual(
    electronNativeApplicationAckSamples([
      {
        renderer: {
          native_input_to_application_frame_ack: {
            receipt_scope:
              "trusted-dom-native-event-receipt-to-next-request-animation-frame-callback-not-physical-scanout",
            physical_scanout_observed: false,
            input_event_count: 2,
            acknowledged_event_count: 1,
            samples_ms: [4],
          },
        },
      },
    ]),
    [],
  );
});

test("Electron optimized runs cannot inherit a renderer development server", () => {
  assert.deepEqual(
    optimizedElectronBenchmarkEnvironment({
      KEEP: "yes",
      NODE_ENV: "development",
      VITE_DEV_SERVER_URL: "http://127.0.0.1:5173",
      ELECTRON_RENDERER_URL: "http://127.0.0.1:5174",
      BP_RENDERER_DEV_SERVER_URL: "http://127.0.0.1:5175",
    }),
    {
      KEEP: "yes",
      NODE_ENV: "production",
      BP_DISABLE_RENDERER_DEV_SERVER: "1",
    },
  );
});

test("Electron comparison view state maps live tab identity and satisfies the v5 receipt", () => {
  const fixtureIds = ["single", "density"];
  const pdfPaths = ["/fixtures/single.pdf", "/fixtures/density.pdf"];
  const observed = {
    window_bounds_window_logical: { x: 0, y: 0, width: 1200, height: 800 },
    viewport_bounds_window_logical: {
      x: 356,
      y: 132,
      width: 844,
      height: 668,
    },
    display_scale_factor: 1,
    layout_mode: "single-page",
    zoom_mode: "fit-page",
    zoom_percent: 72.5,
    left_sidebar_visible: true,
    left_sidebar_width_logical: 300,
    right_sidebar_visible: false,
    right_sidebar_width_logical: 0,
    active_path: pdfPaths[1],
    tabs: [
      { filePath: pdfPaths[0], active: false },
      { filePath: pdfPaths[1], active: true },
    ],
  };
  const event = (checkpoint) => ({
    schema_version: 1,
    runtime: "electron",
    scenario: "multi-document-session",
    event: "comparison-view-state",
    ...buildElectronComparisonViewStateEvent({
      checkpoint,
      component: "multi-document-session",
      fixtureIds,
      pdfPaths,
      observed,
    }),
  });
  const assessment = buildViewStateReceiptV5(
    {
      iterations: [
        {
          events: [event("measurement-start"), event("measurement-end")],
        },
      ],
    },
    {
      implementation: "electron",
      journey: "multi-document-session",
      component: "multi-document-session",
      fixture_ids: fixtureIds,
    },
  );
  assert.equal(assessment.passed, true);
  assert.equal(
    assessment.receipt.snapshots[0].active_document.fixture_id,
    "density",
  );
  assert.equal(assessment.receipt.snapshots[0].active_document.tab_index, 1);
  assert.equal(
    assessment.receipt.snapshots[0].active_document.open_document_count,
    2,
  );
});

test("Electron comparison view-state checkpoints are exactly once and ordered", () => {
  const gate = createElectronComparisonViewStateCheckpointGate(true);
  assert.equal(gate.claim("measurement-start"), true);
  assert.equal(gate.complete(), false);
  assert.throws(
    () => gate.claim("measurement-start"),
    /must be measurement-end/,
  );
  assert.equal(gate.claim("measurement-end"), true);
  assert.equal(gate.complete(), true);
  assert.throws(() => gate.claim("measurement-end"), /must be complete/);

  const disabled = createElectronComparisonViewStateCheckpointGate(false);
  assert.equal(disabled.claim("measurement-start"), false);
  assert.equal(disabled.claim("measurement-end"), false);
  assert.equal(disabled.complete(), true);
});

test("Electron dynamic samples record fresh source age and fail closed when stale", () => {
  const command = {
    duration_ms: 50,
    observer: { rate_hz: 60, expected_sample_count: 4 },
    required_sample_fields: [
      "visible_page_ready_fraction",
      "visible_raster_ready_area_fraction",
      "visible_raster_pixel_density",
    ],
  };
  const ticks = [0, 16.666666667, 33.333333333, 50].map(
    (scheduled_offset_ms, sample_index) => ({
      sample_index,
      scheduled_offset_ms,
      observed_monotonic_ms: 100 + scheduled_offset_ms,
    }),
  );
  const states = ticks.map((tick, index) => ({
    state_sequence: index + 1,
    runner_observed_monotonic_ms: tick.observed_monotonic_ms - 2,
    active_page: 1,
    visible_page_count: 1,
    visible_pages: [
      {
        visible_intersection_area_css_px2: 100,
        current_raster_ready_area_fraction: 1,
        current_raster_device_pixels_per_css_pixel: 4,
      },
    ],
    scroll_offset_css_px: { x: 0, y: index * 10 },
    render_generation: index + 1,
  }));
  const samples = bindElectronDynamicObserverSamples(ticks, states, command);
  assert.equal(samples.length, 4);
  assert.equal(samples[3].application_state_age_ms, 2);
  assert.equal(samples[3].application_state_observed_monotonic_ms, 148);
  assert.equal(samples[3].observer_tick_actual_offset_ms, 50);
  assert.equal(samples[3].observer_tick_schedule_error_ms, 0);

  assert.throws(
    () =>
      bindElectronDynamicObserverSamples(ticks, states.slice(0, 1), command),
    /application state age exceeds 33\.333333333 ms/,
  );
});
import { buildDevelopmentScenarioContract } from "./scenario-contract.mjs";
import { buildScenarioContractV4 } from "./scenario-contract-v4.mjs";
import { canonicalSha256 } from "./run-paired-v4.mjs";
import { buildDistanceBoundedDynamicWheelPlan } from "./gpui-native-x11.mjs";

test("Electron accepts unavailable local NVIDIA diagnostics but rejects required missing evidence", () => {
  assert.equal(
    electronGpuEvidencePassed({
      qualification: { required: false, passed: true },
    }),
    true,
  );
  assert.equal(
    electronGpuEvidencePassed({
      qualification: { required: true, passed: false },
    }),
    false,
  );
  assert.equal(electronGpuEvidencePassed(null), false);
});

test("distance-bounded dynamic wheel preserves 120 Hz phases without overshooting 50 viewports", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const command = createElectronV5ExecutionContext({
    workload,
    parentScenario: "nasa-long-document",
    componentScenario: "viewer-dynamic-fidelity",
  }).execution_contract.commands[0];
  const plan = buildDistanceBoundedDynamicWheelPlan({
    command,
    viewportHeightCssPx: 600,
    wheelDeltaCssPx: 53,
  });
  assert.equal(plan.requested_forward_viewport_heights, 50);
  assert.equal(plan.requested_forward_distance_css_px, 30_000);
  assert.equal(plan.forward_event_count, 566);
  assert.equal(plan.reverse_event_count, 566);
  assert.ok(
    Math.abs(plan.distance_error_css_px) <= plan.maximum_distance_error_css_px,
  );
  assert.ok(plan.forward_event_count < 2_400);
});

test("builds the exact inclusive 120 Hz rectangle pointer stream", () => {
  const command = {
    id: "rectangle:create-sparse",
    pointer_path: {
      rate_hz: 120,
      duration_ms: 3000,
      expected_sample_count: 361,
      start: { x: 72, y: 144 },
      finish: { x: 252, y: 240 },
      interpolation: "linear-inclusive",
    },
  };

  const samples = buildAnnotationPointerSamples(command);
  assert.equal(samples.length, 361);
  assert.deepEqual(samples[0], { t_ms: 0, x: 72, y: 144 });
  assert.deepEqual(samples[180], { t_ms: 1500, x: 162, y: 192 });
  assert.deepEqual(samples[360], { t_ms: 3000, x: 252, y: 240 });
});

test("accepts exact cache-pressure semantics while upload bytes remain fail-closed", () => {
  const sequence = ["navigate", "zoom", "pan", "return-page-1"];
  const observation = {
    expected_cycles: 5,
    expected_image_identity: "page-1-sha256",
    cycles: Array.from({ length: 5 }, (_, cycle) => ({
      cycle: cycle + 1,
      actions: [...sequence],
      navigation_current: true,
      zoom_current: true,
      pan: { before: { left: 0, top: 0 }, after: { left: 180, top: 140 } },
      cancellation_count: cycle === 0 ? 1 : 0,
      stale_visible_surface_frames: 0,
      presentation_current: true,
      image_identity: "page-1-sha256",
    })),
    expected_sequence: sequence,
    cache: {
      max_page_url_bytes: 80,
      page_url_byte_limit: 120,
      max_thumbnail_bytes: 40,
      thumbnail_byte_limit: 64,
      max_decoded_render_bytes: 28,
      decoded_render_byte_limit: 32,
    },
    upload_byte_count: null,
  };

  const evidence = assessElectronCachePressureEvidence(observation);
  assert.equal(evidence.semantic_passed, true);
  assert.equal(evidence.passed, false);
  assert.deepEqual(evidence.milestones, {
    "declared-cache-byte-limit-held": true,
    "decoded-byte-limit-held": true,
    "upload-byte-count-recorded": false,
  });

  for (const mutate of [
    (candidate) => {
      candidate.cycles.length = 4;
    },
    (candidate) => {
      candidate.cycles[2].actions = [
        "navigate",
        "pan",
        "zoom",
        "return-page-1",
      ];
    },
    (candidate) => {
      candidate.cycles[2].pan.after = { ...candidate.cycles[2].pan.before };
    },
    (candidate) => {
      candidate.cycles[0].cancellation_count = 0;
    },
    (candidate) => {
      candidate.cycles[1].stale_visible_surface_frames = 1;
    },
    (candidate) => {
      candidate.cycles[3].presentation_current = false;
    },
    (candidate) => {
      candidate.cycles[4].image_identity = "wrong-page";
    },
    (candidate) => {
      candidate.cache.max_page_url_bytes = 121;
    },
    (candidate) => {
      candidate.cache.max_decoded_render_bytes = 33;
    },
  ]) {
    const candidate = structuredClone(observation);
    mutate(candidate);
    assert.equal(
      assessElectronCachePressureEvidence(candidate).semantic_passed,
      false,
    );
  }
});

test("builds the dense Rectangle replay contract from the three frozen source commands", async () => {
  const workload = await loadComparisonWorkload();
  const dense = workload.journeys
    .flatMap(({ commands }) => commands)
    .find(({ id }) => id === "rectangle:repeat-dense");

  const contract = buildElectronDenseRectangleSourceContract(workload, dense);

  assert.deepEqual(contract.command_ids, [
    "rectangle:create-sparse",
    "rectangle:select-move-resize",
    "rectangle:properties-history",
  ]);
  assert.deepEqual(
    contract.commands.map(({ id }) => id),
    contract.command_ids,
  );
  assert.equal(contract.require_exact_fields, true);
});

test("projects the frozen density commands into 100 maintained page-2 rectangles", () => {
  const markups = buildElectronDenseFixtureMarkups(
    {
      commands: [
        {
          annotation_id: "bp-annotation-density-v1:p002:rectangle:0001",
          page_id: "bp-annotation-density-v1:page:002",
          operation: "create-rectangle",
          bounds_pdf: { x: 48, y: 96, width: 36, height: 24 },
          style: {
            stroke_rgba: [0.113725, 0.431373, 0.847059, 1],
            fill_rgba: [0.113725, 0.431373, 0.847059, 0.08],
            stroke_width_pt: 1,
            stroke_style: "dashed",
          },
        },
      ],
    },
    1,
  );

  assert.deepEqual(markups, [
    {
      id: "bp-annotation-density-v1:p002:rectangle:0001",
      pageIndex: 1,
      kind: "rectangle",
      rect: { x: 48, y: 96, width: 36, height: 24 },
      appearance: {
        stroke: { color: "#1d6ed8", widthPt: 1, style: "dashed" },
        fill: { color: "#1d6ed814" },
        opacity: 1,
        blendMode: "normal",
      },
    },
  ]);
});

test("matches only the exact page-2 geometry-index generation to the seeded rectangles", () => {
  const seed = [
    {
      id: "b",
      pageIndex: 1,
      kind: "rectangle",
      rect: { x: 40, y: 50, width: 30, height: 20 },
    },
    {
      id: "a",
      pageIndex: 1,
      kind: "rectangle",
      rect: { x: 10, y: 20, width: 30, height: 20 },
    },
  ];
  const index = {
    pageIndex: 1,
    totalMarkupCount: 2,
    indexedMarkupCount: 2,
    generation: [
      { id: seed[1].id, bounds: seed[1].rect },
      { id: seed[0].id, bounds: seed[0].rect },
    ],
  };
  assert.equal(electronDenseGeometryIndexMatchesSeed(index, seed), true);
  assert.equal(
    electronDenseGeometryIndexMatchesSeed({ ...index, pageIndex: 0 }, seed),
    false,
  );
  assert.equal(
    electronDenseGeometryIndexMatchesSeed(
      {
        ...index,
        generation: [
          { id: seed[1].id, bounds: { ...seed[1].rect, width: 31 } },
          { id: seed[0].id, bounds: seed[0].rect },
        ],
      },
      seed,
    ),
    false,
  );
});

test("uses an unfilled rectangle edge away from resize handles for maintained UI moves", () => {
  assert.deepEqual(
    rectangleMoveEdgePoint({
      rect: { x: 72, y: 144, width: 180, height: 96 },
    }),
    { x: 117, y: 240 },
  );
});

test("accepts native rectangle selection drift within one device pixel and rejects more", () => {
  const surface = { pixels_per_point_x: 0.95, pixels_per_point_y: 0.95 };
  const within = assessElectronNativeSelectPoint({
    observed: { x: 116.52960526315789, y: 239.67680921052636 },
    expected: { x: 117, y: 240 },
    surface,
    devicePixelRatio: 1,
  });
  assert.ok(Math.abs(within.delta.x + 0.4703947368421093) < 1e-12);
  assert.ok(Math.abs(within.delta.y + 0.32319078947363766) < 1e-12);
  assert.equal(within.tolerance_pt, 1 / 0.95);
  assert.equal(within.passed, true);
  assert.equal(
    assessElectronNativeSelectPoint({
      observed: { x: 115.8, y: 240 },
      expected: { x: 117, y: 240 },
      surface,
      devicePixelRatio: 1,
    }).passed,
    false,
  );
});

test("remaps only canonical markup ids in untimed hook actions", () => {
  assert.deepEqual(
    remapElectronUntimedAction(
      {
        hook: "applyMarkupMutation",
        args: [
          {
            kind: "tool-transform",
            markupId: "comparison:length:1",
            startPoint: { x: 306, y: 510 },
            currentPoint: { x: 342, y: 510 },
          },
        ],
        capture: "committed",
      },
      [{ canonical_id: "comparison:length:1", observed_id: "live-length-7" }],
    ),
    {
      hook: "applyMarkupMutation",
      args: [
        {
          kind: "tool-transform",
          markupId: "live-length-7",
          startPoint: { x: 306, y: 510 },
          currentPoint: { x: 342, y: 510 },
        },
      ],
      capture: "committed",
    },
  );
});

test("converts the declared device-pixel geometry oracle into PDF points", () => {
  const input = {
    coordinate_mapping: {
      surface: {
        bounds: { width: 463.109375, height: 599.90625 },
        view_box: { width: 612, height: 792 },
      },
    },
  };
  assert.ok(annotationGeometryTolerancePt(input, 1) > 1.32);
  assert.ok(annotationGeometryTolerancePt(input, 1) < 1.33);
  assert.ok(
    annotationGeometryTolerancePt(input, 1.25) >
      annotationGeometryTolerancePt(input, 1),
  );
});

test("maps the exact rectangle properties command onto maintained UI controls", () => {
  assert.deepEqual(
    buildElectronRectanglePropertiesUiPlan({
      properties: {
        stroke: "#dc2626ff",
        fill: "#dc26261f",
        width_pt: 3,
        dash: "dashed",
        opacity: 0.88,
      },
    }),
    [
      { kind: "color", label: "Color", value: "#dc2626" },
      { kind: "color", label: "Fill Color", value: "#dc2626" },
      { kind: "number", label: "Line Width", value: 3 },
      { kind: "select", label: "Line Style", value: "Dashed" },
      { kind: "number", label: "Opacity", value: 88 },
      { kind: "number", label: "Fill Opacity", value: (31 / 255) * 100 },
    ],
  );
});

test("resolves the visible Base UI switch instead of its hidden associated input", () => {
  const dom = new JSDOM(`
    <aside data-testid="right-sidebar">
      <div data-slot="field">
        <label for="locked-hidden">Locked</label>
        <button data-slot="switch" role="switch" aria-checked="false"></button>
        <input id="locked-hidden" type="checkbox" style="position:fixed;left:-1px;top:-1px;width:1px;height:1px" />
      </div>
    </aside>
  `);
  const root = dom.window.document.querySelector(
    '[data-testid="right-sidebar"]',
  );

  const control = resolveLabeledPropertyControl(root, "Locked", "switch");

  assert.equal(control.getAttribute("data-slot"), "switch");
  assert.equal(control.getAttribute("role"), "switch");
});

test("replays each enabled history action without dispatching a boundary no-op", async () => {
  const states = ["initial", "styled", "styled", "unlocked"];
  let index = 3;
  let dispatchCount = 0;
  const snapshot = async () => states[index];
  const undo = await replayHistoryToBoundary({
    snapshot,
    canDispatch: async () => index > 0,
    dispatch: async () => {
      index -= 1;
      dispatchCount += 1;
    },
  });
  const redo = await replayHistoryToBoundary({
    snapshot,
    canDispatch: async () => index < states.length - 1,
    dispatch: async () => {
      index += 1;
      dispatchCount += 1;
    },
  });

  assert.deepEqual(undo, {
    changed: 3,
    state_changes: 2,
    boundary_reached: true,
  });
  assert.deepEqual(redo, {
    changed: 3,
    state_changes: 2,
    boundary_reached: true,
  });
  assert.equal(index, 3);
  assert.equal(dispatchCount, 6);
});

test("requires a rejected locked transform to preserve the exact history position", () => {
  const before = { past: 9, future: 0, currentRevision: 9, savedRevision: 0 };

  assert.equal(sameDocumentHistoryPosition(before, { ...before }), true);
  assert.equal(
    sameDocumentHistoryPosition(before, {
      ...before,
      past: 10,
      currentRevision: 10,
    }),
    false,
  );
  assert.equal(sameDocumentHistoryPosition(before, null), false);
});

test("requires every exact Electron viewer milestone before an iteration succeeds", () => {
  const contract = {
    commands: [
      {
        id: "viewer:navigate-normalized",
        expected_milestones: [
          "target-page-current",
          "settled-current-generation-250ms",
        ],
      },
    ],
  };
  assert.equal(
    comparisonMilestonesSucceeded(
      [
        {
          event: "comparison-milestone",
          command_id: "viewer:navigate-normalized",
          milestone: "target-page-current",
        },
        {
          event: "comparison-milestone",
          command_id: "viewer:navigate-normalized",
          milestone: "settled-current-generation-250ms",
        },
      ],
      contract,
    ),
    true,
  );
  assert.equal(
    comparisonMilestonesSucceeded(
      [
        {
          event: "comparison-milestone",
          command_id: "viewer:navigate-normalized",
          milestone: "target-page-current",
        },
      ],
      contract,
    ),
    false,
  );
});

test("editor-create also requires one passing exact-state record per command", () => {
  const contract = {
    require_exact_fields: true,
    command_ids: ["text:create"],
    commands: [{ id: "text:create", expected_milestones: ["text-shaped"] }],
  };
  const milestone = {
    event: "comparison-milestone",
    command_id: "text:create",
    milestone: "text-shaped",
  };
  assert.equal(comparisonMilestonesSucceeded([milestone], contract), false);
  assert.equal(
    comparisonMilestonesSucceeded(
      [
        milestone,
        {
          event: "comparison-command-exact-state",
          command_id: "text:create",
          passed: false,
        },
      ],
      contract,
    ),
    false,
  );
  assert.equal(
    comparisonMilestonesSucceeded(
      [
        milestone,
        {
          event: "comparison-command-exact-state",
          command_id: "text:create",
          passed: true,
        },
      ],
      contract,
    ),
    true,
  );
});

test("builds the exact inclusive Catmull-Rom highlight stream", () => {
  const controlPoints = [
    [90, 330],
    [150, 337],
    [220, 329],
    [300, 334],
  ];
  const samples = buildAnnotationPointerSamples({
    id: "highlight:create",
    pointer_path: {
      rate_hz: 120,
      duration_ms: 3000,
      expected_sample_count: 361,
      control_points: controlPoints,
      interpolation: "catmull-rom-inclusive",
    },
  });

  assert.equal(samples.length, 361);
  for (const [sampleIndex, controlPoint] of [0, 1, 2, 3].map((index) => [
    index * 120,
    controlPoints[index],
  ])) {
    assert.deepEqual(
      { x: samples[sampleIndex].x, y: samples[sampleIndex].y },
      { x: controlPoint[0], y: controlPoint[1] },
    );
  }
  assert.equal(samples[360].t_ms, 3000);
});

test("requires real highlight sample reduction and native geometry when available", () => {
  const reduced = {
    kind: "highlight",
    paths: [Array.from({ length: 331 }, () => ({ x: 0, y: 0 }))],
  };
  assert.equal(electronHighlightSmoothingEvidence(reduced, 361).passed, true);
  assert.equal(
    electronHighlightSmoothingEvidence(reduced, 361, { matched: true }).passed,
    true,
  );
  assert.equal(
    electronHighlightSmoothingEvidence(reduced, 361, { matched: false }).passed,
    false,
  );
  assert.equal(
    electronHighlightSmoothingEvidence(
      {
        kind: "highlight",
        paths: [Array.from({ length: 361 }, () => ({ x: 0, y: 0 }))],
      },
      361,
    ).passed,
    false,
  );
});

test("separates exact editor semantic state from observed presentation and upload evidence", async () => {
  const workload = await loadComparisonWorkload();
  const commands = new Map(
    workload.journeys
      .flatMap(({ commands }) => commands)
      .map((command) => [command.id, command]),
  );
  const text = assessElectronEditorCreateEvidence(commands.get("text:create"), {
    input: { pdf_point: { x: 210, y: 426 } },
    page_size: { width: 612, height: 792 },
    history_delta: 1,
    markup: {
      kind: "text-box",
      text: "Beam B-12 / revision 3",
      rect: { x: 90, y: 390, width: 240, height: 72 },
      fontFamily: "Helvetica",
      fontSizePt: 14,
      color: "#111827",
      textAlign: "left",
    },
    presentation: {
      element_present: true,
      visible: true,
      text_content: "Beam B-12 / revision 3",
      computed_text_length: 142,
      bbox: { width: 142, height: 14 },
    },
  });
  assert.ok(Object.values(text.exact_fields).every(Boolean));
  assert.ok(Object.values(text.milestones).every(Boolean));

  const scale = assessElectronEditorCreateEvidence(
    commands.get("length:set-scale"),
    {
      scale: { paper_points: 72, real_world_value: 1, unit: "m", precision: 2 },
    },
  );
  assert.ok(Object.values(scale.exact_fields).every(Boolean));
  assert.equal(scale.milestones["measurement-scale-current"], true);

  const length = assessElectronEditorCreateEvidence(
    commands.get("length:create"),
    {
      history_delta: 1,
      markup: {
        kind: "length",
        start: { x: 90, y: 510 },
        end: { x: 306, y: 510 },
      },
      presentation: { text_content: "3.00 m", bbox: { width: 40, height: 12 } },
    },
  );
  assert.ok(Object.values(length.exact_fields).every(Boolean));
  assert.ok(Object.values(length.milestones).every(Boolean));

  const checkerBytes = await readFile(
    new URL(
      "./results/public-fixtures-v1/bp-image-checker-v1.png",
      import.meta.url,
    ),
  );
  const imageEvidence = {
    checker_asset_bytes: checkerBytes,
    input: { input_lane: "native-x11-xtest", pdf_point: { x: 432, y: 444 } },
    page_size: { width: 612, height: 792 },
    history_delta: 1,
    markup: {
      kind: "image",
      rect: { x: 294.3, y: 340.725, width: 275.4, height: 206.55 },
      mimeType: "image/png",
      dataUrl: `data:image/png;base64,${checkerBytes.toString("base64")}`,
    },
    presentation: {
      element_present: true,
      visible: true,
      image_resource_element_present: true,
      decoded_width: 512,
      decoded_height: 384,
      decoded_rgba_payload_bytes: 786432,
      animation_frames_after_markup: 2,
    },
    native_input_completed: true,
  };
  imageEvidence.renderer_resource_submission_receipt =
    buildElectronImageRendererResourceSubmissionReceipt(
      commands.get("image:create"),
      imageEvidence,
    );
  const image = assessElectronEditorCreateEvidence(
    commands.get("image:create"),
    imageEvidence,
  );
  assert.ok(Object.values(image.exact_fields).every(Boolean));
  assert.equal(image.milestones["bitmap-decoded"], true);
  assert.equal(image.milestones["annotation-painted"], true);
  assert.equal(image.milestones["bitmap-upload-recorded"], true);
  assert.equal(image.milestones["decoded-payload-bytes-exact"], true);
  assert.equal(
    image.milestones["renderer-resource-submission-bytes-exact"],
    true,
  );
  assert.equal(image.milestones["annotation-presented"], true);
  assert.equal(
    image.renderer_resource_submission_receipt.physical_bus_upload_bytes,
    null,
  );
});

test("records the exact checker as an app-level renderer-resource submission after native input", async () => {
  const checkerBytes = await readFile(
    new URL(
      "./results/public-fixtures-v1/bp-image-checker-v1.png",
      import.meta.url,
    ),
  );
  const command = {
    id: "image:create",
    asset_id: "bp-image-checker-v1",
    placement: {
      point: { x: 432, y: 444 },
      sizing: "natural-size-page-contained",
      max_page_fraction: 0.45,
      fixture_page_size_points: { width: 612, height: 792 },
    },
  };
  const receipt = buildElectronImageRendererResourceSubmissionReceipt(command, {
    checker_asset_bytes: checkerBytes,
    input: {
      input_lane: "native-x11-xtest",
      pdf_point: { x: 432, y: 444 },
    },
    page_size: { width: 612, height: 792 },
    history_delta: 1,
    markup: {
      kind: "image",
      rect: { x: 294.3, y: 340.725, width: 275.4, height: 206.55 },
      mimeType: "image/png",
      dataUrl: `data:image/png;base64,${checkerBytes.toString("base64")}`,
    },
    presentation: {
      element_present: true,
      visible: true,
      image_resource_element_present: true,
      decoded_width: 512,
      decoded_height: 384,
      decoded_rgba_payload_bytes: 786432,
      animation_frames_after_markup: 2,
    },
    native_input_completed: true,
    legacy_chromium_trace: null,
  });

  assert.equal(
    receipt.receipt_kind,
    "electron-app-renderer-image-resource-submission-v1",
  );
  assert.equal(
    receipt.evidence_scope,
    "app-level-renderer-resource-submission-not-physical-gpu-upload",
  );
  assert.deepEqual(receipt.asset, {
    asset_id: "bp-image-checker-v1",
    encoded_sha256:
      "fcc714d1ac60ed4b88abf7297830479c7557cb9d219033e7a5a5ad4d6ec18dda",
    markup_payload_sha256:
      "fcc714d1ac60ed4b88abf7297830479c7557cb9d219033e7a5a5ad4d6ec18dda",
    encoded_bytes: 3153,
    source_width_px: 512,
    source_height_px: 384,
    decoded_rgba_payload_bytes: 786432,
    decoded_rgba_payload_basis:
      "browser-decoded-image-drawn-to-canvas-image-data-rgba8-byte-length",
  });
  assert.equal(receipt.exact_checker_asset, true);
  assert.equal(receipt.exact_markup_semantic_state, true);
  assert.equal(receipt.renderer_resource_submission_observed, true);
  assert.equal(receipt.decoded_payload_bytes, 786432);
  assert.equal(receipt.renderer_resource_submission_bytes, 786432);
  assert.equal(receipt.presented_after_native_input, true);
  assert.equal(receipt.physical_bus_upload_bytes, null);
  assert.equal(receipt.legacy_chromium_trace, null);
  assert.equal(receipt.passed, true);
});

test("requires native Image placement to restore and verify target-window focus first", () => {
  const restored = assessElectronNativeWindowFocusRestoration({
    target_window_id: "4194307",
    before_active_window_id: "6292339",
    before_document_has_focus: false,
    after_active_window_id: "4194307",
    after_document_has_focus: true,
  });
  assert.equal(restored.restoration_required, true);
  assert.equal(restored.contractual_image_placement_input, false);
  assert.equal(restored.passed, true);

  assert.equal(
    assessElectronNativeWindowFocusRestoration({
      ...restored,
      target_window_id: "4194307",
      before_active_window_id: "6292339",
      before_document_has_focus: false,
      after_active_window_id: "6292339",
      after_document_has_focus: true,
    }).passed,
    false,
  );
});

test("records exact semantic Image resize submission and presentation without native input", async () => {
  const checkerBytes = await readFile(
    new URL(
      "./results/public-fixtures-v1/bp-image-checker-v1.png",
      import.meta.url,
    ),
  );
  const command = {
    id: "image:resize-history",
    replacement_bounds: { x: 360, y: 390, width: 180, height: 135 },
    resource_observation: {
      decoded_payload_bytes: 786432,
      renderer_resource_submission_bytes: 786432,
      physical_bus_upload_bytes: null,
    },
  };
  const evidence = {
    checker_asset_bytes: checkerBytes,
    after_redo_markup: {
      kind: "image",
      rect: command.replacement_bounds,
      mimeType: "image/png",
      dataUrl: `data:image/png;base64,${checkerBytes.toString("base64")}`,
    },
    presentation: {
      element_present: true,
      visible: true,
      image_resource_element_present: true,
      decoded_width: 512,
      decoded_height: 384,
      decoded_rgba_payload_bytes: 786432,
      animation_frames_after_markup: 2,
    },
  };
  const receipt = buildElectronImageResizeRendererResourceSubmissionReceipt(
    command,
    evidence,
  );

  assert.equal(receipt.renderer_resource_submission_observed, true);
  assert.equal(receipt.renderer_resource_submission_bytes, 786432);
  assert.equal(receipt.presented_after_semantic_edit, true);
  assert.equal(receipt.presented_after_native_input, false);
  assert.equal(receipt.physical_bus_upload_bytes, null);
  assert.equal(receipt.passed, true);

  evidence.presentation.visible = false;
  assert.equal(
    buildElectronImageResizeRendererResourceSubmissionReceipt(command, evidence)
      .passed,
    false,
  );
});

test("selects frozen decimal precision through maintained Page Scale controls", async () => {
  const workload = await loadComparisonWorkload();
  const command = workload.journeys
    .flatMap(({ commands }) => commands)
    .find(({ id }) => id === "length:set-scale");

  assert.deepEqual(editorCreatePrecisionUiSelection(command), [
    { test_id: "page-scale-precision-mode", visible_label: "Decimal" },
    { test_id: "page-scale-precision-value", visible_label: "0.01" },
  ]);
});

test("targets the open Base UI option instead of the select trigger and reads SelectValue", () => {
  const dom = new JSDOM(`
    <button role="combobox" data-slot="select-trigger" data-testid="page-scale-precision-mode" aria-expanded="true">
      <span data-slot="select-value">Decimal</span><svg>▼</svg>
    </button>
    <div data-slot="select-content">
      <div role="option" data-slot="select-item" aria-selected="true" data-selected>Decimal</div>
      <div role="option" data-slot="select-item" aria-selected="false">Fraction</div>
    </div>
  `);
  const { document } = dom.window;
  const trigger = document.querySelector(
    '[data-testid="page-scale-precision-mode"]',
  );
  const option = findVisibleElectronSelectOption(document, "Decimal", () => ({
    width: 100,
    height: 24,
  }));

  assert.equal(option?.getAttribute("role"), "option");
  assert.equal(option?.getAttribute("data-slot"), "select-item");
  assert.notEqual(option, trigger);
  assert.equal(electronSelectTriggerRetainsValue(trigger, "Decimal"), true);
  assert.equal(electronSelectTriggerRetainsValue(trigger, "Fraction"), false);
});

test("accepts only the documented raster-density rounding margin", () => {
  assert.equal(electronSettledDensityIsCurrent(1), true);
  assert.equal(electronSettledDensityIsCurrent(0.979), true);
  assert.equal(electronSettledDensityIsCurrent(0.974), false);
});

test("measures the promoted visible detail raster instead of a stale whole-page preview", () => {
  const dom = new JSDOM(`
    <div id="page">
      <canvas id="preview" width="450" height="600" data-render-quality="preview"></canvas>
      <canvas id="detail" width="1000" height="800" data-render-quality="detail-crop"></canvas>
      <canvas id="offscreen" width="4000" height="4000" data-render-quality="detail-crop"></canvas>
    </div>
  `);
  const { document } = dom.window;
  const bounds = new Map([
    [
      "preview",
      { left: 0, top: 0, right: 1000, bottom: 700, width: 1000, height: 700 },
    ],
    [
      "detail",
      { left: 100, top: 100, right: 600, bottom: 500, width: 500, height: 400 },
    ],
    [
      "offscreen",
      {
        left: 2000,
        top: 0,
        right: 3000,
        bottom: 1000,
        width: 1000,
        height: 1000,
      },
    ],
  ]);

  assert.equal(
    measureElectronVisibleRasterDensity(
      document.querySelector("#page"),
      { left: 0, top: 0, right: 1200, bottom: 800 },
      2,
      (element) => bounds.get(element.id),
    ),
    1,
  );
});

test("opens a fresh zoom menu before accepting a visible preset", () => {
  assert.equal(
    electronZoomPresetMenuReady({
      trigger_expanded: true,
      preset_visible: true,
    }),
    true,
  );
  assert.equal(
    electronZoomPresetMenuReady({
      trigger_expanded: false,
      preset_visible: true,
    }),
    false,
  );
  assert.equal(
    electronZoomPresetMenuReady({
      trigger_expanded: true,
      preset_visible: false,
    }),
    false,
  );
});

test("builds one renderer-side semantic pan schedule from the frozen pointer stream", () => {
  const command = {
    rate_hz: 2,
    duration_ms: 1000,
    normalized_viewport_points: [
      [0.25, 0.25],
      [0.75, 0.5],
    ],
  };

  assert.deepEqual(
    buildElectronSemanticPanFrames(
      command,
      { width: 800, height: 600 },
      { left: 500, top: 400 },
    ),
    [
      { sample_index: 0, t_ms: 0, scroll_left: 500, scroll_top: 400 },
      { sample_index: 1, t_ms: 500, scroll_left: 300, scroll_top: 325 },
      { sample_index: 2, t_ms: 1000, scroll_left: 100, scroll_top: 250 },
    ],
  );
});

test("plans a maintained 400 percent zoom with a centered, pixel-distinct length gesture", async () => {
  const workload = await loadComparisonWorkload();
  const command = workload.journeys
    .flatMap(({ commands }) => commands)
    .find(({ id }) => id === "length:create");

  assert.deepEqual(
    buildEditorCreateHighZoomPlan(command, {
      zoom_percent: 400,
      viewport_width_px: 1100,
      viewport_height_px: 700,
    }),
    {
      zoom_preset_test_id: "viewer-zoom-preset-400",
      zoom_percent: 400,
      close_menu_with_escape: true,
      focus_pdf_point: { x: 198, y: 510 },
      endpoint_distance_px: 864,
      viewport_margin_px: 24,
      endpoints_fit_viewport: true,
    },
  );
  assert.deepEqual(
    buildEditorCreateHighZoomPlan(command, {
      viewport_width_px: 900,
      viewport_height_px: 700,
    }),
    {
      zoom_preset_test_id: "viewer-zoom-preset-200",
      zoom_percent: 200,
      close_menu_with_escape: true,
      focus_pdf_point: { x: 198, y: 510 },
      endpoint_distance_px: 432,
      viewport_margin_px: 24,
      endpoints_fit_viewport: true,
    },
  );
});

test("waits for maintained Length activation and the first endpoint draft before the second click", () => {
  assert.deepEqual(buildEditorCreateLengthInteractionPlan(), {
    active_tool_test_id: "tool-length",
    active_tool_aria_pressed: "true",
    scroll_into_view: { block: "center", inline: "nearest" },
    inter_endpoint_animation_frames: 2,
  });
});

test("maps exact PDF points with the editor transform instead of the bordered SVG box", () => {
  assert.deepEqual(
    pdfPointToCss(
      { x: 90, y: 510 },
      {
        bounds: { left: 97, top: -90, width: 1222, height: 1582 },
        view_box: { x: 0, y: 0, width: 612, height: 792 },
        pixels_per_pdf_point: 2,
      },
    ),
    { x: 277, y: 474 },
  );
});

test("maps native PDF clicks from the observed surface bottom at the nominal zoom scale", () => {
  const mapped = pdfPointToNativeCss(
    { x: 90, y: 510 },
    {
      bounds: { left: 178, top: -90, width: 1222, height: 1582 },
      view_box: { x: 0, y: 0, width: 612, height: 792 },
      pixels_per_pdf_point: 2,
    },
  );
  assert.deepEqual(mapped, { x: 358, y: 472 });
});

test("accepts maintained Text autosize and natural Image placement after the reviewed v3 correction", async () => {
  const workload = await loadComparisonWorkload();
  const commands = new Map(
    workload.journeys
      .flatMap(({ commands }) => commands)
      .map((command) => [command.id, command]),
  );

  assert.deepEqual(
    classifyEditorCreateBoundsContract(commands.get("text:create"), {
      kind: "text-box",
      rect: { x: 203.4055, y: 416.8681, width: 151.6201, height: 21 },
    }),
    {
      command_id: "text:create",
      native_layout_semantics_match: true,
      frozen_single_transaction_replayable: true,
      status: "frozen-command-replayable",
      reason: null,
      proposed_change: null,
    },
  );
  assert.equal(
    classifyEditorCreateBoundsContract(
      commands.get("image:create"),
      {
        kind: "image",
        rect: { x: 294.3, y: 340.725, width: 275.4, height: 206.55 },
      },
      { width: 612, height: 792 },
    ).status,
    "frozen-command-replayable",
  );

  const imageCommand = commands.get("image:create");
  const expected = { x: 294.3, y: 340.725, width: 275.4, height: 206.55 };
  const nativeInput = {
    input_lane: "native-x11-xtest",
    surface: { pixels_per_pdf_point: 0.76 },
  };
  const atBoundary = classifyEditorCreateBoundsContract(
    imageCommand,
    {
      kind: "image",
      rect: {
        ...expected,
        x: expected.x + 0.5 / 0.76,
        y: expected.y - 0.5 / 0.76,
      },
    },
    { width: 612, height: 792 },
    nativeInput,
  );
  assert.equal(atBoundary.native_layout_semantics_match, true);
  assert.equal(atBoundary.native_pixel_rounding.max_per_axis_px, 0.5);
  assert.equal(atBoundary.native_pixel_rounding.dimensions_exact, true);
  assert.equal(atBoundary.native_pixel_rounding.aspect_ratio_exact, true);
  assert.equal(atBoundary.native_pixel_rounding.page_fraction_held, true);

  assert.equal(
    classifyEditorCreateBoundsContract(
      imageCommand,
      {
        kind: "image",
        rect: {
          ...expected,
          x: expected.x + 0.5001 / 0.76,
        },
      },
      { width: 612, height: 792 },
      nativeInput,
    ).native_layout_semantics_match,
    false,
  );
});

test("turns the frozen continuous path into exact-rate CDP wheel phases", () => {
  const phases = buildContinuousScrollPhases(
    {
      input_rate_hz: 120,
      path: {
        forward_duration_ms: 20000,
        forward_viewport_heights: 50,
        pause_duration_ms: 2000,
        reverse_duration_ms: 10000,
        finish_page: 1,
      },
    },
    640,
  );

  assert.deepEqual(
    phases.map(({ name, event_count, duration_ms }) => ({
      name,
      event_count,
      duration_ms,
    })),
    [
      { name: "forward", event_count: 2400, duration_ms: 20000 },
      { name: "pause", event_count: 0, duration_ms: 2000 },
      { name: "reverse", event_count: 1200, duration_ms: 10000 },
    ],
  );
  assert.equal(phases[0].delta_y * phases[0].event_count, 50 * 640);
  assert.equal(phases[2].delta_y * phases[2].event_count, -50 * 640);
});

test("builds the frozen high-zoom pan as a 5 second 120 Hz inclusive path", () => {
  const points = [
    [0.5, 0.5],
    [0.75, 0.5],
    [0.75, 0.75],
    [0.25, 0.75],
    [0.25, 0.25],
    [0.5, 0.5],
  ];
  const samples = buildNormalizedPointerSamples({
    id: "viewer:pan-usgs",
    duration_ms: 5000,
    rate_hz: 120,
    normalized_viewport_points: points,
  });
  assert.equal(samples.length, 601);
  for (let index = 0; index < points.length; index += 1) {
    assert.deepEqual(
      { x: samples[index * 120].x, y: samples[index * 120].y },
      { x: points[index][0], y: points[index][1] },
    );
  }
  assert.equal(samples.at(-1).t_ms, 5000);
});

test("accepts native X11 only for Electron scenarios with complete native action paths", () => {
  assert.deepEqual(
    [...electronNativeX11Scenarios],
    [
      "open-pdf",
      "viewer-layout",
      "page-navigation",
      "zoom",
      "high-zoom-pan",
      "annotation-create",
      "annotation-transform",
      "editor-create",
      "continuous-scroll",
      "fit-modes",
    ],
  );
});

test("maps a target page to an exact native thumbnail scrollbar thumb drag", () => {
  const replay = buildElectronNativePageNavigationReplay({
    windowId: "42",
    pageNumber: 6,
    pageCount: 11,
    track: { x: 200, y: 100, width: 12, height: 500 },
    thumb: { x: 200, y: 100, width: 12, height: 50 },
    logicalSize: { width: 1200, height: 800 },
    windowGeometry: {
      width: 1208,
      height: 804,
      content: { x: 4, y: 2, width: 1200, height: 800 },
    },
  });
  assert.deepEqual(replay.logical, {
    start: { x: 206, y: 125 },
    finish: { x: 206, y: 350 },
    normalized_page_position: 0.5,
  });
  assert.deepEqual(replay.args, [
    "mousemove",
    "--window",
    "42",
    "210",
    "127",
    "mousedown",
    "1",
    "mousemove",
    "--window",
    "42",
    "210",
    "352",
    "mouseup",
    "1",
  ]);
});

test("clamps a native editor pan into repeatable in-viewport segments", () => {
  assert.deepEqual(
    buildElectronNativeViewportPanSegment({
      viewport_bounds: { x: 48, y: 161, width: 1050, height: 625 },
      delta_px: { x: -215.6470588235294, y: -321.21212121212113 },
    }),
    {
      start: { x: 573, y: 473.5 },
      finish: { x: 788.6470588235294, y: 785 },
      requested_finish: { x: 788.6470588235294, y: 794.7121212121211 },
      clamped: true,
    },
  );
});

test("maps the frozen high-zoom path to a direct middle-button XTEST replay", () => {
  const replay = buildElectronNativePanReplay(
    {
      id: "viewer:pan-usgs",
      duration_ms: 5000,
      rate_hz: 120,
      normalized_viewport_points: [
        [0.5, 0.5],
        [0.75, 0.5],
        [0.75, 0.75],
        [0.25, 0.75],
        [0.25, 0.25],
        [0.5, 0.5],
      ],
    },
    {
      bounds: { x: 100, y: 80, width: 1000, height: 640 },
      window_logical_size: { width: 1200, height: 800 },
    },
    {
      window_id: "4194307",
      geometry: { width: 1200, height: 800 },
    },
  );

  assert.equal(replay.pixel_samples.length, 601);
  assert.equal(replay.metadata.button, 2);
  assert.equal(replay.metadata.scheduled_duration_ms, 5000);
  assert.deepEqual(replay.pixel_samples[0], {
    x: 600,
    y: 400,
    scheduled_ms: 0,
  });
  assert.deepEqual(replay.pixel_samples.at(-1), {
    x: 600,
    y: 400,
    scheduled_ms: 5000,
  });
});

test("keeps native file chooser text and confirmation commands isolated", () => {
  assert.deepEqual(
    buildElectronNativeFileDialogCommands("/fixtures/nasa.pdf"),
    [
      ["key", "--clearmodifiers", "ctrl+l"],
      ["type", "--clearmodifiers", "--delay", "1", "/fixtures/nasa.pdf"],
    ],
  );
});

test("maps the verified native PDF chooser to its maintained Open button", () => {
  assert.deepEqual(
    buildElectronNativeFileDialogOpenClick({
      window_id: "6291463",
      title: "Open PDFs",
      geometry: {
        window: "6291463",
        x: 79,
        y: 93,
        width: 1124,
        height: 822,
        screen: 0,
        visible: true,
      },
    }),
    {
      x: 1153,
      y: 875,
      button: 1,
      input_window_id: "6291463",
      dialog_geometry: {
        window: "6291463",
        x: 79,
        y: 93,
        width: 1124,
        height: 822,
        screen: 0,
        visible: true,
      },
      open_button_center_inset_px: { right: 50, bottom: 40 },
    },
  );
});

test("keeps every native custom-hex edit valid while replacing one digit at a time", () => {
  assert.deepEqual(
    buildElectronNativeStableHexReplacementCommands("42", "#111827"),
    [
      ["key", "--window", "42", "--clearmodifiers", "Home", "Right"],
      ["key", "--window", "42", "--clearmodifiers", "shift+Right"],
      ["type", "--window", "42", "--clearmodifiers", "--delay", "1", "1"],
      ["key", "--window", "42", "--clearmodifiers", "shift+Right"],
      ["type", "--window", "42", "--clearmodifiers", "--delay", "1", "1"],
      ["key", "--window", "42", "--clearmodifiers", "shift+Right"],
      ["type", "--window", "42", "--clearmodifiers", "--delay", "1", "1"],
      ["key", "--window", "42", "--clearmodifiers", "shift+Right"],
      ["type", "--window", "42", "--clearmodifiers", "--delay", "1", "8"],
      ["key", "--window", "42", "--clearmodifiers", "shift+Right"],
      ["type", "--window", "42", "--clearmodifiers", "--delay", "1", "2"],
      ["key", "--window", "42", "--clearmodifiers", "shift+Right"],
      ["type", "--window", "42", "--clearmodifiers", "--delay", "1", "7"],
    ],
  );
  assert.throws(
    () => buildElectronNativeStableHexReplacementCommands("42", "#abcd"),
    /six-digit hex/,
  );
});

test("native open, pan, and transform evidence fail closed on their exact oracles", () => {
  const open = {
    requested_path: "/fixtures/nasa.pdf",
    observed_path: "/fixtures/nasa.pdf",
    native_window_target_verified: true,
    native_open_action_completed: true,
    preview_current_generation: true,
    settled_current_generation_250ms: true,
    presentation_current: true,
  };
  assert.equal(assessElectronNativeOpenEvidence(open).passed, true);
  assert.equal(
    assessElectronNativeOpenEvidence({
      ...open,
      observed_path: "/fixtures/other.pdf",
    }).passed,
    false,
  );
  assert.equal(
    assessElectronNativeOpenEvidence({ ...open, presentation_current: false })
      .passed,
    false,
  );

  const pan = {
    expected_sample_count: 601,
    acknowledged_sample_count: 601,
    timing_within_tolerance: true,
    presentation_current: true,
    probe: {
      max_visible_raster_count: 2,
      max_visible_raster_pixels: 4096 * 4096,
      stale_visible_surface_frames: 0,
    },
    settled_density: 1,
  };
  assert.equal(assessElectronNativePanEvidence(pan).passed, true);
  assert.equal(
    assessElectronNativePanEvidence({ ...pan, acknowledged_sample_count: 600 })
      .passed,
    false,
  );
  assert.equal(
    assessElectronNativePanEvidence({ ...pan, timing_within_tolerance: false })
      .passed,
    false,
  );

  const transform = {
    move_sample_count_matches: true,
    resize_sample_count_matches: true,
    move_timing_within_tolerance: true,
    resize_timing_within_tolerance: true,
    hit_test_selected: true,
    move_history_delta: 1,
    resize_history_delta: 1,
    geometry_exact: true,
    presentation_current: true,
  };
  assert.equal(assessElectronNativeTransformEvidence(transform).passed, true);
  assert.equal(
    assessElectronNativeTransformEvidence({
      ...transform,
      move_history_delta: 2,
    }).passed,
    false,
  );
  assert.equal(
    assessElectronNativeTransformEvidence({
      ...transform,
      geometry_exact: false,
    }).passed,
    false,
  );
});

test("evaluates only directly observed viewer geometry, pan, and reopen evidence", () => {
  const geometry = evaluateVisiblePageGeometry([
    {
      page_index: 0,
      expected_width: 612,
      expected_height: 792,
      observed_width: 306,
      observed_height: 396,
    },
  ]);
  assert.equal(geometry.passed, true);
  assert.equal(evaluateVisiblePageGeometry([]).passed, false);

  assert.deepEqual(
    assessHighZoomPanEvidence({
      probe: {
        max_visible_raster_count: 2,
        max_visible_raster_pixels: 4096 * 4096,
        stale_visible_surface_frames: 0,
      },
      settled_density: 1,
    }),
    {
      visible_tiles_bounded: true,
      stale_generations_presented_zero: true,
      settled_density_at_least_1: true,
    },
  );
  assert.equal(
    assessHighZoomPanEvidence({
      probe: {
        max_visible_raster_count: 1,
        max_visible_raster_pixels: 1,
        stale_visible_surface_frames: 1,
      },
      settled_density: 1,
    }).stale_generations_presented_zero,
    false,
  );

  const closeReopen = assessCloseReopenEvidence({
    closed: {
      document_path: null,
      tab_count: 0,
      render_cache_bytes: 0,
      thumbnail_cache_bytes: 0,
      document_canvas_count: 0,
    },
    memory: { before_close: {}, after_close: {} },
    reopened: {
      document_path_matches: true,
      page_render_ready: true,
      current_surface_ready: true,
      settled_for_ms: 250,
    },
  });
  assert.deepEqual(closeReopen, {
    document_resources_released: true,
    memory_recovery_recorded: true,
    document_reopened: true,
    settled_current_generation_250ms: true,
  });
});

test("rejects the wrong expanded-scenario fixture before Electron launches", () => {
  const contract = {
    fixture_id: "bp-annotation-density-v1",
    fixture_sha256: "expected",
  };
  assert.equal(
    validateExpandedScenarioFixture({ sha256: "expected" }, contract),
    null,
  );
  assert.match(
    validateExpandedScenarioFixture({ sha256: "wrong" }, contract),
    /bp-annotation-density-v1 requires PDF SHA-256 expected; received wrong/,
  );
  assert.equal(
    validateExpandedScenarioFixture({ sha256: "anything" }, null),
    null,
  );
});

test("parses an opt-in v4 parent without changing the selected v3 component", () => {
  const options = parseElectronRunnerArguments([
    "--scenario",
    "zoom",
    "--v4-scenario",
    "engineering-sheet",
    "--pdf",
    "fixtures/engineering.pdf",
  ]);
  assert.equal(options.scenario, "zoom");
  assert.equal(options.v4Scenario, "engineering-sheet");
  assert.match(options.pdf, /\/fixtures\/engineering\.pdf$/);

  const defaultOptions = parseElectronRunnerArguments([
    "--scenario",
    "zoom",
    "--pdf",
    "fixtures/engineering.pdf",
  ]);
  assert.equal(defaultOptions.v4Scenario, undefined);

  for (const scenario of ["fit-modes", "cache-pressure-recovery"]) {
    const local = parseElectronRunnerArguments([
      "--scenario",
      scenario,
      "--v4-scenario",
      "engineering-sheet",
      "--pdf",
      "fixtures/engineering.pdf",
    ]);
    assert.equal(local.scenario, scenario);
    assert.equal(local.v4Scenario, "engineering-sheet");
  }
  assert.throws(
    () =>
      parseElectronRunnerArguments([
        "--scenario",
        "fit-modes",
        "--pdf",
        "fixtures/engineering.pdf",
      ]),
    /fit-modes requires --v4-scenario engineering-sheet/,
  );
});

test("accepts exact maintained fit-page and fit-width presentation evidence", () => {
  const observations = ["fit-page", "fit-width"].map((mode) => ({
    mode,
    diagnostics: { zoom_preset: mode, page_render_ready: true },
    current_generation_presented: true,
    settled_for_ms: 250,
    visible_raster_resources: { count: 2, max_pixels: 4_000_000 },
    settled_density: 1,
  }));
  const evidence = assessElectronEngineeringFitModesEvidence(observations);
  assert.deepEqual(evidence.milestones, {
    "fit-state-current": true,
    "visible-tiles-bounded": true,
    "settled-density-at-least-1": true,
  });
  assert.equal(evidence.passed, true);
  assert.equal(
    assessElectronEngineeringFitModesEvidence(
      observations.map((observation) => ({
        ...observation,
        settled_density: 0.979,
      })),
    ).passed,
    true,
  );
  assert.equal(
    assessElectronEngineeringFitModesEvidence([
      observations[0],
      { ...observations[1], settled_density: 0.974 },
    ]).passed,
    false,
  );
});

test("accepts five bounded engineering cache cycles only after recorded resource recovery", () => {
  const observation = {
    expected_cycles: 5,
    cycles: Array.from({ length: 5 }, (_, index) => ({
      cycle: index + 1,
      actions: ["zoom", "pan", "fit-page"],
      presentation_current: true,
      decoded_render_bytes: 8_000_000 + index,
      renderer_resource_submission_bytes: 4_000_000 + index,
      physical_bus_upload_bytes: null,
    })),
    cache: {
      max_page_url_bytes: 16_000_000,
      page_url_byte_limit: 120 * 1024 * 1024,
      max_thumbnail_bytes: 8_000_000,
      thumbnail_byte_limit: 64 * 1024 * 1024,
      max_decoded_render_bytes: 16_000_000,
      decoded_render_byte_limit: 32 * 1024 * 1024,
      max_renderer_resource_submission_bytes: 12_000_000,
      renderer_resource_submission_byte_limit: 32 * 1024 * 1024,
    },
    recovery: {
      before: {
        document_count: 1,
        render_cache_bytes: 16_000_000,
        decoded_render_bytes: 12_000_000,
        renderer_resource_submission_bytes: 4_000_000,
        process_metrics: { totalWorkingSetKiB: 200_000 },
      },
      after: {
        document_count: 0,
        render_cache_bytes: 0,
        decoded_render_bytes: 0,
        renderer_resource_submission_bytes: 0,
        process_metrics: { totalWorkingSetKiB: 190_000 },
      },
      released_render_bytes: 32_000_000,
    },
  };
  const evidence = assessElectronEngineeringCacheRecoveryEvidence(observation);
  assert.deepEqual(evidence.milestones, {
    "declared-cache-byte-limit-held": true,
    "decoded-byte-limit-held": true,
    "renderer-resource-submission-bytes-exact": true,
    "memory-recovery-recorded": true,
  });
  assert.equal(evidence.passed, true);

  const noRecovery = structuredClone(observation);
  noRecovery.recovery.after.renderer_resource_submission_bytes = 1;
  assert.equal(
    assessElectronEngineeringCacheRecoveryEvidence(noRecovery).passed,
    false,
  );
  const short = structuredClone(observation);
  short.cycles.length = 4;
  assert.equal(
    assessElectronEngineeringCacheRecoveryEvidence(short).passed,
    false,
  );
});

test("accepts target-specific v4 open evidence only with its crop or virtualization oracle", () => {
  const launch = assessElectronV4OpenEvidence(
    {
      id: "small:launch-cold",
      expected_milestones: [
        "process-started",
        "native-window-presented",
        "interactive-shell",
      ],
    },
    {
      process_started: true,
      native_window_presented: true,
      interactive_shell: true,
    },
  );
  assert.equal(launch.passed, true);

  const engineering = assessElectronV4OpenEvidence(
    {
      id: "engineering:open-settle",
      expected_milestones: [
        "document-opened",
        "preview-current-generation",
        "settled-current-generation-250ms",
        "fixed-crops-matched",
      ],
    },
    {
      document_opened: true,
      preview_current_generation: true,
      settled_current_generation_250ms: true,
      fixed_crops_matched: true,
    },
  );
  assert.equal(engineering.passed, true);
  assert.equal(
    assessElectronV4OpenEvidence(
      {
        id: "nasa:open-settle",
        expected_milestones: [
          "document-opened",
          "preview-current-generation",
          "settled-current-generation-250ms",
          "virtual-page-window-bounded",
        ],
      },
      {
        document_opened: true,
        preview_current_generation: true,
        settled_current_generation_250ms: true,
        virtual_page_window_bounded: false,
      },
    ).passed,
    false,
  );
});

test("maps native target-specific open receipts for every representative PDF parent", async () => {
  const v4Workload = await loadComparisonWorkloadV4();
  for (const parentScenario of [
    "small-shell-open",
    "nasa-long-document",
    "engineering-sheet",
  ]) {
    const parentContract = buildScenarioContractV4(v4Workload, parentScenario);
    const context = createElectronV4ExecutionContext({
      parentContract,
      parentWorkload: v4Workload,
      componentScenario: "open-pdf",
      componentContract: null,
      inputLane: "native-x11-xtest",
    });
    const events = context.component_contract.commands.flatMap((command) => [
      ...command.expected_milestones.map((milestone) => ({
        event: "comparison-milestone",
        command_id: command.id,
        milestone,
      })),
      {
        event: "comparison-command-exact-state",
        command_id: command.id,
        passed: true,
      },
    ]);
    const evidence = buildElectronV4ComponentEvidence(context, events, {
      input_lane: "native-x11-xtest",
      exact_manifest_replay: true,
      command_results: context.component_contract.commands.map((command) => ({
        command_id: command.id,
        exact_fields: Object.fromEntries(
          command.expected_milestones.map((milestone) => [
            milestone.replaceAll("-", "_"),
            true,
          ]),
        ),
      })),
    });
    assert.equal(evidence.component_receipts_passed, true, parentScenario);
    assert.deepEqual(evidence.unmapped_command_ids, [], parentScenario);
  }
});

test("rejects an unlisted v4 component through the CLI before PDF or Electron access", () => {
  const result = spawnSync(
    process.execPath,
    [
      fileURLToPath(new URL("./electron-runner.mjs", import.meta.url)),
      "--scenario",
      "zoom",
      "--v4-scenario",
      "dense-mixed-editing",
      "--pdf",
      "/fixture-that-must-not-be-read.pdf",
    ],
    { encoding: "utf8" },
  );

  assert.equal(result.status, 2);
  assert.match(
    result.stderr,
    /zoom is not listed for v4 parent dense-mixed-editing/,
  );
  assert.doesNotMatch(result.stderr, /ENOENT|no such file/i);
});

test("binds only a listed v3 component to the exact v4 parent fixture and provenance", async () => {
  const [v3Workload, v4Workload] = await Promise.all([
    loadComparisonWorkload(),
    loadComparisonWorkloadV4(),
  ]);
  const parentContract = buildScenarioContractV4(
    v4Workload,
    "dense-mixed-editing",
  );
  const componentContract = buildDevelopmentScenarioContract(
    v3Workload,
    "annotation-create",
    "native-x11-xtest",
  );
  const context = createElectronV4ExecutionContext({
    parentContract,
    parentWorkload: v4Workload,
    componentScenario: "annotation-create",
    componentContract,
  });

  assert.equal(context.provenance.protocol_version, "bp-perf-v4");
  assert.equal(context.provenance.manifest_id, "bp-perf-v4-decision-1");
  assert.equal(context.provenance.parent_scenario, "dense-mixed-editing");
  assert.equal(context.provenance.parent_journey_id, "dense-mixed-editing-v1");
  assert.equal(context.provenance.component_scenario, "annotation-create");
  assert.equal(
    context.execution_contract.fixture_id,
    "bp-annotation-density-v1",
  );
  assert.equal(
    context.execution_contract.fixture_sha256,
    parentContract.fixture_sha256,
  );

  assert.throws(
    () =>
      createElectronV4ExecutionContext({
        parentContract,
        parentWorkload: v4Workload,
        componentScenario: "zoom",
        componentContract,
      }),
    /zoom is not listed for v4 parent dense-mixed-editing/,
  );
});

test("maps exact passing component evidence to retained v4 command receipts", async () => {
  const [v3Workload, v4Workload] = await Promise.all([
    loadComparisonWorkload(),
    loadComparisonWorkloadV4(),
  ]);
  const componentContract = buildDevelopmentScenarioContract(
    v3Workload,
    "annotation-create",
    "native-x11-xtest",
  );
  const context = createElectronV4ExecutionContext({
    parentContract: buildScenarioContractV4(v4Workload, "dense-mixed-editing"),
    parentWorkload: v4Workload,
    componentScenario: "annotation-create",
    componentContract,
  });
  const events = componentContract.commands.flatMap((command) =>
    command.expected_milestones.map((milestone) => ({
      event: "comparison-milestone",
      command_id: command.id,
      milestone,
    })),
  );
  const evidence = buildElectronV4ComponentEvidence(context, events, {
    exact_manifest_replay: true,
    command_results: componentContract.commands.map((command) => ({
      command_id: command.id,
      manifest_milestones_complete: true,
    })),
  });

  assert.equal(evidence.component_receipts_passed, true);
  assert.deepEqual(
    evidence.command_receipts.map(({ command_id: commandId }) => commandId),
    ["rectangle:create-sparse", "highlight:create"],
  );
  assert(
    evidence.command_receipts.every(
      (receipt) =>
        receipt.live === true &&
        receipt.passed === true &&
        receipt.mapping_status === "exact-semantic-map" &&
        /^[0-9a-f]{64}$/.test(receipt.source_evidence_sha256) &&
        /^[0-9a-f]{64}$/.test(receipt.evidence_sha256),
    ),
  );
  for (const receipt of evidence.command_receipts) {
    assert.equal(
      receipt.evidence_sha256,
      canonicalSha256({
        parent_scenario: receipt.parent_scenario,
        component_scenario: receipt.component_scenario,
        command_id: receipt.command_id,
        source_command_id: receipt.source_command_id,
        mapping_status: receipt.mapping_status,
        component_execution_passed: receipt.component_execution_passed,
        proven_milestones: receipt.proven_milestones,
        missing_milestones: receipt.missing_milestones,
      }),
    );
  }
});

test("retains exact editor-workload receipts when a sibling Image command fails", async () => {
  const [v3Workload, v4Workload] = await Promise.all([
    loadComparisonWorkload(),
    loadComparisonWorkloadV4(),
  ]);
  const componentContract = buildDevelopmentScenarioContract(
    v3Workload,
    "editor-workload",
  );
  const context = createElectronV4ExecutionContext({
    parentContract: buildScenarioContractV4(v4Workload, "dense-mixed-editing"),
    parentWorkload: v4Workload,
    componentScenario: "editor-workload",
    componentContract,
    inputLane: "cdp-input-diagnostic",
  });
  const failingCommandId = "image:resize-history";
  const events = context.command_mappings.flatMap(
    ({ command_id: commandId, source_command_id: sourceCommandId }) =>
      commandId === failingCommandId
        ? []
        : context.parent_contract.commands
            .find(({ id }) => id === commandId)
            .expected_milestones.map((milestone) => ({
              event: "comparison-milestone",
              command_id: sourceCommandId,
              milestone,
            })),
  );
  const evidence = buildElectronV4ComponentEvidence(context, events, {
    input_lane: "cdp-input-diagnostic",
    exact_manifest_replay: false,
    command_results: context.command_mappings.map(
      ({ source_command_id: sourceCommandId }) => ({
        command_id: sourceCommandId,
      }),
    ),
  });
  const byId = new Map(
    evidence.command_receipts.map((receipt) => [receipt.command_id, receipt]),
  );

  assert.equal(evidence.component_execution_passed, false);
  assert.equal(evidence.component_receipts_passed, false);
  for (const commandId of [
    "rectangle:repeat-dense",
    "highlight:edit-history",
    "text:edit-resize-history",
    "length:edit-endpoint-history",
  ]) {
    assert.equal(byId.get(commandId).passed, true, commandId);
    assert.equal(
      byId.get(commandId).component_execution_passed,
      true,
      commandId,
    );
    assert.deepEqual(byId.get(commandId).missing_milestones, [], commandId);
  }
  assert.equal(byId.get(failingCommandId).passed, false);
  assert.equal(byId.get(failingCommandId).component_execution_passed, false);
  assert(byId.get(failingCommandId).missing_milestones.length > 0);
});

test("accepts semantic CDP image presentation only with an exact renderer receipt", async () => {
  const [v3Workload, v4Workload] = await Promise.all([
    loadComparisonWorkload(),
    loadComparisonWorkloadV4(),
  ]);
  const componentContract = buildDevelopmentScenarioContract(
    v3Workload,
    "editor-workload",
  );
  const context = createElectronV4ExecutionContext({
    parentContract: buildScenarioContractV4(v4Workload, "dense-mixed-editing"),
    parentWorkload: v4Workload,
    componentScenario: "editor-workload",
    componentContract,
    inputLane: "cdp-input-diagnostic",
  });
  const imageCommandId = "image:resize-history";
  const presentationMilestones = new Set([
    "renderer-resource-submission-bytes-exact",
    "bitmap-presented-from-decoded-payload",
  ]);
  const events = context.command_mappings.flatMap(
    ({ command_id: commandId, source_command_id: sourceCommandId }) =>
      context.parent_contract.commands
        .find(({ id }) => id === commandId)
        .expected_milestones.filter(
          (milestone) =>
            commandId !== imageCommandId ||
            !presentationMilestones.has(milestone),
        )
        .map((milestone) => ({
          event: "comparison-milestone",
          command_id: sourceCommandId,
          milestone,
        })),
  );
  const commandResults = context.command_mappings.map(
    ({ command_id: commandId, source_command_id: sourceCommandId }) => ({
      command_id: sourceCommandId,
      ...(commandId === imageCommandId
        ? {
            renderer_resource_submission_receipt: {
              renderer_resource_submission_observed: true,
              renderer_resource_submission_bytes: 786432,
              presented_after_native_input: false,
            },
            observation: {
              presentation: {
                element_present: true,
                visible: true,
                animation_frames_after_markup: 2,
              },
            },
          }
        : {}),
    }),
  );
  const exact = buildElectronV4ComponentEvidence(context, events, {
    input_lane: "cdp-input-diagnostic",
    exact_manifest_replay: false,
    command_results: commandResults,
  });
  const exactImage = exact.command_receipts.find(
    ({ command_id: commandId }) => commandId === imageCommandId,
  );
  assert.equal(exactImage.passed, true);
  assert.deepEqual(exactImage.missing_milestones, []);

  commandResults.find(
    ({ command_id: commandId }) => commandId === exactImage.source_command_id,
  ).renderer_resource_submission_receipt.renderer_resource_submission_observed =
    false;
  const absent = buildElectronV4ComponentEvidence(context, events, {
    input_lane: "cdp-input-diagnostic",
    exact_manifest_replay: false,
    command_results: commandResults,
  });
  const absentImage = absent.command_receipts.find(
    ({ command_id: commandId }) => commandId === imageCommandId,
  );
  assert.equal(absentImage.passed, false);
  assert(
    absentImage.missing_milestones.includes(
      "bitmap-presented-from-decoded-payload",
    ),
  );
});

test("v4 Electron scroll retains missing-raster rates as diagnostics", async () => {
  const [v3Workload, v4Workload] = await Promise.all([
    loadComparisonWorkload(),
    loadComparisonWorkloadV4(),
  ]);
  const context = createElectronV4ExecutionContext({
    parentContract: buildScenarioContractV4(v4Workload, "nasa-long-document"),
    parentWorkload: v4Workload,
    componentScenario: "continuous-scroll",
    componentContract: buildDevelopmentScenarioContract(
      v3Workload,
      "continuous-scroll",
      "native-x11-xtest",
    ),
    inputLane: "native-x11-xtest",
  });
  const command = context.component_contract.commands[0];
  assert(
    command.expected_milestones.includes("visible-raster-readiness-observed"),
  );
  assert.equal(
    command.expected_milestones.includes(
      "blank-current-generation-frames-zero",
    ),
    false,
  );
  const events = command.expected_milestones.map((milestone) => ({
    event: "comparison-milestone",
    command_id: command.id,
    milestone,
  }));
  const expanded = (frameCount) => ({
    input_lane: "native-x11-xtest",
    exact_manifest_replay: true,
    command_results: [
      {
        command_id: command.id,
        observed: {
          probe: {
            frame_count: frameCount,
            blank_current_generation_frames: frameCount === 0 ? 0 : 17,
          },
        },
      },
    ],
  });

  assert.equal(
    buildElectronV4ComponentEvidence(context, events, expanded(0))
      .component_receipts_passed,
    false,
  );
  const passed = buildElectronV4ComponentEvidence(
    context,
    events,
    expanded(120),
  );
  assert.equal(passed.component_receipts_passed, true);
  assert(
    passed.command_receipts[0].proven_milestones.includes(
      "visible-raster-readiness-observed",
    ),
  );
});

test("accepts the exact CDP components assigned by the central v4 runner", () => {
  for (const component of [
    "viewer-layout",
    "page-navigation",
    "zoom",
    "high-zoom-pan",
    "fit-modes",
    "cache-pressure",
    "close-reopen",
    "annotation-properties-history",
    "editor-workload",
    "persistence-workload",
    "cache-pressure-recovery",
  ]) {
    assert.equal(
      electronV4ComponentLaneEligible(component, "cdp-input-diagnostic"),
      true,
    );
  }
  for (const component of [
    "open-pdf",
    "annotation-create",
    "annotation-transform",
    "editor-create",
    "continuous-scroll",
  ]) {
    assert.equal(
      electronV4ComponentLaneEligible(component, "cdp-input-diagnostic"),
      false,
    );
    assert.equal(
      electronV4ComponentLaneEligible(component, "native-x11-xtest"),
      true,
    );
  }
});

test("qualifies engineering zoom only when every displayed generation stayed current", async () => {
  const [v3Workload, v4Workload] = await Promise.all([
    loadComparisonWorkload(),
    loadComparisonWorkloadV4(),
  ]);
  const componentContract = buildDevelopmentScenarioContract(
    v3Workload,
    "zoom",
    "native-x11-xtest",
  );
  const context = createElectronV4ExecutionContext({
    parentContract: buildScenarioContractV4(v4Workload, "engineering-sheet"),
    parentWorkload: v4Workload,
    componentScenario: "zoom",
    componentContract,
  });
  const events = componentContract.commands[0].expected_milestones.map(
    (milestone) => ({
      event: "comparison-milestone",
      command_id: "viewer:zoom-sequence",
      milestone,
    }),
  );
  const evidence = buildElectronV4ComponentEvidence(context, events, {
    exact_manifest_replay: true,
    command_results: [
      {
        command_id: "viewer:zoom-sequence",
        manifest_milestones_complete: true,
        exact_fields: {
          presentation_current: true,
          stale_generations_presented_zero: true,
        },
      },
    ],
  });
  const byId = new Map(
    evidence.command_receipts.map((receipt) => [receipt.command_id, receipt]),
  );

  assert.equal(evidence.component_receipts_passed, true);
  assert.deepEqual(evidence.parent_blocked_commands, []);
  assert.deepEqual(evidence.parent_blocked_command_receipts, []);
  assert.deepEqual(evidence.unmapped_command_ids, []);
  assert.equal(evidence.parent_execution_eligible, false);
  assert.deepEqual(
    byId.get("engineering:zoom-sequence").missing_milestones,
    [],
  );
  assert.equal(byId.get("engineering:zoom-sequence").passed, true);

  const failed = buildElectronV4ComponentEvidence(context, events, {
    exact_manifest_replay: true,
    command_results: [
      {
        command_id: "viewer:zoom-sequence",
        manifest_milestones_complete: true,
        exact_fields: {
          presentation_current: true,
          stale_generations_presented_zero: false,
        },
      },
    ],
  });
  assert.equal(failed.component_receipts_passed, false);
  assert.deepEqual(failed.command_receipts[0].missing_milestones, [
    "stale-generations-presented-zero",
  ]);
});

test("retains only the exact Electron engineering zoom density defect without passing its receipt", async () => {
  const [v3Workload, v4Workload] = await Promise.all([
    loadComparisonWorkload(),
    loadComparisonWorkloadV4(),
  ]);
  const componentContract = buildDevelopmentScenarioContract(
    v3Workload,
    "zoom",
    "semantic-diagnostic",
  );
  const context = createElectronV4ExecutionContext({
    parentContract: buildScenarioContractV4(v4Workload, "engineering-sheet"),
    parentWorkload: v4Workload,
    componentScenario: "zoom",
    componentContract,
    inputLane: "cdp-input-diagnostic",
  });
  const events = [
    {
      event: "comparison-milestone",
      command_id: "viewer:zoom-sequence",
      milestone: "zoom-state-current",
    },
  ];
  const zoomResults = [
    100, 200, 400, 800, 1600, 400, 100, 800, 200, 100, 1200, 100,
  ].map((percent) => ({
    diagnostics: { zoom: percent / 100 },
    presentation_current: true,
    render_page: { errors: 0 },
  }));
  const expanded = {
    input_lane: "cdp-input-diagnostic",
    exact_manifest_replay: false,
    command_results: [
      {
        command_id: "viewer:zoom-sequence",
        observations: {
          zoom_results: zoomResults,
          generation_probe: {
            frame_count: 12,
            stale_visible_surface_frames: 0,
          },
        },
        exact_fields: {
          presentation_current: true,
          stale_generations_presented_zero: true,
        },
      },
    ],
  };

  const evidence = buildElectronV4ComponentEvidence(context, events, expanded);
  assert.equal(evidence.component_receipts_passed, false);
  assert.equal(
    evidence.known_baseline_defect_id,
    "electron-engineering-zoom-density-and-raster-bound-v1",
  );
  assert.deepEqual(evidence.command_receipts[0].proven_milestones, [
    "zoom-state-current",
    "stale-generations-presented-zero",
  ]);
  assert.deepEqual(evidence.command_receipts[0].missing_milestones, [
    "visible-tiles-bounded",
    "settled-density-at-least-1",
  ]);

  expanded.command_results[0].observations.zoom_results[4].presentation_current =
    false;
  assert.equal(
    buildElectronV4ComponentEvidence(context, events, expanded)
      .known_baseline_defect_id,
    null,
  );
});

test("maps exact Electron renderer-resource submissions onto NASA cache pressure", async () => {
  const [v3Workload, v4Workload] = await Promise.all([
    loadComparisonWorkload(),
    loadComparisonWorkloadV4(),
  ]);
  const componentContract = buildDevelopmentScenarioContract(
    v3Workload,
    "cache-pressure",
  );
  const context = createElectronV4ExecutionContext({
    parentContract: buildScenarioContractV4(v4Workload, "nasa-long-document"),
    parentWorkload: v4Workload,
    componentScenario: "cache-pressure",
    componentContract,
  });
  const events = [
    "declared-cache-byte-limit-held",
    "decoded-byte-limit-held",
  ].map((milestone) => ({
    event: "comparison-milestone",
    command_id: "viewer:cache-pressure",
    milestone,
  }));
  const cycle = {
    renderer_resource_submission_bytes: 4_000_000,
    physical_bus_upload_bytes: null,
  };
  const exact = buildElectronV4ComponentEvidence(context, events, {
    input_lane: "cdp-input-diagnostic",
    exact_manifest_replay: false,
    semantic_manifest_replay: true,
    command_results: [
      {
        command_id: "viewer:cache-pressure",
        semantic_passed: true,
        observation: {
          cycles: Array.from({ length: 5 }, () => ({ ...cycle })),
          cache: { renderer_resource_submission_byte_limit: 32 * 1024 * 1024 },
        },
      },
    ],
  });
  assert.equal(exact.component_receipts_passed, true);
  assert.deepEqual(exact.command_receipts[0].proven_milestones, [
    "declared-cache-byte-limit-held",
    "decoded-byte-limit-held",
    "renderer-resource-submission-bytes-exact",
  ]);

  const absent = buildElectronV4ComponentEvidence(context, events, {
    input_lane: "cdp-input-diagnostic",
    exact_manifest_replay: false,
    semantic_manifest_replay: true,
    command_results: [
      {
        command_id: "viewer:cache-pressure",
        semantic_passed: true,
        observation: {
          cycles: Array.from({ length: 5 }, (_, index) => ({
            ...cycle,
            renderer_resource_submission_bytes:
              index === 4 ? 0 : cycle.renderer_resource_submission_bytes,
          })),
          cache: { renderer_resource_submission_byte_limit: 32 * 1024 * 1024 },
        },
      },
    ],
  });
  assert.equal(absent.component_receipts_passed, false);
});

test("maps timestamped native Electron pan evidence onto engineering pan", async () => {
  const [v3Workload, v4Workload] = await Promise.all([
    loadComparisonWorkload(),
    loadComparisonWorkloadV4(),
  ]);
  const componentContract = buildDevelopmentScenarioContract(
    v3Workload,
    "high-zoom-pan",
    "native-x11-xtest",
  );
  const context = createElectronV4ExecutionContext({
    parentContract: buildScenarioContractV4(v4Workload, "engineering-sheet"),
    parentWorkload: v4Workload,
    componentScenario: "high-zoom-pan",
    componentContract,
  });
  const events = componentContract.commands[0].expected_milestones.map(
    (milestone) => ({
      event: "comparison-milestone",
      command_id: "viewer:pan-usgs",
      milestone,
    }),
  );
  const expanded = {
    input_lane: "native-x11-xtest",
    exact_manifest_replay: true,
    command_results: [
      {
        command_id: "viewer:pan-usgs",
        exact_fields: {
          timing: true,
          visible_tiles_bounded: true,
          stale_generations_presented_zero: true,
          settled_density_at_least_1: true,
        },
      },
    ],
  };
  assert.equal(
    buildElectronV4ComponentEvidence(context, events, expanded)
      .component_receipts_passed,
    true,
  );
  expanded.command_results[0].exact_fields.timing = false;
  assert.equal(
    buildElectronV4ComponentEvidence(context, events, expanded)
      .component_receipts_passed,
    false,
  );
});

test("reports a missing NASA or USGS file as a locked-corpus blocker", () => {
  assert.match(
    formatFixtureAccessError(
      new Error("ENOENT"),
      { fixture_id: "nasa-apollo-summary-526-v1" },
      "/cache/nasa.pdf",
    ),
    /^BLOCKED locked corpus nasa-apollo-summary-526-v1 is absent/,
  );
});

test("labels expanded Electron replay as a CDP diagnostic and never decision timing", async () => {
  const workload = await loadComparisonWorkload();
  for (const scenario of [
    "annotation-create",
    "editor-create",
    "continuous-scroll",
  ]) {
    const metadata = runnerComparisonMetadata(workload, "electron", scenario);
    assert.equal(metadata.execution_lane, "cdp-input-diagnostic");
    assert.equal(metadata.scenario_status, "supported-diagnostic");
    assert.equal(metadata.diagnostic_timing_eligible, true);
    assert.equal(metadata.decision_timing_eligible, false);
    assert.equal(metadata.feature_coverage.ready, false);
  }
  for (const scenario of ["viewer-layout", "close-reopen"]) {
    const metadata = runnerComparisonMetadata(workload, "electron", scenario);
    assert.equal(metadata.execution_lane, "development-subset");
    assert.equal(metadata.scenario_status, "supported-diagnostic");
    assert.equal(metadata.diagnostic_timing_eligible, true);
    assert.equal(metadata.decision_timing_eligible, false);
    assert.equal(metadata.feature_coverage.ready, false);
  }
  const highZoomPan = runnerComparisonMetadata(
    workload,
    "electron",
    "high-zoom-pan",
  );
  assert.equal(highZoomPan.scenario_status, "blocked-unsupported");
  assert.equal(
    highZoomPan.blocked_reason,
    "electron-high-zoom-pan-live-proof-missing",
  );
});

test("native X11 Electron timing fails closed until exact replay and full feature coverage", async () => {
  const workload = await loadComparisonWorkload();
  const notRun = electronComparisonMetadata(
    workload,
    "annotation-create",
    "native-x11-xtest",
    [],
  );
  assert.equal(notRun.execution_lane, "native-x11-xtest");
  assert.equal(notRun.diagnostic_timing_eligible, false);
  assert.equal(notRun.decision_timing_eligible, false);
  assert.equal(
    notRun.blocked_reason,
    "native-replay-has-not-passed-exact-milestones-and-timing",
  );

  const exactDiagnostic = electronComparisonMetadata(
    workload,
    "annotation-create",
    "native-x11-xtest",
    [
      {
        success: true,
        renderer: { expanded_comparison: { exact_manifest_replay: true } },
      },
    ],
  );
  assert.equal(exactDiagnostic.diagnostic_timing_eligible, true);
  assert.equal(exactDiagnostic.decision_timing_eligible, false);
  assert.equal(
    exactDiagnostic.blocked_reason,
    "full-comparison-feature-coverage-incomplete",
  );
});

test("Electron v5 contexts bind only exact hard-component commands and frozen bytes", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const property = createElectronV5ExecutionContext({
    workload,
    parentScenario: "dense-mixed-editing",
    componentScenario: "native-property-edit-undo",
  });
  assert.equal(property.protocol_version, "bp-perf-v5");
  assert.equal(
    property.workload_artifact_sha256,
    "cc4f8b8940556390b8d16a6baae43e8a5a022541fba90beea08869e692ee920e",
  );
  assert.equal(
    property.workload_byte_sha256,
    "e7b2540c7d455a30e52ee64a6819745fe0ad49a6512f887df4631aac72054f6d",
  );
  assert.deepEqual(property.execution_contract.command_ids, [
    "annotation:native-property-edit-undo",
  ]);
  assert.deepEqual(property.execution_contract.fixture_ids, [
    "bp-annotation-density-v1",
  ]);
  assert.equal(property.benefit_metrics_eligible, false);
  assert.throws(
    () =>
      createElectronV5ExecutionContext({
        workload,
        parentScenario: "dense-mixed-editing",
        componentScenario: "multi-document-session",
      }),
    /is not a component/,
  );
});

test("Electron v6 context binds an inherited native benefit component without replacing v4 execution", async () => {
  const { workload, byte_sha256: workloadByteSha256 } =
    await loadComparisonWorkloadV6();
  const context = createElectronV6ExecutionContext({
    workload,
    workloadByteSha256,
    parentScenario: "small-shell-open",
    componentScenario: "open-pdf",
  });

  assert.equal(context.protocol_version, "bp-perf-v6");
  assert.equal(
    context.scenario_contract_version,
    "bp-perf-v6-representative-1",
  );
  assert.equal(context.manifest_id, "bp-perf-v6-decision-2");
  assert.equal(context.benefit_metrics_eligible, true);
  assert.equal(context.parent_scenario, "small-shell-open");
  assert.equal(context.component_scenario, "open-pdf");
  assert.deepEqual(context.execution_contract.fixture_ids, [
    "bp-single-page-v1",
  ]);
  assert.throws(
    () =>
      createElectronV6ExecutionContext({
        workload,
        workloadByteSha256,
        parentScenario: "engineering-sheet",
        componentScenario: "zoom",
      }),
    /is not a benefit-eligible component/,
  );
  assert.throws(
    () =>
      createElectronV6ExecutionContext({
        workload,
        workloadByteSha256: "0".repeat(64),
        parentScenario: "small-shell-open",
        componentScenario: "open-pdf",
      }),
    /workload byte SHA-256 changed/,
  );
});

test("Electron CLI accepts v6 metadata only beside the matching inherited native v4 path", () => {
  const options = parseElectronRunnerArguments([
    "--scenario",
    "open-pdf",
    "--v4-scenario",
    "small-shell-open",
    "--v6-scenario",
    "small-shell-open",
    "--pdf",
    "/fixtures/single.pdf",
    "--input-lane",
    "native-x11-xtest",
  ]);
  assert.equal(options.v4Scenario, "small-shell-open");
  assert.equal(options.v6Scenario, "small-shell-open");
  assert.match(options.output, /electron-v6-small-shell-open-open-pdf\.json$/);

  const base = [
    "--scenario",
    "open-pdf",
    "--pdf",
    "/fixtures/single.pdf",
    "--input-lane",
    "native-x11-xtest",
  ];
  assert.throws(
    () =>
      parseElectronRunnerArguments([
        ...base,
        "--v6-scenario",
        "small-shell-open",
      ]),
    /requires the inherited --v4-scenario/,
  );
  assert.throws(
    () =>
      parseElectronRunnerArguments([
        ...base,
        "--v4-scenario",
        "small-shell-open",
        "--v6-scenario",
        "nasa-long-document",
      ]),
    /must name the same parent journey/,
  );
  assert.throws(
    () =>
      parseElectronRunnerArguments([
        "--scenario",
        "zoom",
        "--v4-scenario",
        "engineering-sheet",
        "--v6-scenario",
        "engineering-sheet",
        "--pdf",
        "/fixtures/engineering.pdf",
        "--input-lane",
        "native-x11-xtest",
      ]),
    /is not a benefit-eligible component/,
  );
});

test("Electron CLI accepts exact v5 component parents and repeated multi-document fixtures", () => {
  const property = parseElectronRunnerArguments([
    "--scenario",
    "native-property-edit-undo",
    "--v5-scenario",
    "dense-mixed-editing",
    "--pdf",
    "/fixtures/dense.pdf",
    "--input-lane",
    "native-x11-xtest",
  ]);
  assert.equal(property.v5Scenario, "dense-mixed-editing");
  assert.deepEqual(property.pdfs, ["/fixtures/dense.pdf"]);

  const multi = parseElectronRunnerArguments([
    "--scenario",
    "multi-document-session",
    "--pdf",
    "/fixtures/one.pdf",
    "--pdf",
    "/fixtures/nasa.pdf",
    "--pdf",
    "/fixtures/engineering.pdf",
    "--pdf",
    "/fixtures/dense.pdf",
    "--input-lane",
    "native-x11-xtest",
  ]);
  assert.equal(multi.pdf, "/fixtures/one.pdf");
  assert.equal(multi.pdfs.length, 4);
  const dynamic = parseElectronRunnerArguments([
    "--scenario",
    "viewer-dynamic-fidelity",
    "--v5-scenario",
    "nasa-long-document",
    "--pdf",
    "/fixtures/nasa.pdf",
    "--input-lane",
    "native-x11-xtest",
  ]);
  assert.equal(dynamic.v5Scenario, "nasa-long-document");
  assert.deepEqual(dynamic.pdfs, ["/fixtures/nasa.pdf"]);
  assert.equal(
    electronV5ComponentScenarios.has("viewer-dynamic-fidelity"),
    true,
  );
  assert.throws(
    () =>
      parseElectronRunnerArguments([
        "--scenario",
        "native-snap-transform-120hz",
        "--pdf",
        "/fixtures/dense.pdf",
      ]),
    /requires --v5-scenario dense-mixed-editing/,
  );
});

test("Electron v5 multi-document fixtures fail closed on count, order, or hash", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const context = createElectronV5ExecutionContext({
    workload,
    parentScenario: "multi-document-session",
    componentScenario: "multi-document-session",
  });
  const pdfs = context.execution_contract.fixture_ids.map((fixtureId) => ({
    sha256: context.execution_contract.fixture_sha256_by_id[fixtureId],
  }));
  assert.equal(
    validateOrderedElectronScenarioFixtures(pdfs, context.execution_contract),
    null,
  );
  assert.match(
    validateOrderedElectronScenarioFixtures(
      pdfs.slice(0, 3),
      context.execution_contract,
    ),
    /requires 4 ordered PDF fixtures/,
  );
  assert.match(
    validateOrderedElectronScenarioFixtures(
      [pdfs[1], pdfs[0], pdfs[2], pdfs[3]],
      context.execution_contract,
    ),
    /ordered --pdf index 0/,
  );
});

test("Electron native v5 snap replay has 361 inclusive timestamped samples", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const command = createElectronV5ExecutionContext({
    workload,
    parentScenario: "dense-mixed-editing",
    componentScenario: "native-snap-transform-120hz",
  }).execution_contract.commands[0];
  const plan = buildElectronNativeSnapReplayPlan(command);
  assert.equal(plan.rate_hz, 120);
  assert.equal(plan.duration_ms, 3000);
  assert.equal(plan.pdf_samples.length, 361);
  assert.deepEqual(plan.pdf_samples[0], { x: 162, y: 252, t_ms: 0 });
  assert.deepEqual(plan.pdf_samples.at(-1), {
    x: 259,
    y: 335,
    t_ms: 3000,
  });
  assert.equal(plan.grid_spacing_mm, 6.35);
  assert.deepEqual(plan.sensitivity, {
    value: 8,
    unit: "css-px",
    threshold_norm: "per-axis-l-infinity",
    inclusive: true,
  });
});

test("Electron v5 native property assessment requires one commit and one undo", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const command = createElectronV5ExecutionContext({
    workload,
    parentScenario: "dense-mixed-editing",
    componentScenario: "native-property-edit-undo",
  }).execution_contract.commands[0];
  const observed = {
    trusted_native_input: true,
    before: 1.5,
    committed: 4,
    after_undo: 1.5,
    commit_count: 1,
    undo_count: 1,
    canonical_state_restored: true,
    native_presentation_acknowledged: true,
    thumbnail_current: true,
  };
  assert.equal(
    assessElectronNativePropertyEditUndoV5(command, observed).passed,
    true,
  );
  assert.equal(
    assessElectronNativePropertyEditUndoV5(command, {
      ...observed,
      commit_count: 2,
    }).passed,
    false,
  );
  const electronBaseline = assessElectronNativePropertyEditUndoV5(command, {
    ...observed,
    after_undo: 4,
    commit_count: 2,
    undo_count: 1,
    effective_history_revision_delta: 2,
    application_undo_count: 1,
    canonical_state_restored: false,
    known_baseline_defect_id:
      "electron-numeric-property-input-blur-duplicate-history-v1",
  });
  assert.equal(electronBaseline.passed, true);
  assert.equal(electronBaseline.candidate_passed, false);
  assert.equal(electronBaseline.electron_baseline_passed, true);
});

test("Electron v5 native snap assessment rejects near-exact geometry", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const command = createElectronV5ExecutionContext({
    workload,
    parentScenario: "dense-mixed-editing",
    componentScenario: "native-snap-transform-120hz",
  }).execution_contract.commands[0];
  const observed = {
    trusted_native_input: true,
    observed_sample_count: 361,
    snap_enabled: true,
    sensitivity_css_px: 8,
    observed_pixels_per_point: 0.75,
    derived_threshold_points: 8 / 0.75,
    observed_raw_delta_points: { x: 97, y: 83 },
    observed_snap_correction_points: { x: -7, y: 7 },
    snap_target_acquired_count: 1,
    snap_guide_presented_count: 1,
    observed_final_rectangle: command.expected_final_rectangle,
    gesture_commit_count: 1,
    undo_redo_exact: true,
    thumbnail_current: true,
  };
  assert.equal(
    assessElectronNativeSnapTransformV5(command, observed).passed,
    true,
  );
  const drifted = {
    ...observed,
    observed_final_rectangle: {
      ...command.expected_final_rectangle,
      x1: command.expected_final_rectangle.x1 + 0.001,
    },
  };
  assert.equal(
    assessElectronNativeSnapTransformV5(command, drifted).passed,
    false,
  );
  assert.equal(
    electronV5ComponentScenarios.has("multi-document-session"),
    true,
  );
});

test("Electron v5 multi-document assessment retains the measured two-revision baseline", () => {
  const summary = {
    opened_fixture_ids: [
      "bp-single-page-v1",
      "nasa-apollo-summary-526-v1",
      "bp-engineering-sheet-v1",
      "bp-annotation-density-v1",
    ],
    switch_sequence: [
      "nasa-apollo-summary-526-v1",
      "bp-single-page-v1",
      "bp-engineering-sheet-v1",
      "bp-annotation-density-v1",
    ],
    close_sequence: [
      "bp-single-page-v1",
      "bp-engineering-sheet-v1",
      "nasa-apollo-summary-526-v1",
    ],
    process_restart_count: 0,
    observed_process_ids: [1234],
    stable_process_id: 1234,
    per_document_state_isolated: true,
    current_raster_receipt_count: 8,
    dense_rectangle_property_user_gesture_count: 1,
    dense_rectangle_property_history_revision_delta: 2,
    dense_rectangle_stroke_width_points: 4,
    closed_document_resources_released: true,
    remaining_document_count: 1,
    remaining_fixture_id: "bp-annotation-density-v1",
    dense_document_active: true,
    aggregate_resource_observations_complete: true,
    interactive_document_shell: true,
  };
  assert.equal(assessElectronMultiDocumentSessionV5(summary).passed, true);
  assert.equal(
    assessElectronMultiDocumentSessionV5({
      ...summary,
      dense_rectangle_property_history_revision_delta: 1,
    }).passed,
    false,
  );
});

test("classifies an active second tab with no visible layouts or render work as a product scheduler defect", () => {
  assert.deepEqual(
    classifyElectronSecondTabRasterBlocker({
      requested_path: "/fixtures/nasa.pdf",
      active_path: "/fixtures/nasa.pdf",
      tab_count: 2,
      page_count: 526,
      visible_page_indices: [],
      queued_page_renders: 0,
      inflight_page_renders: 0,
      last_page_render_error: null,
      session_last_page_render_error: null,
    }),
    {
      classification: "maintained-product-visible-layout-scheduling-defect",
      nvidia_expected_to_change_outcome: false,
    },
  );
  assert.equal(
    classifyElectronSecondTabRasterBlocker({
      requested_path: "/fixtures/nasa.pdf",
      active_path: "/fixtures/one.pdf",
      tab_count: 1,
    }).classification,
    "runner-sequencing-or-open-failure",
  );
});

test("materializes only the exact bounded live second-NASA defect as benchmark-ineligible", () => {
  const observed = {
    requested_path: "/fixtures/nasa.pdf",
    active_path: "/fixtures/nasa.pdf",
    tab_count: 2,
    page_count: 526,
    visible_page_indices: [],
    queued_page_renders: 0,
    inflight_page_renders: 0,
    last_page_render_error: null,
    session_last_page_render_error: null,
    activated_fixture_id: "nasa-apollo-summary-526-v1",
    activation_ordinal: 2,
    live_application_observed: true,
    bounded_wait_completed: true,
    live_observation_duration_ms: 5_001,
    visible_raster_presented: false,
    error_presented: false,
  };
  assert.deepEqual(buildElectronSecondNasaBaselineSummary(observed), {
    known_baseline_defect_id:
      "electron-multi-document-second-nasa-visible-pages-empty-v1",
    activated_fixture_id: "nasa-apollo-summary-526-v1",
    activation_ordinal: 2,
    visible_page_indices: [],
    queued_raster_count: 0,
    inflight_raster_count: 0,
    visible_raster_presented: false,
    error_presented: false,
    benchmark_metrics_eligible: false,
    benchmark_metrics_missing: [
      "cpu_seconds",
      "cgroup_peak_memory_bytes",
      "product_wall_or_latency_ms",
      "application_frame_interval_p95_ms",
      "native_input_to_application_frame_ack_p95_ms",
      "baseline_adjusted_gpu_peak_memory_mib",
      "baseline_adjusted_gpu_utilization_p95_percent",
    ],
  });
  assert.equal(
    buildElectronSecondNasaBaselineSummary({
      ...observed,
      live_observation_duration_ms: 4_999,
    }),
    null,
  );
  assert.equal(
    buildElectronSecondNasaBaselineSummary({
      ...observed,
      last_page_render_error: "GPU context lost",
    }),
    null,
  );
});

test("accepts a dynamic crop only when the screenshot brackets one stable page position", () => {
  const stable = {
    state_sequence: 70,
    painted_state_sequence: 70,
    scroll_offset_css_px: { x: 0, y: 12_038 },
    visible_pages: [
      {
        page_number: 15,
        painted_outer_page_bounds_window_logical: {
          x: 320,
          y: 140,
          width: 640,
          height: 828,
        },
        current_raster_ready_area_fraction: 1,
        painted_generation_current: true,
        painted_render_generation: 42,
      },
    ],
  };
  const sequenceAdvanced = structuredClone(stable);
  sequenceAdvanced.state_sequence = 71;
  sequenceAdvanced.painted_state_sequence = 71;
  assert.equal(
    electronCropCaptureStateStable(stable, sequenceAdvanced, 15),
    true,
  );
  assert.equal(
    electronCropCaptureStateStable(
      stable,
      {
        ...structuredClone(stable),
        scroll_offset_css_px: { x: 0, y: 12_158 },
      },
      15,
    ),
    false,
  );
  assert.equal(electronCropCaptureStateStable(stable, stable, 29), false);
});

test("Electron v5 receipts preserve exact frozen milestone order and fail closed", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const context = createElectronV5ExecutionContext({
    workload,
    parentScenario: "dense-mixed-editing",
    componentScenario: "native-property-edit-undo",
  });
  const command = context.execution_contract.commands[0];
  const result = {
    command_id: command.id,
    observed_milestones: [...command.expected_milestones],
    manifest_milestones_complete: true,
    exact_fields: Object.fromEntries(
      command.expected_milestones.map((milestone) => [milestone, true]),
    ),
  };
  const passing = buildElectronV5ComponentEvidence(context, [result], {
    trusted_native_input: true,
  });
  assert.equal(passing.passed, true);
  assert.deepEqual(
    passing.command_receipts[0].proven_milestones,
    command.expected_milestones,
  );
  assert.match(passing.command_receipts[0].evidence_sha256, /^[0-9a-f]{64}$/);

  const reordered = buildElectronV5ComponentEvidence(
    context,
    [
      {
        ...result,
        observed_milestones: [...command.expected_milestones].reverse(),
      },
    ],
    {},
  );
  assert.equal(reordered.passed, false);
});

test("Electron second-NASA receipt retains one honest partial live command and three non-live commands", async () => {
  const workload = await loadMaterializedComparisonWorkloadV5();
  const context = createElectronV5ExecutionContext({
    workload,
    parentScenario: "multi-document-session",
    componentScenario: "multi-document-session",
  });
  const [openCommand, ...unexecutedCommands] =
    context.execution_contract.commands;
  const results = [
    {
      command_id: openCommand.id,
      live: true,
      observed_milestones: ["application-process-id-recorded"],
      manifest_milestones_complete: false,
      exact_fields: Object.fromEntries(
        openCommand.expected_milestones.map((milestone) => [
          milestone,
          milestone === "application-process-id-recorded",
        ]),
      ),
    },
    ...unexecutedCommands.map((command) => ({
      command_id: command.id,
      live: false,
      observed_milestones: [],
      manifest_milestones_complete: false,
      exact_fields: Object.fromEntries(
        command.expected_milestones.map((milestone) => [milestone, false]),
      ),
    })),
  ];
  const evidence = buildElectronV5ComponentEvidence(context, results, {
    known_baseline_defect_id:
      "electron-multi-document-second-nasa-visible-pages-empty-v1",
  });
  assert.equal(evidence.passed, false);
  assert.equal(evidence.benefit_metrics_eligible, false);
  assert.deepEqual(
    evidence.command_receipts.map(({ live, passed, proven_milestones }) => ({
      live,
      passed,
      proven_milestones,
    })),
    [
      {
        live: true,
        passed: false,
        proven_milestones: ["application-process-id-recorded"],
      },
      { live: false, passed: false, proven_milestones: [] },
      { live: false, passed: false, proven_milestones: [] },
      { live: false, passed: false, proven_milestones: [] },
    ],
  );
  assert.ok(
    evidence.command_receipts.every(({ evidence_sha256 }) =>
      /^[0-9a-f]{64}$/.test(evidence_sha256),
    ),
  );
});

test("Electron v5 retains maintained baseline disparities without injecting benchmark state", () => {
  const property = electronV5MaintainedUiCapability(
    "native-property-edit-undo",
  );
  assert.equal(property.supported, true);
  assert.match(property.maintained_baseline, /known-baseline-history-defect/);
  assert.equal(property.mutation_path, "maintained-public-ui-only");

  const snap = electronV5MaintainedUiCapability("native-snap-transform-120hz");
  assert.equal(snap.supported, true);
  assert.match(snap.maintained_baseline, /8 CSS px sensitivity/);

  const multi = electronV5MaintainedUiCapability("multi-document-session");
  assert.equal(multi.supported, true);
  assert.match(multi.maintained_baseline, /two history revisions/);
});
