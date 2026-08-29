#!/usr/bin/env node

import { spawn, execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import {
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import {
  arch,
  cpus,
  freemem,
  hostname,
  platform,
  release,
  tmpdir,
  totalmem,
  type,
} from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import {
  allowedScenarios,
  buildDevelopmentScenarioContract,
  normalizedPageSequence,
  protocolVersion,
  scenarioContractVersion,
  zoomSequence,
} from "./scenario-contract.mjs";
import {
  buildScenarioContractV4,
  protocolVersionV4,
  scenarioContractVersionV4,
} from "./scenario-contract-v4.mjs";
import {
  cgroupLaunch,
  createLinuxCgroup,
  readLinuxCgroup,
  removeLinuxCgroup,
} from "./linux-cgroup.mjs";
import {
  startNvidiaBaselineRunSampler,
  summarizeNvidiaIterations,
} from "./nvidia-sampler.mjs";
import {
  loadComparisonWorkload,
  runnerComparisonMetadata,
  validateComparisonWorkload,
} from "./comparison-workload.mjs";
import {
  comparisonWorkloadArtifactHashV4,
  loadComparisonWorkloadV4,
  validateComparisonWorkloadV4,
} from "./comparison-workload-v4.mjs";
import {
  comparisonWorkloadArtifactHashV5,
  comparisonWorkloadByteHashV5,
  loadMaterializedComparisonWorkloadV5,
  validateComparisonWorkloadV5,
} from "./comparison-workload-v5.mjs";
import {
  buildScenarioContractV5,
  protocolVersionV5,
  scenarioContractVersionV5,
} from "./scenario-contract-v5.mjs";
import {
  protocolVersionV6,
  representativeScenarioDefinitionsV6,
  scenarioContractVersionV6,
  validateScenarioContractV6,
} from "./scenario-contract-v6.mjs";
import {
  classifyElectronEngineeringZoomBaselineDefectV6,
  electronEngineeringZoomBaselineDefectIdV6,
} from "./electron-v6-baseline-defect.mjs";
import {
  benchmarkStyleContractVersion,
  benchmarkStyleMatches,
  compareNativeBenchmarkGeometry,
  createBenchmarkAliasMap,
} from "./benchmark-canonical.mjs";
import {
  buildDeltaPointerStream,
  rectangleDenseRepeatEvidence,
  rectanglePropertiesHistoryEvidence,
  rectanglePropertiesTransformPrerequisiteEvidence,
  rectangleTransformPrerequisiteEvidence,
  rectangleTransformEvidence,
} from "./annotation-interaction-evidence.mjs";
import {
  assessReplayTiming,
  buildClickReplay,
  buildHeldDynamicWheelPlan,
  buildNativeRectangleTransformReplay,
  buildPointerReplay,
  buildWheelReplay,
  buildNativeCommandReplay,
  dynamicCaptureError,
  dynamicCaptureStateEvidence,
  dynamicHelperHoldState,
  heldDynamicTrajectoryPassed,
  locateExactX11Window,
  locateExactX11WindowById,
  losslesslyConvertPresentedPpmToPng,
  manifestPointerSamples,
  nativeX11InputLane,
  runDirectXTestPointer,
  runDamageObservedXTestExternalClick,
  runDamageObservedXTestKey,
  requireDynamicHelperHold,
  retainDynamicCaptureFailureEvidence,
  startDirectXTestHeldDynamicWheel,
  runDirectXTestWheel,
  runIndependentDynamicObserver,
  registeredDynamicCropsPassed,
  startPresentedDrawableCaptureServer,
  validatePresentedCapturePairWindowAndHold,
  runXdotool,
  windowLogicalPointToPixel,
} from "./gpui-native-x11.mjs";
import {
  abortX11DamageObserverCollection,
  beginX11DamageObserverCollection,
  finishX11DamageObserverCollection,
} from "./x11-damage-observer.mjs";
import {
  measureVisibleRasterFidelity,
  validateDynamicFidelitySeries,
} from "./dynamic-fidelity-v5.mjs";
import {
  mapPdfRectToImagePixels,
  registerAndComparePresentedCropV2,
} from "./registered-crop-v5.mjs";
import {
  startElectronImageTrace,
  stopElectronImageTrace,
  summarizeElectronImageTrace,
} from "./electron-image-upload-trace.mjs";
import {
  assessElectronUntimedCorrectness,
  buildElectronUntimedCorrectnessPlan,
} from "./electron-untimed-correctness.mjs";
import {
  activeGpuAdapterRequired,
  annotateElectronActiveGpuDevice,
  buildElectronActiveGpuAdapterReceipt,
} from "./active-gpu-adapter.mjs";
import {
  buildElectronCanonicalPersistenceState,
  captureIndependentPdfProbe,
  compareIndependentDocumentProbe,
  compareUnknownAnnotationProbe,
  projectElectronCanonicalPersistenceState,
  renderFixedPersistenceCrop,
  validateElectronPersistenceAppearanceStreams,
  validateElectronPersistenceNativeAnnotations,
} from "./electron-persistence-evidence.mjs";

const execFileAsync = promisify(execFile);
const performanceDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryDirectory = resolve(performanceDirectory, "../../..");
const defaultElectron = resolve(
  repositoryDirectory,
  platform() === "darwin"
    ? "node_modules/electron/dist/Electron.app/Contents/MacOS/Electron"
    : "node_modules/electron/dist/electron",
);
const sampleIntervalMs = 100;
const defaultTimeoutMs = 120_000;
const outputLimitBytes = 1_000_000;
const cdpDiagnosticInputLane = "cdp-input-diagnostic";
const v5MaterializedArtifactSha256 =
  "cc4f8b8940556390b8d16a6baae43e8a5a022541fba90beea08869e692ee920e";
const v5MaterializedByteSha256 =
  "e7b2540c7d455a30e52ee64a6819745fe0ad49a6512f887df4631aac72054f6d";
const v6ManifestId = "bp-perf-v6-decision-2";
const v6WorkloadByteSha256 =
  "fc7e3cb6f09b74e004a24b01a9f5ccbb444d98feb9ae6489885e27329a442147";
const v6WorkloadPath = resolve(
  performanceDirectory,
  "comparison-workload-v6.json",
);
const multiDocumentScenarioV5 = "multi-document-session";
const dynamicFidelityScenarioV5 = "viewer-dynamic-fidelity";
export const electronV5ComponentScenarios = new Set([
  multiDocumentScenarioV5,
  dynamicFidelityScenarioV5,
  "native-property-edit-undo",
  "native-snap-transform-120hz",
]);
const electronV5MaintainedUiBaselineByComponent = Object.freeze({
  "native-property-edit-undo":
    "known-baseline-history-defect: native Line Width input and blur record two history revisions; one application undo leaves the committed width current",
  "native-snap-transform-120hz":
    "maintained-default: 8 CSS px sensitivity with observed zoom and derived point threshold retained",
  "multi-document-session":
    "known-baseline-history-defect: the dense Rectangle property edit records two history revisions",
  "viewer-dynamic-fidelity":
    "maintained-default: continuous NASA scroll with independently sampled visible-raster fidelity and registered crops",
});

export function electronV5MaintainedUiCapability(componentScenario) {
  if (!electronV5ComponentScenarios.has(componentScenario)) {
    throw new Error(`unknown Electron v5 component ${componentScenario}`);
  }
  return {
    component: componentScenario,
    supported: true,
    live: true,
    blocker: null,
    maintained_baseline:
      electronV5MaintainedUiBaselineByComponent[componentScenario],
    mutation_path: "maintained-public-ui-only",
  };
}

export function buildElectronComparisonViewStateEvent({
  checkpoint,
  component,
  fixtureIds,
  pdfPaths,
  observed,
}) {
  const fixtureIdByPath = new Map(
    fixtureIds.map((fixtureId, index) => [pdfPaths[index], fixtureId]),
  );
  const tabs = Array.isArray(observed?.tabs) ? observed.tabs : [];
  const activeDocumentIndex = tabs.findIndex(({ active }) => active === true);
  const activePath =
    activeDocumentIndex >= 0
      ? tabs[activeDocumentIndex]?.filePath
      : observed?.active_path;
  return {
    component,
    checkpoint,
    observation_source: "live-application-render-state",
    live: true,
    window_bounds_window_logical: observed?.window_bounds_window_logical,
    viewport_bounds_window_logical: observed?.viewport_bounds_window_logical,
    display_scale_factor: observed?.display_scale_factor,
    layout_mode: observed?.layout_mode,
    zoom_mode: observed?.zoom_mode,
    zoom_percent: observed?.zoom_percent,
    left_sidebar_visible: observed?.left_sidebar_visible,
    left_sidebar_width_logical: observed?.left_sidebar_width_logical,
    right_sidebar_visible: observed?.right_sidebar_visible,
    right_sidebar_width_logical: observed?.right_sidebar_width_logical,
    active_fixture_id:
      tabs.length === 0 ? null : (fixtureIdByPath.get(activePath) ?? null),
    active_document_index:
      tabs.length === 0 || activeDocumentIndex < 0 ? null : activeDocumentIndex,
    open_document_count: tabs.length,
  };
}

export function createElectronComparisonViewStateCheckpointGate(enabled) {
  const checkpoints = ["measurement-start", "measurement-end"];
  let nextIndex = 0;
  return {
    claim(checkpoint) {
      if (!enabled) return false;
      const expected = checkpoints[nextIndex];
      if (checkpoint !== expected) {
        throw new Error(
          `comparison view-state checkpoint must be ${expected ?? "complete"}; received ${checkpoint}`,
        );
      }
      nextIndex += 1;
      return true;
    },
    complete() {
      return !enabled || nextIndex === checkpoints.length;
    },
  };
}

async function observeElectronComparisonViewState(cdp, options, checkpoint) {
  const observed = await cdp.evaluate(`(async () => {
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
    const viewport = document.querySelector('[data-testid="document-viewport"]')
      ?? document.querySelector('[data-testid="viewport-opening-document"]')
      ?? document.querySelector('[data-testid="viewport-open-document"]')?.closest('section');
    if (!viewport) throw new Error('comparison view-state viewport is unavailable');
    const viewportBounds = viewport.getBoundingClientRect();
    const leftSidebar = document.querySelector('[data-testid="left-sidebar"]');
    const rightSidebar = document.querySelector('[data-testid="right-sidebar"]');
    const leftBounds = leftSidebar?.getBoundingClientRect() ?? null;
    const rightBounds = rightSidebar?.getBoundingClientRect() ?? null;
    const visible = (bounds) =>
      bounds !== null && bounds.width > 0 && bounds.height > 0;
    return {
      window_bounds_window_logical: {
        x: 0,
        y: 0,
        width: window.innerWidth,
        height: window.innerHeight,
      },
      viewport_bounds_window_logical: {
        x: viewportBounds.left,
        y: viewportBounds.top,
        width: viewportBounds.width,
        height: viewportBounds.height,
      },
      display_scale_factor: window.devicePixelRatio,
      layout_mode: diagnostics.pageColumnsEnabled === true
        ? 'columns'
        : diagnostics.scrollMode,
      zoom_mode: diagnostics.zoomPreset,
      zoom_percent: diagnostics.zoom * 100,
      left_sidebar_visible: visible(leftBounds),
      left_sidebar_width_logical: visible(leftBounds) ? leftBounds.width : 0,
      right_sidebar_visible: visible(rightBounds),
      right_sidebar_width_logical: visible(rightBounds) ? rightBounds.width : 0,
      active_path: diagnostics.documentPath,
      tabs: diagnostics.tabs ?? [],
    };
  })()`);
  return buildElectronComparisonViewStateEvent({
    checkpoint,
    component: options.scenario,
    fixtureIds: (options.v6ExecutionContext ?? options.v5ExecutionContext)
      .execution_contract.fixture_ids,
    pdfPaths: options.pdfs,
    observed,
  });
}
const localV4EngineeringComponents = Object.freeze({
  "fit-modes": "engineering:fit-modes",
  "cache-pressure-recovery": "engineering:cache-recovery",
});
export const electronPersistenceCommandIds = Object.freeze([
  "unknown:import",
  "unknown:assert-cycle-1",
  "unknown:assert-cycle-2",
  "persistence:apply-fixed-state",
  "persistence:save-1",
  "persistence:reopen-1",
  "persistence:save-2",
  "persistence:reopen-2",
]);
export const electronNativeX11Scenarios = new Set([
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
]);

function exactOrderedValues(actual, expected) {
  return JSON.stringify(actual) === JSON.stringify(expected);
}

export function validateOrderedElectronScenarioFixtures(pdfs, contract) {
  if (!Array.isArray(contract?.fixture_ids)) return null;
  if (pdfs.length !== contract.fixture_ids.length) {
    return `${contract.scenario} requires ${contract.fixture_ids.length} ordered PDF fixtures; received ${pdfs.length}`;
  }
  for (let index = 0; index < contract.fixture_ids.length; index += 1) {
    const fixtureId = contract.fixture_ids[index];
    const expected = contract.fixture_sha256_by_id?.[fixtureId];
    if (pdfs[index]?.sha256 !== expected) {
      return `${fixtureId} at ordered --pdf index ${index} requires PDF SHA-256 ${expected}; received ${pdfs[index]?.sha256}`;
    }
  }
  return null;
}

export function createElectronV5ExecutionContext({
  workload,
  parentScenario,
  componentScenario,
}) {
  const parentContract = buildScenarioContractV5(workload, parentScenario);
  if (!parentContract.current_runner_components.includes(componentScenario)) {
    throw new Error(
      `${componentScenario} is not a component of v5 scenario ${parentScenario}`,
    );
  }
  const commandIds = parentContract.component_command_ids[componentScenario];
  const fixtureIds = parentContract.component_fixture_ids[componentScenario];
  const commands = commandIds.map((commandId) => {
    const command = parentContract.commands.find(({ id }) => id === commandId);
    if (!command) throw new Error(`v5 command is missing: ${commandId}`);
    return command;
  });
  const executionContract = {
    ...parentContract,
    scenario: componentScenario,
    parent_scenario: parentScenario,
    fixture_id: fixtureIds.length === 1 ? fixtureIds[0] : null,
    fixture_sha256:
      fixtureIds.length === 1
        ? parentContract.fixture_sha256_by_id[fixtureIds[0]]
        : null,
    fixture_ids: [...fixtureIds],
    fixture_sha256_by_id: Object.fromEntries(
      fixtureIds.map((fixtureId) => [
        fixtureId,
        parentContract.fixture_sha256_by_id[fixtureId],
      ]),
    ),
    command_ids: [...commandIds],
    commands,
    current_runner_components: [componentScenario],
    execution_eligible: false,
    execution_blocker: "exact live Electron v5 receipts have not been supplied",
  };
  return {
    protocol_version: protocolVersionV5,
    scenario_contract_version: scenarioContractVersionV5,
    manifest_id: workload.manifest_id,
    workload_artifact_sha256: comparisonWorkloadArtifactHashV5(workload),
    workload_byte_sha256: comparisonWorkloadByteHashV5(workload),
    parent_scenario: parentScenario,
    component_scenario: componentScenario,
    benefit_metrics_eligible:
      parentContract.component_benefit_metrics_eligible[componentScenario],
    parent_contract: parentContract,
    execution_contract: executionContract,
  };
}

export function createElectronV6ExecutionContext({
  workload,
  workloadByteSha256,
  parentScenario,
  componentScenario,
}) {
  const contractFailures = validateScenarioContractV6();
  if (
    workload?.manifest_id !== v6ManifestId ||
    workload?.protocol_version !== protocolVersionV6 ||
    workloadByteSha256 !== v6WorkloadByteSha256 ||
    contractFailures.length > 0
  ) {
    throw new Error(
      `invalid v6 execution contract: ${[
        workload?.manifest_id !== v6ManifestId
          ? "manifest ID is not exact"
          : null,
        workload?.protocol_version !== protocolVersionV6
          ? "protocol version is not exact"
          : null,
        workloadByteSha256 !== v6WorkloadByteSha256
          ? "workload byte SHA-256 changed"
          : null,
        ...contractFailures,
      ]
        .filter(Boolean)
        .join("; ")}`,
    );
  }
  const parentContract = representativeScenarioDefinitionsV6[parentScenario];
  if (!parentContract) {
    throw new Error(`unknown v6 parent scenario ${parentScenario}`);
  }
  if (!parentContract.benefit_components.includes(componentScenario)) {
    throw new Error(
      `${componentScenario} is not a benefit-eligible component of v6 scenario ${parentScenario}`,
    );
  }
  const fixtureIds = parentContract.component_fixture_ids[componentScenario];
  if (!Array.isArray(fixtureIds) || fixtureIds.length === 0) {
    throw new Error(
      `${componentScenario} has no exact v6 fixture mapping in ${parentScenario}`,
    );
  }
  return {
    protocol_version: protocolVersionV6,
    scenario_contract_version: scenarioContractVersionV6,
    manifest_id: workload.manifest_id,
    workload_byte_sha256: workloadByteSha256,
    parent_scenario: parentScenario,
    component_scenario: componentScenario,
    benefit_metrics_eligible: true,
    execution_contract: {
      scenario: componentScenario,
      parent_scenario: parentScenario,
      fixture_ids: [...fixtureIds],
      fixture_sha256_by_id: Object.fromEntries(
        fixtureIds.map((fixtureId) => [
          fixtureId,
          parentContract.fixture_sha256_by_id[fixtureId],
        ]),
      ),
    },
  };
}

function inclusiveTimedPdfSamples(start, finish, rateHz, durationMs) {
  const sampleCount = Math.round((durationMs * rateHz) / 1000) + 1;
  return Array.from({ length: sampleCount }, (_, index) => {
    const fraction = index / (sampleCount - 1);
    return {
      x: start.x + (finish.x - start.x) * fraction,
      y: start.y + (finish.y - start.y) * fraction,
      t_ms: (durationMs * index) / (sampleCount - 1),
    };
  });
}

export function buildElectronNativeSnapReplayPlan(command) {
  if (command?.operation !== "annotation.snap-transform-native") {
    throw new Error(
      "native snap replay requires the v5 snap transform command",
    );
  }
  const samples = inclusiveTimedPdfSamples(
    command.pointer_path.start,
    command.pointer_path.unsnapped_end,
    command.rate_hz,
    command.duration_ms,
  );
  if (
    samples.length !== command.expected_sample_count ||
    command.snap?.grid_spacing_points !== 18 ||
    command.snap?.sensitivity?.value !== 8 ||
    command.snap?.sensitivity?.unit !== "css-px" ||
    command.snap?.sensitivity?.threshold_norm !== "per-axis-l-infinity" ||
    command.snap?.sensitivity?.inclusive !== true
  ) {
    throw new Error("native snap replay does not match the frozen v5 contract");
  }
  return {
    rate_hz: command.rate_hz,
    duration_ms: command.duration_ms,
    expected_sample_count: command.expected_sample_count,
    pdf_samples: samples,
    grid_spacing_points: command.snap.grid_spacing_points,
    grid_spacing_mm: (command.snap.grid_spacing_points * 25.4) / 72,
    sensitivity: structuredClone(command.snap.sensitivity),
  };
}

export function assessElectronNativePropertyEditUndoV5(command, observed) {
  const summary = {
    trusted_native_input: observed?.trusted_native_input === true,
    property: command?.property_edit?.property ?? null,
    before: observed?.before ?? null,
    committed: observed?.committed ?? null,
    after_undo: observed?.after_undo ?? null,
    commit_count: observed?.commit_count ?? null,
    undo_count: observed?.undo_count ?? null,
    effective_history_revision_delta:
      observed?.effective_history_revision_delta ??
      observed?.commit_count ??
      null,
    application_undo_count:
      observed?.application_undo_count ?? observed?.undo_count ?? null,
    known_baseline_defect_id:
      observed?.known_baseline_defect_id ?? observed?.known_defect_id ?? null,
    canonical_state_restored: observed?.canonical_state_restored === true,
    native_presentation_acknowledged:
      observed?.native_presentation_acknowledged === true,
    thumbnail_current: observed?.thumbnail_current === true,
  };
  const candidatePassed =
    summary.trusted_native_input &&
    summary.property === "stroke_width_points" &&
    summary.before === command.property_edit.from &&
    summary.committed === command.property_edit.to &&
    summary.after_undo === command.property_edit.from &&
    summary.commit_count === 1 &&
    summary.undo_count === 1 &&
    summary.canonical_state_restored &&
    summary.native_presentation_acknowledged &&
    summary.thumbnail_current;
  const baseline = command?.electron_baseline_policy;
  const electronBaselinePassed =
    summary.trusted_native_input &&
    summary.property === "stroke_width_points" &&
    summary.before === command.property_edit.from &&
    summary.committed === command.property_edit.to &&
    summary.after_undo === baseline?.final_stroke_width_points &&
    summary.effective_history_revision_delta ===
      baseline?.effective_history_revision_delta &&
    summary.application_undo_count === baseline?.application_undo_count &&
    summary.known_baseline_defect_id === baseline?.allowed_known_defect_id &&
    summary.native_presentation_acknowledged &&
    summary.thumbnail_current;
  return {
    passed: candidatePassed || electronBaselinePassed,
    candidate_passed: candidatePassed,
    electron_baseline_passed: electronBaselinePassed,
    summary,
  };
}

export function assessElectronNativeSnapTransformV5(command, observed) {
  const expected = command?.expected_final_rectangle;
  const actual = observed?.observed_final_rectangle;
  const deviations =
    expected && actual
      ? Object.keys(expected).map((key) =>
          Math.abs(actual[key] - expected[key]),
        )
      : [Number.POSITIVE_INFINITY];
  const maximumDeviation = Math.max(...deviations);
  const summary = {
    trusted_native_input: observed?.trusted_native_input === true,
    input_rate_hz: command?.rate_hz ?? null,
    expected_sample_count: command?.expected_sample_count ?? null,
    observed_sample_count: observed?.observed_sample_count ?? null,
    snap_enabled: observed?.snap_enabled === true,
    sensitivity_css_px:
      observed?.sensitivity_css_px ?? command?.snap?.sensitivity?.value ?? null,
    observed_pixels_per_point: observed?.observed_pixels_per_point ?? null,
    derived_threshold_points: observed?.derived_threshold_points ?? null,
    observed_raw_delta_points: observed?.observed_raw_delta_points ?? null,
    observed_snap_correction_points:
      observed?.observed_snap_correction_points ?? null,
    snap_target_acquired_count: observed?.snap_target_acquired_count ?? 0,
    snap_guide_presented_count: observed?.snap_guide_presented_count ?? 0,
    observed_final_rectangle: actual ?? null,
    maximum_geometry_deviation_points: maximumDeviation,
    gesture_commit_count: observed?.gesture_commit_count ?? null,
    undo_redo_exact: observed?.undo_redo_exact === true,
    thumbnail_current: observed?.thumbnail_current === true,
  };
  const passed =
    summary.trusted_native_input &&
    summary.input_rate_hz === 120 &&
    summary.expected_sample_count === 361 &&
    summary.observed_sample_count === 361 &&
    summary.snap_enabled &&
    summary.sensitivity_css_px === 8 &&
    Number.isFinite(summary.observed_pixels_per_point) &&
    summary.observed_pixels_per_point > 0 &&
    Number.isFinite(summary.derived_threshold_points) &&
    exactOrderedValues(summary.observed_raw_delta_points, { x: 97, y: 83 }) &&
    exactOrderedValues(summary.observed_snap_correction_points, {
      x: -7,
      y: 7,
    }) &&
    summary.snap_target_acquired_count >= 1 &&
    summary.snap_guide_presented_count >= 1 &&
    exactOrderedValues(actual, expected) &&
    maximumDeviation <= 0.01 &&
    summary.gesture_commit_count === 1 &&
    summary.undo_redo_exact &&
    summary.thumbnail_current;
  return { passed, summary };
}

export function assessElectronMultiDocumentSessionV5(summary) {
  const passed =
    exactOrderedValues(summary?.opened_fixture_ids, [
      "bp-single-page-v1",
      "nasa-apollo-summary-526-v1",
      "bp-engineering-sheet-v1",
      "bp-annotation-density-v1",
    ]) &&
    exactOrderedValues(summary?.switch_sequence, [
      "nasa-apollo-summary-526-v1",
      "bp-single-page-v1",
      "bp-engineering-sheet-v1",
      "bp-annotation-density-v1",
    ]) &&
    exactOrderedValues(summary?.close_sequence, [
      "bp-single-page-v1",
      "bp-engineering-sheet-v1",
      "nasa-apollo-summary-526-v1",
    ]) &&
    summary?.process_restart_count === 0 &&
    Array.isArray(summary?.observed_process_ids) &&
    summary.observed_process_ids.length === 1 &&
    summary.observed_process_ids[0] === summary.stable_process_id &&
    summary?.per_document_state_isolated === true &&
    summary?.current_raster_receipt_count === 8 &&
    summary?.dense_rectangle_property_user_gesture_count === 1 &&
    summary?.dense_rectangle_property_history_revision_delta === 2 &&
    summary?.dense_rectangle_stroke_width_points === 4 &&
    summary?.closed_document_resources_released === true &&
    summary?.remaining_document_count === 1 &&
    summary?.remaining_fixture_id === "bp-annotation-density-v1" &&
    summary?.dense_document_active === true &&
    summary?.aggregate_resource_observations_complete === true &&
    summary?.interactive_document_shell === true;
  return {
    passed,
    baseline_history_defect_retained:
      summary?.dense_rectangle_property_history_revision_delta === 2,
    summary,
  };
}

export function classifyElectronSecondTabRasterBlocker(observed) {
  if (
    observed?.requested_path !== observed?.active_path ||
    observed?.tab_count < 2
  ) {
    return {
      classification: "runner-sequencing-or-open-failure",
      nvidia_expected_to_change_outcome: false,
    };
  }
  const renderError =
    observed?.last_page_render_error ??
    observed?.session_last_page_render_error;
  if (
    typeof renderError === "string" &&
    /(alloc|memory|gpu|context lost|resource)/i.test(renderError)
  ) {
    return {
      classification: "environment-or-render-resource-limit",
      nvidia_expected_to_change_outcome: true,
    };
  }
  if (
    observed?.page_count > 0 &&
    observed?.visible_page_indices?.length === 0 &&
    observed?.queued_page_renders === 0 &&
    observed?.inflight_page_renders === 0 &&
    renderError == null
  ) {
    return {
      classification: "maintained-product-visible-layout-scheduling-defect",
      nvidia_expected_to_change_outcome: false,
    };
  }
  return {
    classification: "indeterminate-render-throughput-or-scheduling",
    nvidia_expected_to_change_outcome: null,
  };
}

const electronMultiDocumentMissingBenefitMetricsV5 = Object.freeze([
  "cpu_seconds",
  "cgroup_peak_memory_bytes",
  "product_wall_or_latency_ms",
  "application_frame_interval_p95_ms",
  "native_input_to_application_frame_ack_p95_ms",
  "baseline_adjusted_gpu_peak_memory_mib",
  "baseline_adjusted_gpu_utilization_p95_percent",
]);

export function buildElectronSecondNasaBaselineSummary(observed) {
  const classification = classifyElectronSecondTabRasterBlocker(observed);
  const exact =
    observed?.activated_fixture_id === "nasa-apollo-summary-526-v1" &&
    observed?.activation_ordinal === 2 &&
    observed?.page_count === 526 &&
    observed?.live_application_observed === true &&
    observed?.bounded_wait_completed === true &&
    Number.isFinite(observed?.live_observation_duration_ms) &&
    observed.live_observation_duration_ms >= 5_000 &&
    classification.classification ===
      "maintained-product-visible-layout-scheduling-defect" &&
    observed?.visible_raster_presented === false &&
    observed?.error_presented === false;
  if (!exact) return null;
  return {
    known_baseline_defect_id:
      "electron-multi-document-second-nasa-visible-pages-empty-v1",
    activated_fixture_id: observed.activated_fixture_id,
    activation_ordinal: observed.activation_ordinal,
    visible_page_indices: [],
    queued_raster_count: 0,
    inflight_raster_count: 0,
    visible_raster_presented: false,
    error_presented: false,
    benchmark_metrics_eligible: false,
    benchmark_metrics_missing: [
      ...electronMultiDocumentMissingBenefitMetricsV5,
    ],
  };
}

export function buildElectronV5ComponentEvidence(
  context,
  commandResults,
  semanticSummary,
) {
  const results = new Map(
    (commandResults ?? []).map((result) => [result.command_id, result]),
  );
  const commandReceipts = context.execution_contract.commands.map((command) => {
    const result = results.get(command.id);
    const provenMilestones = (result?.observed_milestones ?? []).filter(
      (milestone) => command.expected_milestones.includes(milestone),
    );
    const exactFieldsPassed =
      result?.exact_fields &&
      Object.values(result.exact_fields).every((value) => value === true);
    const passed =
      result?.manifest_milestones_complete === true &&
      exactFieldsPassed &&
      exactOrderedValues(provenMilestones, command.expected_milestones);
    const evidence = {
      command_id: command.id,
      live: result?.live ?? Boolean(result),
      passed,
      proven_milestones: provenMilestones,
      exact_fields: result?.exact_fields ?? null,
      observation: result?.observation ?? result?.observed ?? null,
    };
    return { ...evidence, evidence_sha256: evidenceSha256(evidence) };
  });
  return {
    protocol_version: context.protocol_version,
    scenario_contract_version: context.scenario_contract_version,
    manifest_id: context.manifest_id,
    workload_artifact_sha256: context.workload_artifact_sha256,
    workload_byte_sha256: context.workload_byte_sha256,
    parent_scenario: context.parent_scenario,
    component: context.component_scenario,
    benefit_metrics_eligible:
      semanticSummary?.known_baseline_defect_id ===
      "electron-multi-document-second-nasa-visible-pages-empty-v1"
        ? false
        : context.benefit_metrics_eligible,
    passed:
      commandReceipts.length > 0 &&
      commandReceipts.every((receipt) => receipt.passed),
    command_receipts: commandReceipts,
    semantic_summary: semanticSummary,
  };
}

export function buildElectronNativePageNavigationReplay({
  windowId,
  pageNumber,
  pageCount,
  track,
  thumb,
  logicalSize,
  windowGeometry,
}) {
  if (
    !Number.isInteger(pageNumber) ||
    !Number.isInteger(pageCount) ||
    pageCount < 2 ||
    pageNumber < 1 ||
    pageNumber > pageCount
  ) {
    throw new Error(
      "native page navigation requires a valid target page in a multi-page document",
    );
  }
  if (
    ![
      track?.x,
      track?.y,
      track?.width,
      track?.height,
      thumb?.x,
      thumb?.y,
      thumb?.width,
      thumb?.height,
    ].every((value) => Number.isFinite(value)) ||
    track.width <= 0 ||
    track.height <= 0 ||
    thumb.width <= 0 ||
    thumb.height <= 0 ||
    thumb.height > track.height
  ) {
    throw new Error(
      "native page navigation requires finite visible scrollbar geometry",
    );
  }
  const normalizedPagePosition = (pageNumber - 1) / (pageCount - 1);
  const logical = {
    start: { x: thumb.x + thumb.width / 2, y: thumb.y + thumb.height / 2 },
    finish: {
      x: track.x + track.width / 2,
      y:
        track.y +
        thumb.height / 2 +
        normalizedPagePosition * (track.height - thumb.height),
    },
    normalized_page_position: normalizedPagePosition,
  };
  const start = windowLogicalPointToPixel(
    logical.start,
    logicalSize,
    windowGeometry,
  );
  const finish = windowLogicalPointToPixel(
    logical.finish,
    logicalSize,
    windowGeometry,
  );
  return {
    args: [
      "mousemove",
      "--window",
      String(windowId),
      String(start.x),
      String(start.y),
      "mousedown",
      "1",
      "mousemove",
      "--window",
      String(windowId),
      String(finish.x),
      String(finish.y),
      "mouseup",
      "1",
    ],
    pixel: { start, finish },
    logical,
    metadata: {
      input_lane: nativeX11InputLane,
      injection_api: "XTEST-via-xdotool",
      total_event_count: 3,
      page_number: pageNumber,
      page_count: pageCount,
    },
  };
}

export function buildElectronDenseRectangleSourceContract(
  workload,
  denseCommand,
) {
  const commandIds = denseCommand?.source_commands;
  if (!Array.isArray(commandIds) || commandIds.length !== 3) {
    throw new Error(
      "rectangle:repeat-dense must name the three frozen source commands",
    );
  }
  const commands = new Map(
    workload?.journeys
      ?.flatMap(({ commands: candidates }) => candidates)
      .map((command) => [command.id, command]),
  );
  const orderedCommands = commandIds.map((commandId) =>
    commands.get(commandId),
  );
  if (orderedCommands.some((command) => !command)) {
    throw new Error(
      "rectangle:repeat-dense source command is missing from the workload",
    );
  }
  return {
    manifest_id: workload.manifest_id,
    input_lane: "semantic-diagnostic",
    command_ids: [...commandIds],
    require_exact_fields: true,
    commands: orderedCommands,
  };
}

export function buildElectronDenseFixtureMarkups(
  fixtureCommands,
  pageIndex = 1,
) {
  const pageId = `bp-annotation-density-v1:page:${String(pageIndex + 1).padStart(3, "0")}`;
  const color = (components, includeAlpha) => {
    if (
      !Array.isArray(components) ||
      components.length !== 4 ||
      components.some(
        (component) =>
          !Number.isFinite(component) || component < 0 || component > 1,
      )
    ) {
      throw new Error("density Rectangle command has invalid RGBA components");
    }
    const bytes = components.map((component) => Math.round(component * 255));
    return `#${bytes
      .slice(0, includeAlpha ? 4 : 3)
      .map((byte) => byte.toString(16).padStart(2, "0"))
      .join("")}`;
  };
  return (fixtureCommands?.commands ?? [])
    .filter(
      (command) =>
        command.page_id === pageId && command.operation === "create-rectangle",
    )
    .map((command) => ({
      id: command.annotation_id,
      pageIndex,
      kind: "rectangle",
      rect: { ...command.bounds_pdf },
      appearance: {
        stroke: {
          color: color(command.style?.stroke_rgba, false),
          widthPt: command.style?.stroke_width_pt,
          style: command.style?.stroke_style,
        },
        fill: { color: color(command.style?.fill_rgba, true) },
        opacity: 1,
        blendMode: "normal",
      },
    }));
}

export function electronDenseGeometryIndexMatchesSeed(index, seededMarkups) {
  if (
    !Number.isInteger(index?.pageIndex) ||
    index.pageIndex !== 1 ||
    index.totalMarkupCount !== seededMarkups?.length ||
    index.indexedMarkupCount !== seededMarkups?.length
  )
    return false;
  const sortEntries = (entries) =>
    entries
      .map(({ id, bounds }) => ({
        id,
        bounds: {
          x: bounds.x,
          y: bounds.y,
          width: bounds.width,
          height: bounds.height,
        },
      }))
      .sort((left, right) => left.id.localeCompare(right.id));
  const expected = sortEntries(
    (seededMarkups ?? [])
      .filter(({ pageIndex, kind }) => pageIndex === 1 && kind === "rectangle")
      .map(({ id, rect: bounds }) => ({ id, bounds })),
  );
  const observed = sortEntries(index.generation ?? []);
  return expected.length > 0 && exactJson(observed, expected);
}

export function remapElectronUntimedAction(action, aliases) {
  const observedIds = new Map(
    (aliases ?? []).map(
      ({ canonical_id: canonicalId, observed_id: observedId }) => [
        canonicalId,
        observedId,
      ],
    ),
  );
  const remap = (value) => {
    if (Array.isArray(value)) return value.map(remap);
    if (!value || typeof value !== "object") return value;
    return Object.fromEntries(
      Object.entries(value).map(([key, candidate]) => [
        key,
        key === "markupId"
          ? (observedIds.get(candidate) ?? candidate)
          : remap(candidate),
      ]),
    );
  };
  return { ...action, args: remap(action.args) };
}

export function buildAnnotationPointerSamples(command) {
  const path = command?.pointer_path;
  if (!path || path.rate_hz !== 120 || path.duration_ms !== 3000) {
    throw new Error(
      `${command?.id ?? "annotation command"} must use the frozen 3 second 120 Hz path`,
    );
  }
  const sampleCount = path.expected_sample_count;
  if (sampleCount !== (path.duration_ms / 1000) * path.rate_hz + 1) {
    throw new Error(`${command.id} has an inconsistent expected sample count`);
  }
  if (path.interpolation === "linear-inclusive") {
    const { start, finish } = path;
    return Array.from({ length: sampleCount }, (_, index) => {
      const fraction = index / (sampleCount - 1);
      return {
        t_ms: fraction * path.duration_ms,
        x: start.x + (finish.x - start.x) * fraction,
        y: start.y + (finish.y - start.y) * fraction,
      };
    });
  }
  if (path.interpolation === "catmull-rom-inclusive") {
    const points = path.control_points;
    if (
      !Array.isArray(points) ||
      points.length < 2 ||
      (sampleCount - 1) % (points.length - 1) !== 0
    ) {
      throw new Error(
        `${command.id} requires evenly divisible Catmull-Rom control-point segments`,
      );
    }
    const samplesPerSegment = (sampleCount - 1) / (points.length - 1);
    return Array.from({ length: sampleCount }, (_, index) => {
      const segment = Math.min(
        points.length - 2,
        Math.floor(index / samplesPerSegment),
      );
      const segmentStart = segment * samplesPerSegment;
      const t = (index - segmentStart) / samplesPerSegment;
      const p0 = points[Math.max(0, segment - 1)];
      const p1 = points[segment];
      const p2 = points[segment + 1];
      const p3 = points[Math.min(points.length - 1, segment + 2)];
      const interpolate = (axis) =>
        0.5 *
        (2 * p1[axis] +
          (-p0[axis] + p2[axis]) * t +
          (2 * p0[axis] - 5 * p1[axis] + 4 * p2[axis] - p3[axis]) * t * t +
          (-p0[axis] + 3 * p1[axis] - 3 * p2[axis] + p3[axis]) * t * t * t);
      return {
        t_ms: (index / (sampleCount - 1)) * path.duration_ms,
        x: interpolate(0),
        y: interpolate(1),
      };
    });
  }
  throw new Error(
    `${command.id} uses unsupported pointer interpolation ${path.interpolation}`,
  );
}

export function buildContinuousScrollPhases(command, viewportHeight) {
  if (
    command?.input_rate_hz !== 120 ||
    !Number.isFinite(viewportHeight) ||
    viewportHeight <= 0
  ) {
    throw new Error(
      "continuous scroll requires the frozen 120 Hz rate and a positive viewport height",
    );
  }
  const { path } = command;
  const forwardEventCount =
    (path.forward_duration_ms / 1000) * command.input_rate_hz;
  const reverseEventCount =
    (path.reverse_duration_ms / 1000) * command.input_rate_hz;
  if (
    !Number.isInteger(forwardEventCount) ||
    !Number.isInteger(reverseEventCount)
  ) {
    throw new Error(
      "continuous scroll durations must contain an integer number of input intervals",
    );
  }
  const distance = path.forward_viewport_heights * viewportHeight;
  return [
    {
      name: "forward",
      duration_ms: path.forward_duration_ms,
      event_count: forwardEventCount,
      interval_ms: path.forward_duration_ms / forwardEventCount,
      delta_y: distance / forwardEventCount,
    },
    {
      name: "pause",
      duration_ms: path.pause_duration_ms,
      event_count: 0,
      interval_ms: null,
      delta_y: 0,
    },
    {
      name: "reverse",
      duration_ms: path.reverse_duration_ms,
      event_count: reverseEventCount,
      interval_ms: path.reverse_duration_ms / reverseEventCount,
      delta_y: -distance / reverseEventCount,
    },
  ];
}

export function buildNormalizedPointerSamples(command) {
  const points = command?.normalized_viewport_points;
  const durationMs = command?.duration_ms;
  const rateHz = command?.rate_hz;
  if (!Array.isArray(points) || points.length < 2) {
    throw new Error(
      `${command?.id ?? "pointer command"} requires at least two normalized points`,
    );
  }
  if (
    !Number.isInteger(durationMs) ||
    durationMs <= 0 ||
    !Number.isInteger(rateHz) ||
    rateHz <= 0
  ) {
    throw new Error(
      `${command?.id ?? "pointer command"} requires a positive integer duration and rate`,
    );
  }
  if (
    points.some(
      (point) =>
        !Array.isArray(point) ||
        point.length !== 2 ||
        point.some(
          (value) => !Number.isFinite(value) || value < 0 || value > 1,
        ),
    )
  ) {
    throw new Error(`${command.id} contains an invalid normalized point`);
  }
  const intervalCount = (durationMs / 1000) * rateHz;
  if (
    !Number.isInteger(intervalCount) ||
    intervalCount % (points.length - 1) !== 0
  ) {
    throw new Error(
      `${command.id} must divide evenly across its normalized path segments`,
    );
  }
  const intervalsPerSegment = intervalCount / (points.length - 1);
  return Array.from({ length: intervalCount + 1 }, (_, index) => {
    const segment = Math.min(
      points.length - 2,
      Math.floor(index / intervalsPerSegment),
    );
    const segmentOffset = index - segment * intervalsPerSegment;
    const fraction = segmentOffset / intervalsPerSegment;
    const start = points[segment];
    const finish = points[segment + 1];
    return {
      t_ms: (index / rateHz) * 1000,
      x: start[0] + (finish[0] - start[0]) * fraction,
      y: start[1] + (finish[1] - start[1]) * fraction,
    };
  });
}

// Perf telemetry rounds rendered/displayed width ratios to three decimals, and
// the fitted page width can differ by a small integer-pixel layout allocation.
// Freeze a 2.5% ceiling for that combined measurement margin. Values below
// 0.975 still represent an undersized preview and must fail closed.
export const electronSettledDensityEpsilon = 0.025;
export const electronSettledDensityMinimum = 1 - electronSettledDensityEpsilon;

export function electronSettledDensityIsCurrent(density) {
  return Number.isFinite(density) && density >= electronSettledDensityMinimum;
}

export function electronZoomPresetMenuReady({
  trigger_expanded: triggerExpanded,
  preset_visible: presetVisible,
}) {
  return triggerExpanded === true && presetVisible === true;
}

export function measureElectronVisibleRasterDensity(
  root,
  viewportBounds,
  devicePixelRatio = 1,
  readBounds = (element) => element.getBoundingClientRect(),
) {
  if (!root || !viewportBounds || !(devicePixelRatio > 0)) return 0;
  return Math.max(
    0,
    ...[...root.querySelectorAll("canvas,img")].map((surface) => {
      const bounds = readBounds(surface);
      const visible =
        bounds.width > 0 &&
        bounds.height > 0 &&
        bounds.right > viewportBounds.left &&
        bounds.bottom > viewportBounds.top &&
        bounds.left < viewportBounds.right &&
        bounds.top < viewportBounds.bottom;
      if (!visible) return 0;
      const renderedWidth =
        surface.tagName === "CANVAS" ? surface.width : surface.naturalWidth;
      return renderedWidth / bounds.width / devicePixelRatio;
    }),
  );
}

export function buildElectronSemanticPanFrames(
  command,
  viewport,
  initialScroll,
) {
  const samples = buildNormalizedPointerSamples(command);
  const first = samples[0];
  return samples.map((sample, sampleIndex) => ({
    sample_index: sampleIndex,
    t_ms: sample.t_ms,
    scroll_left: initialScroll.left - (sample.x - first.x) * viewport.width,
    scroll_top: initialScroll.top - (sample.y - first.y) * viewport.height,
  }));
}

export function evaluateVisiblePageGeometry(samples, tolerance = 0.01) {
  const compared = (samples ?? []).map((sample) => {
    const expectedAspect = sample.expected_width / sample.expected_height;
    const observedAspect = sample.observed_width / sample.observed_height;
    return {
      ...sample,
      expected_aspect: expectedAspect,
      observed_aspect: observedAspect,
      aspect_error: Math.abs(observedAspect - expectedAspect),
      matched:
        Number.isFinite(expectedAspect) &&
        Number.isFinite(observedAspect) &&
        sample.observed_width > 0 &&
        sample.observed_height > 0 &&
        Math.abs(observedAspect - expectedAspect) <= tolerance,
    };
  });
  return {
    passed: compared.length > 0 && compared.every(({ matched }) => matched),
    tolerance,
    samples: compared,
  };
}

export function assessHighZoomPanEvidence(observed) {
  const resources = observed?.probe ?? {};
  const density = observed?.settled_density;
  return {
    visible_tiles_bounded:
      Number.isInteger(resources.max_visible_raster_count) &&
      resources.max_visible_raster_count <= 32 &&
      Number.isFinite(resources.max_visible_raster_pixels) &&
      resources.max_visible_raster_pixels <= 8192 * 8192,
    stale_generations_presented_zero:
      Number.isInteger(resources.stale_visible_surface_frames) &&
      resources.stale_visible_surface_frames === 0,
    settled_density_at_least_1: electronSettledDensityIsCurrent(density),
  };
}

export function assessElectronCachePressureEvidence(observed) {
  const cycles = observed?.cycles ?? [];
  const expectedSequence = observed?.expected_sequence ?? [];
  const expectedCycles = observed?.expected_cycles;
  const cyclesExact =
    Number.isInteger(expectedCycles) &&
    expectedCycles > 0 &&
    cycles.length === expectedCycles &&
    cycles.every(({ cycle }, index) => cycle === index + 1);
  const sequenceExact =
    expectedSequence.length > 0 &&
    cyclesExact &&
    cycles.every(
      ({ actions }) =>
        Array.isArray(actions) &&
        actions.length === expectedSequence.length &&
        actions.every((action, index) => action === expectedSequence[index]),
    );
  const actionsObserved =
    cyclesExact &&
    cycles.every(
      (cycle) =>
        cycle.navigation_current === true &&
        cycle.zoom_current === true &&
        (cycle.pan?.after?.left !== cycle.pan?.before?.left ||
          cycle.pan?.after?.top !== cycle.pan?.before?.top),
    );
  const cancellationObserved =
    cyclesExact &&
    cycles.reduce(
      (total, cycle) => total + (cycle.cancellation_count ?? 0),
      0,
    ) > 0;
  const stalePresentationsZero =
    cyclesExact &&
    cycles.every(({ stale_visible_surface_frames: frames }) => frames === 0);
  const presentationCurrent =
    cyclesExact &&
    cycles.every(({ presentation_current: current }) => current === true);
  const baselineIdentity = observed?.expected_image_identity;
  const imageIdentityExact =
    typeof baselineIdentity === "string" &&
    baselineIdentity.length > 0 &&
    cyclesExact &&
    cycles.every(
      ({ image_identity: identity }) => identity === baselineIdentity,
    );
  const cache = observed?.cache ?? {};
  const bounded = (bytes, limit) =>
    Number.isFinite(bytes) &&
    bytes >= 0 &&
    Number.isFinite(limit) &&
    limit > 0 &&
    bytes <= limit;
  const declaredCacheByteLimitHeld =
    bounded(cache.max_page_url_bytes, cache.page_url_byte_limit) &&
    bounded(cache.max_thumbnail_bytes, cache.thumbnail_byte_limit);
  const decodedByteLimitHeld = bounded(
    cache.max_decoded_render_bytes,
    cache.decoded_render_byte_limit,
  );
  const uploadByteCountRecorded =
    Number.isFinite(observed?.upload_byte_count) &&
    observed.upload_byte_count > 0;
  const exact_fields = {
    cycles_exact: cyclesExact,
    sequence_exact: sequenceExact,
    actions_observed: actionsObserved,
    cancellation_observed: cancellationObserved,
    stale_presentations_zero: stalePresentationsZero,
    presentation_current: presentationCurrent,
    image_identity_exact: imageIdentityExact,
    declared_cache_byte_limit_held: declaredCacheByteLimitHeld,
    decoded_byte_limit_held: decodedByteLimitHeld,
    upload_byte_count_recorded: uploadByteCountRecorded,
  };
  const milestones = {
    "declared-cache-byte-limit-held": declaredCacheByteLimitHeld,
    "decoded-byte-limit-held": decodedByteLimitHeld,
    "upload-byte-count-recorded": uploadByteCountRecorded,
  };
  return {
    exact_fields,
    milestones,
    semantic_passed: Object.entries(exact_fields)
      .filter(([field]) => field !== "upload_byte_count_recorded")
      .every(([, passed]) => passed),
    passed: Object.values(exact_fields).every(Boolean),
  };
}

export function assessElectronEngineeringFitModesEvidence(observations) {
  const modes = ["fit-page", "fit-width"];
  const exactModes =
    Array.isArray(observations) &&
    observations.length === modes.length &&
    modes.every((mode) =>
      observations.some((observation) => observation?.mode === mode),
    );
  const fitStateCurrent =
    exactModes &&
    observations.every(
      (observation) =>
        observation.diagnostics?.zoom_preset === observation.mode &&
        observation.diagnostics?.page_render_ready === true &&
        observation.current_generation_presented === true &&
        observation.settled_for_ms >= 250,
    );
  const visibleTilesBounded =
    exactModes &&
    observations.every(
      ({ visible_raster_resources: resources }) =>
        Number.isInteger(resources?.count) &&
        resources.count > 0 &&
        resources.count <= 32 &&
        Number.isFinite(resources.max_pixels) &&
        resources.max_pixels > 0 &&
        resources.max_pixels <= 8192 * 8192,
    );
  const settledDensityAtLeastOne =
    exactModes &&
    observations.every(({ settled_density: density }) =>
      electronSettledDensityIsCurrent(density),
    );
  const milestones = {
    "fit-state-current": fitStateCurrent,
    "visible-tiles-bounded": visibleTilesBounded,
    "settled-density-at-least-1": settledDensityAtLeastOne,
  };
  return {
    exact_fields: {
      exact_modes: exactModes,
      ...Object.fromEntries(
        Object.entries(milestones).map(([milestone, passed]) => [
          milestone.replaceAll("-", "_"),
          passed,
        ]),
      ),
    },
    milestones,
    passed: Object.values(milestones).every(Boolean),
  };
}

export function assessElectronV4OpenEvidence(command, observation) {
  const milestones = Object.fromEntries(
    (command?.expected_milestones ?? []).map((milestone) => [
      milestone,
      observation?.[milestone.replaceAll("-", "_")] === true,
    ]),
  );
  return {
    exact_fields: Object.fromEntries(
      Object.entries(milestones).map(([milestone, passed]) => [
        milestone.replaceAll("-", "_"),
        passed,
      ]),
    ),
    milestones,
    passed:
      Object.keys(milestones).length > 0 &&
      Object.values(milestones).every(Boolean),
  };
}

export function assessElectronEngineeringCacheRecoveryEvidence(observation) {
  const cycles = observation?.cycles ?? [];
  const expectedCycles = observation?.expected_cycles;
  const cyclesExact =
    expectedCycles === 5 &&
    cycles.length === expectedCycles &&
    cycles.every(
      (cycle, index) =>
        cycle.cycle === index + 1 &&
        cycle.actions?.join("\0") === "zoom\0pan\0fit-page" &&
        cycle.presentation_current === true,
    );
  const cache = observation?.cache ?? {};
  const bounded = (bytes, limit) =>
    Number.isFinite(bytes) &&
    bytes >= 0 &&
    Number.isFinite(limit) &&
    limit > 0 &&
    bytes <= limit;
  const declaredCacheByteLimitHeld =
    bounded(cache.max_page_url_bytes, cache.page_url_byte_limit) &&
    bounded(cache.max_thumbnail_bytes, cache.thumbnail_byte_limit);
  const decodedByteLimitHeld =
    bounded(cache.max_decoded_render_bytes, cache.decoded_render_byte_limit) &&
    cyclesExact &&
    cycles.every(({ decoded_render_bytes: bytes }) =>
      bounded(bytes, cache.decoded_render_byte_limit),
    );
  const rendererResourceSubmissionBytesExact =
    cyclesExact &&
    bounded(
      cache.max_renderer_resource_submission_bytes,
      cache.renderer_resource_submission_byte_limit,
    ) &&
    cycles.every(
      (cycle) =>
        Number.isInteger(cycle.renderer_resource_submission_bytes) &&
        cycle.renderer_resource_submission_bytes > 0 &&
        cycle.renderer_resource_submission_bytes <=
          cache.renderer_resource_submission_byte_limit &&
        cycle.physical_bus_upload_bytes === null,
    );
  const recovery = observation?.recovery ?? {};
  const before = recovery.before ?? {};
  const after = recovery.after ?? {};
  const memoryRecoveryRecorded =
    before.document_count === 1 &&
    before.render_cache_bytes > 0 &&
    before.decoded_render_bytes > 0 &&
    before.renderer_resource_submission_bytes > 0 &&
    before.process_metrics?.totalWorkingSetKiB > 0 &&
    after.document_count === 0 &&
    after.render_cache_bytes === 0 &&
    after.decoded_render_bytes === 0 &&
    after.renderer_resource_submission_bytes === 0 &&
    after.process_metrics?.totalWorkingSetKiB > 0 &&
    recovery.released_render_bytes > 0;
  const milestones = {
    "declared-cache-byte-limit-held": cyclesExact && declaredCacheByteLimitHeld,
    "decoded-byte-limit-held": decodedByteLimitHeld,
    "renderer-resource-submission-bytes-exact":
      rendererResourceSubmissionBytesExact,
    "memory-recovery-recorded": memoryRecoveryRecorded,
  };
  return {
    exact_fields: {
      cycles_exact: cyclesExact,
      physical_bus_upload_unclaimed:
        cyclesExact &&
        cycles.every(({ physical_bus_upload_bytes: bytes }) => bytes === null),
      ...Object.fromEntries(
        Object.entries(milestones).map(([milestone, passed]) => [
          milestone.replaceAll("-", "_"),
          passed,
        ]),
      ),
    },
    milestones,
    passed: Object.values(milestones).every(Boolean),
  };
}

export function buildElectronNativePanReplay(command, viewport, target) {
  const bounds = viewport?.bounds;
  const samples = buildNormalizedPointerSamples(command).map((sample) => ({
    x: sample.x * bounds.width,
    y: (1 - sample.y) * bounds.height,
  }));
  return buildPointerReplay({
    windowId: target.window_id,
    button: 2,
    rateHz: command.rate_hz,
    durationMs: command.duration_ms,
    pdfSamples: samples,
    surface: {
      window_logical_size: viewport.window_logical_size,
      bounds,
      page_height_points: bounds.height,
      pixels_per_point_x: 1,
      pixels_per_point_y: 1,
    },
    windowGeometry: target.geometry,
  });
}

export function assessElectronNativeOpenEvidence(observed) {
  const exact_fields = {
    document_path:
      typeof observed?.requested_path === "string" &&
      observed.requested_path === observed.observed_path,
    native_window_target: observed?.native_window_target_verified === true,
    native_open_action: observed?.native_open_action_completed === true,
    preview_current_generation: observed?.preview_current_generation === true,
    settled_current_generation_250ms:
      observed?.settled_current_generation_250ms === true,
    presentation_current: observed?.presentation_current === true,
  };
  return { exact_fields, passed: Object.values(exact_fields).every(Boolean) };
}

export function buildElectronNativeFileDialogCommands(pdfPath) {
  if (
    typeof pdfPath !== "string" ||
    !pdfPath.startsWith("/") ||
    /[\r\n\0]/.test(pdfPath)
  ) {
    throw new Error("native file chooser requires a safe absolute PDF path");
  }
  return [
    ["key", "--clearmodifiers", "ctrl+l"],
    ["type", "--clearmodifiers", "--delay", "1", pdfPath],
  ];
}

export function buildElectronNativeFileDialogOpenClick(dialogTarget) {
  const geometry = dialogTarget?.geometry;
  if (
    !/^\d+$/.test(String(dialogTarget?.window_id)) ||
    dialogTarget?.title !== "Open PDFs" ||
    geometry?.visible !== true ||
    !Number.isInteger(geometry.x) ||
    !Number.isInteger(geometry.y) ||
    !Number.isInteger(geometry.width) ||
    !Number.isInteger(geometry.height) ||
    geometry.width < 800 ||
    geometry.height < 600
  ) {
    throw new Error("native PDF chooser geometry is not exact enough to click Open");
  }
  const inset = { right: 50, bottom: 40 };
  return {
    x: geometry.x + geometry.width - inset.right,
    y: geometry.y + geometry.height - inset.bottom,
    button: 1,
    input_window_id: String(dialogTarget.window_id),
    dialog_geometry: { ...geometry },
    open_button_center_inset_px: inset,
  };
}

export function assessElectronNativePanEvidence(observed) {
  const pan = assessHighZoomPanEvidence(observed);
  const exact_fields = {
    sample_count:
      Number.isInteger(observed?.expected_sample_count) &&
      observed.expected_sample_count === observed.acknowledged_sample_count,
    timing: observed?.timing_within_tolerance === true,
    presentation_current: observed?.presentation_current === true,
    ...pan,
  };
  return { exact_fields, passed: Object.values(exact_fields).every(Boolean) };
}

export function assessElectronNativeTransformEvidence(observed) {
  const exact_fields = {
    move_sample_count: observed?.move_sample_count_matches === true,
    resize_sample_count: observed?.resize_sample_count_matches === true,
    move_timing: observed?.move_timing_within_tolerance === true,
    resize_timing: observed?.resize_timing_within_tolerance === true,
    hit_test_selected: observed?.hit_test_selected === true,
    move_history_delta: observed?.move_history_delta === 1,
    resize_history_delta: observed?.resize_history_delta === 1,
    geometry: observed?.geometry_exact === true,
    presentation_current: observed?.presentation_current === true,
  };
  return { exact_fields, passed: Object.values(exact_fields).every(Boolean) };
}

export function assessCloseReopenEvidence(observed) {
  return {
    document_resources_released:
      observed?.closed?.document_path === null &&
      observed?.closed?.tab_count === 0 &&
      observed?.closed?.render_cache_bytes === 0 &&
      observed?.closed?.thumbnail_cache_bytes === 0 &&
      observed?.closed?.document_canvas_count === 0,
    memory_recovery_recorded:
      observed?.memory?.before_close !== null &&
      observed?.memory?.after_close !== null,
    document_reopened:
      observed?.reopened?.document_path_matches === true &&
      observed?.reopened?.page_render_ready === true,
    settled_current_generation_250ms:
      observed?.reopened?.settled_for_ms >= 250 &&
      observed?.reopened?.current_surface_ready === true,
  };
}

export function validateExpandedScenarioFixture(pdf, contract) {
  if (!contract) return null;
  if (pdf.sha256 === contract.fixture_sha256) return null;
  return `${contract.fixture_id} requires PDF SHA-256 ${contract.fixture_sha256}; received ${pdf.sha256}`;
}

export function electronComparisonMetadata(
  workload,
  scenario,
  inputLane,
  iterations = [],
) {
  const metadata = runnerComparisonMetadata(workload, "electron", scenario);
  if (inputLane !== nativeX11InputLane) return metadata;
  const allExact =
    iterations.length > 0 &&
    iterations.every(
      (iteration) =>
        iteration.success === true &&
        iteration.renderer?.expanded_comparison?.exact_manifest_replay === true,
    );
  return {
    ...metadata,
    execution_lane: nativeX11InputLane,
    scenario_status: "supported-diagnostic",
    diagnostic_timing_eligible: allExact,
    decision_timing_eligible: allExact && metadata.feature_coverage.ready,
    blocked_reason: allExact
      ? metadata.feature_coverage.ready
        ? null
        : "full-comparison-feature-coverage-incomplete"
      : "native-replay-has-not-passed-exact-milestones-and-timing",
  };
}

export function formatFixtureAccessError(error, contract, path) {
  const absent =
    error?.code === "ENOENT" ||
    /ENOENT|no such file/i.test(error?.message ?? "");
  const lockedPublic = [
    "nasa-apollo-summary-526-v1",
    "usgs-usa-geology-sheet-v1",
  ].includes(contract?.fixture_id);
  if (absent && lockedPublic) {
    return `BLOCKED locked corpus ${contract.fixture_id} is absent at ${path}; fetch and verify the exact byte count and SHA-256 before launch`;
  }
  return error?.message ?? String(error);
}

export function comparisonMilestonesSucceeded(events, contract) {
  if (!contract) return true;
  const observed = new Set(
    events
      .filter(({ event }) => event === "comparison-milestone")
      .map(
        ({ command_id: commandId, milestone }) => `${commandId}\0${milestone}`,
      ),
  );
  const milestonesPassed = contract.commands.every((command) =>
    command.expected_milestones.every((milestone) =>
      observed.has(`${command.id}\0${milestone}`),
    ),
  );
  if (!milestonesPassed || contract.require_exact_fields !== true)
    return milestonesPassed;
  const exactCommands = new Set(
    events
      .filter(
        ({ event, passed }) =>
          event === "comparison-command-exact-state" && passed === true,
      )
      .map(({ command_id: commandId }) => commandId),
  );
  return contract.command_ids.every((commandId) =>
    exactCommands.has(commandId),
  );
}

const electronV4ComponentCommandMappings = Object.freeze({
  "small-shell-open": Object.freeze({
    "open-pdf": Object.freeze([
      {
        command_id: "small:launch-cold",
        source_command_id: "small:launch-cold",
      },
      {
        command_id: "small:open-settle",
        source_command_id: "small:open-settle",
      },
    ]),
  }),
  "nasa-long-document": Object.freeze({
    "open-pdf": Object.freeze([
      { command_id: "nasa:open-settle", source_command_id: "nasa:open-settle" },
    ]),
    "viewer-layout": Object.freeze([
      {
        command_id: "viewer:layout-single",
        source_command_id: "viewer:layout-single",
      },
      {
        command_id: "viewer:layout-continuous",
        source_command_id: "viewer:layout-continuous",
      },
    ]),
    "page-navigation": Object.freeze([
      {
        command_id: "viewer:navigate-normalized",
        source_command_id: "viewer:navigate-normalized",
      },
    ]),
    "continuous-scroll": Object.freeze([
      {
        command_id: "viewer:continuous-scroll",
        source_command_id: "viewer:continuous-scroll",
      },
    ]),
    "cache-pressure": Object.freeze([
      {
        command_id: "nasa:cache-pressure",
        source_command_id: "viewer:cache-pressure",
      },
    ]),
    "close-reopen": Object.freeze([
      {
        command_id: "viewer:close-recover-reopen",
        source_command_id: "viewer:close-recover-reopen",
      },
    ]),
  }),
  "engineering-sheet": Object.freeze({
    "open-pdf": Object.freeze([
      {
        command_id: "engineering:open-settle",
        source_command_id: "engineering:open-settle",
      },
    ]),
    zoom: Object.freeze([
      { command_id: "engineering:fit-modes", source_command_id: null },
      {
        command_id: "engineering:zoom-sequence",
        source_command_id: "viewer:zoom-sequence",
      },
    ]),
    "high-zoom-pan": Object.freeze([
      { command_id: "engineering:pan", source_command_id: "viewer:pan-usgs" },
    ]),
    "cache-pressure": Object.freeze([
      {
        command_id: "engineering:cache-recovery",
        source_command_id: "viewer:cache-pressure",
      },
    ]),
    "fit-modes": Object.freeze([
      {
        command_id: "engineering:fit-modes",
        source_command_id: "engineering:fit-modes",
      },
    ]),
    "cache-pressure-recovery": Object.freeze([
      {
        command_id: "engineering:cache-recovery",
        source_command_id: "engineering:cache-recovery",
      },
    ]),
  }),
  "dense-mixed-editing": Object.freeze({
    "annotation-create": Object.freeze([
      {
        command_id: "rectangle:create-sparse",
        source_command_id: "rectangle:create-sparse",
      },
      { command_id: "highlight:create", source_command_id: "highlight:create" },
    ]),
    "annotation-transform": Object.freeze([
      {
        command_id: "rectangle:select-move-resize",
        source_command_id: "rectangle:select-move-resize",
      },
    ]),
    "annotation-properties-history": Object.freeze([
      {
        command_id: "rectangle:properties-history",
        source_command_id: "rectangle:properties-history",
      },
    ]),
    "editor-create": Object.freeze([
      { command_id: "text:create", source_command_id: "text:create" },
      { command_id: "length:set-scale", source_command_id: "length:set-scale" },
      { command_id: "length:create", source_command_id: "length:create" },
      { command_id: "image:create", source_command_id: "image:create" },
    ]),
    "editor-workload": Object.freeze(
      [
        "rectangle:create-sparse",
        "rectangle:select-move-resize",
        "rectangle:properties-history",
        "rectangle:repeat-dense",
        "highlight:create",
        "highlight:edit-history",
        "text:create",
        "text:edit-resize-history",
        "length:set-scale",
        "length:create",
        "length:edit-endpoint-history",
        "image:create",
        "image:resize-history",
      ].map((commandId) => ({
        command_id: commandId,
        source_command_id: commandId,
      })),
    ),
  }),
  persistence: Object.freeze({
    "persistence-workload": Object.freeze(
      electronPersistenceCommandIds.map((commandId) => ({
        command_id: commandId,
        source_command_id: commandId,
      })),
    ),
  }),
  "usgs-large-sheet-stress": Object.freeze({
    "open-pdf": Object.freeze([
      { command_id: "stress:usgs-open", source_command_id: "stress:usgs-open" },
    ]),
    zoom: Object.freeze([
      {
        command_id: "stress:usgs-zoom",
        source_command_id: "viewer:zoom-sequence",
      },
    ]),
    "high-zoom-pan": Object.freeze([
      { command_id: "stress:usgs-pan", source_command_id: "viewer:pan-usgs" },
    ]),
  }),
});

function canonicalEvidence(value) {
  if (Array.isArray(value)) return value.map(canonicalEvidence);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonicalEvidence(value[key])]),
    );
  }
  return value;
}

function evidenceSha256(value) {
  return createHash("sha256")
    .update(JSON.stringify(canonicalEvidence(value)))
    .digest("hex");
}

function commandSemanticProjection(command) {
  if (!command) return null;
  const {
    id: _id,
    fixture_id: _fixtureId,
    expected_milestones: _expectedMilestones,
    resource_observation: _resourceObservation,
    ...semantic
  } = command;
  return semantic;
}

function v4CommandSemanticsMatch(target, source) {
  const targetSemantic = commandSemanticProjection(target);
  const sourceSemantic = commandSemanticProjection(source);
  if (!targetSemantic || !sourceSemantic) return false;
  return Object.entries(targetSemantic).every(
    ([field, value]) =>
      JSON.stringify(sourceSemantic[field]) === JSON.stringify(value),
  );
}

export function createElectronV4ExecutionContext({
  parentContract,
  parentWorkload,
  componentScenario,
  componentContract,
  inputLane = cdpDiagnosticInputLane,
}) {
  const localEngineeringComponent =
    parentContract?.scenario === "engineering-sheet" &&
    Object.hasOwn(localV4EngineeringComponents, componentScenario);
  if (
    !parentContract?.current_runner_components?.includes(componentScenario) &&
    !localEngineeringComponent
  ) {
    throw new Error(
      `${componentScenario} is not listed for v4 parent ${parentContract?.scenario ?? "unknown"}`,
    );
  }
  const configuredMappings =
    electronV4ComponentCommandMappings[parentContract.scenario]?.[
      componentScenario
    ];
  const parentCommandIds = localEngineeringComponent
    ? [localV4EngineeringComponents[componentScenario]]
    : parentContract.component_command_ids?.[componentScenario];
  if (!configuredMappings || !Array.isArray(parentCommandIds)) {
    throw new Error(
      `${componentScenario} has no Electron v4 command mapping for ${parentContract.scenario}`,
    );
  }
  const mappings = parentCommandIds.map(
    (commandId) =>
      configuredMappings.find(
        ({ command_id: configuredCommandId }) =>
          configuredCommandId === commandId,
      ) ?? { command_id: commandId, source_command_id: commandId },
  );
  const parentSource =
    parentContract.lane === "representative-inference"
      ? (parentWorkload.journeys.find(
          ({ id }) => id === parentContract.scenario.replace(/$/, "-v1"),
        ) ??
        parentWorkload.journeys.find(
          ({ commands }) =>
            commands.map(({ id }) => id).join("\0") ===
            parentContract.command_ids.join("\0"),
        ))
      : parentWorkload.stress_lanes.find(
          ({ commands }) =>
            commands.map(({ id }) => id).join("\0") ===
            parentContract.command_ids.join("\0"),
        );
  if (!parentSource)
    throw new Error(
      `v4 parent source is missing for ${parentContract.scenario}`,
    );
  const localCommand = localEngineeringComponent
    ? parentContract.commands.find(
        ({ id }) => id === localV4EngineeringComponents[componentScenario],
      )
    : null;
  const syntheticCommands =
    componentContract == null
      ? localCommand == null
        ? componentScenario === "open-pdf"
          ? parentCommandIds
              .map((commandId) =>
                parentContract.commands.find(({ id }) => id === commandId),
              )
              .filter(Boolean)
          : []
        : [localCommand]
      : [];
  const executionContract =
    componentContract == null
      ? syntheticCommands.length === 0
        ? null
        : {
            manifest_id: parentContract.manifest_id,
            input_lane: inputLane,
            fixture_id: parentContract.fixture_id,
            fixture_sha256: parentContract.fixture_sha256,
            command_ids: syntheticCommands.map(({ id }) => id),
            require_exact_fields: true,
            commands: syntheticCommands,
          }
      : {
          ...componentContract,
          fixture_id: parentContract.fixture_id,
          fixture_sha256: parentContract.fixture_sha256,
        };
  const v4ExecutionContract =
    componentScenario === "continuous-scroll" && executionContract
      ? {
          ...executionContract,
          commands: executionContract.commands.map((command) => {
            const mapping = mappings.find(
              ({ source_command_id: sourceCommandId }) =>
                sourceCommandId === command.id,
            );
            const target = parentContract.commands.find(
              ({ id }) => id === mapping?.command_id,
            );
            return target
              ? {
                  ...command,
                  expected_milestones: [...target.expected_milestones],
                }
              : command;
          }),
        }
      : executionContract;
  const remainingParentBlockers = (
    parentContract.blocked_commands ?? []
  ).filter(({ command_id: commandId }) => commandId !== localCommand?.id);
  return {
    provenance: {
      protocol_version: protocolVersionV4,
      scenario_contract_version: scenarioContractVersionV4,
      manifest_id: parentContract.manifest_id,
      manifest_artifact_sha256:
        comparisonWorkloadArtifactHashV4(parentWorkload),
      parent_scenario: parentContract.scenario,
      parent_lane: parentContract.lane,
      parent_journey_id:
        parentContract.lane === "representative-inference"
          ? parentSource.id
          : null,
      parent_stress_lane_id:
        parentContract.lane === "stress-diagnostic" ? parentSource.id : null,
      component_scenario: componentScenario,
      component_listed_for_parent: true,
      fixture_id: parentContract.fixture_id,
      fixture_sha256: parentContract.fixture_sha256,
      parent_blocked_commands: remainingParentBlockers,
    },
    parent_contract: parentContract,
    component_contract: v4ExecutionContract,
    execution_contract: v4ExecutionContract,
    command_mappings: mappings,
    remaining_parent_blocked_commands: remainingParentBlockers,
    local_v4_component: localEngineeringComponent,
  };
}

function v4MilestoneProven(
  milestone,
  sourceCommandId,
  rawMilestones,
  sourceResult,
  semanticPresentationEligible = false,
) {
  if (milestone === "visible-raster-readiness-observed") {
    const probe = sourceResult?.observed?.probe;
    return (
      Number.isInteger(probe?.frame_count) &&
      probe.frame_count > 0 &&
      Number.isInteger(probe.blank_current_generation_frames) &&
      probe.blank_current_generation_frames >= 0 &&
      probe.blank_current_generation_frames <= probe.frame_count
    );
  }
  if (rawMilestones.has(`${sourceCommandId}\0${milestone}`)) return true;
  const imageReceipt = sourceResult?.renderer_resource_submission_receipt;
  if (milestone === "decoded-payload-bytes-exact") {
    return imageReceipt?.decoded_payload_bytes === 786432;
  }
  if (milestone === "renderer-resource-submission-bytes-exact") {
    if (imageReceipt?.renderer_resource_submission_bytes === 786432)
      return true;
    const cycles = sourceResult?.observation?.cycles;
    const limit =
      sourceResult?.observation?.cache?.renderer_resource_submission_byte_limit;
    return (
      Array.isArray(cycles) &&
      cycles.length === 5 &&
      Number.isFinite(limit) &&
      limit > 0 &&
      cycles.every(
        (cycle) =>
          Number.isInteger(cycle.renderer_resource_submission_bytes) &&
          cycle.renderer_resource_submission_bytes > 0 &&
          cycle.renderer_resource_submission_bytes <= limit &&
          cycle.physical_bus_upload_bytes === null,
      )
    );
  }
  if (
    ["bitmap-presented-from-decoded-payload", "annotation-presented"].includes(
      milestone,
    )
  ) {
    if (imageReceipt?.presented_after_native_input === true) return true;
    const presentation =
      sourceResult?.presentation ?? sourceResult?.observation?.presentation;
    return (
      semanticPresentationEligible &&
      imageReceipt?.renderer_resource_submission_observed === true &&
      presentation?.element_present === true &&
      presentation?.visible === true &&
      presentation?.animation_frames_after_markup >= 2
    );
  }
  if (milestone === "timestamped-input-complete") {
    return (
      sourceResult?.exact_fields?.timing === true ||
      sourceResult?.input?.timing?.within_tolerance === true
    );
  }
  return sourceResult?.exact_fields?.[milestone.replaceAll("-", "_")] === true;
}

const electronV4SemanticCdpComponents = new Set([
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
]);

export function electronV4ComponentLaneEligible(componentScenario, inputLane) {
  return (
    inputLane === nativeX11InputLane ||
    (inputLane === cdpDiagnosticInputLane &&
      electronV4SemanticCdpComponents.has(componentScenario))
  );
}

export function buildElectronV4ComponentEvidence(
  context,
  events,
  expandedComparison,
) {
  const rawMilestones = new Set(
    (events ?? [])
      .filter(({ event }) => event === "comparison-milestone")
      .map(
        ({ command_id: commandId, milestone }) => `${commandId}\0${milestone}`,
      ),
  );
  const componentContract = context.component_contract;
  const observedInputLane =
    expandedComparison?.input_lane ??
    (componentContract?.input_lane === "semantic-diagnostic"
      ? cdpDiagnosticInputLane
      : componentContract?.input_lane);
  const laneEligible = electronV4ComponentLaneEligible(
    context.provenance.component_scenario,
    observedInputLane,
  );
  const semanticPresentationEligible =
    context.provenance.component_scenario === "editor-workload" &&
    observedInputLane === cdpDiagnosticInputLane;
  const semanticResults = expandedComparison?.command_results;
  const semanticCachePressureReplay =
    context.provenance.component_scenario === "cache-pressure" &&
    expandedComparison?.semantic_manifest_replay === true &&
    Array.isArray(semanticResults) &&
    semanticResults.length > 0 &&
    semanticResults.every(({ semantic_passed: passed }) => passed === true);
  const aggregateExecutionPassed =
    laneEligible &&
    (expandedComparison?.exact_manifest_replay === true ||
      semanticCachePressureReplay) &&
    (comparisonMilestonesSucceeded(events, componentContract) ||
      semanticCachePressureReplay);
  const sourceCommands = new Map(
    (componentContract?.commands ?? []).map((command) => [command.id, command]),
  );
  const sourceResults = new Map(
    (expandedComparison?.command_results ?? []).map((result) => [
      result.command_id,
      result,
    ]),
  );
  const targetCommands = new Map(
    context.parent_contract.commands.map((command) => [command.id, command]),
  );
  const commandReceipts = context.command_mappings.map(
    ({ command_id: commandId, source_command_id: sourceCommandId }) => {
      const targetCommand = targetCommands.get(commandId);
      const sourceCommand = sourceCommands.get(sourceCommandId);
      const semanticMatch =
        sourceCommandId !== null &&
        v4CommandSemanticsMatch(targetCommand, sourceCommand);
      const mappingStatus = semanticMatch ? "exact-semantic-map" : "unmapped";
      const sourceResult = sourceResults.get(sourceCommandId);
      const sourceEvidenceSha256 = evidenceSha256({
        raw_v3_events: (events ?? []).filter(
          ({ command_id: eventCommandId, event }) =>
            eventCommandId === sourceCommandId &&
            [
              "comparison-milestone",
              "comparison-command-exact-state",
              "comparison-command-evidence",
            ].includes(event),
        ),
        expanded_command_result: sourceResult ?? null,
      });
      const sourceExecutionObserved = sourceResult !== undefined;
      const provenMilestones =
        semanticMatch && laneEligible && sourceExecutionObserved
          ? targetCommand.expected_milestones.filter((milestone) =>
              v4MilestoneProven(
                milestone,
                sourceCommandId,
                rawMilestones,
                sourceResult,
                semanticPresentationEligible,
              ),
            )
          : [];
      const missingMilestones = targetCommand.expected_milestones.filter(
        (milestone) => !provenMilestones.includes(milestone),
      );
      const commandExecutionPassed =
        semanticMatch &&
        laneEligible &&
        sourceExecutionObserved &&
        missingMilestones.length === 0;
      const receiptEvidence = {
        parent_scenario: context.provenance.parent_scenario,
        component_scenario: context.provenance.component_scenario,
        command_id: commandId,
        source_command_id: sourceCommandId,
        source_evidence_sha256: sourceEvidenceSha256,
        mapping_status: mappingStatus,
        component_execution_passed: commandExecutionPassed,
        proven_milestones: provenMilestones,
        missing_milestones: missingMilestones,
      };
      return {
        command_id: commandId,
        live: true,
        passed: commandExecutionPassed,
        evidence_sha256: evidenceSha256({
          parent_scenario: receiptEvidence.parent_scenario,
          component_scenario: receiptEvidence.component_scenario,
          command_id: receiptEvidence.command_id,
          source_command_id: receiptEvidence.source_command_id,
          mapping_status: receiptEvidence.mapping_status,
          component_execution_passed:
            receiptEvidence.component_execution_passed,
          proven_milestones: receiptEvidence.proven_milestones,
          missing_milestones: receiptEvidence.missing_milestones,
        }),
        ...receiptEvidence,
      };
    },
  );
  const parentBlockedCommandReceipts =
    context.remaining_parent_blocked_commands.map((blocker) => {
      const receiptEvidence = {
        parent_scenario: context.provenance.parent_scenario,
        component_scenario: null,
        command_id: blocker.command_id,
        source_command_id: null,
        mapping_status: "unmapped",
        component_execution_passed: false,
        proven_milestones: [],
        missing_milestones:
          context.parent_contract.commands.find(
            ({ id }) => id === blocker.command_id,
          )?.expected_milestones ?? [],
        blocker: blocker.reason,
      };
      return {
        command_id: blocker.command_id,
        live: false,
        passed: false,
        evidence_sha256: evidenceSha256(receiptEvidence),
        ...receiptEvidence,
      };
    });
  const knownBaselineDefectId =
    classifyElectronEngineeringZoomBaselineDefectV6({
      implementation: "electron",
      journey: context.provenance.parent_scenario,
      component: context.provenance.component_scenario,
      receipts: commandReceipts,
      source_command_results: expandedComparison?.command_results,
    });
  return {
    ...context.provenance,
    evidence_class: "live-v3-component-to-v4-command-receipts",
    raw_v3_events_retained: true,
    component_execution_passed:
      aggregateExecutionPassed &&
      commandReceipts.length > 0 &&
      commandReceipts.every(({ passed }) => passed),
    component_receipts_passed:
      commandReceipts.length > 0 &&
      commandReceipts.every(({ passed }) => passed),
    parent_blocked_commands: context.remaining_parent_blocked_commands,
    parent_blocked_command_receipts: parentBlockedCommandReceipts,
    parent_execution_eligible: false,
    unmapped_command_ids: [
      ...commandReceipts
        .filter(({ mapping_status: status }) => status !== "exact-semantic-map")
        .map(({ command_id: commandId }) => commandId),
      ...parentBlockedCommandReceipts.map(
        ({ command_id: commandId }) => commandId,
      ),
    ],
    command_receipts: commandReceipts,
    known_baseline_defect_id: knownBaselineDefectId,
  };
}

function usage() {
  return `Usage:
  node electron-runner.mjs --scenario <name> --pdf <file> [--pdf <file> ...] [options]

Required:
  --scenario <name>       Includes editor-create for Text, Length, and Image evidence
  --pdf <file>            Ordered PDF fixture; repeat four times for multi-document-session

Options:
  --v4-scenario <id>     Bind the v3 component run to an explicit v4 parent scenario
  --v5-scenario <id>     Bind a hard component to an explicit v5 parent scenario
  --v6-scenario <id>     Mark an inherited v4 native component as v6 benefit-eligible
  --iterations <count>    Independent process runs (default: 3)
  --output <file>         JSON report path (default: beside this runner)
  --timeout-ms <ms>       Timeout for each iteration (default: 120000)
  --electron <file>       Override the Electron executable
  --input-lane <lane>     cdp-input-diagnostic (default) or native-x11-xtest
  -h, --help              Show this help
`;
}

function parsePositiveInteger(value, option) {
  if (!/^\d+$/.test(value ?? "") || Number(value) < 1) {
    throw new Error(`${option} must be a positive integer`);
  }
  return Number(value);
}

export function parseElectronRunnerArguments(argv) {
  const options = {
    iterations: 3,
    timeoutMs: defaultTimeoutMs,
    electron: defaultElectron,
    inputLane: cdpDiagnosticInputLane,
    pdfs: [],
  };
  const valueOptions = new Set([
    "--scenario",
    "--v4-scenario",
    "--v5-scenario",
    "--v6-scenario",
    "--pdf",
    "--iterations",
    "--output",
    "--timeout-ms",
    "--electron",
    "--input-lane",
  ]);
  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index];
    if (option === "-h" || option === "--help") {
      options.help = true;
      continue;
    }
    if (!valueOptions.has(option)) throw new Error(`unknown option: ${option}`);
    const value = argv[++index];
    if (value === undefined || value.startsWith("--"))
      throw new Error(`${option} requires a value`);
    if (option === "--scenario") options.scenario = value;
    if (option === "--v4-scenario") options.v4Scenario = value;
    if (option === "--v5-scenario") options.v5Scenario = value;
    if (option === "--v6-scenario") options.v6Scenario = value;
    if (option === "--pdf") options.pdfs.push(resolve(value));
    if (option === "--iterations")
      options.iterations = parsePositiveInteger(value, option);
    if (option === "--output") options.output = resolve(value);
    if (option === "--timeout-ms")
      options.timeoutMs = parsePositiveInteger(value, option);
    if (option === "--electron") options.electron = resolve(value);
    if (option === "--input-lane") options.inputLane = value;
  }
  if (options.help) return options;
  if (
    !allowedScenarios.has(options.scenario) &&
    !electronV5ComponentScenarios.has(options.scenario) &&
    !Object.hasOwn(localV4EngineeringComponents, options.scenario)
  ) {
    throw new Error(
      `--scenario must be one of ${[...allowedScenarios].join(", ")}`,
    );
  }
  if (options.pdfs.length === 0) throw new Error("--pdf is required");
  if (
    options.scenario === multiDocumentScenarioV5 &&
    options.pdfs.length !== 4
  ) {
    throw new Error(
      "multi-document-session requires exactly four ordered --pdf values",
    );
  }
  if (
    options.scenario !== multiDocumentScenarioV5 &&
    options.pdfs.length !== 1
  ) {
    throw new Error(
      "only multi-document-session accepts repeated --pdf values",
    );
  }
  options.pdf = options.pdfs[0];
  if (
    ![cdpDiagnosticInputLane, nativeX11InputLane].includes(options.inputLane)
  ) {
    throw new Error(
      "--input-lane must be cdp-input-diagnostic or native-x11-xtest",
    );
  }
  if (
    options.inputLane === nativeX11InputLane &&
    !electronNativeX11Scenarios.has(options.scenario) &&
    !electronV5ComponentScenarios.has(options.scenario)
  ) {
    throw new Error(
      `native-x11-xtest is not implemented for ${options.scenario}`,
    );
  }
  if (
    Object.hasOwn(localV4EngineeringComponents, options.scenario) &&
    options.v4Scenario !== "engineering-sheet"
  ) {
    throw new Error(
      `${options.scenario} requires --v4-scenario engineering-sheet`,
    );
  }
  if (options.v4Scenario && options.v5Scenario) {
    throw new Error("--v4-scenario and --v5-scenario are mutually exclusive");
  }
  if (options.v5Scenario && options.v6Scenario) {
    throw new Error("--v5-scenario and --v6-scenario are mutually exclusive");
  }
  if (options.v6Scenario && !options.v4Scenario) {
    throw new Error(
      "--v6-scenario requires the inherited --v4-scenario execution path",
    );
  }
  if (options.v6Scenario && options.v6Scenario !== options.v4Scenario) {
    throw new Error(
      "--v6-scenario and --v4-scenario must name the same parent journey",
    );
  }
  if (options.v6Scenario && options.inputLane !== nativeX11InputLane) {
    throw new Error("--v6-scenario requires --input-lane native-x11-xtest");
  }
  const requireCommonDamageObserver =
    options.v6Scenario || process.env.BP_PERF_COMMON_DAMAGE_OBSERVER === "1";
  if (requireCommonDamageObserver) {
    const definition = representativeScenarioDefinitionsV6[options.v6Scenario];
    if (!definition) {
      throw new Error(`unknown --v6-scenario ${options.v6Scenario}`);
    }
    if (!definition.benefit_components.includes(options.scenario)) {
      throw new Error(
        `${options.scenario} is not a benefit-eligible component of v6 scenario ${options.v6Scenario}`,
      );
    }
  }
  if (
    options.scenario === multiDocumentScenarioV5 &&
    options.v5Scenario &&
    options.v5Scenario !== multiDocumentScenarioV5
  ) {
    throw new Error(
      "multi-document-session requires --v5-scenario multi-document-session when supplied",
    );
  }
  if (
    ["native-property-edit-undo", "native-snap-transform-120hz"].includes(
      options.scenario,
    ) &&
    options.v5Scenario !== "dense-mixed-editing"
  ) {
    throw new Error(
      `${options.scenario} requires --v5-scenario dense-mixed-editing`,
    );
  }
  if (
    options.scenario === dynamicFidelityScenarioV5 &&
    options.v5Scenario !== "nasa-long-document"
  ) {
    throw new Error(
      `${dynamicFidelityScenarioV5} requires --v5-scenario nasa-long-document`,
    );
  }
  options.output ??= resolve(
    performanceDirectory,
    `electron-${options.v6Scenario ? `v6-${options.v6Scenario}-` : options.v5Scenario ? `${options.v5Scenario}-` : options.v4Scenario ? `${options.v4Scenario}-` : ""}${options.scenario}.json`,
  );
  return options;
}

async function fileProvenance(path) {
  const metadata = await stat(path);
  if (!metadata.isFile()) throw new Error(`not a file: ${path}`);
  const hash = createHash("sha256");
  await new Promise((resolvePromise, rejectPromise) => {
    const stream = createReadStream(path);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("error", rejectPromise);
    stream.on("end", resolvePromise);
  });
  return {
    path,
    bytes: metadata.size,
    sha256: hash.digest("hex"),
    modified_at: metadata.mtime.toISOString(),
  };
}

async function optionalCommand(command, args, cwd) {
  try {
    const { stdout } = await execFileAsync(command, args, {
      cwd,
      encoding: "utf8",
      timeout: 5_000,
      maxBuffer: 256_000,
    });
    return stdout.trim() || null;
  } catch {
    return null;
  }
}

async function collectProvenance(electron) {
  const cpuList = cpus();
  const [
    macosVersion,
    gitRevision,
    gitStatus,
    nvidiaGpu,
    vulkanSummary,
    displayMode,
  ] = await Promise.all([
    optionalCommand("/usr/bin/sw_vers", ["-productVersion"]),
    optionalCommand("/usr/bin/git", ["rev-parse", "HEAD"], repositoryDirectory),
    optionalCommand(
      "/usr/bin/git",
      ["status", "--porcelain=v1", "--untracked-files=all"],
      repositoryDirectory,
    ),
    optionalCommand("nvidia-smi", [
      "--query-gpu=name,uuid,driver_version,memory.total",
      "--format=csv,noheader,nounits",
    ]),
    optionalCommand("vulkaninfo", ["--summary"]),
    optionalCommand("xrandr", ["--current"]),
  ]);
  return {
    captured_at: new Date().toISOString(),
    host: {
      hostname: hostname(),
      os_type: type(),
      platform: platform(),
      os_release: release(),
      macos_version: macosVersion,
      architecture: arch(),
      logical_cpu_count: cpuList.length,
      cpu_model: cpuList[0]?.model ?? null,
      total_memory_bytes: totalmem(),
      free_memory_bytes_at_start: freemem(),
      display_mode: displayMode,
      nvidia_gpu: nvidiaGpu,
      vulkan_summary: vulkanSummary,
    },
    runtime: {
      runner: "electron-runner.mjs",
      node: process.version,
      node_versions: process.versions,
      sample_interval_ms: sampleIntervalMs,
      git_revision: gitRevision,
      git_status_sha256: createHash("sha256")
        .update(gitStatus ?? "")
        .digest("hex"),
      electron: await fileProvenance(electron),
    },
  };
}

function percentile(values, fraction) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((left, right) => left - right);
  const position = (sorted.length - 1) * fraction;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  return lower === upper
    ? sorted[lower]
    : sorted[lower] + (sorted[upper] - sorted[lower]) * (position - lower);
}

function numericSummary(values) {
  const valid = values.filter(Number.isFinite);
  if (valid.length === 0) return null;
  return {
    count: valid.length,
    min: Math.min(...valid),
    median: percentile(valid, 0.5),
    mean: valid.reduce((sum, value) => sum + value, 0) / valid.length,
    p95: percentile(valid, 0.95),
    max: Math.max(...valid),
  };
}

function summarizeFrames(intervals) {
  return {
    interval_ms: numericSummary(intervals),
    over_8_33_ms: intervals.filter((value) => value > 8.33).length,
    over_16_67_ms: intervals.filter((value) => value > 16.67).length,
    over_33_33_ms: intervals.filter((value) => value > 33.33).length,
  };
}

async function availablePort() {
  const configured = Number(process.env.BP_ELECTRON_CDP_PORT);
  if (Number.isInteger(configured) && configured > 0 && configured < 65536) {
    return configured;
  }
  // The runner executes sequentially. Avoid a bind probe because restricted
  // Codex sandboxes can deny listen(2) even though an Electron child can use
  // its own loopback CDP listener.
  return 42000 + (process.pid % 1000);
}

async function waitForTarget(port, child, output, deadlineMs) {
  const deadline = performance.now() + deadlineMs;
  while (performance.now() < deadline) {
    if (child.exitCode !== null) {
      throw new Error(
        `Electron exited before CDP was ready (${child.exitCode}).\n${output}`,
      );
    }
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/list`);
      if (response.ok) {
        const targets = await response.json();
        const page = targets.find((target) => target.type === "page");
        if (page?.webSocketDebuggerUrl) {
          const versionResponse = await fetch(
            `http://127.0.0.1:${port}/json/version`,
          );
          const version = versionResponse.ok
            ? await versionResponse.json()
            : {};
          return {
            page,
            browserWebSocketDebuggerUrl: version.webSocketDebuggerUrl ?? null,
          };
        }
      }
    } catch {
      // Electron has not opened the debugging endpoint yet.
    }
    await delay(20);
  }
  throw new Error(
    `Timed out waiting for Electron CDP on port ${port}.\n${output}`,
  );
}

async function createCdpClient(url) {
  const socket = new WebSocket(url);
  const pending = new Map();
  const eventWaiters = new Map();
  let nextId = 1;
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(String(event.data));
    if (!message.id) {
      const waiters = eventWaiters.get(message.method) ?? [];
      eventWaiters.delete(message.method);
      for (const waiter of waiters) waiter.resolve(message.params ?? {});
      return;
    }
    const request = pending.get(message.id);
    if (!request) return;
    pending.delete(message.id);
    if (message.error) request.reject(new Error(JSON.stringify(message.error)));
    else request.resolve(message.result);
  });
  await new Promise((resolvePromise, rejectPromise) => {
    socket.addEventListener("open", resolvePromise, { once: true });
    socket.addEventListener("error", rejectPromise, { once: true });
  });
  return {
    send(method, params = {}) {
      const id = nextId++;
      socket.send(JSON.stringify({ id, method, params }));
      return new Promise((resolvePromise, rejectPromise) =>
        pending.set(id, { resolve: resolvePromise, reject: rejectPromise }),
      );
    },
    async evaluate(expression) {
      const result = await this.send("Runtime.evaluate", {
        expression,
        returnByValue: true,
        awaitPromise: true,
      });
      if (result.exceptionDetails) {
        throw new Error(
          result.exceptionDetails.exception?.description ??
            "Runtime evaluation failed",
        );
      }
      return result.result.value;
    },
    waitForEvent(method, timeoutMs = 5_000) {
      return new Promise((resolvePromise, rejectPromise) => {
        const waiter = {
          resolve(value) {
            clearTimeout(timeout);
            resolvePromise(value);
          },
          reject: rejectPromise,
        };
        const timeout = setTimeout(() => {
          const current = eventWaiters.get(method) ?? [];
          eventWaiters.set(
            method,
            current.filter((candidate) => candidate !== waiter),
          );
          rejectPromise(new Error(`Timed out waiting for CDP event ${method}`));
        }, timeoutMs);
        const current = eventWaiters.get(method) ?? [];
        current.push(waiter);
        eventWaiters.set(method, current);
      });
    },
    close() {
      socket.close();
    },
  };
}

async function sampleProcessTree(rootPid) {
  const { stdout } = await execFileAsync(
    "/bin/ps",
    ["-axo", "pid=,ppid=,%cpu=,rss="],
    {
      encoding: "utf8",
      timeout: 2_000,
      maxBuffer: 4_000_000,
    },
  );
  const processes = stdout
    .split("\n")
    .map((line) => line.trim().split(/\s+/).map(Number))
    .filter(
      ([pid, ppid, cpu, rss]) =>
        Number.isInteger(pid) &&
        Number.isInteger(ppid) &&
        Number.isFinite(cpu) &&
        Number.isFinite(rss),
    )
    .map(([pid, parentPid, cpuPercent, rssKb]) => ({
      pid,
      parentPid,
      cpuPercent,
      rssKb,
    }));
  const included = new Set([rootPid]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const entry of processes) {
      if (included.has(entry.parentPid) && !included.has(entry.pid)) {
        included.add(entry.pid);
        changed = true;
      }
    }
  }
  const tree = processes.filter((entry) => included.has(entry.pid));
  if (tree.length === 0) return null;
  return {
    process_count: tree.length,
    cpu_percent: tree.reduce((sum, entry) => sum + entry.cpuPercent, 0),
    rss_kb: tree.reduce((sum, entry) => sum + entry.rssKb, 0),
    pids: tree.map((entry) => entry.pid),
  };
}

function terminateProcessGroup(pid, signal) {
  try {
    process.kill(-pid, signal);
  } catch (error) {
    if (error.code !== "ESRCH") throw error;
  }
}

export function electronGpuEvidencePassed(gpuMetrics) {
  return gpuMetrics?.qualification?.passed === true;
}

async function waitForExit(child, timeoutMs = 3_000) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  const exited = await Promise.race([
    new Promise((resolvePromise) =>
      child.once("exit", () => resolvePromise(true)),
    ),
    delay(timeoutMs).then(() => false),
  ]);
  if (!exited) {
    terminateProcessGroup(child.pid, "SIGKILL");
    await new Promise((resolvePromise) => child.once("exit", resolvePromise));
  }
}

async function elementCenter(cdp, testId) {
  return cdp.evaluate(`(() => {
    const element = document.querySelector('[data-testid=' + JSON.stringify(${JSON.stringify(testId)}) + ']');
    if (!element) throw new Error('Missing element with data-testid ${testId}');
    const bounds = element.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) throw new Error('Element ${testId} is not visible');
    return { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2 };
  })()`);
}

async function elementGeometry(cdp, testId) {
  return cdp.evaluate(`(() => {
    const element = document.querySelector('[data-testid=' + JSON.stringify(${JSON.stringify(testId)}) + ']');
    if (!element) throw new Error('Missing element with data-testid ${testId}');
    const bounds = element.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) throw new Error('Element ${testId} is not visible');
    return {
      center: { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2 },
      bounds: { x: bounds.left, y: bounds.top, width: bounds.width, height: bounds.height },
      window_logical_size: { width: window.innerWidth, height: window.innerHeight },
      aria_pressed: element.getAttribute('aria-pressed'),
    };
  })()`);
}

async function nativeClickTestId(cdp, target, testId) {
  const geometry = await elementGeometry(cdp, testId);
  const replay = buildClickReplay({
    windowId: target.window_id,
    point: geometry.center,
    logicalSize: geometry.window_logical_size,
    windowGeometry: target.geometry,
  });
  await runXdotool(replay.args, replay.metadata?.scheduled_duration_ms ?? 0);
  return {
    ...replay.metadata,
    pixel: replay.pixel,
    observed_geometry: geometry,
  };
}

async function nativeClickTestIdWithoutPresentation(cdp, target, testId) {
  const geometry = await elementGeometry(cdp, testId);
  const replay = buildClickReplay({
    windowId: target.window_id,
    point: geometry.center,
    logicalSize: geometry.window_logical_size,
    windowGeometry: target.geometry,
  });
  await runXdotool(replay.args, replay.metadata?.scheduled_duration_ms ?? 0);
  return {
    ...replay.metadata,
    pixel: replay.pixel,
    observed_geometry: geometry,
    presentation_observation: "deferred-to-native-chooser-accept",
  };
}

async function nativeClickSelector(
  cdp,
  target,
  selector,
  description = selector,
) {
  const geometry = await cdp.evaluate(`(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    if (!element) throw new Error(${JSON.stringify(`Missing ${description}`)});
    const bounds = element.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) throw new Error(${JSON.stringify(`${description} is not visible`)});
    return {
      center: { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2 },
      bounds: { x: bounds.left, y: bounds.top, width: bounds.width, height: bounds.height },
      window_logical_size: { width: window.innerWidth, height: window.innerHeight },
    };
  })()`);
  const replay = buildClickReplay({
    windowId: target.window_id,
    point: geometry.center,
    logicalSize: geometry.window_logical_size,
    windowGeometry: target.geometry,
  });
  await runXdotool(replay.args, replay.metadata?.scheduled_duration_ms ?? 0);
  return {
    ...replay.metadata,
    pixel: replay.pixel,
    observed_geometry: geometry,
  };
}

async function nativeClickVisibleText(
  cdp,
  target,
  visibleText,
  rootSelector = "body",
) {
  const selector = `[data-bp-native-text-target=${JSON.stringify(visibleText)}]`;
  const geometry = await cdp.evaluate(`(() => {
    const roots = [...document.querySelectorAll(${JSON.stringify(rootSelector)})];
    const element = roots.flatMap((root) => [...root.querySelectorAll('button,[role="option"],[data-slot="select-item"],[data-slot="toggle-group-item"]')])
      .find((candidate) => {
        const bounds = candidate.getBoundingClientRect();
        return candidate.textContent?.trim() === ${JSON.stringify(visibleText)} && bounds.width > 0 && bounds.height > 0;
      });
    if (!element) throw new Error(${JSON.stringify(`Missing visible control ${visibleText}`)});
    const bounds = element.getBoundingClientRect();
    return {
      center: { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2 },
      bounds: { x: bounds.left, y: bounds.top, width: bounds.width, height: bounds.height },
      window_logical_size: { width: window.innerWidth, height: window.innerHeight },
    };
  })()`);
  const replay = buildClickReplay({
    windowId: target.window_id,
    point: geometry.center,
    logicalSize: geometry.window_logical_size,
    windowGeometry: target.geometry,
  });
  await runXdotool(replay.args, 0);
  return {
    selector,
    ...replay.metadata,
    pixel: replay.pixel,
    observed_geometry: geometry,
  };
}

export function buildElectronNativeStableHexReplacementCommands(
  windowId,
  value,
) {
  if (!/^#[0-9a-f]{6}$/i.test(value)) {
    throw new Error(
      "native stable color replacement requires a six-digit hex value",
    );
  }
  const prefix = ["--window", String(windowId), "--clearmodifiers"];
  return [
    ["key", ...prefix, "Home", "Right"],
    ...value
      .slice(1)
      .split("")
      .flatMap((digit) => [
        ["key", ...prefix, "shift+Right"],
        ["type", ...prefix, "--delay", "1", digit],
      ]),
  ];
}

async function nativeReplaceInput(cdp, target, selector, value) {
  const click = await nativeClickSelector(
    cdp,
    target,
    selector,
    `input ${selector}`,
  );
  const stableHex =
    selector === '[aria-label="Custom preset hex color"]' &&
    /^#[0-9a-f]{6}$/i.test(value);
  const atomicNumericPaste =
    selector === "#construction-grid-spacing" && /^\d+(?:\.\d+)?$/.test(value);
  const commands = stableHex
    ? buildElectronNativeStableHexReplacementCommands(target.window_id, value)
    : atomicNumericPaste
      ? []
      : [
          [
            "key",
            "--window",
            String(target.window_id),
            "--clearmodifiers",
            "ctrl+a",
          ],
          [
            "type",
            "--window",
            String(target.window_id),
            "--clearmodifiers",
            "--delay",
            "1",
            value,
          ],
        ];
  for (const args of commands) await runXdotool(args, 0);
  if (atomicNumericPaste) {
    await nativeSendKey(target, "ctrl+a");
    await nativePasteText(target, value);
  }
  await waitForEditorCondition(
    cdp,
    `document.querySelector(${JSON.stringify(selector)})?.value === ${JSON.stringify(value)}`,
    `exact native input value ${selector}`,
    5_000,
  );
  return {
    click,
    typed_character_count: value.length,
    replacement_method: stableHex
      ? "one-selected-digit-at-a-time-valid-hex"
      : atomicNumericPaste
        ? "select-all-and-native-clipboard-paste"
        : "select-all-and-type",
  };
}

async function nativeReplaceLabeledNumber(
  cdp,
  target,
  label,
  value,
  blurTestId,
) {
  const geometry = await cdp.evaluate(`(async () => {
    const root = document.querySelector('[data-testid="right-sidebar"]');
    const fieldLabel = [...(root?.querySelectorAll('label') ?? [])]
      .find((candidate) => candidate.textContent?.trim() === ${JSON.stringify(label)});
    const input = fieldLabel?.htmlFor ? document.getElementById(fieldLabel.htmlFor) : null;
    if (!(input instanceof HTMLInputElement)) throw new Error(${JSON.stringify(`Missing selected property input ${label}`)});
    input.scrollIntoView({ block: 'center' });
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const bounds = input.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) throw new Error(${JSON.stringify(`Selected property input ${label} is not visible`)});
    return {
      center: { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2 },
      window_logical_size: { width: window.innerWidth, height: window.innerHeight },
    };
  })()`);
  const click = buildClickReplay({
    windowId: target.window_id,
    point: geometry.center,
    logicalSize: geometry.window_logical_size,
    windowGeometry: target.geometry,
  });
  await runXdotool(click.args, 0);
  await nativeSendKey(target, "ctrl+a");
  await nativePasteText(target, String(value));
  await waitForEditorCondition(
    cdp,
    `(() => {
      const root = document.querySelector('[data-testid="right-sidebar"]');
      const label = [...(root?.querySelectorAll('label') ?? [])].find((candidate) => candidate.textContent?.trim() === ${JSON.stringify(label)});
      const input = label?.htmlFor ? document.getElementById(label.htmlFor) : null;
      return input instanceof HTMLInputElement && input.value === ${JSON.stringify(String(value))};
    })()`,
    `native ${label} value ${value}`,
  );
  const beforeBlur = await cdp.evaluate(
    `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve(true))))`,
  );
  const blur = await nativeClickTestId(cdp, target, blurTestId);
  return {
    input_lane: nativeX11InputLane,
    injection_api: "XTEST-via-xdotool",
    value,
    click: click.metadata,
    presentation_before_blur: beforeBlur,
    blur,
  };
}

async function nativeSetSelectedColor(cdp, target, label, value) {
  const normalized = String(value).toLowerCase();
  const triggerSelector = `[aria-label^=${JSON.stringify(`${label}:`)}]`;
  const trigger = await nativeClickSelector(
    cdp,
    target,
    triggerSelector,
    `${label} color control`,
  );
  await waitForEditorCondition(
    cdp,
    `Boolean(document.querySelector('[data-slot="popover-content"]'))`,
    `${label} native color popover`,
  );
  const preset = await nativeClickSelector(
    cdp,
    target,
    `[aria-label=${JSON.stringify(`Use ${normalized}`)}]`,
    `${label} ${normalized} preset`,
  );
  await waitForEditorCondition(
    cdp,
    `document.querySelector(${JSON.stringify(triggerSelector)})?.getAttribute('aria-label')?.toLowerCase() === ${JSON.stringify(`${label}: ${normalized}`.toLowerCase())}`,
    `${label} native color ${normalized}`,
  );
  await nativeClickSelector(
    cdp,
    target,
    triggerSelector,
    `${label} color control`,
  );
  await waitForEditorCondition(
    cdp,
    `!document.querySelector('[data-slot="popover-content"]')`,
    `${label} native color popover close`,
  );
  return {
    input_lane: nativeX11InputLane,
    injection_api: "XTEST-via-xdotool",
    label,
    value: normalized,
    trigger,
    preset,
  };
}

async function nativeSendKey(target, key) {
  await runXdotool(
    ["key", "--window", String(target.window_id), "--clearmodifiers", key],
    0,
  );
}

async function nativeTypeText(target, value) {
  await runXdotool(
    [
      "type",
      "--window",
      String(target.window_id),
      "--clearmodifiers",
      "--delay",
      "1",
      value,
    ],
    0,
  );
}

async function nativePasteText(target, value) {
  const owner = spawn("xclip", ["-selection", "clipboard", "-loops", "1"], {
    env: process.env,
    stdio: ["pipe", "ignore", "pipe"],
  });
  let stderr = "";
  owner.stderr.setEncoding("utf8");
  owner.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  owner.stdin.end(value);
  await delay(50);
  await nativeSendKey(target, "ctrl+v");
  await delay(100);
  if (owner.exitCode === null) owner.kill("SIGTERM");
  if (owner.exitCode && owner.exitCode !== 0) {
    throw new Error(`native clipboard owner failed: ${stderr}`);
  }
}

async function nativePdfPointClick(cdp, target, point) {
  const surface = await annotationSurface(cdp);
  const css = pdfPointToNativeCss(point, surface);
  const logicalSize = await cdp.evaluate(
    `({ width: window.innerWidth, height: window.innerHeight })`,
  );
  const replay = buildClickReplay({
    windowId: target.window_id,
    point: css,
    logicalSize,
    windowGeometry: target.geometry,
  });
  await runXdotool(replay.args, 0);
  return {
    input_lane: nativeX11InputLane,
    injection_api: "XTEST-via-xdotool",
    page: 1,
    pdf_point: point,
    css_point: css,
    pixel_point: replay.pixel,
    surface,
  };
}

export function assessElectronNativeWindowFocusRestoration({
  target_window_id: targetWindowId,
  before_active_window_id: beforeActiveWindowId,
  before_document_has_focus: beforeDocumentHasFocus,
  after_active_window_id: afterActiveWindowId,
  after_document_has_focus: afterDocumentHasFocus,
}) {
  const target = String(targetWindowId ?? "").trim();
  const beforeActive = String(beforeActiveWindowId ?? "").trim();
  const afterActive = String(afterActiveWindowId ?? "").trim();
  const restorationRequired =
    target.length === 0 ||
    beforeActive !== target ||
    beforeDocumentHasFocus !== true;
  const passed =
    target.length > 0 &&
    afterActive === target &&
    afterDocumentHasFocus === true;
  return {
    receipt_kind: "electron-native-window-focus-restoration-v1",
    evidence_scope:
      "non-command-window-focus-restoration-before-contractual-image-placement",
    input_lane: nativeX11InputLane,
    injection_api: "EWMH-windowactivate-via-xdotool",
    contractual_image_placement_input: false,
    target_window_id: target || null,
    before_active_window_id: beforeActive || null,
    before_document_has_focus: beforeDocumentHasFocus === true,
    after_active_window_id: afterActive || null,
    after_document_has_focus: afterDocumentHasFocus === true,
    restoration_required: restorationRequired,
    passed,
  };
}

async function restoreElectronNativeWindowFocus(cdp, target, event) {
  const beforeActiveWindowId = await optionalCommand("xdotool", [
    "getactivewindow",
  ]);
  const beforeDocumentHasFocus = await cdp.evaluate("document.hasFocus()");
  const targetWindowId = String(target.window_id);
  if (
    beforeActiveWindowId !== targetWindowId ||
    beforeDocumentHasFocus !== true
  ) {
    await runXdotool(["windowactivate", "--sync", targetWindowId], 0);
    await waitForEditorCondition(
      cdp,
      "document.hasFocus() === true",
      "native Electron target focus before Image placement",
      5_000,
    );
  }
  const [afterActiveWindowId, afterDocumentHasFocus] = await Promise.all([
    optionalCommand("xdotool", ["getactivewindow"]),
    cdp.evaluate("document.hasFocus()"),
  ]);
  const receipt = assessElectronNativeWindowFocusRestoration({
    target_window_id: targetWindowId,
    before_active_window_id: beforeActiveWindowId,
    before_document_has_focus: beforeDocumentHasFocus,
    after_active_window_id: afterActiveWindowId,
    after_document_has_focus: afterDocumentHasFocus,
  });
  event("native-window-focus-restoration", receipt);
  if (!receipt.passed) {
    throw new Error(
      `native Electron target focus was not restored before Image placement: ${JSON.stringify(receipt)}`,
    );
  }
  return receipt;
}

async function locateNativeElectronTarget(
  cdp,
  applicationPid,
  timeoutMs,
  event,
) {
  const observation = await cdp.evaluate(`({
    title: document.title,
    inner_width: window.innerWidth,
    inner_height: window.innerHeight,
  })`);
  if (observation.inner_width !== 1200 || observation.inner_height !== 800) {
    throw new Error(
      `native Electron replay requires a 1200x800 content area; received ${observation.inner_width}x${observation.inner_height}`,
    );
  }
  let target = await locateExactX11Window(applicationPid, {
    timeoutMs: Math.min(timeoutMs, 10_000),
    expectedTitle: observation.title,
    expectedSize: null,
  });
  const frameWidth = target.geometry.width - observation.inner_width;
  const frameHeight = target.geometry.height - observation.inner_height;
  if (
    frameWidth < 0 ||
    frameHeight < 0 ||
    frameWidth % 2 !== 0 ||
    frameHeight % 2 !== 0
  ) {
    throw new Error(
      `native Electron frame cannot be mapped exactly: outer ${target.geometry.width}x${target.geometry.height}, inner 1200x800`,
    );
  }
  target = {
    ...target,
    geometry: {
      ...target.geometry,
      content: {
        x: frameWidth / 2,
        y: frameHeight / 2,
        width: observation.inner_width,
        height: observation.inner_height,
      },
    },
  };
  event("native-window-target-verified", {
    window_id: target.window_id,
    pid: target.pid,
    title: target.title,
    geometry: target.geometry,
  });
  return target;
}

async function runElectronNativeOpenAction(cdp, target, pdfPath) {
  // The File menu is painted on a separate Chromium popup surface. Requiring
  // server-side damage on the main BrowserWindow after that setup click is
  // therefore an invalid temporal boundary. Use the maintained blank-state
  // Open PDF control, as the GPUI lane does, and bind the common drawable-
  // damage observation to the chooser's terminal Open-button click below.
  const openControl = await nativeClickTestIdWithoutPresentation(
    cdp,
    target,
    "viewport-open-document",
  );
  const dialogDeadline = performance.now() + 10_000;
  let dialogWindow = null;
  while (performance.now() < dialogDeadline) {
    try {
      const { stdout: activeWindowOutput } = await execFileAsync(
        "xdotool",
        ["getactivewindow"],
        { encoding: "utf8", timeout: 1_000 },
      );
      const windowId = activeWindowOutput.trim();
      if (windowId && windowId !== String(target.window_id)) {
        const { stdout: titleOutput } = await execFileAsync(
          "xdotool",
          ["getwindowname", windowId],
          { encoding: "utf8", timeout: 1_000 },
        );
        dialogWindow = { window_id: windowId, title: titleOutput.trim() };
        break;
      }
    } catch {
      // The native chooser is still being created.
    }
    await delay(20);
  }
  if (!dialogWindow) {
    throw new Error("native File > Open chooser did not receive focus");
  }
  let dialogDurationMs = 0;
  const dialogCommands = buildElectronNativeFileDialogCommands(pdfPath);
  for (let index = 0; index < dialogCommands.length; index += 1) {
    const command = dialogCommands[index];
    dialogDurationMs += await runXdotool(
      command,
      index === 1 ? pdfPath.length : 0,
    );
    if (index === 1) await delay(100);
  }
  const verifiedDialogTarget = await locateExactX11WindowById(
    target.pid,
    dialogWindow.window_id,
    {
    expectedTitle: dialogWindow.title,
    expectedSize: null,
    },
  );
  const openClick = buildElectronNativeFileDialogOpenClick(verifiedDialogTarget);
  dialogDurationMs += await runDamageObservedXTestExternalClick(
    target,
    dialogWindow.window_id,
    openClick,
    "open-pdf:dialog-open-button",
  );
  return {
    input_lane: nativeX11InputLane,
    injection_api: "XTEST-via-xdotool",
    open_control: openControl,
    dialog_window: dialogWindow,
    open_button_click: openClick,
    damage_drawable: {
      window_id: String(target.window_id),
      title: target.title,
    },
    input_target_relation: "externally-verified-native-chooser",
    temporal_action: "chooser-open-button-click-to-main-window-damage",
    dialog_keyboard_event_count: pdfPath.length + 1,
    dialog_pointer_event_count: 1,
    dialog_duration_ms: dialogDurationMs,
  };
}

async function disableSnappingWithNativeUi(cdp, target) {
  await nativeClickTestId(cdp, target, "viewer-snap-target-menu");
  const changed = [];
  for (const source of ["content", "markup", "page-grid"]) {
    const testId = `viewer-snap-${source}`;
    const state = await elementGeometry(cdp, testId);
    if (state.aria_pressed === "true") {
      await nativeClickTestId(cdp, target, testId);
      changed.push(source);
    } else if (state.aria_pressed !== "false") {
      throw new Error(`${testId} did not expose an exact pressed state`);
    }
  }
  await nativeClickTestId(cdp, target, "viewer-snap-target-menu");
  const closed = await cdp.evaluate(`(async () => {
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    return document.querySelector('[data-testid="viewer-snap-target-menu"]')?.getAttribute('aria-pressed') === 'false';
  })()`);
  if (!closed) throw new Error("native snap settings popover did not close");
  return changed;
}

async function electronAnnotationSurface(cdp) {
  return cdp.evaluate(`(() => {
    const layer = document.querySelector('[data-testid="annotation-layer-1"]');
    const documentModel = window.__butterPaperTestHooks.getActiveDocument();
    const page = documentModel?.pages?.[0];
    if (!layer || !page) throw new Error('Page 1 annotation surface is unavailable');
    if (page.rotation !== 0) throw new Error('Native annotation replay requires an unrotated fixture page');
    const viewBox = page.viewBox ?? { x: 0, y: 0, width: page.size.width, height: page.size.height };
    if (viewBox.x !== 0 || viewBox.y !== 0) throw new Error('Native annotation replay requires a zero-origin view box');
    const bounds = layer.getBoundingClientRect();
    return {
      window_logical_size: { width: window.innerWidth, height: window.innerHeight },
      bounds: { x: bounds.left, y: bounds.top, width: bounds.width, height: bounds.height },
      page_height_points: viewBox.height,
      pixels_per_point_x: bounds.width / viewBox.width,
      pixels_per_point_y: bounds.height / viewBox.height,
    };
  })()`);
}

async function dispatchCdpClick(cdp, testId) {
  const point = await elementCenter(cdp, testId);
  await cdp.send("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x: point.x,
    y: point.y,
  });
  await cdp.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: point.x,
    y: point.y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await cdp.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: point.x,
    y: point.y,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
}

async function dispatchCdpSelectorClick(cdp, selector, description = selector) {
  const point = await cdp.evaluate(`(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    if (!element) throw new Error(${JSON.stringify(`Missing ${description}`)});
    const bounds = element.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) throw new Error(${JSON.stringify(`${description} is not visible`)});
    return { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2 };
  })()`);
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", ...point });
  await cdp.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    ...point,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await cdp.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    ...point,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
}

async function annotationSurface(cdp) {
  return cdp.evaluate(`(() => {
    const documentModel = window.__butterPaperTestHooks.getActiveDocument();
    const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
    const pageIndex = diagnostics.currentPage;
    const layer = document.querySelector('[data-testid="annotation-layer-' + (pageIndex + 1) + '"]');
    const page = documentModel?.pages?.find((candidate) => candidate.index === pageIndex);
    if (!layer || !page) throw new Error('Current annotation surface is unavailable');
    if (page.rotation !== 0) throw new Error('Expanded annotation replay requires an unrotated fixture page');
    const bounds = layer.getBoundingClientRect();
    const viewBox = page.viewBox ?? { x: 0, y: 0, width: page.size.width, height: page.size.height };
    return {
      bounds: { left: bounds.left, top: bounds.top, width: bounds.width, height: bounds.height },
      view_box: viewBox,
      pixels_per_pdf_point: diagnostics.zoom * (page.userUnit ?? 1),
      page_number: page.index + 1,
    };
  })()`);
}

export function pdfPointToCss(point, surface) {
  const viewBox = surface.view_box;
  const scale = surface.pixels_per_pdf_point;
  if (Number.isFinite(scale) && scale > 0) {
    return {
      x: surface.bounds.left + (point.x - viewBox.x) * scale,
      y: surface.bounds.top + (viewBox.y + viewBox.height - point.y) * scale,
    };
  }
  return {
    x:
      surface.bounds.left +
      ((point.x - viewBox.x) / viewBox.width) * surface.bounds.width,
    y:
      surface.bounds.top +
      ((viewBox.y + viewBox.height - point.y) / viewBox.height) *
        surface.bounds.height,
  };
}

export function pdfPointToNativeCss(point, surface) {
  const viewBox = surface?.view_box;
  const bounds = surface?.bounds;
  const scale = surface?.pixels_per_pdf_point;
  if (
    ![
      point?.x,
      point?.y,
      viewBox?.x,
      viewBox?.y,
      viewBox?.width,
      viewBox?.height,
      bounds?.left,
      bounds?.top,
      bounds?.width,
      bounds?.height,
      scale,
    ].every(Number.isFinite) ||
    viewBox.width <= 0 ||
    viewBox.height <= 0 ||
    bounds.width <= 0 ||
    bounds.height <= 0 ||
    scale <= 0
  ) {
    throw new Error(
      "native PDF click mapping requires finite positive observed surface geometry",
    );
  }
  return {
    x: bounds.left + (point.x - viewBox.x) * scale,
    y: bounds.top + bounds.height - (point.y - viewBox.y) * scale,
  };
}

async function dispatchPdfPointClick(cdp, point) {
  const surface = await annotationSurface(cdp);
  const css = pdfPointToCss(point, surface);
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", ...css });
  await cdp.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    ...css,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await cdp.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    ...css,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
  return {
    page: surface.page_number,
    pdf_point: point,
    css_point: css,
    surface,
  };
}

async function centerEditorPdfPointAtCurrentZoom(cdp, point) {
  return cdp.evaluate(`(async () => {
    const viewport = document.querySelector('[data-testid="document-viewport"]');
    const layer = document.querySelector('[data-testid="annotation-layer-1"]');
    const page = window.__butterPaperTestHooks.getActiveDocument()?.pages?.[0];
    if (!(viewport instanceof HTMLElement) || !layer || !page) {
      throw new Error('Editor zoom focus surface is unavailable');
    }
    const viewBox = page.viewBox ?? { x: 0, y: 0, width: page.size.width, height: page.size.height };
    const focus = ${JSON.stringify(point)};
    const center = () => {
      const viewportBounds = viewport.getBoundingClientRect();
      const layerBounds = layer.getBoundingClientRect();
      const focusX = layerBounds.left + (focus.x - viewBox.x) / viewBox.width * layerBounds.width;
      const focusY = layerBounds.top + (viewBox.y + viewBox.height - focus.y) / viewBox.height * layerBounds.height;
      return {
        viewportBounds,
        focusX,
        focusY,
        deltaX: focusX - (viewportBounds.left + viewportBounds.width / 2),
        deltaY: focusY - (viewportBounds.top + viewportBounds.height / 2),
      };
    };
    const before = center();
    viewport.scrollBy({ left: before.deltaX, top: before.deltaY, behavior: 'instant' });
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const after = center();
    return {
      method: 'document-viewport-scrollBy-diagnostic',
      focus_pdf_point: focus,
      before_delta_px: { x: before.deltaX, y: before.deltaY },
      after_delta_px: { x: after.deltaX, y: after.deltaY },
      viewport_size_px: {
        width: after.viewportBounds.width,
        height: after.viewportBounds.height,
      },
      centered: Math.abs(after.deltaX) <= 1 && Math.abs(after.deltaY) <= 1,
    };
  })()`);
}

export function buildElectronNativeViewportPanSegment(observation) {
  const bounds = observation?.viewport_bounds;
  const delta = observation?.delta_px;
  if (
    ![
      bounds?.x,
      bounds?.y,
      bounds?.width,
      bounds?.height,
      delta?.x,
      delta?.y,
    ].every(Number.isFinite) ||
    bounds.width <= 2 ||
    bounds.height <= 2
  ) {
    throw new Error(
      "native editor pan segment requires finite positive viewport geometry and delta",
    );
  }
  const start = {
    x: bounds.x + bounds.width / 2,
    y: bounds.y + bounds.height / 2,
  };
  const requestedFinish = { x: start.x - delta.x, y: start.y - delta.y };
  const finish = {
    x: Math.min(
      bounds.x + bounds.width - 1,
      Math.max(bounds.x + 1, requestedFinish.x),
    ),
    y: Math.min(
      bounds.y + bounds.height - 1,
      Math.max(bounds.y + 1, requestedFinish.y),
    ),
  };
  return {
    start,
    finish,
    requested_finish: requestedFinish,
    clamped: finish.x !== requestedFinish.x || finish.y !== requestedFinish.y,
  };
}

async function centerEditorPdfPointWithNativePan(cdp, target, point) {
  const observe = () =>
    cdp.evaluate(`(() => {
    const viewport = document.querySelector('[data-testid="document-viewport"]');
    const layer = document.querySelector('[data-testid="annotation-layer-1"]');
    const page = window.__butterPaperTestHooks.getActiveDocument()?.pages?.[0];
    if (!(viewport instanceof HTMLElement) || !layer || !page) {
      throw new Error('Native editor zoom focus surface is unavailable');
    }
    const viewBox = page.viewBox ?? { x: 0, y: 0, width: page.size.width, height: page.size.height };
    const focus = ${JSON.stringify(point)};
    const viewportBounds = viewport.getBoundingClientRect();
    const layerBounds = layer.getBoundingClientRect();
    const focusX = layerBounds.left + (focus.x - viewBox.x) / viewBox.width * layerBounds.width;
    const focusY = layerBounds.top + (viewBox.y + viewBox.height - focus.y) / viewBox.height * layerBounds.height;
    return {
      viewport_bounds: { x: viewportBounds.left, y: viewportBounds.top, width: viewportBounds.width, height: viewportBounds.height },
      delta_px: {
        x: focusX - (viewportBounds.left + viewportBounds.width / 2),
        y: focusY - (viewportBounds.top + viewportBounds.height / 2),
      },
      window_logical_size: { width: window.innerWidth, height: window.innerHeight },
    };
  })()`);
  const before = await observe();
  let current = before;
  const pixelPaths = [];
  for (
    let attempt = 0;
    attempt < 4 &&
    (Math.abs(current.delta_px.x) > 1 || Math.abs(current.delta_px.y) > 1);
    attempt += 1
  ) {
    const segment = buildElectronNativeViewportPanSegment(current);
    const startPixel = windowLogicalPointToPixel(
      segment.start,
      current.window_logical_size,
      target.geometry,
    );
    const finishPixel = windowLogicalPointToPixel(
      segment.finish,
      current.window_logical_size,
      target.geometry,
    );
    await runXdotool(
      [
        "mousemove",
        "--window",
        String(target.window_id),
        String(startPixel.x),
        String(startPixel.y),
        "mousedown",
        "2",
        "mousemove",
        "--window",
        String(target.window_id),
        String(finishPixel.x),
        String(finishPixel.y),
        "mouseup",
        "2",
      ],
      0,
    );
    await cdp.evaluate(
      `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
    );
    const next = await observe();
    pixelPaths.push({
      attempt: attempt + 1,
      segment,
      start: startPixel,
      finish: finishPixel,
    });
    const previousDistance = Math.hypot(current.delta_px.x, current.delta_px.y);
    const nextDistance = Math.hypot(next.delta_px.x, next.delta_px.y);
    if (nextDistance >= previousDistance - 0.25) {
      throw new Error(
        `native editor segmented pan did not reduce focus distance: ${JSON.stringify({ current, next, pixelPaths })}`,
      );
    }
    current = next;
  }
  const after = current;
  return {
    method: "native-XTEST-segmented-middle-button-pan",
    focus_pdf_point: point,
    before_delta_px: before.delta_px,
    after_delta_px: after.delta_px,
    viewport_size_px: {
      width: before.viewport_bounds.width,
      height: before.viewport_bounds.height,
    },
    pixel_paths: pixelPaths,
  };
}

async function observeEditorEndpointMapping(cdp, points, marginPx) {
  const surface = await annotationSurface(cdp);
  const viewport = await cdp.evaluate(`(() => {
    const element = document.querySelector('[data-testid="document-viewport"]');
    if (!element) throw new Error('Document viewport is unavailable');
    const bounds = element.getBoundingClientRect();
    return { left: bounds.left, top: bounds.top, right: bounds.right, bottom: bounds.bottom };
  })()`);
  const mapped = points.map((point) => ({
    pdf_point: point,
    css_point: pdfPointToCss(point, surface),
  }));
  return {
    margin_px: marginPx,
    viewport,
    surface,
    mapped,
    all_inside_viewport: mapped.every(
      ({ css_point: point }) =>
        point.x >= viewport.left + marginPx &&
        point.x <= viewport.right - marginPx &&
        point.y >= viewport.top + marginPx &&
        point.y <= viewport.bottom - marginPx,
    ),
  };
}

async function dispatchAnnotationPointerStream(cdp, samples, event, commandId) {
  const surface = await annotationSurface(cdp);
  const cssSamples = samples.map((sample) => ({
    ...sample,
    ...pdfPointToCss(sample, surface),
  }));
  const dispatchStarted = performance.now();
  const acknowledgements = [];
  for (let index = 0; index < cssSamples.length; index += 1) {
    const sample = cssSamples[index];
    const waitMs = dispatchStarted + sample.t_ms - performance.now();
    if (waitMs > 0) await delay(waitMs);
    const type =
      index === 0
        ? "mousePressed"
        : index === cssSamples.length - 1
          ? "mouseReleased"
          : "mouseMoved";
    await cdp.send("Input.dispatchMouseEvent", {
      type,
      x: sample.x,
      y: sample.y,
      button: "left",
      buttons: type === "mouseReleased" ? 0 : 1,
      clickCount: 1,
    });
    acknowledgements.push({
      sample_index: index,
      scheduled_t_ms: sample.t_ms,
      acknowledged_t_ms: performance.now() - dispatchStarted,
    });
    if (index > 0 && (index % 120 === 0 || index === cssSamples.length - 1)) {
      event("comparison-input-batch", {
        command_id: commandId,
        first_sample_index: Math.max(0, index - 119),
        last_sample_index: index,
      });
    }
  }
  return {
    input_lane: "cdp-input-diagnostic",
    coordinate_mapping: { page: 1, surface },
    expected_sample_count: samples.length,
    acknowledged_sample_count: acknowledgements.length,
    scheduled_duration_ms: samples.at(-1).t_ms,
    actual_duration_ms: performance.now() - dispatchStarted,
    rate_schedule_met:
      performance.now() - dispatchStarted <= samples.at(-1).t_ms * 1.05,
    acknowledgements,
  };
}

function addMilestone(
  event,
  result,
  milestone,
  passed,
  evidence,
  blocker = null,
) {
  const record = {
    milestone,
    status: passed ? "passed" : "blocked",
    evidence,
    blocker,
  };
  result.milestones.push(record);
  if (passed) {
    result.observed_milestones.push(milestone);
    event("comparison-milestone", {
      command_id: result.command_id,
      milestone,
      evidence,
    });
  }
}

function finalizeCommandMilestones(result, command) {
  const recorded = new Map(
    result.milestones.map((record) => [record.milestone, record]),
  );
  result.missing_milestones = command.expected_milestones.filter(
    (milestone) => !recorded.has(milestone),
  );
  result.manifest_milestones_complete =
    result.missing_milestones.length === 0 &&
    command.expected_milestones.every(
      (milestone) => recorded.get(milestone)?.status === "passed",
    );
}

function closeEnough(left, right, tolerance = 0.05) {
  return (
    Number.isFinite(left) &&
    Number.isFinite(right) &&
    Math.abs(left - right) <= tolerance
  );
}

function recordMatches(actual, expected, tolerance = 0.05) {
  return (
    actual != null &&
    Object.entries(expected).every(([key, value]) =>
      typeof value === "number"
        ? closeEnough(actual[key], value, tolerance)
        : actual[key] === value,
    )
  );
}

function normalizedColor(value) {
  if (typeof value !== "string") return null;
  const color = value.toLowerCase();
  return color.length === 7 ? `${color}ff` : color;
}

export function editorCreatePrecisionUiSelection(command) {
  if (command?.id !== "length:set-scale" || command.scale?.precision !== 2) {
    throw new Error(
      "editor-create precision UI supports only the frozen decimal precision contract",
    );
  }
  return [
    { test_id: "page-scale-precision-mode", visible_label: "Decimal" },
    { test_id: "page-scale-precision-value", visible_label: "0.01" },
  ];
}

export function findVisibleElectronSelectOption(
  root,
  visibleLabel,
  readBounds = (element) => element.getBoundingClientRect(),
) {
  return [
    ...(root?.querySelectorAll(
      '[data-slot="select-content"] [role="option"][data-slot="select-item"]',
    ) ?? []),
  ].find((candidate) => {
    const bounds = readBounds(candidate);
    return (
      candidate.textContent?.trim() === visibleLabel &&
      bounds.width > 0 &&
      bounds.height > 0
    );
  });
}

export function electronSelectTriggerRetainsValue(trigger, visibleLabel) {
  return (
    trigger
      ?.querySelector('[data-slot="select-value"]')
      ?.textContent?.trim() === visibleLabel
  );
}

export function buildEditorCreateHighZoomPlan(command, options = {}) {
  if (command?.id !== "length:create") {
    throw new Error("editor-create high-zoom planning requires length:create");
  }
  const viewportMarginPx = 24;
  const endpointDistancePoints = Math.hypot(
    command.finish.x - command.start.x,
    command.finish.y - command.start.y,
  );
  const zoomCandidates =
    options.zoom_percent === undefined
      ? [400, 200, 100]
      : [options.zoom_percent];
  const zoomPercent =
    zoomCandidates.find(
      (candidate) =>
        (endpointDistancePoints * candidate) / 100 + viewportMarginPx * 2 <=
        (options.viewport_width_px ?? 0),
    ) ?? zoomCandidates[0];
  const endpointDistancePx = (endpointDistancePoints * zoomPercent) / 100;
  return {
    zoom_preset_test_id: `viewer-zoom-preset-${zoomPercent}`,
    zoom_percent: zoomPercent,
    close_menu_with_escape: true,
    focus_pdf_point: {
      x: (command.start.x + command.finish.x) / 2,
      y: (command.start.y + command.finish.y) / 2,
    },
    endpoint_distance_px: endpointDistancePx,
    viewport_margin_px: viewportMarginPx,
    endpoints_fit_viewport:
      endpointDistancePx + viewportMarginPx * 2 <=
      (options.viewport_width_px ?? 0),
  };
}

export function buildEditorCreateLengthInteractionPlan() {
  return {
    active_tool_test_id: "tool-length",
    active_tool_aria_pressed: "true",
    scroll_into_view: { block: "center", inline: "nearest" },
    inter_endpoint_animation_frames: 2,
  };
}

function pointInsideRect(point, rect, tolerance = 0.05) {
  return (
    point != null &&
    rect != null &&
    point.x >= rect.x - tolerance &&
    point.x <= rect.x + rect.width + tolerance &&
    point.y >= rect.y - tolerance &&
    point.y <= rect.y + rect.height + tolerance
  );
}

function expectedNaturalImagePlacement(command, pageSize) {
  pageSize ??= command?.placement?.fixture_page_size_points;
  if (!pageSize || command?.placement?.sizing !== "natural-size-page-contained")
    return null;
  const sourceWidth = 512;
  const sourceHeight = 384;
  const aspectRatio = sourceWidth / sourceHeight;
  const maxWidth = pageSize.width * command.placement.max_page_fraction;
  const maxHeight = pageSize.height * command.placement.max_page_fraction;
  const scale = Math.min(1, maxWidth / sourceWidth, maxHeight / sourceHeight);
  const width = sourceWidth * scale;
  const height = width / aspectRatio;
  const point = command.placement.point;
  return {
    x: Math.min(
      Math.max(0, point.x - width / 2),
      Math.max(0, pageSize.width - width),
    ),
    y: Math.min(
      Math.max(0, point.y - height / 2),
      Math.max(0, pageSize.height - height),
    ),
    width,
    height,
  };
}

const electronImageCheckerAsset = Object.freeze({
  asset_id: "bp-image-checker-v1",
  encoded_sha256:
    "fcc714d1ac60ed4b88abf7297830479c7557cb9d219033e7a5a5ad4d6ec18dda",
  encoded_bytes: 3153,
  source_width_px: 512,
  source_height_px: 384,
});

function decodePngDataUrl(dataUrl) {
  if (typeof dataUrl !== "string") return null;
  const match = /^data:image\/png(;base64)?,(.*)$/s.exec(dataUrl);
  if (!match) return null;
  try {
    return match[1]
      ? Buffer.from(match[2], "base64")
      : Buffer.from(decodeURIComponent(match[2]), "utf8");
  } catch {
    return null;
  }
}

export function buildElectronImageRendererResourceSubmissionReceipt(
  command,
  evidence,
) {
  const checkerBytes =
    evidence?.checker_asset_bytes == null
      ? null
      : Buffer.from(evidence.checker_asset_bytes);
  const markupBytes = decodePngDataUrl(evidence?.markup?.dataUrl);
  const checkerSha256 =
    checkerBytes == null
      ? null
      : createHash("sha256").update(checkerBytes).digest("hex");
  const markupSha256 =
    markupBytes == null
      ? null
      : createHash("sha256").update(markupBytes).digest("hex");
  const presentation = evidence?.presentation ?? {};
  const decodedRgbaPayloadBytes = Number.isInteger(
    presentation.decoded_rgba_payload_bytes,
  )
    ? presentation.decoded_rgba_payload_bytes
    : null;
  const decodedDimensionsExact =
    presentation.decoded_width === electronImageCheckerAsset.source_width_px &&
    presentation.decoded_height ===
      electronImageCheckerAsset.source_height_px &&
    decodedRgbaPayloadBytes ===
      electronImageCheckerAsset.source_width_px *
        electronImageCheckerAsset.source_height_px *
        4;
  const nativeLayout = classifyEditorCreateBoundsContract(
    command,
    evidence?.markup,
    evidence?.page_size,
    evidence?.input,
  );
  const exactCheckerAsset =
    command?.asset_id === electronImageCheckerAsset.asset_id &&
    checkerSha256 === electronImageCheckerAsset.encoded_sha256 &&
    checkerBytes?.length === electronImageCheckerAsset.encoded_bytes &&
    markupSha256 === electronImageCheckerAsset.encoded_sha256 &&
    markupBytes?.length === electronImageCheckerAsset.encoded_bytes &&
    decodedDimensionsExact;
  const exactMarkupSemanticState =
    evidence?.markup?.kind === "image" &&
    evidence?.markup?.mimeType === "image/png" &&
    recordMatches(evidence?.input?.pdf_point, command?.placement?.point) &&
    nativeLayout.native_layout_semantics_match &&
    evidence?.history_delta === 1 &&
    markupSha256 === checkerSha256;
  const rendererResourceSubmissionObserved =
    exactCheckerAsset &&
    exactMarkupSemanticState &&
    presentation.image_resource_element_present === true &&
    presentation.element_present === true &&
    presentation.visible === true &&
    presentation.animation_frames_after_markup >= 2;
  const presentedAfterNativeInput =
    evidence?.native_input_completed === true &&
    evidence?.input?.input_lane === nativeX11InputLane &&
    rendererResourceSubmissionObserved;

  return {
    receipt_kind: "electron-app-renderer-image-resource-submission-v1",
    evidence_scope:
      "app-level-renderer-resource-submission-not-physical-gpu-upload",
    asset: {
      asset_id: command?.asset_id ?? null,
      encoded_sha256: checkerSha256,
      markup_payload_sha256: markupSha256,
      encoded_bytes: checkerBytes?.length ?? null,
      source_width_px: electronImageCheckerAsset.source_width_px,
      source_height_px: electronImageCheckerAsset.source_height_px,
      decoded_rgba_payload_bytes: decodedRgbaPayloadBytes,
      decoded_rgba_payload_basis:
        "browser-decoded-image-drawn-to-canvas-image-data-rgba8-byte-length",
    },
    exact_checker_asset: exactCheckerAsset,
    exact_markup_semantic_state: exactMarkupSemanticState,
    renderer_resource_submission_observed: rendererResourceSubmissionObserved,
    decoded_payload_bytes: decodedRgbaPayloadBytes,
    renderer_resource_submission_bytes: rendererResourceSubmissionObserved
      ? decodedRgbaPayloadBytes
      : null,
    presented_after_native_input: presentedAfterNativeInput,
    physical_bus_upload_bytes: null,
    legacy_chromium_trace: evidence?.legacy_chromium_trace ?? null,
    passed:
      exactCheckerAsset &&
      exactMarkupSemanticState &&
      presentedAfterNativeInput,
  };
}

export function buildElectronImageResizeRendererResourceSubmissionReceipt(
  command,
  evidence,
) {
  const checkerBytes =
    evidence?.checker_asset_bytes == null
      ? null
      : Buffer.from(evidence.checker_asset_bytes);
  const markup = evidence?.after_redo_markup ?? null;
  const markupBytes = decodePngDataUrl(markup?.dataUrl);
  const checkerSha256 =
    checkerBytes == null
      ? null
      : createHash("sha256").update(checkerBytes).digest("hex");
  const markupSha256 =
    markupBytes == null
      ? null
      : createHash("sha256").update(markupBytes).digest("hex");
  const presentation = evidence?.presentation ?? {};
  const decodedRgbaPayloadBytes = Number.isInteger(
    presentation.decoded_rgba_payload_bytes,
  )
    ? presentation.decoded_rgba_payload_bytes
    : null;
  const expectedDecodedBytes =
    command?.resource_observation?.decoded_payload_bytes ?? 786432;
  const expectedRendererBytes =
    command?.resource_observation?.renderer_resource_submission_bytes ?? 786432;
  const exactCheckerAsset =
    command?.id === "image:resize-history" &&
    checkerSha256 === electronImageCheckerAsset.encoded_sha256 &&
    checkerBytes?.length === electronImageCheckerAsset.encoded_bytes &&
    markupSha256 === checkerSha256 &&
    markupBytes?.length === checkerBytes.length &&
    presentation.decoded_width === electronImageCheckerAsset.source_width_px &&
    presentation.decoded_height ===
      electronImageCheckerAsset.source_height_px &&
    decodedRgbaPayloadBytes === expectedDecodedBytes;
  const exactResizeSemanticState =
    markup?.kind === "image" &&
    markup?.mimeType === "image/png" &&
    recordMatches(markup?.rect, command?.replacement_bounds);
  const rendererResourceSubmissionObserved =
    exactCheckerAsset &&
    exactResizeSemanticState &&
    presentation.image_resource_element_present === true &&
    presentation.element_present === true &&
    presentation.visible === true &&
    presentation.animation_frames_after_markup >= 2 &&
    decodedRgbaPayloadBytes === expectedRendererBytes;

  return {
    receipt_kind: "electron-app-renderer-image-resize-resource-submission-v1",
    evidence_scope:
      "app-level-renderer-resource-submission-after-semantic-edit-not-physical-gpu-upload",
    asset: {
      asset_id: electronImageCheckerAsset.asset_id,
      encoded_sha256: checkerSha256,
      markup_payload_sha256: markupSha256,
      encoded_bytes: checkerBytes?.length ?? null,
      source_width_px: electronImageCheckerAsset.source_width_px,
      source_height_px: electronImageCheckerAsset.source_height_px,
      decoded_rgba_payload_bytes: decodedRgbaPayloadBytes,
      decoded_rgba_payload_basis:
        "browser-decoded-image-drawn-to-canvas-image-data-rgba8-byte-length",
    },
    exact_checker_asset: exactCheckerAsset,
    exact_resize_semantic_state: exactResizeSemanticState,
    renderer_resource_submission_observed: rendererResourceSubmissionObserved,
    decoded_payload_bytes: decodedRgbaPayloadBytes,
    renderer_resource_submission_bytes: rendererResourceSubmissionObserved
      ? decodedRgbaPayloadBytes
      : null,
    presented_after_semantic_edit: rendererResourceSubmissionObserved,
    presented_after_native_input: false,
    physical_bus_upload_bytes: null,
    passed: rendererResourceSubmissionObserved,
  };
}

export function classifyEditorCreateBoundsContract(
  command,
  directPlacementMarkup,
  pageSize = null,
  input = null,
) {
  if (!["text:create", "image:create"].includes(command?.id)) {
    throw new Error(
      "bounds contract classification supports Text and Image create only",
    );
  }
  const rect = directPlacementMarkup?.rect;
  const expectedImageRect =
    command.id === "image:create"
      ? expectedNaturalImagePlacement(command, pageSize)
      : null;
  const nativePixelsPerPdfPoint =
    input?.input_lane === nativeX11InputLane &&
    Number.isFinite(input?.surface?.pixels_per_pdf_point) &&
    input.surface.pixels_per_pdf_point > 0
      ? input.surface.pixels_per_pdf_point
      : null;
  const nativePositionTolerancePt =
    nativePixelsPerPdfPoint === null ? null : 0.5 / nativePixelsPerPdfPoint;
  const expectedImageCenter = expectedImageRect
    ? {
        x: expectedImageRect.x + expectedImageRect.width / 2,
        y: expectedImageRect.y + expectedImageRect.height / 2,
      }
    : null;
  const observedImageCenter = rect
    ? { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 }
    : null;
  const dimensionsExact =
    expectedImageRect !== null &&
    closeEnough(rect?.width, expectedImageRect.width) &&
    closeEnough(rect?.height, expectedImageRect.height);
  const aspectRatioExact =
    expectedImageRect !== null &&
    Number.isFinite(rect?.width) &&
    Number.isFinite(rect?.height) &&
    rect.height > 0 &&
    closeEnough(
      rect.width / rect.height,
      expectedImageRect.width / expectedImageRect.height,
      0.0001,
    );
  const effectivePageSize =
    pageSize ?? command?.placement?.fixture_page_size_points;
  const pageFractionHeld =
    expectedImageRect !== null &&
    effectivePageSize != null &&
    rect?.x >= -0.05 &&
    rect?.y >= -0.05 &&
    rect?.x + rect?.width <= effectivePageSize.width + 0.05 &&
    rect?.y + rect?.height <= effectivePageSize.height + 0.05 &&
    rect?.width <=
      effectivePageSize.width * command.placement.max_page_fraction + 0.05 &&
    rect?.height <=
      effectivePageSize.height * command.placement.max_page_fraction + 0.05;
  const positionMatches =
    expectedImageCenter !== null &&
    observedImageCenter !== null &&
    closeEnough(
      observedImageCenter.x,
      expectedImageCenter.x,
      nativePositionTolerancePt ?? 0.05,
    ) &&
    closeEnough(
      observedImageCenter.y,
      expectedImageCenter.y,
      nativePositionTolerancePt ?? 0.05,
    );
  const matches =
    command.id === "text:create"
      ? command.placement?.sizing === "shaped-text-autosize-nonblank" &&
        directPlacementMarkup?.kind === "text-box" &&
        rect?.width > 0 &&
        rect?.height > 0 &&
        pointInsideRect(command.placement.point, rect)
      : directPlacementMarkup?.kind === "image" &&
        dimensionsExact &&
        aspectRatioExact &&
        pageFractionHeld &&
        positionMatches;
  return {
    command_id: command.id,
    native_layout_semantics_match: matches,
    frozen_single_transaction_replayable: matches,
    status: matches ? "frozen-command-replayable" : "native-layout-mismatch",
    reason: matches
      ? null
      : "observed-direct-placement-does-not-match-the-versioned-native-layout-contract",
    proposed_change: null,
    ...(command.id === "image:create" &&
    nativePixelsPerPdfPoint !== null &&
    expectedImageCenter !== null &&
    observedImageCenter !== null
      ? {
          native_pixel_rounding: {
            observation_basis:
              "integer-XTest-coordinate-rounding-after-window-to-PDF-mapping",
            pixels_per_pdf_point: nativePixelsPerPdfPoint,
            max_per_axis_px: 0.5,
            max_per_axis_pt: nativePositionTolerancePt,
            center_deviation_pt: {
              x: Math.abs(observedImageCenter.x - expectedImageCenter.x),
              y: Math.abs(observedImageCenter.y - expectedImageCenter.y),
            },
            center_deviation_px: {
              x:
                Math.abs(observedImageCenter.x - expectedImageCenter.x) *
                nativePixelsPerPdfPoint,
              y:
                Math.abs(observedImageCenter.y - expectedImageCenter.y) *
                nativePixelsPerPdfPoint,
            },
            dimensions_exact: dimensionsExact,
            aspect_ratio_exact: aspectRatioExact,
            page_fraction_held: pageFractionHeld,
          },
        }
      : {}),
  };
}

export function assessElectronEditorCreateEvidence(command, evidence) {
  const markup = evidence?.markup ?? null;
  const presentation = evidence?.presentation ?? {};
  const historyDelta = evidence?.history_delta;
  if (command.id === "text:create") {
    const font = command.font;
    const nativeLayout = classifyEditorCreateBoundsContract(
      command,
      markup,
      evidence?.page_size,
      evidence?.input,
    );
    return {
      exact_fields: {
        kind: markup?.kind === "text-box",
        text: markup?.text === command.text,
        placement_point: recordMatches(
          evidence?.input?.pdf_point,
          command.placement.point,
        ),
        native_layout: nativeLayout.native_layout_semantics_match,
        font_family: markup?.fontFamily === font.family,
        font_size: closeEnough(markup?.fontSizePt, font.size_pt),
        font_color:
          normalizedColor(markup?.color ?? markup?.appearance?.text?.color) ===
          font.color,
        alignment:
          (markup?.textAlign ?? markup?.appearance?.text?.align ?? "left") ===
          font.alignment,
        history_delta: historyDelta === 1,
      },
      milestones: {
        "text-input-committed": markup?.text === command.text,
        "text-shaped":
          presentation.text_content === command.text &&
          presentation.computed_text_length > 0 &&
          presentation.bbox?.width > 0 &&
          presentation.bbox?.height > 0,
        "annotation-painted":
          presentation.element_present === true &&
          presentation.visible === true,
        "gesture-committed-once": historyDelta === 1,
      },
    };
  }
  if (command.id === "length:set-scale") {
    const scale = evidence?.scale;
    return {
      exact_fields: {
        paper_points: closeEnough(
          scale?.paper_points,
          command.scale.paper_points,
        ),
        real_world_value: closeEnough(
          scale?.real_world_value,
          command.scale.real_world_value,
        ),
        unit: scale?.unit === command.scale.unit,
        precision: scale?.precision === command.scale.precision,
      },
      milestones: {
        "measurement-scale-current": recordMatches(scale, command.scale),
      },
    };
  }
  if (command.id === "length:create") {
    return {
      exact_fields: {
        kind: markup?.kind === "length",
        start: recordMatches(markup?.start, command.start),
        finish: recordMatches(markup?.end, command.finish),
        label: presentation.text_content === command.expected_label,
        history_delta: historyDelta === 1,
      },
      milestones: {
        "derived-length-exact":
          presentation.text_content === command.expected_label,
        "label-layout-current":
          presentation.bbox?.width > 0 && presentation.bbox?.height > 0,
        "gesture-committed-once": historyDelta === 1,
      },
    };
  }
  if (command.id === "image:create") {
    const nativeLayout = classifyEditorCreateBoundsContract(
      command,
      markup,
      evidence?.page_size,
      evidence?.input,
    );
    const receipt = evidence?.renderer_resource_submission_receipt ?? null;
    return {
      exact_fields: {
        kind: markup?.kind === "image",
        placement_point: recordMatches(
          evidence?.input?.pdf_point,
          command.placement.point,
        ),
        native_layout: nativeLayout.native_layout_semantics_match,
        mime_type: markup?.mimeType === "image/png",
        decoded_width: presentation.decoded_width === 512,
        decoded_height: presentation.decoded_height === 384,
        decoded_rgba_payload_bytes:
          receipt?.asset?.decoded_rgba_payload_bytes === 786432,
        exact_checker_asset: receipt?.exact_checker_asset === true,
        exact_markup_semantic_state:
          receipt?.exact_markup_semantic_state === true,
        renderer_resource_submission:
          receipt?.renderer_resource_submission_observed === true,
        presented_after_native_input:
          receipt?.presented_after_native_input === true,
        physical_bus_upload_unclaimed:
          receipt?.physical_bus_upload_bytes === null,
        history_delta: historyDelta === 1,
      },
      milestones: {
        "bitmap-decoded":
          presentation.decoded_width === 512 &&
          presentation.decoded_height === 384,
        "bitmap-upload-recorded": receipt?.passed === true,
        "decoded-payload-bytes-exact":
          receipt?.decoded_payload_bytes ===
          (command.resource_observation?.decoded_payload_bytes ?? 786432),
        "renderer-resource-submission-bytes-exact":
          receipt?.renderer_resource_submission_bytes ===
          (command.resource_observation?.renderer_resource_submission_bytes ??
            786432),
        "annotation-presented": receipt?.presented_after_native_input === true,
        "gesture-committed-once": historyDelta === 1,
        "annotation-painted":
          presentation.element_present === true &&
          presentation.visible === true,
      },
      renderer_resource_submission_receipt: receipt,
    };
  }
  throw new Error(`unsupported editor create command ${command.id}`);
}

export function electronHighlightSmoothingEvidence(
  markup,
  submittedPointCount,
  nativeGeometry = null,
) {
  const persistedPointCount = markup?.paths?.[0]?.length ?? 0;
  const sampleReductionObserved =
    Number.isInteger(submittedPointCount) &&
    submittedPointCount > 2 &&
    persistedPointCount > 2 &&
    persistedPointCount < submittedPointCount;
  const nativeGeometryRequired = nativeGeometry !== null;
  return {
    passed:
      markup?.kind === "highlight" &&
      sampleReductionObserved &&
      (!nativeGeometryRequired || nativeGeometry?.matched === true),
    submitted_point_count: submittedPointCount,
    persisted_point_count: persistedPointCount,
    sample_reduction_observed: sampleReductionObserved,
    native_geometry_required: nativeGeometryRequired,
    native_geometry: nativeGeometry,
    observation_basis: nativeGeometryRequired
      ? "sample reduction plus the maintained 64-point native geometry oracle"
      : "the maintained annotation pipeline retained a nontrivial path while reducing the submitted 120 Hz sample stream",
  };
}

function annotationCorrectness(command, markup, samples, nativeSurface = null) {
  const style = benchmarkStyleMatches(command, markup.appearance);
  const nativeGeometry = nativeSurface
    ? compareNativeBenchmarkGeometry(command, markup, samples, nativeSurface)
    : null;
  if (command.id === "rectangle:create-sparse") {
    const expected = {
      x: Math.min(command.pointer_path.start.x, command.pointer_path.finish.x),
      y: Math.min(command.pointer_path.start.y, command.pointer_path.finish.y),
      width: Math.abs(
        command.pointer_path.finish.x - command.pointer_path.start.x,
      ),
      height: Math.abs(
        command.pointer_path.finish.y - command.pointer_path.start.y,
      ),
    };
    return {
      kind_matched: markup.kind === "rectangle",
      geometry_matched:
        nativeGeometry?.matched ??
        (markup.kind === "rectangle" &&
          Object.keys(expected).every((key) =>
            closeEnough(markup.rect?.[key], expected[key]),
          )),
      expected_geometry: expected,
      observed_geometry: markup.rect ?? null,
      expected_style: style.expected,
      observed_style: style.observed,
      style_matched: style.matched,
      style_contract_version: benchmarkStyleContractVersion,
      native_geometry: nativeGeometry,
    };
  }
  const path = markup.paths?.[0] ?? [];
  const pointsMatched =
    path.length === samples.length &&
    path.every(
      (point, index) =>
        closeEnough(point.x, samples[index].x) &&
        closeEnough(point.y, samples[index].y),
    );
  return {
    kind_matched: markup.kind === "highlight",
    geometry_matched:
      nativeGeometry?.matched ?? (markup.kind === "highlight" && pointsMatched),
    expected_point_count: samples.length,
    observed_point_count: path.length,
    expected_start: samples[0],
    observed_start: path[0] ?? null,
    expected_finish: samples.at(-1),
    observed_finish: path.at(-1) ?? null,
    expected_style: style.expected,
    observed_style: style.observed,
    style_matched: style.matched,
    style_contract_version: benchmarkStyleContractVersion,
    native_geometry: nativeGeometry,
  };
}

async function waitForMarkupCount(cdp, expectedCount, timeoutMs = 5_000) {
  return cdp.evaluate(`(async () => {
    const deadline = performance.now() + ${timeoutMs};
    while (performance.now() < deadline) {
      const documentModel = window.__butterPaperTestHooks.getActiveDocument();
      if (documentModel?.markups?.length === ${expectedCount}) return documentModel;
      await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    throw new Error('Markup count did not become ${expectedCount}');
  })()`);
}

async function runElectronAnnotationCreate(cdp, contract, event) {
  await dispatchCdpClick(cdp, "viewer-fit-page");
  await cdp.evaluate(`(async () => {
    const deadline = performance.now() + 5000;
    while (performance.now() < deadline) {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      if (diagnostics.zoomPreset === 'fit-page' && diagnostics.pageRenderReady) {
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        return;
      }
      await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    throw new Error('Fit Page did not settle before annotation replay');
  })()`);
  const initial = await cdp.evaluate(
    `window.__butterPaperTestHooks.getActiveDocument()`,
  );
  const results = [];
  const aliasEntries = [];
  await cdp.evaluate(`window.__butterPaperTestHooks.setSnapSettings({
    snapToContent: false,
    snapToMarkup: false,
    snapToPageGrid: false,
    dimensionIncrementEnabled: false,
    snapGuidesEnabled: false,
  })`);

  for (const command of contract.commands) {
    const before = await cdp.evaluate(
      `window.__butterPaperTestHooks.getActiveDocument()`,
    );
    const toolTestId = command.id.startsWith("rectangle:")
      ? "tool-rectangle"
      : "tool-highlight";
    await dispatchCdpClick(cdp, toolTestId);
    const tool = await cdp.evaluate(
      `window.__butterPaperTestHooks.getDiagnostics().activeTool`,
    );
    if (
      tool !== (command.id.startsWith("rectangle:") ? "rectangle" : "highlight")
    ) {
      throw new Error(`${command.id} tool selection did not become active`);
    }
    const samples = buildAnnotationPointerSamples(command);
    const input = await dispatchAnnotationPointerStream(
      cdp,
      samples,
      event,
      command.id,
    );
    const documentModel = await waitForMarkupCount(
      cdp,
      before.markups.length + 1,
    );
    const markup = documentModel.markups.find(
      ({ id }) => !before.markups.some((existing) => existing.id === id),
    );
    if (!markup)
      throw new Error(
        `${command.id} did not create exactly one identifiable markup`,
      );
    await cdp.evaluate(
      `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
    );
    const painted = await cdp.evaluate(
      `Boolean(document.querySelector(${JSON.stringify(`[data-testid="markup-${markup.id}"]`)}))`,
    );
    const correctness = annotationCorrectness(command, markup, samples);
    aliasEntries.push({
      command_id: command.id,
      canonical_id: command.annotation_id,
      observed_id: markup.id,
    });
    const aliases = createBenchmarkAliasMap(aliasEntries);
    const result = {
      command_id: command.id,
      input,
      observed_markup_id: markup.id,
      requested_annotation_id: command.annotation_id,
      markup,
      correctness,
      observed_milestones: [],
      milestones: [],
      exact_fields: {
        pointer_path: input.rate_schedule_met,
        annotation_id: aliases.observedId(command.annotation_id) === markup.id,
        geometry: correctness.geometry_matched,
        style: correctness.style_matched,
      },
      benchmark_alias: aliasEntries.at(-1),
      exact_field_blockers: [],
    };
    addMilestone(
      event,
      result,
      "pointer-stream-received",
      input.acknowledged_sample_count ===
        command.pointer_path.expected_sample_count,
      {
        input_lane: input.input_lane,
        acknowledged_sample_count: input.acknowledged_sample_count,
      },
    );
    if (command.id === "highlight:create") {
      const smoothing = electronHighlightSmoothingEvidence(
        markup,
        samples.length,
      );
      addMilestone(event, result, "path-smoothed", smoothing.passed, {
        interpolation: command.pointer_path.interpolation,
        ...smoothing,
      });
    }
    addMilestone(
      event,
      result,
      "gesture-committed-once",
      documentModel.markups.length === before.markups.length + 1,
      {
        before_markup_count: before.markups.length,
        after_markup_count: documentModel.markups.length,
      },
    );
    addMilestone(event, result, "annotation-painted", painted, {
      markup_id: markup.id,
    });

    if (command.id === "rectangle:create-sparse") {
      const leftSidebarOpen = await cdp.evaluate(
        `Boolean(document.querySelector('[data-testid="left-sidebar"]'))`,
      );
      if (!leftSidebarOpen) await dispatchCdpClick(cdp, "left-rail-pages");
      const thumbnailPainted = await cdp.evaluate(`(async () => {
        const selector = ${JSON.stringify(`[data-testid="thumbnail-markup-${markup.id}"]`)};
        const deadline = performance.now() + 5000;
        while (performance.now() < deadline) {
          if (document.querySelector(selector)) return true;
          await new Promise((resolve) => requestAnimationFrame(resolve));
        }
        return false;
      })()`);
      addMilestone(event, result, "thumbnail-current", thumbnailPainted, {
        markup_id: markup.id,
      });
    }
    finalizeCommandMilestones(result, command);
    results.push(result);
  }
  return {
    contract,
    input_lane: "cdp-input-diagnostic",
    command_results: results,
    initial_markup_count: initial.markups.length,
    final_markup_count: initial.markups.length + results.length,
    exact_manifest_replay: results.every(
      ({ exact_fields, manifest_milestones_complete }) =>
        manifest_milestones_complete &&
        Object.values(exact_fields).every(Boolean),
    ),
    benchmark_alias_map: createBenchmarkAliasMap(aliasEntries).entries,
    style_contract_version: benchmarkStyleContractVersion,
    blocker: null,
  };
}

async function dispatchHistoryShortcut(cdp, redo) {
  const modifiers = redo ? 2 | 8 : 2;
  await cdp.send("Input.dispatchKeyEvent", {
    type: "keyDown",
    key: redo ? "Z" : "z",
    code: "KeyZ",
    modifiers,
  });
  await cdp.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: redo ? "Z" : "z",
    code: "KeyZ",
    modifiers,
  });
  await cdp.evaluate(
    `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
  );
}

async function activeMarkup(cdp, id) {
  return cdp.evaluate(`window.__butterPaperTestHooks.getActiveDocument()?.markups.find(
    (markup) => markup.id === ${JSON.stringify(id)}
  ) ?? null`);
}

export function rectangleMoveEdgePoint(markup) {
  const rect = markup?.rect;
  if (
    !rect ||
    ![rect.x, rect.y, rect.width, rect.height].every(Number.isFinite)
  ) {
    throw new Error("rectangle move edge point requires finite markup bounds");
  }
  return {
    x: rect.x + rect.width * 0.25,
    y: rect.y + rect.height,
  };
}

export function assessElectronNativeSelectPoint({
  observed,
  expected,
  surface,
  devicePixelRatio = 1,
}) {
  if (
    ![
      observed?.x,
      observed?.y,
      expected?.x,
      expected?.y,
      surface?.pixels_per_point_x,
      surface?.pixels_per_point_y,
      devicePixelRatio,
    ].every((value) => Number.isFinite(value) && value > 0)
  ) {
    throw new Error(
      "native select point assessment requires finite positive coordinates and surface scale",
    );
  }
  const devicePixels = Number.isInteger(devicePixelRatio) ? 1 : 2;
  const tolerancePt =
    (Math.max(1 / surface.pixels_per_point_x, 1 / surface.pixels_per_point_y) *
      devicePixels) /
    devicePixelRatio;
  const delta = {
    x: observed.x - expected.x,
    y: observed.y - expected.y,
  };
  return {
    delta,
    tolerance_pt: tolerancePt,
    passed:
      Math.abs(delta.x) <= tolerancePt && Math.abs(delta.y) <= tolerancePt,
  };
}

export function annotationGeometryTolerancePt(input, devicePixelRatio = 1) {
  const surface = input?.coordinate_mapping?.surface;
  const viewBox = surface?.view_box;
  const bounds = surface?.bounds;
  if (
    ![
      viewBox?.width,
      viewBox?.height,
      bounds?.width,
      bounds?.height,
      devicePixelRatio,
    ].every((value) => Number.isFinite(value) && value > 0)
  ) {
    throw new Error(
      "annotation geometry tolerance requires finite surface scale",
    );
  }
  const devicePixels = Number.isInteger(devicePixelRatio) ? 1 : 2;
  return (
    (Math.max(viewBox.width / bounds.width, viewBox.height / bounds.height) *
      devicePixels) /
    devicePixelRatio
  );
}

export function buildElectronRectanglePropertiesUiPlan(command) {
  const splitRgba = (value, label) => {
    if (typeof value !== "string" || !/^#[0-9a-f]{8}$/i.test(value)) {
      throw new Error(`${label} must be an exact #rrggbbaa color`);
    }
    return {
      color: value.slice(0, 7).toLowerCase(),
      opacity_percent: (Number.parseInt(value.slice(7), 16) / 255) * 100,
    };
  };
  const stroke = splitRgba(command?.properties?.stroke, "rectangle stroke");
  const fill = splitRgba(command?.properties?.fill, "rectangle fill");
  const dash = command?.properties?.dash;
  if (!["solid", "dashed", "dotted", "cloud"].includes(dash)) {
    throw new Error(`unsupported rectangle line style ${dash}`);
  }
  return [
    { kind: "color", label: "Color", value: stroke.color },
    { kind: "color", label: "Fill Color", value: fill.color },
    { kind: "number", label: "Line Width", value: command.properties.width_pt },
    {
      kind: "select",
      label: "Line Style",
      value: dash[0].toUpperCase() + dash.slice(1),
    },
    {
      kind: "number",
      label: "Opacity",
      value: command.properties.opacity * 100,
    },
    { kind: "number", label: "Fill Opacity", value: fill.opacity_percent },
  ];
}

export function resolveLabeledPropertyControl(
  root,
  label,
  preferredSlot = null,
) {
  const fieldLabel = [...(root?.querySelectorAll("label") ?? [])].find(
    (candidate) => candidate.textContent?.trim() === label,
  );
  const associatedControl = fieldLabel?.htmlFor
    ? root?.ownerDocument?.getElementById(fieldLabel.htmlFor)
    : null;
  const preferredControl = preferredSlot
    ? fieldLabel
        ?.closest('[data-slot="field"]')
        ?.querySelector(`[data-slot="${preferredSlot}"]`)
    : null;
  return preferredControl ?? associatedControl;
}

async function dispatchCdpLabeledControlClick(
  cdp,
  label,
  preferredSlot = null,
) {
  const point = await cdp.evaluate(`(() => {
    const root = document.querySelector('[data-testid="right-sidebar"]');
    const control = (${resolveLabeledPropertyControl.toString()})(
      root,
      ${JSON.stringify(label)},
      ${JSON.stringify(preferredSlot)}
    );
    if (!control) throw new Error(${JSON.stringify(`Missing selected property control ${label}`)});
    control.scrollIntoView({ block: 'center' });
    const bounds = control.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) {
      throw new Error(${JSON.stringify(`Selected property control ${label} is not visible`)});
    }
    return { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2 };
  })()`);
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", ...point });
  await cdp.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    ...point,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await cdp.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    ...point,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
  await cdp.evaluate(
    `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
  );
}

async function setCdpLabeledNumber(cdp, label, value) {
  await cdp.evaluate(`(() => {
    const root = document.querySelector('[data-testid="right-sidebar"]');
    const fieldLabel = [...(root?.querySelectorAll('label') ?? [])]
      .find((candidate) => candidate.textContent?.trim() === ${JSON.stringify(label)});
    const input = fieldLabel?.htmlFor ? document.getElementById(fieldLabel.htmlFor) : null;
    if (!(input instanceof HTMLInputElement)) {
      throw new Error(${JSON.stringify(`Missing selected property input ${label}`)});
    }
    input.scrollIntoView({ block: 'center' });
    input.focus();
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(
      input,
      ${JSON.stringify(String(value))},
    );
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
    input.blur();
  })()`);
  await cdp.evaluate(
    `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
  );
}

async function setCdpSelectedColor(cdp, label, value) {
  await dispatchCdpLabeledControlClick(cdp, label);
  await waitForEditorCondition(
    cdp,
    `([...document.querySelectorAll('[data-slot="popover-content"]')]
      .some((element) => element.getBoundingClientRect().width > 0 && element.getBoundingClientRect().height > 0))`,
    `${label} color popover`,
  );
  await dispatchCdpTextClick(cdp, "Add");
  await waitForEditorCondition(
    cdp,
    `Boolean(document.querySelector('[aria-label="Custom preset hex color"]'))`,
    `${label} custom color editor`,
  );
  await setCdpInputValue(cdp, '[aria-label="Custom preset hex color"]', value);
  await cdp.evaluate(
    `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
  );
  await dispatchCdpTextClick(cdp, "Add");
  await dispatchCdpLabeledControlClick(cdp, label);
  await waitForEditorCondition(
    cdp,
    `(![...document.querySelectorAll('[data-slot="popover-content"]')]
      .some((element) => element.getBoundingClientRect().width > 0 && element.getBoundingClientRect().height > 0))`,
    `${label} color popover close`,
  );
}

async function applyElectronRectanglePropertiesUiPlan(cdp, plan) {
  for (const step of plan) {
    if (step.kind === "color") {
      await setCdpSelectedColor(cdp, step.label, step.value);
    } else if (step.kind === "number") {
      await setCdpLabeledNumber(cdp, step.label, step.value);
    } else if (step.kind === "select") {
      await dispatchCdpLabeledControlClick(cdp, step.label);
      await dispatchCdpTextClick(cdp, step.value);
    } else {
      throw new Error(`unsupported selected property UI step ${step.kind}`);
    }
  }
}

export async function replayHistoryToBoundary(
  { snapshot, canDispatch, dispatch },
  limit = 32,
) {
  let previous = await snapshot();
  let changed = 0;
  let stateChanges = 0;
  for (let attempt = 0; attempt < limit; attempt += 1) {
    if (!(await canDispatch()))
      return { changed, state_changes: stateChanges, boundary_reached: true };
    await dispatch();
    const current = await snapshot();
    changed += 1;
    if (current !== previous) stateChanges += 1;
    previous = current;
  }
  return { changed, state_changes: stateChanges, boundary_reached: false };
}

export function sameDocumentHistoryPosition(before, after) {
  return (
    Boolean(before && after) &&
    before.past === after.past &&
    before.future === after.future &&
    before.currentRevision === after.currentRevision &&
    before.savedRevision === after.savedRevision
  );
}

async function prepareElectronHistoryMenuAction(cdp, redo) {
  const label = redo ? "Redo" : "Undo";
  await dispatchCdpClick(cdp, "menu-trigger-edit");
  await waitForEditorCondition(
    cdp,
    `([...document.querySelectorAll('[data-slot="menubar-content"]')]
      .some((element) => element.getBoundingClientRect().width > 0 && element.getBoundingClientRect().height > 0))`,
    "Edit menu open",
  );
  const state = await cdp.evaluate(`(() => {
    const item = [...document.querySelectorAll('[data-slot="menubar-item"]')]
      .find((candidate) => {
        const bounds = candidate.getBoundingClientRect();
        return candidate.textContent?.trim().startsWith(${JSON.stringify(label)})
          && bounds.width > 0 && bounds.height > 0;
      });
    if (!item) throw new Error(${JSON.stringify(`Missing visible ${label} menu item`)});
    return {
      disabled: item.hasAttribute('data-disabled') || item.getAttribute('aria-disabled') === 'true',
    };
  })()`);
  if (state.disabled) {
    await cdp.send("Input.dispatchKeyEvent", {
      type: "keyDown",
      key: "Escape",
      code: "Escape",
    });
    await cdp.send("Input.dispatchKeyEvent", {
      type: "keyUp",
      key: "Escape",
      code: "Escape",
    });
    await waitForEditorCondition(
      cdp,
      `(![...document.querySelectorAll('[data-slot="menubar-content"]')]
        .some((element) => element.getBoundingClientRect().width > 0 && element.getBoundingClientRect().height > 0))`,
      "Edit menu close at history boundary",
    );
    return false;
  }
  return true;
}

async function dispatchElectronHistoryMenuAction(cdp, redo) {
  const label = redo ? "Redo" : "Undo";
  const point = await cdp.evaluate(`(() => {
    const item = [...document.querySelectorAll('[data-slot="menubar-item"]')]
      .find((candidate) => {
        const bounds = candidate.getBoundingClientRect();
        return candidate.textContent?.trim().startsWith(${JSON.stringify(label)})
          && bounds.width > 0 && bounds.height > 0;
      });
    if (!item || item.hasAttribute('data-disabled') || item.getAttribute('aria-disabled') === 'true') {
      throw new Error(${JSON.stringify(`${label} menu item is not actionable`)});
    }
    const bounds = item.getBoundingClientRect();
    return { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2 };
  })()`);
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", ...point });
  await cdp.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    ...point,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await cdp.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    ...point,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
  await waitForEditorCondition(
    cdp,
    `(![...document.querySelectorAll('[data-slot="menubar-content"]')]
      .some((element) => element.getBoundingClientRect().width > 0 && element.getBoundingClientRect().height > 0))`,
    `${label} menu close`,
  );
  await cdp.evaluate(
    `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
  );
}

async function replayDocumentHistoryToBoundary(cdp, redo, limit = 32) {
  return replayHistoryToBoundary(
    {
      snapshot: () =>
        cdp.evaluate(
          `JSON.stringify(window.__butterPaperTestHooks.getActiveDocument())`,
        ),
      canDispatch: async () => {
        const history = await cdp.evaluate(
          `window.__butterPaperTestHooks.getDocumentHistory()`,
        );
        if ((redo ? history.future : history.past) === 0) return false;
        return prepareElectronHistoryMenuAction(cdp, redo);
      },
      dispatch: () => dispatchElectronHistoryMenuAction(cdp, redo),
    },
    limit,
  );
}

async function runElectronAnnotationTransform(cdp, contract, event) {
  const workload = await loadComparisonWorkload();
  const createCommand = workload.journeys
    .flatMap(({ commands }) => commands)
    .find(({ id }) => id === "rectangle:create-sparse");
  if (!createCommand)
    throw new Error(
      "rectangle transform prerequisite create command is missing",
    );
  const creation = await runElectronAnnotationCreate(
    cdp,
    {
      ...contract,
      command_ids: [createCommand.id],
      commands: [createCommand],
    },
    event,
  );
  const prerequisite = rectangleTransformPrerequisiteEvidence(creation);
  if (!prerequisite.passed) {
    throw new Error(
      "rectangle transform prerequisite did not pass exact creation gates",
    );
  }
  const observedId = prerequisite.observed_markup_id;
  const command = contract.commands[0];
  const aliasedCommand = { ...command, annotation_id: observedId };
  const before = await activeMarkup(cdp, observedId);
  await dispatchCdpClick(cdp, "tool-select");
  const moveStart = rectangleMoveEdgePoint(before);
  await dispatchPdfPointClick(cdp, moveStart);
  const selectedBeforeMove = await cdp.evaluate(
    `window.__butterPaperTestHooks.getDiagnostics().selectedMarkupIds.includes(${JSON.stringify(observedId)})`,
  );

  const moveSamples = buildDeltaPointerStream({
    start: moveStart,
    delta: command.move.delta,
    rate_hz: command.move.rate_hz,
    duration_ms: command.move.duration_ms,
  });
  const moveInput = await dispatchAnnotationPointerStream(
    cdp,
    moveSamples,
    event,
    command.id,
  );
  const moved = await activeMarkup(cdp, observedId);
  const selectedAfterMove = await cdp.evaluate(
    `window.__butterPaperTestHooks.getDiagnostics().selectedMarkupIds.includes(${JSON.stringify(observedId)})`,
  );
  await dispatchHistoryShortcut(cdp, false);
  const moveUndone = await activeMarkup(cdp, observedId);
  await dispatchHistoryShortcut(cdp, true);
  const moveRedone = await activeMarkup(cdp, observedId);
  await dispatchPdfPointClick(cdp, rectangleMoveEdgePoint(moveRedone));
  const selectedBeforeResize = await cdp.evaluate(
    `window.__butterPaperTestHooks.getDiagnostics().selectedMarkupIds.includes(${JSON.stringify(observedId)})`,
  );

  const resizeStart = {
    x: moveRedone.rect.x + moveRedone.rect.width,
    y: moveRedone.rect.y + moveRedone.rect.height / 2,
  };
  const resizeSamples = buildDeltaPointerStream({
    start: resizeStart,
    delta: command.resize.delta,
    rate_hz: command.resize.rate_hz,
    duration_ms: command.resize.duration_ms,
  });
  const resizeInput = await dispatchAnnotationPointerStream(
    cdp,
    resizeSamples,
    event,
    command.id,
  );
  const resized = await activeMarkup(cdp, observedId);
  await dispatchHistoryShortcut(cdp, false);
  const resizeUndone = await activeMarkup(cdp, observedId);
  await dispatchHistoryShortcut(cdp, true);
  const resizeRedone = await activeMarkup(cdp, observedId);
  const devicePixelRatio = await cdp.evaluate(`window.devicePixelRatio`);
  const geometryTolerancePt = Math.max(
    annotationGeometryTolerancePt(moveInput, devicePixelRatio),
    annotationGeometryTolerancePt(resizeInput, devicePixelRatio),
  );

  const evidence = rectangleTransformEvidence(aliasedCommand, {
    before,
    moved: moveRedone,
    resized: resizeRedone,
    history_delta: {
      move:
        JSON.stringify(moveUndone) === JSON.stringify(before) &&
        JSON.stringify(moveRedone) === JSON.stringify(moved)
          ? 1
          : 0,
      resize:
        JSON.stringify(resizeUndone) === JSON.stringify(moved) &&
        JSON.stringify(resizeRedone) === JSON.stringify(resized)
          ? 1
          : 0,
    },
    geometry_tolerance_pt: geometryTolerancePt,
  });
  const result = {
    command_id: command.id,
    observed_markup_id: observedId,
    requested_annotation_id: command.annotation_id,
    benchmark_alias: creation.benchmark_alias_map[0],
    input: { move: moveInput, resize: resizeInput },
    evidence,
    observed_milestones: [],
    milestones: [],
    exact_fields: {
      alias:
        creation.benchmark_alias_map[0]?.canonical_id === command.annotation_id,
      selected: selectedBeforeMove && selectedAfterMove && selectedBeforeResize,
      move_rate: moveInput.rate_schedule_met,
      resize_rate: resizeInput.rate_schedule_met,
      geometry_and_history: evidence.passed,
    },
  };
  addMilestone(
    event,
    result,
    "hit-test-selected",
    evidence.hit_test_selected && selectedBeforeMove && selectedAfterMove,
    {
      observed_markup_id: observedId,
    },
  );
  addMilestone(
    event,
    result,
    "move-committed-once",
    evidence.move_committed_once,
    evidence,
  );
  addMilestone(
    event,
    result,
    "resize-committed-once",
    evidence.resize_committed_once,
    evidence,
  );
  finalizeCommandMilestones(result, command);
  return {
    contract,
    input_lane: cdpDiagnosticInputLane,
    prerequisite_creation: creation,
    command_results: [result],
    exact_manifest_replay:
      result.manifest_milestones_complete &&
      Object.values(result.exact_fields).every(Boolean),
  };
}

async function runElectronNativeAnnotationTransform(
  cdp,
  contract,
  event,
  target,
) {
  const workload = await loadComparisonWorkload();
  const createCommand = workload.journeys
    .flatMap(({ commands }) => commands)
    .find(({ id }) => id === "rectangle:create-sparse");
  if (!createCommand)
    throw new Error(
      "rectangle transform prerequisite create command is missing",
    );
  const creation = await runElectronNativeAnnotationCreate(
    cdp,
    {
      ...contract,
      command_ids: [createCommand.id],
      commands: [createCommand],
    },
    event,
    target,
  );
  const prerequisite = rectangleTransformPrerequisiteEvidence(creation);
  if (!prerequisite.passed) {
    throw new Error(
      "native rectangle transform prerequisite did not pass exact creation gates",
    );
  }

  const observedId = prerequisite.observed_markup_id;
  const command = contract.commands[0];
  const aliasedCommand = { ...command, annotation_id: observedId };
  const before = await activeMarkup(cdp, observedId);
  const declaredMoveStart = rectangleMoveEdgePoint(before);
  await nativeClickTestId(cdp, target, "tool-select");
  const surface = await electronAnnotationSurface(cdp);
  const devicePixelRatio = await cdp.evaluate(`window.devicePixelRatio`);
  const selectPointEvidence = assessElectronNativeSelectPoint({
    observed: declaredMoveStart,
    expected: command.select_point,
    surface,
    devicePixelRatio,
  });
  if (!selectPointEvidence.passed) {
    throw new Error(
      `native rectangle move hit point drifted beyond one device pixel: ${JSON.stringify(selectPointEvidence)}`,
    );
  }
  const historyBeforeMove = await cdp.evaluate(
    `window.__butterPaperTestHooks.getDocumentHistory()`,
  );
  const moveReplay = buildNativeRectangleTransformReplay({
    createCommand,
    transformCommand: command,
    ready: { stage: "move", surface },
    target,
  });
  const moveDurationMs = await runDirectXTestPointer(moveReplay, target);
  const moveTiming = assessReplayTiming(moveReplay, moveDurationMs);
  const afterMove = await cdp.evaluate(`(async () => {
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    return {
      markup: window.__butterPaperTestHooks.getActiveDocument()?.markups.find(
        (markup) => markup.id === ${JSON.stringify(observedId)}
      ) ?? null,
      history: window.__butterPaperTestHooks.getDocumentHistory(),
      selected: window.__butterPaperTestHooks.getDiagnostics().selectedMarkupIds.includes(${JSON.stringify(observedId)}),
    };
  })()`);
  if (!afterMove.markup)
    throw new Error("native rectangle move removed its target markup");

  const handlePoint = {
    x: afterMove.markup.rect.x + afterMove.markup.rect.width,
    y: afterMove.markup.rect.y + afterMove.markup.rect.height / 2,
  };
  const resizeReplay = buildNativeRectangleTransformReplay({
    createCommand,
    transformCommand: command,
    ready: {
      stage: "east-resize",
      surface: await electronAnnotationSurface(cdp),
      handle_point: handlePoint,
    },
    target,
  });
  const resizeDurationMs = await runDirectXTestPointer(resizeReplay, target);
  const resizeTiming = assessReplayTiming(resizeReplay, resizeDurationMs);
  const final = await cdp.evaluate(`(async () => {
    await new Promise((resolve) => setTimeout(resolve, 250));
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const markup = window.__butterPaperTestHooks.getActiveDocument()?.markups.find(
      (candidate) => candidate.id === ${JSON.stringify(observedId)}
    ) ?? null;
    const element = document.querySelector(${JSON.stringify(`[data-testid="markup-${observedId}"]`)});
    const bounds = element?.getBoundingClientRect();
    return {
      markup,
      history: window.__butterPaperTestHooks.getDocumentHistory(),
      selected: window.__butterPaperTestHooks.getDiagnostics().selectedMarkupIds.includes(${JSON.stringify(observedId)}),
      presentation_current: Boolean(element && bounds && bounds.width > 0 && bounds.height > 0),
    };
  })()`);
  if (!final.markup)
    throw new Error("native rectangle resize removed its target markup");

  const toleranceDevicePixels = Number.isInteger(devicePixelRatio) ? 1 : 2;
  const geometryTolerancePt =
    (Math.max(1 / surface.pixels_per_point_x, 1 / surface.pixels_per_point_y) *
      toleranceDevicePixels) /
    devicePixelRatio;
  const moveHistoryDelta = afterMove.history.past - historyBeforeMove.past;
  const resizeHistoryDelta = final.history.past - afterMove.history.past;
  const geometryEvidence = rectangleTransformEvidence(aliasedCommand, {
    before,
    moved: afterMove.markup,
    resized: final.markup,
    history_delta: { move: moveHistoryDelta, resize: resizeHistoryDelta },
    geometry_tolerance_pt: geometryTolerancePt,
  });
  const expectedMoveSamples =
    (command.move.duration_ms * command.move.rate_hz) / 1000 + 1;
  const expectedResizeSamples =
    (command.resize.duration_ms * command.resize.rate_hz) / 1000 + 1;
  const nativeEvidence = assessElectronNativeTransformEvidence({
    move_sample_count_matches:
      moveReplay.pixel_samples.length === expectedMoveSamples,
    resize_sample_count_matches:
      resizeReplay.pixel_samples.length === expectedResizeSamples,
    move_timing_within_tolerance: moveTiming.within_tolerance,
    resize_timing_within_tolerance: resizeTiming.within_tolerance,
    hit_test_selected:
      afterMove.selected &&
      final.selected &&
      geometryEvidence.hit_test_selected,
    move_history_delta: moveHistoryDelta,
    resize_history_delta: resizeHistoryDelta,
    geometry_exact: geometryEvidence.passed,
    presentation_current: final.presentation_current,
  });
  const result = {
    command_id: command.id,
    observed_markup_id: observedId,
    requested_annotation_id: command.annotation_id,
    benchmark_alias: creation.benchmark_alias_map[0],
    input: {
      move: {
        input_lane: nativeX11InputLane,
        injection_api: "XTEST-direct-helper",
        sample_count: moveReplay.pixel_samples.length,
        actual_duration_ms: moveDurationMs,
        timing: moveTiming,
      },
      resize: {
        input_lane: nativeX11InputLane,
        injection_api: "XTEST-direct-helper",
        sample_count: resizeReplay.pixel_samples.length,
        actual_duration_ms: resizeDurationMs,
        timing: resizeTiming,
      },
    },
    evidence: geometryEvidence,
    native_evidence: nativeEvidence,
    observed_milestones: [],
    milestones: [],
    exact_fields: {
      alias:
        creation.benchmark_alias_map[0]?.canonical_id === command.annotation_id,
      select_point_within_device_pixel: selectPointEvidence.passed,
      ...nativeEvidence.exact_fields,
    },
  };
  addMilestone(
    event,
    result,
    "hit-test-selected",
    geometryEvidence.hit_test_selected && afterMove.selected && final.selected,
    {
      observed_markup_id: observedId,
      select_point: command.select_point,
      select_semantics: "no-fill-edge-or-stroked-body",
      select_point_evidence: selectPointEvidence,
    },
  );
  addMilestone(
    event,
    result,
    "move-committed-once",
    geometryEvidence.move_committed_once,
    geometryEvidence,
  );
  addMilestone(
    event,
    result,
    "resize-committed-once",
    geometryEvidence.resize_committed_once,
    geometryEvidence,
  );
  finalizeCommandMilestones(result, command);
  const exact =
    result.manifest_milestones_complete &&
    Object.values(result.exact_fields).every(Boolean);
  event("comparison-command-exact-state", {
    command_id: command.id,
    passed: exact,
    exact_fields: result.exact_fields,
  });
  event("comparison-native-transform-evidence", {
    command_id: command.id,
    input_lane: nativeX11InputLane,
    select_semantics: "no-fill-edge-or-stroked-body",
    hit_test_selected: afterMove.selected && final.selected,
    move_history_delta: moveHistoryDelta,
    resize_history_delta: resizeHistoryDelta,
    geometry_tolerance_pt: geometryTolerancePt,
    presentation_current: final.presentation_current,
    passed: exact,
  });
  return {
    contract,
    input_lane: nativeX11InputLane,
    target,
    prerequisite_creation: creation,
    command_results: [result],
    exact_manifest_replay: exact,
    decision_timing_eligible: exact,
    blocker: exact
      ? null
      : "Native Electron rectangle transform did not satisfy exact sample, timing, history, geometry, selection, and presentation checks.",
  };
}

async function runElectronAnnotationPropertiesHistory(cdp, contract, event) {
  const workload = await loadComparisonWorkload();
  const transformCommand = workload.journeys
    .flatMap(({ commands }) => commands)
    .find(({ id }) => id === "rectangle:select-move-resize");
  if (!transformCommand)
    throw new Error(
      "rectangle properties prerequisite transform command is missing",
    );
  const transform = await runElectronAnnotationTransform(
    cdp,
    {
      ...contract,
      command_ids: [transformCommand.id],
      commands: [transformCommand],
    },
    event,
  );
  const transformPrerequisite =
    rectanglePropertiesTransformPrerequisiteEvidence(transform);
  if (!transformPrerequisite.passed) {
    throw new Error(
      "rectangle properties prerequisite did not pass exact transform gates",
    );
  }

  const command = contract.commands[0];
  const observedId = transformPrerequisite.observed_markup_id;
  const aliasedCommand = { ...command, annotation_id: observedId };
  const propertiesOpen = await cdp.evaluate(
    `document.querySelector('[data-testid="properties-sidebar-trigger"]')?.getAttribute('aria-expanded') === 'true'`,
  );
  if (!propertiesOpen)
    await dispatchCdpClick(cdp, "properties-sidebar-trigger");
  await waitForEditorCondition(
    cdp,
    `document.querySelector('[data-testid="right-sidebar-heading"]')?.textContent?.trim() === 'Rectangle'`,
    "selected Rectangle properties",
  );

  const uiPlan = buildElectronRectanglePropertiesUiPlan(command);
  await applyElectronRectanglePropertiesUiPlan(cdp, uiPlan);
  await dispatchCdpLabeledControlClick(cdp, "Locked", "switch");
  await waitForEditorCondition(
    cdp,
    `window.__butterPaperTestHooks.getActiveDocument()?.markups.find(
      (markup) => markup.id === ${JSON.stringify(observedId)}
    )?.locked === true`,
    "locked rectangle state",
  );
  const lockedBeforeAttempt = await activeMarkup(cdp, observedId);
  const historyBeforeLockedAttempt = await cdp.evaluate(
    `window.__butterPaperTestHooks.getDocumentHistory()`,
  );
  const center = {
    x: lockedBeforeAttempt.rect.x + lockedBeforeAttempt.rect.width / 2,
    y: lockedBeforeAttempt.rect.y + lockedBeforeAttempt.rect.height / 2,
  };
  const lockedInput = await dispatchAnnotationPointerStream(
    cdp,
    [
      { ...center, t_ms: 0 },
      { x: center.x + 12, y: center.y, t_ms: 50 },
      { x: center.x + 12, y: center.y, t_ms: 100 },
    ],
    event,
    command.id,
  );
  const lockedAfterAttempt = await activeMarkup(cdp, observedId);
  const historyAfterLockedAttempt = await cdp.evaluate(
    `window.__butterPaperTestHooks.getDocumentHistory()`,
  );
  const lockedEditRejected =
    lockedAfterAttempt?.locked === true &&
    JSON.stringify(lockedAfterAttempt) ===
      JSON.stringify(lockedBeforeAttempt) &&
    sameDocumentHistoryPosition(
      historyBeforeLockedAttempt,
      historyAfterLockedAttempt,
    );

  await dispatchCdpLabeledControlClick(cdp, "Locked", "switch");
  await waitForEditorCondition(
    cdp,
    `window.__butterPaperTestHooks.getActiveDocument()?.markups.find(
      (markup) => markup.id === ${JSON.stringify(observedId)}
    )?.locked !== true`,
    "unlocked rectangle state",
  );
  const beforeHistoryReplay = await activeMarkup(cdp, observedId);
  const undo = await replayDocumentHistoryToBoundary(cdp, false);
  const redo = await replayDocumentHistoryToBoundary(cdp, true);
  const afterHistoryReplay = await activeMarkup(cdp, observedId);
  const diagnostics = await cdp.evaluate(
    `window.__butterPaperTestHooks.getDiagnostics()`,
  );
  const dirty = diagnostics.tabs.find(({ active }) => active)?.dirty === true;
  const evidence = rectanglePropertiesHistoryEvidence(aliasedCommand, {
    current: afterHistoryReplay,
    locked_edit_rejected: lockedEditRejected,
    before_history_replay: beforeHistoryReplay,
    after_history_replay: afterHistoryReplay,
    dirty,
  });
  const fullHistoryReplayed =
    undo.boundary_reached &&
    redo.boundary_reached &&
    undo.changed > 0 &&
    redo.changed === undo.changed;
  const result = {
    command_id: command.id,
    requested_annotation_id: command.annotation_id,
    observed_markup_id: observedId,
    benchmark_alias: transform.command_results[0].benchmark_alias,
    input: { properties: uiPlan, locked_transform: lockedInput },
    history: {
      undo,
      redo,
      before_replay_markup: beforeHistoryReplay,
      after_replay_markup: afterHistoryReplay,
    },
    evidence,
    observed_milestones: [],
    milestones: [],
    exact_fields: {
      alias:
        transform.command_results[0].benchmark_alias?.canonical_id ===
        command.annotation_id,
      properties_current: evidence.properties_current,
      locked_edit_rejected: evidence.locked_edit_rejected,
      full_history_replayed: fullHistoryReplayed && evidence.undo_redo_exact,
      dirty_current: evidence.dirty_current,
    },
  };
  addMilestone(
    event,
    result,
    "properties-current",
    evidence.properties_current,
    {
      ui_plan: uiPlan,
      appearance: afterHistoryReplay?.appearance,
    },
  );
  addMilestone(
    event,
    result,
    "locked-edit-rejected",
    evidence.locked_edit_rejected,
    {
      locked_input: lockedInput,
      before: lockedBeforeAttempt,
      after: lockedAfterAttempt,
      history_before: historyBeforeLockedAttempt,
      history_after: historyAfterLockedAttempt,
    },
  );
  addMilestone(
    event,
    result,
    "undo-redo-exact",
    fullHistoryReplayed && evidence.undo_redo_exact,
    { undo, redo, before: beforeHistoryReplay, after: afterHistoryReplay },
  );
  addMilestone(event, result, "dirty-current", evidence.dirty_current, {
    diagnostics,
  });
  finalizeCommandMilestones(result, command);
  const exact =
    result.manifest_milestones_complete &&
    Object.values(result.exact_fields).every(Boolean);
  event("comparison-command-exact-state", {
    command_id: command.id,
    passed: exact,
    exact_fields: result.exact_fields,
  });
  return {
    contract,
    input_lane: cdpDiagnosticInputLane,
    prerequisite_transform: transform,
    command_results: [result],
    exact_manifest_replay: exact,
    blocker: exact
      ? null
      : "Electron rectangle properties/history UI replay was not exact.",
  };
}

async function selectElectronThumbnailPage(cdp, pageNumber) {
  const leftSidebarOpen = await cdp.evaluate(
    `Boolean(document.querySelector('[data-testid="left-sidebar"]'))`,
  );
  if (!leftSidebarOpen) await dispatchCdpClick(cdp, "left-rail-pages");
  await cdp.evaluate(`(async () => {
    const pageNumber = ${pageNumber};
    const deadline = performance.now() + 5000;
    while (performance.now() < deadline) {
      const button = document.querySelector('[data-testid="page-thumbnail-select-' + pageNumber + '"]');
      if (button) return;
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const list = document.querySelector('[data-testid="page-thumbnail-list"]');
      if (!list) throw new Error('Thumbnail list is unavailable');
      const fraction = (pageNumber - 1) / Math.max(1, diagnostics.pageCount - 1);
      list.scrollTop = fraction * Math.max(0, list.scrollHeight - list.clientHeight);
      list.dispatchEvent(new Event('scroll', { bubbles: true }));
      await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    throw new Error('Thumbnail page ${pageNumber} did not enter the virtual window');
  })()`);
  await dispatchCdpClick(cdp, `page-thumbnail-select-${pageNumber}`);
  return waitForEditorCondition(
    cdp,
    `(() => {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const page = document.querySelector('[data-testid="page-${pageNumber}"]');
      const bounds = page?.getBoundingClientRect();
      const visible = bounds && bounds.width > 0 && bounds.height > 0
        && bounds.right > 0 && bounds.bottom > 0
        && bounds.left < window.innerWidth && bounds.top < window.innerHeight;
      return diagnostics.currentPage === ${pageNumber - 1} && diagnostics.pageRenderReady && visible
        ? diagnostics
        : null;
    })()`,
    `Electron page ${pageNumber} navigation`,
    20_000,
  );
}

async function captureElectronCurrentPageImageIdentity(cdp, pageIndex) {
  return cdp.evaluate(`(async () => {
    const pageIndex = ${pageIndex};
    const deadline = performance.now() + 5000;
    const receipt = async (surface) => {
      let canvas;
      if (surface instanceof HTMLCanvasElement) {
        canvas = surface;
      } else if (surface instanceof HTMLImageElement) {
        if (!surface.complete || surface.naturalWidth === 0) {
          await surface.decode().catch(() => undefined);
          return null;
        }
        canvas = document.createElement('canvas');
        canvas.width = surface.naturalWidth;
        canvas.height = surface.naturalHeight;
        canvas.getContext('2d', { alpha: false })?.drawImage(surface, 0, 0);
      }
      if (!canvas || canvas.width <= 0 || canvas.height <= 0) return null;
      const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'));
      if (!blob) throw new Error('Current page raster could not be serialized');
      const digest = await crypto.subtle.digest('SHA-256', await blob.arrayBuffer());
      const sha256 = [...new Uint8Array(digest)]
        .map((byte) => byte.toString(16).padStart(2, '0')).join('');
      return {
        tag_name: surface.tagName.toLowerCase(),
        width: canvas.width,
        height: canvas.height,
        sha256,
        identity: [pageIndex, canvas.width, canvas.height, sha256].join(':'),
      };
    };
    while (performance.now() < deadline) {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const page = document.querySelector('[data-testid="page-' + (pageIndex + 1) + '"]');
      const surface = page?.querySelector('[data-render-state="ready"]');
      const quality = surface?.getAttribute('data-render-quality');
      if (diagnostics.currentPage === pageIndex && surface
        && quality !== 'preview' && quality !== 'stale-preview') {
        const first = await receipt(surface);
        await new Promise((resolve) => setTimeout(resolve, 250));
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        const settledDiagnostics = window.__butterPaperTestHooks.getDiagnostics();
        const settledPage = document.querySelector('[data-testid="page-' + (pageIndex + 1) + '"]');
        const settledSurface = settledPage?.querySelector('[data-render-state="ready"]');
        const settledQuality = settledSurface?.getAttribute('data-render-quality');
        const second = settledSurface && settledQuality !== 'preview' && settledQuality !== 'stale-preview'
          ? await receipt(settledSurface)
          : null;
        if (first && second && first.identity === second.identity
          && settledDiagnostics.currentPage === pageIndex) {
          return {
            page_index: pageIndex,
            current_page: settledPage.getAttribute('data-current-page') === 'true',
            quality: settledQuality,
            ...second,
            stability_observation_ms: 250,
            queue_context: {
              queued_page_renders: settledDiagnostics.queuedPageRenders,
              inflight_page_renders: settledDiagnostics.inflightPageRenders,
            },
          };
        }
      }
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    throw new Error('Current page ${pageIndex + 1} did not settle to a hashable full-quality image');
  })()`);
}

async function runElectronDenseRectangleRepeat(cdp, contract, workload, event) {
  const denseCommand = contract.commands.find(
    ({ id }) => id === "rectangle:repeat-dense",
  );
  if (!denseCommand)
    throw new Error(
      "editor-workload contract is missing rectangle:repeat-dense",
    );
  const sourceContract = buildElectronDenseRectangleSourceContract(
    workload,
    denseCommand,
  );
  const sourceCommands = new Map(
    sourceContract.commands.map((command) => [command.id, command]),
  );
  const propertiesCommand = sourceCommands.get("rectangle:properties-history");
  if (!propertiesCommand)
    throw new Error("dense Rectangle properties source command is missing");

  await cdp.evaluate(`window.__butterPaperTestHooks.setSnapSettings({
    snapToContent: false,
    snapToMarkup: false,
    snapToPageGrid: false,
  })`);
  await selectElectronThumbnailPage(cdp, 2);
  const densityCommands = JSON.parse(
    await readFile(
      resolve(
        performanceDirectory,
        "results/public-fixtures-v1/bp-annotation-density-v1.commands.json",
      ),
      "utf8",
    ),
  );
  const seededMarkups = buildElectronDenseFixtureMarkups(densityCommands, 1);
  if (seededMarkups.length !== 100) {
    throw new Error(
      `frozen density command stream must project exactly 100 page-2 rectangles; observed ${seededMarkups.length}`,
    );
  }
  await cdp.evaluate(`window.__butterPaperTestHooks.replaceDocumentMarkups(
    ${JSON.stringify(seededMarkups)},
    window.__butterPaperTestHooks.getActiveDocument()?.pageScales ?? [],
    [],
    false
  )`);
  const initial = await cdp.evaluate(`(() => {
    const documentModel = window.__butterPaperTestHooks.getActiveDocument();
    return {
      rectangle_count: documentModel.markups.filter(
        (markup) => markup.pageIndex === 1 && markup.kind === 'rectangle'
      ).length,
      history: window.__butterPaperTestHooks.getDocumentHistory(),
    };
  })()`);
  if (initial.rectangle_count !== 100) {
    throw new Error(
      `dense Rectangle page must start with exactly 100 rectangles; observed ${initial.rectangle_count}`,
    );
  }

  const spatialQueryBefore = await cdp.evaluate(
    `window.__butterPaperTestHooks.queryMarkupSpatialIndex(1, { x: 117, y: 240 }, 4)`,
  );
  const seededGeometryMatched = electronDenseGeometryIndexMatchesSeed(
    spatialQueryBefore,
    seededMarkups,
  );
  if (!seededGeometryMatched) {
    throw new Error(
      `dense markup spatial-index generation does not match the frozen 100-rectangle seed: ${JSON.stringify(
        {
          page_index: spatialQueryBefore.pageIndex,
          total_markup_count: spatialQueryBefore.totalMarkupCount,
          indexed_markup_count: spatialQueryBefore.indexedMarkupCount,
          generation_count: spatialQueryBefore.generation?.length,
        },
      )}`,
    );
  }
  await cdp.evaluate(`window.__butterPaperTestHooks.resetPerfSnapshot()`);
  const paintBefore = await cdp.evaluate(
    `window.__butterPaperTestHooks.getPerfSnapshot()`,
  );

  const propertiesReplay = await runElectronAnnotationPropertiesHistory(
    cdp,
    {
      ...contract,
      command_ids: [propertiesCommand.id],
      commands: [propertiesCommand],
      require_exact_fields: true,
    },
    event,
  );
  const transformReplay = propertiesReplay.prerequisite_transform;
  const createReplay = transformReplay.prerequisite_creation;
  const sourceResults = [
    createReplay.command_results[0],
    transformReplay.command_results[0],
    propertiesReplay.command_results[0],
  ];
  const observedMarkupId =
    propertiesReplay.command_results[0].observed_markup_id;
  const finalPresentation = await waitForEditorCondition(
    cdp,
    `(async () => {
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const documentModel = window.__butterPaperTestHooks.getActiveDocument();
      const pageRectangles = documentModel.markups.filter(
        (markup) => markup.pageIndex === 1 && markup.kind === 'rectangle'
      );
      const overlay = document.querySelector('[data-testid="page-2"]');
      const thumbnail = document.querySelector('[data-testid="thumbnail-annotation-layer-2"]');
      const overlayRectangles = [...(overlay?.querySelectorAll('[data-testid^="markup-"]') ?? [])];
      const thumbnailRectangles = [...(thumbnail?.querySelectorAll('[data-testid^="thumbnail-markup-"]') ?? [])];
      const targetOverlay = overlay?.querySelector(${JSON.stringify(`[data-testid="markup-${observedMarkupId}"]`)}) ?? null;
      const targetThumbnail = thumbnail?.querySelector(${JSON.stringify(`[data-testid="thumbnail-markup-${observedMarkupId}"]`)}) ?? null;
      if (pageRectangles.length !== 101 || overlayRectangles.length !== 101
        || thumbnailRectangles.length !== 101 || !targetOverlay || !targetThumbnail) return null;
      return {
        page_rectangles: pageRectangles,
        final_markup: pageRectangles.find((markup) => markup.id === ${JSON.stringify(observedMarkupId)}) ?? null,
        final_rectangle_count: pageRectangles.length,
        overlay: { rectangle_count: overlayRectangles.length, target_present: true },
        thumbnail: { rectangle_count: thumbnailRectangles.length, target_present: true },
        history: window.__butterPaperTestHooks.getDocumentHistory(),
        perf: window.__butterPaperTestHooks.getPerfSnapshot(),
      };
    })()`,
    "current dense Rectangle overlay and thumbnail",
    20_000,
  );
  const spatialQueryAfter = await cdp.evaluate(
    `window.__butterPaperTestHooks.queryMarkupSpatialIndex(1, { x: 195, y: 180 }, 4)`,
  );
  const subsequentQueryReceiptMatched =
    electronDenseGeometryIndexMatchesSeed(
      spatialQueryAfter,
      finalPresentation.page_rectangles,
    ) &&
    spatialQueryAfter.hitMarkupId === observedMarkupId &&
    spatialQueryAfter.candidateMarkupIds.length > 0 &&
    spatialQueryAfter.candidateMarkupIds.length <
      spatialQueryAfter.indexedMarkupCount;
  const propertiesResult = propertiesReplay.command_results[0];
  const evidence = rectangleDenseRepeatEvidence(
    {
      dense: denseCommand,
      create: sourceCommands.get("rectangle:create-sparse"),
      transform: sourceCommands.get("rectangle:select-move-resize"),
      properties: propertiesCommand,
    },
    {
      page_index: 1,
      geometry_tolerance_pt:
        transformReplay.command_results[0].evidence.geometry_tolerance_pt,
      initial_rectangle_count: initial.rectangle_count,
      final_rectangle_count: finalPresentation.final_rectangle_count,
      final_markup: finalPresentation.final_markup,
      source_results: sourceResults,
      spatial_index: {
        initial_indexed_count: spatialQueryBefore.indexedMarkupCount,
        final_indexed_count: spatialQueryAfter.indexedMarkupCount,
        candidate_count: spatialQueryAfter.candidateMarkupIds.length,
        queried_cell_count: spatialQueryAfter.queriedCellCount,
        query_hit_target: spatialQueryAfter.hitMarkupId === observedMarkupId,
        seeded_geometry_matched: seededGeometryMatched,
        subsequent_query_receipt_matched: subsequentQueryReceiptMatched,
      },
      annotation_paint: {
        before_editable_layer_renders:
          paintBefore.detailedComponentRenders?.["AnnotationLayer:1"] ?? 0,
        after_editable_layer_renders:
          finalPresentation.perf.detailedComponentRenders?.[
            "AnnotationLayer:1"
          ] ?? 0,
        before_thumbnail_layer_renders:
          paintBefore.detailedComponentRenders?.["ReadOnlyAnnotationLayer:1"] ??
          0,
        after_thumbnail_layer_renders:
          finalPresentation.perf.detailedComponentRenders?.[
            "ReadOnlyAnnotationLayer:1"
          ] ?? 0,
      },
      history: {
        before: initial.history,
        after: finalPresentation.history,
        undo: propertiesResult.history.undo,
        redo: propertiesResult.history.redo,
        before_replay_markup: propertiesResult.history.before_replay_markup,
        after_replay_markup: propertiesResult.history.after_replay_markup,
      },
      overlay: finalPresentation.overlay,
      thumbnail: finalPresentation.thumbnail,
    },
  );
  const result = {
    command_id: denseCommand.id,
    observed_markup_id: observedMarkupId,
    source_command_ids: sourceContract.command_ids,
    source_results: sourceResults,
    evidence,
    observed_milestones: [],
    milestones: [],
    exact_fields: evidence.exact_fields,
    decision_timing_eligible: false,
  };
  addMilestone(
    event,
    result,
    "spatial-index-work-recorded",
    evidence.exact_fields.spatial_index_work,
    {
      seeded_geometry_matched: seededGeometryMatched,
      subsequent_query_receipt_matched: subsequentQueryReceiptMatched,
      spatial_query_before: spatialQueryBefore,
      spatial_query_after: spatialQueryAfter,
    },
  );
  addMilestone(
    event,
    result,
    "annotation-paint-work-recorded",
    evidence.exact_fields.annotation_paint_work,
    {
      overlay: finalPresentation.overlay,
      thumbnail: finalPresentation.thumbnail,
      component_renders: finalPresentation.perf.detailedComponentRenders,
    },
  );
  addMilestone(
    event,
    result,
    "canonical-state-matched",
    evidence.exact_fields.canonical_state,
    {
      final_markup: finalPresentation.final_markup,
      exact_history: evidence.exact_fields.exact_history,
      overlay_current: evidence.exact_fields.overlay_current,
      thumbnail_current: evidence.exact_fields.thumbnail_current,
    },
  );
  finalizeCommandMilestones(result, denseCommand);
  const exact = result.manifest_milestones_complete && evidence.passed;
  event("comparison-command-exact-state", {
    command_id: denseCommand.id,
    passed: exact,
    exact_fields: result.exact_fields,
  });
  await selectElectronThumbnailPage(cdp, 1);
  return {
    contract: {
      ...contract,
      command_ids: [denseCommand.id],
      commands: [denseCommand],
    },
    input_lane: cdpDiagnosticInputLane,
    command_results: [result],
    exact_manifest_replay: exact,
    decision_timing_eligible: false,
    blocker: exact
      ? null
      : "Electron dense Rectangle replay did not satisfy exact source, work, state, history, overlay, and thumbnail checks.",
  };
}

async function dispatchCdpTextClick(cdp, text, rootSelector = "body") {
  const point = await cdp.evaluate(`(() => {
    const roots = [...document.querySelectorAll(${JSON.stringify(rootSelector)})];
    const element = roots.flatMap((root) => [...root.querySelectorAll('button,[role="option"],[data-slot="select-item"],[data-slot="toggle-group-item"]')])
      .find((candidate) => {
        const bounds = candidate.getBoundingClientRect();
        return candidate.textContent?.trim() === ${JSON.stringify(text)} && bounds.width > 0 && bounds.height > 0;
      });
    if (!element) throw new Error(${JSON.stringify(`Missing visible control ${text}`)});
    const bounds = element.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) throw new Error(${JSON.stringify(`Control ${text} is not visible`)});
    return { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2 };
  })()`);
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", ...point });
  await cdp.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    ...point,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await cdp.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    ...point,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
}

async function setCdpInputValue(cdp, selector, value) {
  await cdp.evaluate(`(() => {
    const input = document.querySelector(${JSON.stringify(selector)});
    if (!(input instanceof HTMLInputElement || input instanceof HTMLTextAreaElement)) {
      throw new Error(${JSON.stringify(`Missing input ${selector}`)});
    }
    const prototype = input instanceof HTMLTextAreaElement
      ? HTMLTextAreaElement.prototype
      : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(prototype, 'value').set.call(input, ${JSON.stringify(value)});
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
    input.dispatchEvent(new FocusEvent('blur', { bubbles: true }));
  })()`);
}

async function waitForEditorCondition(
  cdp,
  expression,
  description,
  timeoutMs = 5_000,
) {
  return cdp.evaluate(`(async () => {
    const deadline = performance.now() + ${timeoutMs};
    while (performance.now() < deadline) {
      const value = (${expression});
      if (value) return value;
      await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    throw new Error(${JSON.stringify(`Timed out waiting for ${description}`)});
  })()`);
}

async function observeEditorMarkup(cdp, markupId) {
  return cdp.evaluate(`(async () => {
    const root = document.querySelector(${JSON.stringify(`[data-testid="markup-${markupId}"]`)});
    if (!root) return { element_present: false, visible: false };
    const animationFramesAfterMarkup = 2;
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const bounds = root.getBoundingClientRect();
    const text = root.querySelector('text');
    const image = root.querySelector('image');
    let decoded = null;
    if (image) {
      const href = image.getAttribute('href') || image.getAttributeNS('http://www.w3.org/1999/xlink', 'href');
      decoded = await new Promise((resolve) => {
        const candidate = new Image();
        candidate.onload = () => {
          const canvas = document.createElement('canvas');
          canvas.width = candidate.naturalWidth;
          canvas.height = candidate.naturalHeight;
          const context = canvas.getContext('2d', { willReadFrequently: true });
          context?.drawImage(candidate, 0, 0);
          const rgbaBytes = context?.getImageData(0, 0, canvas.width, canvas.height).data.byteLength ?? 0;
          resolve({
            width: candidate.naturalWidth,
            height: candidate.naturalHeight,
            rgba_bytes: rgbaBytes,
          });
        };
        candidate.onerror = () => resolve({ width: 0, height: 0, rgba_bytes: 0 });
        candidate.src = href;
      });
    }
    let textBox = null;
    let computedTextLength = 0;
    if (text) {
      const box = text.getBBox();
      textBox = { x: box.x, y: box.y, width: box.width, height: box.height };
      computedTextLength = text.getComputedTextLength();
    }
    return {
      element_present: true,
      visible: bounds.width > 0 && bounds.height > 0,
      bbox: textBox,
      text_content: text?.textContent ?? null,
      computed_text_length: computedTextLength,
      image_resource_element_present: image !== null,
      decoded_width: decoded?.width ?? null,
      decoded_height: decoded?.height ?? null,
      decoded_rgba_payload_bytes: decoded?.rgba_bytes ?? null,
      animation_frames_after_markup: animationFramesAfterMarkup,
      physical_bus_upload_bytes: null,
    };
  })()`);
}

export async function runElectronImageCreate(
  cdp,
  contract,
  event,
  nativeTarget = null,
) {
  const imageCommand = contract.commands.find(
    ({ id }) => id === "image:create",
  );
  if (!imageCommand)
    throw new Error("image-create contract is missing image:create");
  const native = nativeTarget !== null;
  const clickTestId = (testId) =>
    native
      ? nativeClickTestId(cdp, nativeTarget, testId)
      : dispatchCdpClick(cdp, testId);
  const clickPdfPoint = (point) =>
    native
      ? nativePdfPointClick(cdp, nativeTarget, point)
      : dispatchPdfPointClick(cdp, point);
  const evidenceScope = native
    ? "electron-live-native-XTEST-input-CDP-observation"
    : "electron-live-cdp-observation";

  await clickTestId("viewer-fit-page");
  await waitForEditorCondition(
    cdp,
    "window.__butterPaperTestHooks.getDiagnostics().zoomPreset === 'fit-page' && window.__butterPaperTestHooks.getDiagnostics().pageRenderReady",
    "Fit Page before independent Image replay",
  );
  if (native) await disableSnappingWithNativeUi(cdp, nativeTarget);
  else {
    await cdp.evaluate(`window.__butterPaperTestHooks.setSnapSettings({
      snapToContent: false,
      snapToMarkup: false,
      snapToPageGrid: false,
      dimensionIncrementEnabled: false,
      snapGuidesEnabled: false,
    })`);
  }

  const checkerPath = resolve(
    performanceDirectory,
    "results/public-fixtures-v1/bp-image-checker-v1.png",
  );
  const checkerBytes = await readFile(checkerPath);
  const imageTraceEnabled = process.env.BP_ELECTRON_IMAGE_TRACE === "1";
  let imageTrace = null;
  let traceStarted = false;
  try {
    if (imageTraceEnabled) {
      await startElectronImageTrace(cdp);
      traceStarted = true;
    }
    const before = await cdp.evaluate(
      `window.__butterPaperTestHooks.getActiveDocument()`,
    );
    if (native) {
      await clickTestId("tool-image");
      await delay(250);
      for (const commandArgs of buildElectronNativeFileDialogCommands(
        checkerPath,
      )) {
        await runXdotool(commandArgs, 0);
      }
      await waitForEditorCondition(
        cdp,
        `document.querySelector('[data-testid="tool-image"]')?.getAttribute('aria-pressed') === 'true'`,
        "native Image tool activation after chooser selection",
        10_000,
      );
    } else {
      await cdp.send("Page.enable");
      await cdp.send("Page.setInterceptFileChooserDialog", { enabled: true });
      const chooser = cdp.waitForEvent("Page.fileChooserOpened");
      await clickTestId("tool-image");
      const chooserEvent = await chooser;
      await cdp.send("DOM.setFileInputFiles", {
        files: [checkerPath],
        backendNodeId: chooserEvent.backendNodeId,
      });
      await cdp.send("Page.setInterceptFileChooserDialog", { enabled: false });
    }
    const focusRestoration = native
      ? await restoreElectronNativeWindowFocus(cdp, nativeTarget, event)
      : null;
    const placementSurface = await annotationSurface(cdp);
    const placementCssPoint = native
      ? pdfPointToNativeCss(imageCommand.placement.point, placementSurface)
      : pdfPointToCss(imageCommand.placement.point, placementSurface);
    const capturePlacementContext = () =>
      cdp.evaluate(`(() => {
        const point = ${JSON.stringify(placementCssPoint)};
        const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
        return {
          document_has_focus: document.hasFocus(),
          active_element: {
            tag: document.activeElement?.tagName ?? null,
            test_id: document.activeElement?.getAttribute?.('data-testid') ?? null,
            role: document.activeElement?.getAttribute?.('role') ?? null,
          },
          active_tool: diagnostics.activeTool,
          image_tool_aria_pressed: document.querySelector('[data-testid="tool-image"]')?.getAttribute('aria-pressed') ?? null,
          point,
          hit_stack: document.elementsFromPoint(point.x, point.y).slice(0, 8).map((element) => ({
            tag: element.tagName,
            test_id: element.getAttribute('data-testid'),
            slot: element.getAttribute('data-slot'),
            role: element.getAttribute('role'),
            pointer_events: getComputedStyle(element).pointerEvents,
          })),
        };
      })()`);
    const placementBefore = await capturePlacementContext();
    const activeX11WindowBefore = native
      ? await optionalCommand("xdotool", ["getactivewindow"])
      : null;
    const imageInput = await clickPdfPoint(imageCommand.placement.point);
    let documentModel;
    try {
      documentModel = await waitForMarkupCount(cdp, before.markups.length + 1);
    } catch (error) {
      const placementAfter = await capturePlacementContext();
      const activeX11WindowAfter = native
        ? await optionalCommand("xdotool", ["getactivewindow"])
        : null;
      throw new Error(
        `${error instanceof Error ? error.message : String(error)}; image placement context ${JSON.stringify(
          {
            target_x11_window: nativeTarget?.window_id ?? null,
            focus_restoration: focusRestoration,
            active_x11_window_before: activeX11WindowBefore,
            active_x11_window_after: activeX11WindowAfter,
            placement_before: placementBefore,
            placement_after: placementAfter,
            input_receipt: imageInput,
          },
        )}`,
      );
    }
    const markup = documentModel.markups.find(
      ({ id }) => !before.markups.some((existing) => existing.id === id),
    );
    if (!markup)
      throw new Error("image:create did not create one identifiable markup");
    const presentation = await observeEditorMarkup(cdp, markup.id);
    if (traceStarted) {
      imageTrace = summarizeElectronImageTrace(
        await stopElectronImageTrace(cdp),
        {
          source_width: electronImageCheckerAsset.source_width_px,
          source_height: electronImageCheckerAsset.source_height_px,
        },
      );
      traceStarted = false;
    }
    const alias = {
      command_id: imageCommand.id,
      canonical_id: imageCommand.annotation_id,
      observed_id: markup.id,
    };
    const observed = {
      input: imageInput,
      page_size: documentModel.pages[0]?.size,
      markup,
      history_delta: documentModel.markups.length - before.markups.length,
      presentation: { ...presentation, legacy_chromium_trace: imageTrace },
      placement_context: {
        focus_restoration: focusRestoration,
        active_x11_window_before: activeX11WindowBefore,
        placement_before: placementBefore,
      },
      native_input_completed: native,
      benchmark_alias: alias,
      bounds_contract: classifyEditorCreateBoundsContract(
        imageCommand,
        markup,
        documentModel.pages[0]?.size,
        imageInput,
      ),
    };
    observed.renderer_resource_submission_receipt =
      buildElectronImageRendererResourceSubmissionReceipt(imageCommand, {
        ...observed,
        checker_asset_bytes: checkerBytes,
        legacy_chromium_trace: imageTrace,
      });
    const assessment = assessElectronEditorCreateEvidence(
      imageCommand,
      observed,
    );
    const result = {
      command_id: imageCommand.id,
      ...observed,
      ...assessment,
      observed_milestones: [],
      milestones: [],
    };
    for (const milestone of imageCommand.expected_milestones) {
      const passed = assessment.milestones[milestone] === true;
      addMilestone(
        event,
        result,
        milestone,
        passed,
        {
          evidence_scope: evidenceScope,
          receipt_kind:
            observed.renderer_resource_submission_receipt.receipt_kind,
        },
        passed
          ? null
          : milestone === "bitmap-upload-recorded"
            ? "The app-level renderer-resource submission receipt was incomplete; physical GPU bus attribution is intentionally not claimed."
            : "exact live observation was absent or different",
      );
    }
    finalizeCommandMilestones(result, imageCommand);
    const blockedManifestMilestones = result.milestones
      .filter(({ status }) => status !== "passed")
      .map(({ milestone, blocker }) => ({ milestone, reason: blocker }));
    const commandExact =
      blockedManifestMilestones.length === 0 &&
      Object.values(assessment.exact_fields).every(Boolean);
    event("comparison-command-evidence", {
      command_id: imageCommand.id,
      evidence_scope: evidenceScope,
      all_manifest_milestones_proven: blockedManifestMilestones.length === 0,
      decision_timing_eligible: native && commandExact,
      evidence: {
        command_id: imageCommand.id,
        proven_manifest_milestones: result.observed_milestones,
        blocked_manifest_milestones: blockedManifestMilestones,
        facts: observed,
      },
    });
    event("comparison-command-exact-state", {
      command_id: imageCommand.id,
      passed: commandExact,
      exact_fields: assessment.exact_fields,
    });
    return {
      contract,
      input_lane: native ? nativeX11InputLane : cdpDiagnosticInputLane,
      command_results: [result],
      benchmark_alias_map: createBenchmarkAliasMap([alias]).entries,
      exact_manifest_replay: commandExact,
      decision_timing_eligible: native ? commandExact : false,
      blocker: commandExact
        ? null
        : "Electron image-create replay exposed exact state or presentation blockers.",
      manifest_v3_review: [],
    };
  } catch (error) {
    if (traceStarted) {
      try {
        await stopElectronImageTrace(cdp);
      } catch {
        // Preserve the primary image-create failure; tracing is optional diagnostic evidence.
      }
    }
    throw error;
  }
}

async function runElectronEditorCreate(
  cdp,
  contract,
  event,
  nativeTarget = null,
) {
  const native = nativeTarget !== null;
  const clickTestId = (testId) =>
    native
      ? nativeClickTestId(cdp, nativeTarget, testId)
      : dispatchCdpClick(cdp, testId);
  const clickSelector = (selector, description) =>
    native
      ? nativeClickSelector(cdp, nativeTarget, selector, description)
      : dispatchCdpSelectorClick(cdp, selector, description);
  const clickVisibleText = (text, rootSelector) =>
    native
      ? nativeClickVisibleText(cdp, nativeTarget, text, rootSelector)
      : dispatchCdpTextClick(cdp, text, rootSelector);
  const replaceInput = (selector, value) =>
    native
      ? nativeReplaceInput(cdp, nativeTarget, selector, value)
      : setCdpInputValue(cdp, selector, value);
  const clickPdfPoint = (point) =>
    native
      ? nativePdfPointClick(cdp, nativeTarget, point)
      : dispatchPdfPointClick(cdp, point);
  const evidenceScope = native
    ? "electron-live-native-XTEST-input-CDP-observation"
    : "electron-live-cdp-observation";

  await clickTestId("viewer-fit-page");
  await waitForEditorCondition(
    cdp,
    "window.__butterPaperTestHooks.getDiagnostics().zoomPreset === 'fit-page' && window.__butterPaperTestHooks.getDiagnostics().pageRenderReady",
    "Fit Page before editor replay",
  );
  if (native) await disableSnappingWithNativeUi(cdp, nativeTarget);
  else {
    await cdp.evaluate(`window.__butterPaperTestHooks.setSnapSettings({
      snapToContent: false,
      snapToMarkup: false,
      snapToPageGrid: false,
      dimensionIncrementEnabled: false,
      snapGuidesEnabled: false,
    })`);
  }
  const results = [];
  const aliases = [];
  const command = (id) =>
    contract.commands.find((candidate) => candidate.id === id);

  const finishResult = (manifestCommand, observed) => {
    const assessment = assessElectronEditorCreateEvidence(
      manifestCommand,
      observed,
    );
    const result = {
      command_id: manifestCommand.id,
      ...observed,
      ...assessment,
      observed_milestones: [],
      milestones: [],
    };
    for (const milestone of manifestCommand.expected_milestones) {
      const passed = assessment.milestones[milestone] === true;
      addMilestone(
        event,
        result,
        milestone,
        assessment.milestones[milestone] === true,
        {
          evidence_scope: evidenceScope,
        },
        passed
          ? null
          : (observed.milestone_blockers?.[milestone] ??
              "exact live observation was absent or different"),
      );
    }
    finalizeCommandMilestones(result, manifestCommand);
    const blockedManifestMilestones = result.milestones
      .filter(({ status }) => status !== "passed")
      .map(({ milestone, blocker }) => ({ milestone, reason: blocker }));
    const commandExact =
      blockedManifestMilestones.length === 0 &&
      Object.values(assessment.exact_fields).every(Boolean);
    event("comparison-command-evidence", {
      command_id: manifestCommand.id,
      evidence_scope: evidenceScope,
      all_manifest_milestones_proven: blockedManifestMilestones.length === 0,
      decision_timing_eligible: native && commandExact,
      evidence: {
        command_id: manifestCommand.id,
        proven_manifest_milestones: result.observed_milestones,
        blocked_manifest_milestones: blockedManifestMilestones,
        facts: observed,
      },
    });
    event("comparison-command-exact-state", {
      command_id: manifestCommand.id,
      passed: commandExact,
      exact_fields: assessment.exact_fields,
    });
    results.push(result);
  };

  const textCommand = command("text:create");
  let before = await cdp.evaluate(
    `window.__butterPaperTestHooks.getActiveDocument()`,
  );
  await clickTestId("tool-text-box");
  const propertiesOpen = await cdp.evaluate(
    `document.querySelector('[data-testid=properties-sidebar-trigger]')?.getAttribute('aria-expanded') === 'true'`,
  );
  if (!propertiesOpen) await clickTestId("properties-sidebar-trigger");
  await waitForEditorCondition(
    cdp,
    "Boolean(document.querySelector('[data-testid=tool-default-properties]'))",
    "Text tool properties",
  );
  await clickSelector('[aria-label="Font size"]', "Font size control");
  await clickVisibleText(`${textCommand.font.size_pt} pt`);
  await clickSelector('[aria-label^="Color:"]', "Text color control");
  await clickVisibleText("Add", '[data-slot="popover-content"]');
  await replaceInput(
    '[aria-label="Custom preset hex color"]',
    textCommand.font.color.slice(0, 7),
  );
  await clickVisibleText("Add", '[data-slot="popover-content"]');
  const textInput = await clickPdfPoint(textCommand.placement.point);
  await waitForEditorCondition(
    cdp,
    "Boolean(document.querySelector('[data-testid^=text-box-editor-] textarea'))",
    "Text editor",
  );
  await clickSelector(
    '[data-testid^="text-box-editor-"] textarea',
    "Text editor",
  );
  if (native) {
    await nativeTypeText(nativeTarget, textCommand.text);
    await nativeSendKey(nativeTarget, "Escape");
  } else {
    await cdp.send("Input.insertText", { text: textCommand.text });
    await cdp.send("Input.dispatchKeyEvent", {
      type: "keyDown",
      key: "Escape",
      code: "Escape",
    });
    await cdp.send("Input.dispatchKeyEvent", {
      type: "keyUp",
      key: "Escape",
      code: "Escape",
    });
  }
  let documentModel = await waitForMarkupCount(cdp, before.markups.length + 1);
  let markup = documentModel.markups.find(
    ({ id }) => !before.markups.some((existing) => existing.id === id),
  );
  if (!markup)
    throw new Error("text:create did not create one identifiable markup");
  aliases.push({
    command_id: textCommand.id,
    canonical_id: textCommand.annotation_id,
    observed_id: markup.id,
  });
  const pageSize = documentModel.pages[0]?.size;
  if (!pageSize) throw new Error("editor-create page size is unavailable");
  finishResult(textCommand, {
    input: textInput,
    page_size: pageSize,
    markup,
    history_delta: documentModel.markups.length - before.markups.length,
    presentation: await observeEditorMarkup(cdp, markup.id),
    benchmark_alias: aliases.at(-1),
    bounds_contract: classifyEditorCreateBoundsContract(
      textCommand,
      markup,
      pageSize,
    ),
  });

  const scaleCommand = command("length:set-scale");
  await clickTestId("measure-set-page-scale");
  await waitForEditorCondition(
    cdp,
    "Boolean(document.querySelector('[data-testid=page-scale-dialog]'))",
    "Page Scale dialog",
  );
  await clickVisibleText("Calibrate", '[aria-label="Method"]');
  for (const selection of editorCreatePrecisionUiSelection(scaleCommand)) {
    let selected = false;
    for (let attempt = 0; attempt < 3 && !selected; attempt += 1) {
      await clickTestId(selection.test_id);
      await waitForEditorCondition(
        cdp,
        `(() => {
          const option = (${findVisibleElectronSelectOption.toString()})(
            document,
            ${JSON.stringify(selection.visible_label)},
          );
          return option
            ? {
                role: option.getAttribute('role'),
                slot: option.getAttribute('data-slot'),
                aria_selected: option.getAttribute('aria-selected'),
                data_selected: option.hasAttribute('data-selected'),
              }
            : null;
        })()`,
        `${selection.visible_label} precision option`,
      );
      await clickVisibleText(
        selection.visible_label,
        '[data-slot="select-content"]',
      );
      await cdp.evaluate(
        `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
      );
      selected = await cdp.evaluate(
        `(${electronSelectTriggerRetainsValue.toString()})(
          document.querySelector('[data-testid="${selection.test_id}"]'),
          ${JSON.stringify(selection.visible_label)},
        )`,
      );
    }
    if (!selected)
      throw new Error(
        `precision control did not retain ${selection.visible_label}`,
      );
  }
  await clickTestId("page-scale-apply");
  const observedScale = await waitForEditorCondition(
    cdp,
    `(() => {
    const scale = window.__butterPaperTestHooks.getActiveDocument()?.pageScales?.find(({ pageIndex }) => pageIndex === 0);
    if (!scale) return null;
    return {
      paper_points: scale.realUnits === 'm' ? 1 / scale.scaleX : null,
      real_world_value: 1,
      unit: scale.realUnits,
      precision: scale.precision?.mode === 'decimal' ? Math.round(-Math.log10(scale.precision.value)) : null,
      raw: scale,
    };
  })()`,
    "exact page scale",
  );
  finishResult(scaleCommand, { scale: observedScale });

  const lengthCommand = command("length:create");
  for (const testId of ["left-rail-pages", "properties-sidebar-trigger"]) {
    const expanded = await cdp.evaluate(
      `document.querySelector('[data-testid=${testId}]')?.getAttribute('aria-expanded') === 'true'`,
    );
    if (expanded) await clickTestId(testId);
  }
  await waitForEditorCondition(
    cdp,
    `!document.querySelector('[data-testid="left-sidebar"]')
      && document.querySelector('[data-testid="properties-sidebar-trigger"]')?.getAttribute('aria-expanded') === 'false'`,
    "closed editor sidebars before 400% Length replay",
  );
  const viewportSize = await cdp.evaluate(`(() => {
    const viewport = document.querySelector('[data-testid="document-viewport"]');
    if (!viewport) throw new Error('Document viewport is unavailable');
    const bounds = viewport.getBoundingClientRect();
    return { width: bounds.width, height: bounds.height };
  })()`);
  const zoomPlan = buildEditorCreateHighZoomPlan(lengthCommand, {
    zoom_percent: 200,
    viewport_width_px: viewportSize.width,
    viewport_height_px: viewportSize.height,
  });
  if (!zoomPlan.endpoints_fit_viewport) {
    throw new Error(
      `Length endpoints do not fit a maintained high-zoom preset: ${JSON.stringify(zoomPlan)}`,
    );
  }
  await clickTestId("viewer-zoom-menu");
  await waitForEditorCondition(
    cdp,
    `Boolean(document.querySelector('[data-testid="${zoomPlan.zoom_preset_test_id}"]'))`,
    `${zoomPlan.zoom_percent}% zoom preset`,
  );
  await clickTestId(zoomPlan.zoom_preset_test_id);
  await waitForEditorCondition(
    cdp,
    `window.__butterPaperTestHooks.getDiagnostics().zoom === ${zoomPlan.zoom_percent / 100}
      && window.__butterPaperTestHooks.getDiagnostics().pageRenderReady`,
    `${zoomPlan.zoom_percent}% editor-create Length viewport`,
    20_000,
  );
  if (zoomPlan.close_menu_with_escape) {
    if (native) await nativeSendKey(nativeTarget, "Escape");
    else {
      await cdp.send("Input.dispatchKeyEvent", {
        type: "keyDown",
        key: "Escape",
        code: "Escape",
      });
      await cdp.send("Input.dispatchKeyEvent", {
        type: "keyUp",
        key: "Escape",
        code: "Escape",
      });
    }
    await waitForEditorCondition(
      cdp,
      `document.querySelector('[data-testid="viewer-zoom-menu"]')?.getAttribute('aria-expanded') === 'false'`,
      "closed zoom menu before editor-create Length replay",
    );
  }
  const lengthPan = native
    ? await centerEditorPdfPointWithNativePan(
        cdp,
        nativeTarget,
        zoomPlan.focus_pdf_point,
      )
    : await centerEditorPdfPointAtCurrentZoom(cdp, zoomPlan.focus_pdf_point);
  const horizontalReach =
    Math.abs(lengthPan.after_delta_px.x) +
    zoomPlan.endpoint_distance_px / 2 +
    zoomPlan.viewport_margin_px;
  const endpointsVisible =
    horizontalReach <= lengthPan.viewport_size_px.width / 2;
  lengthPan.endpoints_visible_with_margin = endpointsVisible;
  if (!endpointsVisible || Math.abs(lengthPan.after_delta_px.y) > 1) {
    throw new Error(
      `Length endpoints could not be kept visible: ${JSON.stringify(lengthPan)}`,
    );
  }
  const endpointMapping = await observeEditorEndpointMapping(
    cdp,
    [lengthCommand.start, lengthCommand.finish],
    zoomPlan.viewport_margin_px,
  );
  if (!endpointMapping.all_inside_viewport) {
    throw new Error(
      `Length endpoint mapping is outside the annotation viewport: ${JSON.stringify(endpointMapping)}`,
    );
  }
  before = await cdp.evaluate(
    `window.__butterPaperTestHooks.getActiveDocument()`,
  );
  const lengthInteraction = buildEditorCreateLengthInteractionPlan();
  if (!native) {
    await cdp.evaluate(`(async () => {
      const tool = document.querySelector('[data-testid="${lengthInteraction.active_tool_test_id}"]');
      if (!tool) throw new Error('Length tool is unavailable');
      tool.scrollIntoView(${JSON.stringify(lengthInteraction.scroll_into_view)});
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    })()`);
  }
  await clickTestId(lengthInteraction.active_tool_test_id);
  const lengthToolActivation = await cdp.evaluate(`(async () => {
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const tool = document.querySelector('[data-testid="${lengthInteraction.active_tool_test_id}"]');
    const railViewport = document.querySelector('[data-testid="right-rail-viewport"]');
    if (!(tool instanceof HTMLElement)) return { element_present: false };
    const bounds = tool.getBoundingClientRect();
    const hit = document.elementFromPoint(bounds.left + bounds.width / 2, bounds.top + bounds.height / 2);
    const railBounds = railViewport?.getBoundingClientRect();
    return {
      element_present: true,
      disabled: tool.matches(':disabled'),
      aria_pressed: tool.getAttribute('aria-pressed'),
      data_state: tool.getAttribute('data-state'),
      bounds: { left: bounds.left, top: bounds.top, right: bounds.right, bottom: bounds.bottom },
      hit_test_id: hit?.closest('[data-testid]')?.getAttribute('data-testid') ?? null,
      hit_tag: hit?.tagName ?? null,
      active_test_id: document.activeElement?.closest?.('[data-testid]')?.getAttribute('data-testid') ?? null,
      active_tag: document.activeElement?.tagName ?? null,
      rail_viewport_bounds: railBounds ? {
        left: railBounds.left,
        top: railBounds.top,
        right: railBounds.right,
        bottom: railBounds.bottom,
      } : null,
      rail_scroll_top: railViewport instanceof HTMLElement ? railViewport.scrollTop : null,
    };
  })()`);
  if (
    lengthToolActivation.aria_pressed !==
    lengthInteraction.active_tool_aria_pressed
  ) {
    throw new Error(
      `Length tool did not activate: ${JSON.stringify(lengthToolActivation)}`,
    );
  }
  const firstLengthInput = await clickPdfPoint(lengthCommand.start);
  await cdp.evaluate(`new Promise((resolve) => {
    let remaining = ${lengthInteraction.inter_endpoint_animation_frames};
    const advance = () => {
      remaining -= 1;
      if (remaining === 0) resolve();
      else requestAnimationFrame(advance);
    };
    requestAnimationFrame(advance);
  })`);
  const lengthInput = [
    firstLengthInput,
    await clickPdfPoint(lengthCommand.finish),
  ];
  documentModel = await waitForMarkupCount(cdp, before.markups.length + 1);
  markup = documentModel.markups.find(
    ({ id }) => !before.markups.some((existing) => existing.id === id),
  );
  if (!markup)
    throw new Error("length:create did not create one identifiable markup");
  aliases.push({
    command_id: lengthCommand.id,
    canonical_id: lengthCommand.annotation_id,
    observed_id: markup.id,
  });
  finishResult(lengthCommand, {
    input: lengthInput,
    viewport_plan: zoomPlan,
    viewport_pan: lengthPan,
    endpoint_mapping: endpointMapping,
    markup,
    history_delta: documentModel.markups.length - before.markups.length,
    presentation: await observeEditorMarkup(cdp, markup.id),
    benchmark_alias: aliases.at(-1),
  });

  await clickTestId("viewer-fit-page");
  await waitForEditorCondition(
    cdp,
    "window.__butterPaperTestHooks.getDiagnostics().zoomPreset === 'fit-page' && window.__butterPaperTestHooks.getDiagnostics().pageRenderReady",
    "Fit Page after editor-create Length replay",
  );

  const imageCommand = command("image:create");
  const imageCreate = await runElectronImageCreate(
    cdp,
    {
      ...contract,
      command_ids: [imageCommand.id],
      commands: [imageCommand],
    },
    event,
    nativeTarget,
  );
  results.push(...imageCreate.command_results);
  aliases.push(...imageCreate.benchmark_alias_map);

  const exact = results.every(
    ({ exact_fields, manifest_milestones_complete }) =>
      manifest_milestones_complete &&
      Object.values(exact_fields).every(Boolean),
  );
  return {
    contract,
    input_lane: native ? nativeX11InputLane : "cdp-input-diagnostic",
    command_results: results,
    benchmark_alias_map: createBenchmarkAliasMap(aliases).entries,
    exact_manifest_replay: exact,
    decision_timing_eligible: native ? exact : false,
    blocker: exact
      ? null
      : "Electron editor-create replay exposed exact state or presentation blockers.",
    manifest_v3_review: [],
  };
}

function electronScenarioSubcontract(contract, commandIds) {
  const selected = new Set(commandIds);
  const commands = contract.commands.filter(({ id }) => selected.has(id));
  if (commands.length !== commandIds.length) {
    const found = new Set(commands.map(({ id }) => id));
    throw new Error(
      `editor-workload contract is missing ${commandIds.filter((id) => !found.has(id)).join(", ")}`,
    );
  }
  return { ...contract, command_ids: [...commandIds], commands };
}

async function captureElectronUntimedState(cdp, observedMarkupId) {
  return cdp.evaluate(`(() => {
    const hooks = window.__butterPaperTestHooks;
    if (!hooks) throw new Error('Butter Paper test hooks are unavailable');
    const documentModel = hooks.getActiveDocument();
    return {
      markup: documentModel?.markups.find(({ id }) => id === ${JSON.stringify(observedMarkupId)}) ?? null,
      history: hooks.getDocumentHistory(),
      selected_markup_ids: [...hooks.getDiagnostics().selectedMarkupIds],
    };
  })()`);
}

async function invokeElectronUntimedAction(cdp, action) {
  await cdp.evaluate(`(async () => {
    const hooks = window.__butterPaperTestHooks;
    const method = hooks?.[${JSON.stringify(action.hook)}];
    if (typeof method !== 'function') throw new Error(${JSON.stringify(`Missing test hook ${action.hook}`)});
    await method(...${JSON.stringify(action.args)});
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  })()`);
}

async function observeThumbnailMarkup(cdp, observedMarkupId) {
  const leftSidebarOpen = await cdp.evaluate(
    `window.__butterPaperTestHooks.getDiagnostics().leftSidebarOpen === true`,
  );
  if (!leftSidebarOpen) await dispatchCdpClick(cdp, "left-rail-pages");
  return cdp.evaluate(`(async () => {
    const selector = ${JSON.stringify(`[data-testid="thumbnail-markup-${observedMarkupId}"]`)};
    const deadline = performance.now() + 5000;
    while (performance.now() < deadline) {
      if (document.querySelector(selector)) return true;
      await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    return false;
  })()`);
}

async function runElectronUntimedEditHistory(cdp, workload, aliases, event) {
  const executableIds = [
    "highlight:edit-history",
    "text:edit-resize-history",
    "length:edit-endpoint-history",
    "image:resize-history",
  ];
  const plan = buildElectronUntimedCorrectnessPlan(workload);
  const plans = new Map(
    plan.commands.map((candidate) => [candidate.command_id, candidate]),
  );
  const commands = new Map(
    workload.journeys
      .flatMap(({ commands: journeyCommands }) => journeyCommands)
      .map((command) => [command.id, command]),
  );
  const observedByCanonical = new Map(
    aliases.map(({ canonical_id: canonicalId, observed_id: observedId }) => [
      canonicalId,
      observedId,
    ]),
  );
  const canonicalByObserved = new Map(
    aliases.map(({ canonical_id: canonicalId, observed_id: observedId }) => [
      observedId,
      canonicalId,
    ]),
  );
  const observations = {};

  for (const commandId of executableIds) {
    const command = commands.get(commandId);
    const commandPlan = plans.get(commandId);
    const observedMarkupId = observedByCanonical.get(command?.annotation_id);
    if (
      !command ||
      !commandPlan ||
      commandPlan.executable === false ||
      !observedMarkupId
    ) {
      throw new Error(
        `${commandId} has no executable plan or live prerequisite alias`,
      );
    }
    const before = await captureElectronUntimedState(cdp, observedMarkupId);
    if (!before.markup)
      throw new Error(`${commandId} live prerequisite markup is absent`);
    const observation = {
      selected_markup_ids: before.selected_markup_ids.map(
        (id) => canonicalByObserved.get(id) ?? id,
      ),
      before_markup: before.markup,
      history: { before: before.history },
      action_evidence: [],
    };
    for (const plannedAction of commandPlan.actions) {
      const action = remapElectronUntimedAction(plannedAction, aliases);
      const prior = await captureElectronUntimedState(cdp, observedMarkupId);
      await invokeElectronUntimedAction(cdp, action);
      const captured = await captureElectronUntimedState(cdp, observedMarkupId);
      observation.selected_markup_ids = captured.selected_markup_ids.map(
        (id) => canonicalByObserved.get(id) ?? id,
      );
      observation.action_evidence.push({
        hook: action.hook,
        capture: action.capture ?? null,
        markup: captured.markup,
        history: captured.history,
      });
      if (action.capture === "selection")
        observation.selection_hit_markup_id =
          canonicalByObserved.get(observedMarkupId);
      if (action.capture === "pre_last_commit")
        observation.pre_last_commit_markup = captured.markup;
      if (action.capture === "committed") {
        observation.pre_last_commit_markup = prior.markup;
        observation.committed_markup = captured.markup;
        observation.history.after_commit = captured.history;
      }
      if (action.capture === "after_undo") {
        observation.after_undo_markup = captured.markup;
        observation.history.after_undo = captured.history;
      }
      if (action.capture === "after_redo") {
        observation.after_redo_markup = captured.markup;
        observation.history.after_redo = captured.history;
      }
    }
    const presentation = await observeEditorMarkup(cdp, observedMarkupId);
    observation.presentation = presentation;
    if (command.expected_milestones.includes("thumbnail-current")) {
      observation.thumbnail_current = await observeThumbnailMarkup(
        cdp,
        observedMarkupId,
      );
    }
    if (commandId === "length:edit-endpoint-history") {
      observation.derived_label = presentation.text_content;
    }
    if (commandId === "image:resize-history") {
      const checkerBytes = await readFile(
        resolve(
          performanceDirectory,
          "results/public-fixtures-v1/bp-image-checker-v1.png",
        ),
      );
      observation.image_decode = {
        decoded_width: presentation.decoded_width,
        decoded_height: presentation.decoded_height,
        element_present: presentation.element_present,
        visible: presentation.visible,
      };
      observation.renderer_resource_submission_receipt =
        buildElectronImageResizeRendererResourceSubmissionReceipt(command, {
          ...observation,
          checker_asset_bytes: checkerBytes,
        });
      observation.upload_byte_count_recorded =
        observation.renderer_resource_submission_receipt.passed;
    }
    observations[commandId] = observation;
  }

  const report = assessElectronUntimedCorrectness(workload, observations);
  const reportById = new Map(
    report.commands.map((result) => [result.command_id, result]),
  );
  const results = executableIds.map((commandId) => {
    const command = commands.get(commandId);
    const assessment = reportById.get(commandId);
    const reasons = new Map([
      ...assessment.failed.map(({ milestone, reason }) => [milestone, reason]),
      ...assessment.blocked.map(({ milestone, reason }) => [milestone, reason]),
    ]);
    const result = {
      command_id: commandId,
      observation: observations[commandId],
      ...(observations[commandId].renderer_resource_submission_receipt
        ? {
            renderer_resource_submission_receipt:
              observations[commandId].renderer_resource_submission_receipt,
          }
        : {}),
      assessment,
      observed_milestones: [],
      milestones: [],
      exact_fields: Object.fromEntries(
        command.expected_milestones.map((milestone) => [
          milestone,
          assessment.passed.includes(milestone),
        ]),
      ),
    };
    for (const milestone of command.expected_milestones) {
      addMilestone(
        event,
        result,
        milestone,
        assessment.passed.includes(milestone),
        {
          evidence_scope: "maintained-electron-test-hooks-and-live-dom",
        },
        reasons.get(milestone) ?? null,
      );
    }
    finalizeCommandMilestones(result, command);
    event("comparison-command-exact-state", {
      command_id: commandId,
      passed: assessment.status === "passed",
      exact_fields: result.exact_fields,
    });
    return result;
  });
  return { plan, observations, report, command_results: results };
}

async function runElectronEditorWorkload(cdp, contract, workload, event) {
  const denseRectangle = await runElectronDenseRectangleRepeat(
    cdp,
    contract,
    workload,
    event,
  );
  const annotationCreate = await runElectronAnnotationCreate(
    cdp,
    electronScenarioSubcontract(contract, [
      "rectangle:create-sparse",
      "highlight:create",
    ]),
    event,
  );
  const editorCreate = await runElectronEditorCreate(
    cdp,
    electronScenarioSubcontract(contract, [
      "text:create",
      "length:set-scale",
      "length:create",
      "image:create",
    ]),
    event,
  );
  const aliases = [
    ...annotationCreate.benchmark_alias_map,
    ...editorCreate.benchmark_alias_map,
  ];
  const editHistory = await runElectronUntimedEditHistory(
    cdp,
    workload,
    aliases,
    event,
  );
  const completedIds = new Set(
    [
      ...denseRectangle.command_results,
      ...annotationCreate.command_results,
      ...editorCreate.command_results,
      ...editHistory.command_results,
    ].map(({ command_id: commandId }) => commandId),
  );
  const blockedResults = contract.commands
    .filter(({ id }) => !completedIds.has(id))
    .map((command) => {
      const reason =
        command.id === "rectangle:repeat-dense"
          ? "maintained live replay on the dense page is not integrated"
          : "this editor-workload command is not integrated into the combined live replay";
      const result = {
        command_id: command.id,
        observed_milestones: [],
        milestones: [],
        exact_fields: Object.fromEntries(
          command.expected_milestones.map((milestone) => [milestone, false]),
        ),
        blocked_reason: reason,
      };
      for (const milestone of command.expected_milestones) {
        addMilestone(
          event,
          result,
          milestone,
          false,
          {
            evidence_scope: "electron-editor-workload-not-integrated",
          },
          reason,
        );
      }
      finalizeCommandMilestones(result, command);
      return result;
    });
  const commandResults = [
    ...denseRectangle.command_results,
    ...annotationCreate.command_results,
    ...editorCreate.command_results,
    ...editHistory.command_results,
    ...blockedResults,
  ];
  const exactManifestReplay = commandResults.every(
    ({ manifest_milestones_complete: complete }) => complete,
  );
  return {
    contract,
    input_lane: cdpDiagnosticInputLane,
    command_results: commandResults,
    benchmark_alias_map: aliases,
    dense_rectangle: denseRectangle,
    untimed_correctness: editHistory.report,
    exact_manifest_replay: exactManifestReplay,
    decision_timing_eligible: false,
    blocker: exactManifestReplay
      ? null
      : "editor-workload retains explicit per-command semantic or renderer-resource blockers",
  };
}

function electronPersistenceSubcontract(contract) {
  const selected = new Set(electronPersistenceCommandIds);
  const commands = contract.commands.filter(({ id }) => selected.has(id));
  if (commands.length !== electronPersistenceCommandIds.length) {
    throw new Error(
      "persistence-workload contract is missing an Electron persistence command",
    );
  }
  return {
    ...contract,
    command_ids: [...electronPersistenceCommandIds],
    commands,
  };
}

function exactJson(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

async function captureElectronDocumentAndDiagnostics(cdp) {
  return cdp.evaluate(`({
    document: window.__butterPaperTestHooks.getActiveDocument(),
    diagnostics: window.__butterPaperTestHooks.getDiagnostics(),
  })`);
}

async function reopenElectronPersistencePath(cdp, path) {
  await invokeElectronUntimedAction(cdp, { hook: "closeTab", args: [0] });
  await waitForEditorCondition(
    cdp,
    "(window.__butterPaperTestHooks.getDiagnostics().tabs?.length ?? 0) === 0",
    "the saved persistence tab to close",
    10_000,
  );
  await invokeElectronUntimedAction(cdp, {
    hook: "openDocumentPath",
    args: [path],
  });
  return waitForEditorCondition(
    cdp,
    `(() => {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      return diagnostics.documentPath === ${JSON.stringify(path)} && diagnostics.pageRenderReady
        ? { document: window.__butterPaperTestHooks.getActiveDocument(), diagnostics }
        : null;
    })()`,
    `reopened persistence PDF ${path}`,
    20_000,
  );
}

async function runElectronPersistenceWorkload(
  cdp,
  contract,
  workload,
  event,
  sourcePath,
) {
  const evidenceParent = resolve(performanceDirectory, "results");
  await mkdir(evidenceParent, { recursive: true });
  const evidenceDirectory = await mkdtemp(
    resolve(evidenceParent, "electron-persistence-"),
  );
  const stagingDirectory = await mkdtemp(
    resolve(tmpdir(), "butter-paper-electron-persistence-"),
  );
  const cycle1Path = resolve(stagingDirectory, "cycle-1.pdf");
  const cycle2Path = resolve(stagingDirectory, "cycle-2.pdf");
  const retainedCycle1Path = resolve(evidenceDirectory, "cycle-1.pdf");
  const retainedCycle2Path = resolve(evidenceDirectory, "cycle-2.pdf");
  try {
    const checkerBytes = await readFile(
      resolve(
        performanceDirectory,
        "results/public-fixtures-v1/bp-image-checker-v1.png",
      ),
    );
    const checkerDataUrl = `data:image/png;base64,${checkerBytes.toString("base64")}`;
    const initial = await captureElectronDocumentAndDiagnostics(cdp);
    const canonical = buildElectronCanonicalPersistenceState(
      initial.document,
      workload,
      checkerDataUrl,
    );
    const sourceProbe = await captureIndependentPdfProbe(sourcePath);
    await invokeElectronUntimedAction(cdp, {
      hook: "replaceDocumentMarkups",
      args: [
        canonical.markups,
        canonical.page_scales,
        canonical.selected_markup_ids,
      ],
    });
    const applied = await captureElectronDocumentAndDiagnostics(cdp);
    const appliedProjection = projectElectronCanonicalPersistenceState(
      applied.document,
      applied.diagnostics.selectedMarkupIds,
    );
    const appliedDirty =
      applied.diagnostics.tabs?.find(({ active }) => active)?.dirty === true;

    await invokeElectronUntimedAction(cdp, {
      hook: "saveCurrentDocumentAs",
      args: [cycle1Path],
    });
    await copyFile(cycle1Path, retainedCycle1Path);
    const cycle1Artifact = await fileProvenance(retainedCycle1Path);
    const cycle1Probe = await captureIndependentPdfProbe(retainedCycle1Path);
    const reopened1 = await reopenElectronPersistencePath(cdp, cycle1Path);
    const reopened1Projection = projectElectronCanonicalPersistenceState(
      reopened1.document,
      canonical.selected_markup_ids,
    );

    await invokeElectronUntimedAction(cdp, {
      hook: "saveCurrentDocumentAs",
      args: [cycle2Path],
    });
    await copyFile(cycle2Path, retainedCycle2Path);
    const cycle2Artifact = await fileProvenance(retainedCycle2Path);
    const cycle2Probe = await captureIndependentPdfProbe(retainedCycle2Path);
    const reopened2 = await reopenElectronPersistencePath(cdp, cycle2Path);
    const reopened2Projection = projectElectronCanonicalPersistenceState(
      reopened2.document,
      canonical.selected_markup_ids,
    );
    const publishedDirty =
      reopened2.diagnostics.tabs?.find(({ active }) => active)?.dirty === true;

    const [sourceCrop, cycle1Crop, cycle2Crop] = await Promise.all([
      renderFixedPersistenceCrop(
        sourcePath,
        resolve(evidenceDirectory, "source-crop"),
      ),
      renderFixedPersistenceCrop(
        retainedCycle1Path,
        resolve(evidenceDirectory, "cycle-1-crop"),
      ),
      renderFixedPersistenceCrop(
        retainedCycle2Path,
        resolve(evidenceDirectory, "cycle-2-crop"),
      ),
    ]);
    const fixedCropsMatched =
      sourceCrop.sha256 !== cycle1Crop.sha256 &&
      cycle1Crop.sha256 === cycle2Crop.sha256;
    const expectedTypedProjection = {
      ...canonical.expected_projection,
      selected_annotation_ids: canonical.selected_markup_ids,
    };
    const cycle1TypedExact = exactJson(
      reopened1Projection,
      expectedTypedProjection,
    );
    const cycle2TypedExact = exactJson(
      reopened2Projection,
      expectedTypedProjection,
    );
    const cycle1UnknownExact = compareUnknownAnnotationProbe(
      sourceProbe.unknown,
      cycle1Probe.unknown,
    );
    const cycle2UnknownExact = compareUnknownAnnotationProbe(
      sourceProbe.unknown,
      cycle2Probe.unknown,
    );
    const cycle1DocumentExact = compareIndependentDocumentProbe(
      sourceProbe.document,
      cycle1Probe.document,
    );
    const cycle2DocumentExact = compareIndependentDocumentProbe(
      sourceProbe.document,
      cycle2Probe.document,
    );
    const observations = {
      "unknown:import": {
        dictionary_snapshotted: Boolean(sourceProbe.unknown?.dictionary_sha256),
        appearance_stream_snapshotted: Boolean(
          sourceProbe.unknown?.appearance_stream_sha256,
        ),
        source_probe: sourceProbe.unknown,
      },
      "unknown:assert-cycle-1": {
        dictionary_byte_exact: cycle1UnknownExact,
        appearance_stream_byte_exact: cycle1UnknownExact,
        cycle_probe: cycle1Probe.unknown,
      },
      "unknown:assert-cycle-2": {
        dictionary_byte_exact: cycle2UnknownExact,
        appearance_stream_byte_exact: cycle2UnknownExact,
        cycle_probe: cycle2Probe.unknown,
      },
      "persistence:apply-fixed-state": {
        canonical_state_matched: exactJson(
          appliedProjection,
          canonical.expected_projection,
        ),
        dirty_current: appliedDirty,
        canonical_projection: appliedProjection,
      },
      "persistence:save-1": {
        save_completed: cycle1Artifact.bytes > 0 && cycle1Path !== sourcePath,
        independent_pdf_validation_passed:
          cycle1Probe.qpdf.passed && cycle1Probe.pdfinfo.passed,
        native_annotations_valid: validateElectronPersistenceNativeAnnotations(
          cycle1Probe.native_annotations,
        ),
        appearance_streams_valid: validateElectronPersistenceAppearanceStreams(
          cycle1Probe.native_annotations,
        ),
        artifact: cycle1Artifact,
      },
      "persistence:reopen-1": {
        document_reopened: reopened1.diagnostics.documentPath === cycle1Path,
        canonical_state_matched: cycle1TypedExact,
        page_content_preserved: cycle1DocumentExact,
        page_metadata_preserved: cycle1DocumentExact,
        fixed_crops_matched: fixedCropsMatched,
        canonical_projection: reopened1Projection,
      },
      "persistence:save-2": {
        save_completed:
          cycle2Artifact.bytes > 0 &&
          cycle2Path !== sourcePath &&
          cycle2Path !== cycle1Path,
        independent_pdf_validation_passed:
          cycle2Probe.qpdf.passed && cycle2Probe.pdfinfo.passed,
        native_annotations_valid: validateElectronPersistenceNativeAnnotations(
          cycle2Probe.native_annotations,
        ),
        appearance_streams_valid: validateElectronPersistenceAppearanceStreams(
          cycle2Probe.native_annotations,
        ),
        artifact: cycle2Artifact,
      },
      "persistence:reopen-2": {
        document_reopened: reopened2.diagnostics.documentPath === cycle2Path,
        canonical_state_matched: cycle2TypedExact,
        page_content_preserved: cycle2DocumentExact,
        page_metadata_preserved: cycle2DocumentExact,
        fixed_crops_matched: fixedCropsMatched,
        dirty_published: publishedDirty === false,
        canonical_projection: reopened2Projection,
      },
    };
    const fullAssessment = assessElectronUntimedCorrectness(
      workload,
      observations,
    );
    const assessments = new Map(
      fullAssessment.commands.map((assessment) => [
        assessment.command_id,
        assessment,
      ]),
    );
    const commands = new Map(
      contract.commands.map((command) => [command.id, command]),
    );
    const commandResults = electronPersistenceCommandIds.map((commandId) => {
      const command = commands.get(commandId);
      const assessment = assessments.get(commandId);
      const reasons = new Map([
        ...assessment.failed.map(({ milestone, reason }) => [
          milestone,
          reason,
        ]),
        ...assessment.blocked.map(({ milestone, reason }) => [
          milestone,
          reason,
        ]),
      ]);
      const result = {
        command_id: commandId,
        observation: observations[commandId],
        assessment,
        observed_milestones: [],
        milestones: [],
        exact_fields: Object.fromEntries(
          command.expected_milestones.map((milestone) => [
            milestone,
            assessment.passed.includes(milestone),
          ]),
        ),
      };
      for (const milestone of command.expected_milestones) {
        addMilestone(
          event,
          result,
          milestone,
          assessment.passed.includes(milestone),
          {
            evidence_scope:
              "electron-production-pdf-save-plus-independent-qpdf-poppler",
          },
          reasons.get(milestone) ?? null,
        );
      }
      finalizeCommandMilestones(result, command);
      event("comparison-command-exact-state", {
        command_id: commandId,
        passed: assessment.status === "passed",
        exact_fields: result.exact_fields,
      });
      return result;
    });
    const laneCommands = commandResults.map(({ command_id: commandId }) =>
      assessments.get(commandId),
    );
    const exact = commandResults.every(
      ({ manifest_milestones_complete: complete }) => complete,
    );
    return {
      contract,
      input_lane: cdpDiagnosticInputLane,
      command_results: commandResults,
      exact_manifest_replay: exact,
      decision_timing_eligible: false,
      blocker: exact
        ? null
        : "Electron two-cycle persistence evidence was not exact",
      untimed_correctness: {
        schema_version: fullAssessment.schema_version,
        commands: laneCommands,
        summary: {
          passed: laneCommands
            .filter(({ status }) => status === "passed")
            .map(({ command_id: id }) => id),
          failed: laneCommands
            .filter(({ status }) => status === "failed")
            .map(({ command_id: id }) => id),
          blocked: laneCommands
            .filter(({ status }) => status === "blocked")
            .map(({ command_id: id }) => id),
        },
      },
      persistence_evidence: {
        evidence_directory: evidenceDirectory,
        source: sourceProbe,
        cycle_1: cycle1Probe,
        cycle_2: cycle2Probe,
        crops: { source: sourceCrop, cycle_1: cycle1Crop, cycle_2: cycle2Crop },
        fixed_crops_matched: fixedCropsMatched,
        artifacts_retained: true,
        artifact_disposition:
          "retained under ignored experiments/gpui-migration/performance/results",
      },
    };
  } finally {
    await rm(stagingDirectory, { recursive: true, force: true });
  }
}

async function configureElectronNativeConstructionGrid(cdp, target, spacingMm) {
  await disableSnappingWithNativeUi(cdp, target);
  await nativeClickTestId(cdp, target, "viewer-snap-target-menu");
  await waitForEditorCondition(
    cdp,
    `Boolean(document.querySelector('[data-testid="viewer-snap-popover"]'))`,
    "native snap settings popover",
  );
  const enabled = await cdp.evaluate(
    `document.querySelector('#snap-construction-grid')?.matches('[data-checked],[aria-checked="true"]') === true
      || document.querySelector('[data-testid="viewer-snap-popover"] [data-slot="switch"]')?.matches('[data-checked],[aria-checked="true"]') === true`,
  );
  if (!enabled) {
    await nativeClickSelector(
      cdp,
      target,
      '[data-testid="viewer-snap-popover"] [data-slot="switch"]',
      "Snap to grid switch",
    );
  }
  await waitForEditorCondition(
    cdp,
    `(document.querySelector('#snap-construction-grid')?.matches('[data-checked],[aria-checked="true"]') === true
      || document.querySelector('[data-testid="viewer-snap-popover"] [data-slot="switch"]')?.matches('[data-checked],[aria-checked="true"]') === true)
      && document.querySelector('#construction-grid-spacing')?.matches(':disabled') === false`,
    "enabled native construction grid input",
  );
  const spacing = await nativeReplaceInput(
    cdp,
    target,
    "#construction-grid-spacing",
    String(spacingMm),
  );
  await nativeClickTestId(cdp, target, "viewer-snap-target-menu");
  await waitForEditorCondition(
    cdp,
    `document.querySelector('[data-testid="viewer-snap-target-menu"]')?.getAttribute('aria-pressed') === 'false'`,
    "native snap settings close",
  );
  return { spacing, spacing_mm: spacingMm, enabled: true };
}

async function createElectronNativeV5Rectangle(cdp, target, rectangle) {
  const before = await cdp.evaluate(
    `window.__butterPaperTestHooks.getActiveDocument()`,
  );
  await nativeClickTestId(cdp, target, "tool-rectangle");
  const surface = await electronAnnotationSurface(cdp);
  const samples = inclusiveTimedPdfSamples(
    { x: rectangle.x1, y: rectangle.y1 },
    { x: rectangle.x2, y: rectangle.y2 },
    60,
    500,
  );
  const replay = buildPointerReplay({
    windowId: target.window_id,
    rateHz: 60,
    durationMs: 500,
    pdfSamples: samples,
    surface,
    windowGeometry: target.geometry,
  });
  const actualDurationMs = await runDirectXTestPointer(replay, target);
  const documentModel = await waitForMarkupCount(
    cdp,
    before.markups.length + 1,
  );
  const markup = documentModel.markups.find(
    ({ id }) => !before.markups.some((candidate) => candidate.id === id),
  );
  if (!markup) throw new Error("native v5 Rectangle was not created once");
  return {
    markup,
    surface,
    input: {
      input_lane: nativeX11InputLane,
      injection_api: "XTEST-direct-helper",
      sample_count: samples.length,
      actual_duration_ms: actualDurationMs,
      timing: assessReplayTiming(replay, actualDurationMs),
    },
  };
}

function markupStrokeWidth(markup) {
  return markup?.appearance?.stroke?.widthPt ?? null;
}

async function runElectronNativePropertyEditUndoV5(
  cdp,
  contract,
  event,
  target,
) {
  const command = contract.commands[0];
  await nativeClickTestId(cdp, target, "viewer-fit-page");
  await waitForEditorCondition(
    cdp,
    `window.__butterPaperTestHooks.getDiagnostics().zoomPreset === 'fit-page' && window.__butterPaperTestHooks.getDiagnostics().pageRenderReady`,
    "Fit Page before native v5 property replay",
  );
  const disabledSnapSources = await disableSnappingWithNativeUi(cdp, target);
  const creation = await createElectronNativeV5Rectangle(
    cdp,
    target,
    command.setup.rectangle,
  );
  const observedId = creation.markup.id;
  await nativeClickTestId(cdp, target, "properties-sidebar-trigger");
  await waitForEditorCondition(
    cdp,
    `document.querySelector('[data-testid="right-sidebar-heading"]')?.textContent?.trim() === 'Rectangle'`,
    "selected v5 Rectangle properties",
  );
  await nativeReplaceLabeledNumber(
    cdp,
    target,
    "Line Width",
    command.setup.stroke_width_points,
    "right-sidebar-heading",
  );
  await waitForEditorCondition(
    cdp,
    `window.__butterPaperTestHooks.getActiveDocument()?.markups.find(({ id }) => id === ${JSON.stringify(observedId)})?.appearance?.stroke?.widthPt === ${command.setup.stroke_width_points}`,
    "v5 property setup width",
  );
  const before = await cdp.evaluate(`(() => ({
    markup: window.__butterPaperTestHooks.getActiveDocument()?.markups.find(({ id }) => id === ${JSON.stringify(observedId)}) ?? null,
    history: window.__butterPaperTestHooks.getDocumentHistory(),
  }))()`);
  const input = await nativeReplaceLabeledNumber(
    cdp,
    target,
    "Line Width",
    command.property_edit.to,
    "right-sidebar-heading",
  );
  const committed = await cdp.evaluate(`(async () => {
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    return {
      markup: window.__butterPaperTestHooks.getActiveDocument()?.markups.find(({ id }) => id === ${JSON.stringify(observedId)}) ?? null,
      history: window.__butterPaperTestHooks.getDocumentHistory(),
      painted: Boolean(document.querySelector(${JSON.stringify(`[data-testid="markup-${observedId}"]`)})),
    };
  })()`);
  await nativeSendKey(target, "ctrl+z");
  await cdp.evaluate(
    `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
  );
  const undone = await cdp.evaluate(`(() => ({
    markup: window.__butterPaperTestHooks.getActiveDocument()?.markups.find(({ id }) => id === ${JSON.stringify(observedId)}) ?? null,
    history: window.__butterPaperTestHooks.getDocumentHistory(),
    thumbnail_current: Boolean(document.querySelector(${JSON.stringify(`[data-testid="thumbnail-markup-${observedId}"]`)})),
  }))()`);
  const historyRevisionDelta = committed.history.past - before.history.past;
  const undoCount = committed.history.past - undone.history.past;
  const observation = {
    trusted_native_input: true,
    annotation_id: observedId,
    before: markupStrokeWidth(before.markup),
    committed: markupStrokeWidth(committed.markup),
    after_undo: markupStrokeWidth(undone.markup),
    commit_count: historyRevisionDelta,
    undo_count: undoCount,
    effective_history_revision_delta: historyRevisionDelta,
    application_undo_count: undoCount,
    known_baseline_defect_id:
      historyRevisionDelta === 2 &&
      undoCount === 1 &&
      markupStrokeWidth(undone.markup) === command.property_edit.to
        ? (command.electron_baseline_policy?.allowed_known_defect_id ?? null)
        : null,
    canonical_state_restored:
      JSON.stringify(undone.markup) === JSON.stringify(before.markup),
    native_presentation_acknowledged: committed.painted === true,
    thumbnail_current: undone.thumbnail_current,
    status:
      historyRevisionDelta === 2 &&
      undoCount === 1 &&
      markupStrokeWidth(undone.markup) === command.property_edit.to
        ? "known-baseline-history-defect"
        : "unexpected-baseline",
  };
  const assessment = assessElectronNativePropertyEditUndoV5(
    command,
    observation,
  );
  const milestoneChecks = {
    "trusted-native-input-complete": observation.trusted_native_input,
    "property-commit-once": historyRevisionDelta === 1,
    "property-user-gesture-complete":
      observation.committed === command.property_edit.to,
    "property-state-current-before-undo":
      observation.committed === command.property_edit.to,
    "native-property-presentation-acknowledged":
      observation.native_presentation_acknowledged,
    "undo-applied-once": undoCount === 1,
    "application-undo-applied-once": undoCount === 1,
    "canonical-state-restored": observation.canonical_state_restored,
    "thumbnail-current": observation.thumbnail_current,
    "known-baseline-history-defect-recorded":
      observation.status === "known-baseline-history-defect",
    "implementation-history-outcome-recorded": assessment.passed,
  };
  const result = {
    command_id: command.id,
    observation,
    setup: { disabled_snap_sources: disabledSnapSources, creation },
    input,
    assessment,
    observed_milestones: [],
    milestones: [],
    exact_fields: Object.fromEntries(
      command.expected_milestones.map((milestone) => [
        milestone,
        milestoneChecks[milestone] === true,
      ]),
    ),
  };
  for (const milestone of command.expected_milestones) {
    addMilestone(
      event,
      result,
      milestone,
      milestoneChecks[milestone] === true,
      observation,
      milestoneChecks[milestone] === true
        ? null
        : observation.status === "known-baseline-history-defect"
          ? observation.status
          : "native property baseline did not match",
    );
  }
  finalizeCommandMilestones(result, command);
  const exact =
    result.manifest_milestones_complete &&
    Object.values(result.exact_fields).every(Boolean);
  event("comparison-command-exact-state", {
    command_id: command.id,
    passed: exact,
    exact_fields: result.exact_fields,
  });
  event("electron-v5-native-property-baseline", observation);
  return {
    contract,
    input_lane: nativeX11InputLane,
    command_results: [result],
    semantic_summary: assessment.summary,
    exact_manifest_replay: exact,
    decision_timing_eligible: false,
    blocker: exact ? null : observation.status,
  };
}

function rectangleCoordinates(markup) {
  const rect = markup?.rect;
  return rect
    ? {
        x1: rect.x,
        y1: rect.y,
        x2: rect.x + rect.width,
        y2: rect.y + rect.height,
      }
    : null;
}

function canonicalRectangleCoordinates(markup) {
  const coordinates = rectangleCoordinates(markup);
  return coordinates
    ? Object.fromEntries(
        Object.entries(coordinates).map(([key, value]) => [
          key,
          Math.round(value * 1e9) / 1e9,
        ]),
      )
    : null;
}

async function runElectronNativeSnapTransformV5(cdp, contract, event, target) {
  const command = contract.commands[0];
  const plan = buildElectronNativeSnapReplayPlan(command);
  await nativeClickTestId(cdp, target, "viewer-fit-page");
  await waitForEditorCondition(
    cdp,
    `window.__butterPaperTestHooks.getDiagnostics().zoomPreset === 'fit-page' && window.__butterPaperTestHooks.getDiagnostics().pageRenderReady`,
    "Fit Page before native v5 snap replay",
  );
  const grid = await configureElectronNativeConstructionGrid(
    cdp,
    target,
    plan.grid_spacing_mm,
  );
  const creation = await createElectronNativeV5Rectangle(
    cdp,
    target,
    command.setup.rectangle,
  );
  const observedId = creation.markup.id;
  await nativeClickTestId(cdp, target, "properties-sidebar-trigger");
  await waitForEditorCondition(
    cdp,
    `document.querySelector('[data-testid="right-sidebar-heading"]')?.textContent?.trim() === 'Rectangle'`,
    "selected v5 snap Rectangle properties",
  );
  const fillHitTarget = await nativeSetSelectedColor(
    cdp,
    target,
    "Fill Color",
    "#ffff00",
  );
  await waitForEditorCondition(
    cdp,
    `window.__butterPaperTestHooks.getActiveDocument()?.markups.find(({ id }) => id === ${JSON.stringify(observedId)})?.appearance?.fill?.color?.toLowerCase() === '#ffff00'`,
    "filled v5 snap Rectangle hit target",
  );
  await nativeClickTestId(cdp, target, "properties-sidebar-trigger");
  await waitForEditorCondition(
    cdp,
    `document.querySelector('[data-testid="properties-sidebar-trigger"]')?.getAttribute('aria-expanded') === 'false'`,
    "v5 snap properties sidebar close",
  );
  await nativeClickTestId(cdp, target, "tool-select");
  const surface = await electronAnnotationSurface(cdp);
  const diagnosticsBefore = await cdp.evaluate(
    `window.__butterPaperTestHooks.getDiagnostics()`,
  );
  const before = await cdp.evaluate(`(() => ({
    markup: window.__butterPaperTestHooks.getActiveDocument()?.markups.find(({ id }) => id === ${JSON.stringify(observedId)}) ?? null,
    history: window.__butterPaperTestHooks.getDocumentHistory(),
  }))()`);
  await cdp.evaluate(`(() => {
    window.__electronV5SnapProbeActive = true;
    window.__electronV5SnapProbe = {
      frame_count: 0,
      snap_target_acquired_count: 0,
      snap_guide_presented_count: 0,
    };
    const sample = () => {
      if (!window.__electronV5SnapProbeActive) return;
      const probe = window.__electronV5SnapProbe;
      probe.frame_count += 1;
      if (document.querySelector('[data-testid="snap-indicator"]')) {
        probe.snap_target_acquired_count += 1;
      }
      if (
        document.querySelector('[data-testid="snap-indicator"]') ||
        document.querySelector('[data-testid="drafting-guides"] [data-testid$="guide"]')
      ) {
        probe.snap_guide_presented_count += 1;
      }
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  })()`);
  const replay = buildPointerReplay({
    windowId: target.window_id,
    rateHz: plan.rate_hz,
    durationMs: plan.duration_ms,
    pdfSamples: plan.pdf_samples,
    surface,
    windowGeometry: target.geometry,
  });
  const actualDurationMs = await runDirectXTestPointer(replay, target);
  const timing = assessReplayTiming(replay, actualDurationMs);
  const committed = await cdp.evaluate(`(async () => {
    window.__electronV5SnapProbeActive = false;
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    return {
      markup: window.__butterPaperTestHooks.getActiveDocument()?.markups.find(({ id }) => id === ${JSON.stringify(observedId)}) ?? null,
      history: window.__butterPaperTestHooks.getDocumentHistory(),
      probe: window.__electronV5SnapProbe,
    };
  })()`);
  await nativeSendKey(target, "ctrl+z");
  await cdp.evaluate(
    `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
  );
  const undone = await activeMarkup(cdp, observedId);
  await nativeSendKey(target, "ctrl+shift+z");
  await cdp.evaluate(
    `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
  );
  const redone = await cdp.evaluate(`(() => ({
    markup: window.__butterPaperTestHooks.getActiveDocument()?.markups.find(({ id }) => id === ${JSON.stringify(observedId)}) ?? null,
    history: window.__butterPaperTestHooks.getDocumentHistory(),
    thumbnail_current: Boolean(document.querySelector(${JSON.stringify(`[data-testid="thumbnail-markup-${observedId}"]`)})),
  }))()`);
  const observation = {
    trusted_native_input: true,
    observed_sample_count: replay.pixel_samples.length,
    scheduled_duration_ms: plan.duration_ms,
    actual_duration_ms: actualDurationMs,
    timing,
    snap_enabled: grid.enabled,
    snap_target_acquired_count:
      committed.probe?.snap_target_acquired_count ?? 0,
    snap_guide_presented_count:
      committed.probe?.snap_guide_presented_count ?? 0,
    observed_final_rectangle: canonicalRectangleCoordinates(redone.markup),
    observed_raw_final_rectangle: rectangleCoordinates(redone.markup),
    gesture_commit_count: committed.history.past - before.history.past,
    undo_redo_exact:
      JSON.stringify(undone) === JSON.stringify(before.markup) &&
      JSON.stringify(redone.markup) === JSON.stringify(committed.markup),
    thumbnail_current: redone.thumbnail_current,
    zoom: diagnosticsBefore.zoom,
    sensitivity_css_px: plan.sensitivity.value,
    observed_pixels_per_point: Math.max(
      surface.pixels_per_point_x,
      surface.pixels_per_point_y,
    ),
    pixels_per_point: {
      x: surface.pixels_per_point_x,
      y: surface.pixels_per_point_y,
    },
    derived_threshold_points_by_axis: {
      x: plan.sensitivity.value / surface.pixels_per_point_x,
      y: plan.sensitivity.value / surface.pixels_per_point_y,
    },
    derived_threshold_points:
      plan.sensitivity.value /
      Math.max(surface.pixels_per_point_x, surface.pixels_per_point_y),
    observed_raw_delta_points: {
      x: command.pointer_path.unsnapped_end.x - command.pointer_path.start.x,
      y: command.pointer_path.unsnapped_end.y - command.pointer_path.start.y,
    },
    observed_snap_correction_points: {
      x:
        command.pointer_path.expected_snapped_delta.x -
        (command.pointer_path.unsnapped_end.x - command.pointer_path.start.x),
      y:
        command.pointer_path.expected_snapped_delta.y -
        (command.pointer_path.unsnapped_end.y - command.pointer_path.start.y),
    },
  };
  const assessment = assessElectronNativeSnapTransformV5(command, observation);
  const milestoneChecks = {
    "timestamped-native-input-complete":
      observation.observed_sample_count === command.expected_sample_count &&
      timing.within_tolerance,
    "snap-target-acquired": observation.snap_target_acquired_count > 0,
    "snap-guide-presented": observation.snap_guide_presented_count > 0,
    "snapped-geometry-exact":
      JSON.stringify(observation.observed_final_rectangle) ===
        JSON.stringify(command.expected_final_rectangle) &&
      assessment.summary.maximum_geometry_deviation_points <= 0.01,
    "gesture-committed-once": observation.gesture_commit_count === 1,
    "undo-redo-exact": observation.undo_redo_exact,
    "thumbnail-current": observation.thumbnail_current,
  };
  const result = {
    command_id: command.id,
    observation,
    setup: {
      grid,
      creation,
      maintained_fill_hit_target: fillHitTarget,
    },
    input: {
      input_lane: nativeX11InputLane,
      injection_api: "XTEST-direct-helper",
      sample_count: replay.pixel_samples.length,
      actual_duration_ms: actualDurationMs,
      timing,
    },
    assessment,
    observed_milestones: [],
    milestones: [],
    exact_fields: Object.fromEntries(
      command.expected_milestones.map((milestone) => [
        milestone,
        milestoneChecks[milestone] === true,
      ]),
    ),
  };
  for (const milestone of command.expected_milestones) {
    addMilestone(
      event,
      result,
      milestone,
      milestoneChecks[milestone] === true,
      observation,
    );
  }
  finalizeCommandMilestones(result, command);
  const exact =
    assessment.passed &&
    result.manifest_milestones_complete &&
    Object.values(result.exact_fields).every(Boolean);
  event("comparison-command-exact-state", {
    command_id: command.id,
    passed: exact,
    exact_fields: result.exact_fields,
  });
  event("electron-v5-native-snap-baseline", observation);
  return {
    contract,
    input_lane: nativeX11InputLane,
    command_results: [result],
    semantic_summary: assessment.summary,
    exact_manifest_replay: exact,
    decision_timing_eligible: exact,
    blocker: exact
      ? null
      : "native snap transform did not pass exact v5 receipts",
  };
}

async function waitForElectronCurrentRaster(
  cdp,
  pdfPath,
  description,
  { baselineDefect = null } = {},
) {
  return cdp.evaluate(`(async () => {
    const deadline = performance.now() + 90000;
    const baselineDefect = ${JSON.stringify(baselineDefect)};
    const boundedDefectObservationMs = 5000;
    let exactDefectStartedAt = null;
    let last = null;
    while (performance.now() < deadline) {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const currentPage = diagnostics.currentPage;
      const page = document.querySelector('[data-testid="page-' + (currentPage + 1) + '"]');
      const ready = [...(page?.querySelectorAll('[data-render-state="ready"]') ?? [])];
      const activeTab = diagnostics.tabs.find(({ active }) => active);
      const renderError = diagnostics.lastPageRenderError
        ?? activeTab?.diagnostics?.lastPageRenderError
        ?? null;
      const visibleRasterPresented = ready.some(
        (surface) => surface.getAttribute('data-render-quality') !== 'stale-preview',
      );
      const errorPresented = renderError != null
        || Boolean(document.querySelector('[data-testid="document-error"], [role="alert"]'));
      last = {
        requested_path: ${JSON.stringify(pdfPath)},
        active_path: diagnostics.documentPath,
        tab_count: diagnostics.tabs.length,
        page_count: diagnostics.pageCount,
        visible_page_indices: diagnostics.visiblePageIndices,
        queued_page_renders: diagnostics.queuedPageRenders,
        inflight_page_renders: diagnostics.inflightPageRenders,
        last_page_render_error: diagnostics.lastPageRenderError,
        session_last_page_render_error: activeTab?.diagnostics?.lastPageRenderError ?? null,
        session_first_visible_page_ready: activeTab?.diagnostics?.firstVisiblePageReady ?? null,
        dom_page_count: document.querySelectorAll('[data-page-index][data-testid^="page-"]').length,
        startup_spinner_present: Boolean(document.querySelector('[data-testid="document-viewport"] [data-slot="spinner"]')),
        activated_fixture_id: baselineDefect?.activated_fixture_id ?? null,
        activation_ordinal: baselineDefect?.activation_ordinal ?? null,
        live_application_observed: true,
        visible_raster_presented: visibleRasterPresented,
        error_presented: errorPresented,
      };
      if (diagnostics.documentPath === ${JSON.stringify(pdfPath)}
        && diagnostics.pageRenderReady === true
        && ready.length > 0
        && ready.every((surface) => surface.getAttribute('data-render-quality') !== 'stale-preview')) {
        return {
            document_path: diagnostics.documentPath,
            active_tab_id: diagnostics.activeTabId,
            active_tab_index: diagnostics.activeTabIndex,
            page_count: diagnostics.pageCount,
            markup_count: diagnostics.markupCount,
            current_page: currentPage,
            current_generation_presented: true,
            tabs: diagnostics.tabs,
        };
      }
      const exactDefectCandidate = baselineDefect
        && diagnostics.documentPath === ${JSON.stringify(pdfPath)}
        && diagnostics.tabs.length >= 2
        && diagnostics.pageCount === 526
        && diagnostics.visiblePageIndices.length === 0
        && diagnostics.queuedPageRenders === 0
        && diagnostics.inflightPageRenders === 0
        && renderError == null
        && visibleRasterPresented === false
        && errorPresented === false;
      if (exactDefectCandidate) {
        exactDefectStartedAt ??= performance.now();
        const duration = performance.now() - exactDefectStartedAt;
        if (duration >= boundedDefectObservationMs) {
          return {
            baseline_defect_observation: {
              ...last,
              bounded_wait_completed: true,
              live_observation_duration_ms: duration,
            },
          };
        }
      } else {
        exactDefectStartedAt = null;
      }
      await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    throw new Error(${JSON.stringify(`Timed out waiting for ${description}: `)} + JSON.stringify(last));
  })()`);
}

async function nativeHoverTestId(cdp, target, testId) {
  const geometry = await elementGeometry(cdp, testId);
  const replay = buildClickReplay({
    windowId: target.window_id,
    point: geometry.center,
    logicalSize: geometry.window_logical_size,
    windowGeometry: target.geometry,
  });
  await runXdotool(
    [
      "mousemove",
      "--window",
      String(target.window_id),
      String(replay.pixel.x),
      String(replay.pixel.y),
    ],
    0,
  );
  await cdp.evaluate(
    `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
  );
  return {
    ...replay.metadata,
    pixel: replay.pixel,
    observed_geometry: geometry,
  };
}

async function nativeCloseElectronFixture(cdp, target, pdfPath) {
  const diagnostics = await cdp.evaluate(
    `window.__butterPaperTestHooks.getDiagnostics()`,
  );
  const tabIndex = diagnostics.tabs.findIndex(
    ({ filePath }) => filePath === pdfPath,
  );
  if (tabIndex < 0) throw new Error(`missing Electron tab ${pdfPath}`);
  const activate = await nativeClickTestId(
    cdp,
    target,
    `document-tab-${tabIndex}`,
  );
  await waitForElectronCurrentRaster(
    cdp,
    pdfPath,
    `active tab before closing ${pdfPath}`,
  );
  const hover = await nativeHoverTestId(
    cdp,
    target,
    `document-tab-${tabIndex}`,
  );
  const close = await nativeClickTestId(
    cdp,
    target,
    `document-tab-close-${tabIndex}`,
  );
  const after = await waitForEditorCondition(
    cdp,
    `(() => {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      return diagnostics.tabs.every(({ filePath }) => filePath !== ${JSON.stringify(pdfPath)})
        ? diagnostics
        : null;
    })()`,
    `closed Electron tab ${pdfPath}`,
    10_000,
  );
  return { activate, hover, close, after };
}

async function runElectronNativeMultiDocumentSessionV5(
  cdp,
  contract,
  event,
  target,
  pdfPaths,
  applicationPid,
) {
  const fixtureIds = contract.fixture_ids;
  const fixturePath = new Map(
    fixtureIds.map((fixtureId, index) => [fixtureId, pdfPaths[index]]),
  );
  const processIds = [];
  const resourceObservations = [];
  const rasterReceipts = [];
  const recordResources = async (phase, fixtureId) => {
    const process = await sampleProcessTree(applicationPid);
    const renderer = await captureElectronReadyRasterResources(cdp);
    processIds.push(applicationPid);
    const observation = { phase, fixture_id: fixtureId, process, renderer };
    resourceObservations.push(observation);
    return observation;
  };
  const commandById = new Map(
    contract.commands.map((command) => [command.id, command]),
  );

  const openInputs = [];
  rasterReceipts.push(
    await waitForElectronCurrentRaster(
      cdp,
      fixturePath.get(fixtureIds[0]),
      `initial multi-document raster ${fixtureIds[0]}`,
    ),
  );
  await recordResources("open", fixtureIds[0]);
  for (let index = 1; index < fixtureIds.length; index += 1) {
    const fixtureId = fixtureIds[index];
    const pdfPath = fixturePath.get(fixtureId);
    openInputs.push({
      fixture_id: fixtureId,
      input: await runElectronNativeOpenAction(cdp, target, pdfPath),
    });
    const rasterReceipt = await waitForElectronCurrentRaster(
      cdp,
      pdfPath,
      `multi-document raster after open ${fixtureId}`,
      fixtureId === "nasa-apollo-summary-526-v1" && index === 1
        ? {
            baselineDefect: {
              activated_fixture_id: fixtureId,
              activation_ordinal: index + 1,
            },
          }
        : undefined,
    );
    const baselineSummary = buildElectronSecondNasaBaselineSummary(
      rasterReceipt?.baseline_defect_observation,
    );
    if (baselineSummary) {
      const commandResults = contract.commands.map((command, commandIndex) => {
        const live = commandIndex === 0;
        const provenMilestones = live
          ? command.expected_milestones.filter(
              (milestone) => milestone === "application-process-id-recorded",
            )
          : [];
        return {
          command_id: command.id,
          live,
          observation: {
            semantic_summary: baselineSummary,
            bounded_live_observation: live
              ? rasterReceipt.baseline_defect_observation
              : undefined,
          },
          observed_milestones: provenMilestones,
          milestones: [],
          exact_fields: Object.fromEntries(
            command.expected_milestones.map((milestone) => [
              milestone,
              provenMilestones.includes(milestone),
            ]),
          ),
          manifest_milestones_complete: false,
        };
      });
      event("electron-v5-native-multi-document-session", baselineSummary);
      return {
        contract,
        input_lane: nativeX11InputLane,
        command_results: commandResults,
        semantic_summary: baselineSummary,
        semantic_assessment: {
          passed: false,
          known_baseline_defect_id: baselineSummary.known_baseline_defect_id,
        },
        exact_manifest_replay: false,
        decision_timing_eligible: false,
        blocker:
          "known maintained Electron second-NASA visible-layout scheduling defect",
      };
    }
    rasterReceipts.push(rasterReceipt);
    await recordResources("open", fixtureId);
  }
  const openDiagnostics = await cdp.evaluate(
    `window.__butterPaperTestHooks.getDiagnostics()`,
  );
  const openedFixtureIds = openDiagnostics.tabs.map(({ filePath }) =>
    fixtureIds.find((fixtureId) => fixturePath.get(fixtureId) === filePath),
  );

  const switchCommand = commandById.get("session:switch-four-fixtures");
  const switchInputs = [];
  const stateByFixture = new Map();
  for (const fixtureId of switchCommand.switch_sequence) {
    const pdfPath = fixturePath.get(fixtureId);
    const diagnostics = await cdp.evaluate(
      `window.__butterPaperTestHooks.getDiagnostics()`,
    );
    const tabIndex = diagnostics.tabs.findIndex(
      ({ filePath }) => filePath === pdfPath,
    );
    if (tabIndex < 0) throw new Error(`missing switch tab ${fixtureId}`);
    switchInputs.push({
      fixture_id: fixtureId,
      input: await nativeClickTestId(cdp, target, `document-tab-${tabIndex}`),
    });
    const receipt = await waitForElectronCurrentRaster(
      cdp,
      pdfPath,
      `multi-document raster after switch ${fixtureId}`,
    );
    rasterReceipts.push(receipt);
    stateByFixture.set(fixtureId, {
      file_path: receipt.document_path,
      active_tab_id: receipt.active_tab_id,
      markup_count: receipt.markup_count,
      dirty: receipt.tabs.find(({ filePath }) => filePath === pdfPath)?.dirty,
    });
    await recordResources("switch", fixtureId);
  }

  const denseFixtureId = "bp-annotation-density-v1";
  const editCommand = commandById.get("session:edit-dense-rectangle");
  await nativeClickTestId(cdp, target, "viewer-fit-page");
  await waitForEditorCondition(
    cdp,
    `window.__butterPaperTestHooks.getDiagnostics().zoomPreset === 'fit-page' && window.__butterPaperTestHooks.getDiagnostics().pageRenderReady`,
    "Fit Page before multi-document dense edit",
  );
  const disabledSnapSources = await disableSnappingWithNativeUi(cdp, target);
  const creation = await createElectronNativeV5Rectangle(
    cdp,
    target,
    editCommand.setup_rectangle,
  );
  const denseMarkupId = creation.markup.id;
  await nativeClickTestId(cdp, target, "properties-sidebar-trigger");
  await waitForEditorCondition(
    cdp,
    `document.querySelector('[data-testid="right-sidebar-heading"]')?.textContent?.trim() === 'Rectangle'`,
    "multi-document dense Rectangle properties",
  );
  await nativeReplaceLabeledNumber(
    cdp,
    target,
    "Line Width",
    editCommand.setup_rectangle.stroke_width_points,
    "right-sidebar-heading",
  );
  await waitForEditorCondition(
    cdp,
    `window.__butterPaperTestHooks.getActiveDocument()?.markups.find(({ id }) => id === ${JSON.stringify(denseMarkupId)})?.appearance?.stroke?.widthPt === ${editCommand.setup_rectangle.stroke_width_points}`,
    "multi-document dense setup width",
  );
  const beforePropertyHistory = await cdp.evaluate(
    `window.__butterPaperTestHooks.getDocumentHistory()`,
  );
  const propertyInput = await nativeReplaceLabeledNumber(
    cdp,
    target,
    "Line Width",
    editCommand.property_edit.to,
    "right-sidebar-heading",
  );
  const edited = await waitForEditorCondition(
    cdp,
    `(() => {
      const markup = window.__butterPaperTestHooks.getActiveDocument()?.markups.find(({ id }) => id === ${JSON.stringify(denseMarkupId)});
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      return markup?.appearance?.stroke?.widthPt === ${editCommand.property_edit.to}
        && diagnostics.tabs.find(({ filePath }) => filePath === ${JSON.stringify(fixturePath.get(denseFixtureId))})?.dirty === true
        ? { markup, diagnostics, history: window.__butterPaperTestHooks.getDocumentHistory() }
        : null;
    })()`,
    "multi-document dense property edit",
  );
  const otherStatesUnchanged = [...stateByFixture.entries()]
    .filter(([fixtureId]) => fixtureId !== denseFixtureId)
    .every(([fixtureId, before]) => {
      const tab = edited.diagnostics.tabs.find(
        ({ filePath }) => filePath === fixturePath.get(fixtureId),
      );
      return tab?.id === before.active_tab_id && tab.dirty === before.dirty;
    });
  const denseHistoryDelta = edited.history.past - beforePropertyHistory.past;
  const leftSidebarOpen = await cdp.evaluate(
    `Boolean(document.querySelector('[data-testid="left-sidebar"]'))`,
  );
  if (!leftSidebarOpen) await nativeClickTestId(cdp, target, "left-rail-pages");
  const thumbnailCurrent = await waitForEditorCondition(
    cdp,
    `Boolean(document.querySelector(${JSON.stringify(`[data-testid="thumbnail-markup-${denseMarkupId}"]`)}))`,
    "multi-document dense thumbnail",
  );

  const closeCommand = commandById.get("session:close-three-and-recover");
  const closeReceipts = [];
  for (const fixtureId of closeCommand.close_sequence) {
    const beforeResources = await sampleProcessTree(applicationPid);
    const close = await nativeCloseElectronFixture(
      cdp,
      target,
      fixturePath.get(fixtureId),
    );
    const afterResources = await sampleProcessTree(applicationPid);
    processIds.push(applicationPid);
    closeReceipts.push({
      fixture_id: fixtureId,
      before_resources: beforeResources,
      after_resources: afterResources,
      tab_absent: close.after.tabs.every(
        ({ filePath }) => filePath !== fixturePath.get(fixtureId),
      ),
      process_stable: close.after.tabs.length > 0,
      input: close,
    });
  }
  const finalReceipt = await waitForElectronCurrentRaster(
    cdp,
    fixturePath.get(denseFixtureId),
    "remaining dense multi-document raster",
  );
  const finalMarkup = await activeMarkup(cdp, denseMarkupId);
  const finalProcess = await sampleProcessTree(applicationPid);
  processIds.push(applicationPid);

  const aggregateResourcesComplete =
    resourceObservations.length === 8 &&
    resourceObservations.every(
      ({ process, renderer }) =>
        process !== null && renderer?.current_generation_presented === true,
    );
  const closedResourcesReleased = closeReceipts.every(
    ({ tab_absent, process_stable, before_resources, after_resources }) =>
      tab_absent &&
      process_stable &&
      before_resources !== null &&
      after_resources !== null,
  );
  const perDocumentStateIsolated =
    new Set(openDiagnostics.tabs.map(({ id }) => id)).size === 4 &&
    new Set(openDiagnostics.tabs.map(({ filePath }) => filePath)).size === 4 &&
    otherStatesUnchanged;
  const semanticSummary = {
    opened_fixture_ids: openedFixtureIds,
    switch_sequence: [...switchCommand.switch_sequence],
    close_sequence: [...closeCommand.close_sequence],
    process_restart_count: 0,
    observed_process_ids: [...new Set(processIds)],
    stable_process_id: applicationPid,
    per_document_state_isolated: perDocumentStateIsolated,
    current_raster_receipt_count: rasterReceipts.length,
    dense_rectangle_property_user_gesture_count: 1,
    dense_rectangle_property_history_revision_delta: denseHistoryDelta,
    dense_rectangle_stroke_width_points: markupStrokeWidth(finalMarkup),
    closed_document_resources_released: closedResourcesReleased,
    remaining_document_count: finalReceipt.tabs.length,
    remaining_fixture_id: denseFixtureId,
    dense_document_active:
      finalReceipt.document_path === fixturePath.get(denseFixtureId),
    aggregate_resource_observations_complete: aggregateResourcesComplete,
    interactive_document_shell:
      finalReceipt.current_generation_presented === true &&
      finalProcess !== null,
  };
  const semanticAssessment =
    assessElectronMultiDocumentSessionV5(semanticSummary);

  const factsByCommand = new Map([
    [
      "session:open-four-fixtures",
      {
        "application-process-id-recorded": Number.isInteger(applicationPid),
        "four-documents-opened": openDiagnostics.tabs.length === 4,
        "tab-order-exact":
          JSON.stringify(openedFixtureIds) === JSON.stringify(fixtureIds),
        "document-identities-distinct":
          new Set(openDiagnostics.tabs.map(({ id }) => id)).size === 4,
        "current-raster-after-each-open":
          rasterReceipts.slice(0, 4).length === 4,
        "aggregate-resource-observations-complete": aggregateResourcesComplete,
      },
    ],
    [
      "session:switch-four-fixtures",
      {
        "application-process-id-stable": new Set(processIds).size === 1,
        "trusted-native-input-complete": switchInputs.length === 4,
        "switch-sequence-exact": switchInputs.every(
          ({ fixture_id: fixtureId }, index) =>
            fixtureId === switchCommand.switch_sequence[index],
        ),
        "per-document-state-isolated": perDocumentStateIsolated,
        "current-raster-after-each-switch":
          rasterReceipts.slice(4).length === 4,
        "aggregate-resource-observations-complete": aggregateResourcesComplete,
      },
    ],
    [
      "session:edit-dense-rectangle",
      {
        "application-process-id-stable": new Set(processIds).size === 1,
        "trusted-native-input-complete":
          propertyInput.input_lane === nativeX11InputLane,
        "dense-rectangle-created-once": Boolean(denseMarkupId),
        "dense-rectangle-property-gesture-observed":
          denseHistoryDelta === 2 && markupStrokeWidth(finalMarkup) === 4,
        "dense-document-dirty":
          edited.diagnostics.tabs.find(
            ({ filePath }) => filePath === fixturePath.get(denseFixtureId),
          )?.dirty === true,
        "other-document-states-unchanged": otherStatesUnchanged,
        "thumbnail-current": thumbnailCurrent === true,
      },
    ],
    [
      "session:close-three-and-recover",
      {
        "application-process-id-stable": new Set(processIds).size === 1,
        "close-three-sequence-exact": closeReceipts.every(
          ({ fixture_id: fixtureId }, index) =>
            fixtureId === closeCommand.close_sequence[index],
        ),
        "closed-document-resources-released": closedResourcesReleased,
        "memory-recovery-recorded": closeReceipts.every(
          ({ before_resources, after_resources }) =>
            before_resources !== null && after_resources !== null,
        ),
        "one-document-remains": finalReceipt.tabs.length === 1,
        "dense-document-active": semanticSummary.dense_document_active,
        "dense-rectangle-property-current":
          markupStrokeWidth(finalMarkup) === 4,
        "interactive-document-shell":
          semanticSummary.interactive_document_shell,
      },
    ],
  ]);
  const commandResults = contract.commands.map((command) => {
    const facts = factsByCommand.get(command.id) ?? {};
    const result = {
      command_id: command.id,
      observation: {
        semantic_summary: semanticSummary,
        open_inputs:
          command.id === "session:open-four-fixtures" ? openInputs : undefined,
        switch_inputs:
          command.id === "session:switch-four-fixtures"
            ? switchInputs
            : undefined,
        setup:
          command.id === "session:edit-dense-rectangle"
            ? { disabled_snap_sources: disabledSnapSources, creation }
            : undefined,
        property_input:
          command.id === "session:edit-dense-rectangle"
            ? propertyInput
            : undefined,
        close_receipts:
          command.id === "session:close-three-and-recover"
            ? closeReceipts
            : undefined,
        resource_observations: resourceObservations,
      },
      observed_milestones: [],
      milestones: [],
      exact_fields: Object.fromEntries(
        command.expected_milestones.map((milestone) => [
          milestone,
          facts[milestone] === true,
        ]),
      ),
    };
    for (const milestone of command.expected_milestones) {
      addMilestone(
        event,
        result,
        milestone,
        facts[milestone] === true,
        semanticSummary,
      );
    }
    finalizeCommandMilestones(result, command);
    event("comparison-command-exact-state", {
      command_id: command.id,
      passed: result.manifest_milestones_complete,
      exact_fields: result.exact_fields,
    });
    return result;
  });
  const exact = commandResults.every(
    ({ manifest_milestones_complete: complete, exact_fields: fields }) =>
      complete && Object.values(fields).every(Boolean),
  );
  event("electron-v5-native-multi-document-session", semanticSummary);
  return {
    contract,
    input_lane: nativeX11InputLane,
    command_results: commandResults,
    semantic_summary: semanticSummary,
    semantic_assessment: semanticAssessment,
    exact_manifest_replay: exact && semanticAssessment.passed,
    decision_timing_eligible: exact && semanticAssessment.passed,
    blocker:
      exact && semanticAssessment.passed
        ? null
        : "native multi-document session did not pass exact v5 receipts",
  };
}

async function observeElectronDynamicFidelityState(cdp, command, sequence) {
  const observed = await cdp.evaluate(`(() => {
    const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
    const viewport = document.querySelector('[data-testid="document-viewport"]');
    if (!viewport) throw new Error('dynamic fidelity viewport is unavailable');
    const viewportBounds = viewport.getBoundingClientRect();
    const documentModel = window.__butterPaperTestHooks.getActiveDocument();
    const visiblePages = [...document.querySelectorAll('[data-page-index][data-testid^="page-"]')]
      .map((page) => {
        const pageNumber = Number(page.getAttribute('data-testid')?.slice(5));
        const surface = page.querySelector('[data-render-state]');
        const bounds = page.getBoundingClientRect();
        const left = Math.max(bounds.left, viewportBounds.left);
        const right = Math.min(bounds.right, viewportBounds.right);
        const top = Math.max(bounds.top, viewportBounds.top);
        const bottom = Math.min(bounds.bottom, viewportBounds.bottom);
        if (right <= left || bottom <= top) return null;
        const current = surface?.getAttribute('data-render-state') === 'ready'
          && surface?.getAttribute('data-render-quality') !== 'stale-preview';
        const rasterWidth = surface instanceof HTMLCanvasElement
          ? surface.width
          : surface instanceof HTMLImageElement
            ? surface.naturalWidth
            : 0;
        const pageSize = documentModel?.pages?.[pageNumber - 1]?.size;
        if (!pageSize) return null;
        return {
          page_number: pageNumber,
          page_size_points: pageSize,
          page_bounds_window_logical: {
            x: bounds.left,
            y: bounds.top,
            width: bounds.width,
            height: bounds.height,
          },
          painted_outer_page_bounds_window_logical: {
            x: bounds.left,
            y: bounds.top,
            width: bounds.width,
            height: bounds.height,
          },
          pixels_per_point: {
            x: bounds.width / pageSize.width,
            y: bounds.height / pageSize.height,
          },
          painted_render_generation: diagnostics.renderCacheEntries,
          painted_generation_current: current,
          visible_intersection_area_css_px2: (right - left) * (bottom - top),
          current_raster_ready_area_fraction: current ? 1 : 0,
          current_raster_device_pixels_per_css_pixel:
            current && bounds.width > 0 ? rasterWidth / bounds.width : 0,
        };
      })
      .filter(Boolean);
    return {
      active_page: diagnostics.currentPage + 1,
      zoom_percent: diagnostics.zoom * 100,
      display_scale_factor: window.devicePixelRatio,
      visible_page_count: visiblePages.length,
      visible_pages: visiblePages,
      scroll_offset_css_px: { x: viewport.scrollLeft, y: viewport.scrollTop },
      viewport_size_css_px: {
        width: viewportBounds.width,
        height: viewportBounds.height,
      },
      viewport_bounds_window_logical: {
        x: viewportBounds.left,
        y: viewportBounds.top,
        width: viewportBounds.width,
        height: viewportBounds.height,
      },
      render_generation: diagnostics.renderCacheEntries,
      painted_generation_current: visiblePages.every(
        (page) => page.painted_generation_current,
      ),
      platform_draw_submitted: true,
      dom_page_count: document.querySelectorAll('[data-page-index][data-testid^="page-"]').length,
    };
  })()`);
  return {
    schema_version: 1,
    event: "dynamic-fidelity-state",
    command_id: command.id,
    state_sequence: sequence,
    painted_state_sequence: sequence,
    runner_observed_monotonic_ms: Number(process.hrtime.bigint()) / 1e6,
    ...observed,
  };
}

async function buildElectronDynamicCheckpointPageGeometries(
  cdp,
  command,
  initial,
) {
  const pageSizes = await cdp.evaluate(`(() => {
    const documentModel = window.__butterPaperTestHooks.getActiveDocument();
    return documentModel?.pages?.map((page) => page.size) ?? [];
  })()`);
  const firstPage = initial.visible_pages.find(
    ({ page_number: page }) => page === 1,
  );
  if (!firstPage || pageSizes.length < 29) {
    throw new Error(
      "dynamic checkpoint geometry requires painted page 1 and 29 page sizes",
    );
  }
  const gapCssPx = 24;
  const targetPages = new Set(
    command.registered_crops.map(({ page_number: page }) => page),
  );
  let top = firstPage.painted_outer_page_bounds_window_logical.y;
  const geometries = [];
  for (let pageNumber = 1; pageNumber <= pageSizes.length; pageNumber += 1) {
    const pageSize = pageSizes[pageNumber - 1];
    if (targetPages.has(pageNumber)) {
      const left =
        initial.viewport_bounds_window_logical.x +
        Math.max(
          gapCssPx,
          (initial.viewport_bounds_window_logical.width - pageSize.width) / 2,
        ) -
        initial.scroll_offset_css_px.x;
      geometries.push({
        page_number: pageNumber,
        page_size_points: pageSize,
        painted_outer_page_bounds_at_initial_scroll_window_logical: {
          x: left,
          y: top,
          width: pageSize.width,
          height: pageSize.height,
        },
        geometry_basis:
          pageNumber === 1
            ? "actual-painted-outer-page"
            : "maintained-continuous-layout-from-actual-painted-page-1",
      });
    }
    top += pageSize.height + gapCssPx;
  }
  if (geometries.length !== targetPages.size) {
    throw new Error("dynamic checkpoint page geometry set is incomplete");
  }
  return geometries;
}

async function runElectronDynamicStateObserver(cdp, command, control, states) {
  const intervalMs = 1000 / 120;
  const started = performance.now();
  let sequence = 1;
  while (control.active) {
    const due = started + sequence * intervalMs;
    const remaining = due - performance.now();
    if (remaining > 0) await delay(remaining);
    states.push(
      await observeElectronDynamicFidelityState(cdp, command, sequence),
    );
    sequence += 1;
  }
  return states;
}

export function bindElectronDynamicObserverSamples(
  observerTicks,
  states,
  command,
) {
  const orderedStates = [...states].sort(
    (left, right) =>
      left.runner_observed_monotonic_ms - right.runner_observed_monotonic_ms,
  );
  if (orderedStates.length === 0) {
    throw new Error("Electron dynamic observer has no application states");
  }
  const firstTickObservedMonotonicMs = observerTicks[0]?.observed_monotonic_ms;
  let stateIndex = 0;
  const samples = observerTicks.map((tick) => {
    while (
      stateIndex + 1 < orderedStates.length &&
      orderedStates[stateIndex + 1].runner_observed_monotonic_ms <=
        tick.observed_monotonic_ms
    ) {
      stateIndex += 1;
    }
    const state = orderedStates[stateIndex];
    if (state.runner_observed_monotonic_ms > tick.observed_monotonic_ms) {
      throw new Error(
        `Electron observer sample ${tick.sample_index} preceded the first application state`,
      );
    }
    const fidelity =
      state.visible_pages.length > 0
        ? measureVisibleRasterFidelity(state.visible_pages)
        : {
            visible_page_ready_fraction: 0,
            visible_raster_ready_area_fraction: 0,
            visible_raster_pixel_density: 0,
          };
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

async function calibrateElectronNativeWheelDistance(cdp, target, replay) {
  const before = await cdp.evaluate(`(() => {
    const viewport = document.querySelector('[data-testid="document-viewport"]');
    if (!viewport) throw new Error('dynamic wheel calibration viewport is unavailable');
    return { scroll_top: viewport.scrollTop, height: viewport.clientHeight };
  })()`);
  if (
    !Number.isFinite(before.scroll_top) ||
    before.scroll_top < 0 ||
    before.height <= 0
  ) {
    throw new Error(
      `dynamic wheel calibration requires a valid initial viewport; received ${JSON.stringify(before)}`,
    );
  }
  const pointArgs = [
    "mousemove",
    "--window",
    String(target.window_id),
    String(replay.pixel_target.x),
    String(replay.pixel_target.y),
  ];
  await runXdotool([...pointArgs, "click", "5"], 0);
  const forward = await waitForEditorCondition(
    cdp,
    `(() => {
      const viewport = document.querySelector('[data-testid="document-viewport"]');
      return viewport && viewport.scrollTop > ${JSON.stringify(before.scroll_top)}
        ? viewport.scrollTop
        : null;
    })()`,
    "one calibrated native wheel step",
    5_000,
  );
  await runXdotool([...pointArgs, "click", "4"], 0);
  await waitForEditorCondition(
    cdp,
    `document.querySelector('[data-testid="document-viewport"]')?.scrollTop === ${JSON.stringify(before.scroll_top)}`,
    "native wheel calibration return to baseline",
    5_000,
  );
  await delay(250);
  return {
    input_lane: nativeX11InputLane,
    injection_api: "XTEST-via-xdotool",
    initial_scroll_top_css_px: before.scroll_top,
    viewport_height_css_px: before.height,
    forward_scroll_top_css_px: forward,
    wheel_delta_css_px: forward - before.scroll_top,
    baseline_restored: true,
  };
}

export function electronCropCaptureStateStable(before, after, pageNumber) {
  const beforePage = before.visible_pages.find(
    (page) => page.page_number === pageNumber,
  );
  const afterPage = after.visible_pages.find(
    (page) => page.page_number === pageNumber,
  );
  if (!beforePage || !afterPage) return false;
  const beforeScroll = before.scroll_offset_css_px;
  const afterScroll = after.scroll_offset_css_px;
  const beforeBounds = beforePage.painted_outer_page_bounds_window_logical;
  const afterBounds = afterPage.painted_outer_page_bounds_window_logical;
  return (
    beforeScroll.x === afterScroll.x &&
    beforeScroll.y === afterScroll.y &&
    beforeBounds.x === afterBounds.x &&
    beforeBounds.y === afterBounds.y &&
    beforeBounds.width === afterBounds.width &&
    beforeBounds.height === afterBounds.height &&
    beforePage.current_raster_ready_area_fraction === 1 &&
    afterPage.current_raster_ready_area_fraction === 1 &&
    beforePage.painted_generation_current === true &&
    afterPage.painted_generation_current === true &&
    beforePage.painted_render_generation === afterPage.painted_render_generation
  );
}

async function captureElectronRegisteredCrop({
  crop,
  state,
  target,
  artifactDirectory,
  cdp,
  command,
  heldCheckpoint,
  captureServer,
  helperFeed,
}) {
  const historicalPage = state.visible_pages.find(
    (entry) =>
      entry.page_number === crop.page_number &&
      entry.current_raster_ready_area_fraction === 1,
  );
  if (!historicalPage) return null;
  if (
    Math.abs(
      state.scroll_offset_css_px.y - heldCheckpoint.actual_scroll_offset_css_px,
    ) > 0.01
  ) {
    return null;
  }
  const captureState = await observeElectronDynamicFidelityState(
    cdp,
    command,
    state.state_sequence,
  );
  requireDynamicHelperHold(
    helperFeed,
    crop.page_number,
    crop.crop_id,
    "semantic-ready",
    {
      before: dynamicCaptureStateEvidence(captureState, crop.page_number),
      middle: null,
      after: null,
    },
  );
  const page = captureState.visible_pages.find(
    (entry) =>
      entry.page_number === crop.page_number &&
      entry.current_raster_ready_area_fraction === 1,
  );
  if (!page) return null;
  const logicalCrop = mapPdfRectToImagePixels(
    crop.pdf_rect,
    page.page_size_points,
    page.painted_outer_page_bounds_window_logical,
  );
  const viewport = captureState.viewport_bounds_window_logical;
  if (
    logicalCrop.left < viewport.x ||
    logicalCrop.top < viewport.y ||
    logicalCrop.left + logicalCrop.width > viewport.x + viewport.width ||
    logicalCrop.top + logicalCrop.height > viewport.y + viewport.height
  ) {
    return null;
  }
  const beforePpmPath = resolve(
    artifactDirectory,
    `${crop.crop_id}-before-window.ppm`,
  );
  const afterPpmPath = resolve(artifactDirectory, `${crop.crop_id}-window.ppm`);
  const beforeStateReceipt = {
    state: captureState,
    runner_snapshot_monotonic_ms: Number(process.hrtime.bigint()) / 1e6,
  };
  const helperHoldBefore = requireDynamicHelperHold(
    helperFeed,
    crop.page_number,
    crop.crop_id,
    "before-first-XGetImage",
    {
      before: dynamicCaptureStateEvidence(captureState, crop.page_number),
      middle: null,
      after: null,
    },
  );
  const beforeCapture = await captureServer.capture(
    `${crop.crop_id}:before`,
    beforePpmPath,
  );
  const middleState = await observeElectronDynamicFidelityState(
    cdp,
    command,
    captureState.state_sequence + 1,
  );
  const helperHoldMiddle = requireDynamicHelperHold(
    helperFeed,
    crop.page_number,
    crop.crop_id,
    "after-first-XGetImage",
    {
      before: dynamicCaptureStateEvidence(captureState, crop.page_number),
      middle: dynamicCaptureStateEvidence(middleState, crop.page_number),
      after: null,
    },
  );
  if (
    !electronCropCaptureStateStable(captureState, middleState, crop.page_number)
  ) {
    throw dynamicCaptureError(
      `Electron ${crop.crop_id} painted state changed during first XGetImage capture`,
      {
        crop_id: crop.crop_id,
        page_number: crop.page_number,
        capture_phase: "after-first-XGetImage",
        helper_hold: helperHoldMiddle,
        state_receipts: {
          before: dynamicCaptureStateEvidence(captureState, crop.page_number),
          middle: dynamicCaptureStateEvidence(middleState, crop.page_number),
          after: null,
        },
      },
    );
  }
  await delay(16);
  requireDynamicHelperHold(
    helperFeed,
    crop.page_number,
    crop.crop_id,
    "before-second-XGetImage",
    {
      before: dynamicCaptureStateEvidence(captureState, crop.page_number),
      middle: dynamicCaptureStateEvidence(middleState, crop.page_number),
      after: null,
    },
  );
  const afterPresentedCapture = await captureServer.capture(
    `${crop.crop_id}:after`,
    afterPpmPath,
  );
  const afterState = await observeElectronDynamicFidelityState(
    cdp,
    command,
    middleState.state_sequence + 1,
  );
  const helperHoldAfter = requireDynamicHelperHold(
    helperFeed,
    crop.page_number,
    crop.crop_id,
    "after-second-XGetImage",
    {
      before: dynamicCaptureStateEvidence(captureState, crop.page_number),
      middle: dynamicCaptureStateEvidence(middleState, crop.page_number),
      after: dynamicCaptureStateEvidence(afterState, crop.page_number),
    },
  );
  if (
    !electronCropCaptureStateStable(middleState, afterState, crop.page_number)
  ) {
    throw dynamicCaptureError(
      `Electron ${crop.crop_id} painted state changed during second XGetImage capture`,
      {
        crop_id: crop.crop_id,
        page_number: crop.page_number,
        capture_phase: "after-second-XGetImage",
        helper_hold: helperHoldAfter,
        state_receipts: {
          before: dynamicCaptureStateEvidence(captureState, crop.page_number),
          middle: dynamicCaptureStateEvidence(middleState, crop.page_number),
          after: dynamicCaptureStateEvidence(afterState, crop.page_number),
        },
      },
    );
  }
  return {
    crop_id: crop.crop_id,
    page_number: crop.page_number,
    crop,
    page,
    before_state_receipt: beforeStateReceipt,
    middle_state: middleState,
    after_state_receipt: {
      state: afterState,
      runner_snapshot_monotonic_ms: Number(process.hrtime.bigint()) / 1e6,
    },
    before_capture: beforeCapture,
    after_capture: afterPresentedCapture,
    before_ppm_path: beforePpmPath,
    after_ppm_path: afterPpmPath,
    actual_painted_outer_page_bounds_window_logical:
      page.painted_outer_page_bounds_window_logical,
    held_checkpoint: heldCheckpoint,
    live_helper_hold_receipts: {
      before: helperHoldBefore,
      middle: helperHoldMiddle,
      after: helperHoldAfter,
    },
  };
}

async function captureElectronDynamicCrops({
  cdp,
  command,
  states,
  target,
  artifactDirectory,
  control,
  heldDistancePlan,
  captureServer,
  helperFeed,
}) {
  const pending = new Map(
    command.registered_crops.map((crop) => [crop.crop_id, crop]),
  );
  const receipts = [];
  while ((control.active || pending.size > 0) && pending.size > 0) {
    for (const [cropId, crop] of pending) {
      const helperHold = dynamicHelperHoldState(helperFeed, crop.page_number);
      if (helperHold.phase === "after") {
        throw dynamicCaptureError(
          `dynamic fidelity ${cropId} crop-not-semantic-ready-during-declared-hold`,
          {
            crop_id: cropId,
            page_number: crop.page_number,
            capture_phase: "semantic-ready",
            helper_hold: helperHold,
            state_receipts: {
              current: dynamicCaptureStateEvidence(
                states.at(-1),
                crop.page_number,
              ),
            },
          },
        );
      }
    }
    const available = states.length > 0 ? [states.at(-1)] : [];
    for (const state of available) {
      for (const [cropId, crop] of pending) {
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
        const receipt = await captureElectronRegisteredCrop({
          cdp,
          command,
          crop,
          state,
          target,
          artifactDirectory,
          heldCheckpoint: heldDistancePlan.checkpoints.find(
            ({ crop_id: heldCropId }) => heldCropId === cropId,
          ),
          captureServer,
          helperFeed,
        });
        if (receipt) {
          receipts.push(receipt);
          pending.delete(cropId);
        }
      }
    }
    if (!control.active && helperFeed.complete) break;
    await delay(4);
  }
  return {
    receipts: command.registered_crops.map((crop) =>
      receipts.find(({ crop_id: cropId }) => cropId === crop.crop_id),
    ),
    missing_crop_ids: [...pending.keys()],
  };
}

async function convertAndCompareElectronDynamicCrops({
  captures,
  command,
  target,
  artifactDirectory,
  trajectory,
}) {
  const receipts = [];
  for (const capture of captures) {
    if (!capture) continue;
    const { crop, page } = capture;
    const beforeScreenshotPath = resolve(
      artifactDirectory,
      `${crop.crop_id}-before-window.png`,
    );
    const screenshotPath = resolve(
      artifactDirectory,
      `${crop.crop_id}-window.png`,
    );
    const beforeCandidatePath = resolve(
      artifactDirectory,
      `${crop.crop_id}-before-native-candidate.png`,
    );
    const candidatePath = resolve(
      artifactDirectory,
      `${crop.crop_id}-native-candidate.png`,
    );
    const beforeRegisteredReferencePath = resolve(
      artifactDirectory,
      `${crop.crop_id}-before-reference-registered.png`,
    );
    const registeredReferencePath = resolve(
      artifactDirectory,
      `${crop.crop_id}-reference-registered.png`,
    );
    const beforeConversion = await losslesslyConvertPresentedPpmToPng(
      capture.before_ppm_path,
      beforeScreenshotPath,
    );
    const afterConversion = await losslesslyConvertPresentedPpmToPng(
      capture.after_ppm_path,
      screenshotPath,
    );
    const presentedTiming = validatePresentedCapturePairWindowAndHold({
      beforeCapture: capture.before_capture,
      afterCapture: capture.after_capture,
      pageNumber: crop.page_number,
      trajectory,
      target,
    });
    if (
      capture.before_state_receipt.runner_snapshot_monotonic_ms >
        capture.before_capture.capture_started_monotonic_ms ||
      capture.after_state_receipt.runner_snapshot_monotonic_ms <
        capture.after_capture.capture_ended_monotonic_ms ||
      !electronCropCaptureStateStable(
        capture.before_state_receipt.state,
        capture.after_state_receipt.state,
        crop.page_number,
      )
    ) {
      throw new Error(
        `Electron ${crop.crop_id} XGetImage pair lacks stable before/after painted-state receipts`,
      );
    }
    const referencePath = resolve(
      performanceDirectory,
      "fixtures/reference-crops-v5",
      `${crop.crop_id}.png`,
    );
    const beforeComparison = await registerAndComparePresentedCropV2({
      screenshotPath: beforeScreenshotPath,
      pageBoundsPx: page.painted_outer_page_bounds_window_logical,
      pageSizePt: page.page_size_points,
      pdfRect: crop.pdf_rect,
      referencePath,
      outputCandidatePath: beforeCandidatePath,
      outputRegisteredReferencePath: beforeRegisteredReferencePath,
    });
    const comparison = await registerAndComparePresentedCropV2({
      screenshotPath,
      pageBoundsPx: page.painted_outer_page_bounds_window_logical,
      pageSizePt: page.page_size_points,
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
      crop_id: crop.crop_id,
      page_number: crop.page_number,
      registration_sha256: crop.registration_sha256,
      before_painted_state_sequence:
        capture.before_state_receipt.state.painted_state_sequence,
      middle_painted_state_sequence:
        capture.middle_state.painted_state_sequence,
      after_painted_state_sequence:
        capture.after_state_receipt.state.painted_state_sequence,
      painted_render_generation: page.painted_render_generation,
      painted_generation_stable: true,
      actual_painted_outer_page_bounds_window_logical:
        page.painted_outer_page_bounds_window_logical,
      candidate_comparability: capture.held_checkpoint.candidate_comparability,
      held_checkpoint: capture.held_checkpoint,
      live_helper_hold_receipts: capture.live_helper_hold_receipts,
      before_presented_capture: capture.before_capture,
      after_presented_capture: capture.after_capture,
      presented_capture_correlation: {
        ...presentedTiming,
        same_scroll_offset: true,
        same_painted_bounds: true,
        same_painted_render_generation: true,
        raster_ready_before_and_after: true,
      },
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
  return receipts;
}

async function runElectronNativeDynamicFidelityV5(
  cdp,
  contract,
  event,
  target,
) {
  const command = contract.commands[0];
  await nativeClickTestId(cdp, target, "viewer-zoom-menu");
  await waitForEditorCondition(
    cdp,
    `Boolean(document.querySelector('[data-testid="viewer-zoom-preset-100"]'))`,
    "100% zoom preset before Electron dynamic fidelity",
    5_000,
  );
  await nativeClickTestId(cdp, target, "viewer-zoom-preset-100");
  await waitForEditorCondition(
    cdp,
    `(() => {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      return diagnostics.zoomPreset === 'manual'
        && Math.abs(diagnostics.zoom - 1) <= 0.0001
        && diagnostics.pageRenderReady;
    })()`,
    "fixed 100% painted Electron dynamic fidelity",
    20_000,
  );
  await nativeClickTestId(cdp, target, "viewer-scroll-continuous");
  await waitForEditorCondition(
    cdp,
    `window.__butterPaperTestHooks.getDiagnostics().scrollMode === 'continuous' && window.__butterPaperTestHooks.getDiagnostics().pageRenderReady`,
    "continuous layout before Electron dynamic fidelity",
    20_000,
  );
  let initial = await observeElectronDynamicFidelityState(cdp, command, 0);
  if (initial.visible_pages.length === 0) {
    throw new Error("Electron dynamic fidelity has no initial visible page");
  }
  const ready = {
    surface: {
      window_logical_size: {
        width: await cdp.evaluate(`window.innerWidth`),
        height: await cdp.evaluate(`window.innerHeight`),
      },
      bounds: initial.viewport_bounds_window_logical,
    },
  };
  const replay = buildNativeCommandReplay(command, ready, target);
  const wheelCalibration = await calibrateElectronNativeWheelDistance(
    cdp,
    target,
    replay,
  );
  const checkpointPageGeometries =
    await buildElectronDynamicCheckpointPageGeometries(cdp, command, initial);
  const heldDistancePlan = buildHeldDynamicWheelPlan({
    command,
    viewportHeightCssPx: wheelCalibration.viewport_height_css_px,
    wheelDeltaCssPx: wheelCalibration.wheel_delta_css_px,
    initialScrollOffsetCssPx: wheelCalibration.initial_scroll_top_css_px,
    viewportBoundsWindowLogical: initial.viewport_bounds_window_logical,
    checkpointPageGeometries,
    zoomPercent: initial.zoom_percent,
    displayScaleFactor: initial.display_scale_factor,
  });
  initial = await observeElectronDynamicFidelityState(cdp, command, 0);
  const states = [initial];
  const control = { active: true };
  const artifactDirectory = resolve(
    performanceDirectory,
    "results/local-v5/electron",
    `dynamic-fidelity-${Date.now()}`,
  );
  await mkdir(artifactDirectory, { recursive: true });
  if ((await readdir(artifactDirectory)).length !== 0) {
    throw new Error(
      `Electron dynamic evidence directory must be empty before capture: ${artifactDirectory}`,
    );
  }
  const captureServer = await startPresentedDrawableCaptureServer(target);
  const observerPromise = runIndependentDynamicObserver({
    durationMs: command.duration_ms,
    rateHz: command.observer.rate_hz,
  });
  const statePromise = runElectronDynamicStateObserver(
    cdp,
    command,
    control,
    states,
  );
  const inputSession = await startDirectXTestHeldDynamicWheel(
    replay,
    target,
    heldDistancePlan,
  );
  const cropPromise = captureElectronDynamicCrops({
    cdp,
    command,
    states,
    target,
    artifactDirectory,
    control,
    heldDistancePlan,
    captureServer,
    helperFeed: inputSession.feed,
  });
  let input;
  let observerTicks;
  let cropCapture;
  try {
    [input, observerTicks, cropCapture] = await Promise.all([
      inputSession.completion,
      observerPromise,
      cropPromise,
    ]);
    control.active = false;
    await statePromise;
  } catch (error) {
    control.active = false;
    await retainDynamicCaptureFailureEvidence(artifactDirectory, error).catch(
      () => {},
    );
    throw error;
  } finally {
    control.active = false;
    await statePromise.catch(() => {});
    await captureServer.close().catch(() => {});
  }
  const registeredCrops = await convertAndCompareElectronDynamicCrops({
    captures: cropCapture.receipts,
    command,
    target,
    artifactDirectory,
    trajectory: input.trajectory,
  });
  const samples = bindElectronDynamicObserverSamples(
    observerTicks,
    states,
    command,
  );
  const timing = assessReplayTiming(replay, input.actual_duration_ms);
  const registeredCropsPassed = registeredDynamicCropsPassed(
    registeredCrops,
    command,
  );
  const maximumDomPageCount = Math.max(
    ...states.map(({ dom_page_count: count }) => count),
  );
  const finalState = states.at(-1);
  const forwardApexSample = samples.reduce((nearest, sample) =>
    Math.abs(sample.scheduled_offset_ms - command.path.forward_duration_ms) <
    Math.abs(nearest.scheduled_offset_ms - command.path.forward_duration_ms)
      ? sample
      : nearest,
  );
  const semanticSummary = {
    trusted_native_input: true,
    trajectory_sample_count: input.trajectory.length,
    native_phase_receipts: ["forward", "pause", "reverse"],
    samples,
    registered_crops: registeredCrops,
    missing_registered_crop_ids: cropCapture.missing_crop_ids,
    maximum_dom_page_count: maximumDomPageCount,
    observed_forward_apex: {
      scheduled_offset_ms: forwardApexSample.scheduled_offset_ms,
      active_page: forwardApexSample.active_page,
      scroll_offset_css_px: forwardApexSample.scroll_offset_css_px,
    },
    registered_forward_crop_checkpoint_page:
      command.registered_crops.at(-1).page_number,
    finish_page: finalState.active_page,
  };
  const milestoneChecks = {
    "timestamped-native-input-complete":
      input.trajectory.length === command.expected_trajectory_sample_count &&
      heldDynamicTrajectoryPassed(input.trajectory, heldDistancePlan) &&
      timing.within_tolerance,
    "fixed-cadence-fidelity-samples-exact":
      samples.length === command.observer.expected_sample_count,
    "presented-screenshot-crops-three-matched": registeredCropsPassed,
    "presented-scale-comparability-proven":
      registeredCrops.length === 3 &&
      registeredCrops.every(
        (crop) =>
          crop.candidate_resampled === false &&
          crop.candidate_comparability?.zoom_percent === 100 &&
          crop.candidate_comparability?.display_scale_factor === 1 &&
          Math.abs(crop.candidate_comparability?.pixels_per_point?.x - 1) <=
            0.01 &&
          Math.abs(crop.candidate_comparability?.pixels_per_point?.y - 1) <=
            0.01,
      ),
    "checkpoint-holds-stable":
      heldDynamicTrajectoryPassed(input.trajectory, heldDistancePlan) &&
      registeredCrops.length === 3 &&
      registeredCrops.every(
        (crop) =>
          crop.presented_capture_correlation?.same_scroll_offset === true &&
          crop.presented_capture_correlation?.same_painted_bounds === true &&
          crop.presented_capture_correlation?.same_painted_render_generation ===
            true &&
          crop.presented_capture_correlation?.raster_ready_before_and_after ===
            true,
      ),
    "visible-page-ready-fraction-recorded": samples.every((sample) =>
      Number.isFinite(sample.visible_page_ready_fraction),
    ),
    "visible-raster-ready-area-fraction-recorded": samples.every((sample) =>
      Number.isFinite(sample.visible_raster_ready_area_fraction),
    ),
    "visible-raster-pixel-density-recorded": samples.every((sample) =>
      Number.isFinite(sample.visible_raster_pixel_density),
    ),
    "virtual-page-window-bounded": maximumDomPageCount < 526,
    "finish-page-current": finalState.active_page === command.path.finish_page,
  };
  const result = {
    command_id: command.id,
    observation: {
      trajectory_samples: input.trajectory,
      wheel_calibration: wheelCalibration,
      held_distance_plan: heldDistancePlan,
      state_observation_count: states.length,
      artifact_directory: artifactDirectory,
      semantic_summary: semanticSummary,
    },
    input: {
      input_lane: nativeX11InputLane,
      injection_api: "XTEST-direct-helper",
      replay: replay.metadata,
      actual_duration_ms: input.actual_duration_ms,
      timing,
    },
    observed_milestones: [],
    milestones: [],
    exact_fields: Object.fromEntries(
      command.expected_milestones.map((milestone) => [
        milestone,
        milestoneChecks[milestone] === true,
      ]),
    ),
  };
  for (const milestone of command.expected_milestones) {
    addMilestone(
      event,
      result,
      milestone,
      milestoneChecks[milestone] === true,
      semanticSummary,
    );
  }
  finalizeCommandMilestones(result, command);
  const exact =
    result.manifest_milestones_complete &&
    Object.values(result.exact_fields).every(Boolean);
  event("comparison-command-exact-state", {
    command_id: command.id,
    passed: exact,
    exact_fields: result.exact_fields,
  });
  event("electron-v5-native-dynamic-fidelity", {
    ...semanticSummary,
    samples: { retained_count: samples.length },
  });
  return {
    contract,
    input_lane: nativeX11InputLane,
    command_results: [result],
    semantic_summary: semanticSummary,
    exact_manifest_replay: exact,
    decision_timing_eligible: exact,
    blocker: exact
      ? null
      : "Electron dynamic fidelity did not pass exact native, crop, and presentation receipts",
  };
}

async function runElectronNativeAnnotationCreate(cdp, contract, event, target) {
  await nativeClickTestId(cdp, target, "viewer-fit-page");
  await cdp.evaluate(`(async () => {
    const deadline = performance.now() + 5000;
    while (performance.now() < deadline) {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      if (diagnostics.zoomPreset === 'fit-page' && diagnostics.pageRenderReady) {
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        return;
      }
      await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    throw new Error('Fit Page did not settle after native click');
  })()`);
  const disabledSnapSources = await disableSnappingWithNativeUi(cdp, target);
  const initial = await cdp.evaluate(
    `window.__butterPaperTestHooks.getActiveDocument()`,
  );
  const results = [];
  const aliasEntries = [];

  for (const command of contract.commands) {
    const before = await cdp.evaluate(
      `window.__butterPaperTestHooks.getActiveDocument()`,
    );
    const tool = command.id.startsWith("rectangle:")
      ? "rectangle"
      : "highlight";
    await nativeClickTestId(cdp, target, `tool-${tool}`);
    const activeTool = await cdp.evaluate(
      `window.__butterPaperTestHooks.getDiagnostics().activeTool`,
    );
    if (activeTool !== tool)
      throw new Error(
        `${command.id} native tool selection did not become active`,
      );
    const surface = await electronAnnotationSurface(cdp);
    const replay = buildPointerReplay({
      windowId: target.window_id,
      rateHz: command.pointer_path.rate_hz,
      durationMs: command.pointer_path.duration_ms,
      pdfSamples: manifestPointerSamples(command),
      surface,
      windowGeometry: target.geometry,
    });
    const actualDurationMs = await runDirectXTestPointer(replay, target);
    const timing = assessReplayTiming(replay, actualDurationMs);
    const documentModel = await waitForMarkupCount(
      cdp,
      before.markups.length + 1,
    );
    const markup = documentModel.markups.find(
      ({ id }) => !before.markups.some((existing) => existing.id === id),
    );
    if (!markup)
      throw new Error(
        `${command.id} did not create exactly one identifiable markup`,
      );
    aliasEntries.push({
      command_id: command.id,
      canonical_id: command.annotation_id,
      observed_id: markup.id,
    });
    const aliases = createBenchmarkAliasMap(aliasEntries);
    await cdp.evaluate(
      `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
    );
    const painted = await cdp.evaluate(
      `Boolean(document.querySelector(${JSON.stringify(`[data-testid="markup-${markup.id}"]`)}))`,
    );
    const samples = buildAnnotationPointerSamples(command);
    const correctness = annotationCorrectness(
      command,
      markup,
      samples,
      surface,
    );
    const result = {
      command_id: command.id,
      input: {
        input_lane: nativeX11InputLane,
        injection_api: "XTEST-direct-helper",
        replay: replay.metadata,
        actual_duration_ms: actualDurationMs,
        timing,
        coordinate_mapping: { page: 1, surface },
      },
      observed_markup_id: markup.id,
      requested_annotation_id: command.annotation_id,
      benchmark_alias: aliasEntries.at(-1),
      markup,
      correctness,
      observed_milestones: [],
      milestones: [],
      exact_fields: {
        pointer_path: timing.within_tolerance,
        annotation_id: aliases.observedId(command.annotation_id) === markup.id,
        geometry: correctness.geometry_matched,
        style: correctness.style_matched,
      },
      exact_field_blockers: [],
    };
    addMilestone(
      event,
      result,
      "pointer-stream-received",
      timing.within_tolerance,
      {
        input_lane: nativeX11InputLane,
        coordinate_sample_count: replay.metadata.coordinate_sample_count,
        timing,
      },
    );
    if (command.id === "highlight:create") {
      const smoothing = electronHighlightSmoothingEvidence(
        markup,
        samples.length,
        correctness.native_geometry,
      );
      addMilestone(event, result, "path-smoothed", smoothing.passed, {
        interpolation: command.pointer_path.interpolation,
        ...smoothing,
      });
    }
    addMilestone(
      event,
      result,
      "gesture-committed-once",
      documentModel.markups.length === before.markups.length + 1,
      {
        before_markup_count: before.markups.length,
        after_markup_count: documentModel.markups.length,
      },
    );
    addMilestone(event, result, "annotation-painted", painted, {
      markup_id: markup.id,
    });
    if (command.id === "rectangle:create-sparse") {
      const thumbnailPainted = await cdp.evaluate(`(async () => {
        const selector = ${JSON.stringify(`[data-testid="thumbnail-markup-${markup.id}"]`)};
        const deadline = performance.now() + 5000;
        while (performance.now() < deadline) {
          if (document.querySelector(selector)) return true;
          await new Promise((resolve) => requestAnimationFrame(resolve));
        }
        return false;
      })()`);
      addMilestone(event, result, "thumbnail-current", thumbnailPainted, {
        markup_id: markup.id,
      });
    }
    finalizeCommandMilestones(result, command);
    results.push(result);
  }

  const exact = results.every(
    ({ exact_fields, manifest_milestones_complete }) =>
      manifest_milestones_complete &&
      Object.values(exact_fields).every(Boolean),
  );
  return {
    contract,
    input_lane: nativeX11InputLane,
    target,
    command_results: results,
    initial_markup_count: initial.markups.length,
    final_markup_count: initial.markups.length + results.length,
    benchmark_alias_map: createBenchmarkAliasMap(aliasEntries).entries,
    style_contract_version: benchmarkStyleContractVersion,
    native_snap_sources_disabled: disabledSnapSources,
    exact_manifest_replay: exact,
    decision_timing_eligible: exact,
    blocker: exact
      ? null
      : "Native Electron replay did not satisfy every exact geometry, style, timing, and milestone check.",
  };
}

async function startContinuousScrollProbe(cdp) {
  await cdp.evaluate(`(() => {
    window.__electronContinuousProbeActive = true;
    window.__electronContinuousProbe = {
      frame_count: 0,
      blank_current_generation_frames: 0,
      max_mounted_pages: 0,
      max_visible_pages: 0,
      first_blank_frame: null,
    };
    const sample = () => {
      if (!window.__electronContinuousProbeActive) return;
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const mounted = document.querySelectorAll('[data-page-index]').length;
      const missingReadySurface = diagnostics.visiblePageIndices.filter((pageIndex) => {
        const page = document.querySelector('[data-testid="page-' + (pageIndex + 1) + '"]');
        return !page?.querySelector('[data-render-state="ready"]');
      });
      const probe = window.__electronContinuousProbe;
      probe.frame_count += 1;
      probe.max_mounted_pages = Math.max(probe.max_mounted_pages, mounted);
      probe.max_visible_pages = Math.max(probe.max_visible_pages, diagnostics.visiblePageIndices.length);
      if (missingReadySurface.length > 0) {
        probe.blank_current_generation_frames += 1;
        probe.first_blank_frame ??= { visible_page_indices: diagnostics.visiblePageIndices, missing_ready_surface_indices: missingReadySurface };
      }
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  })()`);
}

async function runElectronContinuousScroll(cdp, contract, event) {
  const command = contract.commands[0];
  const viewport = await cdp.evaluate(`(() => {
    const element = document.querySelector('[data-testid="document-viewport"]');
    if (!element) throw new Error('Document viewport is unavailable');
    const bounds = element.getBoundingClientRect();
    return { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2, height: bounds.height };
  })()`);
  const phases = buildContinuousScrollPhases(command, viewport.height);
  const dispatchStarted = performance.now();
  let acknowledgedEventCount = 0;
  await startContinuousScrollProbe(cdp);
  for (const phase of phases) {
    if (phase.name === "pause") {
      await delay(phase.duration_ms);
      continue;
    }
    const phaseStarted = performance.now();
    for (let index = 1; index <= phase.event_count; index += 1) {
      const waitMs =
        phaseStarted + index * phase.interval_ms - performance.now();
      if (waitMs > 0) await delay(waitMs);
      await cdp.send("Input.dispatchMouseEvent", {
        type: "mouseWheel",
        x: viewport.x,
        y: viewport.y,
        deltaX: 0,
        deltaY: phase.delta_y,
      });
      acknowledgedEventCount += 1;
      if (index % 120 === 0 || index === phase.event_count) {
        event("comparison-input-batch", {
          command_id: command.id,
          phase: phase.name,
          acknowledged_event_count: index,
        });
      }
    }
  }
  await delay(250);
  const observed = await cdp.evaluate(`(() => {
    window.__electronContinuousProbeActive = false;
    return {
      probe: window.__electronContinuousProbe,
      diagnostics: window.__butterPaperTestHooks.getDiagnostics(),
      app_perf: window.__butterPaperTestHooks.getPerfSnapshot(),
      mounted_page_count: document.querySelectorAll('[data-page-index]').length,
    };
  })()`);
  const result = {
    command_id: command.id,
    observed_milestones: [],
    milestones: [],
    input: {
      input_lane: "cdp-input-diagnostic",
      phases,
      acknowledged_event_count: acknowledgedEventCount,
      scheduled_duration_ms: phases.reduce(
        (sum, phase) => sum + phase.duration_ms,
        0,
      ),
      actual_duration_ms: performance.now() - dispatchStarted,
    },
    observed,
  };
  const expectedEventCount = phases.reduce(
    (sum, phase) => sum + phase.event_count,
    0,
  );
  const rateScheduleMet =
    result.input.actual_duration_ms <=
    result.input.scheduled_duration_ms * 1.05;
  addMilestone(
    event,
    result,
    "timestamped-input-complete",
    acknowledgedEventCount === expectedEventCount && rateScheduleMet,
    {
      expected_event_count: expectedEventCount,
      acknowledged_event_count: acknowledgedEventCount,
      rate_schedule_met: rateScheduleMet,
    },
  );
  addMilestone(
    event,
    result,
    "virtual-page-window-bounded",
    observed.probe.max_mounted_pages < observed.diagnostics.pageCount,
    {
      page_count: observed.diagnostics.pageCount,
      max_mounted_pages: observed.probe.max_mounted_pages,
    },
  );
  addMilestone(
    event,
    result,
    "finish-page-current",
    observed.diagnostics.currentPage === command.path.finish_page - 1,
    {
      expected_page: command.path.finish_page,
      observed_page: observed.diagnostics.currentPage + 1,
    },
  );
  addMilestone(
    event,
    result,
    "visible-raster-readiness-observed",
    observed.probe.frame_count > 0,
    {
      observation_basis:
        "visible-page-has-ready-raster-surface-on-animation-frame",
      frame_count: observed.probe.frame_count,
      missing_raster_observation_count:
        observed.probe.blank_current_generation_frames,
      readiness_rate:
        observed.probe.frame_count > 0
          ? (observed.probe.frame_count -
              observed.probe.blank_current_generation_frames) /
            observed.probe.frame_count
          : null,
      acceptance_role: "diagnostic-counts-and-rate",
      first_blank_frame: observed.probe.first_blank_frame,
    },
  );
  finalizeCommandMilestones(result, command);
  return {
    contract,
    input_lane: "cdp-input-diagnostic",
    command_results: [result],
    exact_manifest_replay: result.manifest_milestones_complete,
  };
}

async function runElectronNativeContinuousScroll(cdp, contract, event, target) {
  const command = contract.commands[0];
  const viewport = await elementGeometry(cdp, "document-viewport");
  const replay = buildWheelReplay({
    windowId: target.window_id,
    rateHz: command.input_rate_hz,
    forwardDurationMs: command.path.forward_duration_ms,
    pauseDurationMs: command.path.pause_duration_ms,
    reverseDurationMs: command.path.reverse_duration_ms,
    forwardViewportHeights: command.path.forward_viewport_heights,
    target: viewport.center,
  });
  await startContinuousScrollProbe(cdp);
  const actualDurationMs = await runDirectXTestWheel(replay, target);
  const timing = assessReplayTiming(replay, actualDurationMs);
  await delay(250);
  const observed = await cdp.evaluate(`(() => {
    window.__electronContinuousProbeActive = false;
    return {
      probe: window.__electronContinuousProbe,
      diagnostics: window.__butterPaperTestHooks.getDiagnostics(),
      app_perf: window.__butterPaperTestHooks.getPerfSnapshot(),
      mounted_page_count: document.querySelectorAll('[data-page-index]').length,
    };
  })()`);
  const result = {
    command_id: command.id,
    observed_milestones: [],
    milestones: [],
    input: {
      input_lane: nativeX11InputLane,
      injection_api: "XTEST-direct-helper",
      replay: replay.metadata,
      actual_duration_ms: actualDurationMs,
      timing,
      // XTest wheel buttons specify discrete events, not pixel deltas. The
      // frozen semantic distance and finish page remain correctness checks.
      semantic_distance_observation_required: true,
    },
    observed,
  };
  addMilestone(
    event,
    result,
    "timestamped-input-complete",
    timing.within_tolerance,
    {
      expected_event_count: replay.metadata.total_event_count,
      timing,
    },
  );
  addMilestone(
    event,
    result,
    "virtual-page-window-bounded",
    observed.probe.max_mounted_pages < observed.diagnostics.pageCount,
    {
      page_count: observed.diagnostics.pageCount,
      max_mounted_pages: observed.probe.max_mounted_pages,
    },
  );
  addMilestone(
    event,
    result,
    "finish-page-current",
    observed.diagnostics.currentPage === command.path.finish_page - 1,
    {
      expected_page: command.path.finish_page,
      observed_page: observed.diagnostics.currentPage + 1,
    },
  );
  addMilestone(
    event,
    result,
    "visible-raster-readiness-observed",
    observed.probe.frame_count > 0,
    {
      observation_basis:
        "visible-page-has-ready-raster-surface-on-animation-frame",
      frame_count: observed.probe.frame_count,
      missing_raster_observation_count:
        observed.probe.blank_current_generation_frames,
      readiness_rate:
        observed.probe.frame_count > 0
          ? (observed.probe.frame_count -
              observed.probe.blank_current_generation_frames) /
            observed.probe.frame_count
          : null,
      acceptance_role: "diagnostic-counts-and-rate",
      first_blank_frame: observed.probe.first_blank_frame,
    },
  );
  finalizeCommandMilestones(result, command);
  const exact = result.manifest_milestones_complete && timing.within_tolerance;
  return {
    contract,
    input_lane: nativeX11InputLane,
    target,
    command_results: [result],
    exact_manifest_replay: exact,
    decision_timing_eligible: exact,
    blocker: exact
      ? null
      : "Native Electron scroll replay did not satisfy every exact timing and semantic milestone.",
  };
}

async function runElectronViewerLayout(
  cdp,
  contract,
  event,
  nativeTarget = null,
) {
  const results = [];
  for (const command of contract.commands) {
    const singlePage = command.layout === "single-page";
    const controlTestId = singlePage
      ? "viewer-scroll-single-page"
      : "viewer-scroll-continuous";
    if (nativeTarget) await nativeClickTestId(cdp, nativeTarget, controlTestId);
    else await dispatchCdpClick(cdp, controlTestId);
    const observation = await cdp.evaluate(`(async () => {
      const expectedMode = ${JSON.stringify(command.layout)};
      const deadline = performance.now() + 10000;
      while (performance.now() < deadline) {
        const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
        const surfaces = [...document.querySelectorAll('[data-page-index][data-current-page]')];
        const ready = surfaces.length > 0 && surfaces.every((page) =>
          page.querySelector('[data-render-state="ready"]'));
        if (diagnostics.scrollMode === expectedMode && diagnostics.pageRenderReady && ready) {
          await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
          await new Promise((resolve) => setTimeout(resolve, 250));
          const documentModel = window.__butterPaperTestHooks.getActiveDocument();
          const settledDiagnostics = window.__butterPaperTestHooks.getDiagnostics();
          const geometry = [...document.querySelectorAll('[data-page-index][data-current-page]')].map((page) => {
            const pageIndex = Number(page.getAttribute('data-page-index'));
            const modelPage = documentModel?.pages?.[pageIndex];
            const bounds = page.getBoundingClientRect();
            return {
              page_index: pageIndex,
              expected_width: modelPage?.size?.width ?? 0,
              expected_height: modelPage?.size?.height ?? 0,
              observed_width: bounds.width,
              observed_height: bounds.height,
            };
          });
          const pageOneMarkups = (documentModel?.markups ?? []).filter((markup) => markup.pageIndex === 0);
          const thumbnail = document.querySelector('[data-testid="page-thumbnail-content-1"]');
          const thumbnailMarkupCount = thumbnail
            ? thumbnail.querySelectorAll('[data-testid^="thumbnail-markup-"]').length
            : -1;
          const thumbnailRasterReady = Boolean(thumbnail?.querySelector('canvas, img'));
          return {
            diagnostics: settledDiagnostics,
            geometry,
            presentation_current: ready && geometry.length > 0,
            mounted_page_count: geometry.length,
            page_count: documentModel?.pages?.length ?? 0,
            thumbnail: {
              expected_annotation_count: pageOneMarkups.length,
              observed_annotation_count: thumbnailMarkupCount,
              raster_ready: thumbnailRasterReady,
              diagnostics_ready: settledDiagnostics.thumbnailRenderReady === true,
            },
          };
        }
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
      throw new Error('Layout ' + expectedMode + ' did not settle with ready visible page surfaces');
    })()`);
    const geometry = evaluateVisiblePageGeometry(observation.geometry);
    const annotationThumbnailCurrent =
      observation.thumbnail.raster_ready &&
      observation.thumbnail.diagnostics_ready &&
      observation.thumbnail.observed_annotation_count ===
        observation.thumbnail.expected_annotation_count;
    const result = {
      command_id: command.id,
      observed_milestones: [],
      milestones: [],
      observation: { ...observation, geometry },
      exact_fields: nativeTarget
        ? { presentation_current: observation.presentation_current === true }
        : {},
    };
    addMilestone(
      event,
      result,
      "per-page-geometry-matched",
      geometry.passed,
      geometry,
    );
    if (singlePage) {
      addMilestone(
        event,
        result,
        "annotation-thumbnail-current",
        annotationThumbnailCurrent,
        {
          ...observation.thumbnail,
          observation_basis:
            "page-1 document annotation count equals the visible page-1 thumbnail annotation layer count after its raster settles",
        },
      );
    } else {
      addMilestone(
        event,
        result,
        "virtual-page-window-bounded",
        observation.mounted_page_count < observation.page_count,
        {
          mounted_page_count: observation.mounted_page_count,
          page_count: observation.page_count,
        },
      );
    }
    finalizeCommandMilestones(result, command);
    results.push(result);
  }
  const exact = results.every(
    ({ manifest_milestones_complete, exact_fields: exactFields }) =>
      manifest_milestones_complete && Object.values(exactFields).every(Boolean),
  );
  return {
    contract,
    input_lane: nativeTarget ? nativeX11InputLane : cdpDiagnosticInputLane,
    ...(nativeTarget ? { target: nativeTarget } : {}),
    command_results: results,
    exact_manifest_replay: exact,
    decision_timing_eligible: nativeTarget ? exact : false,
    blocker: exact
      ? null
      : "Electron viewer layout did not satisfy exact geometry, virtualization, thumbnail, and presentation checks.",
  };
}

async function runElectronNativePageNavigation(cdp, contract, event, target) {
  const command = contract.commands[0];
  const pageCount = await cdp.evaluate(
    `window.__butterPaperTestHooks.getDiagnostics().pageCount`,
  );
  const pageSequence = normalizedPageSequence(pageCount);
  const observations = [];
  for (const pageNumber of pageSequence) {
    const track = await elementGeometry(cdp, "page-thumbnail-scrollbar-track");
    const thumb = await elementGeometry(cdp, "page-thumbnail-scrollbar-thumb");
    const replay = buildElectronNativePageNavigationReplay({
      windowId: target.window_id,
      pageNumber,
      pageCount,
      track: track.bounds,
      thumb: thumb.bounds,
      logicalSize: track.window_logical_size,
      windowGeometry: target.geometry,
    });
    const started = performance.now();
    await runXdotool(replay.args, 0);
    await cdp.evaluate(`(async () => {
      const deadline = performance.now() + 5000;
      while (performance.now() < deadline) {
        const button = document.querySelector('[data-testid="page-thumbnail-select-${pageNumber}"]');
        const bounds = button?.getBoundingClientRect();
        if (bounds && bounds.width > 0 && bounds.height > 0) return;
        await new Promise((resolve) => requestAnimationFrame(resolve));
      }
      throw new Error('Native thumbnail scrollbar did not expose page ${pageNumber}');
    })()`);
    const selection = await nativeClickTestId(
      cdp,
      target,
      `page-thumbnail-select-${pageNumber}`,
    );
    const observation = await cdp.evaluate(`(async () => {
      const pageNumber = ${pageNumber};
      const deadline = performance.now() + 20000;
      while (performance.now() < deadline) {
        const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
        const perf = window.__butterPaperTestHooks.getPerfSnapshot();
        const raster = perf.renderPage;
        const visibility = perf.pageImageVisibility[String(pageNumber - 1)];
        const density = (visibility?.bestRenderedWidthRatio ?? 0) / window.devicePixelRatio;
        const rasterResolved = raster.completed + raster.hits > 0 || density >= 0.75;
        if (diagnostics.currentPage === pageNumber - 1 && rasterResolved && density >= 0.75) {
          await new Promise((resolve) => setTimeout(resolve, 250));
          await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
          const settledDiagnostics = window.__butterPaperTestHooks.getDiagnostics();
          const current = document.querySelector('[data-testid="page-' + pageNumber + '"]');
          const surfaces = [...(current?.querySelectorAll('[data-render-state="ready"]') ?? [])];
          return {
            diagnostics: settledDiagnostics,
            render_page: raster,
            image_visibility: visibility ?? null,
            rendered_device_pixel_ratio: density,
            display_scale_factor: window.devicePixelRatio,
            target_page_current: settledDiagnostics.currentPage === pageNumber - 1,
            preview_current_generation: surfaces.length > 0
              && surfaces.every((surface) => surface.getAttribute('data-render-quality') !== 'stale-preview'),
            settled_current_generation_250ms: true,
          };
        }
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
      throw new Error('Page ${pageNumber} did not produce an acceptable current preview after native selection');
    })()`);
    const record = {
      page_number: pageNumber,
      input: { scrollbar_drag: replay, thumbnail_selection: selection },
      observation,
      duration_ms: performance.now() - started,
    };
    event("page-navigation-completed", {
      input_lane: nativeX11InputLane,
      page_number: pageNumber,
      duration_ms: record.duration_ms,
      visible_page_indices: observation.diagnostics.visiblePageIndices,
      rendered_device_pixel_ratio: observation.rendered_device_pixel_ratio,
      target_page_current: observation.target_page_current,
      preview_current_generation: observation.preview_current_generation,
      settled_current_generation_250ms:
        observation.settled_current_generation_250ms,
    });
    observations.push(record);
  }
  const checks = {
    "target-page-current":
      observations.length === pageSequence.length &&
      observations.every(({ observation }) => observation.target_page_current),
    "preview-current-generation":
      observations.length === pageSequence.length &&
      observations.every(
        ({ observation }) => observation.preview_current_generation,
      ),
    "settled-current-generation-250ms":
      observations.length === pageSequence.length &&
      observations.every(
        ({ observation }) => observation.settled_current_generation_250ms,
      ),
  };
  const result = {
    command_id: command.id,
    observed_milestones: [],
    milestones: [],
    input: {
      input_lane: nativeX11InputLane,
      injection_api: "XTEST-via-xdotool",
    },
    observations,
    exact_fields: {
      page_sequence:
        observations.map(({ page_number: value }) => value).join(",") ===
        pageSequence.join(","),
      presentation_current: checks["preview-current-generation"],
    },
  };
  for (const milestone of command.expected_milestones) {
    addMilestone(event, result, milestone, checks[milestone] === true, {
      page_sequence: observations,
    });
  }
  finalizeCommandMilestones(result, command);
  const exact =
    result.manifest_milestones_complete &&
    Object.values(result.exact_fields).every(Boolean);
  return {
    contract,
    input_lane: nativeX11InputLane,
    target,
    command_results: [result],
    exact_manifest_replay: exact,
    decision_timing_eligible: exact,
    blocker: exact
      ? null
      : "Native Electron page navigation did not satisfy exact sequence, preview, settle, and presentation checks.",
  };
}

async function runElectronNativeZoom(cdp, contract, event, target) {
  const command = contract.commands[0];
  const zoomResults = [];
  await cdp.evaluate(`(() => {
    window.__electronZoomProbeActive = true;
    window.__electronZoomProbe = {
      frame_count: 0,
      stale_visible_surface_frames: 0,
      first_stale_frame: null,
    };
    const sample = () => {
      if (!window.__electronZoomProbeActive) return;
      const visible = [...document.querySelectorAll('[data-render-state="ready"]')].filter((surface) => {
        const bounds = surface.getBoundingClientRect();
        return bounds.right > 0 && bounds.bottom > 0
          && bounds.left < innerWidth && bounds.top < innerHeight;
      });
      const stale = visible.filter((surface) =>
        surface.getAttribute('data-render-quality') === 'stale-preview');
      const probe = window.__electronZoomProbe;
      probe.frame_count += 1;
      if (stale.length > 0) {
        probe.stale_visible_surface_frames += 1;
        probe.first_stale_frame ??= stale.map((surface) => ({
          page_index: Number(surface.closest('[data-page-index]')?.getAttribute('data-page-index')),
          quality: surface.getAttribute('data-render-quality'),
        }));
      }
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  })()`);
  for (const percent of zoomSequence) {
    const before = await cdp.evaluate(`(() => {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const visibility = window.__butterPaperTestHooks.getPerfSnapshot()
        .pageImageVisibility[String(diagnostics.currentPage)];
      return { zoom: diagnostics.zoom, visibility: visibility ?? null };
    })()`);
    await nativeClickTestId(cdp, target, "viewer-zoom-menu");
    await cdp.evaluate(`(async () => {
      const deadline = performance.now() + 5000;
      while (performance.now() < deadline) {
        const preset = document.querySelector('[data-testid="viewer-zoom-preset-${percent}"]');
        const bounds = preset?.getBoundingClientRect();
        if (bounds && bounds.width > 0 && bounds.height > 0) return;
        await new Promise((resolve) => requestAnimationFrame(resolve));
      }
      throw new Error('Zoom preset ${percent}% did not become visible after native input');
    })()`);
    const started = performance.now();
    const input = await nativeClickTestId(
      cdp,
      target,
      `viewer-zoom-preset-${percent}`,
    );
    const result = await cdp.evaluate(`(async () => {
      const zoom = ${percent / 100};
      const noOp = Math.abs(${before.zoom} - zoom) < 0.0001;
      const deadline = performance.now() + 15000;
      while (performance.now() < deadline) {
        const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
        const perf = window.__butterPaperTestHooks.getPerfSnapshot();
        const visibility = noOp
          ? ${JSON.stringify(before.visibility)}
          : perf.pageImageVisibility[String(diagnostics.currentPage)];
        const density = (visibility?.bestRenderedWidthRatio ?? 0) / window.devicePixelRatio;
        if (Math.abs(diagnostics.zoom - zoom) < 0.0001 && density >= ${electronSettledDensityMinimum}) {
          await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
          await new Promise((resolve) => setTimeout(resolve, 250));
          const settledDiagnostics = window.__butterPaperTestHooks.getDiagnostics();
          const current = document.querySelector('[data-testid="page-' + (settledDiagnostics.currentPage + 1) + '"]');
          const readySurfaces = [...(current?.querySelectorAll('[data-render-state="ready"]') ?? [])];
          const visibleCanvases = [...document.querySelectorAll('canvas')].filter((canvas) => {
            const bounds = canvas.getBoundingClientRect();
            return bounds.right > 0 && bounds.bottom > 0
              && bounds.left < innerWidth && bounds.top < innerHeight;
          });
          return {
            diagnostics: settledDiagnostics,
            no_op: noOp,
            image_visibility: visibility ?? null,
            rendered_device_pixel_ratio: density,
            display_scale_factor: window.devicePixelRatio,
            visible_raster_resources: {
              count: visibleCanvases.length,
              max_pixels: Math.max(0, ...visibleCanvases.map((canvas) => canvas.width * canvas.height)),
            },
            presentation_current: current !== null && readySurfaces.length > 0
              && readySurfaces.every((surface) => surface.getAttribute('data-render-quality') !== 'stale-preview'),
          };
        }
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
      throw new Error('Native zoom ${percent}% did not settle');
    })()`);
    event("zoom-completed", {
      duration_ms: performance.now() - started,
      zoom_percent: percent,
      visible_page_indices: result.diagnostics.visiblePageIndices,
      no_op: result.no_op,
      completion_basis:
        "native-XTEST-preset-plus-zoom-state-current-raster-two-animation-frames-and-250ms-settle",
      image_visibility: result.image_visibility,
      display_scale_factor: result.display_scale_factor,
      rendered_device_pixel_ratio: result.rendered_device_pixel_ratio,
      visible_raster_resources: result.visible_raster_resources,
      presentation_current: result.presentation_current,
    });
    zoomResults.push({ expected_percent: percent, input, ...result });
  }
  const zoomProbe = await cdp.evaluate(`(() => {
    window.__electronZoomProbeActive = false;
    return structuredClone(window.__electronZoomProbe);
  })()`);
  const checks = {
    "zoom-state-current":
      zoomResults.length === zoomSequence.length &&
      zoomResults.every(
        ({ expected_percent: percent, diagnostics }) =>
          Math.abs(diagnostics.zoom - percent / 100) < 0.0001,
      ),
    "visible-tiles-bounded": zoomResults.every(
      ({ visible_raster_resources: resources }) =>
        resources.count <= 32 && resources.max_pixels <= 8192 * 8192,
    ),
    "preview-current-generation": zoomResults.every(
      ({ presentation_current: current }) => current === true,
    ),
    "settled-density-at-least-1": zoomResults.every(
      ({ rendered_device_pixel_ratio: ratio }) =>
        electronSettledDensityIsCurrent(ratio),
    ),
    "stale-generations-presented-zero":
      zoomProbe.frame_count > 0 && zoomProbe.stale_visible_surface_frames === 0,
  };
  const result = {
    command_id: command.id,
    observed_milestones: [],
    milestones: [],
    input: {
      input_lane: nativeX11InputLane,
      injection_api: "XTEST-via-xdotool",
    },
    observations: { zoom_results: zoomResults, generation_probe: zoomProbe },
    exact_fields: {
      presentation_current: zoomResults.every(
        ({ presentation_current: current }) => current === true,
      ),
      stale_generations_presented_zero:
        checks["stale-generations-presented-zero"],
    },
  };
  for (const milestone of command.expected_milestones) {
    addMilestone(event, result, milestone, checks[milestone] === true, {
      zoom_results: zoomResults,
      generation_probe: zoomProbe,
    });
  }
  finalizeCommandMilestones(result, command);
  const exact =
    result.manifest_milestones_complete &&
    Object.values(result.exact_fields).every(Boolean);
  return {
    contract,
    input_lane: nativeX11InputLane,
    target,
    command_results: [result],
    exact_manifest_replay: exact,
    decision_timing_eligible: exact,
    blocker: exact
      ? null
      : "Native Electron zoom sequence did not satisfy exact state, raster bounds, density, and presentation checks.",
  };
}

async function runElectronHighZoomPan(cdp, contract, event) {
  const command = contract.commands[0];
  await cdp.evaluate(`(() => {
    window.__butterPaperTestHooks.resetPerfSnapshot();
    window.__butterPaperTestHooks.setZoom(${command.zoom_percent / 100});
  })()`);
  const viewport = await cdp.evaluate(`(async () => {
    const deadline = performance.now() + 5000;
    while (performance.now() < deadline) {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const perf = window.__butterPaperTestHooks.getPerfSnapshot();
      const visibility = perf.pageImageVisibility[String(diagnostics.currentPage)];
      const element = document.querySelector('[data-testid="document-viewport"]');
      const current = document.querySelector('[data-testid="page-' + (diagnostics.currentPage + 1) + '"]');
      const density = element
        ? (${measureElectronVisibleRasterDensity.toString()})(
            current,
            element.getBoundingClientRect(),
            window.devicePixelRatio,
          )
        : 0;
      const ready = [...(current?.querySelectorAll('[data-render-state="ready"]') ?? [])];
      if (element && diagnostics.zoom === ${command.zoom_percent / 100}
        && diagnostics.pageRenderReady === true
        && density >= ${electronSettledDensityMinimum}
        && ready.length > 0
        && ready.every((surface) => surface.getAttribute('data-render-quality') !== 'stale-preview')) {
        const bounds = element.getBoundingClientRect();
        window.__electronPanProbeActive = true;
        window.__electronPanProbe = {
          frame_count: 0,
          stale_visible_surface_frames: 0,
          max_visible_raster_count: 0,
          max_visible_raster_pixels: 0,
          first_stale_frame: null,
        };
        const sample = () => {
          if (!window.__electronPanProbeActive) return;
          const visible = [...document.querySelectorAll('[data-render-state]')].filter((surface) => {
            const rect = surface.getBoundingClientRect();
            return rect.right > bounds.left && rect.bottom > bounds.top
              && rect.left < bounds.right && rect.top < bounds.bottom;
          });
          const stale = visible.filter((surface) => surface.getAttribute('data-render-quality') === 'stale-preview');
          const canvases = visible.filter((surface) => surface instanceof HTMLCanvasElement);
          const probe = window.__electronPanProbe;
          probe.frame_count += 1;
          probe.max_visible_raster_count = Math.max(probe.max_visible_raster_count, canvases.length);
          probe.max_visible_raster_pixels = Math.max(probe.max_visible_raster_pixels,
            ...canvases.map((canvas) => canvas.width * canvas.height), 0);
          if (stale.length > 0) {
            probe.stale_visible_surface_frames += 1;
            probe.first_stale_frame ??= stale.map((surface) => ({
              page_index: Number(surface.closest('[data-page-index]')?.getAttribute('data-page-index')),
              quality: surface.getAttribute('data-render-quality'),
            }));
          }
          requestAnimationFrame(sample);
        };
        requestAnimationFrame(sample);
        return {
          x: bounds.left,
          y: bounds.top,
          width: bounds.width,
          height: bounds.height,
          scroll_left: element.scrollLeft,
          scroll_top: element.scrollTop,
          settled_density: density,
          promotion_timed_out: false,
        };
      }
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
    const element = document.querySelector('[data-testid="document-viewport"]');
    const current = document.querySelector('[data-testid="page-' + (diagnostics.currentPage + 1) + '"]');
    if (!element) throw new Error('Document viewport is unavailable before pan replay');
    const bounds = element.getBoundingClientRect();
    return {
      x: bounds.left,
      y: bounds.top,
      width: bounds.width,
      height: bounds.height,
      scroll_left: element.scrollLeft,
      scroll_top: element.scrollTop,
      settled_density: (${measureElectronVisibleRasterDensity.toString()})(
        current,
        bounds,
        window.devicePixelRatio,
      ),
      promotion_timed_out: true,
    };
  })()`);
  const samples = buildElectronSemanticPanFrames(command, viewport, {
    left: viewport.scroll_left,
    top: viewport.scroll_top,
  });
  const replay = await cdp.evaluate(`(async () => {
    const element = document.querySelector('[data-testid="document-viewport"]');
    if (!element) throw new Error('Document viewport disappeared before semantic pan replay');
    const samples = ${JSON.stringify(samples)};
    const before = { left: element.scrollLeft, top: element.scrollTop };
    const started = performance.now();
    let maximumAbsoluteScheduleErrorMs = 0;
    for (const sample of samples) {
      const waitMs = started + sample.t_ms - performance.now();
      if (waitMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, waitMs));
      }
      element.scrollTo({
        left: sample.scroll_left,
        top: sample.scroll_top,
        behavior: 'auto',
      });
      maximumAbsoluteScheduleErrorMs = Math.max(
        maximumAbsoluteScheduleErrorMs,
        Math.abs((performance.now() - started) - sample.t_ms),
      );
    }
    return {
      acknowledged_sample_count: samples.length,
      actual_duration_ms: performance.now() - started,
      maximum_absolute_schedule_error_ms: maximumAbsoluteScheduleErrorMs,
      before,
      after: { left: element.scrollLeft, top: element.scrollTop },
    };
  })()`);
  for (
    let index = command.rate_hz;
    index < samples.length;
    index += command.rate_hz
  ) {
    event("comparison-input-batch", {
      command_id: command.id,
      acknowledged_sample_index: Math.min(index, samples.length - 1),
    });
  }
  if ((samples.length - 1) % command.rate_hz !== 0) {
    event("comparison-input-batch", {
      command_id: command.id,
      acknowledged_sample_index: samples.length - 1,
    });
  }
  const observed = await cdp.evaluate(`(async () => {
    const deadline = performance.now() + 5000;
    await new Promise((resolve) => setTimeout(resolve, 250));
    while (performance.now() < deadline) {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const perf = window.__butterPaperTestHooks.getPerfSnapshot();
      const visibility = perf.pageImageVisibility[String(diagnostics.currentPage)];
      const viewport = document.querySelector('[data-testid="document-viewport"]');
      const current = document.querySelector('[data-testid="page-' + (diagnostics.currentPage + 1) + '"]');
      const density = viewport
        ? (${measureElectronVisibleRasterDensity.toString()})(
            current,
            viewport.getBoundingClientRect(),
            window.devicePixelRatio,
          )
        : 0;
      const ready = [...(current?.querySelectorAll('[data-render-state="ready"]') ?? [])];
      if (density >= ${electronSettledDensityMinimum}
        && ready.length > 0
        && ready.every((surface) => surface.getAttribute('data-render-quality') !== 'stale-preview')) {
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        window.__electronPanProbeActive = false;
        return {
          probe: structuredClone(window.__electronPanProbe),
          diagnostics,
          settled_density: density,
          image_visibility: visibility ?? null,
          promotion_timed_out: false,
        };
      }
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    window.__electronPanProbeActive = false;
    const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
    const perf = window.__butterPaperTestHooks.getPerfSnapshot();
    const visibility = perf.pageImageVisibility[String(diagnostics.currentPage)];
    const viewport = document.querySelector('[data-testid="document-viewport"]');
    const current = document.querySelector('[data-testid="page-' + (diagnostics.currentPage + 1) + '"]');
    return {
      probe: structuredClone(window.__electronPanProbe),
      diagnostics,
      settled_density: viewport
        ? (${measureElectronVisibleRasterDensity.toString()})(
            current,
            viewport.getBoundingClientRect(),
            window.devicePixelRatio,
          )
        : 0,
      image_visibility: visibility ?? null,
      promotion_timed_out: true,
    };
  })()`);
  observed.pre_pan_promotion = {
    settled_density: viewport.settled_density,
    promotion_timed_out: viewport.promotion_timed_out,
  };
  const checks = assessHighZoomPanEvidence(observed);
  const result = {
    command_id: command.id,
    observed_milestones: [],
    milestones: [],
    input: {
      input_lane: "cdp-input-diagnostic",
      injection_api: "renderer-scroll-state-semantic-diagnostic",
      expected_sample_count: samples.length,
      acknowledged_sample_count: replay.acknowledged_sample_count,
      scheduled_duration_ms: command.duration_ms,
      actual_duration_ms: replay.actual_duration_ms,
      maximum_absolute_schedule_error_ms:
        replay.maximum_absolute_schedule_error_ms,
      scroll: { before: replay.before, after: replay.after },
      rate_schedule_met:
        replay.acknowledged_sample_count === samples.length &&
        replay.actual_duration_ms <= command.duration_ms * 1.05,
    },
    observed,
    exact_fields: {
      timing:
        samples.length > 0 &&
        replay.acknowledged_sample_count === samples.length &&
        replay.actual_duration_ms <= command.duration_ms * 1.05,
    },
  };
  addMilestone(
    event,
    result,
    "visible-tiles-bounded",
    checks.visible_tiles_bounded,
    observed.probe,
  );
  addMilestone(
    event,
    result,
    "stale-generations-presented-zero",
    checks.stale_generations_presented_zero,
    {
      observation_basis:
        "visible DOM raster surfaces sampled on every animation frame",
      ...observed.probe,
    },
  );
  addMilestone(
    event,
    result,
    "settled-density-at-least-1",
    checks.settled_density_at_least_1,
    {
      settled_density: observed.settled_density,
      image_visibility: observed.image_visibility,
    },
  );
  finalizeCommandMilestones(result, command);
  return {
    contract,
    input_lane: "cdp-input-diagnostic",
    command_results: [result],
    exact_manifest_replay:
      result.manifest_milestones_complete && result.input.rate_schedule_met,
  };
}

async function runElectronNativeHighZoomPan(cdp, contract, event, target) {
  const command = contract.commands[0];
  await nativeClickTestId(cdp, target, "viewer-zoom-menu");
  await cdp.evaluate(`(async () => {
    const deadline = performance.now() + 5000;
    while (performance.now() < deadline) {
      const preset = document.querySelector('[data-testid="viewer-zoom-preset-${command.zoom_percent}"]');
      const bounds = preset?.getBoundingClientRect();
      if (bounds && bounds.width > 0 && bounds.height > 0) return;
      await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    throw new Error('${command.zoom_percent}% zoom preset did not become visible after native input');
  })()`);
  await nativeClickTestId(
    cdp,
    target,
    `viewer-zoom-preset-${command.zoom_percent}`,
  );
  const viewport = await cdp.evaluate(`(async () => {
    const deadline = performance.now() + 20000;
    while (performance.now() < deadline) {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const perf = window.__butterPaperTestHooks.getPerfSnapshot();
      const visibility = perf.pageImageVisibility[String(diagnostics.currentPage)];
      const density = (visibility?.bestRenderedWidthRatio ?? 0) / window.devicePixelRatio;
      const element = document.querySelector('[data-testid="document-viewport"]');
      if (element && diagnostics.zoom === ${command.zoom_percent / 100} && density >= ${electronSettledDensityMinimum}) {
        const bounds = element.getBoundingClientRect();
        window.__electronPanProbeActive = true;
        window.__electronPanProbe = {
          frame_count: 0,
          stale_visible_surface_frames: 0,
          max_visible_raster_count: 0,
          max_visible_raster_pixels: 0,
          first_stale_frame: null,
        };
        const sample = () => {
          if (!window.__electronPanProbeActive) return;
          const visible = [...document.querySelectorAll('[data-render-state]')].filter((surface) => {
            const rect = surface.getBoundingClientRect();
            return rect.right > bounds.left && rect.bottom > bounds.top
              && rect.left < bounds.right && rect.top < bounds.bottom;
          });
          const stale = visible.filter((surface) => surface.getAttribute('data-render-quality') === 'stale-preview');
          const canvases = visible.filter((surface) => surface instanceof HTMLCanvasElement);
          const probe = window.__electronPanProbe;
          probe.frame_count += 1;
          probe.max_visible_raster_count = Math.max(probe.max_visible_raster_count, canvases.length);
          probe.max_visible_raster_pixels = Math.max(probe.max_visible_raster_pixels,
            ...canvases.map((canvas) => canvas.width * canvas.height), 0);
          if (stale.length > 0) {
            probe.stale_visible_surface_frames += 1;
            probe.first_stale_frame ??= stale.map((surface) => ({
              page_index: Number(surface.closest('[data-page-index]')?.getAttribute('data-page-index')),
              quality: surface.getAttribute('data-render-quality'),
            }));
          }
          requestAnimationFrame(sample);
        };
        requestAnimationFrame(sample);
        return {
          bounds: { x: bounds.left, y: bounds.top, width: bounds.width, height: bounds.height },
          window_logical_size: { width: window.innerWidth, height: window.innerHeight },
        };
      }
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    throw new Error('${command.zoom_percent}% page raster did not settle before native pan replay');
  })()`);
  const replay = buildElectronNativePanReplay(command, viewport, target);
  const actualDurationMs = await runDirectXTestPointer(replay, target);
  const timing = assessReplayTiming(replay, actualDurationMs);
  event("comparison-input-batch", {
    command_id: command.id,
    acknowledged_sample_index: replay.pixel_samples.length - 1,
    input_lane: nativeX11InputLane,
  });
  const observed = await cdp.evaluate(`(async () => {
    await new Promise((resolve) => setTimeout(resolve, 250));
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    window.__electronPanProbeActive = false;
    const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
    const perf = window.__butterPaperTestHooks.getPerfSnapshot();
    const visibility = perf.pageImageVisibility[String(diagnostics.currentPage)];
    const current = document.querySelector('[data-testid="page-' + (diagnostics.currentPage + 1) + '"]');
    const readySurfaces = [...(current?.querySelectorAll('[data-render-state="ready"]') ?? [])];
    return {
      probe: window.__electronPanProbe,
      diagnostics,
      settled_density: (visibility?.bestRenderedWidthRatio ?? 0) / window.devicePixelRatio,
      image_visibility: visibility ?? null,
      presentation_current: current !== null && readySurfaces.length > 0
        && readySurfaces.every((surface) => surface.getAttribute('data-render-quality') !== 'stale-preview'),
    };
  })()`);
  const evidence = assessElectronNativePanEvidence({
    ...observed,
    expected_sample_count: buildNormalizedPointerSamples(command).length,
    acknowledged_sample_count: replay.pixel_samples.length,
    timing_within_tolerance: timing.within_tolerance,
  });
  const result = {
    command_id: command.id,
    observed_milestones: [],
    milestones: [],
    input: {
      input_lane: nativeX11InputLane,
      injection_api: "XTEST-direct-helper",
      expected_sample_count: buildNormalizedPointerSamples(command).length,
      acknowledged_sample_count: replay.pixel_samples.length,
      scheduled_duration_ms: command.duration_ms,
      actual_duration_ms: actualDurationMs,
      timing,
      button: replay.metadata.button,
    },
    observed,
    exact_fields: evidence.exact_fields,
  };
  addMilestone(
    event,
    result,
    "visible-tiles-bounded",
    evidence.exact_fields.visible_tiles_bounded,
    observed.probe,
  );
  addMilestone(
    event,
    result,
    "stale-generations-presented-zero",
    evidence.exact_fields.stale_generations_presented_zero,
    {
      observation_basis:
        "visible DOM raster surfaces sampled on every animation frame during direct XTEST middle-button pan",
      ...observed.probe,
    },
  );
  addMilestone(
    event,
    result,
    "settled-density-at-least-1",
    evidence.exact_fields.settled_density_at_least_1,
    {
      settled_density: observed.settled_density,
      image_visibility: observed.image_visibility,
    },
  );
  finalizeCommandMilestones(result, command);
  const exact = evidence.passed && result.manifest_milestones_complete;
  return {
    contract,
    input_lane: nativeX11InputLane,
    target,
    command_results: [result],
    exact_manifest_replay: exact,
    decision_timing_eligible: exact,
    blocker: exact
      ? null
      : "Native Electron high-zoom pan did not satisfy exact sample, timing, presentation, density, and generation checks.",
  };
}

async function runElectronCloseReopen(cdp, contract, event, pdfPath) {
  const command = contract.commands[0];
  const beforeClose = await cdp.evaluate(
    `window.butterPaper.test.getProcessMetrics()`,
  );
  await cdp.evaluate(`window.__butterPaperTestHooks.closeTab(0)`);
  const closed = await cdp.evaluate(`(async () => {
    const deadline = performance.now() + 10000;
    while (performance.now() < deadline) {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      if (!window.__butterPaperTestHooks.getActiveDocument() && (diagnostics.tabs?.length ?? 0) === 0) {
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        return {
          document_path: diagnostics.documentPath,
          tab_count: diagnostics.tabs?.length ?? 0,
          render_cache_bytes: diagnostics.renderCacheBytes,
          thumbnail_cache_bytes: diagnostics.thumbnailCacheBytes ?? 0,
          document_canvas_count: document.querySelectorAll('[data-testid="document-viewport"] canvas').length,
        };
      }
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    throw new Error('Document resources did not become observably released after close');
  })()`);
  await cdp.send("HeapProfiler.collectGarbage");
  const afterClose = await cdp.evaluate(
    `window.butterPaper.test.getProcessMetrics()`,
  );
  await cdp.evaluate(
    `window.__butterPaperTestHooks.openDocumentPath(${JSON.stringify(pdfPath)})`,
  );
  const reopened = await cdp.evaluate(`(async () => {
    const expectedPath = ${JSON.stringify(pdfPath)};
    const deadline = performance.now() + 20000;
    while (performance.now() < deadline) {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const currentPage = diagnostics.currentPage;
      const surface = document.querySelector('[data-testid="page-' + (currentPage + 1) + '"] [data-render-state="ready"]');
      if (diagnostics.documentPath === expectedPath && diagnostics.pageRenderReady && surface) {
        const settledStarted = performance.now();
        await new Promise((resolve) => setTimeout(resolve, 250));
        const settledDiagnostics = window.__butterPaperTestHooks.getDiagnostics();
        const settledSurface = document.querySelector('[data-testid="page-' + (settledDiagnostics.currentPage + 1) + '"] [data-render-state="ready"]');
        return {
          document_path_matches: settledDiagnostics.documentPath === expectedPath,
          page_render_ready: settledDiagnostics.pageRenderReady === true,
          current_surface_ready: Boolean(settledSurface),
          settled_for_ms: performance.now() - settledStarted,
        };
      }
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    throw new Error('Document did not reopen with a current raster surface');
  })()`);
  const observed = {
    closed,
    memory: { before_close: beforeClose, after_close: afterClose },
    reopened,
  };
  const checks = assessCloseReopenEvidence(observed);
  const result = {
    command_id: command.id,
    observed_milestones: [],
    milestones: [],
    observed,
  };
  const evidence = {
    "document-resources-released": closed,
    "memory-recovery-recorded": observed.memory,
    "document-reopened": reopened,
    "settled-current-generation-250ms": reopened,
  };
  for (const milestone of command.expected_milestones) {
    const key = milestone.replaceAll("-", "_");
    addMilestone(
      event,
      result,
      milestone,
      checks[key] === true,
      evidence[milestone],
    );
  }
  finalizeCommandMilestones(result, command);
  return {
    contract,
    input_lane: "cdp-input-diagnostic",
    command_results: [result],
    exact_manifest_replay: result.manifest_milestones_complete,
  };
}

function electronRenderCancellationCount(perf) {
  return (
    (perf?.renderPage?.abortedBeforeStart ?? 0) +
    (perf?.renderPage?.abortedAfterStart ?? 0) +
    Object.values(perf?.obsoleteRenderCompletions ?? {}).reduce(
      (total, count) => total + count,
      0,
    )
  );
}

async function runElectronCachePressure(cdp, contract, event) {
  const command = contract.commands[0];
  await selectElectronThumbnailPage(cdp, 1);
  await cdp.evaluate(`window.__butterPaperTestHooks.setZoom(1)`);
  const baselineIdentity = await captureElectronCurrentPageImageIdentity(
    cdp,
    0,
  );
  await cdp.evaluate(`(() => {
    window.__butterPaperTestHooks.resetPerfSnapshot();
    window.__electronCacheProbeActive = true;
    window.__electronCacheProbe = {
      frame_count: 0,
      stale_visible_surface_frames: 0,
      first_stale_frame: null,
      max_page_url_bytes: 0,
      max_thumbnail_bytes: 0,
      max_decoded_render_bytes: 0,
      page_url_byte_limit: null,
      thumbnail_byte_limit: null,
      decoded_render_byte_limit: null,
    };
    const sample = () => {
      if (!window.__electronCacheProbeActive) return;
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const visible = [...document.querySelectorAll('[data-render-state="ready"]')].filter((surface) => {
        const bounds = surface.getBoundingClientRect();
        return bounds.right > 0 && bounds.bottom > 0
          && bounds.left < innerWidth && bounds.top < innerHeight;
      });
      const stale = visible.filter((surface) => surface.getAttribute('data-render-quality') === 'stale-preview');
      const probe = window.__electronCacheProbe;
      probe.frame_count += 1;
      probe.max_page_url_bytes = Math.max(probe.max_page_url_bytes, diagnostics.pageUrlCacheBytes ?? 0);
      probe.max_thumbnail_bytes = Math.max(probe.max_thumbnail_bytes, diagnostics.thumbnailCacheBytes ?? 0);
      probe.max_decoded_render_bytes = Math.max(
        probe.max_decoded_render_bytes,
        diagnostics.decodedRenderCacheBytes ?? 0,
      );
      probe.page_url_byte_limit = diagnostics.pageUrlCacheByteLimit ?? null;
      probe.thumbnail_byte_limit = diagnostics.thumbnailCacheByteLimit ?? null;
      probe.decoded_render_byte_limit = diagnostics.decodedRenderCacheByteLimit ?? null;
      if (stale.length > 0) {
        probe.stale_visible_surface_frames += 1;
        probe.first_stale_frame ??= stale.map((surface) => ({
          page_index: Number(surface.closest('[data-page-index]')?.getAttribute('data-page-index')),
          quality: surface.getAttribute('data-render-quality'),
        }));
      }
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  })()`);

  const pageCount = await cdp.evaluate(
    `window.__butterPaperTestHooks.getDiagnostics().pageCount`,
  );
  const cycles = [];
  for (let cycleIndex = 0; cycleIndex < command.cycles; cycleIndex += 1) {
    const targetPage = Math.max(
      2,
      Math.round(((cycleIndex + 1) * pageCount) / command.cycles),
    );
    const beforePerf = await cdp.evaluate(
      `window.__butterPaperTestHooks.getPerfSnapshot()`,
    );
    const beforeProbe = await cdp.evaluate(
      `structuredClone(window.__electronCacheProbe)`,
    );
    const actions = [];

    await selectElectronThumbnailPage(cdp, targetPage);
    const navigationCurrent = await cdp.evaluate(
      `window.__butterPaperTestHooks.getDiagnostics().currentPage === ${targetPage - 1}`,
    );
    actions.push("navigate");

    const requestsBeforeZoom = await cdp.evaluate(
      `window.__butterPaperTestHooks.getPerfSnapshot().renderPage.requests`,
    );
    await cdp.evaluate(`window.__butterPaperTestHooks.setZoom(4)`);
    await waitForEditorCondition(
      cdp,
      `(() => {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const perf = window.__butterPaperTestHooks.getPerfSnapshot();
      return diagnostics.zoom === 4 && perf.renderPage.requests > ${requestsBeforeZoom};
    })()`,
      `Electron cache cycle ${cycleIndex + 1} zoom render request`,
      10_000,
    );
    actions.push("zoom");

    const pan = await cdp.evaluate(`(() => {
      const viewport = document.querySelector('[data-testid="document-viewport"]');
      if (!viewport) throw new Error('Document viewport is unavailable');
      const before = { left: viewport.scrollLeft, top: viewport.scrollTop };
      viewport.scrollBy({ left: 180, top: 140, behavior: 'instant' });
      viewport.dispatchEvent(new Event('scroll', { bubbles: true }));
      return {
        before,
        after: { left: viewport.scrollLeft, top: viewport.scrollTop },
      };
    })()`);
    actions.push("pan");

    await delay(25);
    await cdp.evaluate(`window.__butterPaperTestHooks.setZoom(1)`);
    await selectElectronThumbnailPage(cdp, 1);
    actions.push("return-page-1");
    const identity = await captureElectronCurrentPageImageIdentity(cdp, 0);
    const resources = await captureElectronReadyRasterResources(cdp);
    const afterPerf = await cdp.evaluate(
      `window.__butterPaperTestHooks.getPerfSnapshot()`,
    );
    const afterProbe = await cdp.evaluate(
      `structuredClone(window.__electronCacheProbe)`,
    );
    const cancellationCount =
      electronRenderCancellationCount(afterPerf) -
      electronRenderCancellationCount(beforePerf);
    const cycle = {
      cycle: cycleIndex + 1,
      target_page: targetPage,
      actions,
      navigation_current: navigationCurrent,
      zoom_current: true,
      pan,
      cancellation_count: cancellationCount,
      stale_visible_surface_frames:
        afterProbe.stale_visible_surface_frames -
        beforeProbe.stale_visible_surface_frames,
      presentation_current:
        identity.page_index === 0 &&
        identity.current_page === true &&
        identity.quality !== "stale-preview",
      image_identity: identity.identity,
      image: identity,
      decoded_render_bytes: resources.diagnostics.decodedRenderCacheBytes,
      renderer_resource_submission_bytes:
        resources.visible_raster_resources.rgba8_submission_bytes,
      renderer_resource_submission_basis:
        "visible-canvas-width-times-height-times-four-rgba8-bytes",
      physical_bus_upload_bytes: null,
      render_page: afterPerf.renderPage,
    };
    event("cache-pressure-cycle-completed", cycle);
    cycles.push(cycle);
  }
  const probe = await cdp.evaluate(`(() => {
    window.__electronCacheProbeActive = false;
    return structuredClone(window.__electronCacheProbe);
  })()`);
  const observation = {
    expected_cycles: command.cycles,
    expected_sequence: command.sequence,
    expected_image_identity: baselineIdentity.identity,
    cycles,
    baseline_image: baselineIdentity,
    cache: {
      max_page_url_bytes: probe.max_page_url_bytes,
      page_url_byte_limit: probe.page_url_byte_limit,
      max_thumbnail_bytes: probe.max_thumbnail_bytes,
      thumbnail_byte_limit: probe.thumbnail_byte_limit,
      max_decoded_render_bytes: probe.max_decoded_render_bytes,
      decoded_render_byte_limit: probe.decoded_render_byte_limit,
      max_renderer_resource_submission_bytes: Math.max(
        0,
        ...cycles.map(({ renderer_resource_submission_bytes: bytes }) => bytes),
      ),
      renderer_resource_submission_byte_limit: 32 * 8192 * 8192 * 4,
    },
    upload_byte_count: null,
    probe,
  };
  const evidence = assessElectronCachePressureEvidence(observation);
  const result = {
    command_id: command.id,
    observed_milestones: [],
    milestones: [],
    observation,
    exact_fields: evidence.exact_fields,
    semantic_passed: evidence.semantic_passed,
    decision_timing_eligible: false,
  };
  addMilestone(
    event,
    result,
    "declared-cache-byte-limit-held",
    evidence.milestones["declared-cache-byte-limit-held"],
    observation.cache,
  );
  addMilestone(
    event,
    result,
    "decoded-byte-limit-held",
    evidence.milestones["decoded-byte-limit-held"],
    observation.cache,
  );
  addMilestone(
    event,
    result,
    "upload-byte-count-recorded",
    false,
    { upload_byte_count: null },
    "A physical-GPU run has not supplied an Electron GPU upload-byte receipt.",
  );
  finalizeCommandMilestones(result, command);
  return {
    contract,
    input_lane: "cdp-input-diagnostic",
    command_results: [result],
    exact_manifest_replay: false,
    semantic_manifest_replay: evidence.semantic_passed,
    decision_timing_eligible: false,
    blocker: evidence.semantic_passed
      ? "Physical-GPU upload-byte evidence is still required."
      : "Electron cache-pressure semantic replay did not satisfy every non-upload exact field.",
  };
}

async function captureElectronReadyRasterResources(cdp) {
  return cdp.evaluate(`(() => {
    const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
    const currentPage = diagnostics.currentPage;
    const current = document.querySelector('[data-testid="page-' + (currentPage + 1) + '"]');
    const viewport = document.querySelector('[data-testid="document-viewport"]');
    const readySurfaces = [...(current?.querySelectorAll('[data-render-state="ready"]') ?? [])];
    const visibleCanvases = [...document.querySelectorAll('canvas')].filter((canvas) => {
      const bounds = canvas.getBoundingClientRect();
      return bounds.right > 0 && bounds.bottom > 0
        && bounds.left < innerWidth && bounds.top < innerHeight;
    });
    const visibility = window.__butterPaperTestHooks.getPerfSnapshot()
      .pageImageVisibility[String(currentPage)];
    return {
      diagnostics,
      current_generation_presented: readySurfaces.length > 0
        && readySurfaces.every((surface) => surface.getAttribute('data-render-quality') !== 'stale-preview'),
      settled_density: viewport
        ? (${measureElectronVisibleRasterDensity.toString()})(
            current,
            viewport.getBoundingClientRect(),
            window.devicePixelRatio,
          )
        : 0,
      visible_raster_resources: {
        count: visibleCanvases.length,
        max_pixels: Math.max(0, ...visibleCanvases.map((canvas) => canvas.width * canvas.height)),
        rgba8_submission_bytes: visibleCanvases.reduce(
          (total, canvas) => total + canvas.width * canvas.height * 4,
          0,
        ),
      },
      physical_bus_upload_bytes: null,
    };
  })()`);
}

async function captureElectronFixedCropEvidence(cdp, fixtureId) {
  const cropPath = resolve(
    performanceDirectory,
    `results/public-fixtures-v1/${fixtureId}.crops.json`,
  );
  const oracle = JSON.parse(await readFile(cropPath, "utf8"));
  const capture = async () =>
    cdp.evaluate(`(async () => {
    const oracle = ${JSON.stringify(oracle)};
    const documentModel = window.__butterPaperTestHooks.getActiveDocument();
    const page = document.querySelector('[data-testid="page-1"]');
    const canvases = [...(page?.querySelectorAll('canvas') ?? [])]
      .filter((canvas) => canvas.width > 0 && canvas.height > 0)
      .sort((left, right) => right.width * right.height - left.width * left.height);
    const canvas = canvases[0];
    const pageSize = documentModel?.pages?.[0]?.size;
    if (!canvas || !pageSize) return [];
    const context = canvas.getContext('2d', { willReadFrequently: true });
    if (!context) return [];
    const digest = async (bytes) => [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))]
      .map((byte) => byte.toString(16).padStart(2, '0')).join('');
    const receipts = [];
    for (const crop of oracle.crops) {
      const x = Math.max(0, Math.floor(crop.pdf_rect.x / pageSize.width * canvas.width));
      const y = Math.max(0, Math.floor(
        (pageSize.height - crop.pdf_rect.y - crop.pdf_rect.height) / pageSize.height * canvas.height,
      ));
      const width = Math.max(1, Math.min(canvas.width - x,
        Math.ceil(crop.pdf_rect.width / pageSize.width * canvas.width)));
      const height = Math.max(1, Math.min(canvas.height - y,
        Math.ceil(crop.pdf_rect.height / pageSize.height * canvas.height)));
      const pixels = context.getImageData(x, y, width, height).data;
      receipts.push({
        crop_id: crop.crop_id,
        pixel_rect: { x, y, width, height },
        rgba8_bytes: pixels.byteLength,
        rgba8_sha256: await digest(pixels),
      });
    }
    return receipts;
  })()`);
  const first = await capture();
  await delay(250);
  await cdp.evaluate(
    `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
  );
  const second = await capture();
  return {
    fixture_id: fixtureId,
    oracle_crop_count: oracle.crops.length,
    first,
    second,
    matched:
      first.length === oracle.crops.length &&
      second.length === oracle.crops.length &&
      first.every(
        (receipt, index) =>
          receipt.rgba8_bytes > 0 &&
          receipt.rgba8_sha256 === second[index]?.rgba8_sha256,
      ),
  };
}

async function runElectronV4OpenEvidence(
  cdp,
  contract,
  event,
  options,
  nativeTarget,
  ready,
) {
  const openState = await cdp.evaluate(`(async () => {
    await new Promise((resolve) => setTimeout(resolve, 250));
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
    const current = document.querySelector('[data-testid="page-' + (diagnostics.currentPage + 1) + '"]');
    const readySurfaces = [...(current?.querySelectorAll('[data-render-state="ready"]') ?? [])];
    return {
      document_path: diagnostics.documentPath,
      page_count: diagnostics.pageCount,
      mounted_page_count: document.querySelectorAll('[data-page-index]').length,
      app_root_present: Boolean(document.querySelector('[data-testid="app-root"]')),
      preview_current_generation: diagnostics.pageRenderReady === true
        && readySurfaces.length > 0
        && readySurfaces.every((surface) => surface.getAttribute('data-render-quality') !== 'stale-preview'),
    };
  })()`);
  const needsFixedCrops = contract.commands.some(
    ({ expected_milestones: milestones }) =>
      milestones.includes("fixed-crops-matched"),
  );
  const fixedCrops = needsFixedCrops
    ? await captureElectronFixedCropEvidence(cdp, contract.fixture_id)
    : null;
  const results = [];
  for (const command of contract.commands) {
    const observation =
      command.id === "small:launch-cold"
        ? {
            process_started: true,
            native_window_presented: nativeTarget !== null,
            interactive_shell: openState.app_root_present === true,
          }
        : {
            document_opened: openState.document_path === options.pdf,
            preview_current_generation: openState.preview_current_generation,
            settled_current_generation_250ms: true,
            fixed_crops_matched: fixedCrops?.matched === true,
            virtual_page_window_bounded:
              openState.page_count > 1 &&
              openState.mounted_page_count < openState.page_count,
          };
    const assessment = assessElectronV4OpenEvidence(command, observation);
    const result = {
      command_id: command.id,
      observation: { ...observation, fixed_crops: fixedCrops, ready },
      exact_fields: assessment.exact_fields,
      observed_milestones: [],
      milestones: [],
    };
    for (const milestone of command.expected_milestones) {
      addMilestone(
        event,
        result,
        milestone,
        assessment.milestones[milestone] === true,
        {
          observation,
          fixed_crops: fixedCrops,
          observation_basis:
            "maintained native open plus exact path, current raster, 250ms settle, and target-specific oracle",
        },
      );
    }
    finalizeCommandMilestones(result, command);
    event("comparison-command-exact-state", {
      command_id: command.id,
      passed: assessment.passed,
      exact_fields: assessment.exact_fields,
    });
    results.push(result);
  }
  const exact =
    nativeTarget !== null &&
    results.every(
      ({ manifest_milestones_complete, exact_fields: fields }) =>
        manifest_milestones_complete && Object.values(fields).every(Boolean),
    );
  return {
    contract,
    input_lane: nativeTarget ? nativeX11InputLane : cdpDiagnosticInputLane,
    command_results: results,
    exact_manifest_replay: exact,
    decision_timing_eligible: false,
    blocker: exact
      ? null
      : "V4 open did not prove every target-specific native launch, settle, crop, or virtualization milestone.",
  };
}

async function runElectronEngineeringFitModes(
  cdp,
  contract,
  event,
  nativeTarget = null,
) {
  const command = contract.commands[0];
  const clickTestId = (testId) =>
    nativeTarget
      ? nativeClickTestId(cdp, nativeTarget, testId)
      : dispatchCdpClick(cdp, testId);
  const observations = [];
  for (const { mode, testId } of [
    { mode: "fit-page", testId: "viewer-fit-page" },
    { mode: "fit-width", testId: "viewer-fit-width" },
  ]) {
    const input = await clickTestId(testId);
    const started = performance.now();
    await waitForEditorCondition(
      cdp,
      `(() => {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const current = document.querySelector('[data-testid="page-' + (diagnostics.currentPage + 1) + '"]');
      const ready = [...(current?.querySelectorAll('[data-render-state="ready"]') ?? [])];
      return diagnostics.zoomPreset === ${JSON.stringify(mode)}
        && diagnostics.pageRenderReady === true
        && ready.length > 0
        && ready.every((surface) => surface.getAttribute('data-render-quality') !== 'stale-preview');
    })()`,
      `${mode} engineering-sheet presentation`,
      20_000,
    );
    await delay(250);
    await cdp.evaluate(
      `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
    );
    const resources = await captureElectronReadyRasterResources(cdp);
    observations.push({
      mode,
      input,
      diagnostics: {
        zoom_preset: resources.diagnostics.zoomPreset,
        page_render_ready: resources.diagnostics.pageRenderReady,
      },
      current_generation_presented: resources.current_generation_presented,
      settled_for_ms: performance.now() - started,
      visible_raster_resources: resources.visible_raster_resources,
      settled_density: resources.settled_density,
      physical_bus_upload_bytes: null,
    });
  }
  const assessment = assessElectronEngineeringFitModesEvidence(observations);
  const result = {
    command_id: command.id,
    observations,
    exact_fields: assessment.exact_fields,
    observed_milestones: [],
    milestones: [],
  };
  for (const milestone of command.expected_milestones) {
    addMilestone(
      event,
      result,
      milestone,
      assessment.milestones[milestone] === true,
      {
        observations,
        observation_basis:
          "maintained fit controls plus current ready raster, two animation frames, and 250ms settle",
      },
    );
  }
  finalizeCommandMilestones(result, command);
  event("comparison-command-exact-state", {
    command_id: command.id,
    passed: assessment.passed,
    exact_fields: assessment.exact_fields,
  });
  event("comparison-v4-component-evidence", {
    command_id: command.id,
    component_scenario: "fit-modes",
    passed: assessment.passed,
    observations,
    milestone_ids: command.expected_milestones,
  });
  const exact = assessment.passed && result.manifest_milestones_complete;
  return {
    contract,
    input_lane: nativeTarget ? nativeX11InputLane : cdpDiagnosticInputLane,
    command_results: [result],
    exact_manifest_replay: exact,
    decision_timing_eligible: nativeTarget ? exact : false,
    blocker: exact
      ? null
      : "Engineering fit modes did not prove exact state, bounded rasters, density, and settle evidence.",
  };
}

async function runElectronEngineeringCachePressureRecovery(
  cdp,
  contract,
  event,
  nativeTarget = null,
) {
  const command = contract.commands[0];
  const clickTestId = (testId) =>
    nativeTarget
      ? nativeClickTestId(cdp, nativeTarget, testId)
      : dispatchCdpClick(cdp, testId);
  await clickTestId("viewer-fit-page");
  await waitForEditorCondition(
    cdp,
    "window.__butterPaperTestHooks.getDiagnostics().zoomPreset === 'fit-page' && window.__butterPaperTestHooks.getDiagnostics().pageRenderReady",
    "engineering cache baseline fit-page",
    20_000,
  );
  await cdp.evaluate(`(() => {
    window.__electronEngineeringCacheProbeActive = true;
    window.__electronEngineeringCacheProbe = {
      max_page_url_bytes: 0,
      max_thumbnail_bytes: 0,
      max_decoded_render_bytes: 0,
    };
    const sample = () => {
      if (!window.__electronEngineeringCacheProbeActive) return;
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const probe = window.__electronEngineeringCacheProbe;
      probe.max_page_url_bytes = Math.max(probe.max_page_url_bytes, diagnostics.pageUrlCacheBytes ?? 0);
      probe.max_thumbnail_bytes = Math.max(probe.max_thumbnail_bytes, diagnostics.thumbnailCacheBytes ?? 0);
      probe.max_decoded_render_bytes = Math.max(
        probe.max_decoded_render_bytes,
        diagnostics.decodedRenderCacheBytes ?? 0,
      );
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  })()`);

  const cycles = [];
  const zoomPercents = [200, 400, 800, 1600, 400];
  for (let cycleIndex = 0; cycleIndex < command.cycles; cycleIndex += 1) {
    const percent = zoomPercents[cycleIndex];
    await clickTestId("viewer-zoom-menu");
    await waitForEditorCondition(
      cdp,
      `Boolean(document.querySelector('[data-testid="viewer-zoom-preset-${percent}"]'))`,
      `${percent}% engineering cache zoom preset`,
    );
    const zoomInput = await clickTestId(`viewer-zoom-preset-${percent}`);
    await waitForEditorCondition(
      cdp,
      `(() => {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const current = document.querySelector('[data-testid="page-' + (diagnostics.currentPage + 1) + '"]');
      return diagnostics.zoom === ${percent / 100}
        && diagnostics.pageRenderReady === true
        && Boolean(current?.querySelector('[data-render-state="ready"]'));
    })()`,
      `${percent}% engineering cache raster`,
      20_000,
    );
    await cdp.send("Input.dispatchKeyEvent", {
      type: "keyDown",
      key: "Escape",
      code: "Escape",
    });
    await cdp.send("Input.dispatchKeyEvent", {
      type: "keyUp",
      key: "Escape",
      code: "Escape",
    });
    await waitForEditorCondition(
      cdp,
      `(() => {
      const preset = document.querySelector('[data-testid="viewer-zoom-preset-${percent}"]');
      const bounds = preset?.getBoundingClientRect();
      return !bounds || bounds.width === 0 || bounds.height === 0;
    })()`,
      `${percent}% engineering cache zoom menu dismissal`,
      5_000,
    );
    await cdp.evaluate(
      `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
    );
    const pan = await cdp.evaluate(`(() => {
      const viewport = document.querySelector('[data-testid="document-viewport"]');
      if (!(viewport instanceof HTMLElement)) throw new Error('Document viewport is unavailable');
      const before = { left: viewport.scrollLeft, top: viewport.scrollTop };
      viewport.scrollBy({
        left: Math.min(180, Math.max(0, viewport.scrollWidth - viewport.clientWidth)),
        top: Math.min(140, Math.max(0, viewport.scrollHeight - viewport.clientHeight)),
        behavior: 'instant',
      });
      viewport.dispatchEvent(new Event('scroll', { bubbles: true }));
      return { before, after: { left: viewport.scrollLeft, top: viewport.scrollTop } };
    })()`);
    await clickTestId("viewer-fit-page");
    await waitForEditorCondition(
      cdp,
      `(() => {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const current = document.querySelector('[data-testid="page-' + (diagnostics.currentPage + 1) + '"]');
      return diagnostics.zoomPreset === 'fit-page'
        && diagnostics.pageRenderReady === true
        && Boolean(current?.querySelector('[data-render-state="ready"]'));
    })()`,
      `engineering cache cycle ${cycleIndex + 1} return fit-page`,
      20_000,
    );
    await cdp.evaluate(
      `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
    );
    const resources = await captureElectronReadyRasterResources(cdp);
    const cycle = {
      cycle: cycleIndex + 1,
      actions: ["zoom", "pan", "fit-page"],
      zoom_percent: percent,
      input: { zoom: zoomInput, pan },
      presentation_current: resources.current_generation_presented,
      decoded_render_bytes: resources.diagnostics.decodedRenderCacheBytes ?? 0,
      renderer_resource_submission_bytes:
        resources.visible_raster_resources.rgba8_submission_bytes,
      renderer_resource_submission_measurement_basis:
        "sum-visible-canvas-width-times-height-times-four-rgba8-bytes",
      physical_bus_upload_bytes: null,
    };
    event("engineering-cache-cycle-completed", cycle);
    cycles.push(cycle);
  }

  const beforeResources = await captureElectronReadyRasterResources(cdp);
  const beforeProcessMetrics = await cdp.evaluate(
    `window.butterPaper.test.getProcessMetrics()`,
  );
  const probe = await cdp.evaluate(`(() => {
    window.__electronEngineeringCacheProbeActive = false;
    return structuredClone(window.__electronEngineeringCacheProbe);
  })()`);
  const before = {
    document_count: beforeResources.diagnostics.tabs?.length ?? 0,
    render_cache_bytes: beforeResources.diagnostics.renderCacheBytes ?? 0,
    decoded_render_bytes:
      beforeResources.diagnostics.decodedRenderCacheBytes ?? 0,
    renderer_resource_submission_bytes:
      beforeResources.visible_raster_resources.rgba8_submission_bytes,
    process_metrics: beforeProcessMetrics,
  };
  await cdp.evaluate(`window.__butterPaperTestHooks.closeTab(0)`);
  const afterState = await waitForEditorCondition(
    cdp,
    `(() => {
    const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
    if ((diagnostics.tabs?.length ?? 0) !== 0 || window.__butterPaperTestHooks.getActiveDocument()) return null;
    return {
      document_count: 0,
      render_cache_bytes: diagnostics.renderCacheBytes ?? 0,
      decoded_render_bytes: diagnostics.decodedRenderCacheBytes ?? 0,
      renderer_resource_submission_bytes: [...document.querySelectorAll('canvas')]
        .reduce((total, canvas) => total + canvas.width * canvas.height * 4, 0),
    };
  })()`,
    "engineering cache post-close resource release",
    10_000,
  );
  await cdp.send("HeapProfiler.collectGarbage");
  const afterProcessMetrics = await cdp.evaluate(
    `window.butterPaper.test.getProcessMetrics()`,
  );
  const after = { ...afterState, process_metrics: afterProcessMetrics };
  const maxRendererSubmissionBytes = Math.max(
    0,
    ...cycles.map(({ renderer_resource_submission_bytes: bytes }) => bytes),
  );
  const observation = {
    expected_cycles: command.cycles,
    cycles,
    cache: {
      max_page_url_bytes: probe.max_page_url_bytes,
      page_url_byte_limit:
        beforeResources.diagnostics.pageUrlCacheByteLimit ?? null,
      max_thumbnail_bytes: probe.max_thumbnail_bytes,
      thumbnail_byte_limit:
        beforeResources.diagnostics.thumbnailCacheByteLimit ?? null,
      max_decoded_render_bytes: probe.max_decoded_render_bytes,
      decoded_render_byte_limit:
        beforeResources.diagnostics.decodedRenderCacheByteLimit ?? null,
      max_renderer_resource_submission_bytes: maxRendererSubmissionBytes,
      renderer_resource_submission_byte_limit: 32 * 8192 * 8192 * 4,
    },
    recovery: {
      before,
      after,
      released_render_bytes:
        before.render_cache_bytes + before.renderer_resource_submission_bytes,
    },
  };
  const assessment =
    assessElectronEngineeringCacheRecoveryEvidence(observation);
  const result = {
    command_id: command.id,
    observation,
    exact_fields: assessment.exact_fields,
    observed_milestones: [],
    milestones: [],
  };
  for (const milestone of command.expected_milestones) {
    addMilestone(
      event,
      result,
      milestone,
      assessment.milestones[milestone] === true,
      {
        observation,
        observation_basis:
          "five maintained zoom/pan/fit cycles plus post-close cache and process snapshots",
      },
    );
  }
  finalizeCommandMilestones(result, command);
  event("comparison-command-exact-state", {
    command_id: command.id,
    passed: assessment.passed,
    exact_fields: assessment.exact_fields,
  });
  event("comparison-v4-component-evidence", {
    command_id: command.id,
    component_scenario: "cache-pressure-recovery",
    passed: assessment.passed,
    observation,
    milestone_ids: command.expected_milestones,
  });
  const exact = assessment.passed && result.manifest_milestones_complete;
  return {
    contract,
    input_lane: nativeTarget ? nativeX11InputLane : cdpDiagnosticInputLane,
    command_results: [result],
    exact_manifest_replay: exact,
    decision_timing_eligible: false,
    blocker: exact
      ? null
      : "Engineering cache pressure did not prove five exact bounded cycles and post-close recovery.",
  };
}

async function runRendererScenario(cdp, options, event, applicationPid) {
  await cdp.evaluate(`(async () => {
    const deadline = performance.now() + 20000;
    while (performance.now() < deadline) {
      if (window.__butterPaperTestHooks && document.querySelector('[data-testid="app-root"]')) return;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    throw new Error('Butter Paper test hooks did not become ready');
  })()`);
  const identity = await cdp.evaluate(`(async () => ({
    title: document.title,
    href: location.href,
    has_app_root: Boolean(document.querySelector('[data-testid="app-root"]')),
    device_pixel_ratio: window.devicePixelRatio,
    metadata: await window.butterPaper.application.getMetadata(),
  }))()`);
  if (!identity.has_app_root || identity.metadata?.development !== true) {
    throw new Error(
      `CDP target is not the Butter Paper development renderer: ${JSON.stringify(identity)}`,
    );
  }
  await cdp.evaluate(
    `window.__butterPaperTestHooks.setWindowBounds({ width: 1200, height: 800 })`,
  );
  await cdp.evaluate(
    `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
  );
  event("shell-ready", { identity });

  await cdp.evaluate(`(() => {
    window.__electronPerfFrames = [];
    window.__electronPerfFrameActive = true;
    window.__electronNativeApplicationFrameAck = {
      input_event_count: 0,
      acknowledged_event_count: 0,
      samples_ms: [],
      event_types: {},
      receipt_scope: "trusted-dom-native-event-receipt-to-next-request-animation-frame-callback-not-physical-scanout",
      physical_scanout_observed: false,
    };
    if (${JSON.stringify(options.inputLane === nativeX11InputLane)}) {
      const timing = window.__electronNativeApplicationFrameAck;
      const observeNativeInput = (inputEvent) => {
        if (inputEvent.isTrusted !== true) return;
        const receivedAt = performance.now();
        timing.input_event_count += 1;
        timing.event_types[inputEvent.type] =
          (timing.event_types[inputEvent.type] ?? 0) + 1;
        requestAnimationFrame((frameTime) => {
          timing.samples_ms.push(Math.max(0, frameTime - receivedAt));
          timing.acknowledged_event_count += 1;
        });
      };
      for (const eventType of ["pointerup", "wheel", "keyup"]) {
        window.addEventListener(eventType, observeNativeInput, true);
      }
      window.__electronNativeApplicationFrameAckCleanup = () => {
        for (const eventType of ["pointerup", "wheel", "keyup"]) {
          window.removeEventListener(eventType, observeNativeInput, true);
        }
      };
    }
    let previous;
    const sample = (time) => {
      if (!window.__electronPerfFrameActive) return;
      if (previous !== undefined) window.__electronPerfFrames.push(time - previous);
      previous = time;
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  })()`);

  let nativeTarget = null;
  if (options.inputLane === nativeX11InputLane) {
    nativeTarget = await locateNativeElectronTarget(
      cdp,
      applicationPid,
      options.timeoutMs,
      event,
    );
  }

  const observeMatchedViewState =
    (options.v6ExecutionContext ?? options.v5ExecutionContext)
      ?.benefit_metrics_eligible === true;
  const matchedViewStateGate = createElectronComparisonViewStateCheckpointGate(
    observeMatchedViewState,
  );
  if (matchedViewStateGate.claim("measurement-start")) {
    event(
      "comparison-view-state",
      await observeElectronComparisonViewState(
        cdp,
        options,
        "measurement-start",
      ),
    );
  }

  const openStarted = performance.now();
  event("pdf-open-requested");
  const nativeOpenInput =
    options.inputLane === nativeX11InputLane
      ? await runElectronNativeOpenAction(cdp, nativeTarget, options.pdf)
      : null;
  if (options.inputLane !== nativeX11InputLane) {
    await cdp.evaluate(
      `window.__butterPaperTestHooks.openDocumentPath(${JSON.stringify(options.pdf)})`,
    );
  }
  const ready = await cdp.evaluate(`(async () => {
    const deadline = performance.now() + ${Math.min(options.timeoutMs, 90_000)};
    while (performance.now() < deadline) {
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const perf = window.__butterPaperTestHooks.getPerfSnapshot();
      const visibility = perf.pageImageVisibility["0"];
      const density = (visibility?.bestRenderedWidthRatio ?? 0) / window.devicePixelRatio;
      if (
        diagnostics.documentPath &&
        diagnostics.pageRenderReady &&
        density >= 0.75
      ) return { diagnostics, perf };
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    throw new Error('Timed out waiting for an acceptable first-page preview');
  })()`);
  await cdp.evaluate(
    `new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
  );
  event("first-page-visible", {
    duration_ms: performance.now() - openStarted,
    page_count: ready.diagnostics.pageCount,
    acceptable_preview_visible_ms:
      ready.perf.firstPageAcceptablePreviewVisibleMs,
    image_visibility: ready.perf.pageImageVisibility["0"] ?? null,
    display_scale_factor: identity.device_pixel_ratio,
    rendered_device_pixel_ratio:
      (ready.perf.pageImageVisibility["0"]?.bestRenderedWidthRatio ?? 0) /
      identity.device_pixel_ratio,
  });

  let expandedComparison = null;
  if (options.inputLane === nativeX11InputLane) {
    const observedOpen = await cdp.evaluate(`(async () => {
      await new Promise((resolve) => setTimeout(resolve, 250));
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
      const currentPage = diagnostics.currentPage;
      const current = document.querySelector('[data-testid="page-' + (currentPage + 1) + '"]');
      const readySurfaces = [...(current?.querySelectorAll('[data-render-state="ready"]') ?? [])];
      return {
        observed_path: diagnostics.documentPath,
        preview_current_generation: diagnostics.pageRenderReady === true
          && readySurfaces.length > 0
          && readySurfaces.every((surface) => surface.getAttribute('data-render-quality') !== 'stale-preview'),
        settled_current_generation_250ms: true,
        presentation_current: current !== null && readySurfaces.length > 0,
      };
    })()`);
    const nativeOpenObservation = {
      requested_path: options.pdf,
      native_window_target_verified: nativeTarget !== null,
      native_open_action_completed: nativeOpenInput !== null,
      ...observedOpen,
    };
    const nativeOpenEvidence = assessElectronNativeOpenEvidence(
      nativeOpenObservation,
    );
    event("viewer-native-launch-open-evidence", {
      input_lane: nativeX11InputLane,
      ...nativeOpenObservation,
      ...nativeOpenEvidence,
      physical_scanout_observed: false,
      observation_basis:
        "XTEST File > Open plus exact document path and current ready raster after 250ms and two animation frames",
    });
    if (options.scenario === "open-pdf") {
      expandedComparison = {
        input_lane: nativeX11InputLane,
        input: nativeOpenInput,
        evidence: nativeOpenEvidence,
        exact_manifest_replay: nativeOpenEvidence.passed,
        decision_timing_eligible: false,
        blocker: nativeOpenEvidence.passed
          ? null
          : "Native Electron launch/open proof did not satisfy every exact path, generation, settle, and presentation check.",
      };
    }
  }
  if (options.scenario === "open-pdf" && options.v4ExecutionContext) {
    expandedComparison = await runElectronV4OpenEvidence(
      cdp,
      options.comparisonScenarioContract,
      event,
      options,
      options.inputLane === nativeX11InputLane ? nativeTarget : null,
      ready,
    );
  }
  if (options.scenario === "annotation-create") {
    expandedComparison =
      options.inputLane === nativeX11InputLane
        ? await runElectronNativeAnnotationCreate(
            cdp,
            options.comparisonScenarioContract,
            event,
            nativeTarget,
          )
        : await runElectronAnnotationCreate(
            cdp,
            options.comparisonScenarioContract,
            event,
          );
  }
  if (options.scenario === "annotation-transform") {
    expandedComparison =
      options.inputLane === nativeX11InputLane
        ? await runElectronNativeAnnotationTransform(
            cdp,
            options.comparisonScenarioContract,
            event,
            nativeTarget,
          )
        : await runElectronAnnotationTransform(
            cdp,
            options.comparisonScenarioContract,
            event,
          );
  }
  if (options.scenario === "annotation-properties-history") {
    expandedComparison = await runElectronAnnotationPropertiesHistory(
      cdp,
      options.comparisonScenarioContract,
      event,
    );
  }
  if (options.scenario === "native-property-edit-undo") {
    expandedComparison = await runElectronNativePropertyEditUndoV5(
      cdp,
      options.comparisonScenarioContract,
      event,
      nativeTarget,
    );
  }
  if (options.scenario === "native-snap-transform-120hz") {
    expandedComparison = await runElectronNativeSnapTransformV5(
      cdp,
      options.comparisonScenarioContract,
      event,
      nativeTarget,
    );
  }
  if (options.scenario === multiDocumentScenarioV5) {
    expandedComparison = await runElectronNativeMultiDocumentSessionV5(
      cdp,
      options.comparisonScenarioContract,
      event,
      nativeTarget,
      options.pdfs,
      applicationPid,
    );
  }
  if (options.scenario === dynamicFidelityScenarioV5) {
    expandedComparison = await runElectronNativeDynamicFidelityV5(
      cdp,
      options.comparisonScenarioContract,
      event,
      nativeTarget,
    );
  }
  if (options.scenario === "editor-create") {
    expandedComparison = await runElectronEditorCreate(
      cdp,
      options.comparisonScenarioContract,
      event,
      options.inputLane === nativeX11InputLane ? nativeTarget : null,
    );
  }
  if (options.scenario === "editor-workload") {
    expandedComparison = await runElectronEditorWorkload(
      cdp,
      options.comparisonScenarioContract,
      options.comparisonWorkload,
      event,
    );
  }
  if (options.scenario === "persistence-workload") {
    expandedComparison = await runElectronPersistenceWorkload(
      cdp,
      options.comparisonScenarioContract,
      options.comparisonWorkload,
      event,
      options.pdf,
    );
  }
  if (options.scenario === "continuous-scroll") {
    expandedComparison =
      options.inputLane === nativeX11InputLane
        ? await runElectronNativeContinuousScroll(
            cdp,
            options.comparisonScenarioContract,
            event,
            nativeTarget,
          )
        : await runElectronContinuousScroll(
            cdp,
            options.comparisonScenarioContract,
            event,
          );
  }
  if (options.scenario === "viewer-layout") {
    expandedComparison = await runElectronViewerLayout(
      cdp,
      options.comparisonScenarioContract,
      event,
      options.inputLane === nativeX11InputLane ? nativeTarget : null,
    );
  }
  if (options.scenario === "high-zoom-pan") {
    expandedComparison =
      options.inputLane === nativeX11InputLane
        ? await runElectronNativeHighZoomPan(
            cdp,
            options.comparisonScenarioContract,
            event,
            nativeTarget,
          )
        : await runElectronHighZoomPan(
            cdp,
            options.comparisonScenarioContract,
            event,
          );
  }
  if (options.scenario === "close-reopen") {
    expandedComparison = await runElectronCloseReopen(
      cdp,
      options.comparisonScenarioContract,
      event,
      options.pdf,
    );
  }
  if (options.scenario === "cache-pressure") {
    expandedComparison = await runElectronCachePressure(
      cdp,
      options.comparisonScenarioContract,
      event,
    );
  }
  if (options.scenario === "fit-modes") {
    expandedComparison = await runElectronEngineeringFitModes(
      cdp,
      options.comparisonScenarioContract,
      event,
      options.inputLane === nativeX11InputLane ? nativeTarget : null,
    );
  }
  if (options.scenario === "cache-pressure-recovery") {
    expandedComparison = await runElectronEngineeringCachePressureRecovery(
      cdp,
      options.comparisonScenarioContract,
      event,
      options.inputLane === nativeX11InputLane ? nativeTarget : null,
    );
  }

  if (
    options.scenario === "page-navigation" &&
    options.inputLane === nativeX11InputLane
  ) {
    expandedComparison = await runElectronNativePageNavigation(
      cdp,
      options.comparisonScenarioContract,
      event,
      nativeTarget,
    );
  }

  if (
    options.scenario === "page-navigation" &&
    options.inputLane !== nativeX11InputLane
  ) {
    const command = options.comparisonScenarioContract.commands[0];
    const pageSequence = normalizedPageSequence(ready.diagnostics.pageCount);
    const navigationResults = [];
    for (const pageNumber of pageSequence) {
      const started = performance.now();
      const result = await cdp.evaluate(`(async () => {
        const pageNumber = ${pageNumber};
        window.__butterPaperTestHooks.resetPerfSnapshot();
        const list = document.querySelector('[data-testid="page-thumbnail-list"]');
        if (!list) throw new Error('Thumbnail list is unavailable');
        const fraction = (pageNumber - 1) / Math.max(1, window.__butterPaperTestHooks.getDiagnostics().pageCount - 1);
        list.scrollTop = fraction * Math.max(0, list.scrollHeight - list.clientHeight);
        list.dispatchEvent(new Event('scroll', { bubbles: true }));
        const deadline = performance.now() + 5000;
        let button;
        while (performance.now() < deadline) {
          button = document.querySelector('[data-testid="page-thumbnail-select-' + pageNumber + '"]');
          if (button) break;
          await new Promise((resolve) => requestAnimationFrame(resolve));
        }
        if (!button) throw new Error('Page ' + pageNumber + ' thumbnail did not virtualize');
        button.click();
        const settleDeadline = performance.now() + 10000;
        while (performance.now() < settleDeadline) {
          const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
          const perf = window.__butterPaperTestHooks.getPerfSnapshot();
          const raster = perf.renderPage;
          const visibility = perf.pageImageVisibility[String(pageNumber - 1)];
          const renderedDevicePixelRatio =
            (visibility?.bestRenderedWidthRatio ?? 0) / window.devicePixelRatio;
          const acceptablePreviewVisible = renderedDevicePixelRatio >= 0.75;
          const rasterResolved = raster.completed + raster.hits > 0 || acceptablePreviewVisible;
          if (diagnostics.currentPage === pageNumber - 1 && rasterResolved && acceptablePreviewVisible) {
            await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
            return { diagnostics, render_page: raster, image_visibility: visibility, rendered_device_pixel_ratio: renderedDevicePixelRatio, display_scale_factor: window.devicePixelRatio };
          }
          await new Promise((resolve) => setTimeout(resolve, 10));
        }
        throw new Error('Page ' + pageNumber + ' did not produce an acceptable current preview');
      })()`);
      event("page-navigation-completed", {
        duration_ms: performance.now() - started,
        page_number: pageNumber,
        visible_page_indices: result.diagnostics.visiblePageIndices,
        render_page: result.render_page,
        image_visibility: result.image_visibility,
        display_scale_factor: result.display_scale_factor,
        rendered_device_pixel_ratio: result.rendered_device_pixel_ratio,
      });
      navigationResults.push({ page_number: pageNumber, ...result });
    }
    await delay(250);
    for (const milestone of command.expected_milestones) {
      event("comparison-milestone", { command_id: command.id, milestone });
    }
    expandedComparison = {
      contract: options.comparisonScenarioContract,
      input_lane: cdpDiagnosticInputLane,
      command_results: [
        {
          command_id: command.id,
          observations: navigationResults,
          exact_fields: {},
          manifest_milestones_complete: true,
        },
      ],
      exact_manifest_replay: navigationResults.length === pageSequence.length,
      decision_timing_eligible: false,
      blocker: null,
    };
  }

  if (options.scenario === "zoom" && options.inputLane === nativeX11InputLane) {
    expandedComparison = await runElectronNativeZoom(
      cdp,
      options.comparisonScenarioContract,
      event,
      nativeTarget,
    );
  }

  if (options.scenario === "zoom" && options.inputLane !== nativeX11InputLane) {
    const command = options.comparisonScenarioContract.commands[0];
    const zoomResults = [];
    await cdp.evaluate(`(() => {
      window.__electronZoomProbeActive = true;
      window.__electronZoomProbe = { frame_count: 0, stale_visible_surface_frames: 0, first_stale_frame: null };
      const sample = () => {
        if (!window.__electronZoomProbeActive) return;
        const visible = [...document.querySelectorAll('[data-render-state="ready"]')].filter((surface) => {
          const bounds = surface.getBoundingClientRect();
          return bounds.right > 0 && bounds.bottom > 0
            && bounds.left < innerWidth && bounds.top < innerHeight;
        });
        const stale = visible.filter((surface) => surface.getAttribute('data-render-quality') === 'stale-preview');
        const probe = window.__electronZoomProbe;
        probe.frame_count += 1;
        if (stale.length > 0) {
          probe.stale_visible_surface_frames += 1;
          probe.first_stale_frame ??= stale.map((surface) => ({
            page_index: Number(surface.closest('[data-page-index]')?.getAttribute('data-page-index')),
            quality: surface.getAttribute('data-render-quality'),
          }));
        }
        requestAnimationFrame(sample);
      };
      requestAnimationFrame(sample);
    })()`);
    for (const percent of zoomSequence) {
      const beforeZoom = await cdp.evaluate(
        `window.__butterPaperTestHooks.getDiagnostics().zoom`,
      );
      if (Math.abs(beforeZoom - percent / 100) >= 0.0001) {
        await cdp.evaluate(`window.__butterPaperTestHooks.resetPerfSnapshot()`);
      }
      const priorZoomMenuExpanded = await cdp.evaluate(
        `document.querySelector('[data-testid="viewer-zoom-menu"]')?.getAttribute('aria-expanded') === 'true'`,
      );
      if (priorZoomMenuExpanded) {
        await dispatchCdpClick(cdp, "viewer-zoom-menu");
      }
      await waitForEditorCondition(
        cdp,
        `document.querySelector('[data-testid="viewer-zoom-menu"]')?.getAttribute('aria-expanded') !== 'true'
          && ![...document.querySelectorAll('[data-testid^="viewer-zoom-preset-"]')]
            .some((element) => {
              const bounds = element.getBoundingClientRect();
              return bounds.width > 0 && bounds.height > 0;
            })`,
        `closed zoom menu before ${percent}% selection`,
        5_000,
      );
      await dispatchCdpClick(cdp, "viewer-zoom-menu");
      await waitForEditorCondition(
        cdp,
        `document.querySelector('[data-testid="viewer-zoom-menu"]')?.getAttribute('aria-expanded') === 'true'
          && [...document.querySelectorAll('[data-testid^="viewer-zoom-preset-"]')]
            .some((element) => {
              const bounds = element.getBoundingClientRect();
              return bounds.width > 0 && bounds.height > 0;
            })`,
        `open zoom menu before ${percent}% selection`,
        5_000,
      );
      const presetAvailable = await cdp.evaluate(
        `Boolean(document.querySelector('[data-testid="viewer-zoom-preset-${percent}"]'))`,
      );
      const started = performance.now();
      if (presetAvailable) {
        await dispatchCdpClick(cdp, `viewer-zoom-preset-${percent}`);
      } else if (percent === 1200) {
        await dispatchCdpClick(cdp, "viewer-zoom-menu");
        await waitForEditorCondition(
          cdp,
          `document.querySelector('[data-testid="viewer-zoom-menu"]')?.getAttribute('aria-expanded') !== 'true'`,
          "closed zoom menu before frozen 1200% semantic selection",
          5_000,
        );
        await cdp.evaluate(
          `window.__butterPaperTestHooks.setZoom(${percent / 100})`,
        );
      } else {
        throw new Error(`${percent}% zoom is not a maintained preset`);
      }
      const result = await cdp.evaluate(`(async () => {
        const interactionStarted = performance.now();
        const zoom = ${percent / 100};
        const noOp = Math.abs(${beforeZoom} - zoom) < 0.0001;
        const deadline = performance.now() + 5000;
        while (performance.now() < deadline) {
          const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
          const perf = window.__butterPaperTestHooks.getPerfSnapshot();
          const visibility = perf.pageImageVisibility[String(diagnostics.currentPage)];
          const viewport = document.querySelector('[data-testid="document-viewport"]');
          const current = document.querySelector('[data-testid="page-' + (diagnostics.currentPage + 1) + '"]');
          const density = viewport
            ? (${measureElectronVisibleRasterDensity.toString()})(
                current,
                viewport.getBoundingClientRect(),
                window.devicePixelRatio,
              )
            : 0;
          const ready = [...(current?.querySelectorAll('[data-render-state="ready"]') ?? [])];
          if (Math.abs(diagnostics.zoom - zoom) < 0.0001
            && diagnostics.pageRenderReady === true
            && density >= ${electronSettledDensityMinimum}
            && ready.length > 0
            && ready.every((surface) => surface.getAttribute('data-render-quality') !== 'stale-preview')) {
            await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
            const responseDurationMs = performance.now() - interactionStarted;
            // Keep successive requests from collapsing into a single render wave.
            await new Promise((resolve) => setTimeout(resolve, 250));
            const settledPerf = window.__butterPaperTestHooks.getPerfSnapshot();
            const settledDiagnostics = window.__butterPaperTestHooks.getDiagnostics();
            const settledVisibility =
              settledPerf.pageImageVisibility[String(settledDiagnostics.currentPage)];
            const settledCurrent = document.querySelector(
              '[data-testid="page-' + (settledDiagnostics.currentPage + 1) + '"]',
            );
            const settledViewport = document.querySelector('[data-testid="document-viewport"]');
            const renderedDevicePixelRatio = settledViewport
              ? (${measureElectronVisibleRasterDensity.toString()})(
                  settledCurrent,
                  settledViewport.getBoundingClientRect(),
                  window.devicePixelRatio,
                )
              : 0;
            const settledReady = [...(settledCurrent?.querySelectorAll('[data-render-state="ready"]') ?? [])];
            if (renderedDevicePixelRatio < ${electronSettledDensityMinimum}
              || settledReady.length === 0
              || settledReady.some((surface) => surface.getAttribute('data-render-quality') === 'stale-preview')) {
              await new Promise((resolve) => setTimeout(resolve, 10));
              continue;
            }
            const visibleCanvases = [...document.querySelectorAll('canvas')]
              .filter((canvas) => {
                const bounds = canvas.getBoundingClientRect();
                return bounds.right > 0 && bounds.bottom > 0
                  && bounds.left < innerWidth && bounds.top < innerHeight;
              });
            return {
              diagnostics: settledDiagnostics,
              no_op: noOp,
              promotion_timed_out: false,
              response_duration_ms: responseDurationMs,
              render_page: settledPerf.renderPage,
              image_visibility: settledVisibility ?? null,
              rendered_device_pixel_ratio: renderedDevicePixelRatio,
              display_scale_factor: window.devicePixelRatio,
              presentation_current: settledReady.length > 0
                && settledReady.every((surface) => surface.getAttribute('data-render-quality') !== 'stale-preview'),
              visible_raster_resources: {
                count: visibleCanvases.length,
                max_pixels: Math.max(0, ...visibleCanvases.map((canvas) => canvas.width * canvas.height)),
              },
            };
          }
          await new Promise((resolve) => setTimeout(resolve, 10));
        }
        const diagnostics = window.__butterPaperTestHooks.getDiagnostics();
        const perf = window.__butterPaperTestHooks.getPerfSnapshot();
        const visibility = perf.pageImageVisibility[String(diagnostics.currentPage)];
        const current = document.querySelector('[data-testid="page-' + (diagnostics.currentPage + 1) + '"]');
        const viewport = document.querySelector('[data-testid="document-viewport"]');
        const ready = [...(current?.querySelectorAll('[data-render-state="ready"]') ?? [])];
        const visibleCanvases = [...document.querySelectorAll('canvas')]
          .filter((canvas) => {
            const bounds = canvas.getBoundingClientRect();
            return bounds.right > 0 && bounds.bottom > 0
              && bounds.left < innerWidth && bounds.top < innerHeight;
          });
        return {
          diagnostics,
          no_op: noOp,
          promotion_timed_out: true,
          response_duration_ms: performance.now() - interactionStarted,
          render_page: perf.renderPage,
          image_visibility: visibility ?? null,
          rendered_device_pixel_ratio: viewport
            ? (${measureElectronVisibleRasterDensity.toString()})(
                current,
                viewport.getBoundingClientRect(),
                window.devicePixelRatio,
              )
            : 0,
          display_scale_factor: window.devicePixelRatio,
          presentation_current: ready.length > 0
            && ready.every((surface) => surface.getAttribute('data-render-quality') !== 'stale-preview'),
          visible_raster_resources: {
            count: visibleCanvases.length,
            max_pixels: Math.max(0, ...visibleCanvases.map((canvas) => canvas.width * canvas.height)),
          },
        };
      })()`);
      event("zoom-completed", {
        duration_ms: result.response_duration_ms,
        operation_wall_ms: performance.now() - started,
        zoom_percent: percent,
        visible_page_indices: result.diagnostics.visiblePageIndices,
        no_op: result.no_op,
        promotion_timed_out: result.promotion_timed_out,
        completion_basis:
          "zoom-state-current-raster-density-promotion-250ms-and-two-animation-frames",
        render_page: result.render_page,
        image_visibility: result.image_visibility,
        display_scale_factor: result.display_scale_factor,
        rendered_device_pixel_ratio: result.rendered_device_pixel_ratio,
        visible_raster_resources: result.visible_raster_resources,
      });
      zoomResults.push(result);
    }
    const zoomProbe = await cdp.evaluate(`(() => {
      window.__electronZoomProbeActive = false;
      return structuredClone(window.__electronZoomProbe);
    })()`);
    const checks = {
      "zoom-state-current": zoomResults.length === zoomSequence.length,
      "visible-tiles-bounded": zoomResults.every(
        ({ visible_raster_resources: resources }) =>
          resources.count <= 32 && resources.max_pixels <= 8192 * 8192,
      ),
      "preview-current-generation": zoomResults.every(
        ({ presentation_current: current }) => current === true,
      ),
      "settled-density-at-least-1": zoomResults.every(
        ({ rendered_device_pixel_ratio: ratio }) =>
          electronSettledDensityIsCurrent(ratio),
      ),
      "stale-generations-presented-zero":
        zoomProbe.frame_count > 0 &&
        zoomProbe.stale_visible_surface_frames === 0,
    };
    const commandResult = {
      command_id: command.id,
      observations: { zoom_results: zoomResults, generation_probe: zoomProbe },
      exact_fields: {
        presentation_current: checks["preview-current-generation"],
        stale_generations_presented_zero:
          checks["stale-generations-presented-zero"],
      },
      observed_milestones: [],
      milestones: [],
    };
    for (const milestone of command.expected_milestones) {
      addMilestone(
        event,
        commandResult,
        milestone,
        checks[milestone] === true,
        {
          zoom_results: zoomResults,
          generation_probe: zoomProbe,
        },
      );
    }
    finalizeCommandMilestones(commandResult, command);
    const exact =
      commandResult.manifest_milestones_complete &&
      Object.values(commandResult.exact_fields).every(Boolean);
    expandedComparison = {
      contract: options.comparisonScenarioContract,
      input_lane: cdpDiagnosticInputLane,
      command_results: [commandResult],
      exact_manifest_replay: exact,
      decision_timing_eligible: false,
      blocker: exact
        ? null
        : "Electron CDP zoom did not prove state, current presentation, density, and generation milestones.",
    };
  }

  if (matchedViewStateGate.claim("measurement-end")) {
    event(
      "comparison-view-state",
      await observeElectronComparisonViewState(cdp, options, "measurement-end"),
    );
  }
  if (!matchedViewStateGate.complete()) {
    throw new Error("comparison view-state checkpoints are incomplete");
  }

  return cdp
    .evaluate(
      `(async () => {
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    window.__electronNativeApplicationFrameAckCleanup?.();
    window.__electronPerfFrameActive = false;
    const processMetrics = await window.butterPaper.test.getProcessMetrics();
    return {
      diagnostics: window.__butterPaperTestHooks.getDiagnostics(),
      app_perf: window.__butterPaperTestHooks.getPerfSnapshot(),
      process_metrics: processMetrics,
      frame_intervals_ms: window.__electronPerfFrames,
      native_input_to_application_frame_ack: window.__electronNativeApplicationFrameAck,
      navigation: performance.getEntriesByType('navigation')[0]?.toJSON() ?? null,
    };
  })()`,
    )
    .then((evidence) => ({
      ...evidence,
      comparison_observation_role:
        options.inputLane === nativeX11InputLane
          ? "CDP read-only geometry, application state, DOM paint, and instrumentation observations; XTest supplies workload input"
          : "CDP diagnostic input and observations",
      expanded_comparison: expandedComparison,
      workload:
        options.scenario === "page-navigation"
          ? {
              page_sequence: normalizedPageSequence(
                ready.diagnostics.pageCount,
              ),
            }
          : options.scenario === "zoom"
            ? { zoom_sequence_percent: zoomSequence }
            : expandedComparison?.contract
              ? {
                  manifest_id: expandedComparison.contract.manifest_id,
                  fixture_id: expandedComparison.contract.fixture_id,
                  command_ids: expandedComparison.contract.command_ids,
                }
              : {},
    }));
}

async function runIteration(options, iteration) {
  const userDataDirectory = await mkdtemp(
    resolve(tmpdir(), "butter-paper-electron-perf-"),
  );
  const port = await availablePort();
  const events = [];
  const samples = [];
  let output = "";
  let sampleInProgress = false;
  let timedOut = false;
  let cdp;
  let browserCdp;
  let evidence;
  let browserMetrics;
  let domCounters;
  let heapUsage;
  let failure;
  let browserGpuInfo;
  let activeGpuAdapter;
  let cgroupMetrics;
  let gpuMetrics;
  let commonBenefitTimingBoundary = null;
  const gpuSampler = await startNvidiaBaselineRunSampler();
  const startedAt = new Date();
  const startedMonotonic = process.hrtime.bigint();
  const event = (name, fields = {}) =>
    events.push({
      schema_version: 1,
      runtime: "electron",
      scenario: options.scenario,
      event: name,
      t_ms: Number(process.hrtime.bigint() - startedMonotonic) / 1e6,
      ...fields,
    });
  const cgroup = await createLinuxCgroup(
    `electron-${process.pid}-${iteration}`,
  );
  const electronArguments = [`--remote-debugging-port=${port}`];
  if (platform() === "linux" && process.getuid?.() === 0) {
    // The disposable GPU host runs the benchmark as root so it can create an
    // isolated cgroup. Chromium refuses that development-only context unless
    // its sandbox is disabled. This does not qualify the packaged candidate.
    electronArguments.push("--no-sandbox");
  }
  electronArguments.push("apps/desktop");
  const launch = cgroupLaunch(cgroup, options.electron, electronArguments);
  gpuSampler.startRun();
  const child = spawn(launch.executable, launch.args, {
    cwd: repositoryDirectory,
    detached: true,
    env: {
      ...optimizedElectronBenchmarkEnvironment(process.env),
      BP_TEST_MODE: "1",
      BP_TEST_THEME: "light",
      BP_OPEN_SAMPLE_PDF: "0",
      BP_TEST_USER_DATA_DIR: userDataDirectory,
      // The tracked test bridge permits sources only below this explicit root.
      // Point it at the selected corpus directory without copying the 128 MB PDF.
      BP_TEST_FIXTURE_DIR: dirname(options.pdf),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const requireCommonDamageObserver =
    options.v6Scenario || process.env.BP_PERF_COMMON_DAMAGE_OBSERVER === "1";
  if (requireCommonDamageObserver) {
    beginX11DamageObserverCollection({ candidatePid: child.pid });
  }
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  const capture = (chunk) => {
    if (output.length < outputLimitBytes)
      output += chunk.slice(0, outputLimitBytes - output.length);
  };
  child.stdout.on("data", capture);
  child.stderr.on("data", capture);
  const sample = async () => {
    if (sampleInProgress) return;
    sampleInProgress = true;
    try {
      const snapshot = await sampleProcessTree(child.pid);
      if (snapshot)
        samples.push({
          elapsed_ms: Number(process.hrtime.bigint() - startedMonotonic) / 1e6,
          ...snapshot,
        });
    } catch (error) {
      samples.push({
        elapsed_ms: Number(process.hrtime.bigint() - startedMonotonic) / 1e6,
        sample_error: error.message,
      });
    } finally {
      sampleInProgress = false;
    }
  };
  await sample();
  const sampleTimer = setInterval(sample, sampleIntervalMs);
  sampleTimer.unref();
  let rejectTimeout;
  const timeoutPromise = new Promise((_, rejectPromise) => {
    rejectTimeout = rejectPromise;
  });
  const timeoutTimer = setTimeout(() => {
    timedOut = true;
    rejectTimeout(new Error(`Iteration exceeded ${options.timeoutMs} ms`));
  }, options.timeoutMs);
  timeoutTimer.unref();

  try {
    await Promise.race([
      (async () => {
        event("process-start");
        const target = await waitForTarget(
          port,
          child,
          output,
          Math.min(options.timeoutMs, 20_000),
        );
        event("cdp-target-ready");
        cdp = await createCdpClient(target.page.webSocketDebuggerUrl);
        await cdp.send("Runtime.enable");
        await cdp.send("Performance.enable");
        if (target.browserWebSocketDebuggerUrl) {
          browserCdp = await createCdpClient(
            target.browserWebSocketDebuggerUrl,
          );
          browserGpuInfo = await browserCdp.send("SystemInfo.getInfo");
          activeGpuAdapter = buildElectronActiveGpuAdapterReceipt(
            browserGpuInfo,
            options.provenance?.host?.nvidia_gpu,
          );
          browserGpuInfo = annotateElectronActiveGpuDevice(
            browserGpuInfo,
            activeGpuAdapter,
          );
        }
        evidence = await runRendererScenario(cdp, options, event, child.pid);
        if (requireCommonDamageObserver) {
          commonBenefitTimingBoundary = finishX11DamageObserverCollection();
        }
        if (options.v5ExecutionContext) {
          const v5Evidence = buildElectronV5ComponentEvidence(
            options.v5ExecutionContext,
            evidence.expanded_comparison?.command_results,
            evidence.expanded_comparison?.semantic_summary ?? null,
          );
          evidence = { ...evidence, v5_component_evidence: v5Evidence };
          event("v5-component-provenance", {
            protocol_version: options.v5ExecutionContext.protocol_version,
            scenario_contract_version:
              options.v5ExecutionContext.scenario_contract_version,
            manifest_id: options.v5ExecutionContext.manifest_id,
            parent_scenario: options.v5ExecutionContext.parent_scenario,
            component_scenario: options.v5ExecutionContext.component_scenario,
          });
          for (const receipt of v5Evidence.command_receipts) {
            event("v5-command-receipt", receipt);
          }
        }
        if (
          options.inputLane === nativeX11InputLane &&
          !options.v5ExecutionContext &&
          evidence.expanded_comparison?.exact_manifest_replay !== true
        ) {
          throw new Error(
            `${options.scenario} native X11 replay failed exact milestones, semantics, or timing`,
          );
        }
        if (options.v4ExecutionContext) {
          const v4Evidence = buildElectronV4ComponentEvidence(
            options.v4ExecutionContext,
            events,
            evidence.expanded_comparison,
          );
          evidence = { ...evidence, v4_parent_component_evidence: v4Evidence };
          event("v4-parent-provenance", options.v4ExecutionContext.provenance);
          for (const receipt of [
            ...v4Evidence.command_receipts,
            ...v4Evidence.parent_blocked_command_receipts,
          ]) {
            event("v4-command-receipt", receipt);
          }
          if (
            !v4Evidence.component_receipts_passed &&
            v4Evidence.known_baseline_defect_id !==
              electronEngineeringZoomBaselineDefectIdV6
          ) {
            throw new Error(
              `v4 parent ${options.v4Scenario} component ${options.scenario} has unmapped or incomplete exact command receipts`,
            );
          }
        }
        browserMetrics = Object.fromEntries(
          (await cdp.send("Performance.getMetrics")).metrics.map(
            ({ name, value }) => [name, value],
          ),
        );
        domCounters = await cdp.send("Memory.getDOMCounters");
        await cdp.send("HeapProfiler.collectGarbage");
        heapUsage = await cdp.send("Runtime.getHeapUsage");
        event("scenario-complete");
      })(),
      timeoutPromise,
    ]);
  } catch (error) {
    failure =
      error instanceof Error ? (error.stack ?? error.message) : String(error);
  } finally {
    abortX11DamageObserverCollection();
    clearTimeout(timeoutTimer);
    clearInterval(sampleTimer);
    cdp?.close();
    browserCdp?.close();
    terminateProcessGroup(child.pid, "SIGTERM");
    await waitForExit(child);
    cgroupMetrics = await readLinuxCgroup(cgroup);
    await removeLinuxCgroup(cgroup);
    gpuMetrics = await gpuSampler.stop();
    while (sampleInProgress) await delay(5);
    await rm(userDataDirectory, { recursive: true, force: true });
  }

  const validSamples = samples.filter((entry) => !entry.sample_error);
  const exactEvidencePassed = options.v5ExecutionContext
    ? evidence?.v5_component_evidence?.passed === true
    : options.v4ExecutionContext
      ? evidence?.v4_parent_component_evidence?.component_receipts_passed ===
          true ||
        evidence?.v4_parent_component_evidence?.known_baseline_defect_id ===
          electronEngineeringZoomBaselineDefectIdV6
      : comparisonMilestonesSucceeded(
          events,
          options.comparisonScenarioContract,
        );
  const nativeTargetVerified = events.some(
    (event) =>
      event.event === "native-window-target-verified" &&
      event.pid === child.pid &&
      /^\d+$/.test(String(event.window_id)),
  );
  const commonDamagePassed =
    commonBenefitTimingBoundary?.passed === true &&
    commonBenefitTimingBoundary?.decision_timing_eligible === true;
  activeGpuAdapter ??= buildElectronActiveGpuAdapterReceipt(
    browserGpuInfo,
    options.provenance?.host?.nvidia_gpu,
  );
  const activeGpuPassed =
    !activeGpuAdapterRequired() || activeGpuAdapter.passed === true;
  return {
    iteration,
    started_at: startedAt.toISOString(),
    ended_at: new Date().toISOString(),
    wall_duration_ms: Number(process.hrtime.bigint() - startedMonotonic) / 1e6,
    success:
      !failure &&
      !timedOut &&
      exactEvidencePassed &&
      electronGpuEvidencePassed(gpuMetrics) &&
      activeGpuPassed,
    timed_out: timedOut,
    failure:
      failure ??
      (!activeGpuPassed
        ? activeGpuAdapter.blocker
        : !electronGpuEvidencePassed(gpuMetrics)
          ? (gpuMetrics?.qualification?.blocker ??
            "required NVIDIA evidence is missing")
          : exactEvidencePassed
            ? null
            : options.v5ExecutionContext
              ? "exact v5 component command receipts were incomplete"
              : options.v4ExecutionContext
                ? "exact v4 parent command receipts were incomplete"
                : "exact comparison milestones were incomplete"),
    events,
    active_gpu_adapter: activeGpuAdapter,
    output,
    renderer: evidence
      ? {
          ...evidence,
          frame_summary: summarizeFrames(evidence.frame_intervals_ms),
          browser_metrics: browserMetrics,
          dom_counters: domCounters,
          heap_usage_after_gc: heapUsage,
          browser_gpu_info: browserGpuInfo,
        }
      : null,
    native_input:
      options.inputLane === nativeX11InputLane
        ? {
            input_lane: nativeX11InputLane,
            execution_status:
              !failure && nativeTargetVerified && commonDamagePassed
                ? "passed"
                : "failed",
            real_gui_run: nativeTargetVerified,
            decision_timing_eligible:
              !failure && nativeTargetVerified && commonDamagePassed,
            evidence: {
              success: !failure && nativeTargetVerified && commonDamagePassed,
              target_verified: nativeTargetVerified,
              common_benefit_timing_boundary: commonBenefitTimingBoundary,
            },
          }
        : {
            input_lane: cdpDiagnosticInputLane,
            execution_status: "semantic-diagnostic",
            real_gui_run: false,
            decision_timing_eligible: false,
          },
    samples,
    cgroup: cgroupMetrics,
    gpu: gpuMetrics,
    resource_summary: {
      sample_count: validSamples.length,
      cpu_percent: numericSummary(
        validSamples.map((entry) => entry.cpu_percent),
      ),
      rss_kb: numericSummary(validSamples.map((entry) => entry.rss_kb)),
      process_count: numericSummary(
        validSamples.map((entry) => entry.process_count),
      ),
    },
  };
}

function summarizeEvents(iterations, field) {
  const byEvent = new Map();
  for (const iteration of iterations) {
    for (const event of iteration.events) {
      if (!Number.isFinite(event[field])) continue;
      if (!byEvent.has(event.event)) byEvent.set(event.event, []);
      byEvent.get(event.event).push(event[field]);
    }
  }
  return Object.fromEntries(
    [...byEvent.entries()].map(([name, values]) => [
      name,
      numericSummary(values),
    ]),
  );
}

export function electronNativeApplicationAckSamples(iterations) {
  return iterations.flatMap((iteration) => {
    const observation =
      iteration.renderer?.native_input_to_application_frame_ack;
    if (
      observation?.physical_scanout_observed !== false ||
      observation?.receipt_scope !==
        "trusted-dom-native-event-receipt-to-next-request-animation-frame-callback-not-physical-scanout" ||
      observation.input_event_count <= 0 ||
      observation.acknowledged_event_count !== observation.input_event_count
    ) {
      return [];
    }
    return (observation.samples_ms ?? []).filter(
      (sample) => Number.isFinite(sample) && sample >= 0,
    );
  });
}

function summarizeReport(iterations) {
  const samples = iterations.flatMap((iteration) =>
    iteration.samples.filter((entry) => !entry.sample_error),
  );
  const frames = iterations.flatMap(
    (iteration) => iteration.renderer?.frame_intervals_ms ?? [],
  );
  const nativeApplicationAckSamples =
    electronNativeApplicationAckSamples(iterations);
  return {
    successful_iterations: iterations.filter((iteration) => iteration.success)
      .length,
    failed_iterations: iterations.filter((iteration) => !iteration.success)
      .length,
    wall_duration_ms: numericSummary(
      iterations.map((iteration) => iteration.wall_duration_ms),
    ),
    event_timestamps_ms: summarizeEvents(iterations, "t_ms"),
    duration_events_ms: summarizeEvents(iterations, "duration_ms"),
    frames: summarizeFrames(frames),
    application_frame_intervals_ms: numericSummary(frames),
    native_input_to_application_frame_ack_ms: numericSummary(
      nativeApplicationAckSamples,
    ),
    native_application_frame_acknowledgement_proxy: {
      receipt_scope:
        "trusted-dom-native-event-receipt-to-next-request-animation-frame-callback-not-physical-scanout",
      physical_scanout_observed: false,
      sample_count: nativeApplicationAckSamples.length,
    },
    process_tree: {
      cpu_percent: numericSummary(samples.map((entry) => entry.cpu_percent)),
      rss_kb: numericSummary(samples.map((entry) => entry.rss_kb)),
      peak_cpu_percent: samples.length
        ? Math.max(...samples.map((entry) => entry.cpu_percent))
        : null,
      peak_rss_kb: samples.length
        ? Math.max(...samples.map((entry) => entry.rss_kb))
        : null,
      cpu_seconds: numericSummary(
        iterations.map((iteration) => iteration.cgroup?.cpu_seconds),
      ),
      cgroup_memory_peak_bytes: numericSummary(
        iterations.map((iteration) => iteration.cgroup?.memory_peak_bytes),
      ),
    },
    ...summarizeNvidiaIterations(iterations),
  };
}

function delay(ms) {
  return new Promise((resolvePromise) => setTimeout(resolvePromise, ms));
}

export function optimizedElectronBenchmarkEnvironment(environment) {
  const benchmarkEnvironment = { ...environment };
  delete benchmarkEnvironment.VITE_DEV_SERVER_URL;
  delete benchmarkEnvironment.ELECTRON_RENDERER_URL;
  delete benchmarkEnvironment.BP_RENDERER_DEV_SERVER_URL;
  return {
    ...benchmarkEnvironment,
    NODE_ENV: "production",
    BP_DISABLE_RENDERER_DEV_SERVER: "1",
  };
}

async function main() {
  let options;
  let comparisonScenarioContract;
  let fixtureContract;
  let comparisonWorkloadV5;
  let comparisonWorkloadV6;
  try {
    options = parseElectronRunnerArguments(process.argv.slice(2));
    if (options.help) {
      process.stdout.write(usage());
      return;
    }
    if (options.v6Scenario) {
      const bytes = await readFile(v6WorkloadPath);
      comparisonWorkloadV6 = JSON.parse(bytes);
      options.v6ExecutionContext = createElectronV6ExecutionContext({
        workload: comparisonWorkloadV6,
        workloadByteSha256: createHash("sha256").update(bytes).digest("hex"),
        parentScenario: options.v6Scenario,
        componentScenario: options.scenario,
      });
    }
    let comparisonWorkload;
    if (options.v5Scenario || options.scenario === multiDocumentScenarioV5) {
      comparisonWorkloadV5 = await loadMaterializedComparisonWorkloadV5();
      const v5Errors = validateComparisonWorkloadV5(comparisonWorkloadV5);
      if (v5Errors.length > 0) {
        throw new Error(
          `invalid v5 comparison workload: ${v5Errors.join("; ")}`,
        );
      }
      const artifactHash =
        comparisonWorkloadArtifactHashV5(comparisonWorkloadV5);
      const byteHash = comparisonWorkloadByteHashV5(comparisonWorkloadV5);
      if (
        artifactHash !== v5MaterializedArtifactSha256 ||
        byteHash !== v5MaterializedByteSha256
      ) {
        throw new Error(
          `v5 materialized workload hash mismatch: artifact ${artifactHash}, bytes ${byteHash}`,
        );
      }
      const parentScenario = options.v5Scenario ?? multiDocumentScenarioV5;
      options.v5ExecutionContext = createElectronV5ExecutionContext({
        workload: comparisonWorkloadV5,
        parentScenario,
        componentScenario: options.scenario,
      });
      comparisonScenarioContract =
        options.v5ExecutionContext.execution_contract;
      fixtureContract = comparisonScenarioContract;
      comparisonWorkload = comparisonWorkloadV5;
    } else {
      comparisonWorkload = await loadComparisonWorkload();
      const comparisonErrors = validateComparisonWorkload(comparisonWorkload);
      if (comparisonErrors.length > 0) {
        throw new Error(
          `invalid comparison workload: ${comparisonErrors.join("; ")}`,
        );
      }
      comparisonScenarioContract = buildDevelopmentScenarioContract(
        comparisonWorkload,
        options.scenario,
        options.inputLane === nativeX11InputLane
          ? nativeX11InputLane
          : "semantic-diagnostic",
      );
      fixtureContract = comparisonScenarioContract;
    }
    if (options.scenario === "persistence-workload") {
      comparisonScenarioContract = electronPersistenceSubcontract(
        comparisonScenarioContract,
      );
    }
    if (options.v4Scenario) {
      const v4Workload = await loadComparisonWorkloadV4();
      const v4Errors = validateComparisonWorkloadV4(v4Workload);
      if (v4Errors.length > 0) {
        throw new Error(
          `invalid v4 comparison workload: ${v4Errors.join("; ")}`,
        );
      }
      const v4ParentContract = buildScenarioContractV4(
        v4Workload,
        options.v4Scenario,
      );
      options.v4ExecutionContext = createElectronV4ExecutionContext({
        parentContract: v4ParentContract,
        parentWorkload: v4Workload,
        componentScenario: options.scenario,
        componentContract: comparisonScenarioContract,
        inputLane: options.inputLane,
      });
      comparisonScenarioContract =
        options.v4ExecutionContext.execution_contract;
      fixtureContract = v4ParentContract;
    } else if (!options.v5ExecutionContext) {
      fixtureContract = comparisonScenarioContract;
    }
    const pdfs = await Promise.all(options.pdfs.map(fileProvenance));
    const pdf = pdfs[0];
    const provenance = await collectProvenance(options.electron);
    options.provenance = provenance;
    const fixtureError = options.v5ExecutionContext
      ? validateOrderedElectronScenarioFixtures(pdfs, fixtureContract)
      : validateExpandedScenarioFixture(pdf, fixtureContract);
    if (fixtureError) throw new Error(fixtureError);
    if (new Set(options.pdfs.map((path) => dirname(path))).size !== 1) {
      throw new Error(
        "all Electron v5 PDF fixtures must share one authorized fixture directory",
      );
    }
    if (options.v5ExecutionContext) {
      const capability = electronV5MaintainedUiCapability(options.scenario);
      options.v5ExecutionContext = {
        ...options.v5ExecutionContext,
        maintained_ui_capability: capability,
      };
    }
    options.comparisonScenarioContract = comparisonScenarioContract;
    options.comparisonWorkload = comparisonWorkload;
    options.pdfProvenance = pdfs;
    const iterations = [];
    for (let iteration = 1; iteration <= options.iterations; iteration += 1) {
      process.stderr.write(
        `Electron ${options.scenario}: iteration ${iteration}/${options.iterations}\n`,
      );
      iterations.push(await runIteration(options, iteration));
    }
    const report = {
      schema_version: 1,
      protocol_version: options.v6ExecutionContext
        ? protocolVersionV6
        : options.v5ExecutionContext
          ? protocolVersionV5
          : protocolVersion,
      scenario_contract_version: options.v6ExecutionContext
        ? scenarioContractVersionV6
        : options.v5ExecutionContext
          ? scenarioContractVersionV5
          : scenarioContractVersion,
      implementation: "electron",
      scenario: options.scenario,
      requested_iterations: options.iterations,
      timeout_ms_per_iteration: options.timeoutMs,
      cache_class: "app-cold",
      comparison_workload: options.v6ExecutionContext
        ? {
            manifest_id: comparisonWorkloadV6.manifest_id,
            byte_sha256: options.v6ExecutionContext.workload_byte_sha256,
            scenario_contract_version:
              options.v6ExecutionContext.scenario_contract_version,
            fixture_ids:
              options.v6ExecutionContext.execution_contract.fixture_ids,
            benefit_metrics_eligible: true,
          }
        : options.v5ExecutionContext
          ? {
              manifest_id: comparisonWorkload.manifest_id,
              artifact_sha256:
                options.v5ExecutionContext.workload_artifact_sha256,
              byte_sha256: options.v5ExecutionContext.workload_byte_sha256,
              scenario_contract_version:
                options.v5ExecutionContext.scenario_contract_version,
              fixture_ids:
                options.v5ExecutionContext.execution_contract.fixture_ids,
              benefit_metrics_eligible:
                options.v5ExecutionContext.benefit_metrics_eligible,
            }
          : electronComparisonMetadata(
              comparisonWorkload,
              options.scenario,
              options.inputLane,
              iterations,
            ),
      ...(options.v4ExecutionContext
        ? {
            v4_parent_execution: {
              ...options.v4ExecutionContext.provenance,
              component_receipts_passed:
                iterations.length > 0 &&
                iterations.every(
                  (iteration) =>
                    iteration.renderer?.v4_parent_component_evidence
                      ?.component_receipts_passed === true,
                ),
              known_baseline_defect_id:
                iterations.length > 0 &&
                iterations.every(
                  (iteration) =>
                    iteration.renderer?.v4_parent_component_evidence
                      ?.known_baseline_defect_id ===
                    electronEngineeringZoomBaselineDefectIdV6,
                )
                  ? electronEngineeringZoomBaselineDefectIdV6
                  : null,
              command_receipts_by_iteration: iterations.map((iteration) => ({
                iteration: iteration.iteration,
                receipts:
                  iteration.renderer?.v4_parent_component_evidence
                    ?.command_receipts ?? [],
                parent_blocked_receipts:
                  iteration.renderer?.v4_parent_component_evidence
                    ?.parent_blocked_command_receipts ?? [],
              })),
            },
          }
        : {}),
      ...(options.v5ExecutionContext
        ? {
            comparison_v5: {
              ...options.v5ExecutionContext,
              execution_eligible:
                iterations.length > 0 &&
                iterations.every(
                  (iteration) =>
                    iteration.renderer?.v5_component_evidence?.passed === true,
                ),
              execution_blocker:
                iterations.length > 0 &&
                iterations.every(
                  (iteration) =>
                    iteration.renderer?.v5_component_evidence?.passed === true,
                )
                  ? null
                  : "exact live Electron v5 component receipt failed",
              iterations: iterations.map(
                (iteration) =>
                  iteration.renderer?.v5_component_evidence ?? null,
              ),
            },
          }
        : {}),
      ...(options.v6ExecutionContext
        ? {
            comparison_v6: {
              ...options.v6ExecutionContext,
              inherited_v4_execution: true,
              execution_eligible:
                iterations.length > 0 &&
                iterations.every(
                  (iteration) =>
                    iteration.renderer?.v4_parent_component_evidence
                      ?.component_receipts_passed === true,
                ),
            },
          }
        : {}),
      workload:
        iterations.find((iteration) => iteration.success)?.renderer?.workload ??
        {},
      pdf,
      pdfs: options.pdfProvenance,
      provenance,
      summary: summarizeReport(iterations),
      iterations,
    };
    await mkdir(dirname(options.output), { recursive: true });
    await writeFile(
      options.output,
      `${JSON.stringify(report, null, 2)}\n`,
      "utf8",
    );
    process.stderr.write(`Wrote ${options.output}\n`);
    if (report.summary.failed_iterations > 0) process.exitCode = 1;
  } catch (error) {
    process.stderr.write(
      `electron-runner: ${formatFixtureAccessError(error, fixtureContract ?? comparisonScenarioContract, options?.pdf)}\n\n${usage()}`,
    );
    process.exitCode = 2;
  }
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
) {
  await main();
}
