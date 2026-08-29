import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  comparisonWorkloadArtifactHashV4,
  loadMaterializedComparisonWorkloadV4,
} from "./comparison-workload-v4.mjs";
import {
  gpuiMultiDocumentAbsoluteSafetyBudgetsV5,
  representativeJourneyIdsV5,
} from "./decision-contract-v5.mjs";
import {
  crossEngineScanFidelityAlgorithmV2,
  crossEngineScanFidelityParametersV2,
} from "./scan-fidelity-v2.mjs";

export const comparisonWorkloadSchemaVersionV5 = "bp-comparison-workload-v3";
export const comparisonWorkloadManifestIdV5 = "bp-perf-v5-decision-1";
export const sourceWorkloadArtifactSha256V5 =
  "4a826bd3c19f3c7693128961f43064e5b7414e799a0f791b7f3381ed59e897b1";
export const sourceWorkloadByteSha256V5 =
  "8828be7c4c7c05a19007bd315c4fc1844a93278c967d49614387c2ae6cfeff52";

const defaultDescriptorUrl = new URL(
  "./comparison-workload-v5.json",
  import.meta.url,
);
const defaultMaterializedUrl = new URL(
  "./comparison-workload-v5.materialized.json",
  import.meta.url,
);
const defaultV4MaterializedUrl = new URL(
  "./comparison-workload-v4.materialized.json",
  import.meta.url,
);
const sha256Pattern = /^[0-9a-f]{64}$/;

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

function canonicalJson(value) {
  return JSON.stringify(canonicalize(value));
}

function sha256Canonical(value) {
  return createHash("sha256").update(canonicalJson(value)).digest("hex");
}

function sha256Bytes(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function representativeCommandStream(workload) {
  return workload.journeys.flatMap((journey) =>
    journey.commands.map((command) => ({ journey_id: journey.id, command })),
  );
}

function representativeMilestoneStream(workload) {
  return representativeCommandStream(workload).map(
    ({ journey_id: journeyId, command }) => ({
      journey_id: journeyId,
      command_id: command.id,
      milestones: command.expected_milestones,
    }),
  );
}

function commandFixtureIds(journey, command) {
  if (Array.isArray(command.fixture_ids)) return [...command.fixture_ids];
  if (typeof command.fixture_id === "string") return [command.fixture_id];
  return [...journey.fixtures];
}

export function representativeFixtureCommandMappingV5(workload) {
  return workload.journeys.flatMap((journey) =>
    journey.commands.map((command) => ({
      journey_id: journey.id,
      command_id: command.id,
      fixture_ids: commandFixtureIds(journey, command),
    })),
  );
}

function stressCommandStream(workload) {
  return workload.stress_lanes.flatMap((lane) =>
    lane.commands.map((command) => ({ stress_lane_id: lane.id, command })),
  );
}

export function cropRegistrationArtifactV5(crop) {
  const { registration_sha256: _registrationSha256, ...registration } = crop;
  return registration;
}

export function cropRegistrationHashV5(crop) {
  return sha256Canonical(cropRegistrationArtifactV5(crop));
}

export function comparisonWorkloadHashesV5(workload) {
  return {
    representative_command_stream_sha256: sha256Canonical(
      representativeCommandStream(workload),
    ),
    representative_milestone_stream_sha256: sha256Canonical(
      representativeMilestoneStream(workload),
    ),
    representative_fixture_command_mapping_sha256: sha256Canonical(
      representativeFixtureCommandMappingV5(workload),
    ),
    stress_command_stream_sha256: sha256Canonical(
      stressCommandStream(workload),
    ),
    expected_state_sha256: sha256Canonical(workload.expected),
  };
}

export function comparisonWorkloadArtifactHashV5(workload) {
  return sha256Canonical(workload);
}

export function comparisonWorkloadBytesV5(workload) {
  return Buffer.from(`${JSON.stringify(workload, null, 2)}\n`);
}

export function comparisonWorkloadByteHashV5(workload) {
  return sha256Bytes(comparisonWorkloadBytesV5(workload));
}

async function loadFrozenV4Source() {
  const sourceBytes = await readFile(defaultV4MaterializedUrl);
  const observedByteHash = sha256Bytes(sourceBytes);
  if (observedByteHash !== sourceWorkloadByteSha256V5) {
    throw new Error(
      `v5 source workload byte hash changed: expected ${sourceWorkloadByteSha256V5}, got ${observedByteHash}`,
    );
  }
  const source = await loadMaterializedComparisonWorkloadV4(
    fileURLToPath(defaultV4MaterializedUrl),
  );
  const observedArtifactHash = comparisonWorkloadArtifactHashV4(source);
  if (observedArtifactHash !== sourceWorkloadArtifactSha256V5) {
    throw new Error(
      `v5 source workload artifact hash changed: expected ${sourceWorkloadArtifactSha256V5}, got ${observedArtifactHash}`,
    );
  }
  return source;
}

export async function loadComparisonWorkloadV5(
  descriptorPath = fileURLToPath(defaultDescriptorUrl),
) {
  const descriptor = JSON.parse(await readFile(descriptorPath, "utf8"));
  const source = await loadFrozenV4Source();
  if (
    descriptor.source_manifest.artifact_sha256 !==
      sourceWorkloadArtifactSha256V5 ||
    descriptor.source_manifest.byte_sha256 !== sourceWorkloadByteSha256V5
  ) {
    throw new Error("v5 descriptor does not pin the frozen v4 workload bytes");
  }

  const additionalCommands = new Map(
    descriptor.additional_commands.map((command) => [command.id, command]),
  );
  const journeys = structuredClone(source.journeys);
  for (const extension of descriptor.journey_extensions) {
    const journey = journeys.find(({ id }) => id === extension.journey_id);
    if (!journey) {
      throw new Error(
        `v5 extension references unknown journey ${extension.journey_id}`,
      );
    }
    for (const commandId of extension.append_command_ids) {
      const command = additionalCommands.get(commandId);
      if (!command) {
        throw new Error(`v5 extension references unknown command ${commandId}`);
      }
      journey.commands.push(structuredClone(command));
    }
  }
  for (const additionalJourney of descriptor.additional_journeys) {
    journeys.push({
      id: additionalJourney.id,
      fixtures: [...additionalJourney.fixture_ids],
      commands: additionalJourney.command_ids.map((commandId) => {
        const command = additionalCommands.get(commandId);
        if (!command) {
          throw new Error(
            `v5 journey ${additionalJourney.id} references unknown command ${commandId}`,
          );
        }
        return structuredClone(command);
      }),
    });
  }

  return {
    schema_version: descriptor.schema_version,
    manifest_id: descriptor.manifest_id,
    decision_contract_version: descriptor.decision_contract_version,
    supersedes: descriptor.supersedes,
    seed: descriptor.seed,
    coordinate_space: source.coordinate_space,
    environment: structuredClone(source.environment),
    source_manifest: structuredClone(descriptor.source_manifest),
    fixtures: structuredClone(source.fixtures),
    assets: structuredClone(source.assets),
    journeys,
    stress_lanes: structuredClone(source.stress_lanes),
    supplementary_lanes: structuredClone(source.supplementary_lanes),
    expected: {
      ...structuredClone(source.expected),
      v5_hard_component_rules: structuredClone(descriptor.expected_extensions),
    },
    canonical_hashes: structuredClone(descriptor.canonical_hashes),
  };
}

export async function loadMaterializedComparisonWorkloadV5(
  materializedPath = fileURLToPath(defaultMaterializedUrl),
) {
  return JSON.parse(await readFile(materializedPath, "utf8"));
}

export async function materializeComparisonWorkloadV5(
  outputPath = fileURLToPath(defaultMaterializedUrl),
) {
  const workload = await loadComparisonWorkloadV5();
  const bytes = comparisonWorkloadBytesV5(workload);
  await writeFile(outputPath, bytes);
  return {
    output_path: resolve(outputPath),
    bytes: bytes.length,
    byte_sha256: sha256Bytes(bytes),
    artifact_sha256: comparisonWorkloadArtifactHashV5(workload),
  };
}

function commandById(workload, commandId) {
  return representativeCommandStream(workload).find(
    ({ command }) => command.id === commandId,
  )?.command;
}

function exactStringArray(actual, expected) {
  return JSON.stringify(actual) === JSON.stringify(expected);
}

export function validateComparisonWorkloadV5(workload) {
  const errors = [];
  if (workload?.schema_version !== comparisonWorkloadSchemaVersionV5) {
    errors.push(`schema_version must be ${comparisonWorkloadSchemaVersionV5}`);
  }
  if (
    workload?.manifest_id !== comparisonWorkloadManifestIdV5 ||
    workload?.decision_contract_version !== comparisonWorkloadManifestIdV5
  ) {
    errors.push(
      `manifest and decision contract must be ${comparisonWorkloadManifestIdV5}`,
    );
  }
  if (
    workload?.source_manifest?.artifact_sha256 !==
      sourceWorkloadArtifactSha256V5 ||
    workload?.source_manifest?.byte_sha256 !== sourceWorkloadByteSha256V5
  ) {
    errors.push("v5 source manifest must pin exact v4 bytes and artifact");
  }
  const journeys = new Map(
    workload?.journeys?.map((journey) => [journey.id, journey]) ?? [],
  );
  if (!exactStringArray([...journeys.keys()], representativeJourneyIdsV5)) {
    errors.push("v5 representative journey order is not exact");
  }
  const fixtureIds = new Set(workload?.fixtures?.map(({ id }) => id) ?? []);
  const commandIds = new Set();
  for (const { journey_id: journeyId, command } of representativeCommandStream(
    workload,
  )) {
    if (commandIds.has(command.id)) {
      errors.push(`duplicate representative command ${command.id}`);
    }
    commandIds.add(command.id);
    if (
      !Array.isArray(command.expected_milestones) ||
      command.expected_milestones.length === 0
    ) {
      errors.push(`${journeyId}:${command.id} has no expected milestones`);
    }
    for (const fixtureId of commandFixtureIds(
      journeys.get(journeyId),
      command,
    )) {
      if (!fixtureIds.has(fixtureId)) {
        errors.push(
          `${journeyId}:${command.id} maps unknown fixture ${fixtureId}`,
        );
      }
    }
  }

  const dynamic = commandById(workload, "viewer:dynamic-fidelity-scroll");
  if (
    sha256Canonical(
      workload?.expected?.v5_hard_component_rules
        ?.gpui_multi_document_absolute_safety_budgets,
    ) !== sha256Canonical(gpuiMultiDocumentAbsoluteSafetyBudgetsV5)
  ) {
    errors.push("GPUI multi-document absolute safety budgets are not exact");
  }
  if (
    dynamic?.fixture_id !== "nasa-apollo-summary-526-v1" ||
    dynamic?.input_rate_hz !== 120 ||
    dynamic?.duration_ms !== 32_000 ||
    dynamic?.expected_trajectory_sample_count !== 3841 ||
    dynamic?.path?.forward_duration_ms !== 20_000 ||
    dynamic?.path?.forward_motion_duration_ms !== 19_250 ||
    dynamic?.path?.pause_duration_ms !== 2_000 ||
    dynamic?.path?.reverse_duration_ms !== 10_000 ||
    dynamic?.path?.checkpoint_hold_ms !== 250 ||
    dynamic?.path?.checkpoint_hold_count !== 3 ||
    dynamic?.path?.checkpoint_hold_interval_count_each !== 30 ||
    dynamic?.path?.checkpoint_hold_sample_count_each !== 31 ||
    dynamic?.path?.finish_page !== 1 ||
    dynamic?.observer?.rate_hz !== 60 ||
    dynamic?.observer?.expected_sample_count !== 1921
  ) {
    errors.push(
      "dynamic fidelity scroll timing and NASA fixture are not exact",
    );
  }
  const presentation = dynamic?.presentation;
  if (
    presentation?.zoom_mode !== "fixed-percent" ||
    presentation?.zoom_percent !== 100 ||
    presentation?.required_x_device_pixels_per_pdf_point !== 1 ||
    presentation?.required_y_device_pixels_per_pdf_point !== 1 ||
    presentation?.pixels_per_point_tolerance !== 0.01 ||
    presentation?.paired_candidate_scale_tolerance !== 0.01 ||
    presentation?.client_device_scale !== 1 ||
    presentation?.screenshot_source !== "presented-client-drawable" ||
    presentation?.page_bounds_source !== "painted-page-bounds" ||
    presentation?.stability_receipt !== "painted-before-capture-after" ||
    presentation?.candidate_resampling !== "forbidden" ||
    presentation?.reference_resampling !== "downsample-only-lanczos3"
  ) {
    errors.push("dynamic fidelity presentation contract is not exact");
  }
  if (dynamic?.registered_crops?.length !== 3) {
    errors.push("dynamic fidelity must register exactly three crops");
  }
  for (const [index, crop] of (dynamic?.registered_crops ?? []).entries()) {
    const raster = crop.reference_raster;
    const parametersExact = Object.entries(
      crossEngineScanFidelityParametersV2,
    ).every(([key, value]) => raster?.[key] === value);
    if (
      !sha256Pattern.test(crop.registration_sha256 ?? "") ||
      cropRegistrationHashV5(crop) !== crop.registration_sha256 ||
      crop.checkpoint?.hold_index !== index ||
      crop.checkpoint?.hold_ms !== 250 ||
      crop.checkpoint?.zero_input_interval_count !== 30 ||
      crop.checkpoint?.zero_input_sample_count !== 31 ||
      raster?.comparison !== crossEngineScanFidelityAlgorithmV2 ||
      !parametersExact ||
      raster?.candidate_resampling !== "none" ||
      raster?.reference_resampling !== "downsample-only-lanczos3" ||
      !sha256Pattern.test(raster?.reference_crop_sha256 ?? "")
    ) {
      errors.push(
        `${crop.crop_id ?? "unknown crop"}: invalid crop registration`,
      );
    }
  }
  if (
    !exactStringArray(dynamic?.required_sample_fields, [
      "visible_page_ready_fraction",
      "visible_raster_ready_area_fraction",
      "visible_raster_pixel_density",
    ])
  ) {
    errors.push("dynamic fidelity sample field mapping is not exact");
  }
  for (const milestone of [
    "presented-screenshot-crops-three-matched",
    "presented-scale-comparability-proven",
    "checkpoint-holds-stable",
    "visible-page-ready-fraction-recorded",
    "visible-raster-ready-area-fraction-recorded",
    "visible-raster-pixel-density-recorded",
  ]) {
    if (!dynamic?.expected_milestones?.includes(milestone)) {
      errors.push(`dynamic fidelity is missing milestone ${milestone}`);
    }
  }

  const property = commandById(
    workload,
    "annotation:native-property-edit-undo",
  );
  if (
    property?.input_lane !== "native-replay" ||
    property?.property_edit?.from !== 1.5 ||
    property?.property_edit?.to !== 4 ||
    property?.candidate_policy?.effective_history_revision_delta !== 1 ||
    property?.candidate_policy?.application_undo_count !== 1 ||
    property?.candidate_policy?.canonical_state_restored !== true ||
    property?.electron_baseline_policy?.allowed_known_defect_id !==
      "electron-numeric-property-input-blur-duplicate-history-v1" ||
    property?.electron_baseline_policy?.effective_history_revision_delta !==
      2 ||
    property?.electron_baseline_policy?.application_undo_count !== 1 ||
    property?.electron_baseline_policy?.final_stroke_width_points !== 4
  ) {
    errors.push("native property edit and undo state is not exact");
  }
  for (const milestone of [
    "trusted-native-input-complete",
    "property-user-gesture-complete",
    "native-property-presentation-acknowledged",
    "application-undo-applied-once",
    "implementation-history-outcome-recorded",
  ]) {
    if (!property?.expected_milestones?.includes(milestone)) {
      errors.push(`native property edit is missing milestone ${milestone}`);
    }
  }

  const snap = commandById(workload, "annotation:native-snap-transform-120hz");
  const rawDelta = {
    x:
      (snap?.pointer_path?.unsnapped_end?.x ?? Number.NaN) -
      (snap?.pointer_path?.start?.x ?? Number.NaN),
    y:
      (snap?.pointer_path?.unsnapped_end?.y ?? Number.NaN) -
      (snap?.pointer_path?.start?.y ?? Number.NaN),
  };
  const snapCorrection = {
    x:
      (snap?.pointer_path?.expected_snapped_delta?.x ?? Number.NaN) -
      rawDelta.x,
    y:
      (snap?.pointer_path?.expected_snapped_delta?.y ?? Number.NaN) -
      rawDelta.y,
  };
  const snappedFinalRectangle = {
    x1:
      (snap?.setup?.rectangle?.x1 ?? Number.NaN) +
      (snap?.pointer_path?.expected_snapped_delta?.x ?? Number.NaN),
    y1:
      (snap?.setup?.rectangle?.y1 ?? Number.NaN) +
      (snap?.pointer_path?.expected_snapped_delta?.y ?? Number.NaN),
    x2:
      (snap?.setup?.rectangle?.x2 ?? Number.NaN) +
      (snap?.pointer_path?.expected_snapped_delta?.x ?? Number.NaN),
    y2:
      (snap?.setup?.rectangle?.y2 ?? Number.NaN) +
      (snap?.pointer_path?.expected_snapped_delta?.y ?? Number.NaN),
  };
  if (
    snap?.rate_hz !== 120 ||
    snap?.expected_sample_count !== 361 ||
    snap?.snap?.enabled !== true ||
    snap?.snap?.grid_spacing_points !== 18 ||
    snap?.snap?.sensitivity?.value !== 8 ||
    snap?.snap?.sensitivity?.unit !== "css-px" ||
    snap?.snap?.sensitivity?.threshold_norm !== "per-axis-l-infinity" ||
    snap?.snap?.sensitivity?.inclusive !== true ||
    !exactStringArray(rawDelta, { x: 97, y: 83 }) ||
    !exactStringArray(snapCorrection, { x: -7, y: 7 }) ||
    !exactStringArray(snappedFinalRectangle, snap?.expected_final_rectangle) ||
    !exactStringArray(snap?.expected_final_rectangle, {
      x1: 162,
      y1: 234,
      x2: 342,
      y2: 450,
    })
  ) {
    errors.push("native snap transform geometry and 120 Hz path are not exact");
  }
  for (const milestone of [
    "timestamped-native-input-complete",
    "snap-target-acquired",
    "snap-guide-presented",
    "snapped-geometry-exact",
    "gesture-committed-once",
  ]) {
    if (!snap?.expected_milestones?.includes(milestone)) {
      errors.push(`native snap transform is missing milestone ${milestone}`);
    }
  }

  const session = journeys.get("multi-document-session-v1");
  const sessionFixtures = [
    "bp-single-page-v1",
    "nasa-apollo-summary-526-v1",
    "bp-engineering-sheet-v1",
    "bp-annotation-density-v1",
  ];
  if (
    !exactStringArray(session?.fixtures, sessionFixtures) ||
    !exactStringArray(
      session?.commands?.map(({ id }) => id),
      [
        "session:open-four-fixtures",
        "session:switch-four-fixtures",
        "session:edit-dense-rectangle",
        "session:close-three-and-recover",
      ],
    )
  ) {
    errors.push(
      "multi-document session fixture and command order is not exact",
    );
  }
  const sessionCommands = new Map(
    (session?.commands ?? []).map((command) => [command.id, command]),
  );
  for (const commandId of [
    "session:open-four-fixtures",
    "session:switch-four-fixtures",
  ]) {
    if (
      !exactStringArray(
        sessionCommands.get(commandId)?.fixture_ids,
        sessionFixtures,
      )
    ) {
      errors.push(`${commandId}: multi-document fixture mapping is not exact`);
    }
  }
  const sessionEdit = sessionCommands.get("session:edit-dense-rectangle");
  if (
    sessionEdit?.fixture_id !== "bp-annotation-density-v1" ||
    sessionEdit?.property_edit?.property !== "stroke_width_points" ||
    sessionEdit?.property_edit?.from !== 1.5 ||
    sessionEdit?.property_edit?.to !== 4
  ) {
    errors.push("session:edit-dense-rectangle is not exact");
  }
  const sessionClose = sessionCommands.get("session:close-three-and-recover");
  if (
    !exactStringArray(sessionClose?.fixture_ids, [
      "bp-single-page-v1",
      "nasa-apollo-summary-526-v1",
      "bp-engineering-sheet-v1",
    ]) ||
    !exactStringArray(sessionClose?.close_sequence, [
      "bp-single-page-v1",
      "bp-engineering-sheet-v1",
      "nasa-apollo-summary-526-v1",
    ]) ||
    sessionClose?.remaining_fixture_id !== "bp-annotation-density-v1"
  ) {
    errors.push("session:close-three-and-recover is not exact");
  }
  const processIdentityMilestones = (session?.commands ?? []).flatMap(
    ({ expected_milestones: milestones }) => milestones,
  );
  if (
    !processIdentityMilestones.includes("application-process-id-recorded") ||
    processIdentityMilestones.filter(
      (milestone) => milestone === "application-process-id-stable",
    ).length !== 3
  ) {
    errors.push("multi-document session does not prove one stable process");
  }

  const usgs = workload?.stress_lanes?.find(
    ({ id }) => id === "usgs-large-sheet-stress-v1",
  );
  if (
    usgs?.fixture_id !== "usgs-usa-geology-sheet-v1" ||
    usgs?.inference_eligible !== false
  ) {
    errors.push("USGS must remain a non-inferential stress lane");
  }
  const hibbeler = workload?.supplementary_lanes?.find(
    ({ id }) => id === "private-hibbeler-935-v1",
  );
  if (
    hibbeler?.status !== "blocked-not-transferred" ||
    hibbeler?.inference_eligible !== false
  ) {
    errors.push(
      "Hibbeler must remain supplementary and blocked-not-transferred",
    );
  }

  const actualHashes = comparisonWorkloadHashesV5(workload);
  for (const [name, actual] of Object.entries(actualHashes)) {
    if (workload?.canonical_hashes?.[name] !== actual) {
      errors.push(`${name} does not match the canonical v5 stream`);
    }
  }
  return errors;
}

export function assessLiveCommandReceiptsV5(
  workload,
  implementation,
  receipts,
) {
  const expected = representativeCommandStream(workload).map(
    ({ command }) => command.id,
  );
  const byId = new Map(
    (receipts ?? []).map((receipt) => [receipt.command_id, receipt]),
  );
  const failures = [];
  for (const commandId of expected) {
    const receipt = byId.get(commandId);
    if (
      receipt?.live !== true ||
      receipt?.passed !== true ||
      !sha256Pattern.test(receipt?.evidence_sha256 ?? "")
    ) {
      failures.push(
        `${implementation}:${commandId}: live command receipt did not pass`,
      );
    }
  }
  for (const commandId of byId.keys()) {
    if (!expected.includes(commandId)) {
      failures.push(
        `${implementation}:${commandId}: unexpected command receipt`,
      );
    }
  }
  return {
    implementation,
    evidence_class: "live-command-receipts",
    expected_command_count: expected.length,
    observed_command_count: byId.size,
    ready: failures.length === 0,
    failures,
  };
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
) {
  const workload = await loadComparisonWorkloadV5();
  const errors = validateComparisonWorkloadV5(workload);
  const materializeIndex = process.argv.indexOf("--materialize");
  let materialized = null;
  if (materializeIndex >= 0) {
    const outputPath = process.argv[materializeIndex + 1];
    if (!outputPath || outputPath.startsWith("--")) {
      throw new Error("--materialize requires an output path");
    }
    if (errors.length === 0) {
      materialized = await materializeComparisonWorkloadV5(outputPath);
    }
  }
  process.stdout.write(
    `${JSON.stringify(
      {
        status: errors.length === 0 ? "passed" : "failed",
        manifest_id: workload.manifest_id,
        artifact_sha256: comparisonWorkloadArtifactHashV5(workload),
        hashes: comparisonWorkloadHashesV5(workload),
        materialized,
        errors,
      },
      null,
      2,
    )}\n`,
  );
  if (errors.length > 0) process.exitCode = 1;
}
