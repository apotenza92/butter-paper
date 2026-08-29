import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { EventEmitter } from "node:events";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { PassThrough } from "node:stream";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

import { loadComparisonWorkload } from "./comparison-workload.mjs";
import {
  assessReplayTiming,
  buildClickReplay,
  buildDirectClickReplay,
  buildDistanceBoundedDynamicWheelPlan,
  buildHeldDynamicWheelPlan,
  buildNativeViewerShellReplay,
  buildNativeCommandReplay,
  buildNativeRectangleTransformReplay,
  buildNativeV5SnapReplay,
  buildPointerReplay,
  buildWheelReplay,
  captureLongbridgeCompatPresentedCrop,
  closePresentedDrawableCaptureServerStrict,
  bindDynamicObserverSamples,
  dynamicCaptureSemanticTuple,
  dynamicHelperHoldState,
  dynamicRunnerMilestonesV5,
  executeNativeX11Scenario,
  gpuiPaintedCropStateStable,
  heldDynamicTrajectoryPassed,
  losslesslyConvertPresentedPpmToPng,
  longbridgeCompatCaptureSignalProtocol,
  nativeX11LaneMetadata,
  parseX11FrameExtents,
  parseXdotoolGeometry,
  parseXwininfoClientGeometry,
  parseNativeClockSamples,
  parsePresentedDrawableCaptureReceipt,
  pdfPointToWindowPixel,
  registeredDynamicCropsPassed,
  requireDynamicHelperHold,
  retainDynamicCaptureFailureEvidence,
  runNativeX11HelperSelfTest,
  startIndependentDynamicObserver,
  startPresentedDrawableCaptureServer,
  validatePresentedCapturePairCorrelation,
  validatePresentedDrawableCaptureReceipt,
  validateWindowTarget,
  validateGpuiLogicalWindowTarget,
  validateDynamicWheelCalibrationReceipt,
  validateNativeViewerLaunchOpenEvidence,
  validateNativeRectangleTransformEvidence,
  windowLogicalPointToPixel,
} from "./gpui-native-x11.mjs";
import { loadComparisonWorkloadV5 } from "./comparison-workload-v5.mjs";
import { buildScenarioContractV5 } from "./scenario-contract-v5.mjs";
import { validateCompatPresentedCrop } from "./compat-evidence-validator.mjs";

const performanceDirectory = dirname(fileURLToPath(import.meta.url));

async function compatCropHarness(
  t,
  {
    screenshotColor = { r: 0, g: 0, b: 0 },
    referenceColor = { r: 0, g: 0, b: 0 },
  } = {},
) {
  const directory = await mkdtemp(resolve(tmpdir(), "bp-compat-crop-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const screenshotPixels = await sharp({
    create: {
      width: 1200,
      height: 800,
      channels: 3,
      background: { r: 255, g: 255, b: 255 },
    },
  })
    .composite([
      {
        input: await sharp({
          create: {
            width: 540,
            height: 720,
            channels: 3,
            background: screenshotColor,
          },
        })
          .png()
          .toBuffer(),
        left: 136,
        top: 40,
      },
    ])
    .removeAlpha()
    .raw()
    .toBuffer();
  const screenshot = Buffer.concat([
    Buffer.from("P6\n1200 800\n255\n", "ascii"),
    screenshotPixels,
  ]);
  const state = {
    event: "viewer-native-open-evidence",
    command_id: "viewer:open-each",
    comparison_command_id: "small:open-settle",
    fixture_id: "bp-single-page-v1",
    page_id: "bp-single-page-v1:page:001",
    page_size_points: { width: 612, height: 792 },
    painted_outer_page_bounds_window_logical: {
      x: 100,
      y: 4,
      width: 612,
      height: 792,
    },
    window_logical_size: { width: 1200, height: 800 },
    display_scale_factor: 1,
    rendered_device_pixel_ratio: 1,
    painted_render_generation: 4,
    painted_state_sequence: 9,
    runner_observed_monotonic_ms: 100,
  };
  return {
    directory,
    state,
    args: {
      openEvidence: state,
      waitForPostCaptureEvidence: async () => ({
        ...state,
        event: "viewer-native-presented-state",
        runner_observed_monotonic_ms: 140,
      }),
      target: {
        window_id: "4194305",
        geometry: { width: 1200, height: 800 },
      },
      artifactDirectory: directory,
      fixturePath: resolve(
        performanceDirectory,
        "results/public-fixtures-v1/bp-single-page-v1.pdf",
      ),
      registrationPath: resolve(
        performanceDirectory,
        "results/public-fixtures-v1/bp-single-page-v1.crops.json",
      ),
      captureServer: {
        async capture(captureId, path) {
          await writeFile(path, screenshot);
          return {
            event: "presented-drawable-captured",
            capture_id: captureId,
            capture_started_monotonic_ms: 120,
            capture_ended_monotonic_ms: 130,
            window_id: "4194305",
            width: 1200,
            height: 800,
            depth: 24,
            artifact_sha256: createHash("sha256")
              .update(screenshot)
              .digest("hex"),
            ppm_path: path,
            source: "XGetImage-presented-client-drawable",
          };
        },
        async close() {
          return {
            closed: {
              event: "capture-server-closed",
              observed_monotonic_ms: 150,
            },
            outcome: { code: 0, signal: null },
          };
        },
      },
      renderReference: async ({ outputPath }) => {
        await sharp({
          create: {
            width: 1080,
            height: 1440,
            channels: 3,
            background: referenceColor,
          },
        })
          .png()
          .toFile(outputPath);
        return {
          page_number: 1,
          page_size_points: { width: 612, height: 792 },
          dpi: 144,
        };
      },
    },
  };
}

function fakeCaptureHelperProcess() {
  const child = new EventEmitter();
  child.stdout = new PassThrough();
  child.stderr = new PassThrough();
  child.stdin = new PassThrough();
  child.exitCode = null;
  child.signalCode = null;
  child.killed = false;
  child.kill = (signal) => {
    child.killed = true;
    child.signalCode = signal;
    queueMicrotask(() => child.emit("close", null, signal));
    return true;
  };
  return child;
}

test("native dynamic runner owns the exact eight nonterminal v5 milestones", async () => {
  const contract = buildScenarioContractV5(
    await loadComparisonWorkloadV5(),
    "nasa-long-document",
  );
  const command = contract.commands.find(
    ({ id }) => id === "viewer:dynamic-fidelity-scroll",
  );
  const expected = command.expected_milestones.filter(
    (milestone) =>
      !["virtual-page-window-bounded", "finish-page-current"].includes(
        milestone,
      ),
  );
  assert.deepEqual(dynamicRunnerMilestonesV5, expected);
});

test("native helper RGB-mask self-test passes under the strict compiled helper", async () => {
  assert.equal(await runNativeX11HelperSelfTest(), true);
});

test("streams native helper clock samples before its completion receipt", async () => {
  const session = await startIndependentDynamicObserver({
    durationMs: 500,
    rateHz: 4,
  });
  const deadline = Date.now() + 250;
  while (session.feed.samples.length === 0 && Date.now() < deadline) {
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 2));
  }
  assert.equal(session.feed.samples[0]?.sample_index, 0);
  assert.equal(session.feed.samples[0]?.action, "observe");
  assert.equal(session.feed.complete, false);
  const samples = await session.completion;
  assert.equal(samples.length, 3);
  assert.equal(session.feed.complete, true);
});

test("parses and validates exact XGetImage presented-drawable receipts", () => {
  const target = {
    window_id: "4194305",
    geometry: { width: 1200, height: 800 },
  };
  const ready = parsePresentedDrawableCaptureReceipt(
    "ready\t100.25\t4194305\t1200\t800\t24",
  );
  assert.equal(
    validatePresentedDrawableCaptureReceipt(ready, target).source,
    "XGetImage-presented-client-drawable",
  );
  const capture = parsePresentedDrawableCaptureReceipt(
    `capture\tpage-1:before\t110.0\t112.5\t4194305\t1200\t800\t24\t${"a".repeat(64)}\t/tmp/page-1.ppm`,
  );
  assert.equal(
    validatePresentedDrawableCaptureReceipt(capture, target, {
      captureId: "page-1:before",
      ppmPath: "/tmp/page-1.ppm",
    }).capture_ended_monotonic_ms,
    112.5,
  );
  assert.equal(capture.artifact_sha256, "a".repeat(64));
  assert.throws(
    () =>
      validatePresentedDrawableCaptureReceipt(
        { ...capture, window_id: "4194306" },
        target,
      ),
    /does not match the verified X11 client/,
  );
  assert.throws(
    () =>
      parsePresentedDrawableCaptureReceipt(
        `capture\tpage-1\t113\t112\t4194305\t1200\t800\t24\t${"a".repeat(64)}\t/tmp/x.ppm`,
      ),
    /receipt is invalid/,
  );
});

test("losslessly decodes the capture server's binary P6 PPM output", async (t) => {
  const directory = await mkdtemp(resolve(tmpdir(), "bp-x11-ppm-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const ppmPath = resolve(directory, "capture.ppm");
  const pngPath = resolve(directory, "capture.png");
  await writeFile(
    ppmPath,
    Buffer.concat([
      Buffer.from("P6\n2 1\n255\n", "ascii"),
      Buffer.from([0, 0, 0, 255, 255, 255]),
    ]),
  );
  const receipt = await losslesslyConvertPresentedPpmToPng(ppmPath, pngPath);
  assert.equal(receipt.lossless_conversion_verified, true);
  assert.equal(receipt.width, 2);
  assert.equal(receipt.height, 1);
  assert.deepEqual(
    [...(await sharp(pngPath).raw().toBuffer())],
    [0, 0, 0, 255, 255, 255],
  );
});

test("capture-helper acceptance requires graceful close and reaps a failed helper", async () => {
  assert.deepEqual(
    await closePresentedDrawableCaptureServerStrict({
      async close() {
        return {
          closed: {
            event: "capture-server-closed",
            observed_monotonic_ms: 150,
          },
          outcome: { code: 0, signal: null },
        };
      },
    }),
    {
      closed: {
        event: "capture-server-closed",
        observed_monotonic_ms: 150,
      },
      outcome: { code: 0, signal: null },
    },
  );

  let reaped = false;
  await assert.rejects(
    closePresentedDrawableCaptureServerStrict({
      async close() {
        throw new Error("close receipt timeout");
      },
      async terminateAndWait() {
        reaped = true;
        return { exited: true, outcome: { code: null, signal: "SIGTERM" } };
      },
    }),
    /close receipt timeout/,
  );
  assert.equal(reaped, true);

  await assert.rejects(
    closePresentedDrawableCaptureServerStrict({
      async close() {
        return { closed: null, outcome: { code: 0, signal: null } };
      },
      async terminateAndWait() {
        return { exited: true, outcome: { code: 0, signal: null } };
      },
    }),
    /valid close receipt and zero exit/,
  );
});

test("capture server reaps its process when readiness or graceful close times out", async () => {
  const keepAlive = setInterval(() => {}, 100);
  const target = {
    window_id: "4194305",
    geometry: { width: 1200, height: 800 },
  };
  try {
    const readinessChild = fakeCaptureHelperProcess();
    await assert.rejects(
      startPresentedDrawableCaptureServer(target, {
        timeoutMs: 5,
        spawnProcess: () => readinessChild,
      }),
      /readiness/,
    );
    assert.equal(readinessChild.signalCode, "SIGTERM");

    const closeChild = fakeCaptureHelperProcess();
    const serverPromise = startPresentedDrawableCaptureServer(target, {
      timeoutMs: 5,
      spawnProcess: () => closeChild,
    });
    closeChild.stdout.write("ready\t100.25\t4194305\t1200\t800\t24\n");
    const server = await serverPromise;
    await assert.rejects(
      closePresentedDrawableCaptureServerStrict(server),
      /close receipt/,
    );
    assert.equal(closeChild.signalCode, "SIGTERM");
  } finally {
    clearInterval(keepAlive);
  }
});

test("captures the frozen Longbridge single-page crop as independent driver evidence", async (t) => {
  const { args } = await compatCropHarness(t);
  const receipt = await captureLongbridgeCompatPresentedCrop(args);

  assert.equal(receipt.event, "compat-presented-crop-evidence");
  assert.equal(receipt.command_id, "small:open-settle");
  assert.equal(
    receipt.acceptance_source,
    "XGetImage-presented-client-drawable",
  );
  assert.equal(receipt.candidate_resampled, false);
  assert.equal(receipt.painted_generation_stable, true);
  assert.equal(receipt.exact_pixel_match, true);
  assert.equal(
    receipt.presented_drawable_artifact_sha256,
    receipt.retained_ppm_sha256,
  );
  assert.equal(receipt.driver_capture.ppm_path, undefined);
  assert.deepEqual(receipt.extracted_bounds_pixels, {
    x: 136,
    y: 40,
    width: 540,
    height: 720,
  });
  assert.equal(
    validateCompatPresentedCrop([], {
      fixtureId: "bp-single-page-v1",
      commandId: "small:open-settle",
      driverReceipt: receipt,
    }).passed,
    true,
  );
});

test("fails the compatibility crop closed on missing geometry, generation or scale drift, reused artifacts, and pixel mismatch", async (t) => {
  {
    const { args, state } = await compatCropHarness(t);
    const missingBounds = { ...state };
    delete missingBounds.painted_outer_page_bounds_window_logical;
    await assert.rejects(
      captureLongbridgeCompatPresentedCrop({
        ...args,
        openEvidence: missingBounds,
      }),
      /page bounds x is missing/,
    );
  }
  {
    const { args, state } = await compatCropHarness(t);
    await assert.rejects(
      captureLongbridgeCompatPresentedCrop({
        ...args,
        waitForPostCaptureEvidence: async () => ({
          ...state,
          event: "viewer-native-presented-state",
          painted_render_generation: 5,
          runner_observed_monotonic_ms: 140,
        }),
      }),
      /painted_render_generation drifted/,
    );
  }
  {
    const { args } = await compatCropHarness(t);
    const capture = args.captureServer.capture;
    await assert.rejects(
      captureLongbridgeCompatPresentedCrop({
        ...args,
        captureServer: {
          async capture(...captureArgs) {
            return {
              ...(await capture(...captureArgs)),
              artifact_sha256: "f".repeat(64),
            };
          },
        },
      }),
      /retained XGetImage artifact hash does not match/,
    );
  }
  {
    const { args, state } = await compatCropHarness(t);
    await assert.rejects(
      captureLongbridgeCompatPresentedCrop({
        ...args,
        openEvidence: { ...state, display_scale_factor: 2 },
      }),
      /display scale does not map/,
    );
  }
  {
    const { args, directory } = await compatCropHarness(t);
    await writeFile(
      resolve(directory, "compat-single-registration-window.ppm"),
      "stale",
    );
    await assert.rejects(
      captureLongbridgeCompatPresentedCrop(args),
      /artifact already exists/,
    );
  }
  {
    const { args } = await compatCropHarness(t, {
      screenshotColor: { r: 0, g: 0, b: 0 },
      referenceColor: { r: 255, g: 255, b: 255 },
    });
    await assert.rejects(
      captureLongbridgeCompatPresentedCrop(args),
      /failed registered fidelity/,
    );
  }
});

test("correlates both XGetImage captures to one exact stable painted hold", () => {
  const state = {
    runner_observed_monotonic_ms: 105,
    state_sequence: 7,
    painted_state_sequence: 7,
    scroll_offset_css_px: { x: 0, y: 0 },
    visible_pages: [
      {
        page_number: 1,
        current_raster_ready_area_fraction: 1,
        painted_generation_current: true,
        painted_render_generation: 4,
        painted_outer_page_bounds_window_logical: {
          x: 400,
          y: 180,
          width: 610,
          height: 792,
        },
      },
    ],
  };
  const captureReceipt = (id, start, end) => ({
    event: "presented-drawable-captured",
    capture_id: id,
    capture_started_monotonic_ms: start,
    capture_ended_monotonic_ms: end,
    window_id: "4194305",
    width: 1200,
    height: 800,
    depth: 24,
    artifact_sha256: "a".repeat(64),
    ppm_path: `/tmp/${id}.ppm`,
    source: "XGetImage-presented-client-drawable",
  });
  const capture = {
    crop_id: "page-1",
    page_number: 1,
    before_state_receipt: {
      state,
      runner_snapshot_monotonic_ms: 110,
    },
    after_state_receipt: {
      state: structuredClone(state),
      runner_snapshot_monotonic_ms: 170,
    },
    before_capture: captureReceipt("page-1:before", 120, 130),
    after_capture: captureReceipt("page-1:after", 150, 160),
  };
  const trajectory = [
    {
      action: "hold-page-1",
      observed_monotonic_ms: 100,
    },
    {
      action: "hold-page-1",
      observed_monotonic_ms: 200,
    },
  ];
  const target = {
    window_id: "4194305",
    geometry: { width: 1200, height: 800 },
  };
  assert.equal(
    validatePresentedCapturePairCorrelation(capture, trajectory, target)
      .same_painted_render_generation,
    true,
  );
  assert.throws(
    () =>
      validatePresentedCapturePairCorrelation(
        {
          ...capture,
          after_capture: captureReceipt("page-1:after", 150, 220),
        },
        trajectory,
        target,
      ),
    /outside its exact held stream/,
  );
});

test("binds exact independent native ticks to the latest real GPUI raster state", () => {
  const ticks = parseNativeClockSamples(
    "0\t0.000000000\t100.000000\tobserve\n1\t500.000000000\t600.000000\tobserve\n2\t1000.000000000\t1100.000000\tobserve\n",
    3,
  );
  const visible = (ready) => [
    {
      visible_intersection_area_css_px2: 100,
      current_raster_ready_area_fraction: ready,
      current_raster_device_pixels_per_css_pixel: ready ? 4 : 0,
    },
  ];
  const states = [
    {
      event: "dynamic-fidelity-state",
      command_id: "viewer:dynamic-fidelity-scroll",
      runner_observed_monotonic_ms: 99,
      state_sequence: 1,
      active_page: 1,
      visible_page_count: 1,
      visible_pages: visible(0),
      scroll_offset_css_px: { x: 0, y: 0 },
      render_generation: 1,
    },
    {
      event: "dynamic-fidelity-state",
      command_id: "viewer:dynamic-fidelity-scroll",
      runner_observed_monotonic_ms: 590,
      state_sequence: 2,
      active_page: 2,
      visible_page_count: 1,
      visible_pages: visible(1),
      scroll_offset_css_px: { x: 0, y: 200 },
      render_generation: 2,
    },
    {
      event: "dynamic-fidelity-state",
      command_id: "viewer:dynamic-fidelity-scroll",
      runner_observed_monotonic_ms: 1090,
      state_sequence: 3,
      active_page: 3,
      visible_page_count: 1,
      visible_pages: visible(1),
      scroll_offset_css_px: { x: 0, y: 400 },
      render_generation: 3,
    },
  ];
  const samples = bindDynamicObserverSamples(ticks, states, {
    id: "viewer:dynamic-fidelity-scroll",
    duration_ms: 1000,
    observer: { rate_hz: 2, expected_sample_count: 3 },
    required_sample_fields: [
      "visible_page_ready_fraction",
      "visible_raster_ready_area_fraction",
      "visible_raster_pixel_density",
    ],
  });
  assert.equal(samples.length, 3);
  assert.equal(samples[0].visible_page_ready_fraction, 0);
  assert.equal(samples[1].visible_page_ready_fraction, 1);
  assert.equal(samples[1].visible_raster_pixel_density, 4);
  assert.equal(samples[2].application_state_sequence, 3);
  assert.equal(samples[2].application_state_observed_monotonic_ms, 1090);
  assert.equal(samples[2].application_state_age_ms, 10);
  assert.equal(samples[2].observer_tick_actual_offset_ms, 1000);
  assert.equal(samples[2].observer_tick_schedule_error_ms, 0);
});

test("rejects the previously accepted 500 ms state at the 1100 ms tick", () => {
  const ticks = parseNativeClockSamples(
    "0\t0.000000000\t100.000000\tobserve\n1\t500.000000000\t600.000000\tobserve\n2\t1000.000000000\t1100.000000\tobserve\n",
    3,
  );
  const states = [
    {
      event: "dynamic-fidelity-state",
      command_id: "viewer:dynamic-fidelity-scroll",
      runner_observed_monotonic_ms: 99,
      state_sequence: 1,
      active_page: 1,
      visible_page_count: 1,
      visible_pages: [
        {
          visible_intersection_area_css_px2: 100,
          current_raster_ready_area_fraction: 1,
          current_raster_device_pixels_per_css_pixel: 4,
        },
      ],
      scroll_offset_css_px: { x: 0, y: 0 },
      render_generation: 1,
    },
    {
      event: "dynamic-fidelity-state",
      command_id: "viewer:dynamic-fidelity-scroll",
      runner_observed_monotonic_ms: 500,
      state_sequence: 2,
      active_page: 2,
      visible_page_count: 1,
      visible_pages: [
        {
          visible_intersection_area_css_px2: 100,
          current_raster_ready_area_fraction: 1,
          current_raster_device_pixels_per_css_pixel: 4,
        },
      ],
      scroll_offset_css_px: { x: 0, y: 200 },
      render_generation: 2,
    },
  ];
  assert.throws(
    () =>
      bindDynamicObserverSamples(ticks, states, {
        id: "viewer:dynamic-fidelity-scroll",
        duration_ms: 1000,
        observer: { rate_hz: 2, expected_sample_count: 3 },
        required_sample_fields: [
          "visible_page_ready_fraction",
          "visible_raster_ready_area_fraction",
          "visible_raster_pixel_density",
        ],
      }),
    /application state age exceeds 33\.333333333 ms/,
  );
});

test("builds fixed-distance dynamic holds from actual 100-percent painted geometry", () => {
  const command = {
    id: "viewer:dynamic-fidelity-scroll",
    operation: "viewer.dynamic-fidelity-scroll-path",
    input_rate_hz: 120,
    duration_ms: 32_000,
    path: {
      forward_duration_ms: 20_000,
      pause_duration_ms: 2_000,
      reverse_duration_ms: 10_000,
      forward_viewport_heights: 50,
      checkpoint_hold_ms: 250,
    },
    registered_crops: [
      {
        crop_id: "start",
        page_number: 1,
        pdf_rect: { x: 36, y: 612, width: 538, height: 126 },
      },
      {
        crop_id: "middle",
        page_number: 15,
        pdf_rect: { x: 55, y: 252, width: 500, height: 288 },
      },
      {
        crop_id: "apex-checkpoint",
        page_number: 29,
        pdf_rect: { x: 55, y: 54, width: 500, height: 126 },
      },
    ],
  };
  const page = (page_number, y) => ({
    page_number,
    page_size_points: { width: 612, height: 792 },
    painted_outer_page_bounds_at_initial_scroll_window_logical: {
      x: 294,
      y,
      width: 612,
      height: 792,
    },
  });
  const plan = buildHeldDynamicWheelPlan({
    command,
    viewportHeightCssPx: 639,
    wheelDeltaCssPx: 120,
    initialScrollOffsetCssPx: 38,
    viewportBoundsWindowLogical: { x: 0, y: 100, width: 1200, height: 600 },
    checkpointPageGeometries: [
      page(1, 124),
      page(15, 12_000),
      page(29, 23_960),
    ],
    zoomPercent: 100,
    displayScaleFactor: 1,
  });
  assert.equal(plan.expected_trajectory_sample_count, 3841);
  assert.equal(plan.checkpoint_hold_sample_count, 31);
  assert.equal(plan.checkpoint_hold_interval_count, 30);
  assert.equal(plan.checkpoint_hold_duration_ms, 250);
  assert.equal(plan.checkpoints[0].forward_event_count, 0);
  assert.equal(plan.checkpoints[1].forward_event_count, 99);
  assert.equal(plan.checkpoints[2].forward_event_count, 200);
  assert.ok(
    plan.checkpoints.every(
      ({ crop_fully_visible_at_actual_offset: visible }) => visible,
    ),
  );
  assert.ok(
    plan.checkpoints
      .slice(1)
      .every(
        ({ offset_error_css_px: error }) =>
          Math.abs(error) <= plan.maximum_distance_error_css_px,
      ),
  );
  assert.ok(
    plan.checkpoints.every(
      ({ candidate_comparability: candidate }) =>
        candidate.zoom_percent === 100 &&
        candidate.display_scale_factor === 1 &&
        Math.abs(candidate.pixels_per_point.x - 1) <= 0.01 &&
        Math.abs(candidate.pixels_per_point.y - 1) <= 0.01,
    ),
  );
});

test("selects the first event-aligned fully-visible hold when nearest rounding is still clipped", () => {
  const command = {
    id: "viewer:dynamic-fidelity-scroll",
    operation: "viewer.dynamic-fidelity-scroll-path",
    input_rate_hz: 120,
    duration_ms: 32_000,
    path: {
      forward_viewport_heights: 50,
      checkpoint_hold_ms: 250,
    },
    registered_crops: [
      {
        crop_id: "start",
        page_number: 1,
        pdf_rect: { x: 36, y: 612, width: 538, height: 126 },
      },
      {
        crop_id: "middle",
        page_number: 15,
        pdf_rect: { x: 55, y: 252, width: 500, height: 288 },
      },
      {
        crop_id: "apex",
        page_number: 29,
        pdf_rect: { x: 55, y: 54, width: 500, height: 126 },
      },
    ],
  };
  const geometry = (page_number, y) => ({
    page_number,
    page_size_points: { width: 610, height: 792 },
    painted_outer_page_bounds_at_initial_scroll_window_logical: {
      x: 425,
      y,
      width: 610,
      height: 792,
    },
  });
  const plan = buildHeldDynamicWheelPlan({
    command,
    viewportHeightCssPx: 640,
    wheelDeltaCssPx: 120,
    initialScrollOffsetCssPx: 0,
    viewportBoundsWindowLogical: { x: 348, y: 160, width: 764, height: 640 },
    checkpointPageGeometries: [
      geometry(1, 184),
      geometry(15, 11_608),
      geometry(29, 23_032),
    ],
    zoomPercent: 100,
    displayScaleFactor: 1,
  });
  const apex = plan.checkpoints[2];
  assert.equal(apex.earliest_fully_visible_forward_delta_css_px, 22_970);
  assert.equal(apex.forward_event_count, 192);
  assert.equal(apex.actual_forward_delta_css_px, 23_040);
  assert.equal(apex.target_forward_delta_css_px, 23_040);
  assert.equal(apex.offset_error_css_px, 0);
  assert.equal(apex.crop_fully_visible_at_actual_offset, true);
});

test("accepts one excluded candidate-specific wheel calibration and rejects invalid receipts", () => {
  const ready = {
    wheel_calibration_required: true,
    calibrated_wheel_delta_css_px: null,
    initial_scroll_offset_css_px: { x: 0, y: 24 },
  };
  const receipt = {
    candidate_runtime: "gpui",
    input_api: "XTEST-single-wheel-notch",
    calibration_event_count: 1,
    observed_wheel_delta_css_px: 78,
    initial_scroll_offset_css_px: 24,
    post_calibration_scroll_offset_css_px: 24,
    scroll_applied: false,
    timed_trajectory_excluded: true,
  };
  assert.deepEqual(
    validateDynamicWheelCalibrationReceipt(ready, receipt),
    receipt,
  );
  const distancePlan = buildDistanceBoundedDynamicWheelPlan({
    command: {
      operation: "viewer.dynamic-fidelity-scroll-path",
      path: { forward_viewport_heights: 50 },
    },
    viewportHeightCssPx: 640,
    wheelDeltaCssPx: receipt.observed_wheel_delta_css_px,
  });
  assert.equal(distancePlan.forward_event_count, 410);
  assert.equal(distancePlan.reverse_event_count, 410);
  assert.equal(distancePlan.requested_forward_distance_css_px, 32_000);
  assert.equal(distancePlan.scheduled_forward_distance_css_px, 31_980);
  assert.ok(
    Math.abs(distancePlan.distance_error_css_px) <=
      distancePlan.maximum_distance_error_css_px,
  );
  for (const invalid of [
    { ...receipt, observed_wheel_delta_css_px: 0 },
    { ...receipt, observed_wheel_delta_css_px: Number.NaN },
    { ...receipt, calibration_event_count: 2 },
    { ...receipt, post_calibration_scroll_offset_css_px: 102 },
    { ...receipt, scroll_applied: true },
    { ...receipt, timed_trajectory_excluded: false },
  ]) {
    assert.throws(
      () => validateDynamicWheelCalibrationReceipt(ready, invalid),
      /calibration receipt/,
    );
  }
});

test("builds the exact 361-sample 120 Hz native v5 snap replay", async () => {
  const workload = await loadComparisonWorkloadV5();
  const command = buildScenarioContractV5(
    workload,
    "dense-mixed-editing",
  ).commands.find(({ id }) => id === "annotation:native-snap-transform-120hz");
  const replay = buildNativeV5SnapReplay(
    command,
    {
      surface: {
        bounds: { x: 100, y: 0, width: 612, height: 792 },
        page_height_points: 792,
        pixels_per_point: 1,
        window_logical_size: { width: 1_200, height: 800 },
      },
    },
    {
      window_id: 4_194_307,
      geometry: {
        x: 0,
        y: 0,
        width: 1_200,
        height: 800,
        content: { x: 0, y: 0, width: 1_200, height: 800 },
      },
    },
  );
  assert.equal(replay.execution, "direct-pointer");
  assert.equal(replay.pixel_samples.length, 361);
  assert.equal(replay.pixel_samples[0].scheduled_ms, 0);
  assert.equal(replay.pixel_samples.at(-1).scheduled_ms, 3_000);
  assert.equal(replay.metadata.rate_hz, 120);
  assert.equal(replay.metadata.sensitivity_css_px, 8);
  assert.equal(replay.metadata.threshold_norm, "per-axis-l-infinity");
  assert.equal(replay.metadata.inclusive, true);
});

test("rejects native clock catch-up or duplicate timestamps", () => {
  assert.throws(
    () =>
      parseNativeClockSamples("0\t0\t100\tobserve\n1\t10\t100\tobserve\n", 2),
    /sample 1 is invalid/,
  );
});

test("requires all three exact registered crop identities and SSIM thresholds", () => {
  const registered_crops = ["start", "middle", "apex"].map((crop_id) => ({
    crop_id,
    reference_raster: {
      reference_crop_sha256: crop_id.repeat(64).slice(0, 64),
      minimum_ssim: 0.985,
    },
  }));
  const receipts = registered_crops.map((crop) => ({
    crop_id: crop.crop_id,
    passed: true,
    metric: {
      algorithm: "bp-cross-engine-binary-scan-fidelity-v2",
      passed: true,
    },
    candidate_resampled: false,
    reference_resampling: "downsample-only-lanczos3",
    candidate_crop_unchanged: true,
    reference_crop_sha256: crop.reference_raster.reference_crop_sha256,
  }));
  assert.equal(
    registeredDynamicCropsPassed(receipts, { registered_crops }),
    true,
  );
  assert.equal(
    registeredDynamicCropsPassed(
      receipts.map((receipt, index) =>
        index === 1 ? { ...receipt, candidate_crop_unchanged: false } : receipt,
      ),
      { registered_crops },
    ),
    false,
  );
  assert.equal(
    registeredDynamicCropsPassed(receipts.slice(0, 2), { registered_crops }),
    false,
  );
});

test("requires 31 consecutive zero-input samples at all three held checkpoints", () => {
  const trajectory = Array.from({ length: 3841 }, (_, sample_index) => ({
    sample_index,
    scheduled_offset_ms: (sample_index * 32_000) / 3840,
    action: "forward",
  }));
  for (const [action, start] of [
    ["hold-page-1", 0],
    ["hold-page-15", 900],
    ["hold-page-29", 1800],
  ]) {
    for (let index = start; index < start + 31; index += 1) {
      trajectory[index].action = action;
    }
  }
  const plan = {
    expected_trajectory_sample_count: 3841,
    checkpoint_hold_sample_count: 31,
  };
  assert.equal(heldDynamicTrajectoryPassed(trajectory, plan), true);
  trajectory[930].action = "forward";
  assert.equal(heldDynamicTrajectoryPassed(trajectory, plan), false);
});

test("accepts GPUI crop capture only across one unchanged current paint", () => {
  const state = {
    state_sequence: 70,
    painted_state_sequence: 70,
    scroll_offset_css_px: { x: 0, y: 12000 },
    visible_pages: [
      {
        page_number: 15,
        painted_outer_page_bounds_window_logical: {
          x: 294,
          y: 100,
          width: 612,
          height: 792,
        },
        current_raster_ready_area_fraction: 1,
        painted_generation_current: true,
        painted_render_generation: 7,
      },
    ],
  };
  const sequenceAdvanced = structuredClone(state);
  sequenceAdvanced.state_sequence = 71;
  sequenceAdvanced.painted_state_sequence = 71;
  assert.equal(gpuiPaintedCropStateStable(state, sequenceAdvanced, 15), true);
  assert.deepEqual(dynamicCaptureSemanticTuple(state, 15), {
    scroll_offset_css_px: { x: 0, y: 12000 },
    painted_outer_page_bounds_window_logical: {
      x: 294,
      y: 100,
      width: 612,
      height: 792,
    },
    current_raster_ready_area_fraction: 1,
    painted_generation_current: true,
    painted_render_generation: 7,
  });
  const changed = structuredClone(state);
  changed.visible_pages[0].painted_render_generation = 8;
  assert.equal(gpuiPaintedCropStateStable(state, changed, 15), false);
});

test("starts and completes capture only inside the matching live helper hold", () => {
  const feed = {
    samples: [
      {
        sample_index: 852,
        scheduled_offset_ms: 7100,
        observed_monotonic_ms: 100,
        action: "forward",
      },
      {
        sample_index: 853,
        scheduled_offset_ms: 7108.333333,
        observed_monotonic_ms: 108.333333,
        action: "hold-page-15",
      },
    ],
  };
  assert.equal(dynamicHelperHoldState(feed, 15).phase, "inside");
  assert.equal(
    requireDynamicHelperHold(feed, 15, "middle-crop", "before").latest_sample
      .sample_index,
    853,
  );
  feed.samples.push({
    sample_index: 884,
    scheduled_offset_ms: 7366.666667,
    observed_monotonic_ms: 366.666667,
    action: "forward",
  });
  assert.equal(dynamicHelperHoldState(feed, 15).phase, "after");
  assert.throws(
    () => requireDynamicHelperHold(feed, 15, "middle-crop", "after"),
    (error) => {
      assert.match(error.message, /left declared hold during after/);
      assert.equal(
        error.dynamic_capture_failure_evidence.helper_hold.latest_sample.action,
        "forward",
      );
      return true;
    },
  );
});

test("reports an exact missed semantic-ready checkpoint after its declared hold", () => {
  const feed = {
    samples: [
      {
        sample_index: 853,
        scheduled_offset_ms: 7108.333333,
        observed_monotonic_ms: 108.333333,
        action: "hold-page-15",
      },
      {
        sample_index: 883,
        scheduled_offset_ms: 7358.333333,
        observed_monotonic_ms: 358.333333,
        action: "hold-page-15",
      },
      {
        sample_index: 884,
        scheduled_offset_ms: 7366.666667,
        observed_monotonic_ms: 366.666667,
        action: "forward",
      },
    ],
  };
  assert.throws(
    () => requireDynamicHelperHold(feed, 15, "middle-crop", "semantic-ready"),
    /middle-crop crop-not-semantic-ready-during-declared-hold/,
  );
});

test("retains helper hold and semantic tuples for a capture failure", async (t) => {
  const directory = await mkdtemp(resolve(tmpdir(), "bp-dynamic-capture-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const feed = {
    samples: [
      {
        sample_index: 883,
        scheduled_offset_ms: 7358.333333,
        observed_monotonic_ms: 358.333333,
        action: "hold-page-15",
      },
      {
        sample_index: 884,
        scheduled_offset_ms: 7366.666667,
        observed_monotonic_ms: 366.666667,
        action: "forward",
      },
    ],
  };
  let failure;
  try {
    requireDynamicHelperHold(feed, 15, "middle-crop", "after", {
      before: { semantic_tuple: { scroll_offset_css_px: { x: 0, y: 11388 } } },
      middle: { semantic_tuple: { scroll_offset_css_px: { x: 0, y: 11388 } } },
      after: { semantic_tuple: { scroll_offset_css_px: { x: 0, y: 11466 } } },
    });
  } catch (error) {
    failure = error;
  }
  const retained = await retainDynamicCaptureFailureEvidence(
    directory,
    failure,
  );
  const receipt = JSON.parse(await readFile(retained.path, "utf8"));
  assert.equal(receipt.helper_hold.latest_hold_sample.sample_index, 883);
  assert.equal(receipt.helper_hold.latest_sample.action, "forward");
  assert.equal(
    receipt.state_receipts.after.semantic_tuple.scroll_offset_css_px.y,
    11466,
  );
});

test("builds the decision-3 prerequisite, no-fill edge move, and east resize replays", async () => {
  const workload = await loadComparisonWorkload();
  const commands = new Map(
    workload.journeys
      .flatMap(({ commands: journeyCommands }) => journeyCommands)
      .map((command) => [command.id, command]),
  );
  const target = {
    window_id: "4194307",
    geometry: { width: 1200, height: 800 },
  };
  const ready = { surface: editorSurface };
  const create = buildNativeRectangleTransformReplay({
    createCommand: commands.get("rectangle:create-sparse"),
    transformCommand: commands.get("rectangle:select-move-resize"),
    ready: { ...ready, stage: "prerequisite-create" },
    target,
  });
  assert.equal(create.execution, "direct-pointer");
  assert.equal(create.pixel_samples.length, 361);
  assert.deepEqual(create.pixel_samples[0], {
    x: 496,
    y: 648,
    scheduled_ms: 0,
  });
  assert.deepEqual(create.pixel_samples.at(-1), {
    x: 676,
    y: 552,
    scheduled_ms: 3_000,
  });

  const move = buildNativeRectangleTransformReplay({
    createCommand: commands.get("rectangle:create-sparse"),
    transformCommand: commands.get("rectangle:select-move-resize"),
    ready: { ...ready, stage: "move" },
    target,
  });
  assert.equal(move.metadata.coordinate_sample_count, 361);
  assert.deepEqual(move.pixel_samples[0], { x: 541, y: 552, scheduled_ms: 0 });
  assert.deepEqual(move.pixel_samples.at(-1), {
    x: 559,
    y: 564,
    scheduled_ms: 3_000,
  });

  const resize = buildNativeRectangleTransformReplay({
    createCommand: commands.get("rectangle:create-sparse"),
    transformCommand: commands.get("rectangle:select-move-resize"),
    ready: { ...ready, stage: "east-resize", handle_point: { x: 270, y: 180 } },
    target,
  });
  assert.equal(resize.metadata.coordinate_sample_count, 361);
  assert.deepEqual(resize.pixel_samples[0], {
    x: 694,
    y: 612,
    scheduled_ms: 0,
  });
  assert.deepEqual(resize.pixel_samples.at(-1), {
    x: 724,
    y: 612,
    scheduled_ms: 3_000,
  });
});

test("accepts native transform evidence only with exact history, geometry tolerance, and platform draw", () => {
  const evidence = validateNativeRectangleTransformEvidence([
    {
      event: "comparison-native-transform-evidence",
      command_id: "rectangle:select-move-resize",
      select_semantics: "no-fill-edge-or-stroked-body",
      hit_test_selected: true,
      move_history_delta: 1,
      resize_history_delta: 1,
      observed_final_rect: [90, 132, 210, 96],
      expected_final_rect: [90, 132, 210, 96],
      pixels_per_point: 1,
      maximum_geometry_error_device_px: 0,
      geometry_tolerance_device_px: 1,
      gpui_platform_draw_submitted: true,
      physical_scanout_observed: false,
    },
  ]);
  assert.equal(evidence.success, true);
  assert.throws(
    () =>
      validateNativeRectangleTransformEvidence([
        {
          ...evidence.receipt,
          gpui_platform_draw_submitted: false,
        },
      ]),
    /platform draw/,
  );
  assert.throws(
    () =>
      validateNativeRectangleTransformEvidence([
        {
          ...evidence.receipt,
          maximum_geometry_error_device_px: 1.01,
        },
      ]),
    /geometry tolerance/,
  );
  assert.throws(
    () =>
      validateNativeRectangleTransformEvidence([
        {
          ...evidence.receipt,
          observed_final_rect: [90.5, 132, 210, 96],
          pixels_per_point: 2,
          maximum_geometry_error_device_px: 0,
        },
      ]),
    /independent geometry error/,
  );
});

test("builds a real shell probe click and accepts only truthful GPUI draw/open receipts", () => {
  const target = {
    window_id: "4194307",
    geometry: { width: 1200, height: 800 },
  };
  const replay = buildNativeViewerShellReplay(
    {
      control: {
        window_logical_size: { width: 1200, height: 800 },
        point: { x: 600, y: 400 },
      },
    },
    target,
  );
  assert.deepEqual(replay.args, [
    "mousemove",
    "--window",
    "4194307",
    "600",
    "400",
    "click",
    "1",
  ]);

  const evidence = validateNativeViewerLaunchOpenEvidence([
    {
      event: "viewer-native-launch-evidence",
      command_id: "viewer:launch-cold",
      native_input_observed: true,
      gpui_platform_draw_submitted: true,
      physical_scanout_observed: false,
      input_latency_samples_before: 2,
      input_latency_samples_after: 3,
    },
    {
      event: "viewer-native-open-evidence",
      command_id: "viewer:open-each",
      document_opened: true,
      preview_current_generation: true,
      settled_current_generation_ms: 250,
    },
  ]);
  assert.equal(evidence.success, true);
  assert.equal(evidence.physical_scanout_observed, false);

  assert.throws(
    () =>
      validateNativeViewerLaunchOpenEvidence([
        {
          event: "viewer-native-launch-evidence",
          command_id: "viewer:launch-cold",
          native_input_observed: true,
          gpui_platform_draw_submitted: false,
          physical_scanout_observed: false,
          input_latency_samples_before: 2,
          input_latency_samples_after: 2,
        },
      ]),
    /platform draw submission/,
  );
});

test("routes open-pdf through server-side XDamage observation without a synthetic contract scenario field", async () => {
  const previousDisplay = process.env.DISPLAY;
  process.env.DISPLAY = ":test";
  const causalCalls = [];
  const ordinaryCalls = [];
  try {
    const result = await executeNativeX11Scenario({
      pid: 321,
      scenario: "open-pdf",
      // The real open-pdf development contract is null. Scenario identity is
      // an explicit runner input rather than an invented contract property.
      contract: null,
      workload: null,
      timeoutMs: 100,
      events: [
        {
          event: "native-viewer-shell-ready",
          command_id: "viewer:launch-cold",
          control: {
            window_logical_size: { width: 1200, height: 800 },
            point: { x: 600, y: 400 },
          },
        },
        {
          event: "viewer-native-launch-evidence",
          command_id: "viewer:launch-cold",
          native_input_observed: true,
          gpui_platform_draw_submitted: true,
          physical_scanout_observed: false,
          input_latency_samples_before: 0,
          input_latency_samples_after: 1,
        },
        {
          event: "viewer-native-open-evidence",
          command_id: "viewer:open-each",
          document_opened: true,
          preview_current_generation: true,
          settled_current_generation_ms: 250,
        },
      ],
      locateWindow: async () => ({
        window_id: "4194305",
        pid: 321,
        title: "Butter Paper GPUI comparison",
        geometry: {
          x: 40,
          y: 84,
          width: 1200,
          height: 800,
          visible: true,
        },
      }),
      execute: async (...args) => {
        ordinaryCalls.push(args);
        return 0;
      },
      damageObservationEnabled: true,
      executeCausalClick: async (...args) => {
        causalCalls.push(args);
        return 1;
      },
    });
    assert.equal(result.success, true);
    assert.equal(causalCalls.length, 1);
    assert.equal(causalCalls[0][2], "open-pdf:open-control");
    assert.equal(ordinaryCalls.length, 0);
  } finally {
    if (previousDisplay === undefined) delete process.env.DISPLAY;
    else process.env.DISPLAY = previousDisplay;
  }
});

test("returns the compatibility crop as driver evidence without forging application stdout", async (t) => {
  const previousDisplay = process.env.DISPLAY;
  process.env.DISPLAY = ":test";
  const { args, state } = await compatCropHarness(t);
  const events = [
    {
      event: "native-viewer-shell-ready",
      command_id: "viewer:launch-cold",
      control: {
        window_logical_size: { width: 1200, height: 800 },
        point: { x: 600, y: 400 },
      },
    },
    {
      event: "viewer-native-launch-evidence",
      command_id: "viewer:launch-cold",
      native_input_observed: true,
      gpui_platform_draw_submitted: true,
      physical_scanout_observed: false,
      input_latency_samples_before: 0,
      input_latency_samples_after: 1,
    },
    {
      ...state,
      document_opened: true,
      preview_current_generation: true,
      settled_current_generation_ms: 250,
    },
    {
      ...state,
      event: "viewer-native-presented-state",
      runner_observed_monotonic_ms: 140,
    },
  ];
  const signals = [];
  try {
    const result = await executeNativeX11Scenario({
      pid: 321,
      scenario: "open-pdf",
      contract: null,
      workload: null,
      events,
      artifactDirectory: args.artifactDirectory,
      timeoutMs: 100,
      locateWindow: async () => ({
        window_id: "4194305",
        pid: 321,
        title: "Butter Paper GPUI comparison",
        geometry: {
          x: 40,
          y: 84,
          width: 1200,
          height: 800,
          visible: true,
        },
      }),
      execute: async () => 0,
      damageObservationEnabled: false,
      compatPresentedCrop: {
        fixturePath: args.fixturePath,
        registrationPath: args.registrationPath,
        renderReference: args.renderReference,
      },
      startCaptureServer: async () => args.captureServer,
      signalProcess: (targetPid, signal) => {
        signals.push([targetPid, signal]);
        return true;
      },
    });
    assert.equal(
      result.driver_evidence.compat_presented_crop.event,
      "compat-presented-crop-evidence",
    );
    assert.equal(
      events.some(({ event }) => event === "compat-presented-crop-evidence"),
      false,
    );
    assert.deepEqual(signals, [[321, "SIGUSR1"]]);
    assert.deepEqual(longbridgeCompatCaptureSignalProtocol, {
      signal: "SIGUSR1",
      trigger_owner: "native-x11-driver",
      rust_handler_effect:
        "schedule-next-frame-and-emit-viewer-native-presented-state",
      forbidden_payloads: ["artifact-path", "capture-id", "expected-digest"],
    });
  } finally {
    if (previousDisplay === undefined) delete process.env.DISPLAY;
    else process.env.DISPLAY = previousDisplay;
  }
});

const editorSurface = {
  window_logical_size: { width: 1200, height: 800 },
  bounds: { x: 424, y: 0, width: 612, height: 792 },
  page_height_points: 792,
  pixels_per_point: 1,
};

test("builds native editor create clicks and length drag from measured GPUI geometry", async () => {
  const target = {
    window_id: "4194307",
    geometry: { width: 1200, height: 800 },
  };
  const workload = await loadComparisonWorkload();
  const commands = new Map(
    workload.journeys
      .flatMap(({ commands: journeyCommands }) => journeyCommands)
      .map((command) => [command.id, command]),
  );
  const text = buildNativeCommandReplay(
    commands.get("text:create"),
    { surface: editorSurface },
    target,
  );
  assert.equal(text.execution, "xdotool");
  assert.deepEqual(text.args.slice(0, 7), [
    "mousemove",
    "--window",
    "4194307",
    "634",
    "366",
    "click",
    "1",
  ]);
  assert.deepEqual(text.args.slice(7), [
    "type",
    "--clearmodifiers",
    "--delay",
    "1",
    "Beam B-12 / revision 3",
  ]);
  assert.equal(text.metadata.native_text_entry_submitted, true);
  assert.equal(text.metadata.native_text_entry_observed, false);
  assert.equal(text.metadata.document_content_prepopulated, false);

  const length = buildNativeCommandReplay(
    commands.get("length:create"),
    { surface: editorSurface },
    target,
  );
  assert.equal(length.execution, "direct-pointer");
  assert.deepEqual(length.pixel_samples, [
    { x: 514, y: 282, scheduled_ms: 0 },
    { x: 730, y: 282, scheduled_ms: 300 },
  ]);
  assert.equal(length.metadata.scheduled_duration_ms, 300);

  const image = buildNativeCommandReplay(
    commands.get("image:create"),
    { surface: editorSurface, preloaded_asset_id: "bp-image-checker-v1" },
    target,
  );
  assert.equal(image.execution, "xdotool");
  assert.deepEqual(image.pixel, { x: 856, y: 348 });
  assert.equal(image.metadata.native_asset_selection, false);
});

test("builds scale click only from an observed GPUI control and rejects fake upload claims", () => {
  const target = {
    window_id: "4194307",
    geometry: { width: 1200, height: 800 },
  };
  const command = {
    id: "length:set-scale",
    operation: "measurement.set-scale",
    scale: { paper_points: 72, real_world_value: 1, unit: "m", precision: 2 },
  };
  const replay = buildNativeCommandReplay(
    command,
    {
      control: {
        control_id: "comparison-length-scale",
        window_logical_size: { width: 1200, height: 800 },
        bounds: { x: 500, y: 120, width: 160, height: 32 },
      },
    },
    target,
  );
  assert.equal(replay.execution, "xdotool");
  assert.deepEqual(replay.pixel, { x: 580, y: 136 });
  assert.equal(replay.metadata.control_id, "comparison-length-scale");
  assert.equal(replay.metadata.gpu_present_observed, false);
  assert.equal(replay.metadata.gpu_upload_bytes, null);
  assert.throws(
    () => buildNativeCommandReplay(command, {}, target),
    /observed comparison-length-scale control geometry/,
  );
});

test("constructs one timestamped XTest pointer stream with inclusive endpoints", () => {
  const replay = buildPointerReplay({
    windowId: "4194307",
    rateHz: 120,
    durationMs: 3_000,
    pdfSamples: [
      { x: 72, y: 144 },
      { x: 162, y: 192 },
      { x: 252, y: 240 },
    ],
    surface: {
      window_logical_size: { width: 1200, height: 800 },
      bounds: { x: 424, y: 0, width: 612, height: 792 },
      page_height_points: 792,
      pixels_per_point: 1,
    },
    windowGeometry: { width: 1200, height: 800 },
  });

  assert.deepEqual(replay.pixel_samples, [
    { x: 496, y: 648, scheduled_ms: 0 },
    { x: 586, y: 600, scheduled_ms: 1_500 },
    { x: 676, y: 552, scheduled_ms: 3_000 },
  ]);
  assert.deepEqual(replay.args.slice(0, 7), [
    "mousemove",
    "--window",
    "4194307",
    "496",
    "648",
    "mousedown",
    "1",
  ]);
  assert.deepEqual(replay.args.slice(-9, -2), [
    "sleep",
    "1.500000",
    "mousemove",
    "--window",
    "4194307",
    "676",
    "552",
  ]);
  assert.deepEqual(replay.args.slice(-2), ["mouseup", "1"]);
  assert.equal(replay.metadata.coordinate_sample_count, 3);
  assert.equal(replay.metadata.scheduled_duration_ms, 3_000);
});

test("maps native toolbar clicks through the same verified window scale", () => {
  assert.deepEqual(
    windowLogicalPointToPixel(
      { x: 600, y: 400 },
      { width: 1200, height: 800 },
      { width: 2400, height: 1600 },
    ),
    { x: 1200, y: 800 },
  );
  assert.deepEqual(
    windowLogicalPointToPixel(
      { x: 0, y: 0 },
      { width: 1200, height: 800 },
      {
        width: 1208,
        height: 804,
        content: { x: 4, y: 2, width: 1200, height: 800 },
      },
    ),
    { x: 4, y: 2 },
  );
  assert.deepEqual(
    buildClickReplay({
      windowId: "4194307",
      point: { x: 30, y: 40 },
      logicalSize: { width: 1200, height: 800 },
      windowGeometry: { width: 1200, height: 800 },
    }),
    {
      args: ["mousemove", "--window", "4194307", "30", "40", "click", "1"],
      pixel: { x: 30, y: 40 },
      metadata: {
        scheduled_duration_ms: 0,
        scheduled_active_duration_ms: 0,
        total_event_count: 1,
        button: 1,
      },
    },
  );
});

test("converts a mapped click into a direct XTest press and release", () => {
  const click = buildClickReplay({
    windowId: "4194307",
    point: { x: 30, y: 40 },
    logicalSize: { width: 1200, height: 800 },
    windowGeometry: { width: 1200, height: 800 },
  });
  const replay = buildDirectClickReplay(click);
  assert.deepEqual(replay.pixel_samples, [
    { x: 30, y: 40 },
    { x: 30, y: 40 },
  ]);
  assert.equal(replay.metadata.scheduled_duration_ms, 1);
  assert.equal(replay.metadata.scheduled_active_duration_ms, 1);
  assert.equal(replay.metadata.timed_interval_count, 1);
  assert.equal(replay.metadata.input_api, "XTEST-pointer");
});

test("builds a direct middle-button pointer replay for native viewport panning", () => {
  const replay = buildPointerReplay({
    windowId: "4194307",
    button: 2,
    rateHz: 120,
    durationMs: 5_000,
    pdfSamples: [
      { x: 100, y: 700 },
      { x: 200, y: 600 },
    ],
    surface: {
      window_logical_size: { width: 1200, height: 800 },
      bounds: { x: 300, y: 100, width: 612, height: 792 },
      page_height_points: 792,
      pixels_per_point: 1,
    },
    windowGeometry: { width: 1200, height: 800 },
  });

  assert.deepEqual(replay.args.slice(5, 7), ["mousedown", "2"]);
  assert.deepEqual(replay.args.slice(-2), ["mouseup", "2"]);
  assert.equal(replay.metadata.button, 2);
});

test("maps PDF bottom-left points through logical surface and X11 window scale", () => {
  assert.deepEqual(
    pdfPointToWindowPixel(
      { x: 100, y: 700 },
      {
        window_logical_size: { width: 1200, height: 800 },
        bounds: { x: 300, y: 100, width: 612, height: 792 },
        page_height_points: 792,
        pixels_per_point: 1,
      },
      { width: 2400, height: 1600 },
    ),
    { x: 800, y: 384 },
  );
  assert.throws(
    () =>
      pdfPointToWindowPixel(
        { x: 100, y: 0 },
        {
          window_logical_size: { width: 1200, height: 800 },
          bounds: { x: 300, y: 100, width: 612, height: 792 },
          page_height_points: 792,
          pixels_per_point: 1,
        },
        { width: 1200, height: 800 },
      ),
    /outside the visible X11 window/,
  );
});

test("rejects ambiguous, foreign, mistitled, hidden, or resized X11 targets", () => {
  const target = {
    ids: ["4194307"],
    expectedPid: 8123,
    actualPid: 8123,
    expectedTitle: "Butter Paper GPUI comparison",
    actualTitle: "Butter Paper GPUI comparison",
    geometry: { width: 1200, height: 800, visible: true },
  };
  assert.deepEqual(validateWindowTarget(target), {
    window_id: "4194307",
    pid: 8123,
    title: "Butter Paper GPUI comparison",
    geometry: {
      width: 1200,
      height: 800,
      visible: true,
      requested_size: { width: 1200, height: 800 },
      size_verification: "exact-client-size",
    },
  });
  assert.throws(
    () => validateWindowTarget({ ...target, ids: ["1", "2"] }),
    /exactly one/,
  );
  assert.throws(
    () => validateWindowTarget({ ...target, actualPid: 8124 }),
    /PID/,
  );
  assert.throws(
    () => validateWindowTarget({ ...target, actualTitle: "Terminal" }),
    /title/,
  );
  assert.throws(
    () =>
      validateWindowTarget({
        ...target,
        geometry: { ...target.geometry, visible: false },
      }),
    /visible/,
  );
  assert.throws(
    () =>
      validateWindowTarget({
        ...target,
        geometry: { ...target.geometry, width: 1199 },
      }),
    /1200x800/,
  );
});

test("accepts only frame-extents-proven decorated GPUI client geometry", () => {
  const decorated = validateWindowTarget({
    ids: ["4194307"],
    expectedPid: 8123,
    actualPid: 8123,
    actualTitle: "Butter Paper GPUI comparison",
    geometry: {
      width: 1198,
      height: 777,
      visible: true,
      frame_extents: { left: 1, right: 1, top: 20, bottom: 3 },
    },
    allowDecoratedClient: true,
  });
  assert.equal(
    decorated.geometry.size_verification,
    "decorated-client-plus-frame-extents",
  );
  const verified = validateGpuiLogicalWindowTarget(decorated, {
    control: { window_logical_size: { width: 1198, height: 777 } },
  });
  assert.deepEqual(verified.geometry.app_logical_size, {
    width: 1198,
    height: 777,
  });
  assert.equal(
    verified.geometry.size_verification,
    "app-logical-size-plus-decorated-client-frame-extents",
  );

  assert.throws(
    () =>
      validateWindowTarget({
        ids: ["4194307"],
        expectedPid: 8123,
        actualPid: 8123,
        actualTitle: "Butter Paper GPUI comparison",
        geometry: {
          width: 1198,
          height: 777,
          visible: true,
          frame_extents: { left: 1, right: 1, top: 20, bottom: 2 },
        },
        allowDecoratedClient: true,
      }),
    /without matching frame extents/,
  );
  assert.throws(
    () =>
      validateGpuiLogicalWindowTarget(decorated, {
        control: { window_logical_size: { width: 1197, height: 777 } },
      }),
    /does not match verified X11 client 1198x777/,
  );
});

test("parses the machine-readable xdotool geometry without accepting missing fields", () => {
  assert.deepEqual(
    parseXdotoolGeometry(
      "WINDOW=4194307\nX=3\nY=4\nWIDTH=1200\nHEIGHT=800\nSCREEN=0\n",
    ),
    { window: "4194307", x: 3, y: 4, width: 1200, height: 800, screen: 0 },
  );
  assert.throws(
    () => parseXdotoolGeometry("WIDTH=1200\nHEIGHT=800\n"),
    /WINDOW/,
  );
});

test("parses the decorated X11 client origin used by direct XTEST replay", () => {
  const output = `
xwininfo: Window id: 0x400001 "Butter Paper GPUI comparison"

  Absolute upper-left X:  360
  Absolute upper-left Y:  145
  Relative upper-left X:  1
  Relative upper-left Y:  16
  Width: 1200
  Height: 800
`;
  assert.deepEqual(parseXwininfoClientGeometry(output), {
    x: 360,
    y: 145,
    width: 1200,
    height: 800,
  });
  assert.throws(
    () => parseXwininfoClientGeometry("Width: 1200\nHeight: 800\n"),
    /Absolute upper-left X/,
  );
});

test("parses exact EWMH frame extents and rejects absent decoration evidence", () => {
  assert.deepEqual(
    parseX11FrameExtents("_NET_FRAME_EXTENTS(CARDINAL) = 1, 1, 20, 3\n"),
    { left: 1, right: 1, top: 20, bottom: 3 },
  );
  assert.throws(
    () => parseX11FrameExtents('_NET_WM_NAME(UTF8_STRING) = "Butter Paper"\n'),
    /missing exact _NET_FRAME_EXTENTS/,
  );
});

test("constructs the manifest wheel schedule with a real pause", () => {
  const replay = buildWheelReplay({
    windowId: "4194307",
    rateHz: 2,
    forwardDurationMs: 2_000,
    pauseDurationMs: 1_000,
    reverseDurationMs: 1_000,
    forwardViewportHeights: 50,
    target: { x: 600, y: 400 },
  });
  assert.deepEqual(replay.metadata, {
    scheduled_duration_ms: 4_000,
    scheduled_active_duration_ms: 3_000,
    forward_interval_count: 4,
    reverse_interval_count: 2,
    timed_interval_count: 6,
    forward_notches_per_event: 1,
    reverse_notches_per_event: 2,
    forward_physical_click_count: 4,
    reverse_physical_click_count: 4,
    total_physical_click_count: 8,
    expected_application_wheel_event_count: 8,
    expected_peak_viewport_heights: 50,
    rate_hz: 2,
    forward_button: 5,
    reverse_button: 4,
    scheduled_first_ms: 0,
    scheduled_reverse_first_ms: 3_000,
    scheduled_last_ms: 3_500,
  });
  assert.deepEqual(replay.args.slice(0, 5), [
    "mousemove",
    "--window",
    "4194307",
    "600",
    "400",
  ]);
  assert.equal(
    replay.args.filter((argument) => argument === "click").length,
    8,
  );
  assert.ok(replay.args.includes("1.000000"));
});

test("native lane metadata cannot claim a GUI result until replay evidence exists", () => {
  assert.deepEqual(nativeX11LaneMetadata(null), {
    input_lane: "native-x11-xtest",
    injection_api: "XTEST-direct-helper",
    execution_status: "not-run",
    real_gui_run: false,
    decision_timing_eligible: false,
  });
  assert.deepEqual(
    nativeX11LaneMetadata({
      success: true,
      target_verified: true,
      application_success: true,
    }),
    {
      input_lane: "native-x11-xtest",
      injection_api: "XTEST-direct-helper",
      execution_status: "passed",
      real_gui_run: true,
      decision_timing_eligible: true,
    },
  );
  assert.equal(
    nativeX11LaneMetadata({
      success: true,
      target_verified: true,
      application_success: false,
    }).decision_timing_eligible,
    false,
  );
  assert.equal(
    nativeX11LaneMetadata({
      success: false,
      target_verified: true,
      application_success: true,
    }).decision_timing_eligible,
    false,
  );
});

test("rejects replay timing outside the predeclared five-percent jitter budget", () => {
  const replay = {
    metadata: {
      scheduled_duration_ms: 3_000,
      scheduled_active_duration_ms: 3_000,
      timed_interval_count: 360,
    },
  };
  assert.deepEqual(assessReplayTiming(replay, 3_120), {
    within_tolerance: true,
    tolerance_ms: 150,
    duration_error_ms: 120,
    actual_effective_hz: (360 * 1000) / 3_120,
  });
  assert.equal(assessReplayTiming(replay, 3_151).within_tolerance, false);
});

test("allows the fixed process-start budget for an instant native replay", () => {
  const replay = {
    metadata: {
      scheduled_duration_ms: 0,
      scheduled_active_duration_ms: 0,
      total_event_count: 1,
    },
  };
  assert.equal(assessReplayTiming(replay, 149).within_tolerance, true);
  assert.equal(assessReplayTiming(replay, 151).within_tolerance, false);
});
