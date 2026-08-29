import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

export const comparisonWorkloadSchemaVersion = "bp-comparison-workload-v1";
export const comparisonWorkloadManifestId = "bp-perf-v3-decision-3";

const defaultWorkloadUrl = new URL("./comparison-workload.json", import.meta.url);
const requiredFixtureIds = Object.freeze([
  "bp-single-page-v1",
  "bp-multi-page-v1",
  "bp-annotation-density-v1",
  "bp-annotation-all-v1",
  "nasa-apollo-summary-526-v1",
  "usgs-usa-geology-sheet-v1",
]);
const requiredJourneyIds = Object.freeze([
  "viewer-v1",
  "rectangle-v1",
  "highlight-v1",
  "text-v1",
  "length-v1",
  "image-v1",
  "unknown-preservation-v1",
  "save-reopen-v1",
]);

const runnerCapabilities = Object.freeze({
  electron: Object.freeze({
    supported_command_ids: Object.freeze([
      "viewer:launch-cold",
      "viewer:open-each",
      "viewer:layout-single",
      "viewer:layout-continuous",
      "viewer:navigate-normalized",
      "viewer:zoom-sequence",
      "viewer:continuous-scroll",
      "viewer:close-recover-reopen",
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
      "unknown:import",
      "unknown:assert-cycle-1",
      "unknown:assert-cycle-2",
      "persistence:apply-fixed-state",
      "persistence:save-1",
      "persistence:reopen-1",
      "persistence:save-2",
      "persistence:reopen-2",
    ]),
    supported_scenarios: Object.freeze([
      "open-pdf",
      "viewer-layout",
      "page-navigation",
      "zoom",
      "close-reopen",
      "annotation-create",
      "annotation-transform",
      "annotation-properties-history",
      "editor-create",
      "continuous-scroll",
      "editor-workload",
      "persistence-workload",
    ]),
    supported_operations: Object.freeze([
      "app.launch-cold",
      "document.open",
      "viewer.set-layout",
      "viewer.navigate-normalized-sequence",
      "viewer.zoom-sequence",
      "viewer.continuous-scroll-path",
      "document.close-recover-reopen",
      "annotation.rectangle.create",
      "annotation.rectangle.transform",
      "annotation.rectangle.properties-history",
      "annotation.rectangle.repeat-on-dense-page",
      "annotation.highlight.create",
      "annotation.highlight.edit-history",
      "annotation.text.create",
      "annotation.text.edit-resize-history",
      "measurement.set-scale",
      "annotation.length.create",
      "annotation.length.edit-endpoint-history",
      "annotation.image.create",
      "annotation.image.resize-history",
      "annotation.unknown.import-untouched",
      "annotation.unknown.assert-preserved",
      "persistence.apply-canonical-annotation-state",
      "persistence.safe-save",
      "persistence.reopen-and-compare",
    ]),
    supported_milestones: Object.freeze([
      "process-started",
      "native-window-presented",
      "interactive-shell",
      "document-opened",
      "preview-current-generation",
      "target-page-current",
      "zoom-state-current",
      "visible-tiles-bounded",
      "settled-current-generation-250ms",
      "settled-density-at-least-1",
      "per-page-geometry-matched",
      "annotation-thumbnail-current",
      "timestamped-input-complete",
      "virtual-page-window-bounded",
      "finish-page-current",
      "blank-current-generation-frames-zero",
      "document-resources-released",
      "memory-recovery-recorded",
      "document-reopened",
      "pointer-stream-received",
      "gesture-committed-once",
      "hit-test-selected",
      "move-committed-once",
      "resize-committed-once",
      "properties-current",
      "locked-edit-rejected",
      "undo-redo-exact",
      "dirty-current",
      "annotation-painted",
      "thumbnail-current",
      "path-smoothed",
      "path-bounds-current",
      "text-input-committed",
      "text-shaped",
      "selection-current",
      "layout-current",
      "font-persistence-recorded",
      "measurement-scale-current",
      "derived-length-exact",
      "label-layout-current",
      "control-point-current",
      "bitmap-decoded",
      "aspect-ratio-current",
      "spatial-index-work-recorded",
      "annotation-paint-work-recorded",
      "unknown-dictionary-snapshotted",
      "unknown-appearance-stream-snapshotted",
      "unknown-dictionary-exact",
      "unknown-appearance-stream-exact",
      "canonical-state-matched",
      "safe-publication-complete",
      "independent-pdf-validation-passed",
      "native-annotations-valid",
      "appearance-streams-valid",
      "page-content-preserved",
      "page-boxes-rotation-metadata-preserved",
      "fixed-crops-matched",
      "dirty-published",
    ]),
  }),
  gpui: Object.freeze({
    supported_command_ids: Object.freeze([
      "viewer:launch-cold",
      "viewer:open-each",
      "viewer:layout-single",
      "viewer:layout-continuous",
      "viewer:navigate-normalized",
      "viewer:zoom-sequence",
      "viewer:continuous-scroll",
      "viewer:cache-pressure",
      "viewer:close-recover-reopen",
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
      "unknown:import",
      "unknown:assert-cycle-1",
      "unknown:assert-cycle-2",
      "persistence:apply-fixed-state",
      "persistence:save-1",
      "persistence:reopen-1",
      "persistence:save-2",
      "persistence:reopen-2",
    ]),
    supported_scenarios: Object.freeze([
      "open-pdf",
      "viewer-layout",
      "page-navigation",
      "zoom",
      "high-zoom-pan",
      "cache-pressure",
      "close-reopen",
      "annotation-create",
      "annotation-transform",
      "annotation-properties-history",
      "editor-create",
      "continuous-scroll",
      "editor-workload",
      "persistence-workload",
    ]),
    supported_operations: Object.freeze([
      "app.launch-cold",
      "document.open",
      "viewer.set-layout",
      "viewer.navigate-normalized-sequence",
      "viewer.zoom-sequence",
      "viewer.pan-path",
      "viewer.continuous-scroll-path",
      "viewer.cache-pressure-cycles",
      "document.close-recover-reopen",
      "annotation.rectangle.create",
      "annotation.rectangle.transform",
      "annotation.rectangle.properties-history",
      "annotation.rectangle.repeat-on-dense-page",
      "annotation.highlight.create",
      "annotation.highlight.edit-history",
      "annotation.text.create",
      "annotation.text.edit-resize-history",
      "measurement.set-scale",
      "annotation.length.create",
      "annotation.length.edit-endpoint-history",
      "annotation.image.create",
      "annotation.image.resize-history",
      "annotation.unknown.import-untouched",
      "annotation.unknown.assert-preserved",
      "persistence.apply-canonical-annotation-state",
      "persistence.safe-save",
      "persistence.reopen-and-compare",
    ]),
    supported_milestones: Object.freeze([
      "process-started",
      "native-window-presented",
      "interactive-shell",
      "document-opened",
      "preview-current-generation",
      "target-page-current",
      "zoom-state-current",
      "per-page-geometry-matched",
      "annotation-thumbnail-current",
      "virtual-page-window-bounded",
      "visible-tiles-bounded",
      "settled-current-generation-250ms",
      "settled-density-at-least-1",
      "stale-generations-presented-zero",
      "timestamped-input-complete",
      "finish-page-current",
      "blank-current-generation-frames-zero",
      "declared-cache-byte-limit-held",
      "decoded-byte-limit-held",
      "document-resources-released",
      "memory-recovery-recorded",
      "document-reopened",
      "pointer-stream-received",
      "gesture-committed-once",
      "hit-test-selected",
      "move-committed-once",
      "resize-committed-once",
      "properties-current",
      "locked-edit-rejected",
      "undo-redo-exact",
      "dirty-current",
      "annotation-painted",
      "thumbnail-current",
      "path-smoothed",
      "path-bounds-current",
      "text-input-committed",
      "text-shaped",
      "selection-current",
      "layout-current",
      "font-persistence-recorded",
      "measurement-scale-current",
      "derived-length-exact",
      "label-layout-current",
      "control-point-current",
      "bitmap-decoded",
      "bitmap-upload-recorded",
      "aspect-ratio-current",
      "upload-byte-count-recorded",
      "spatial-index-work-recorded",
      "annotation-paint-work-recorded",
      "canonical-state-matched",
      "unknown-dictionary-snapshotted",
      "unknown-appearance-stream-snapshotted",
      "unknown-dictionary-exact",
      "unknown-appearance-stream-exact",
      "safe-publication-complete",
      "independent-pdf-validation-passed",
      "native-annotations-valid",
      "appearance-streams-valid",
      "page-content-preserved",
      "page-boxes-rotation-metadata-preserved",
      "fixed-crops-matched",
      "dirty-published",
    ]),
  }),
});

const explicitScenarioBlockers = Object.freeze({
  electron: Object.freeze({
    "high-zoom-pan": "electron-high-zoom-pan-live-proof-missing",
    "cache-pressure": "electron-cache-pressure-live-semantic-and-gpu-upload-proof-missing",
  }),
});

function canonicalize(value) {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonicalize(value[key])]));
  }
  return value;
}

function canonicalJson(value) {
  return JSON.stringify(canonicalize(value));
}

function sha256(value) {
  return createHash("sha256").update(canonicalJson(value)).digest("hex");
}

function commandStream(workload) {
  return (workload?.journeys ?? []).flatMap((journey) =>
    (journey.commands ?? []).map((command) => ({ journey_id: journey.id, command })),
  );
}

function milestoneStream(workload) {
  return commandStream(workload).map(({ journey_id, command }) => ({
    journey_id,
    command_id: command.id,
    milestones: command.expected_milestones,
  }));
}

export async function loadComparisonWorkload(path = fileURLToPath(defaultWorkloadUrl)) {
  return JSON.parse(await readFile(path, "utf8"));
}

export function comparisonWorkloadHashes(workload) {
  return {
    command_stream_sha256: sha256(commandStream(workload)),
    milestone_stream_sha256: sha256(milestoneStream(workload)),
    expected_state_sha256: sha256(workload?.expected),
  };
}

export function comparisonWorkloadArtifactHash(workload) {
  return sha256(workload);
}

export function validateComparisonWorkload(workload) {
  const errors = [];
  if (workload?.schema_version !== comparisonWorkloadSchemaVersion) {
    errors.push(`schema_version must be ${comparisonWorkloadSchemaVersion}`);
  }
  if (workload?.manifest_id !== comparisonWorkloadManifestId) {
    errors.push(`manifest_id must be ${comparisonWorkloadManifestId}`);
  }
  if (workload?.decision_contract_version !== comparisonWorkloadManifestId) {
    errors.push(`decision_contract_version must be ${comparisonWorkloadManifestId}`);
  }
  if (workload?.coordinate_space !== "pdf-points-bottom-left") {
    errors.push("coordinate_space must be pdf-points-bottom-left");
  }

  const fixtureIds = new Set((workload?.fixtures ?? []).map(({ id }) => id));
  for (const fixtureId of requiredFixtureIds) {
    if (!fixtureIds.has(fixtureId)) errors.push(`missing required fixture ${fixtureId}`);
  }

  const journeys = new Map((workload?.journeys ?? []).map((journey) => [journey.id, journey]));
  for (const journeyId of requiredJourneyIds) {
    if (!journeys.has(journeyId)) errors.push(`missing required journey ${journeyId}`);
  }

  const commandIds = new Set();
  for (const journey of workload?.journeys ?? []) {
    if (!Array.isArray(journey.commands) || journey.commands.length === 0) {
      errors.push(`${journey.id ?? "unnamed journey"} must contain commands`);
      continue;
    }
    for (const command of journey.commands) {
      if (typeof command.id !== "string" || typeof command.operation !== "string") {
        errors.push(`${journey.id} has a command without string id and operation`);
        continue;
      }
      if (commandIds.has(command.id)) errors.push(`duplicate command id ${command.id}`);
      commandIds.add(command.id);
      if (!Array.isArray(command.expected_milestones) || command.expected_milestones.length === 0) {
        errors.push(`${command.id} must declare expected_milestones`);
      }
    }
  }

  const rectangleCreate = commandStream(workload)
    .find(({ command }) => command.id === "rectangle:create-sparse")?.command;
  if (rectangleCreate?.pointer_path?.rate_hz !== 120 || rectangleCreate?.pointer_path?.duration_ms !== 3000) {
    errors.push("rectangle:create-sparse must freeze a 3 second 120 Hz pointer path");
  }
  const saveCycles = commandStream(workload)
    .filter(({ command }) => command.operation === "persistence.safe-save")
    .map(({ command }) => command.cycle);
  const reopenCycles = commandStream(workload)
    .filter(({ command }) => command.operation === "persistence.reopen-and-compare")
    .map(({ command }) => command.cycle);
  if (canonicalJson(saveCycles) !== "[1,2]" || canonicalJson(reopenCycles) !== "[1,2]") {
    errors.push("save-reopen-v1 must freeze exactly two save and reopen cycles");
  }

  const actualHashes = comparisonWorkloadHashes(workload);
  for (const [name, actual] of Object.entries(actualHashes)) {
    if (workload?.canonical_hashes?.[name] !== actual) {
      errors.push(`${name} does not match the canonical ${name.replace("_sha256", "").replaceAll("_", " ")}`);
    }
  }
  return errors;
}

export function buildFeatureCoverageReport(workload, capabilities) {
  if (!capabilities || typeof capabilities.implementation !== "string") {
    throw new Error("coverage capabilities must name an implementation");
  }
  const supportedOperations = new Set(capabilities.supported_operations ?? []);
  const supportedMilestones = new Set(capabilities.supported_milestones ?? []);
  const supportedCommandIds = capabilities.supported_command_ids
    ? new Set(capabilities.supported_command_ids)
    : null;
  const blockedCommands = [];
  let recognizedOperationCount = 0;
  let readyCommandCount = 0;
  const journeys = (workload.journeys ?? []).map((journey) => {
    const commands = journey.commands.map((command) => {
      if (supportedCommandIds && !supportedCommandIds.has(command.id)) {
        const blocked = {
          journey_id: journey.id,
          command_id: command.id,
          operation: command.operation,
          reason: "unsupported-exact-command",
        };
        blockedCommands.push(blocked);
        return { ...blocked, status: "blocked" };
      }
      if (!supportedOperations.has(command.operation)) {
        const blocked = {
          journey_id: journey.id,
          command_id: command.id,
          operation: command.operation,
          reason: "unsupported-operation",
        };
        blockedCommands.push(blocked);
        return { ...blocked, status: "blocked" };
      }
      recognizedOperationCount += 1;
      const missingMilestones = command.expected_milestones.filter(
        (milestone) => !supportedMilestones.has(milestone),
      );
      if (missingMilestones.length > 0) {
        const blocked = {
          journey_id: journey.id,
          command_id: command.id,
          operation: command.operation,
          reason: "unsupported-milestones",
          missing_milestones: missingMilestones,
        };
        blockedCommands.push(blocked);
        return { ...blocked, status: "blocked" };
      }
      readyCommandCount += 1;
      return {
        journey_id: journey.id,
        command_id: command.id,
        operation: command.operation,
        status: "supported",
      };
    });
    return {
      id: journey.id,
      status: commands.every(({ status }) => status === "supported") ? "supported" : "blocked",
      commands,
    };
  });
  const commandCount = commandStream(workload).length;
  const ready = commandCount > 0 && readyCommandCount === commandCount;
  return {
    schema_version: 1,
    manifest_id: workload.manifest_id,
    implementation: capabilities.implementation,
    status: ready ? "ready" : "blocked",
    ready,
    command_count: commandCount,
    recognized_operation_count: recognizedOperationCount,
    ready_command_count: readyCommandCount,
    blocked_command_count: blockedCommands.length,
    journeys,
    blocked_commands: blockedCommands,
  };
}

export function runnerComparisonMetadata(workload, implementation, scenario) {
  const capabilities = runnerCapabilities[implementation];
  if (!capabilities) throw new Error(`unknown comparison implementation ${implementation}`);
  const scenarioSupported = capabilities.supported_scenarios.includes(scenario);
  const blockedReason = explicitScenarioBlockers[implementation]?.[scenario]
    ?? "runner-does-not-implement-scenario";
  return {
    manifest_id: workload.manifest_id,
    manifest_artifact_sha256: comparisonWorkloadArtifactHash(workload),
    manifest_hashes: comparisonWorkloadHashes(workload),
    execution_lane: ["annotation-create", "annotation-properties-history", "editor-create", "continuous-scroll", "editor-workload", "persistence-workload"].includes(scenario)
      ? implementation === "electron" ? "cdp-input-diagnostic" : "semantic-diagnostic"
      : "development-subset",
    scenario_status: scenarioSupported ? "supported-diagnostic" : "blocked-unsupported",
    diagnostic_timing_eligible: scenarioSupported,
    decision_timing_eligible: false,
    blocked_reason: scenarioSupported ? null : blockedReason,
    feature_coverage: buildFeatureCoverageReport(workload, {
      implementation,
      supported_operations: capabilities.supported_operations,
      supported_milestones: capabilities.supported_milestones,
      supported_command_ids: capabilities.supported_command_ids,
    }),
  };
}

export function currentRunnerCoverageReport(workload) {
  const implementations = Object.keys(runnerCapabilities).map((implementation) =>
    buildFeatureCoverageReport(workload, {
      implementation,
      supported_operations: runnerCapabilities[implementation].supported_operations,
      supported_milestones: runnerCapabilities[implementation].supported_milestones,
      supported_command_ids: runnerCapabilities[implementation].supported_command_ids,
    }),
  );
  return {
    schema_version: 1,
    manifest_id: workload.manifest_id,
    manifest_artifact_sha256: comparisonWorkloadArtifactHash(workload),
    status: implementations.every(({ ready }) => ready) ? "ready" : "blocked",
    implementations,
  };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const workload = await loadComparisonWorkload();
  const validationErrors = validateComparisonWorkload(workload);
  if (validationErrors.length > 0) {
    process.stderr.write(`${validationErrors.join("\n")}\n`);
    process.exitCode = 2;
  } else {
    const report = currentRunnerCoverageReport(workload);
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    if (report.status !== "ready") process.exitCode = 1;
  }
}
