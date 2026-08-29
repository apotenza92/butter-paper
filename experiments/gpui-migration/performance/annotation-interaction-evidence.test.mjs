import assert from "node:assert/strict";
import test from "node:test";

import {
  buildDeltaPointerStream,
  rectangleTransformPrerequisiteEvidence,
  rectanglePropertiesTransformPrerequisiteEvidence,
  rectanglePropertiesHistoryEvidence,
  rectangleDenseRepeatEvidence,
  rectangleTransformEvidence,
} from "./annotation-interaction-evidence.mjs";

test("builds the exact inclusive 120 Hz move and resize streams", () => {
  const samples = buildDeltaPointerStream({
    start: { x: 162, y: 192 },
    delta: { x: 18, y: -12 },
    rate_hz: 120,
    duration_ms: 3_000,
  });
  assert.equal(samples.length, 361);
  assert.deepEqual(samples[0], { x: 162, y: 192, t_ms: 0 });
  assert.deepEqual(samples.at(-1), { x: 180, y: 180, t_ms: 3_000 });
});

test("accepts only exact one-commit rectangle move and east resize evidence", () => {
  const command = {
    annotation_id: "comparison:rectangle:sparse:1",
    move: { delta: { x: 18, y: -12 } },
    resize: { handle: "east", delta: { x: 30, y: 0 } },
  };
  const before = { id: command.annotation_id, kind: "rectangle", rect: { x: 72, y: 144, width: 180, height: 96 } };
  const moved = { id: command.annotation_id, kind: "rectangle", rect: { x: 90, y: 132, width: 180, height: 96 } };
  const resized = { id: command.annotation_id, kind: "rectangle", rect: { x: 90, y: 132, width: 210, height: 96 } };
  assert.equal(rectangleTransformEvidence(command, {
    before, moved, resized, history_delta: { move: 1, resize: 1 },
  }).passed, true);
  assert.equal(rectangleTransformEvidence(command, {
    before, moved, resized, history_delta: { move: 2, resize: 1 },
  }).passed, false);
  assert.equal(rectangleTransformEvidence(command, {
    before,
    moved: { ...moved, rect: { ...moved.rect, x: 89.9 } },
    resized: { ...resized, rect: { ...resized.rect, x: 89.9, width: 209.1 } },
    history_delta: { move: 1, resize: 1 },
    geometry_tolerance_pt: 1,
  }).passed, true);
});

test("accepts a real rectangle creation as a transform prerequisite without conflating style parity", () => {
  const creation = {
    command_results: [{
      command_id: "rectangle:create-sparse",
      observed_markup_id: "electron-generated-id",
      correctness: { kind_matched: true, style_matched: false },
      observed_milestones: [
        "pointer-stream-received",
        "gesture-committed-once",
        "annotation-painted",
      ],
      exact_fields: { geometry: false, style: false },
    }],
  };

  assert.deepEqual(rectangleTransformPrerequisiteEvidence(creation), {
    passed: true,
    observed_markup_id: "electron-generated-id",
    required_milestones: {
      "pointer-stream-received": true,
      "gesture-committed-once": true,
      "annotation-painted": true,
    },
    rectangle_kind_matched: true,
  });
  assert.equal(rectangleTransformPrerequisiteEvidence({
    command_results: [{
      ...creation.command_results[0],
      observed_milestones: ["pointer-stream-received", "gesture-committed-once"],
    }],
  }).passed, false);
});

test("accepts exact transform semantics as a properties prerequisite without CDP rate timing", () => {
  const transform = {
    command_results: [{
      command_id: "rectangle:select-move-resize",
      observed_markup_id: "electron-generated-id",
      observed_milestones: [
        "hit-test-selected",
        "move-committed-once",
        "resize-committed-once",
      ],
      evidence: { passed: true },
      exact_fields: { move_rate: false, resize_rate: false },
    }],
  };
  assert.deepEqual(rectanglePropertiesTransformPrerequisiteEvidence(transform), {
    passed: true,
    observed_markup_id: "electron-generated-id",
    semantic_milestones: {
      "hit-test-selected": true,
      "move-committed-once": true,
      "resize-committed-once": true,
    },
    geometry_and_history_exact: true,
  });
  assert.equal(rectanglePropertiesTransformPrerequisiteEvidence({
    command_results: [{ ...transform.command_results[0], evidence: { passed: false } }],
  }).passed, false);
});

test("requires exact properties, locked rejection, history replay, and dirty state", () => {
  const command = {
    annotation_id: "comparison:rectangle:sparse:1",
    properties: {
      stroke: "#dc2626ff",
      fill: "#dc26261f",
      width_pt: 3,
      dash: "dashed",
      opacity: 0.88,
    },
  };
  const current = {
    id: command.annotation_id,
    kind: "rectangle",
    rect: { x: 90, y: 132, width: 210, height: 96 },
    appearance: {
      stroke: { color: "#dc2626", widthPt: 3, style: "dashed" },
      fill: { color: "#dc26261f" },
      opacity: 0.88,
    },
  };
  const evidence = rectanglePropertiesHistoryEvidence(command, {
    current,
    locked_edit_rejected: true,
    before_history_replay: current,
    after_history_replay: structuredClone(current),
    dirty: true,
  });
  assert.equal(evidence.passed, true);
  assert.equal(rectanglePropertiesHistoryEvidence(command, {
    current,
    locked_edit_rejected: false,
    before_history_replay: current,
    after_history_replay: current,
    dirty: true,
  }).passed, false);
  assert.equal(rectanglePropertiesHistoryEvidence(command, {
    current: { ...current, appearance: { ...current.appearance, fill: { ...current.appearance.fill, opacity: 1 } } },
    locked_edit_rejected: true,
    before_history_replay: current,
    after_history_replay: current,
    dirty: true,
  }).properties_current, false);
});

test("requires dense Rectangle source replay, index work, exact history, and current overlay and thumbnail", () => {
  const commands = {
    dense: {
      source_commands: [
        "rectangle:create-sparse",
        "rectangle:select-move-resize",
        "rectangle:properties-history",
      ],
    },
    create: {
      pointer_path: { start: { x: 72, y: 144 }, finish: { x: 252, y: 240 } },
    },
    transform: {
      move: { delta: { x: 18, y: -12 } },
      resize: { handle: "east", delta: { x: 30, y: 0 } },
    },
    properties: {
      properties: {
        stroke: "#dc2626ff",
        fill: "#dc26261f",
        width_pt: 3,
        dash: "dashed",
        opacity: 0.88,
      },
    },
  };
  const finalMarkup = {
    id: "electron-dense-rectangle",
    pageIndex: 1,
    kind: "rectangle",
    rect: { x: 90, y: 132, width: 210, height: 96 },
    appearance: {
      stroke: { color: "#dc2626", opacity: 1, widthPt: 3, style: "dashed" },
      fill: { color: "#dc2626", opacity: 31 / 255 },
      opacity: 0.88,
    },
    locked: false,
  };
  const observation = {
    page_index: 1,
    initial_rectangle_count: 100,
    final_rectangle_count: 101,
    final_markup: finalMarkup,
    source_results: [
      {
        command_id: "rectangle:create-sparse",
        manifest_milestones_complete: true,
        input: { expected_sample_count: 361, acknowledged_sample_count: 361 },
        exact_fields: { annotation_id: true, geometry: true, style: true },
      },
      {
        command_id: "rectangle:select-move-resize",
        manifest_milestones_complete: true,
        input: {
          move: { expected_sample_count: 361, acknowledged_sample_count: 361 },
          resize: { expected_sample_count: 361, acknowledged_sample_count: 361 },
        },
        exact_fields: { alias: true, selected: true, geometry_and_history: true },
      },
      {
        command_id: "rectangle:properties-history",
        manifest_milestones_complete: true,
        exact_fields: {
          alias: true,
          properties_current: true,
          locked_edit_rejected: true,
          full_history_replayed: true,
          dirty_current: true,
        },
      },
    ],
    spatial_index: {
      initial_indexed_count: 100,
      final_indexed_count: 101,
      candidate_count: 4,
      queried_cell_count: 2,
      query_hit_target: true,
      seeded_geometry_matched: true,
      subsequent_query_receipt_matched: true,
    },
    annotation_paint: {
      before_editable_layer_renders: 1,
      after_editable_layer_renders: 9,
      before_thumbnail_layer_renders: 1,
      after_thumbnail_layer_renders: 7,
    },
    history: {
      before: { past: 0, future: 0 },
      after: { past: 9, future: 0 },
      undo: { changed: 9, boundary_reached: true },
      redo: { changed: 9, boundary_reached: true },
      before_replay_markup: finalMarkup,
      after_replay_markup: structuredClone(finalMarkup),
    },
    overlay: { rectangle_count: 101, target_present: true },
    thumbnail: { rectangle_count: 101, target_present: true },
  };

  const evidence = rectangleDenseRepeatEvidence(commands, observation);
  assert.equal(evidence.passed, true);
  assert.ok(Object.values(evidence.exact_fields).every(Boolean));

  const untimedSemanticReplay = structuredClone(observation);
  untimedSemanticReplay.geometry_tolerance_pt = 1;
  untimedSemanticReplay.final_markup.rect.x = 90.05;
  untimedSemanticReplay.history.before_replay_markup.rect.x = 90.05;
  untimedSemanticReplay.history.after_replay_markup.rect.x = 90.05;
  untimedSemanticReplay.source_results = [
    {
      command_id: "rectangle:create-sparse",
      manifest_milestones_complete: true,
      input: {
        expected_sample_count: 361,
        acknowledged_sample_count: 361,
        rate_schedule_met: false,
      },
      exact_fields: {
        pointer_path: false,
        annotation_id: true,
        geometry: true,
        style: true,
      },
    },
    {
      command_id: "rectangle:select-move-resize",
      manifest_milestones_complete: true,
      input: {
        move: {
          expected_sample_count: 361,
          acknowledged_sample_count: 361,
          rate_schedule_met: false,
        },
        resize: {
          expected_sample_count: 361,
          acknowledged_sample_count: 361,
          rate_schedule_met: false,
        },
      },
      exact_fields: {
        alias: true,
        selected: true,
        move_rate: false,
        resize_rate: false,
        geometry_and_history: true,
      },
    },
    {
      command_id: "rectangle:properties-history",
      manifest_milestones_complete: true,
      exact_fields: {
        alias: true,
        properties_current: true,
        locked_edit_rejected: true,
        full_history_replayed: true,
        dirty_current: true,
      },
    },
  ];
  assert.equal(rectangleDenseRepeatEvidence(commands, untimedSemanticReplay).passed, true);

  const semanticallyIncompleteReplay = structuredClone(untimedSemanticReplay);
  semanticallyIncompleteReplay.source_results[1].exact_fields.geometry_and_history = false;
  assert.equal(
    rectangleDenseRepeatEvidence(commands, semanticallyIncompleteReplay).exact_fields.source_replay,
    false,
  );
  const incompleteSampleReplay = structuredClone(untimedSemanticReplay);
  incompleteSampleReplay.source_results[1].input.resize.acknowledged_sample_count = 360;
  assert.equal(
    rectangleDenseRepeatEvidence(commands, incompleteSampleReplay).exact_fields.source_replay,
    false,
  );
  assert.equal(rectangleDenseRepeatEvidence(commands, {
    ...observation,
    spatial_index: { ...observation.spatial_index, candidate_count: 101 },
  }).passed, false);
  assert.equal(rectangleDenseRepeatEvidence(commands, {
    ...observation,
    spatial_index: { ...observation.spatial_index, seeded_geometry_matched: false },
  }).passed, false);
  assert.equal(rectangleDenseRepeatEvidence(commands, {
    ...observation,
    spatial_index: { ...observation.spatial_index, subsequent_query_receipt_matched: false },
  }).passed, false);
  assert.equal(rectangleDenseRepeatEvidence(commands, {
    ...observation,
    thumbnail: { rectangle_count: 100, target_present: false },
  }).passed, false);
  assert.equal(rectangleDenseRepeatEvidence(commands, {
    ...observation,
    history: { ...observation.history, redo: { changed: 8, boundary_reached: true } },
  }).passed, false);
});
