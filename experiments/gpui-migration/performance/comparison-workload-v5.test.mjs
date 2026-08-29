import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  comparisonWorkloadArtifactHashV4,
  loadMaterializedComparisonWorkloadV4,
} from "./comparison-workload-v4.mjs";
import {
  assessLiveCommandReceiptsV5,
  comparisonWorkloadArtifactHashV5,
  comparisonWorkloadByteHashV5,
  comparisonWorkloadHashesV5,
  cropRegistrationHashV5,
  loadComparisonWorkloadV5,
  loadMaterializedComparisonWorkloadV5,
  representativeFixtureCommandMappingV5,
  sourceWorkloadArtifactSha256V5,
  sourceWorkloadByteSha256V5,
  validateComparisonWorkloadV5,
} from "./comparison-workload-v5.mjs";
import {
  gpuiMultiDocumentAbsoluteSafetyBudgetsV5,
  representativeJourneyIdsV5,
} from "./decision-contract-v5.mjs";

const digest = "b".repeat(64);

function commandMap(workload) {
  return new Map(
    workload.journeys
      .flatMap(({ commands }) => commands)
      .map((command) => [command.id, command]),
  );
}

test("materializes v5 from immutable v4 bytes without changing v4", async () => {
  const sourceUrl = new URL(
    "./comparison-workload-v4.materialized.json",
    import.meta.url,
  );
  const beforeBytes = await readFile(sourceUrl);
  const beforeByteHash = createHash("sha256").update(beforeBytes).digest("hex");
  const before = await loadMaterializedComparisonWorkloadV4();
  const workload = await loadComparisonWorkloadV5();
  const afterBytes = await readFile(sourceUrl);
  const after = await loadMaterializedComparisonWorkloadV4();

  assert.equal(beforeByteHash, sourceWorkloadByteSha256V5);
  assert.equal(
    createHash("sha256").update(afterBytes).digest("hex"),
    beforeByteHash,
  );
  assert.equal(
    comparisonWorkloadArtifactHashV4(before),
    sourceWorkloadArtifactSha256V5,
  );
  assert.equal(
    comparisonWorkloadArtifactHashV4(after),
    sourceWorkloadArtifactSha256V5,
  );
  assert.deepEqual(validateComparisonWorkloadV5(workload), []);
  assert.deepEqual(
    workload.journeys.map(({ id }) => id),
    representativeJourneyIdsV5,
  );
});

test("retains every v4 journey command as an exact prefix", async () => {
  const source = await loadMaterializedComparisonWorkloadV4();
  const workload = await loadComparisonWorkloadV5();
  for (const sourceJourney of source.journeys) {
    const v5Journey = workload.journeys.find(
      ({ id }) => id === sourceJourney.id,
    );
    assert.deepEqual(
      v5Journey.commands.slice(0, sourceJourney.commands.length),
      sourceJourney.commands,
    );
  }
  assert.deepEqual(workload.stress_lanes, source.stress_lanes);
  assert.deepEqual(workload.supplementary_lanes, source.supplementary_lanes);
});

test("checks in a byte-stable materialized v5 runtime artifact", async () => {
  const fromDescriptor = await loadComparisonWorkloadV5();
  const materialized = await loadMaterializedComparisonWorkloadV5();
  assert.deepEqual(materialized, fromDescriptor);
  assert.equal(
    comparisonWorkloadByteHashV5(materialized),
    "e7b2540c7d455a30e52ee64a6819745fe0ad49a6512f887df4631aac72054f6d",
  );
  assert.equal(
    comparisonWorkloadArtifactHashV5(materialized),
    "cc4f8b8940556390b8d16a6baae43e8a5a022541fba90beea08869e692ee920e",
  );
  assert.deepEqual(validateComparisonWorkloadV5(materialized), []);
});

test("freezes exact v5 canonical streams and artifact", async () => {
  const workload = await loadComparisonWorkloadV5();
  assert.deepEqual(comparisonWorkloadHashesV5(workload), {
    representative_command_stream_sha256:
      "99ea727689bf4bf725cae92a7d0d86383cf7e85aa0e8252228eed346866e10cd",
    representative_milestone_stream_sha256:
      "91b25071a024ae00c2a96ebb0b6c06849650a04abd2118770dcbfe894247d244",
    representative_fixture_command_mapping_sha256:
      "3b8622cd2cbec09f1e3740240e996764ca30e15f6a6fa5d664fccc592ff2dad4",
    stress_command_stream_sha256:
      "a7e63ced8f1379758d42c56f3a608100270f0131393f65ca7841bd9e3fb0f3e6",
    expected_state_sha256:
      "e2e0b1e812545355cbb76c0e653576f8cff21f2c37c7018dcc9cd86c36e38a8b",
  });
  assert.equal(
    comparisonWorkloadArtifactHashV5(workload),
    "cc4f8b8940556390b8d16a6baae43e8a5a022541fba90beea08869e692ee920e",
  );
});

test("maps the four hard component commands to exact fixtures and milestones", async () => {
  const workload = await loadComparisonWorkloadV5();
  const commands = commandMap(workload);
  const dynamic = commands.get("viewer:dynamic-fidelity-scroll");
  assert.equal(dynamic.registered_crops.length, 3);
  assert.deepEqual(
    dynamic.registered_crops.map(({ registration_sha256: hash }) => hash),
    dynamic.registered_crops.map(cropRegistrationHashV5),
  );
  assert.equal(dynamic.duration_ms, 32000);
  assert.equal(dynamic.observer.expected_sample_count, 1921);
  assert.deepEqual(dynamic.presentation, {
    zoom_mode: "fixed-percent",
    zoom_percent: 100,
    required_x_device_pixels_per_pdf_point: 1,
    required_y_device_pixels_per_pdf_point: 1,
    pixels_per_point_tolerance: 0.01,
    paired_candidate_scale_tolerance: 0.01,
    client_device_scale: 1,
    screenshot_source: "presented-client-drawable",
    page_bounds_source: "painted-page-bounds",
    stability_receipt: "painted-before-capture-after",
    candidate_resampling: "forbidden",
    reference_resampling: "downsample-only-lanczos3",
  });
  assert.equal(dynamic.path.forward_motion_duration_ms, 19250);
  assert.equal(dynamic.path.checkpoint_hold_count, 3);
  assert.deepEqual(
    dynamic.registered_crops.map(({ checkpoint }) => ({
      index: checkpoint.hold_index,
      hold: checkpoint.hold_ms,
      intervals: checkpoint.zero_input_interval_count,
      samples: checkpoint.zero_input_sample_count,
    })),
    [0, 1, 2].map((index) => ({
      index,
      hold: 250,
      intervals: 30,
      samples: 31,
    })),
  );
  assert.deepEqual(
    dynamic.registered_crops.map(
      ({ reference_raster }) => reference_raster.reference_crop_sha256,
    ),
    [
      "9618896b1558e6e92b1b0d10fec4f1480bd74b7e36d749dde07505eabfad8b5a",
      "d0145b0e6e2240098477f7b029d3d52dc912e1633a7cf7dd338b35c50c465a03",
      "c72c0763c9522e475f205ac7b570b8e0cbee1071497bd163aa2c1ae51f099c77",
    ],
  );
  assert.deepEqual(dynamic.required_sample_fields, [
    "visible_page_ready_fraction",
    "visible_raster_ready_area_fraction",
    "visible_raster_pixel_density",
  ]);

  const property = commands.get("annotation:native-property-edit-undo");
  assert.equal(property.input_lane, "native-replay");
  assert.deepEqual(property.candidate_policy, {
    effective_history_revision_delta: 1,
    application_undo_count: 1,
    canonical_state_restored: true,
  });
  assert.deepEqual(property.electron_baseline_policy, {
    allowed_known_defect_id:
      "electron-numeric-property-input-blur-duplicate-history-v1",
    effective_history_revision_delta: 2,
    application_undo_count: 1,
    final_stroke_width_points: 4,
  });

  const snap = commands.get("annotation:native-snap-transform-120hz");
  assert.equal(snap.rate_hz, 120);
  assert.equal(snap.expected_sample_count, 361);
  assert.equal(snap.snap.enabled, true);
  assert.deepEqual(snap.snap.sensitivity, {
    value: 8,
    unit: "css-px",
    threshold_norm: "per-axis-l-infinity",
    inclusive: true,
  });
  const rawDelta = {
    x: snap.pointer_path.unsnapped_end.x - snap.pointer_path.start.x,
    y: snap.pointer_path.unsnapped_end.y - snap.pointer_path.start.y,
  };
  const correction = {
    x: snap.pointer_path.expected_snapped_delta.x - rawDelta.x,
    y: snap.pointer_path.expected_snapped_delta.y - rawDelta.y,
  };
  assert.deepEqual(rawDelta, { x: 97, y: 83 });
  assert.deepEqual(correction, { x: -7, y: 7 });
  assert.deepEqual(snap.expected_final_rectangle, {
    x1: 162,
    y1: 234,
    x2: 342,
    y2: 450,
  });

  const mappings = representativeFixtureCommandMappingV5(workload);
  for (const commandId of [
    "session:open-four-fixtures",
    "session:switch-four-fixtures",
  ]) {
    assert.deepEqual(
      mappings.find(({ command_id: id }) => id === commandId).fixture_ids,
      [
        "bp-single-page-v1",
        "nasa-apollo-summary-526-v1",
        "bp-engineering-sheet-v1",
        "bp-annotation-density-v1",
      ],
    );
  }
  assert.deepEqual(
    mappings.find(({ command_id: id }) => id === "session:edit-dense-rectangle")
      .fixture_ids,
    ["bp-annotation-density-v1"],
  );
  assert.deepEqual(
    mappings.find(
      ({ command_id: id }) => id === "session:close-three-and-recover",
    ).fixture_ids,
    [
      "bp-single-page-v1",
      "nasa-apollo-summary-526-v1",
      "bp-engineering-sheet-v1",
    ],
  );
});

test("materializes the exact GPUI multi-document safety budget contract", async () => {
  const workload = await loadComparisonWorkloadV5();
  assert.deepEqual(
    workload.expected.v5_hard_component_rules
      .gpui_multi_document_absolute_safety_budgets,
    gpuiMultiDocumentAbsoluteSafetyBudgetsV5,
  );
  const broken = structuredClone(workload);
  broken.expected.v5_hard_component_rules.gpui_multi_document_absolute_safety_budgets.metrics.application_frame_interval_p95_ms.maximum = 100;
  assert(
    validateComparisonWorkloadV5(broken).includes(
      "GPUI multi-document absolute safety budgets are not exact",
    ),
  );
});

test("fails closed when a hard command, crop, or receipt is incomplete", async () => {
  const workload = await loadComparisonWorkloadV5();
  const broken = structuredClone(workload);
  const dynamic = commandMap(broken).get("viewer:dynamic-fidelity-scroll");
  dynamic.registered_crops[0].registration_sha256 = "0".repeat(64);
  assert(
    validateComparisonWorkloadV5(broken).some((error) =>
      error.includes("invalid crop registration"),
    ),
  );

  const receipts = workload.journeys.flatMap(({ commands }) =>
    commands.map(({ id }) => ({
      command_id: id,
      live: true,
      passed: true,
      evidence_sha256: digest,
    })),
  );
  assert.equal(
    assessLiveCommandReceiptsV5(workload, "gpui", receipts).ready,
    true,
  );
  receipts.find(
    ({ command_id: commandId }) =>
      commandId === "annotation:native-property-edit-undo",
  ).passed = false;
  assert.equal(
    assessLiveCommandReceiptsV5(workload, "gpui", receipts).ready,
    false,
  );
});

test("publishes a strict immutable-overlay workload schema", async () => {
  const [schema, materializedSchema] = await Promise.all([
    readFile(
      new URL("./comparison-workload-v5.schema.json", import.meta.url),
      "utf8",
    ),
    readFile(
      new URL(
        "./comparison-workload-v5.materialized.schema.json",
        import.meta.url,
      ),
      "utf8",
    ),
  ]).then((values) => values.map(JSON.parse));
  assert.equal(schema.properties.manifest_id.const, "bp-perf-v5-decision-1");
  assert.equal(schema.properties.journey_extensions.minItems, 2);
  assert.equal(schema.properties.additional_journeys.maxItems, 1);
  assert.equal(
    schema.properties.source_manifest.properties.byte_sha256.pattern,
    "^[0-9a-f]{64}$",
  );
  assert.equal(materializedSchema.properties.journeys.minItems, 6);
  assert(
    schema.properties.expected_extensions.required.includes(
      "gpui_multi_document_absolute_safety_budgets",
    ),
  );
  assert.equal(
    schema.$defs.gpuiMultiDocumentAbsoluteSafetyBudgets.properties.metrics.const
      .cgroup_peak_memory_bytes.maximum,
    1_610_612_736,
  );
  assert.equal(
    materializedSchema.properties.expected.properties.v5_hard_component_rules
      .properties.gpui_multi_document_absolute_safety_budgets.$ref,
    "#/$defs/gpuiMultiDocumentAbsoluteSafetyBudgets",
  );
  assert.equal(
    materializedSchema.properties.source_manifest.properties.byte_sha256.const,
    sourceWorkloadByteSha256V5,
  );
});
