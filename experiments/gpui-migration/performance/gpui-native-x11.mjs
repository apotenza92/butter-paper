import { createHash } from "node:crypto";
import { execFile, spawn } from "node:child_process";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import sharp from "sharp";

import {
  measureVisibleRasterFidelity,
  validateDynamicFidelitySeries,
} from "./dynamic-fidelity-v5.mjs";
import {
  mapPdfRectToImagePixels,
  registerAndComparePresentedCropV2,
  renderReferenceCrop,
} from "./registered-crop-v5.mjs";
import {
  retainX11DamageObserverSample,
  parseX11DamageObserverSample,
  runX11DamageObservedAction,
  x11DamageObserverActionContext,
} from "./x11-damage-observer.mjs";

const execFileAsync = promisify(execFile);
const performanceDirectory = dirname(fileURLToPath(import.meta.url));
const directHelperSource = resolve(performanceDirectory, "native-x11-xtest.c");
const directHelperDirectory = resolve(
  performanceDirectory,
  "../../../test-results/gpui-migration-native-input",
);
const directHelperBinary = resolve(directHelperDirectory, "native-x11-xtest");
let directHelperPromise;

export const nativeX11InputLane = "native-x11-xtest";
export const semanticDiagnosticInputLane = "semantic-diagnostic";
export const nativeX11WindowTitle = "Butter Paper GPUI comparison";
export const fixedWindowSize = Object.freeze({ width: 1200, height: 800 });
// Rust-facing protocol: install a SIGUSR1 handler before emitting the open
// receipt. The handler owns no driver paths or expected values. It schedules
// one GPUI next-frame callback and emits viewer-native-presented-state from
// current application state. The driver sends the signal only after its
// XGetImage artifact is closed and hash-bound by the helper receipt.
export const longbridgeCompatCaptureSignalProtocol = Object.freeze({
  signal: "SIGUSR1",
  trigger_owner: "native-x11-driver",
  rust_handler_effect:
    "schedule-next-frame-and-emit-viewer-native-presented-state",
  forbidden_payloads: Object.freeze([
    "artifact-path",
    "capture-id",
    "expected-digest",
  ]),
});
export const dynamicRunnerMilestonesV5 = Object.freeze([
  "timestamped-native-input-complete",
  "fixed-cadence-fidelity-samples-exact",
  "presented-screenshot-crops-three-matched",
  "presented-scale-comparability-proven",
  "checkpoint-holds-stable",
  "visible-page-ready-fraction-recorded",
  "visible-raster-ready-area-fraction-recorded",
  "visible-raster-pixel-density-recorded",
]);

function requireFinite(value, label) {
  if (!Number.isFinite(value)) throw new Error(`${label} must be finite`);
  return value;
}

function requirePositive(value, label) {
  requireFinite(value, label);
  if (value <= 0) throw new Error(`${label} must be positive`);
  return value;
}

function fixed(value) {
  return requirePositive(value, "xdotool delay").toFixed(6);
}

export function parseXdotoolGeometry(text) {
  const fields = Object.fromEntries(
    text
      .trim()
      .split("\n")
      .map((line) => line.split("=", 2)),
  );
  for (const name of ["WINDOW", "X", "Y", "WIDTH", "HEIGHT", "SCREEN"]) {
    if (fields[name] === undefined)
      throw new Error(`xdotool geometry is missing ${name}`);
  }
  const number = (name) => {
    if (!/^-?\d+$/.test(fields[name]))
      throw new Error(`xdotool geometry ${name} is invalid`);
    return Number(fields[name]);
  };
  return {
    window: fields.WINDOW,
    x: number("X"),
    y: number("Y"),
    width: number("WIDTH"),
    height: number("HEIGHT"),
    screen: number("SCREEN"),
  };
}

export function parseXwininfoClientGeometry(text) {
  const field = (label) => {
    const match = text.match(
      new RegExp(`^\\s*${label}:\\s*(-?\\d+)\\s*$`, "m"),
    );
    if (!match) throw new Error(`xwininfo geometry is missing ${label}`);
    return Number(match[1]);
  };
  return {
    x: field("Absolute upper-left X"),
    y: field("Absolute upper-left Y"),
    width: field("Width"),
    height: field("Height"),
  };
}

export function parseX11FrameExtents(text) {
  const match = text.match(
    /_NET_FRAME_EXTENTS\([^)]*\)\s*=\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/,
  );
  if (!match) throw new Error("xprop is missing exact _NET_FRAME_EXTENTS");
  const [left, right, top, bottom] = match.slice(1).map(Number);
  return { left, right, top, bottom };
}

function decoratedClientMatchesRequestedSize(geometry, expectedSize) {
  const extents = geometry?.frame_extents;
  return (
    extents &&
    [extents.left, extents.right, extents.top, extents.bottom].every(
      (value) => Number.isInteger(value) && value >= 0,
    ) &&
    geometry.width + extents.left + extents.right === expectedSize.width &&
    geometry.height + extents.top + extents.bottom === expectedSize.height
  );
}

export function validateWindowTarget({
  ids,
  expectedPid,
  actualPid,
  expectedTitle = nativeX11WindowTitle,
  actualTitle,
  geometry,
  expectedSize = fixedWindowSize,
  allowDecoratedClient = false,
}) {
  if (!Array.isArray(ids) || ids.length !== 1) {
    throw new Error(
      `native X11 replay requires exactly one matching window; received ${ids?.length ?? 0}`,
    );
  }
  if (!Number.isInteger(actualPid) || actualPid !== expectedPid) {
    throw new Error(
      `native X11 replay target PID ${actualPid} does not match launched PID ${expectedPid}`,
    );
  }
  if (actualTitle !== expectedTitle) {
    throw new Error(
      `native X11 replay target title ${JSON.stringify(actualTitle)} does not match ${JSON.stringify(expectedTitle)}`,
    );
  }
  if (geometry?.visible !== true) {
    throw new Error("native X11 replay target is not visible");
  }
  const exactClientSize =
    expectedSize == null ||
    (geometry.width === expectedSize.width &&
      geometry.height === expectedSize.height);
  const exactDecoratedSize =
    expectedSize != null &&
    allowDecoratedClient &&
    decoratedClientMatchesRequestedSize(geometry, expectedSize);
  if (!exactClientSize && !exactDecoratedSize) {
    throw new Error(
      `native X11 replay requires a ${expectedSize.width}x${expectedSize.height} requested window; received ${geometry.width}x${geometry.height} client geometry without matching frame extents`,
    );
  }
  return {
    window_id: ids[0],
    pid: actualPid,
    title: actualTitle,
    geometry: {
      ...geometry,
      requested_size: expectedSize,
      size_verification: exactClientSize
        ? "exact-client-size"
        : "decorated-client-plus-frame-extents",
    },
  };
}

export function validateGpuiLogicalWindowTarget(
  target,
  ready,
  expectedSize = fixedWindowSize,
) {
  const logicalSize = ready?.control?.window_logical_size;
  const geometry = target?.geometry;
  if (
    logicalSize?.width !== geometry?.width ||
    logicalSize?.height !== geometry?.height
  ) {
    throw new Error(
      `GPUI logical view ${logicalSize?.width ?? "missing"}x${logicalSize?.height ?? "missing"} does not match verified X11 client ${geometry?.width ?? "missing"}x${geometry?.height ?? "missing"}`,
    );
  }
  const exactClientSize =
    geometry?.width === expectedSize.width &&
    geometry?.height === expectedSize.height;
  if (
    !exactClientSize &&
    !decoratedClientMatchesRequestedSize(geometry, expectedSize)
  ) {
    throw new Error(
      "GPUI logical view does not match the verified X11 client plus frame extents",
    );
  }
  return {
    ...target,
    geometry: {
      ...geometry,
      app_logical_size: { ...logicalSize },
      requested_size: { ...expectedSize },
      size_verification: exactClientSize
        ? "exact-client-and-app-logical-size"
        : "app-logical-size-plus-decorated-client-frame-extents",
    },
  };
}

export function windowLogicalPointToPixel(point, logicalSize, windowGeometry) {
  const logicalWidth = requirePositive(
    logicalSize?.width,
    "logical window width",
  );
  const logicalHeight = requirePositive(
    logicalSize?.height,
    "logical window height",
  );
  const windowWidth = requirePositive(
    windowGeometry?.width,
    "X11 window width",
  );
  const windowHeight = requirePositive(
    windowGeometry?.height,
    "X11 window height",
  );
  requireFinite(point?.x, "logical point x");
  requireFinite(point?.y, "logical point y");
  const content = windowGeometry.content ?? {
    x: 0,
    y: 0,
    width: windowWidth,
    height: windowHeight,
  };
  for (const [name, value] of Object.entries({
    "content x": content.x,
    "content y": content.y,
    "content width": content.width,
    "content height": content.height,
  }))
    requireFinite(value, name);
  if (content.width <= 0 || content.height <= 0)
    throw new Error("verified X11 content geometry must be positive");
  const mapped = {
    x: Math.round(content.x + (point.x * content.width) / logicalWidth),
    y: Math.round(content.y + (point.y * content.height) / logicalHeight),
  };
  if (
    mapped.x < 0 ||
    mapped.x >= windowWidth ||
    mapped.y < 0 ||
    mapped.y >= windowHeight
  ) {
    throw new Error(
      `logical point (${point.x}, ${point.y}) maps outside the visible X11 window`,
    );
  }
  return mapped;
}

export function buildClickReplay({
  windowId,
  point,
  logicalSize,
  windowGeometry,
}) {
  const mapped = windowLogicalPointToPixel(point, logicalSize, windowGeometry);
  return {
    args: [
      "mousemove",
      "--window",
      String(windowId),
      String(mapped.x),
      String(mapped.y),
      "click",
      "1",
    ],
    pixel: mapped,
    metadata: {
      scheduled_duration_ms: 0,
      scheduled_active_duration_ms: 0,
      total_event_count: 1,
      button: 1,
    },
  };
}

export function buildDirectClickReplay(clickReplay) {
  if (
    !Number.isInteger(clickReplay?.pixel?.x) ||
    !Number.isInteger(clickReplay?.pixel?.y)
  ) {
    throw new Error("direct XTest click replay requires a mapped pixel");
  }
  return {
    ...clickReplay,
    pixel_samples: [clickReplay.pixel, clickReplay.pixel],
    metadata: {
      ...clickReplay.metadata,
      scheduled_duration_ms: 1,
      scheduled_active_duration_ms: 1,
      timed_interval_count: 1,
      total_event_count: 2,
      input_api: "XTEST-pointer",
    },
  };
}

export function buildNativeViewerShellReplay(ready, target) {
  if (!ready?.control?.window_logical_size || !ready?.control?.point) {
    throw new Error(
      "native viewer shell probe requires observed logical window geometry and point",
    );
  }
  const replay = buildClickReplay({
    windowId: target.window_id,
    point: ready.control.point,
    logicalSize: ready.control.window_logical_size,
    windowGeometry: target.geometry,
  });
  return {
    ...replay,
    metadata: {
      ...replay.metadata,
      command_id: "viewer:launch-cold",
      input_api: "XTEST-pointer",
    },
  };
}

export function validateNativeViewerLaunchOpenEvidence(events) {
  const launch = events.find(
    (event) =>
      event.event === "viewer-native-launch-evidence" &&
      event.command_id === "viewer:launch-cold",
  );
  if (
    launch?.native_input_observed !== true ||
    launch?.gpui_platform_draw_submitted !== true ||
    launch?.physical_scanout_observed !== false ||
    !Number.isInteger(launch?.input_latency_samples_before) ||
    !Number.isInteger(launch?.input_latency_samples_after) ||
    launch.input_latency_samples_after <= launch.input_latency_samples_before
  ) {
    throw new Error(
      "viewer:launch-cold lacks a truthful post-input GPUI platform draw submission",
    );
  }
  const open = events.find(
    (event) =>
      event.event === "viewer-native-open-evidence" &&
      event.command_id === "viewer:open-each",
  );
  if (
    open?.document_opened !== true ||
    open?.preview_current_generation !== true ||
    !Number.isFinite(open?.settled_current_generation_ms) ||
    open.settled_current_generation_ms < 250
  ) {
    throw new Error(
      "viewer:open-each lacks a current preview settled for at least 250 ms",
    );
  }
  return {
    success: true,
    launch,
    open,
    gpui_platform_draw_submitted: true,
    physical_scanout_observed: false,
  };
}

function buildPdfClickReplay(command, ready, target, point, metadata = {}) {
  const pixel = pdfPointToWindowPixel(point, ready.surface, target.geometry);
  return {
    args: [
      "mousemove",
      "--window",
      String(target.window_id),
      String(pixel.x),
      String(pixel.y),
      "click",
      "1",
    ],
    pixel,
    execution: "xdotool",
    metadata: {
      scheduled_duration_ms: 0,
      scheduled_active_duration_ms: 0,
      total_event_count: 1,
      button: 1,
      command_id: command.id,
      gpu_present_observed: false,
      gpu_upload_bytes: null,
      ...metadata,
    },
  };
}

export function buildNativeCommandReplay(command, ready, target) {
  if (command.pointer_path) {
    return {
      ...buildPointerReplay({
        windowId: target.window_id,
        rateHz: command.pointer_path.rate_hz,
        durationMs: command.pointer_path.duration_ms,
        pdfSamples: manifestPointerSamples(command),
        surface: ready.surface,
        windowGeometry: target.geometry,
      }),
      execution: "direct-pointer",
    };
  }
  if (
    command.operation === "viewer.continuous-scroll-path" ||
    command.operation === "viewer.dynamic-fidelity-scroll-path"
  ) {
    const path = command.path;
    return {
      ...buildWheelReplay({
        windowId: target.window_id,
        rateHz: command.input_rate_hz,
        forwardDurationMs: path.forward_duration_ms,
        pauseDurationMs: path.pause_duration_ms,
        reverseDurationMs: path.reverse_duration_ms,
        forwardViewportHeights: path.forward_viewport_heights,
        target: {
          x: ready.surface.bounds.x + ready.surface.bounds.width / 2,
          y:
            ready.surface.bounds.y +
            Math.min(ready.surface.bounds.height / 2, 200),
        },
      }),
      execution:
        command.operation === "viewer.dynamic-fidelity-scroll-path"
          ? "direct-dynamic-wheel"
          : "direct-wheel",
    };
  }
  if (command.operation === "annotation.text.create") {
    const replay = buildPdfClickReplay(
      command,
      ready,
      target,
      command.placement?.point,
      {
        native_text_entry_submitted: true,
        native_text_entry_observed: false,
        document_content_prepopulated: false,
        blocked_native_text_entry:
          "the runner submits the frozen ASCII payload, but the report must prove EntityInputHandler changed the document to the exact text in one command history entry",
      },
    );
    replay.args.push("type", "--clearmodifiers", "--delay", "1", command.text);
    replay.metadata.total_event_count += command.text.length;
    replay.metadata.scheduled_duration_ms = command.text.length;
    replay.metadata.scheduled_active_duration_ms = command.text.length;
    return replay;
  }
  if (command.operation === "measurement.set-scale") {
    if (ready.control?.control_id !== "comparison-length-scale") {
      throw new Error(
        "length:set-scale requires observed comparison-length-scale control geometry",
      );
    }
    const bounds = ready.control.bounds;
    const point = {
      x: bounds.x + bounds.width / 2,
      y: bounds.y + bounds.height / 2,
    };
    const replay = buildClickReplay({
      windowId: target.window_id,
      point,
      logicalSize: ready.control.window_logical_size,
      windowGeometry: target.geometry,
    });
    return {
      ...replay,
      execution: "xdotool",
      metadata: {
        ...replay.metadata,
        command_id: command.id,
        control_id: ready.control.control_id,
        gpu_present_observed: false,
        gpu_upload_bytes: null,
      },
    };
  }
  if (command.operation === "annotation.length.create") {
    return {
      ...buildPointerReplay({
        windowId: target.window_id,
        rateHz: 60,
        durationMs: 300,
        pdfSamples: [command.start, command.finish],
        surface: ready.surface,
        windowGeometry: target.geometry,
      }),
      execution: "direct-pointer",
    };
  }
  if (command.operation === "annotation.image.create") {
    if (ready.preloaded_asset_id !== command.asset_id) {
      throw new Error(
        `${command.id} requires preloaded asset ${command.asset_id}`,
      );
    }
    return buildPdfClickReplay(
      command,
      ready,
      target,
      command.placement?.point,
      {
        preloaded_asset_id: ready.preloaded_asset_id,
        native_asset_selection: false,
        blocked_native_asset_selection:
          "the deterministic checker is preloaded; no native file picker was replayed",
      },
    );
  }
  throw new Error(`native-x11-xtest cannot construct ${command.operation}`);
}

function inclusiveDeltaSamples(start, delta, rateHz, durationMs) {
  requireFinite(start?.x, "pointer start x");
  requireFinite(start?.y, "pointer start y");
  requireFinite(delta?.x, "pointer delta x");
  requireFinite(delta?.y, "pointer delta y");
  requirePositive(rateHz, "pointer rate");
  requirePositive(durationMs, "pointer duration");
  const count = Math.round((durationMs * rateHz) / 1000) + 1;
  return Array.from({ length: count }, (_, index) => {
    const fraction = index / (count - 1);
    return {
      x: start.x + delta.x * fraction,
      y: start.y + delta.y * fraction,
    };
  });
}

export function buildNativeRectangleTransformReplay({
  createCommand,
  transformCommand,
  ready,
  target,
}) {
  if (createCommand?.operation !== "annotation.rectangle.create") {
    throw new Error(
      "native rectangle transform requires the manifest rectangle create prerequisite",
    );
  }
  if (transformCommand?.operation !== "annotation.rectangle.transform") {
    throw new Error(
      "native rectangle transform requires the manifest transform command",
    );
  }
  if (createCommand.annotation_id !== transformCommand.annotation_id) {
    throw new Error(
      "native rectangle transform prerequisite annotation ID differs from transform target",
    );
  }
  if (ready?.stage === "prerequisite-create") {
    const replay = buildNativeCommandReplay(createCommand, ready, target);
    return {
      ...replay,
      metadata: {
        ...replay.metadata,
        stage: ready.stage,
        prerequisite_for_command_id: transformCommand.id,
      },
    };
  }
  let start;
  let delta;
  let rateHz;
  let durationMs;
  if (ready?.stage === "move") {
    start = transformCommand.select_point;
    delta = transformCommand.move?.delta;
    rateHz = transformCommand.move?.rate_hz;
    durationMs = transformCommand.move?.duration_ms;
  } else if (ready?.stage === "east-resize") {
    if (transformCommand.resize?.handle !== "east") {
      throw new Error(
        "native rectangle transform supports only the manifest east resize handle",
      );
    }
    if (!ready.handle_point) {
      throw new Error("east resize requires the GPUI-observed handle point");
    }
    start = ready.handle_point;
    delta = transformCommand.resize.delta;
    rateHz = transformCommand.resize.rate_hz;
    durationMs = transformCommand.resize.duration_ms;
  } else {
    throw new Error(`unknown native rectangle transform stage ${ready?.stage}`);
  }
  const replay = buildPointerReplay({
    windowId: target.window_id,
    rateHz,
    durationMs,
    pdfSamples: inclusiveDeltaSamples(start, delta, rateHz, durationMs),
    surface: ready.surface,
    windowGeometry: target.geometry,
  });
  return {
    ...replay,
    execution: "direct-pointer",
    metadata: {
      ...replay.metadata,
      command_id: transformCommand.id,
      stage: ready.stage,
      select_semantics:
        ready.stage === "move"
          ? "no-fill-edge-or-stroked-body"
          : "east-resize-handle",
      gpu_present_observed: false,
      gpu_upload_bytes: null,
    },
  };
}

export function buildNativeV5SnapReplay(command, ready, target) {
  if (command?.operation !== "annotation.snap-transform-native") {
    throw new Error("native v5 snap replay requires the frozen snap command");
  }
  if (
    command.expected_sample_count !== 361 ||
    command.rate_hz !== 120 ||
    command.duration_ms !== 3_000
  ) {
    throw new Error("native v5 snap replay cadence is not exact");
  }
  const start = command.pointer_path?.start;
  const end = command.pointer_path?.unsnapped_end;
  requireFinite(start?.x, "snap pointer start x");
  requireFinite(start?.y, "snap pointer start y");
  requireFinite(end?.x, "snap pointer end x");
  requireFinite(end?.y, "snap pointer end y");
  const samples = Array.from(
    { length: command.expected_sample_count },
    (_, index) => {
      const fraction = index / (command.expected_sample_count - 1);
      return {
        x: start.x + (end.x - start.x) * fraction,
        y: start.y + (end.y - start.y) * fraction,
      };
    },
  );
  const replay = buildPointerReplay({
    windowId: target.window_id,
    rateHz: command.rate_hz,
    durationMs: command.duration_ms,
    pdfSamples: samples,
    surface: ready.surface,
    windowGeometry: target.geometry,
  });
  return {
    ...replay,
    execution: "direct-pointer",
    metadata: {
      ...replay.metadata,
      command_id: command.id,
      sensitivity_css_px: command.snap.sensitivity.value,
      threshold_norm: command.snap.sensitivity.threshold_norm,
      inclusive: command.snap.sensitivity.inclusive,
    },
  };
}

export function validateNativeRectangleTransformEvidence(events) {
  const receipt = events.find(
    (event) =>
      event.event === "comparison-native-transform-evidence" &&
      event.command_id === "rectangle:select-move-resize",
  );
  if (!receipt)
    throw new Error("native rectangle transform evidence is missing");
  if (
    receipt.select_semantics !== "no-fill-edge-or-stroked-body" ||
    receipt.hit_test_selected !== true
  ) {
    throw new Error(
      "native rectangle transform did not prove decision-3 edge hit selection",
    );
  }
  if (receipt.move_history_delta !== 1 || receipt.resize_history_delta !== 1) {
    throw new Error(
      "native rectangle transform did not commit one history transaction per gesture",
    );
  }
  if (
    !Array.isArray(receipt.observed_final_rect) ||
    !Array.isArray(receipt.expected_final_rect) ||
    receipt.observed_final_rect.length !== 4 ||
    receipt.expected_final_rect.length !== 4 ||
    !receipt.observed_final_rect.every(Number.isFinite) ||
    !receipt.expected_final_rect.every(Number.isFinite) ||
    !Number.isFinite(receipt.pixels_per_point) ||
    receipt.pixels_per_point <= 0 ||
    !Number.isFinite(receipt.maximum_geometry_error_device_px) ||
    !Number.isFinite(receipt.geometry_tolerance_device_px) ||
    receipt.maximum_geometry_error_device_px >
      receipt.geometry_tolerance_device_px
  ) {
    throw new Error(
      "native rectangle transform exceeded its declared geometry tolerance",
    );
  }
  const independentMaximumError = Math.max(
    ...receipt.observed_final_rect.map(
      (observed, index) =>
        Math.abs(observed - receipt.expected_final_rect[index]) *
        receipt.pixels_per_point,
    ),
  );
  if (
    Math.abs(
      independentMaximumError - receipt.maximum_geometry_error_device_px,
    ) > 1e-9
  ) {
    throw new Error(
      "native rectangle transform reported the wrong independent geometry error",
    );
  }
  if (
    receipt.gpui_platform_draw_submitted !== true ||
    receipt.physical_scanout_observed !== false
  ) {
    throw new Error(
      "native rectangle transform lacks a truthful GPUI platform draw submission",
    );
  }
  return { success: true, receipt };
}

export function pdfPointToWindowPixel(point, surface, windowGeometry) {
  const logicalWidth = requirePositive(
    surface?.window_logical_size?.width,
    "logical window width",
  );
  const logicalHeight = requirePositive(
    surface?.window_logical_size?.height,
    "logical window height",
  );
  const windowWidth = requirePositive(
    windowGeometry?.width,
    "X11 window width",
  );
  const windowHeight = requirePositive(
    windowGeometry?.height,
    "X11 window height",
  );
  const pixelsPerPointX = requirePositive(
    surface?.pixels_per_point_x ?? surface?.pixels_per_point,
    "surface horizontal pixels per point",
  );
  const pixelsPerPointY = requirePositive(
    surface?.pixels_per_point_y ?? surface?.pixels_per_point,
    "surface vertical pixels per point",
  );
  const pageHeight = requirePositive(
    surface?.page_height_points,
    "page height",
  );
  const bounds = surface?.bounds;
  for (const [name, value] of Object.entries({
    "surface x": bounds?.x,
    "surface y": bounds?.y,
    "PDF x": point?.x,
    "PDF y": point?.y,
  }))
    requireFinite(value, name);
  const logicalX = bounds.x + point.x * pixelsPerPointX;
  const logicalY = bounds.y + (pageHeight - point.y) * pixelsPerPointY;
  if (
    logicalX < bounds.x ||
    logicalX > bounds.x + bounds.width ||
    logicalY < bounds.y ||
    logicalY > bounds.y + bounds.height
  ) {
    throw new Error(
      `PDF point (${point.x}, ${point.y}) is outside the verified page surface`,
    );
  }
  const content = windowGeometry.content ?? {
    x: 0,
    y: 0,
    width: windowWidth,
    height: windowHeight,
  };
  const mapped = {
    x: Math.round(content.x + (logicalX * content.width) / logicalWidth),
    y: Math.round(content.y + (logicalY * content.height) / logicalHeight),
  };
  if (
    mapped.x < 0 ||
    mapped.x >= windowWidth ||
    mapped.y < 0 ||
    mapped.y >= windowHeight
  ) {
    throw new Error(
      `PDF point (${point.x}, ${point.y}) maps outside the visible X11 window`,
    );
  }
  return mapped;
}

function timestampedPixelSamples({
  pdfSamples,
  durationMs,
  surface,
  windowGeometry,
}) {
  if (!Array.isArray(pdfSamples) || pdfSamples.length < 2) {
    throw new Error(
      "pointer replay requires at least two PDF coordinate samples",
    );
  }
  const denominator = pdfSamples.length - 1;
  return pdfSamples.map((point, index) => ({
    ...pdfPointToWindowPixel(point, surface, windowGeometry),
    scheduled_ms: (durationMs * index) / denominator,
  }));
}

export function buildPointerReplay({
  windowId,
  button = 1,
  rateHz,
  durationMs,
  pdfSamples,
  surface,
  windowGeometry,
}) {
  if (!Number.isInteger(button) || button < 1 || button > 3) {
    throw new Error("pointer button must be 1, 2, or 3");
  }
  requirePositive(rateHz, "pointer rate");
  requirePositive(durationMs, "pointer duration");
  const expectedSamples = Math.round((durationMs * rateHz) / 1000) + 1;
  // Small literal sequences are allowed in unit tests. Manifest runs must use
  // the exact inclusive sample count frozen by duration and rate.
  if (pdfSamples.length > 3 && pdfSamples.length !== expectedSamples) {
    throw new Error(
      `pointer replay has ${pdfSamples.length} samples; expected ${expectedSamples}`,
    );
  }
  const pixelSamples = timestampedPixelSamples({
    pdfSamples,
    durationMs,
    surface,
    windowGeometry,
  });
  const first = pixelSamples[0];
  const args = [
    "mousemove",
    "--window",
    String(windowId),
    String(first.x),
    String(first.y),
    "mousedown",
    String(button),
  ];
  for (let index = 1; index < pixelSamples.length; index += 1) {
    const delayMs =
      pixelSamples[index].scheduled_ms - pixelSamples[index - 1].scheduled_ms;
    const sample = pixelSamples[index];
    args.push(
      "sleep",
      fixed(delayMs / 1000),
      "mousemove",
      "--window",
      String(windowId),
      String(sample.x),
      String(sample.y),
    );
  }
  args.push("mouseup", String(button));
  return {
    args,
    pixel_samples: pixelSamples,
    metadata: {
      coordinate_sample_count: pixelSamples.length,
      timed_interval_count: pixelSamples.length - 1,
      rate_hz: rateHz,
      scheduled_first_ms: 0,
      scheduled_last_ms: durationMs,
      scheduled_duration_ms: durationMs,
      scheduled_active_duration_ms: durationMs,
      button,
    },
  };
}

export function buildWheelReplay({
  windowId,
  rateHz,
  forwardDurationMs,
  pauseDurationMs,
  reverseDurationMs,
  forwardViewportHeights,
  target,
}) {
  requirePositive(rateHz, "wheel rate");
  requirePositive(forwardDurationMs, "forward wheel duration");
  requirePositive(reverseDurationMs, "reverse wheel duration");
  requirePositive(forwardViewportHeights, "forward viewport distance");
  requireFinite(pauseDurationMs, "wheel pause");
  if (pauseDurationMs < 0) throw new Error("wheel pause must not be negative");
  const forwardCount = Math.round((forwardDurationMs * rateHz) / 1000);
  const reverseCount = Math.round((reverseDurationMs * rateHz) / 1000);
  const reverseNotchesPerEvent = forwardDurationMs / reverseDurationMs;
  if (!Number.isInteger(reverseNotchesPerEvent) || reverseNotchesPerEvent < 1) {
    throw new Error(
      "native wheel replay requires an integer reverse delta multiplier",
    );
  }
  const intervalSeconds = fixed(1 / rateHz);
  const args = [
    "mousemove",
    "--window",
    String(windowId),
    String(Math.round(target.x)),
    String(Math.round(target.y)),
  ];
  for (let index = 0; index < forwardCount; index += 1) {
    args.push("click", "5", "sleep", intervalSeconds);
  }
  if (pauseDurationMs > 0) args.push("sleep", fixed(pauseDurationMs / 1000));
  for (let index = 0; index < reverseCount; index += 1) {
    for (let notch = 0; notch < reverseNotchesPerEvent; notch += 1)
      args.push("click", "4");
    args.push("sleep", intervalSeconds);
  }
  const reverseFirstMs = forwardDurationMs + pauseDurationMs;
  return {
    args,
    pixel_target: { x: Math.round(target.x), y: Math.round(target.y) },
    metadata: {
      scheduled_duration_ms:
        forwardDurationMs + pauseDurationMs + reverseDurationMs,
      scheduled_active_duration_ms: forwardDurationMs + reverseDurationMs,
      forward_interval_count: forwardCount,
      reverse_interval_count: reverseCount,
      timed_interval_count: forwardCount + reverseCount,
      forward_notches_per_event: 1,
      reverse_notches_per_event: reverseNotchesPerEvent,
      forward_physical_click_count: forwardCount,
      reverse_physical_click_count: reverseCount * reverseNotchesPerEvent,
      total_physical_click_count:
        forwardCount + reverseCount * reverseNotchesPerEvent,
      expected_application_wheel_event_count:
        forwardCount + reverseCount * reverseNotchesPerEvent,
      expected_peak_viewport_heights: forwardViewportHeights,
      rate_hz: rateHz,
      forward_button: 5,
      reverse_button: 4,
      scheduled_first_ms: 0,
      scheduled_reverse_first_ms: reverseFirstMs,
      scheduled_last_ms: reverseFirstMs + reverseDurationMs - 1000 / rateHz,
    },
  };
}

export function nativeX11LaneMetadata(evidence) {
  const realGuiRun = evidence?.target_verified === true;
  const passed =
    realGuiRun &&
    evidence?.success === true &&
    evidence?.application_success === true;
  return {
    input_lane: nativeX11InputLane,
    injection_api: "XTEST-direct-helper",
    execution_status:
      evidence === null || evidence === undefined
        ? "not-run"
        : passed
          ? "passed"
          : "failed",
    real_gui_run: realGuiRun,
    decision_timing_eligible: passed,
  };
}

export function assessReplayTiming(replay, actualDurationMs) {
  const scheduledDurationMs = replay.metadata.scheduled_duration_ms;
  // XTEST pointer streams are measured inside the direct helper. Instant
  // keyboard and click replays launch xdotool, so allow a fixed process-start
  // budget without relaxing the five-percent budget for sustained streams.
  const toleranceMs = Math.max(150, scheduledDurationMs * 0.05);
  const durationErrorMs = actualDurationMs - scheduledDurationMs;
  const timedEvents =
    replay.metadata.timed_interval_count ?? replay.metadata.total_event_count;
  const activeActualMs = Math.max(
    1,
    actualDurationMs -
      (scheduledDurationMs - replay.metadata.scheduled_active_duration_ms),
  );
  return {
    within_tolerance: Math.abs(durationErrorMs) <= toleranceMs,
    tolerance_ms: toleranceMs,
    duration_error_ms: durationErrorMs,
    actual_effective_hz: (timedEvents * 1000) / activeActualMs,
  };
}

export function manifestPointerSamples(command) {
  const path = command?.pointer_path;
  const sampleCount = path?.expected_sample_count;
  if (!Number.isInteger(sampleCount) || sampleCount < 2) {
    throw new Error(
      `${command?.id ?? "pointer command"} has no valid sample count`,
    );
  }
  if (path.interpolation === "linear-inclusive") {
    return Array.from({ length: sampleCount }, (_, index) => {
      const t = index / (sampleCount - 1);
      return {
        x: path.start.x + (path.finish.x - path.start.x) * t,
        y: path.start.y + (path.finish.y - path.start.y) * t,
      };
    });
  }
  if (path.interpolation === "catmull-rom-inclusive") {
    const points = path.control_points.map(([x, y]) => ({ x, y }));
    const segments = points.length - 1;
    const spline = (p0, p1, p2, p3, t) => {
      const t2 = t * t;
      const t3 = t2 * t;
      return (
        0.5 *
        (2 * p1 +
          (-p0 + p2) * t +
          (2 * p0 - 5 * p1 + 4 * p2 - p3) * t2 +
          (-p0 + 3 * p1 - 3 * p2 + p3) * t3)
      );
    };
    return Array.from({ length: sampleCount }, (_, index) => {
      const global = (index / (sampleCount - 1)) * segments;
      const segment = Math.min(Math.floor(global), segments - 1);
      const t = index + 1 === sampleCount ? 1 : global - segment;
      const p0 = points[Math.max(0, segment - 1)];
      const p1 = points[segment];
      const p2 = points[segment + 1];
      const p3 = points[Math.min(points.length - 1, segment + 2)];
      return {
        x: spline(p0.x, p1.x, p2.x, p3.x, t),
        y: spline(p0.y, p1.y, p2.y, p3.y, t),
      };
    });
  }
  throw new Error(
    `${command.id} uses unsupported interpolation ${path.interpolation}`,
  );
}

async function waitForReadyEvent(events, commandId, timeoutMs, stage = null) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const ready = events.find(
      (event) =>
        event.event === "native-input-ready" &&
        event.command_id === commandId &&
        (stage === null || event.stage === stage),
    );
    if (ready) return ready;
    if (events.some((event) => event.event === "scenario-error")) {
      throw new Error(
        `GPUI reported scenario-error before ${commandId} became native-input-ready`,
      );
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 5));
  }
  throw new Error(`timed out waiting for native-input-ready for ${commandId}`);
}

async function waitForCommandEvent(events, eventName, commandId, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const observed = events.find(
      (event) => event.event === eventName && event.command_id === commandId,
    );
    if (observed) return observed;
    if (events.some((event) => event.event === "scenario-error")) {
      throw new Error(
        `GPUI reported scenario-error before ${eventName} for ${commandId}`,
      );
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 5));
  }
  throw new Error(`timed out waiting for ${eventName} for ${commandId}`);
}

async function waitForMatchingEvent(events, predicate, label, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const observed = events.find(predicate);
    if (observed) return observed;
    if (events.some((event) => event.event === "scenario-error")) {
      throw new Error(`GPUI reported scenario-error before ${label}`);
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 5));
  }
  throw new Error(`timed out waiting for ${label}`);
}

export async function runXdotool(args, scheduledDurationMs) {
  const started = process.hrtime.bigint();
  await execFileAsync("xdotool", args, {
    encoding: "utf8",
    timeout: Math.ceil(scheduledDurationMs + 5_000),
    maxBuffer: 64_000,
  });
  return Number(process.hrtime.bigint() - started) / 1e6;
}

export async function runDamageObservedXTestClick(
  replay,
  target,
  actionLabel = "click",
) {
  const damageContext = x11DamageObserverActionContext(actionLabel);
  if (!damageContext) {
    return runXdotool(replay.args, replay.metadata?.scheduled_duration_ms ?? 0);
  }
  if (
    !Number.isInteger(replay?.pixel?.x) ||
    !Number.isInteger(replay?.pixel?.y) ||
    !Number.isInteger(replay?.metadata?.button)
  ) {
    throw new Error(
      "damage-observed XTEST click requires an exact mapped replay",
    );
  }
  const observed = await runX11DamageObservedAction({
    windowId: target.window_id,
    timeoutMs: 5_000,
    action: {
      type: "click",
      x: target.geometry.x + replay.pixel.x,
      y: target.geometry.y + replay.pixel.y,
      button: replay.metadata.button,
    },
    ...damageContext,
  });
  retainX11DamageObserverSample(observed.sample);
  return observed.duration_ms;
}

export async function runDamageObservedXTestExternalClick(
  target,
  inputWindowId,
  screenPoint,
  actionLabel = "external-click",
) {
  if (
    !/^\d+$/.test(String(inputWindowId)) ||
    !Number.isInteger(screenPoint?.x) ||
    !Number.isInteger(screenPoint?.y)
  ) {
    throw new Error(
      "damage-observed external click requires a verified input window and integer screen point",
    );
  }
  const damageContext = x11DamageObserverActionContext(actionLabel);
  if (!damageContext) {
    return runXdotool(
      [
        "mousemove",
        "--sync",
        String(screenPoint.x),
        String(screenPoint.y),
        "click",
        "1",
      ],
      0,
    );
  }
  const observed = await runX11DamageObservedAction({
    windowId: target.window_id,
    timeoutMs: 10_000,
    action: {
      type: "click",
      x: screenPoint.x,
      y: screenPoint.y,
      button: 1,
      inputWindowId,
    },
    ...damageContext,
  });
  retainX11DamageObserverSample(observed.sample);
  return observed.duration_ms;
}

export async function runDamageObservedXTestKey(
  target,
  keysym,
  actionLabel = "key",
  inputWindowId = target?.window_id,
) {
  const damageContext = x11DamageObserverActionContext(actionLabel);
  if (!damageContext) {
    return runXdotool(
      ["key", "--window", String(target.window_id), "--clearmodifiers", keysym],
      0,
    );
  }
  const observed = await runX11DamageObservedAction({
    windowId: target.window_id,
    timeoutMs: 10_000,
    action: { type: "key", keysym, inputWindowId },
    ...damageContext,
  });
  retainX11DamageObserverSample(observed.sample);
  return observed.duration_ms;
}

async function buildDirectXTestHelper() {
  directHelperPromise ??= (async () => {
    await mkdir(directHelperDirectory, { recursive: true });
    await execFileAsync(
      "cc",
      [
        "-std=c11",
        "-O2",
        "-Wall",
        "-Wextra",
        "-Werror",
        directHelperSource,
        "-o",
        directHelperBinary,
        "-lX11",
        "-lXpresent",
        "-lXdamage",
        "-lXfixes",
        "-l:libXtst.so.6",
        "-lm",
      ],
      { encoding: "utf8", timeout: 10_000, maxBuffer: 64_000 },
    );
    return directHelperBinary;
  })();
  return directHelperPromise;
}

export async function runNativeX11HelperSelfTest() {
  const binary = await buildDirectXTestHelper();
  const { stdout, stderr } = await execFileAsync(binary, ["self-test"], {
    encoding: "utf8",
    timeout: 5_000,
    maxBuffer: 64_000,
  });
  if (stdout !== "self-test-ok\n" || stderr !== "") {
    throw new Error("native X11 helper self-test receipt is invalid");
  }
  return true;
}

export function parsePresentedDrawableCaptureReceipt(line) {
  const fields = line.trim().split("\t");
  if (fields[0] === "ready" && fields.length === 6) {
    const receipt = {
      event: "capture-server-ready",
      observed_monotonic_ms: Number(fields[1]),
      window_id: fields[2],
      width: Number(fields[3]),
      height: Number(fields[4]),
      depth: Number(fields[5]),
      source: "XGetImage-presented-client-drawable",
    };
    if (
      !Number.isFinite(receipt.observed_monotonic_ms) ||
      !/^\d+$/.test(receipt.window_id) ||
      !Number.isInteger(receipt.width) ||
      receipt.width <= 0 ||
      !Number.isInteger(receipt.height) ||
      receipt.height <= 0 ||
      !Number.isInteger(receipt.depth) ||
      receipt.depth <= 0
    ) {
      throw new Error("native capture server ready receipt is invalid");
    }
    return receipt;
  }
  if (fields[0] === "capture" && fields.length === 10) {
    const receipt = {
      event: "presented-drawable-captured",
      capture_id: fields[1],
      capture_started_monotonic_ms: Number(fields[2]),
      capture_ended_monotonic_ms: Number(fields[3]),
      window_id: fields[4],
      width: Number(fields[5]),
      height: Number(fields[6]),
      depth: Number(fields[7]),
      artifact_sha256: fields[8],
      ppm_path: fields[9],
      source: "XGetImage-presented-client-drawable",
    };
    if (
      !receipt.capture_id ||
      !Number.isFinite(receipt.capture_started_monotonic_ms) ||
      !Number.isFinite(receipt.capture_ended_monotonic_ms) ||
      receipt.capture_ended_monotonic_ms <
        receipt.capture_started_monotonic_ms ||
      !/^\d+$/.test(receipt.window_id) ||
      !Number.isInteger(receipt.width) ||
      receipt.width <= 0 ||
      !Number.isInteger(receipt.height) ||
      receipt.height <= 0 ||
      !Number.isInteger(receipt.depth) ||
      receipt.depth <= 0 ||
      !/^[a-f0-9]{64}$/.test(receipt.artifact_sha256) ||
      !receipt.ppm_path
    ) {
      throw new Error("native presented-drawable capture receipt is invalid");
    }
    return receipt;
  }
  if (fields[0] === "closed" && fields.length === 2) {
    const observedMonotonicMs = Number(fields[1]);
    if (!Number.isFinite(observedMonotonicMs)) {
      throw new Error("native capture server close receipt is invalid");
    }
    return {
      event: "capture-server-closed",
      observed_monotonic_ms: observedMonotonicMs,
    };
  }
  throw new Error("native capture server emitted an unknown receipt");
}

export async function closePresentedDrawableCaptureServerStrict(server) {
  if (!server || typeof server.close !== "function") {
    throw new Error("capture helper has no close operation");
  }
  let result;
  try {
    result = await server.close();
  } catch (error) {
    if (typeof server.terminateAndWait === "function") {
      try {
        await server.terminateAndWait();
      } catch (cleanupError) {
        throw new AggregateError(
          [error, cleanupError],
          "capture helper failed and could not be reaped",
        );
      }
    }
    throw error;
  }
  if (
    result?.closed?.event !== "capture-server-closed" ||
    !Number.isFinite(result.closed.observed_monotonic_ms) ||
    result?.outcome?.code !== 0 ||
    result.outcome.signal != null
  ) {
    if (typeof server.terminateAndWait === "function") {
      await server.terminateAndWait();
    }
    throw new Error(
      "capture helper acceptance requires a valid close receipt and zero exit",
    );
  }
  return result;
}

function boundedPromise(promise, timeoutMs, label) {
  let timeout;
  return Promise.race([
    promise,
    new Promise((_, reject) => {
      timeout = setTimeout(
        () => reject(new Error(`timed out waiting for ${label}`)),
        timeoutMs,
      );
      timeout.unref?.();
    }),
  ]).finally(() => clearTimeout(timeout));
}

export async function startPresentedDrawableCaptureServer(
  target,
  { timeoutMs = 5_000, spawnProcess = spawn } = {},
) {
  const binary = await buildDirectXTestHelper();
  const child = spawnProcess(
    binary,
    [
      "capture-server",
      String(target.window_id),
      String(target.geometry.width),
      String(target.geometry.height),
    ],
    { stdio: ["pipe", "pipe", "pipe"] },
  );
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  let stdoutBuffer = "";
  let stderr = "";
  let terminalError = null;
  let readyResolve;
  let readyReject;
  const readyPromise = new Promise((resolvePromise, rejectPromise) => {
    readyResolve = resolvePromise;
    readyReject = rejectPromise;
  });
  const pending = new Map();
  let closeResolve;
  const closeReceiptPromise = new Promise((resolvePromise) => {
    closeResolve = resolvePromise;
  });
  const rejectAll = (error) => {
    terminalError = error;
    readyReject(error);
    for (const { reject } of pending.values()) reject(error);
    pending.clear();
  };
  child.stderr.on("data", (chunk) => {
    stderr = `${stderr}${chunk}`.slice(-64_000);
  });
  child.stdout.on("data", (chunk) => {
    stdoutBuffer += chunk;
    for (;;) {
      const newline = stdoutBuffer.indexOf("\n");
      if (newline < 0) break;
      const line = stdoutBuffer.slice(0, newline);
      stdoutBuffer = stdoutBuffer.slice(newline + 1);
      let receipt;
      try {
        receipt = parsePresentedDrawableCaptureReceipt(line);
      } catch (error) {
        rejectAll(error);
        child.kill("SIGTERM");
        continue;
      }
      if (receipt.event === "capture-server-ready") {
        readyResolve(receipt);
      } else if (receipt.event === "presented-drawable-captured") {
        const waiter = pending.get(receipt.capture_id);
        if (!waiter) {
          rejectAll(
            new Error(
              `native capture server returned unexpected ${receipt.capture_id}`,
            ),
          );
          child.kill("SIGTERM");
          continue;
        }
        pending.delete(receipt.capture_id);
        waiter.resolve(receipt);
      } else {
        closeResolve(receipt);
      }
    }
  });
  const exitPromise = new Promise((resolvePromise) => {
    child.once("error", (error) => {
      rejectAll(error);
      resolvePromise({ error });
    });
    child.once("close", (code, signal) => {
      if (code !== 0 && terminalError == null) {
        rejectAll(
          new Error(
            `native capture server exited ${code ?? signal}: ${stderr.trim()}`,
          ),
        );
      }
      resolvePromise({ code, signal });
    });
  });
  const terminateAndWait = async () => {
    if (child.exitCode == null && child.signalCode == null) {
      child.kill("SIGTERM");
    }
    try {
      const outcome = await boundedPromise(
        exitPromise,
        timeoutMs,
        "native capture server SIGTERM exit",
      );
      return { exited: true, outcome };
    } catch (termError) {
      if (child.exitCode == null && child.signalCode == null) {
        child.kill("SIGKILL");
      }
      try {
        const outcome = await boundedPromise(
          exitPromise,
          timeoutMs,
          "native capture server SIGKILL exit",
        );
        return { exited: true, outcome };
      } catch (killError) {
        throw new AggregateError(
          [termError, killError],
          "native capture server could not be reaped",
        );
      }
    }
  };
  let ready;
  try {
    ready = await boundedPromise(
      readyPromise,
      timeoutMs,
      "native capture server readiness",
    );
    validatePresentedDrawableCaptureReceipt(ready, target);
  } catch (error) {
    await terminateAndWait();
    throw error;
  }
  return {
    ready,
    async capture(captureId, ppmPath) {
      if (terminalError) throw terminalError;
      if (!captureId || /[\t\r\n]/.test(captureId)) {
        throw new Error("native capture id is invalid");
      }
      if (!ppmPath || /[\t\r\n]/.test(ppmPath)) {
        throw new Error("native capture path is invalid");
      }
      if (pending.has(captureId)) {
        throw new Error(`native capture ${captureId} is already pending`);
      }
      const receiptPromise = new Promise((resolvePromise, rejectPromise) => {
        pending.set(captureId, {
          resolve: resolvePromise,
          reject: rejectPromise,
        });
      });
      child.stdin.write(`capture\t${captureId}\t${ppmPath}\n`);
      const receipt = await boundedPromise(
        receiptPromise,
        timeoutMs,
        `native capture ${captureId}`,
      );
      validatePresentedDrawableCaptureReceipt(receipt, target, {
        captureId,
        ppmPath,
      });
      return receipt;
    },
    async close() {
      try {
        if (child.exitCode == null && child.signalCode == null) {
          child.stdin.end("close\n");
        }
        const [closed, outcome] = await Promise.all([
          boundedPromise(
            closeReceiptPromise,
            timeoutMs,
            "native capture server close receipt",
          ),
          boundedPromise(exitPromise, timeoutMs, "native capture server exit"),
        ]);
        return { closed, outcome };
      } catch (error) {
        await terminateAndWait();
        throw error;
      }
    },
    terminateAndWait,
    terminate() {
      if (!child.killed && child.exitCode == null) child.kill("SIGTERM");
    },
  };
}

export function validatePresentedDrawableCaptureReceipt(
  receipt,
  target,
  expected = {},
) {
  if (
    receipt?.source !== "XGetImage-presented-client-drawable" ||
    receipt.window_id !== String(target?.window_id) ||
    receipt.width !== target?.geometry?.width ||
    receipt.height !== target?.geometry?.height ||
    !Number.isInteger(receipt.depth) ||
    receipt.depth <= 0 ||
    (receipt.event === "presented-drawable-captured" &&
      !/^[a-f0-9]{64}$/.test(receipt.artifact_sha256 ?? ""))
  ) {
    throw new Error(
      "presented-drawable capture does not match the verified X11 client",
    );
  }
  if (
    expected.captureId !== undefined &&
    receipt.capture_id !== expected.captureId
  ) {
    throw new Error("presented-drawable capture id changed");
  }
  if (expected.ppmPath !== undefined && receipt.ppm_path !== expected.ppmPath) {
    throw new Error("presented-drawable capture path changed");
  }
  return receipt;
}

export async function runDirectXTestPointer(replay, target) {
  if (
    !Array.isArray(replay?.pixel_samples) ||
    replay.pixel_samples.length < 2
  ) {
    throw new Error(
      "direct XTest pointer replay requires mapped pixel samples",
    );
  }
  const damageContext = x11DamageObserverActionContext("pointer");
  const args = [
    "pointer",
    String(Math.round(replay.metadata.scheduled_duration_ms)),
    String(replay.pixel_samples.length),
    String(replay.metadata.button),
  ];
  for (const sample of replay.pixel_samples) {
    args.push(
      String(target.geometry.x + sample.x),
      String(target.geometry.y + sample.y),
    );
  }
  if (damageContext) {
    const observed = await runX11DamageObservedAction({
      windowId: target.window_id,
      timeoutMs: Math.max(
        5_000,
        Math.ceil(replay.metadata.scheduled_duration_ms + 5_000),
      ),
      action: {
        type: "pointer",
        durationMs: Math.round(replay.metadata.scheduled_duration_ms),
        button: replay.metadata.button,
        screenSamples: replay.pixel_samples.map(({ x, y }) => ({
          x: target.geometry.x + x,
          y: target.geometry.y + y,
        })),
      },
      ...damageContext,
    });
    retainX11DamageObserverSample(observed.sample);
    return observed.duration_ms;
  }
  const binary = await buildDirectXTestHelper();
  const started = process.hrtime.bigint();
  await execFileAsync(binary, args, {
    encoding: "utf8",
    timeout: Math.ceil(replay.metadata.scheduled_duration_ms + 5_000),
    maxBuffer: 64_000,
  });
  return Number(process.hrtime.bigint() - started) / 1e6;
}

export async function runDirectXTestPointerObserved(replay, target) {
  if (
    !Array.isArray(replay?.pixel_samples) ||
    replay.pixel_samples.length < 2
  ) {
    throw new Error(
      "observed direct XTest pointer replay requires mapped pixel samples",
    );
  }
  const damageContext = x11DamageObserverActionContext("pointer-observed");
  const args = [
    "pointer",
    String(Math.round(replay.metadata.scheduled_duration_ms)),
    String(replay.pixel_samples.length),
    String(replay.metadata.button),
  ];
  for (const sample of replay.pixel_samples) {
    args.push(
      String(target.geometry.x + sample.x),
      String(target.geometry.y + sample.y),
    );
  }
  if (damageContext) {
    const observed = await runX11DamageObservedAction({
      windowId: target.window_id,
      timeoutMs: Math.max(
        5_000,
        Math.ceil(replay.metadata.scheduled_duration_ms + 5_000),
      ),
      action: {
        type: "pointer",
        durationMs: Math.round(replay.metadata.scheduled_duration_ms),
        button: replay.metadata.button,
        screenSamples: replay.pixel_samples.map(({ x, y }) => ({
          x: target.geometry.x + x,
          y: target.geometry.y + y,
        })),
      },
      ...damageContext,
    });
    retainX11DamageObserverSample(observed.sample);
    return {
      duration_ms: observed.duration_ms,
      injected_samples: observed.sample.injected_samples.map(
        (sample, index) => ({
          ...sample,
          scheduled_offset_ms:
            replay.pixel_samples[index]?.scheduled_ms ??
            (replay.metadata.scheduled_duration_ms * index) /
              (replay.pixel_samples.length - 1),
        }),
      ),
    };
  }
  const binary = await buildDirectXTestHelper();
  const started = process.hrtime.bigint();
  const { stdout } = await execFileAsync(binary, args, {
    encoding: "utf8",
    timeout: Math.ceil(replay.metadata.scheduled_duration_ms + 5_000),
    maxBuffer: 2_000_000,
  });
  return {
    duration_ms: Number(process.hrtime.bigint() - started) / 1e6,
    injected_samples: parseNativeClockSamples(
      stdout,
      replay.pixel_samples.length,
    ),
  };
}

export async function runDirectXTestWheel(replay, target) {
  if (!replay?.pixel_target || !(replay.metadata?.rate_hz > 0)) {
    throw new Error(
      "direct XTest wheel replay requires a mapped target and exact rate",
    );
  }
  const forwardDuration =
    replay.metadata.scheduled_reverse_first_ms -
    (replay.metadata.scheduled_duration_ms -
      replay.metadata.scheduled_active_duration_ms);
  const pauseDuration =
    replay.metadata.scheduled_duration_ms -
    replay.metadata.scheduled_active_duration_ms;
  const reverseDuration =
    replay.metadata.scheduled_duration_ms - forwardDuration - pauseDuration;
  const args = [
    "wheel",
    String(Math.round(forwardDuration)),
    String(Math.round(pauseDuration)),
    String(Math.round(reverseDuration)),
    String(Math.round(replay.metadata.rate_hz)),
    String(replay.metadata.reverse_notches_per_event),
    String(target.geometry.x + replay.pixel_target.x),
    String(target.geometry.y + replay.pixel_target.y),
  ];
  const damageContext = x11DamageObserverActionContext("wheel");
  if (damageContext) {
    const observed = await runX11DamageObservedAction({
      windowId: target.window_id,
      timeoutMs: Math.max(
        5_000,
        Math.ceil(replay.metadata.scheduled_duration_ms + 5_000),
      ),
      action: {
        type: "wheel",
        forwardDurationMs: Math.round(forwardDuration),
        pauseDurationMs: Math.round(pauseDuration),
        reverseDurationMs: Math.round(reverseDuration),
        rateHz: Math.round(replay.metadata.rate_hz),
        reverseNotches: replay.metadata.reverse_notches_per_event,
        x: target.geometry.x + replay.pixel_target.x,
        y: target.geometry.y + replay.pixel_target.y,
      },
      ...damageContext,
    });
    retainX11DamageObserverSample(observed.sample);
    return observed.duration_ms;
  }
  const binary = await buildDirectXTestHelper();
  const started = process.hrtime.bigint();
  await execFileAsync(binary, args, {
    encoding: "utf8",
    timeout: Math.ceil(replay.metadata.scheduled_duration_ms + 5_000),
    maxBuffer: 64_000,
  });
  return Number(process.hrtime.bigint() - started) / 1e6;
}

export function parseNativeClockSamples(stdout, expectedCount) {
  const samples = stdout
    .trim()
    .split("\n")
    .filter(Boolean)
    .map(parseNativeClockSampleLine);
  return validateNativeClockSampleSequence(samples, expectedCount);
}

export function parseNativeClockSampleLine(line) {
  const fields = line.split("\t");
  if (fields.length !== 4 || fields[3].length === 0) {
    throw new Error("native clock sample line is invalid");
  }
  const [index, scheduled, observed, action] = fields;
  return {
    sample_index: Number(index),
    scheduled_offset_ms: Number(scheduled),
    observed_monotonic_ms: Number(observed),
    action,
  };
}

function validateNativeClockSampleSequence(samples, expectedCount) {
  if (samples.length !== expectedCount) {
    throw new Error(
      `native clock emitted ${samples.length}/${expectedCount} samples`,
    );
  }
  for (let index = 0; index < samples.length; index += 1) {
    const sample = samples[index];
    if (
      sample.sample_index !== index ||
      !Number.isFinite(sample.scheduled_offset_ms) ||
      !Number.isFinite(sample.observed_monotonic_ms) ||
      (index > 0 &&
        sample.observed_monotonic_ms <=
          samples[index - 1].observed_monotonic_ms)
    ) {
      throw new Error(`native clock sample ${index} is invalid`);
    }
  }
  return samples;
}

async function startDirectClock(
  args,
  expectedCount,
  timeoutMs,
  { environment = {}, damageCandidatePid } = {},
) {
  const binary = await buildDirectXTestHelper();
  const feed = {
    samples: [],
    complete: false,
    error: null,
  };
  const child = spawn(binary, args, {
    env: { ...process.env, ...environment },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let stdoutBuffer = "";
  let stderr = "";
  let timedOut = false;
  let settled = false;
  const consumeLines = (final = false) => {
    const lines = stdoutBuffer.split("\n");
    stdoutBuffer = final ? "" : (lines.pop() ?? "");
    for (const line of lines) {
      if (line.length === 0) continue;
      if (line.startsWith("damage\t")) {
        if (feed.damage_sample) {
          throw new Error("native clock emitted duplicate Damage samples");
        }
        feed.damage_sample = parseX11DamageObserverSample(line.slice(7), {
          candidatePid: damageCandidatePid,
        });
        retainX11DamageObserverSample(feed.damage_sample);
        continue;
      }
      const sample = parseNativeClockSampleLine(line);
      const previous = feed.samples.at(-1);
      if (
        sample.sample_index !== feed.samples.length ||
        !Number.isFinite(sample.scheduled_offset_ms) ||
        !Number.isFinite(sample.observed_monotonic_ms) ||
        (previous &&
          sample.observed_monotonic_ms <= previous.observed_monotonic_ms)
      ) {
        throw new Error(
          `native clock sample ${feed.samples.length} is invalid`,
        );
      }
      feed.samples.push(sample);
    }
    if (final && stdoutBuffer.trim().length > 0) {
      throw new Error("native clock ended with an incomplete sample");
    }
  };
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    if (settled) return;
    stdoutBuffer += chunk;
    try {
      consumeLines();
    } catch (error) {
      feed.error = error.message;
      child.kill("SIGTERM");
    }
  });
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  const timeout = setTimeout(
    () => {
      timedOut = true;
      child.kill("SIGTERM");
    },
    Math.ceil(timeoutMs + 5_000),
  );
  const completion = new Promise((resolvePromise, rejectPromise) => {
    const reject = (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      feed.complete = true;
      feed.error = error.message;
      rejectPromise(error);
    };
    child.once("error", reject);
    child.once("close", (code, signal) => {
      if (settled) return;
      try {
        consumeLines(true);
        if (feed.error) throw new Error(feed.error);
        if (timedOut) throw new Error("native clock timed out");
        if (code !== 0 || signal) {
          throw new Error(
            `native clock exited with code ${code ?? "null"} signal ${signal ?? "none"}: ${stderr.trim()}`,
          );
        }
        validateNativeClockSampleSequence(feed.samples, expectedCount);
        settled = true;
        clearTimeout(timeout);
        feed.complete = true;
        resolvePromise(feed.samples);
      } catch (error) {
        reject(error);
      }
    });
  });
  return { feed, completion };
}

async function runDirectClock(args, expectedCount, timeoutMs) {
  const session = await startDirectClock(args, expectedCount, timeoutMs);
  return session.completion;
}

export async function runDirectXTestDynamicWheel(replay, target) {
  const durationMs = replay.metadata.scheduled_duration_ms;
  const forwardDuration =
    replay.metadata.scheduled_reverse_first_ms -
    (durationMs - replay.metadata.scheduled_active_duration_ms);
  const pauseDuration =
    durationMs - replay.metadata.scheduled_active_duration_ms;
  const reverseDuration = durationMs - forwardDuration - pauseDuration;
  const expectedCount = Math.round(
    (durationMs * replay.metadata.rate_hz) / 1000 + 1,
  );
  const args = [
    "dynamic-wheel",
    String(Math.round(forwardDuration)),
    String(Math.round(pauseDuration)),
    String(Math.round(reverseDuration)),
    String(Math.round(replay.metadata.rate_hz)),
    String(replay.metadata.reverse_notches_per_event),
    String(target.geometry.x + replay.pixel_target.x),
    String(target.geometry.y + replay.pixel_target.y),
  ];
  const started = process.hrtime.bigint();
  const trajectory = await runDirectClock(args, expectedCount, durationMs);
  return {
    actual_duration_ms: Number(process.hrtime.bigint() - started) / 1e6,
    trajectory,
  };
}

export function buildDistanceBoundedDynamicWheelPlan({
  command,
  viewportHeightCssPx,
  wheelDeltaCssPx,
}) {
  if (command?.operation !== "viewer.dynamic-fidelity-scroll-path") {
    throw new Error("distance-bounded wheel plan requires dynamic fidelity");
  }
  requirePositive(viewportHeightCssPx, "dynamic viewport height");
  requirePositive(wheelDeltaCssPx, "calibrated wheel delta");
  const requestedDistanceCssPx =
    command.path.forward_viewport_heights * viewportHeightCssPx;
  const forwardEventCount = Math.max(
    1,
    Math.round(requestedDistanceCssPx / wheelDeltaCssPx),
  );
  const scheduledDistanceCssPx = forwardEventCount * wheelDeltaCssPx;
  return Object.freeze({
    viewport_height_css_px: viewportHeightCssPx,
    calibrated_wheel_delta_css_px: wheelDeltaCssPx,
    requested_forward_viewport_heights: command.path.forward_viewport_heights,
    requested_forward_distance_css_px: requestedDistanceCssPx,
    forward_event_count: forwardEventCount,
    reverse_event_count: forwardEventCount,
    scheduled_forward_distance_css_px: scheduledDistanceCssPx,
    maximum_distance_error_css_px: wheelDeltaCssPx / 2,
    distance_error_css_px: scheduledDistanceCssPx - requestedDistanceCssPx,
  });
}

function dynamicCandidateComparability({
  pageGeometry,
  zoomPercent,
  displayScaleFactor,
}) {
  if (Math.abs(zoomPercent - 100) > 1e-9) {
    throw new Error("dynamic fidelity requires fixed 100% zoom");
  }
  if (Math.abs(displayScaleFactor - 1) > 1e-9) {
    throw new Error("dynamic fidelity requires display scale factor 1");
  }
  const pageSize = pageGeometry?.page_size_points;
  const paintedBounds =
    pageGeometry?.painted_outer_page_bounds_at_initial_scroll_window_logical;
  const pixelsPerPoint = {
    x:
      requirePositive(paintedBounds?.width, "painted page width") /
      requirePositive(pageSize?.width, "page point width"),
    y:
      requirePositive(paintedBounds?.height, "painted page height") /
      requirePositive(pageSize?.height, "page point height"),
  };
  if (
    Math.abs(pixelsPerPoint.x - 1) > 0.01 + 1e-9 ||
    Math.abs(pixelsPerPoint.y - 1) > 0.01 + 1e-9
  ) {
    throw new Error(
      `dynamic painted page scale is not comparable: ${pixelsPerPoint.x} x ${pixelsPerPoint.y} px/pt`,
    );
  }
  return Object.freeze({
    zoom_percent: zoomPercent,
    display_scale_factor: displayScaleFactor,
    page_size_points: pageSize,
    painted_outer_page_bounds_at_initial_scroll_window_logical: paintedBounds,
    pixels_per_point: Object.freeze(pixelsPerPoint),
    required_pixels_per_point: 1,
    pixels_per_point_tolerance: 0.01,
  });
}

export function buildHeldDynamicWheelPlan({
  command,
  viewportHeightCssPx,
  wheelDeltaCssPx,
  initialScrollOffsetCssPx,
  viewportBoundsWindowLogical,
  checkpointPageGeometries,
  zoomPercent,
  displayScaleFactor,
}) {
  const distancePlan = buildDistanceBoundedDynamicWheelPlan({
    command,
    viewportHeightCssPx,
    wheelDeltaCssPx,
  });
  requireFinite(initialScrollOffsetCssPx, "initial dynamic scroll offset");
  const viewport = viewportBoundsWindowLogical;
  requireFinite(viewport?.x, "dynamic viewport x");
  requireFinite(viewport?.y, "dynamic viewport y");
  requirePositive(viewport?.width, "dynamic viewport width");
  requirePositive(viewport?.height, "dynamic viewport height");
  if (
    !Array.isArray(checkpointPageGeometries) ||
    checkpointPageGeometries.length !== 3 ||
    !Array.isArray(command?.registered_crops) ||
    command.registered_crops.length !== 3
  ) {
    throw new Error(
      "dynamic held replay requires three checkpoint page geometries",
    );
  }
  const geometryByPage = new Map(
    checkpointPageGeometries.map((geometry) => [
      geometry.page_number,
      geometry,
    ]),
  );
  const viewportBottom = viewport.y + viewport.height;
  const checkpoints = command.registered_crops.map((crop, index) => {
    const pageGeometry = geometryByPage.get(crop.page_number);
    if (!pageGeometry) {
      throw new Error(
        `dynamic checkpoint page ${crop.page_number} has no painted geometry`,
      );
    }
    const candidateComparability = dynamicCandidateComparability({
      pageGeometry,
      zoomPercent,
      displayScaleFactor,
    });
    const mappedAtInitial = mapPdfRectToImagePixels(
      crop.pdf_rect,
      pageGeometry.page_size_points,
      pageGeometry.painted_outer_page_bounds_at_initial_scroll_window_logical,
    );
    const earliestDelta = Math.max(
      0,
      mappedAtInitial.top + mappedAtInitial.height - viewportBottom,
    );
    const latestDelta = mappedAtInitial.top - viewport.y;
    if (latestDelta < earliestDelta - 1e-9) {
      throw new Error(
        `dynamic checkpoint ${crop.crop_id} cannot fit in the viewport`,
      );
    }
    const firstFullyVisibleEvent = Math.ceil(
      (earliestDelta - 1e-9) / wheelDeltaCssPx,
    );
    const lastFullyVisibleEvent = Math.floor(
      (latestDelta + 1e-9) / wheelDeltaCssPx,
    );
    const forwardEventCount = index === 0 ? 0 : firstFullyVisibleEvent;
    const actualDelta = forwardEventCount * wheelDeltaCssPx;
    const selectedTargetDelta =
      index === 0 ? earliestDelta : firstFullyVisibleEvent * wheelDeltaCssPx;
    const offsetError = actualDelta - selectedTargetDelta;
    const fullyVisible =
      forwardEventCount >= firstFullyVisibleEvent &&
      forwardEventCount <= lastFullyVisibleEvent &&
      actualDelta + 1e-9 >= earliestDelta &&
      actualDelta <= latestDelta + 1e-9;
    if (
      !fullyVisible ||
      Math.abs(offsetError) > distancePlan.maximum_distance_error_css_px + 1e-9
    ) {
      throw new Error(
        `dynamic checkpoint ${crop.crop_id} is not fully visible within half a calibrated wheel event`,
      );
    }
    return Object.freeze({
      crop_id: crop.crop_id,
      page_number: crop.page_number,
      forward_event_count: forwardEventCount,
      target_scroll_offset_css_px:
        initialScrollOffsetCssPx + selectedTargetDelta,
      actual_scroll_offset_css_px: initialScrollOffsetCssPx + actualDelta,
      target_forward_delta_css_px: selectedTargetDelta,
      actual_forward_delta_css_px: actualDelta,
      earliest_fully_visible_forward_delta_css_px: earliestDelta,
      latest_fully_visible_forward_delta_css_px: latestDelta,
      first_fully_visible_forward_event_count: firstFullyVisibleEvent,
      last_fully_visible_forward_event_count: lastFullyVisibleEvent,
      offset_error_css_px: offsetError,
      maximum_offset_error_css_px: distancePlan.maximum_distance_error_css_px,
      crop_fully_visible_at_actual_offset: fullyVisible,
      mapped_crop_bounds_at_initial_scroll_window_logical: mappedAtInitial,
      candidate_comparability: candidateComparability,
    });
  });
  if (
    checkpoints[0].forward_event_count !== 0 ||
    checkpoints[1].forward_event_count < 1 ||
    checkpoints[1].forward_event_count >= checkpoints[2].forward_event_count ||
    checkpoints[2].forward_event_count >= distancePlan.forward_event_count
  ) {
    throw new Error(
      "dynamic checkpoint event counts are not strictly ordered inside forward motion",
    );
  }
  const holdDurationMs = command.path.checkpoint_hold_ms;
  const holdIntervals = (holdDurationMs * command.input_rate_hz) / 1000;
  if (!Number.isInteger(holdIntervals) || holdIntervals !== 30) {
    throw new Error("dynamic checkpoint hold must be 30 intervals at 120 Hz");
  }
  const expectedTrajectorySampleCount =
    (command.duration_ms * command.input_rate_hz) / 1000 + 1;
  if (expectedTrajectorySampleCount !== 3841) {
    throw new Error("dynamic held replay must retain 3,841 trajectory samples");
  }
  return Object.freeze({
    ...distancePlan,
    zoom_percent: zoomPercent,
    display_scale_factor: displayScaleFactor,
    initial_scroll_offset_css_px: initialScrollOffsetCssPx,
    viewport_bounds_window_logical: viewport,
    checkpoint_hold_duration_ms: holdDurationMs,
    checkpoint_hold_interval_count: holdIntervals,
    checkpoint_hold_sample_count: holdIntervals + 1,
    expected_trajectory_sample_count: expectedTrajectorySampleCount,
    checkpoints: Object.freeze(checkpoints),
  });
}

export function validateDynamicWheelCalibrationReceipt(ready, receipt) {
  const observedDelta = receipt?.observed_wheel_delta_css_px;
  const readyOffset = ready?.initial_scroll_offset_css_px?.y;
  const initialOffset = receipt?.initial_scroll_offset_css_px;
  const postOffset = receipt?.post_calibration_scroll_offset_css_px;
  if (
    ready?.wheel_calibration_required !== true ||
    ready?.calibrated_wheel_delta_css_px != null ||
    receipt?.candidate_runtime !== "gpui" ||
    receipt?.input_api !== "XTEST-single-wheel-notch" ||
    receipt?.calibration_event_count !== 1 ||
    !Number.isFinite(observedDelta) ||
    observedDelta <= 0 ||
    !Number.isFinite(readyOffset) ||
    !Number.isFinite(initialOffset) ||
    !Number.isFinite(postOffset) ||
    Math.abs(initialOffset - readyOffset) > 0.01 ||
    Math.abs(postOffset - readyOffset) > 0.01 ||
    receipt?.scroll_applied !== false ||
    receipt?.timed_trajectory_excluded !== true
  ) {
    throw new Error(
      "dynamic wheel calibration receipt is missing, inconsistent, applied to scroll, or timed",
    );
  }
  return Object.freeze({
    candidate_runtime: receipt.candidate_runtime,
    input_api: receipt.input_api,
    calibration_event_count: 1,
    observed_wheel_delta_css_px: observedDelta,
    initial_scroll_offset_css_px: initialOffset,
    post_calibration_scroll_offset_css_px: postOffset,
    scroll_applied: false,
    timed_trajectory_excluded: true,
  });
}

export async function runDirectXTestDistanceBoundedDynamicWheel(
  replay,
  target,
  plan,
) {
  const durationMs = replay.metadata.scheduled_duration_ms;
  const forwardDuration =
    replay.metadata.scheduled_reverse_first_ms -
    (durationMs - replay.metadata.scheduled_active_duration_ms);
  const pauseDuration =
    durationMs - replay.metadata.scheduled_active_duration_ms;
  const reverseDuration = durationMs - forwardDuration - pauseDuration;
  const expectedCount = Math.round(
    (durationMs * replay.metadata.rate_hz) / 1000 + 1,
  );
  const args = [
    "dynamic-wheel-distance",
    String(Math.round(forwardDuration)),
    String(Math.round(pauseDuration)),
    String(Math.round(reverseDuration)),
    String(Math.round(replay.metadata.rate_hz)),
    String(plan.forward_event_count),
    String(plan.reverse_event_count),
    String(target.geometry.x + replay.pixel_target.x),
    String(target.geometry.y + replay.pixel_target.y),
  ];
  const started = process.hrtime.bigint();
  const trajectory = await runDirectClock(args, expectedCount, durationMs);
  return {
    actual_duration_ms: Number(process.hrtime.bigint() - started) / 1e6,
    trajectory,
    distance_plan: plan,
  };
}

export async function runDirectXTestWheelCalibration(replay, target) {
  const started = process.hrtime.bigint();
  const samples = await runDirectClock(
    [
      "wheel-calibration",
      String(target.geometry.x + replay.pixel_target.x),
      String(target.geometry.y + replay.pixel_target.y),
    ],
    1,
    0,
  );
  return {
    actual_duration_ms: Number(process.hrtime.bigint() - started) / 1e6,
    injected_samples: samples,
    calibration_event_count: 1,
    timed_trajectory_excluded: true,
  };
}

function directXTestHeldDynamicWheelArguments(replay, target, plan) {
  const durationMs = replay.metadata.scheduled_duration_ms;
  const forwardDuration =
    replay.metadata.scheduled_reverse_first_ms -
    (durationMs - replay.metadata.scheduled_active_duration_ms);
  const pauseDuration =
    durationMs - replay.metadata.scheduled_active_duration_ms;
  const reverseDuration = durationMs - forwardDuration - pauseDuration;
  return {
    durationMs,
    args: [
      "dynamic-wheel-held-distance",
      String(Math.round(forwardDuration)),
      String(Math.round(pauseDuration)),
      String(Math.round(reverseDuration)),
      String(Math.round(replay.metadata.rate_hz)),
      String(plan.forward_event_count),
      String(plan.reverse_event_count),
      String(plan.checkpoints[1].forward_event_count),
      String(plan.checkpoints[2].forward_event_count),
      String(plan.checkpoint_hold_interval_count),
      String(target.geometry.x + replay.pixel_target.x),
      String(target.geometry.y + replay.pixel_target.y),
    ],
  };
}

export async function startDirectXTestHeldDynamicWheel(replay, target, plan) {
  const { durationMs, args } = directXTestHeldDynamicWheelArguments(
    replay,
    target,
    plan,
  );
  const started = process.hrtime.bigint();
  const damageContext = x11DamageObserverActionContext(
    "viewer-dynamic-fidelity:held-wheel",
  );
  const clock = await startDirectClock(
    args,
    plan.expected_trajectory_sample_count,
    durationMs,
    damageContext
      ? {
          environment: {
            BP_X11_DAMAGE_WINDOW: String(target.window_id),
            BP_X11_DAMAGE_ACTION_TOKEN: damageContext.actionToken,
            BP_X11_DAMAGE_ACTION_SEQUENCE: String(damageContext.actionSequence),
          },
          damageCandidatePid: damageContext.candidatePid,
        }
      : {},
  );
  return {
    feed: clock.feed,
    completion: clock.completion.then((trajectory) => ({
      actual_duration_ms: Number(process.hrtime.bigint() - started) / 1e6,
      trajectory,
      held_distance_plan: plan,
    })),
  };
}

export async function runDirectXTestHeldDynamicWheel(replay, target, plan) {
  const session = await startDirectXTestHeldDynamicWheel(replay, target, plan);
  return session.completion;
}

export function heldDynamicTrajectoryPassed(trajectory, plan) {
  if (
    !Array.isArray(trajectory) ||
    trajectory.length !== plan?.expected_trajectory_sample_count ||
    trajectory.length !== 3841
  ) {
    return false;
  }
  const expectedHolds = ["hold-page-1", "hold-page-15", "hold-page-29"];
  let previousEnd = -1;
  for (const action of expectedHolds) {
    const indices = trajectory
      .filter((sample) => sample.action === action)
      .map((sample) => sample.sample_index);
    if (
      indices.length !== plan.checkpoint_hold_sample_count ||
      indices.some(
        (sampleIndex, index) =>
          index > 0 && sampleIndex !== indices[index - 1] + 1,
      ) ||
      indices[0] <= previousEnd
    ) {
      return false;
    }
    previousEnd = indices.at(-1);
  }
  return (
    trajectory[0].action === "hold-page-1" &&
    trajectory.at(-1).scheduled_offset_ms === 32_000
  );
}

export async function startIndependentDynamicObserver({ durationMs, rateHz }) {
  const expectedCount = Math.round((durationMs * rateHz) / 1000 + 1);
  return startDirectClock(
    ["clock", String(Math.round(durationMs)), String(Math.round(rateHz))],
    expectedCount,
    durationMs,
  );
}

export async function runIndependentDynamicObserver({ durationMs, rateHz }) {
  const session = await startIndependentDynamicObserver({ durationMs, rateHz });
  return session.completion;
}

export function bindDynamicObserverSamples(
  observerTicks,
  stateEvents,
  command,
) {
  const states = stateEvents
    .filter(
      (event) =>
        event.event === "dynamic-fidelity-state" &&
        event.command_id === command.id &&
        Number.isFinite(event.runner_observed_monotonic_ms),
    )
    .sort(
      (left, right) =>
        left.runner_observed_monotonic_ms - right.runner_observed_monotonic_ms,
    );
  if (states.length === 0) {
    throw new Error("dynamic fidelity observer has no live application state");
  }
  const firstTickObservedMonotonicMs = observerTicks[0]?.observed_monotonic_ms;
  let stateIndex = 0;
  const samples = observerTicks.map((tick) => {
    while (
      stateIndex + 1 < states.length &&
      states[stateIndex + 1].runner_observed_monotonic_ms <=
        tick.observed_monotonic_ms
    ) {
      stateIndex += 1;
    }
    const state = states[stateIndex];
    if (state.runner_observed_monotonic_ms > tick.observed_monotonic_ms) {
      throw new Error(
        `observer sample ${tick.sample_index} preceded the first live state`,
      );
    }
    const fidelity = measureVisibleRasterFidelity(state.visible_pages);
    const observerTickActualOffsetMs =
      tick.observed_monotonic_ms - firstTickObservedMonotonicMs;
    return {
      sample_index: tick.sample_index,
      scheduled_offset_ms: tick.scheduled_offset_ms,
      observed_monotonic_ms: tick.observed_monotonic_ms,
      observer_tick_actual_offset_ms: observerTickActualOffsetMs,
      observer_tick_schedule_error_ms:
        observerTickActualOffsetMs - tick.scheduled_offset_ms,
      application_state_sequence: state.state_sequence,
      application_state_observed_monotonic_ms:
        state.runner_observed_monotonic_ms,
      application_state_age_ms:
        tick.observed_monotonic_ms - state.runner_observed_monotonic_ms,
      active_page: state.active_page,
      visible_page_count: state.visible_page_count,
      scroll_offset_css_px: state.scroll_offset_css_px,
      render_generation: state.render_generation,
      ...fidelity,
    };
  });
  validateDynamicFidelitySeries(samples, command);
  return samples;
}

function monotonicNowMs() {
  return Number(process.hrtime.bigint()) / 1e6;
}

function latestDynamicState(events, commandId) {
  return [...events]
    .reverse()
    .find(
      (event) =>
        event.event === "dynamic-fidelity-state" &&
        event.command_id === commandId,
    );
}

async function collectDynamicPresentedCaptures({
  command,
  events,
  artifactDirectory,
  heldDistancePlan,
  captureServer,
  helperFeed,
}) {
  if (!artifactDirectory) {
    throw new Error("dynamic fidelity requires a retained artifact directory");
  }
  const pending = new Map(
    command.registered_crops.map((crop) => [crop.crop_id, crop]),
  );
  const captures = [];
  const deadline = Date.now() + command.duration_ms + 3_000;
  while (pending.size > 0 && Date.now() < deadline) {
    for (const [cropId, crop] of pending) {
      const helperHold = dynamicHelperHoldState(helperFeed, crop.page_number);
      if (helperHold.phase === "after") {
        const latestState = latestDynamicState(events, command.id);
        throw dynamicCaptureError(
          `dynamic fidelity ${cropId} crop-not-semantic-ready-during-declared-hold`,
          {
            crop_id: cropId,
            page_number: crop.page_number,
            capture_phase: "semantic-ready",
            helper_hold: helperHold,
            state_receipts: {
              current: dynamicCaptureStateEvidence(
                latestState,
                crop.page_number,
              ),
            },
          },
        );
      }
    }
    const state = [...events]
      .reverse()
      .find(
        (event) =>
          event.event === "dynamic-fidelity-state" &&
          event.command_id === command.id,
      );
    if (!state) {
      await new Promise((resolvePromise) => setTimeout(resolvePromise, 4));
      continue;
    }
    for (const [cropId, crop] of pending) {
      const heldCheckpoint = heldDistancePlan.checkpoints.find(
        ({ crop_id: heldCropId }) => heldCropId === cropId,
      );
      const helperHold = dynamicHelperHoldState(helperFeed, crop.page_number);
      if (helperHold.phase === "before") continue;
      if (helperHold.phase === "after") {
        throw dynamicCaptureError(
          `dynamic fidelity ${cropId} crop-not-semantic-ready-during-declared-hold`,
          {
            crop_id: cropId,
            page_number: crop.page_number,
            capture_phase: "semantic-ready",
            helper_hold: helperHold,
            state_receipts: {
              current: dynamicCaptureStateEvidence(state, crop.page_number),
            },
          },
        );
      }
      if (
        !heldCheckpoint ||
        Math.abs(
          state.scroll_offset_css_px?.y -
            heldCheckpoint.actual_scroll_offset_css_px,
        ) > 0.01
      ) {
        continue;
      }
      const page = state.visible_pages?.find(
        (entry) =>
          entry.page_number === crop.page_number &&
          entry.current_raster_ready_area_fraction === 1 &&
          entry.painted_generation_current === true,
      );
      if (!page) continue;
      const paintedBounds = page.painted_outer_page_bounds_window_logical;
      const logicalCrop = mapPdfRectToImagePixels(
        crop.pdf_rect,
        page.page_size_points,
        paintedBounds,
      );
      const viewport = state.viewport_bounds_window_logical;
      if (
        !viewport ||
        logicalCrop.left < viewport.x ||
        logicalCrop.top < viewport.y ||
        logicalCrop.left + logicalCrop.width > viewport.x + viewport.width ||
        logicalCrop.top + logicalCrop.height > viewport.y + viewport.height
      ) {
        continue;
      }
      const beforePpmPath = resolve(
        artifactDirectory,
        `${cropId}-before-window.ppm`,
      );
      const afterPpmPath = resolve(artifactDirectory, `${cropId}-window.ppm`);
      const beforeStateReceipt = {
        state,
        runner_snapshot_monotonic_ms: monotonicNowMs(),
      };
      const helperHoldBefore = requireDynamicHelperHold(
        helperFeed,
        crop.page_number,
        cropId,
        "before-first-XGetImage",
        {
          before: dynamicCaptureStateEvidence(state, crop.page_number),
          middle: null,
          after: null,
        },
      );
      const beforeCapture = await captureServer.capture(
        `${cropId}:before`,
        beforePpmPath,
      );
      const middleState = latestDynamicState(events, command.id);
      const helperHoldMiddle = requireDynamicHelperHold(
        helperFeed,
        crop.page_number,
        cropId,
        "after-first-XGetImage",
        {
          before: dynamicCaptureStateEvidence(state, crop.page_number),
          middle: dynamicCaptureStateEvidence(middleState, crop.page_number),
          after: null,
        },
      );
      if (!gpuiPaintedCropStateStable(state, middleState, crop.page_number)) {
        throw dynamicCaptureError(
          `dynamic fidelity ${cropId} painted state changed during first XGetImage capture`,
          {
            crop_id: cropId,
            page_number: crop.page_number,
            capture_phase: "after-first-XGetImage",
            helper_hold: helperHoldMiddle,
            state_receipts: {
              before: dynamicCaptureStateEvidence(state, crop.page_number),
              middle: dynamicCaptureStateEvidence(
                middleState,
                crop.page_number,
              ),
              after: null,
            },
          },
        );
      }
      await new Promise((resolvePromise) => setTimeout(resolvePromise, 16));
      requireDynamicHelperHold(
        helperFeed,
        crop.page_number,
        cropId,
        "before-second-XGetImage",
        {
          before: dynamicCaptureStateEvidence(state, crop.page_number),
          middle: dynamicCaptureStateEvidence(middleState, crop.page_number),
          after: null,
        },
      );
      const afterCapture = await captureServer.capture(
        `${cropId}:after`,
        afterPpmPath,
      );
      const afterState = latestDynamicState(events, command.id);
      const afterStateReceipt = {
        state: afterState,
        runner_snapshot_monotonic_ms: monotonicNowMs(),
      };
      const helperHoldAfter = requireDynamicHelperHold(
        helperFeed,
        crop.page_number,
        cropId,
        "after-second-XGetImage",
        {
          before: dynamicCaptureStateEvidence(state, crop.page_number),
          middle: dynamicCaptureStateEvidence(middleState, crop.page_number),
          after: dynamicCaptureStateEvidence(afterState, crop.page_number),
        },
      );
      if (
        !gpuiPaintedCropStateStable(middleState, afterState, crop.page_number)
      ) {
        throw dynamicCaptureError(
          `dynamic fidelity ${cropId} painted state changed during second XGetImage capture`,
          {
            crop_id: cropId,
            page_number: crop.page_number,
            capture_phase: "after-second-XGetImage",
            helper_hold: helperHoldAfter,
            state_receipts: {
              before: dynamicCaptureStateEvidence(state, crop.page_number),
              middle: dynamicCaptureStateEvidence(
                middleState,
                crop.page_number,
              ),
              after: dynamicCaptureStateEvidence(afterState, crop.page_number),
            },
          },
        );
      }
      captures.push({
        crop_id: cropId,
        page_number: crop.page_number,
        crop,
        page,
        before_state_receipt: beforeStateReceipt,
        after_state_receipt: afterStateReceipt,
        middle_state: middleState,
        before_capture: beforeCapture,
        after_capture: afterCapture,
        before_ppm_path: beforePpmPath,
        after_ppm_path: afterPpmPath,
        actual_painted_outer_page_bounds_window_logical: paintedBounds,
        held_checkpoint: heldCheckpoint,
        live_helper_hold_receipts: {
          before: helperHoldBefore,
          middle: helperHoldMiddle,
          after: helperHoldAfter,
        },
      });
      pending.delete(cropId);
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 4));
  }
  if (pending.size > 0) {
    throw new Error(
      `dynamic fidelity did not capture registered crops: ${[...pending.keys()].join(", ")}`,
    );
  }
  return captures;
}

export function validatePresentedCapturePairCorrelation(
  capture,
  trajectory,
  target,
) {
  const beforeState = capture?.before_state_receipt?.state;
  const afterState = capture?.after_state_receipt?.state;
  const pageNumber = capture?.page_number;
  const timing = validatePresentedCapturePairWindowAndHold({
    beforeCapture: capture.before_capture,
    afterCapture: capture.after_capture,
    pageNumber,
    trajectory,
    target,
  });
  if (
    capture.before_state_receipt.runner_snapshot_monotonic_ms >
      capture.before_capture.capture_started_monotonic_ms ||
    capture.after_state_receipt.runner_snapshot_monotonic_ms <
      capture.after_capture.capture_ended_monotonic_ms ||
    !gpuiPaintedCropStateStable(beforeState, afterState, pageNumber)
  ) {
    throw new Error(
      `dynamic fidelity ${capture?.crop_id ?? "unknown"} XGetImage pair is not contained in one stable painted hold`,
    );
  }
  return {
    ...timing,
    same_scroll_offset: true,
    same_painted_bounds: true,
    same_painted_render_generation: true,
    raster_ready_before_and_after: true,
  };
}

export function validatePresentedCapturePairWindowAndHold({
  beforeCapture,
  afterCapture,
  pageNumber,
  trajectory,
  target,
}) {
  const expectedAction = `hold-page-${pageNumber}`;
  const holdSamples = trajectory.filter(
    (sample) => sample.action === expectedAction,
  );
  validatePresentedDrawableCaptureReceipt(beforeCapture, target);
  validatePresentedDrawableCaptureReceipt(afterCapture, target);
  if (
    beforeCapture.window_id !== afterCapture.window_id ||
    beforeCapture.width !== afterCapture.width ||
    beforeCapture.height !== afterCapture.height ||
    beforeCapture.depth !== afterCapture.depth ||
    beforeCapture.capture_ended_monotonic_ms >
      afterCapture.capture_started_monotonic_ms ||
    holdSamples.length === 0 ||
    beforeCapture.capture_started_monotonic_ms <
      holdSamples[0].observed_monotonic_ms ||
    afterCapture.capture_ended_monotonic_ms >
      holdSamples.at(-1).observed_monotonic_ms
  ) {
    throw new Error(
      `presented XGetImage pair for page ${pageNumber} is outside its exact held stream`,
    );
  }
  return {
    source: "XGetImage-presented-client-drawable",
    expected_hold_action: expectedAction,
    hold_first_observed_monotonic_ms: holdSamples[0].observed_monotonic_ms,
    hold_last_observed_monotonic_ms: holdSamples.at(-1).observed_monotonic_ms,
    same_verified_window: true,
    same_window_dimensions: true,
  };
}

export async function losslesslyConvertPresentedPpmToPng(ppmPath, pngPath) {
  const ppmBytes = await readFile(ppmPath);
  const header = ppmBytes
    .subarray(0, Math.min(ppmBytes.length, 128))
    .toString("ascii");
  const match = /^P6\n([1-9]\d*) ([1-9]\d*)\n255\n/.exec(header);
  if (!match) {
    throw new Error(
      "presented-drawable capture is not the exact binary P6 format",
    );
  }
  const width = Number(match[1]);
  const height = Number(match[2]);
  const pixelOffset = Buffer.byteLength(match[0], "ascii");
  const expectedPixelBytes = width * height * 3;
  if (
    !Number.isSafeInteger(expectedPixelBytes) ||
    ppmBytes.length !== pixelOffset + expectedPixelBytes
  ) {
    throw new Error("presented-drawable P6 pixel length is invalid");
  }
  const ppm = {
    data: ppmBytes.subarray(pixelOffset),
    info: { width, height, channels: 3 },
  };
  await sharp(ppm.data, {
    raw: { width, height, channels: 3 },
  })
    .png({ compressionLevel: 9 })
    .toFile(pngPath);
  const png = await sharp(pngPath).raw().toBuffer({ resolveWithObject: true });
  const pixelHash = (bytes) => createHash("sha256").update(bytes).digest("hex");
  if (
    ppm.info.width !== png.info.width ||
    ppm.info.height !== png.info.height ||
    ppm.info.channels !== png.info.channels ||
    pixelHash(ppm.data) !== pixelHash(png.data)
  ) {
    throw new Error(
      "PPM to PNG presented-drawable conversion was not lossless",
    );
  }
  const ppmStat = await stat(ppmPath);
  const pngStat = await stat(pngPath);
  return {
    conversion: "sharp-ppm-to-lossless-png-after-timed-stream",
    resampled: false,
    width: ppm.info.width,
    height: ppm.info.height,
    channels: ppm.info.channels,
    pixel_sha256: pixelHash(ppm.data),
    ppm_bytes: ppmStat.size,
    png_bytes: pngStat.size,
    ppm_sha256: createHash("sha256").update(ppmBytes).digest("hex"),
    png_sha256: createHash("sha256")
      .update(await readFile(pngPath))
      .digest("hex"),
    lossless_conversion_verified: true,
  };
}

const longbridgeCompatSinglePageCrop = Object.freeze({
  fixture_id: "bp-single-page-v1",
  fixture_sha256:
    "f31adeeb3f17ef180012fe707cb2f2650854305dab4b16bba34d73652b6d8fdc",
  registration_sha256:
    "cc231d7d5da2ef403509e58565a19fb1855fea3da6aca1436d56dbc38ce218ef",
  command_id: "small:open-settle",
  native_command_id: "viewer:open-each",
  crop_id: "single-registration",
  page_id: "bp-single-page-v1:page:001",
  page_number: 1,
  page_size_points: Object.freeze({ width: 612, height: 792 }),
  pdf_rect: Object.freeze({ x: 36, y: 36, width: 540, height: 720 }),
  reference_dpi: 144,
});

function sha256Bytes(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function requireUnusedCompatArtifacts(paths) {
  for (const path of paths) {
    try {
      await stat(path);
      throw new Error(`compatibility crop artifact already exists: ${path}`);
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }
  }
}

function requireCompatPaintedState(state, target, label) {
  const contract = longbridgeCompatSinglePageCrop;
  if (
    state?.command_id !== contract.native_command_id ||
    state?.comparison_command_id !== contract.command_id ||
    state?.fixture_id !== contract.fixture_id ||
    state?.page_id !== contract.page_id
  ) {
    throw new Error(`${label} is not bound to the frozen compatibility crop`);
  }
  if (
    state?.page_size_points?.width !== contract.page_size_points.width ||
    state?.page_size_points?.height !== contract.page_size_points.height
  ) {
    throw new Error(`${label} page size drifted from the frozen crop`);
  }
  const bounds = state?.painted_outer_page_bounds_window_logical;
  const logical = state?.window_logical_size;
  for (const [name, value] of Object.entries({
    "page bounds x": bounds?.x,
    "page bounds y": bounds?.y,
    "page bounds width": bounds?.width,
    "page bounds height": bounds?.height,
    "logical window width": logical?.width,
    "logical window height": logical?.height,
    "display scale factor": state?.display_scale_factor,
    "rendered device pixel ratio": state?.rendered_device_pixel_ratio,
  })) {
    if (!Number.isFinite(value)) {
      throw new Error(`${label} ${name} is missing or non-finite`);
    }
  }
  if (
    bounds.width <= 0 ||
    bounds.height <= 0 ||
    logical.width <= 0 ||
    logical.height <= 0 ||
    state.display_scale_factor <= 0 ||
    state.rendered_device_pixel_ratio < 1
  ) {
    throw new Error(`${label} painted bounds or scale is invalid`);
  }
  if (
    !Number.isInteger(state?.painted_render_generation) ||
    state.painted_render_generation < 1 ||
    !Number.isInteger(state?.painted_state_sequence) ||
    state.painted_state_sequence < 1 ||
    !Number.isFinite(state?.runner_observed_monotonic_ms)
  ) {
    throw new Error(`${label} lacks a stable painted generation receipt`);
  }
  const scaleX = target?.geometry?.width / logical.width;
  const scaleY = target?.geometry?.height / logical.height;
  const epsilon = 1e-9;
  if (
    !Number.isFinite(scaleX) ||
    !Number.isFinite(scaleY) ||
    Math.abs(scaleX - scaleY) > epsilon ||
    Math.abs(scaleX - state.display_scale_factor) > epsilon
  ) {
    throw new Error(
      `${label} display scale does not map the logical page to the exact X11 client`,
    );
  }
  const pageBoundsPixels = {
    x: bounds.x * scaleX,
    y: bounds.y * scaleY,
    width: bounds.width * scaleX,
    height: bounds.height * scaleY,
  };
  if (
    pageBoundsPixels.x < 0 ||
    pageBoundsPixels.y < 0 ||
    pageBoundsPixels.x + pageBoundsPixels.width > target.geometry.width ||
    pageBoundsPixels.y + pageBoundsPixels.height > target.geometry.height
  ) {
    throw new Error(`${label} painted page lies outside the X11 client`);
  }
  return { bounds, logical, pageBoundsPixels };
}

function requireSameCompatPaintedState(before, after, captureReceipt) {
  if (
    after?.event !== "viewer-native-presented-state" ||
    after.runner_observed_monotonic_ms <
      captureReceipt.capture_ended_monotonic_ms
  ) {
    throw new Error(
      "post-capture painted state did not observe the completed XGetImage capture",
    );
  }
  for (const field of [
    "fixture_id",
    "page_id",
    "painted_render_generation",
    "painted_state_sequence",
    "display_scale_factor",
    "rendered_device_pixel_ratio",
  ]) {
    if (after[field] !== before[field]) {
      throw new Error(`compatibility crop ${field} drifted during capture`);
    }
  }
  for (const field of [
    "page_size_points",
    "painted_outer_page_bounds_window_logical",
    "window_logical_size",
  ]) {
    if (JSON.stringify(after[field]) !== JSON.stringify(before[field])) {
      throw new Error(`compatibility crop ${field} drifted during capture`);
    }
  }
}

/**
 * Captures independent driver evidence. The caller must retain this receipt
 * separately from application stdout and supply it to the compatibility
 * validator as native-runner evidence.
 */
export async function captureLongbridgeCompatPresentedCrop({
  openEvidence,
  waitForPostCaptureEvidence,
  target,
  artifactDirectory,
  fixturePath,
  registrationPath,
  captureServer,
  renderReference = renderReferenceCrop,
}) {
  if (!artifactDirectory) {
    throw new Error(
      "compatibility crop requires a retained artifact directory",
    );
  }
  if (typeof waitForPostCaptureEvidence !== "function") {
    throw new Error("compatibility crop requires a post-capture painted state");
  }
  if (!captureServer || typeof captureServer.capture !== "function") {
    throw new Error("compatibility crop requires the XGetImage capture server");
  }
  const before = requireCompatPaintedState(
    openEvidence,
    target,
    "open evidence",
  );
  const contract = longbridgeCompatSinglePageCrop;
  const [fixtureBytes, registrationBytes] = await Promise.all([
    readFile(fixturePath),
    readFile(registrationPath),
  ]);
  if (sha256Bytes(fixtureBytes) !== contract.fixture_sha256) {
    throw new Error("compatibility fixture bytes drifted");
  }
  if (sha256Bytes(registrationBytes) !== contract.registration_sha256) {
    throw new Error("compatibility crop registration bytes drifted");
  }

  const ppmPath = resolve(
    artifactDirectory,
    "compat-single-registration-window.ppm",
  );
  const screenshotPath = resolve(
    artifactDirectory,
    "compat-single-registration-window.png",
  );
  const referencePath = resolve(
    artifactDirectory,
    "compat-single-registration-reference-144dpi.png",
  );
  const candidatePath = resolve(
    artifactDirectory,
    "compat-single-registration-candidate.png",
  );
  const registeredReferencePath = resolve(
    artifactDirectory,
    "compat-single-registration-reference-registered.png",
  );
  await requireUnusedCompatArtifacts([
    ppmPath,
    screenshotPath,
    referencePath,
    candidatePath,
    registeredReferencePath,
  ]);

  const reference = await renderReference({
    pdfPath: fixturePath,
    pageNumber: contract.page_number,
    pdfRect: contract.pdf_rect,
    dpi: contract.reference_dpi,
    outputPath: referencePath,
  });
  if (
    reference?.page_number !== contract.page_number ||
    reference?.page_size_points?.width !== contract.page_size_points.width ||
    reference?.page_size_points?.height !== contract.page_size_points.height ||
    reference?.dpi !== contract.reference_dpi
  ) {
    throw new Error("compatibility registered reference geometry drifted");
  }

  const captureReceipt = await captureServer.capture(
    "compat-single-registration",
    ppmPath,
  );
  validatePresentedDrawableCaptureReceipt(captureReceipt, target, {
    captureId: "compat-single-registration",
    ppmPath,
  });
  const retainedCaptureSha256 = sha256Bytes(await readFile(ppmPath));
  if (retainedCaptureSha256 !== captureReceipt.artifact_sha256) {
    throw new Error(
      "retained XGetImage artifact hash does not match the capture helper receipt",
    );
  }
  if (
    captureReceipt.capture_started_monotonic_ms <
    openEvidence.runner_observed_monotonic_ms
  ) {
    throw new Error("XGetImage capture preceded the frozen painted state");
  }
  const postCaptureEvidence = await waitForPostCaptureEvidence({
    captureReceipt,
    openEvidence,
  });
  requireCompatPaintedState(
    postCaptureEvidence,
    target,
    "post-capture evidence",
  );
  requireSameCompatPaintedState(
    openEvidence,
    postCaptureEvidence,
    captureReceipt,
  );

  const conversion = await losslesslyConvertPresentedPpmToPng(
    ppmPath,
    screenshotPath,
  );
  if (conversion.ppm_sha256 !== captureReceipt.artifact_sha256) {
    throw new Error("XGetImage artifact changed after receipt verification");
  }
  const comparison = await registerAndComparePresentedCropV2({
    screenshotPath,
    pageBoundsPx: before.pageBoundsPixels,
    pageSizePt: contract.page_size_points,
    pdfRect: contract.pdf_rect,
    referencePath,
    outputCandidatePath: candidatePath,
    outputRegisteredReferencePath: registeredReferencePath,
  });
  if (comparison.metric?.passed !== true) {
    throw new Error("compatibility presented crop failed registered fidelity");
  }
  const mappedPixelBounds = {
    x: comparison.mapped_bounds_pixels.left,
    y: comparison.mapped_bounds_pixels.top,
    width: comparison.mapped_bounds_pixels.width,
    height: comparison.mapped_bounds_pixels.height,
  };
  const extractedPixelBounds = {
    x: comparison.extracted_bounds_pixels.left,
    y: comparison.extracted_bounds_pixels.top,
    width: comparison.extracted_bounds_pixels.width,
    height: comparison.extracted_bounds_pixels.height,
  };
  return Object.freeze({
    event: "compat-presented-crop-evidence",
    command_id: contract.command_id,
    fixture_id: contract.fixture_id,
    crop_id: contract.crop_id,
    page_id: contract.page_id,
    registration_sha256: contract.registration_sha256,
    acceptance_source: "XGetImage-presented-client-drawable",
    candidate_resampled: false,
    presented_drawable_artifact_sha256: captureReceipt.artifact_sha256,
    retained_ppm_sha256: conversion.ppm_sha256,
    candidate_crop_sha256: comparison.candidate_crop_sha256,
    registered_reference_crop_sha256:
      comparison.registered_reference_crop_sha256,
    candidate_dimensions: comparison.candidate_dimensions,
    mapped_bounds_pixels: mappedPixelBounds,
    extracted_bounds_pixels: extractedPixelBounds,
    page_size_points: contract.page_size_points,
    pdf_rect: contract.pdf_rect,
    rendered_device_pixel_ratio: openEvidence.rendered_device_pixel_ratio,
    display_scale_factor: openEvidence.display_scale_factor,
    painted_render_generation: openEvidence.painted_render_generation,
    painted_generation_stable: true,
    exact_pixel_match:
      comparison.candidate_crop_sha256 ===
      comparison.registered_reference_crop_sha256,
    metric: comparison.metric,
    driver_capture: Object.freeze({
      event: captureReceipt.event,
      capture_id: captureReceipt.capture_id,
      capture_started_monotonic_ms: captureReceipt.capture_started_monotonic_ms,
      capture_ended_monotonic_ms: captureReceipt.capture_ended_monotonic_ms,
      window_id: captureReceipt.window_id,
      width: captureReceipt.width,
      height: captureReceipt.height,
      depth: captureReceipt.depth,
      source: captureReceipt.source,
      artifact_sha256: captureReceipt.artifact_sha256,
    }),
    lossless_conversion: conversion,
    screenshot_sha256: comparison.screenshot_sha256,
    candidate_crop_path: candidatePath,
    registered_reference_path: registeredReferencePath,
  });
}

async function convertAndCompareDynamicPresentedCaptures({
  captures,
  command,
  artifactDirectory,
  trajectory,
  target,
}) {
  const receipts = [];
  for (const capture of captures) {
    const crop = capture.crop;
    const cropId = crop.crop_id;
    const beforeScreenshotPath = resolve(
      artifactDirectory,
      `${cropId}-before-window.png`,
    );
    const screenshotPath = resolve(artifactDirectory, `${cropId}-window.png`);
    const beforeCandidatePath = resolve(
      artifactDirectory,
      `${cropId}-before-native-candidate.png`,
    );
    const candidatePath = resolve(
      artifactDirectory,
      `${cropId}-native-candidate.png`,
    );
    const beforeRegisteredReferencePath = resolve(
      artifactDirectory,
      `${cropId}-before-reference-registered.png`,
    );
    const registeredReferencePath = resolve(
      artifactDirectory,
      `${cropId}-reference-registered.png`,
    );
    const beforeConversion = await losslesslyConvertPresentedPpmToPng(
      capture.before_ppm_path,
      beforeScreenshotPath,
    );
    const afterConversion = await losslesslyConvertPresentedPpmToPng(
      capture.after_ppm_path,
      screenshotPath,
    );
    const correlation = validatePresentedCapturePairCorrelation(
      capture,
      trajectory,
      target,
    );
    const referencePath = resolve(
      performanceDirectory,
      "fixtures/reference-crops-v5",
      `${cropId}.png`,
    );
    const beforeComparison = await registerAndComparePresentedCropV2({
      screenshotPath: beforeScreenshotPath,
      pageBoundsPx: capture.actual_painted_outer_page_bounds_window_logical,
      pageSizePt: capture.page.page_size_points,
      pdfRect: crop.pdf_rect,
      referencePath,
      outputCandidatePath: beforeCandidatePath,
      outputRegisteredReferencePath: beforeRegisteredReferencePath,
    });
    const comparison = await registerAndComparePresentedCropV2({
      screenshotPath,
      pageBoundsPx: capture.actual_painted_outer_page_bounds_window_logical,
      pageSizePt: capture.page.page_size_points,
      pdfRect: crop.pdf_rect,
      referencePath,
      outputCandidatePath: candidatePath,
      outputRegisteredReferencePath: registeredReferencePath,
    });
    const candidateCropUnchanged =
      beforeComparison.candidate_crop_sha256 ===
      comparison.candidate_crop_sha256;
    const passed =
      comparison.reference_crop_sha256 ===
        crop.reference_raster.reference_crop_sha256 &&
      comparison.metric.passed === true &&
      candidateCropUnchanged;
    receipts.push({
      crop_id: cropId,
      page_number: crop.page_number,
      before_painted_state_sequence:
        capture.before_state_receipt.state.painted_state_sequence,
      middle_painted_state_sequence:
        capture.middle_state.painted_state_sequence,
      after_painted_state_sequence:
        capture.after_state_receipt.state.painted_state_sequence,
      painted_render_generation: capture.page.painted_render_generation,
      painted_generation_stable: true,
      actual_painted_outer_page_bounds_window_logical:
        capture.actual_painted_outer_page_bounds_window_logical,
      candidate_comparability: capture.held_checkpoint.candidate_comparability,
      held_checkpoint: capture.held_checkpoint,
      live_helper_hold_receipts: capture.live_helper_hold_receipts,
      selection: crop.checkpoint.selection,
      before_presented_capture: capture.before_capture,
      after_presented_capture: capture.after_capture,
      presented_capture_correlation: correlation,
      before_lossless_conversion: beforeConversion,
      after_lossless_conversion: afterConversion,
      before_ppm_path: capture.before_ppm_path,
      ppm_path: capture.after_ppm_path,
      before_screenshot_path: beforeScreenshotPath,
      screenshot_path: screenshotPath,
      before_candidate_crop_path: beforeCandidatePath,
      candidate_crop_path: candidatePath,
      registered_reference_path: registeredReferencePath,
      acceptance_source: "XGetImage-presented-client-drawable",
      candidate_resampled: false,
      candidate_crop_unchanged: candidateCropUnchanged,
      before_candidate_crop_sha256: beforeComparison.candidate_crop_sha256,
      ...comparison,
      passed,
    });
  }
  if (!registeredDynamicCropsPassed(receipts, command)) {
    throw new Error(
      `dynamic fidelity registered crop v2 metric failed: ${receipts
        .filter((receipt) => receipt.passed !== true)
        .map((receipt) => receipt.crop_id)
        .join(", ")}`,
    );
  }
  return receipts;
}

export function gpuiPaintedCropStateStable(before, after, pageNumber) {
  const beforePage = before?.visible_pages?.find(
    (page) => page.page_number === pageNumber,
  );
  const afterPage = after?.visible_pages?.find(
    (page) => page.page_number === pageNumber,
  );
  if (!beforePage || !afterPage) return false;
  return (
    before.scroll_offset_css_px?.x === after.scroll_offset_css_px?.x &&
    before.scroll_offset_css_px?.y === after.scroll_offset_css_px?.y &&
    JSON.stringify(beforePage.painted_outer_page_bounds_window_logical) ===
      JSON.stringify(afterPage.painted_outer_page_bounds_window_logical) &&
    beforePage.current_raster_ready_area_fraction === 1 &&
    afterPage.current_raster_ready_area_fraction === 1 &&
    beforePage.painted_generation_current === true &&
    afterPage.painted_generation_current === true &&
    beforePage.painted_render_generation === afterPage.painted_render_generation
  );
}

export function dynamicCaptureSemanticTuple(state, pageNumber) {
  const page = state?.visible_pages?.find(
    (entry) => entry.page_number === pageNumber,
  );
  if (!state || !page) return null;
  return {
    scroll_offset_css_px: state.scroll_offset_css_px,
    painted_outer_page_bounds_window_logical:
      page.painted_outer_page_bounds_window_logical,
    current_raster_ready_area_fraction: page.current_raster_ready_area_fraction,
    painted_generation_current: page.painted_generation_current,
    painted_render_generation: page.painted_render_generation,
  };
}

export function dynamicCaptureStateEvidence(state, pageNumber) {
  if (!state) return null;
  return {
    state_sequence: state.state_sequence ?? null,
    painted_state_sequence: state.painted_state_sequence ?? null,
    runner_observed_monotonic_ms: state.runner_observed_monotonic_ms ?? null,
    semantic_tuple: dynamicCaptureSemanticTuple(state, pageNumber),
  };
}

export function dynamicHelperHoldState(feed, pageNumber) {
  const expectedAction = `hold-page-${pageNumber}`;
  const samples = Array.isArray(feed?.samples) ? feed.samples : [];
  const holdSamples = samples.filter(
    (sample) => sample.action === expectedAction,
  );
  const latestSample = samples.at(-1) ?? null;
  const firstHoldSample = holdSamples[0] ?? null;
  const latestHoldSample = holdSamples.at(-1) ?? null;
  let phase = "before";
  if (latestSample?.action === expectedAction) phase = "inside";
  else if (
    latestHoldSample &&
    latestSample?.sample_index > latestHoldSample.sample_index
  ) {
    phase = "after";
  }
  return {
    expected_action: expectedAction,
    phase,
    first_hold_sample: firstHoldSample,
    latest_hold_sample: latestHoldSample,
    latest_sample: latestSample,
    helper_complete: feed?.complete === true,
    helper_error: feed?.error ?? null,
  };
}

export function dynamicCaptureError(message, evidence) {
  const error = new Error(message);
  error.dynamic_capture_failure_evidence = evidence;
  return error;
}

export function requireDynamicHelperHold(
  feed,
  pageNumber,
  cropId,
  phaseLabel,
  stateReceipts = null,
) {
  const helperHold = dynamicHelperHoldState(feed, pageNumber);
  if (helperHold.phase === "inside") return helperHold;
  const message =
    helperHold.phase === "after" && phaseLabel === "semantic-ready"
      ? `dynamic fidelity ${cropId} crop-not-semantic-ready-during-declared-hold`
      : `dynamic fidelity ${cropId} capture left declared hold during ${phaseLabel}`;
  throw dynamicCaptureError(message, {
    crop_id: cropId,
    page_number: pageNumber,
    capture_phase: phaseLabel,
    helper_hold: helperHold,
    state_receipts: stateReceipts,
  });
}

export async function retainDynamicCaptureFailureEvidence(
  artifactDirectory,
  error,
) {
  const evidence = error?.dynamic_capture_failure_evidence;
  if (!artifactDirectory || !evidence) return null;
  const path = resolve(
    artifactDirectory,
    "dynamic-fidelity-capture-failure.json",
  );
  const receipt = {
    schema_version: 1,
    status: "failed",
    error: error.message,
    ...evidence,
  };
  await writeFile(path, `${JSON.stringify(receipt, null, 2)}\n`, {
    encoding: "utf8",
    flag: "wx",
  });
  return { path, ...receipt };
}

export function registeredDynamicCropsPassed(receipts, command) {
  if (
    !Array.isArray(receipts) ||
    receipts.length !== command?.registered_crops?.length ||
    receipts.length !== 3
  ) {
    return false;
  }
  const expected = new Map(
    command.registered_crops.map((crop) => [crop.crop_id, crop]),
  );
  return receipts.every((receipt) => {
    const crop = expected.get(receipt.crop_id);
    return (
      crop &&
      receipt.passed === true &&
      receipt.metric?.algorithm === "bp-cross-engine-binary-scan-fidelity-v2" &&
      receipt.metric.passed === true &&
      receipt.candidate_resampled === false &&
      receipt.reference_resampling === "downsample-only-lanczos3" &&
      receipt.candidate_crop_unchanged === true &&
      receipt.reference_crop_sha256 ===
        crop.reference_raster.reference_crop_sha256
    );
  });
}

export const dynamicRunnerResultFile = "dynamic-fidelity-runner-result.json";

async function writeDynamicRunnerResult(
  artifactDirectory,
  { status, commandId, error = null },
) {
  if (!artifactDirectory) {
    throw new Error("dynamic runner result requires an artifact directory");
  }
  const path = resolve(artifactDirectory, dynamicRunnerResultFile);
  const receipt = {
    schema_version: 1,
    command_id: commandId,
    status,
    crop_source: "XGetImage-presented-client-drawable",
    error,
  };
  await writeFile(path, `${JSON.stringify(receipt)}\n`, {
    encoding: "utf8",
    flag: "wx",
  });
  return { path, ...receipt };
}

/**
 * Replays only OS/window-system input. The application remains responsible
 * for emitting native-input-ready geometry and canonical comparison
 * milestones. Absence of either fails closed rather than falling back to the
 * in-process semantic driver.
 */
export async function executeNativeX11Scenario({
  pid,
  scenario,
  contract,
  workload,
  events,
  artifactDirectory = null,
  timeoutMs = 10_000,
  locateWindow = locateExactButterPaperWindow,
  execute = runXdotool,
  executeCausalClick = runDamageObservedXTestClick,
  damageObservationEnabled = execute === runXdotool,
  compatPresentedCrop = null,
  startCaptureServer = startPresentedDrawableCaptureServer,
  signalProcess = process.kill,
}) {
  if (process.platform !== "linux") {
    throw new Error("native-x11-xtest is available only on Linux/X11");
  }
  if (!process.env.DISPLAY)
    throw new Error("native-x11-xtest requires DISPLAY");
  if (typeof scenario !== "string" || scenario.length === 0) {
    throw new Error("native-x11-xtest requires an explicit scenario");
  }
  let target = await locateWindow(pid, { timeoutMs });
  const shellReady = await waitForCommandEvent(
    events,
    "native-viewer-shell-ready",
    "viewer:launch-cold",
    timeoutMs,
  );
  target = validateGpuiLogicalWindowTarget(target, shellReady);
  const shellReplay = buildNativeViewerShellReplay(shellReady, target);
  if (damageObservationEnabled && scenario === "open-pdf") {
    await executeCausalClick(shellReplay, target, "open-pdf:open-control");
  } else {
    await execute(shellReplay.args, shellReplay.metadata.scheduled_duration_ms);
  }
  const launchEvidence = await waitForCommandEvent(
    events,
    "viewer-native-launch-evidence",
    "viewer:launch-cold",
    timeoutMs,
  );
  const multiDocument = scenario === "multi-document-session";
  const openEvidence = multiDocument
    ? null
    : await waitForCommandEvent(
        events,
        "viewer-native-open-evidence",
        "viewer:open-each",
        timeoutMs,
      );
  const viewerEvidence = multiDocument
    ? {
        success:
          launchEvidence.native_input_observed === true &&
          launchEvidence.gpui_platform_draw_submitted === true &&
          launchEvidence.physical_scanout_observed === false &&
          launchEvidence.input_latency_samples_after >
            launchEvidence.input_latency_samples_before,
        launch: launchEvidence,
        open: null,
        gpui_platform_draw_submitted: true,
        physical_scanout_observed: false,
      }
    : validateNativeViewerLaunchOpenEvidence([launchEvidence, openEvidence]);
  const driverEvidence = {};
  if (compatPresentedCrop) {
    if (multiDocument || !openEvidence) {
      throw new Error(
        "compatibility presented-crop capture requires one opened document",
      );
    }
    let captureServer;
    try {
      captureServer = await startCaptureServer(target);
      driverEvidence.compat_presented_crop =
        await captureLongbridgeCompatPresentedCrop({
          openEvidence,
          waitForPostCaptureEvidence: ({ captureReceipt }) => {
            if (
              signalProcess(
                pid,
                longbridgeCompatCaptureSignalProtocol.signal,
              ) !== true
            ) {
              throw new Error(
                "native driver could not signal the post-capture GPUI frame",
              );
            }
            return waitForMatchingEvent(
              events,
              (event) =>
                event.event === "viewer-native-presented-state" &&
                event.command_id === "viewer:open-each" &&
                Number.isFinite(event.runner_observed_monotonic_ms) &&
                event.runner_observed_monotonic_ms >=
                  captureReceipt.capture_ended_monotonic_ms,
              "viewer:open-each post-capture painted state",
              timeoutMs,
            );
          },
          target,
          artifactDirectory,
          fixturePath: compatPresentedCrop.fixturePath,
          registrationPath: compatPresentedCrop.registrationPath,
          captureServer,
          renderReference: compatPresentedCrop.renderReference,
        });
    } finally {
      if (captureServer) {
        await closePresentedDrawableCaptureServerStrict(captureServer);
      }
    }
  }
  if (scenario === "multi-document-session") {
    const commandResults = [];
    for (const command of contract.commands) {
      if (command.id === "session:open-four-fixtures") {
        const receipt = await waitForCommandEvent(
          events,
          "multi-document-command-evidence",
          command.id,
          timeoutMs,
        );
        commandResults.push({
          command_id: command.id,
          execution: "semantic-ordered-open",
          application_receipt: receipt,
          timing: { within_tolerance: true },
        });
        continue;
      }
      if (command.id === "session:switch-four-fixtures") {
        const stages = [];
        for (
          let index = 0;
          index < command.switch_sequence.length;
          index += 1
        ) {
          const stage = `switch-${index}`;
          const ready = await waitForReadyEvent(
            events,
            command.id,
            timeoutMs,
            stage,
          );
          const replay = buildClickReplay({
            windowId: target.window_id,
            point: ready.control.point,
            logicalSize: ready.control.window_logical_size,
            windowGeometry: target.geometry,
          });
          const actualDurationMs =
            execute === runXdotool
              ? await runDamageObservedXTestClick(
                  replay,
                  target,
                  `${command.id}:${stage}`,
                )
              : await execute(
                  replay.args,
                  replay.metadata.scheduled_duration_ms,
                );
          const frame = await waitForMatchingEvent(
            events,
            (event) =>
              event.event === "multi-document-native-frame-evidence" &&
              event.command_id === command.id &&
              event.action_index === index,
            `${command.id} native frame ${index}`,
            timeoutMs,
          );
          stages.push({
            stage,
            fixture_id: command.switch_sequence[index],
            ready_event_t_ms: ready.t_ms,
            actual_duration_ms: actualDurationMs,
            replay: replay.metadata,
            application_frame_receipt: frame,
            timing: { within_tolerance: true },
          });
        }
        const receipt = await waitForCommandEvent(
          events,
          "multi-document-command-evidence",
          command.id,
          timeoutMs,
        );
        commandResults.push({
          command_id: command.id,
          stages,
          application_receipt: receipt,
          timing: { within_tolerance: stages.length === 4 },
        });
        continue;
      }
      if (command.id === "session:edit-dense-rectangle") {
        const ready = await waitForReadyEvent(
          events,
          command.id,
          timeoutMs,
          "property-stroke-width-4pt",
        );
        const replay = buildClickReplay({
          windowId: target.window_id,
          point: ready.control.point,
          logicalSize: ready.control.window_logical_size,
          windowGeometry: target.geometry,
        });
        const actualDurationMs =
          execute === runXdotool
            ? await runDamageObservedXTestClick(
                replay,
                target,
                `${command.id}:property-edit`,
              )
            : await execute(replay.args, replay.metadata.scheduled_duration_ms);
        const frame = await waitForMatchingEvent(
          events,
          (event) =>
            event.event === "multi-document-native-frame-evidence" &&
            event.command_id === command.id &&
            event.action_index === 0,
          `${command.id} native frame`,
          timeoutMs,
        );
        const receipt = await waitForCommandEvent(
          events,
          "multi-document-command-evidence",
          command.id,
          timeoutMs,
        );
        commandResults.push({
          command_id: command.id,
          actual_duration_ms: actualDurationMs,
          replay: replay.metadata,
          application_frame_receipt: frame,
          application_receipt: receipt,
          timing: { within_tolerance: true },
        });
        continue;
      }
      const receipt = await waitForCommandEvent(
        events,
        "multi-document-command-evidence",
        command.id,
        timeoutMs,
      );
      commandResults.push({
        command_id: command.id,
        execution: "semantic-close-with-live-release-receipts",
        application_receipt: receipt,
        timing: { within_tolerance: true },
      });
    }
    return {
      success:
        viewerEvidence.success &&
        commandResults.every(({ timing }) => timing.within_tolerance),
      target_verified: true,
      target,
      viewer: {
        shell_replay: shellReplay.metadata,
        ...viewerEvidence,
      },
      driver_evidence: driverEvidence,
      commands: commandResults,
    };
  }
  const commandResults = [];
  for (const command of contract?.commands ?? []) {
    if (command.operation === "annotation.property-edit-undo-native") {
      const stages = [];
      for (const stage of ["properties-trigger", "property-stroke-width-4pt"]) {
        const ready = await waitForReadyEvent(
          events,
          command.id,
          timeoutMs,
          stage,
        );
        const replay = buildClickReplay({
          windowId: target.window_id,
          point: ready.control.point,
          logicalSize: ready.control.window_logical_size,
          windowGeometry: target.geometry,
        });
        const executedReplay =
          execute === runXdotool ? buildDirectClickReplay(replay) : replay;
        const actualDurationMs =
          execute === runXdotool
            ? await runDirectXTestPointer(executedReplay, target)
            : await execute(
                executedReplay.args,
                executedReplay.metadata.scheduled_duration_ms,
              );
        stages.push({
          stage,
          ready_event_t_ms: ready.t_ms,
          actual_duration_ms: actualDurationMs,
          replay: executedReplay.metadata,
          timing: assessReplayTiming(executedReplay, actualDurationMs),
        });
      }
      const presentation = await waitForCommandEvent(
        events,
        "native-v5-property-presentation-evidence",
        command.id,
        timeoutMs,
      );
      const undoReady = await waitForReadyEvent(
        events,
        command.id,
        timeoutMs,
        "undo-shortcut",
      );
      const undoDurationMs = await execute(["key", "ctrl+z"], 0);
      const applicationReceipt = await waitForCommandEvent(
        events,
        "native-v5-property-application-evidence",
        command.id,
        timeoutMs,
      );
      commandResults.push({
        command_id: command.id,
        stages,
        property_presentation_receipt: presentation,
        undo_ready_event_t_ms: undoReady.t_ms,
        undo_duration_ms: undoDurationMs,
        application_receipt: applicationReceipt,
        execution: "direct-XTEST-clicks-and-xdotool-XTEST-key",
        timing: {
          within_tolerance: stages.every(
            ({ timing }) => timing.within_tolerance,
          ),
        },
      });
      continue;
    }
    if (command.operation === "annotation.snap-transform-native") {
      const ready = await waitForReadyEvent(events, command.id, timeoutMs);
      const replay = buildNativeV5SnapReplay(command, ready, target);
      const observedReplay =
        execute === runXdotool
          ? await runDirectXTestPointerObserved(replay, target)
          : {
              duration_ms: await execute(
                replay.args,
                replay.metadata.scheduled_duration_ms,
              ),
              injected_samples: replay.pixel_samples.map((sample, index) => ({
                sample_index: index,
                scheduled_offset_ms: sample.scheduled_ms,
                observed_monotonic_ms: sample.scheduled_ms,
                action: index === 0 ? "down" : "move",
              })),
            };
      const actualDurationMs = observedReplay.duration_ms;
      const timing = assessReplayTiming(replay, actualDurationMs);
      const presentation = await waitForCommandEvent(
        events,
        "native-v5-snap-presentation-evidence",
        command.id,
        timeoutMs,
      );
      const undoReady = await waitForReadyEvent(
        events,
        command.id,
        timeoutMs,
        "undo-shortcut",
      );
      const undoDurationMs = await execute(["key", "ctrl+z"], 0);
      const redoReady = await waitForReadyEvent(
        events,
        command.id,
        timeoutMs,
        "redo-shortcut",
      );
      const redoDurationMs = await execute(["key", "ctrl+shift+z"], 0);
      const applicationReceipt = await waitForCommandEvent(
        events,
        "native-v5-snap-application-evidence",
        command.id,
        timeoutMs,
      );
      const observedApplication =
        applicationReceipt.observed_application_update_timestamps_ms ?? [];
      if (
        replay.pixel_samples.length !== command.expected_sample_count ||
        observedReplay.injected_samples.length !==
          command.expected_sample_count ||
        observedApplication.length < 3 ||
        applicationReceipt.observed_application_update_count !==
          observedApplication.length ||
        applicationReceipt.first_position_observed !== true ||
        applicationReceipt.final_position_observed !== true
      ) {
        throw new Error(
          `native v5 snap retained ${replay.pixel_samples.length} scheduled, ${observedReplay.injected_samples.length} injected, and ${observedApplication.length} application updates; expected ${command.expected_sample_count} scheduled/injected and at least three application updates`,
        );
      }
      commandResults.push({
        command_id: command.id,
        scheduled_duration_ms: replay.metadata.scheduled_duration_ms,
        actual_duration_ms: actualDurationMs,
        replay: replay.metadata,
        execution: replay.execution,
        timing,
        timestamped_injected_samples: observedReplay.injected_samples.map(
          (sample, index) => ({
            ...sample,
            window_pixel: {
              x: replay.pixel_samples[index].x,
              y: replay.pixel_samples[index].y,
            },
          }),
        ),
        observed_application_update_timestamps_ms: observedApplication,
        property_presentation_receipt: presentation,
        undo_ready_event_t_ms: undoReady.t_ms,
        undo_duration_ms: undoDurationMs,
        redo_ready_event_t_ms: redoReady.t_ms,
        redo_duration_ms: redoDurationMs,
        application_receipt: applicationReceipt,
      });
      continue;
    }
    if (command.operation === "viewer.dynamic-fidelity-scroll-path") {
      let captureServer;
      try {
        const ready = await waitForReadyEvent(events, command.id, timeoutMs);
        await waitForMatchingEvent(
          events,
          (event) =>
            event.event === "dynamic-fidelity-state" &&
            event.command_id === command.id &&
            Array.isArray(event.visible_pages) &&
            event.visible_pages.length > 0,
          `${command.id} initial live state`,
          timeoutMs,
        );
        const replay = buildNativeCommandReplay(command, ready, target);
        const injectedCalibration = await runDirectXTestWheelCalibration(
          replay,
          target,
        );
        const calibrationEvent = await waitForCommandEvent(
          events,
          "native-wheel-calibrated",
          command.id,
          timeoutMs,
        );
        const wheelCalibration = validateDynamicWheelCalibrationReceipt(
          ready,
          calibrationEvent,
        );
        const heldDistancePlan = buildHeldDynamicWheelPlan({
          command,
          viewportHeightCssPx: ready.viewport_bounds_window_logical?.height,
          wheelDeltaCssPx: wheelCalibration.observed_wheel_delta_css_px,
          initialScrollOffsetCssPx: ready.initial_scroll_offset_css_px?.y,
          viewportBoundsWindowLogical: ready.viewport_bounds_window_logical,
          checkpointPageGeometries: ready.checkpoint_page_geometries,
          zoomPercent: ready.zoom_percent,
          displayScaleFactor: ready.display_scale_factor,
        });
        captureServer = await startPresentedDrawableCaptureServer(target);
        const observerPromise = runIndependentDynamicObserver({
          durationMs: command.duration_ms,
          rateHz: command.observer.rate_hz,
        });
        const inputSession = await startDirectXTestHeldDynamicWheel(
          replay,
          target,
          heldDistancePlan,
        );
        const inputPromise = inputSession.completion;
        const capturesPromise = collectDynamicPresentedCaptures({
          command,
          events,
          artifactDirectory,
          heldDistancePlan,
          captureServer,
          helperFeed: inputSession.feed,
        });
        const [observerTicks, input, rawCaptures] = await Promise.all([
          observerPromise,
          inputPromise,
          capturesPromise,
        ]);
        if (
          input.trajectory.length !==
            command.expected_trajectory_sample_count ||
          !heldDynamicTrajectoryPassed(input.trajectory, heldDistancePlan)
        ) {
          throw new Error(
            `dynamic trajectory emitted ${input.trajectory.length}/${command.expected_trajectory_sample_count} samples`,
          );
        }
        const registeredCrops = await convertAndCompareDynamicPresentedCaptures(
          {
            captures: rawCaptures,
            command,
            artifactDirectory,
            trajectory: input.trajectory,
            target,
          },
        );
        const fidelitySamples = bindDynamicObserverSamples(
          observerTicks,
          events,
          command,
        );
        const timing = assessReplayTiming(replay, input.actual_duration_ms);
        const lastEventTime = events.at(-1)?.t_ms ?? 0;
        events.push({
          schema_version: 1,
          event: "dynamic-fidelity-runner-evidence",
          t_ms: lastEventTime,
          command_id: command.id,
          trajectory_samples: input.trajectory,
          fidelity_samples: fidelitySamples,
          expected_trajectory_sample_count:
            command.expected_trajectory_sample_count,
          expected_fidelity_sample_count:
            command.observer.expected_sample_count,
          observer_clock: command.observer.clock,
          native_input_timing: timing,
          wheel_calibration: {
            injection: injectedCalibration,
            application_receipt: wheelCalibration,
          },
          held_distance_plan: heldDistancePlan,
          registered_crops: registeredCrops,
        });
        for (const milestone of dynamicRunnerMilestonesV5) {
          events.push({
            schema_version: 1,
            event: "comparison-milestone",
            t_ms: lastEventTime,
            command_id: command.id,
            milestone,
            evidence_source: "independent-native-runner",
          });
        }
        const runnerResult = await writeDynamicRunnerResult(artifactDirectory, {
          status: "passed",
          commandId: command.id,
        });
        commandResults.push({
          command_id: command.id,
          scheduled_duration_ms: replay.metadata.scheduled_duration_ms,
          actual_duration_ms: input.actual_duration_ms,
          replay: replay.metadata,
          execution: replay.execution,
          timing,
          wheel_calibration: {
            injection: injectedCalibration,
            application_receipt: wheelCalibration,
          },
          held_distance_plan: heldDistancePlan,
          trajectory_samples: input.trajectory,
          fidelity_samples: fidelitySamples,
          registered_crops: registeredCrops,
          runner_result: runnerResult,
        });
      } catch (error) {
        await retainDynamicCaptureFailureEvidence(
          artifactDirectory,
          error,
        ).catch(() => {});
        await writeDynamicRunnerResult(artifactDirectory, {
          status: "failed",
          commandId: command.id,
          error: error.message,
        }).catch(() => {});
        throw error;
      } finally {
        if (captureServer) await captureServer.close().catch(() => {});
      }
      continue;
    }
    if (command.operation === "annotation.rectangle.transform") {
      const createCommand = workload?.journeys
        ?.flatMap(({ commands }) => commands ?? [])
        .find(({ id }) => id === "rectangle:create-sparse");
      if (!createCommand) {
        throw new Error(
          "native annotation-transform requires rectangle:create-sparse from the active workload",
        );
      }
      const stages = [];
      for (const stage of ["prerequisite-create", "move", "east-resize"]) {
        const readyCommandId =
          stage === "prerequisite-create" ? createCommand.id : command.id;
        const ready = await waitForReadyEvent(
          events,
          readyCommandId,
          timeoutMs,
          stage,
        );
        const replay = buildNativeRectangleTransformReplay({
          createCommand,
          transformCommand: command,
          ready,
          target,
        });
        const actualDurationMs =
          execute === runXdotool
            ? await runDirectXTestPointer(replay, target)
            : await execute(replay.args, replay.metadata.scheduled_duration_ms);
        stages.push({
          stage,
          ready_event_t_ms: ready.t_ms,
          scheduled_duration_ms: replay.metadata.scheduled_duration_ms,
          actual_duration_ms: actualDurationMs,
          replay: replay.metadata,
          execution: replay.execution,
          timing: assessReplayTiming(replay, actualDurationMs),
        });
      }
      const receipt = await waitForCommandEvent(
        events,
        "comparison-native-transform-evidence",
        command.id,
        timeoutMs,
      );
      const applicationReceipt = validateNativeRectangleTransformEvidence([
        receipt,
      ]);
      commandResults.push({
        command_id: command.id,
        stages,
        timing: {
          within_tolerance: stages.every(
            (stage) => stage.timing.within_tolerance,
          ),
        },
        application_receipt: applicationReceipt,
      });
      continue;
    }
    const ready = await waitForReadyEvent(events, command.id, timeoutMs);
    const replay = buildNativeCommandReplay(command, ready, target);
    const actualDurationMs =
      execute === runXdotool
        ? replay.execution === "direct-pointer"
          ? await runDirectXTestPointer(replay, target)
          : replay.execution === "direct-wheel"
            ? await runDirectXTestWheel(replay, target)
            : await runXdotool(
                replay.args,
                replay.metadata.scheduled_duration_ms,
              )
        : await execute(replay.args, replay.metadata.scheduled_duration_ms);
    const timing = assessReplayTiming(replay, actualDurationMs);
    const editorCommand = [
      "annotation.text.create",
      "measurement.set-scale",
      "annotation.length.create",
      "annotation.image.create",
    ].includes(command.operation);
    const applicationReceipt = editorCommand
      ? await waitForCommandEvent(
          events,
          "comparison-native-input-evidence",
          command.id,
          timeoutMs,
        )
      : null;
    const textEntryReceipt =
      command.operation === "annotation.text.create"
        ? await waitForCommandEvent(
            events,
            "native-text-entry-observed",
            command.id,
            timeoutMs,
          )
        : null;
    commandResults.push({
      command_id: command.id,
      scheduled_duration_ms: replay.metadata.scheduled_duration_ms,
      actual_duration_ms: actualDurationMs,
      replay: replay.metadata,
      execution: replay.execution,
      timing,
      ready_event_t_ms: ready.t_ms,
      application_receipt: applicationReceipt,
      text_entry_receipt: textEntryReceipt,
    });
  }
  return {
    success:
      viewerEvidence.success &&
      commandResults.every(({ timing }) => timing.within_tolerance),
    target_verified: true,
    target,
    viewer: {
      shell_replay: shellReplay.metadata,
      ...viewerEvidence,
    },
    driver_evidence: driverEvidence,
    commands: commandResults,
  };
}

export async function locateExactButterPaperWindow(
  pid,
  { timeoutMs = 10_000 } = {},
) {
  return locateExactX11Window(pid, {
    timeoutMs,
    expectedTitle: nativeX11WindowTitle,
    expectedSize: fixedWindowSize,
    allowDecoratedClient: true,
  });
}

function regexEscape(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export async function locateExactX11Window(
  pid,
  {
    timeoutMs = 10_000,
    expectedTitle,
    expectedSize = fixedWindowSize,
    allowDecoratedClient = false,
  } = {},
) {
  if (!Number.isInteger(pid) || pid <= 0)
    throw new Error("native X11 replay requires a positive target PID");
  if (typeof expectedTitle !== "string" || expectedTitle.length === 0) {
    throw new Error(
      "native X11 replay requires an exact non-empty window title",
    );
  }
  const deadline = Date.now() + timeoutMs;
  let ids = [];
  while (Date.now() < deadline) {
    try {
      const { stdout } = await execFileAsync(
        "xdotool",
        [
          "search",
          "--onlyvisible",
          "--pid",
          String(pid),
          "--name",
          `^${regexEscape(expectedTitle)}$`,
        ],
        { encoding: "utf8", timeout: 2_000, maxBuffer: 64_000 },
      );
      ids = stdout.trim().split(/\s+/).filter(Boolean);
      if (ids.length > 0) break;
    } catch {
      // A not-yet-created window makes xdotool exit non-zero. Poll only until
      // the bounded deadline; target validation remains fail-closed below.
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 50));
  }
  if (ids.length !== 1) {
    return validateWindowTarget({
      ids,
      expectedPid: pid,
      actualPid: null,
      expectedTitle,
      actualTitle: null,
      geometry: { visible: false },
      expectedSize,
    });
  }
  const windowId = ids[0];
  return inspectExactX11WindowById({
    pid,
    windowId,
    ids,
    expectedTitle,
    expectedSize,
    allowDecoratedClient,
  });
}

export async function locateExactX11WindowById(
  pid,
  windowId,
  {
    expectedTitle,
    expectedSize = fixedWindowSize,
    allowDecoratedClient = false,
  } = {},
) {
  if (!Number.isInteger(pid) || pid <= 0) {
    throw new Error("native X11 replay requires a positive target PID");
  }
  if (!/^\d+$/.test(String(windowId)) || Number(windowId) < 1) {
    throw new Error("native X11 replay requires a positive target window ID");
  }
  if (typeof expectedTitle !== "string" || expectedTitle.length === 0) {
    throw new Error(
      "native X11 replay requires an exact non-empty window title",
    );
  }
  return inspectExactX11WindowById({
    pid,
    windowId: String(windowId),
    ids: [String(windowId)],
    expectedTitle,
    expectedSize,
    allowDecoratedClient,
  });
}

async function inspectExactX11WindowById({
  pid,
  windowId,
  ids,
  expectedTitle,
  expectedSize,
  allowDecoratedClient,
}) {
  const [
    { stdout: pidText },
    { stdout: titleText },
    { stdout: geometryText },
    { stdout: clientGeometryText },
  ] = await Promise.all([
    execFileAsync("xdotool", ["getwindowpid", windowId], {
      encoding: "utf8",
      timeout: 2_000,
    }),
    execFileAsync("xdotool", ["getwindowname", windowId], {
      encoding: "utf8",
      timeout: 2_000,
    }),
    execFileAsync("xdotool", ["getwindowgeometry", "--shell", windowId], {
      encoding: "utf8",
      timeout: 2_000,
    }),
    execFileAsync("xwininfo", ["-id", windowId], {
      encoding: "utf8",
      timeout: 2_000,
    }),
  ]);
  const parsed = parseXdotoolGeometry(geometryText);
  const client = parseXwininfoClientGeometry(clientGeometryText);
  if (parsed.width !== client.width || parsed.height !== client.height) {
    throw new Error(
      "xdotool and xwininfo disagree about the target client size",
    );
  }
  let frameExtents = null;
  if (
    expectedSize &&
    (client.width !== expectedSize.width ||
      client.height !== expectedSize.height)
  ) {
    try {
      const { stdout } = await execFileAsync(
        "xprop",
        ["-id", windowId, "_NET_FRAME_EXTENTS"],
        { encoding: "utf8", timeout: 2_000, maxBuffer: 64_000 },
      );
      frameExtents = parseX11FrameExtents(stdout);
    } catch (error) {
      if (allowDecoratedClient) {
        throw new Error(
          `native X11 replay could not verify decorated client frame extents: ${error.message}`,
        );
      }
    }
  }
  return validateWindowTarget({
    ids,
    expectedPid: pid,
    actualPid: Number(pidText.trim()),
    expectedTitle,
    actualTitle: titleText.trim(),
    geometry: {
      ...parsed,
      x: client.x,
      y: client.y,
      visible: true,
      client_origin_source: "xwininfo-absolute-upper-left",
      frame_extents: frameExtents,
    },
    expectedSize,
    allowDecoratedClient,
  });
}
