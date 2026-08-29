import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

import {
  comparisonWorkloadArtifactHash as v3ArtifactHash,
  loadComparisonWorkload as loadV3Workload,
} from "./comparison-workload.mjs";
import { representativeJourneyIdsV4 } from "./decision-contract-v4.mjs";

export const comparisonWorkloadSchemaVersionV4 = "bp-comparison-workload-v2";
export const comparisonWorkloadManifestIdV4 = "bp-perf-v4-decision-1";

const defaultDescriptorUrl = new URL("./comparison-workload-v4.json", import.meta.url);
const defaultMaterializedUrl = new URL("./comparison-workload-v4.materialized.json", import.meta.url);
const sha256Pattern = /^[0-9a-f]{64}$/;
const forbiddenAsymmetricUploadMilestones = new Set([
  "bitmap-upload-recorded",
  "upload-byte-count-recorded",
]);

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

function commandMap(workload) {
  return new Map(workload.journeys.flatMap(({ commands }) => commands).map((command) => [command.id, command]));
}

function representativeCommandStream(workload) {
  return workload.journeys.flatMap((journey) => journey.commands.map((command) => ({
    journey_id: journey.id,
    command,
  })));
}

function milestoneStream(workload) {
  return representativeCommandStream(workload).map(({ journey_id, command }) => ({
    journey_id,
    command_id: command.id,
    milestones: command.expected_milestones,
  }));
}

function stressCommandStream(workload) {
  return workload.stress_lanes.flatMap((lane) => lane.commands.map((command) => ({
    stress_lane_id: lane.id,
    command,
  })));
}

export function comparisonWorkloadHashesV4(workload) {
  return {
    representative_command_stream_sha256: sha256(representativeCommandStream(workload)),
    representative_milestone_stream_sha256: sha256(milestoneStream(workload)),
    stress_command_stream_sha256: sha256(stressCommandStream(workload)),
    expected_state_sha256: sha256(workload.expected),
  };
}

export function comparisonWorkloadArtifactHashV4(workload) {
  return sha256(workload);
}

function applyCommandOverride(command, override) {
  return override ? { ...command, ...override } : { ...command };
}

export async function loadComparisonWorkloadV4(
  descriptorPath = fileURLToPath(defaultDescriptorUrl),
) {
  const descriptor = JSON.parse(await readFile(descriptorPath, "utf8"));
  const v3 = await loadV3Workload();
  if (v3ArtifactHash(v3) !== descriptor.source_manifest.artifact_sha256) {
    throw new Error("v4 source manifest hash does not match frozen v3");
  }
  const sources = commandMap(v3);
  for (const command of descriptor.additional_commands) sources.set(command.id, command);

  const journeys = descriptor.representative_journeys.map((journey) => ({
    id: journey.id,
    fixtures: [...journey.fixture_ids],
    commands: journey.command_ids.map((commandId) => {
      const source = sources.get(commandId);
      if (!source) throw new Error(`v4 journey ${journey.id} references unknown command ${commandId}`);
      return applyCommandOverride(source, descriptor.command_overrides[commandId]);
    }),
  }));
  const {
    blank_current_generation_frames: _v3BlankFrameRule,
    ...v3MilestoneRules
  } = v3.expected.milestone_rules;
  const workload = {
    schema_version: descriptor.schema_version,
    manifest_id: descriptor.manifest_id,
    decision_contract_version: descriptor.decision_contract_version,
    supersedes: descriptor.supersedes,
    seed: descriptor.seed,
    coordinate_space: descriptor.coordinate_space,
    environment: descriptor.environment,
    source_manifest: descriptor.source_manifest,
    fixtures: descriptor.fixtures,
    assets: descriptor.assets,
    journeys,
    stress_lanes: descriptor.stress_lanes,
    supplementary_lanes: descriptor.supplementary_lanes,
    expected: {
      milestone_rules: {
        ...v3MilestoneRules,
        visible_raster_readiness_observations_minimum:
          descriptor.expected.visible_raster_readiness.minimum_observation_count,
        missing_raster_observation_count:
          descriptor.expected.visible_raster_readiness.missing_raster_observation_count,
        visible_raster_readiness_rate:
          descriptor.expected.visible_raster_readiness.readiness_rate,
      },
      resource_rules: descriptor.expected.resource_rules,
      final_state: v3.expected.final_state,
    },
    canonical_hashes: descriptor.canonical_hashes,
  };
  return workload;
}

export async function loadMaterializedComparisonWorkloadV4(
  materializedPath = fileURLToPath(defaultMaterializedUrl),
) {
  return JSON.parse(await readFile(materializedPath, "utf8"));
}

export function validateComparisonWorkloadV4(workload) {
  const errors = [];
  if (workload?.schema_version !== comparisonWorkloadSchemaVersionV4) {
    errors.push(`schema_version must be ${comparisonWorkloadSchemaVersionV4}`);
  }
  if (workload?.manifest_id !== comparisonWorkloadManifestIdV4
    || workload?.decision_contract_version !== comparisonWorkloadManifestIdV4) {
    errors.push(`manifest and decision contract must be ${comparisonWorkloadManifestIdV4}`);
  }
  if (workload?.coordinate_space !== "pdf-points-bottom-left") {
    errors.push("coordinate_space must be pdf-points-bottom-left");
  }
  const fixtureIds = new Set(workload?.fixtures?.map(({ id }) => id) ?? []);
  if (fixtureIds.has("usgs-usa-geology-sheet-v1")) {
    errors.push("USGS must not be a representative fixture");
  }
  const journeys = new Map(workload?.journeys?.map((journey) => [journey.id, journey]) ?? []);
  for (const journeyId of representativeJourneyIdsV4) {
    if (!journeys.has(journeyId)) errors.push(`missing representative journey ${journeyId}`);
  }
  const commandIds = new Set();
  for (const { journey_id: journeyId, command } of representativeCommandStream(workload)) {
    if (commandIds.has(command.id)) errors.push(`duplicate representative command ${command.id}`);
    commandIds.add(command.id);
    if (!Array.isArray(command.expected_milestones) || command.expected_milestones.length === 0) {
      errors.push(`${journeyId}:${command.id} has no expected milestones`);
    }
    for (const milestone of command.expected_milestones ?? []) {
      if (forbiddenAsymmetricUploadMilestones.has(milestone)) {
        errors.push(`${command.id} retains asymmetric physical upload milestone ${milestone}`);
      }
    }
  }
  const continuousScroll = representativeCommandStream(workload).find(
    ({ command }) => command.id === "viewer:continuous-scroll",
  )?.command;
  if (
    continuousScroll?.expected_milestones?.includes(
      "blank-current-generation-frames-zero",
    ) ||
    !continuousScroll?.expected_milestones?.includes(
      "visible-raster-readiness-observed",
    )
  ) {
    errors.push(
      "v4 continuous scroll must require truthful visible raster readiness observations",
    );
  }
  if (
    workload?.expected?.milestone_rules
      ?.visible_raster_readiness_observations_minimum !== 1 ||
    Object.hasOwn(
      workload?.expected?.milestone_rules ?? {},
      "blank_current_generation_frames",
    )
  ) {
    errors.push(
      "v4 raster readiness must require observations and keep missing counts diagnostic",
    );
  }
  for (const imageId of ["image:create", "image:resize-history"]) {
    const image = [...commandIds].includes(imageId)
      ? representativeCommandStream(workload).find(({ command }) => command.id === imageId)?.command
      : null;
    if (image?.resource_observation?.decoded_payload_bytes !== 786_432
      || image?.resource_observation?.renderer_resource_submission_bytes !== 786_432
      || image?.resource_observation?.physical_bus_upload_bytes !== null) {
      errors.push(`${imageId} must freeze exact decoded and renderer-submission bytes with nullable physical bus bytes`);
    }
  }
  const usgs = workload?.stress_lanes?.find(({ id }) => id === "usgs-large-sheet-stress-v1");
  if (usgs?.fixture_id !== "usgs-usa-geology-sheet-v1" || usgs?.inference_eligible !== false) {
    errors.push("USGS must remain a non-inferential stress lane");
  }
  const hibbeler = workload?.supplementary_lanes?.find(({ id }) => id === "private-hibbeler-935-v1");
  if (hibbeler?.status !== "blocked-not-transferred" || hibbeler?.inference_eligible !== false) {
    errors.push("Hibbeler must remain supplementary and blocked-not-transferred");
  }
  const actualHashes = comparisonWorkloadHashesV4(workload);
  for (const [name, actual] of Object.entries(actualHashes)) {
    if (workload?.canonical_hashes?.[name] !== actual) {
      errors.push(`${name} does not match the canonical v4 stream`);
    }
  }
  return errors;
}

export function buildDeclaredCapabilityReportV4(workload, declaration) {
  const declared = new Set(declaration?.command_ids ?? []);
  const expected = representativeCommandStream(workload).map(({ command }) => command.id);
  const missing = expected.filter((commandId) => !declared.has(commandId));
  return {
    implementation: declaration?.implementation ?? null,
    evidence_class: "planning-declaration-only",
    declared_command_count: expected.length - missing.length,
    expected_command_count: expected.length,
    missing_command_ids: missing,
    execution_eligible: false,
  };
}

export function assessLiveCommandReceiptsV4(workload, implementation, receipts) {
  const expected = representativeCommandStream(workload).map(({ command }) => command.id);
  const byId = new Map((receipts ?? []).map((receipt) => [receipt.command_id, receipt]));
  const failures = [];
  for (const commandId of expected) {
    const receipt = byId.get(commandId);
    if (receipt?.live !== true || receipt?.passed !== true
      || !sha256Pattern.test(receipt?.evidence_sha256 ?? "")) {
      failures.push(`${implementation}:${commandId}: live command receipt did not pass`);
    }
  }
  for (const commandId of byId.keys()) {
    if (!expected.includes(commandId)) failures.push(`${implementation}:${commandId}: unexpected command receipt`);
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

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const workload = await loadComparisonWorkloadV4();
  const errors = validateComparisonWorkloadV4(workload);
  process.stdout.write(`${JSON.stringify({
    status: errors.length === 0 ? "passed" : "failed",
    manifest_id: workload.manifest_id,
    artifact_sha256: comparisonWorkloadArtifactHashV4(workload),
    hashes: comparisonWorkloadHashesV4(workload),
    errors,
  }, null, 2)}\n`);
  if (errors.length > 0) process.exitCode = 1;
}
