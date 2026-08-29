import { canonicalSha256 } from "./run-paired-v4.mjs";

export const viewStateEventNameV5 = "comparison-view-state";
export const viewStateCheckpointsV5 = Object.freeze([
  "measurement-start",
  "measurement-end",
]);

const layoutModes = new Set(["single-page", "continuous"]);
const zoomModes = new Set(["fit-page", "fit-width", "manual"]);
const geometryToleranceLogicalPx = 0.5;
const zoomTolerancePercent = 0.05;
const scaleTolerance = 1e-6;

function finite(value) {
  return typeof value === "number" && Number.isFinite(value);
}

function positive(value) {
  return finite(value) && value > 0;
}

function nonnegative(value) {
  return finite(value) && value >= 0;
}

function canonicalBounds(value) {
  return {
    x: value?.x,
    y: value?.y,
    width: value?.width,
    height: value?.height,
  };
}

function canonicalSidebar(event, side) {
  return {
    visible: event?.[`${side}_sidebar_visible`],
    width_logical: event?.[`${side}_sidebar_width_logical`],
  };
}

function canonicalSnapshot(event) {
  return {
    checkpoint: event?.checkpoint,
    observation_source: event?.observation_source,
    live: event?.live,
    window_bounds_window_logical: canonicalBounds(
      event?.window_bounds_window_logical,
    ),
    viewport_bounds_window_logical: canonicalBounds(
      event?.viewport_bounds_window_logical,
    ),
    display_scale_factor: event?.display_scale_factor,
    layout_mode: event?.layout_mode,
    zoom_mode: event?.zoom_mode,
    zoom_percent: event?.zoom_percent,
    left_sidebar: canonicalSidebar(event, "left"),
    right_sidebar: canonicalSidebar(event, "right"),
    active_document: {
      fixture_id: event?.active_fixture_id,
      tab_index: event?.active_document_index,
      open_document_count: event?.open_document_count,
    },
  };
}

function validateBounds(bounds, label, failures) {
  if (
    !finite(bounds?.x) ||
    !finite(bounds?.y) ||
    !positive(bounds?.width) ||
    !positive(bounds?.height)
  ) {
    failures.push(`${label} must contain finite x/y and positive width/height`);
  }
}

function validateSidebar(sidebar, label, failures) {
  if (typeof sidebar?.visible !== "boolean") {
    failures.push(`${label} visibility must be observed as a boolean`);
    return;
  }
  if (!nonnegative(sidebar.width_logical)) {
    failures.push(`${label} width must be a nonnegative logical-pixel value`);
    return;
  }
  if (sidebar.visible !== sidebar.width_logical > 0) {
    failures.push(`${label} visibility and observed width disagree`);
  }
}

function validateSnapshot(snapshot, fixtureIds, failures) {
  if (!viewStateCheckpointsV5.includes(snapshot.checkpoint)) {
    failures.push(`unknown view-state checkpoint ${snapshot.checkpoint}`);
  }
  if (
    snapshot.live !== true ||
    snapshot.observation_source !== "live-application-render-state"
  ) {
    failures.push(
      `${snapshot.checkpoint}: view state must come from live application render state`,
    );
  }
  validateBounds(
    snapshot.window_bounds_window_logical,
    `${snapshot.checkpoint} window bounds`,
    failures,
  );
  validateBounds(
    snapshot.viewport_bounds_window_logical,
    `${snapshot.checkpoint} viewport bounds`,
    failures,
  );
  const windowBounds = snapshot.window_bounds_window_logical;
  const viewport = snapshot.viewport_bounds_window_logical;
  if (
    finite(windowBounds.x) &&
    finite(windowBounds.y) &&
    positive(windowBounds.width) &&
    positive(windowBounds.height) &&
    finite(viewport.x) &&
    finite(viewport.y) &&
    positive(viewport.width) &&
    positive(viewport.height) &&
    (viewport.x < windowBounds.x - geometryToleranceLogicalPx ||
      viewport.y < windowBounds.y - geometryToleranceLogicalPx ||
      viewport.x + viewport.width >
        windowBounds.x + windowBounds.width + geometryToleranceLogicalPx ||
      viewport.y + viewport.height >
        windowBounds.y + windowBounds.height + geometryToleranceLogicalPx)
  ) {
    failures.push(`${snapshot.checkpoint}: viewport is outside the window`);
  }
  if (!positive(snapshot.display_scale_factor)) {
    failures.push(
      `${snapshot.checkpoint}: display scale factor must be positive`,
    );
  }
  if (!layoutModes.has(snapshot.layout_mode)) {
    failures.push(`${snapshot.checkpoint}: layout mode is not exact`);
  }
  if (!zoomModes.has(snapshot.zoom_mode) || !positive(snapshot.zoom_percent)) {
    failures.push(
      `${snapshot.checkpoint}: zoom mode and observed percentage are required`,
    );
  }
  validateSidebar(
    snapshot.left_sidebar,
    `${snapshot.checkpoint} left sidebar`,
    failures,
  );
  validateSidebar(
    snapshot.right_sidebar,
    `${snapshot.checkpoint} right sidebar`,
    failures,
  );
  const { fixture_id: fixtureId, tab_index: tabIndex } =
    snapshot.active_document;
  const documentCount = snapshot.active_document.open_document_count;
  if (documentCount === 0) {
    if (fixtureId !== null || tabIndex !== null) {
      failures.push(
        `${snapshot.checkpoint}: an empty document set must have no active fixture or tab`,
      );
    }
  } else if (
    !Number.isInteger(documentCount) ||
    documentCount < 1 ||
    !fixtureIds.includes(fixtureId) ||
    !Number.isInteger(tabIndex) ||
    tabIndex < 0 ||
    tabIndex >= documentCount
  ) {
    failures.push(
      `${snapshot.checkpoint}: active fixture and document index/count are invalid`,
    );
  }
}

export function buildViewStateReceiptV5(rawReport, run) {
  const failures = [];
  const iteration = rawReport?.iterations?.[0];
  const matchingEvents = (iteration?.events ?? []).filter(
    (event) =>
      event?.event === viewStateEventNameV5 &&
      event?.component === run.component,
  );
  const snapshots = [];
  for (const checkpoint of viewStateCheckpointsV5) {
    const matches = matchingEvents.filter(
      (event) => event.checkpoint === checkpoint,
    );
    if (matches.length !== 1) {
      failures.push(
        `${run.component}: expected exactly one ${checkpoint} view-state event, got ${matches.length}`,
      );
      continue;
    }
    const snapshot = canonicalSnapshot(matches[0]);
    validateSnapshot(snapshot, run.fixture_ids ?? [], failures);
    snapshots.push(snapshot);
  }
  if (matchingEvents.length !== viewStateCheckpointsV5.length) {
    failures.push(
      `${run.component}: view-state event count must be ${viewStateCheckpointsV5.length}`,
    );
  }
  const payload = {
    schema_version: 1,
    implementation: run.implementation,
    journey: run.journey,
    component: run.component,
    fixture_ids: [...(run.fixture_ids ?? [])],
    snapshots,
  };
  return {
    passed: failures.length === 0,
    failures,
    receipt:
      failures.length === 0
        ? { ...payload, evidence_sha256: canonicalSha256(payload) }
        : null,
  };
}

function compareNumber(left, right, tolerance, label, failures) {
  if (!finite(left) || !finite(right) || Math.abs(left - right) > tolerance) {
    failures.push(`${label} differs: Electron=${left}, GPUI=${right}`);
  }
}

function compareBounds(left, right, label, failures) {
  for (const field of ["x", "y", "width", "height"]) {
    compareNumber(
      left?.[field],
      right?.[field],
      geometryToleranceLogicalPx,
      `${label}.${field}`,
      failures,
    );
  }
}

function compareSnapshot(electron, gpui, failures) {
  const prefix = electron.checkpoint;
  if (electron.checkpoint !== gpui.checkpoint) {
    failures.push(
      `checkpoint order differs: Electron=${electron.checkpoint}, GPUI=${gpui.checkpoint}`,
    );
    return;
  }
  compareBounds(
    electron.window_bounds_window_logical,
    gpui.window_bounds_window_logical,
    `${prefix}.window`,
    failures,
  );
  compareBounds(
    electron.viewport_bounds_window_logical,
    gpui.viewport_bounds_window_logical,
    `${prefix}.viewport`,
    failures,
  );
  compareNumber(
    electron.display_scale_factor,
    gpui.display_scale_factor,
    scaleTolerance,
    `${prefix}.display_scale_factor`,
    failures,
  );
  compareNumber(
    electron.zoom_percent,
    gpui.zoom_percent,
    zoomTolerancePercent,
    `${prefix}.zoom_percent`,
    failures,
  );
  for (const field of ["layout_mode", "zoom_mode"]) {
    if (electron[field] !== gpui[field]) {
      failures.push(
        `${prefix}.${field} differs: Electron=${electron[field]}, GPUI=${gpui[field]}`,
      );
    }
  }
  for (const side of ["left_sidebar", "right_sidebar"]) {
    if (electron[side].visible !== gpui[side].visible) {
      failures.push(`${prefix}.${side}.visible differs`);
    }
    compareNumber(
      electron[side].width_logical,
      gpui[side].width_logical,
      geometryToleranceLogicalPx,
      `${prefix}.${side}.width_logical`,
      failures,
    );
  }
  for (const field of ["fixture_id", "tab_index", "open_document_count"]) {
    if (electron.active_document[field] !== gpui.active_document[field]) {
      failures.push(`${prefix}.active_document.${field} differs`);
    }
  }
}

export function compareViewStateReceiptsV5(electron, gpui) {
  const failures = [];
  for (const [label, receipt] of [
    ["Electron", electron],
    ["GPUI", gpui],
  ]) {
    const { evidence_sha256: evidenceSha256, ...payload } = receipt ?? {};
    if (
      typeof evidenceSha256 !== "string" ||
      canonicalSha256(payload) !== evidenceSha256
    ) {
      failures.push(`${label} view-state receipt hash is absent or invalid`);
    }
  }
  if (
    electron?.implementation !== "electron" ||
    gpui?.implementation !== "gpui"
  ) {
    failures.push("view-state pair must be ordered Electron then GPUI");
  }
  for (const field of ["journey", "component"]) {
    if (electron?.[field] !== gpui?.[field]) {
      failures.push(`view-state receipt ${field} differs`);
    }
  }
  if (
    JSON.stringify(electron?.fixture_ids) !== JSON.stringify(gpui?.fixture_ids)
  ) {
    failures.push("view-state receipt fixture order differs");
  }
  if (
    !Array.isArray(electron?.snapshots) ||
    !Array.isArray(gpui?.snapshots) ||
    electron.snapshots.length !== viewStateCheckpointsV5.length ||
    gpui.snapshots.length !== viewStateCheckpointsV5.length
  ) {
    failures.push("view-state receipts do not contain both checkpoints");
  } else {
    for (let index = 0; index < viewStateCheckpointsV5.length; index += 1) {
      compareSnapshot(
        electron.snapshots[index],
        gpui.snapshots[index],
        failures,
      );
    }
  }
  const payload = {
    schema_version: 1,
    journey: electron?.journey ?? gpui?.journey ?? null,
    component: electron?.component ?? gpui?.component ?? null,
    electron_evidence_sha256: electron?.evidence_sha256 ?? null,
    gpui_evidence_sha256: gpui?.evidence_sha256 ?? null,
    tolerance: {
      geometry_logical_px: geometryToleranceLogicalPx,
      zoom_percent: zoomTolerancePercent,
      display_scale_factor: scaleTolerance,
    },
    passed: failures.length === 0,
    failures,
  };
  return { ...payload, evidence_sha256: canonicalSha256(payload) };
}

export function compareBundleViewStatesV5(electronBundle, gpuiBundle) {
  const failures = [];
  for (const field of ["phase", "journey", "pair"]) {
    if (electronBundle?.[field] !== gpuiBundle?.[field]) {
      failures.push(`bundle ${field} differs`);
    }
  }
  const electronComponents = new Map(
    (electronBundle?.components ?? []).map((component) => [
      component.component,
      component,
    ]),
  );
  const gpuiComponents = new Map(
    (gpuiBundle?.components ?? []).map((component) => [
      component.component,
      component,
    ]),
  );
  const componentMatches = [];
  for (const [component, electron] of electronComponents) {
    const gpui = gpuiComponents.get(component);
    if (!gpui) {
      failures.push(
        `${component}: GPUI component is missing from paired bundle`,
      );
      continue;
    }
    if (
      electron.benefit_metrics_eligible !== true ||
      gpui.benefit_metrics_eligible !== true
    ) {
      componentMatches.push({
        component,
        applicable: false,
        reason: "one-or-both-runtime-results-benefit-ineligible",
      });
      continue;
    }
    const match = compareViewStateReceiptsV5(
      electron.view_state_receipt,
      gpui.view_state_receipt,
    );
    componentMatches.push({ component, applicable: true, ...match });
    failures.push(
      ...match.failures.map((failure) => `${component}: ${failure}`),
    );
  }
  for (const component of gpuiComponents.keys()) {
    if (!electronComponents.has(component)) {
      failures.push(
        `${component}: Electron component is missing from paired bundle`,
      );
    }
  }
  const payload = {
    schema_version: 1,
    phase: electronBundle?.phase ?? gpuiBundle?.phase ?? null,
    journey: electronBundle?.journey ?? gpuiBundle?.journey ?? null,
    pair: electronBundle?.pair ?? gpuiBundle?.pair ?? null,
    passed: failures.length === 0,
    failures,
    component_matches: componentMatches,
  };
  return { ...payload, evidence_sha256: canonicalSha256(payload) };
}
