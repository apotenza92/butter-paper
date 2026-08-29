import assert from "node:assert/strict";
import test from "node:test";

import { loadComparisonWorkload } from "./comparison-workload.mjs";
import {
  assessElectronUntimedCorrectness,
  buildElectronUntimedCorrectnessPlan,
} from "./electron-untimed-correctness.mjs";

const workload = await loadComparisonWorkload();

function command(plan, id) {
  return plan.commands.find((candidate) => candidate.command_id === id);
}

test("builds an untimed plan that uses only generic maintained-app hooks", () => {
  const plan = buildElectronUntimedCorrectnessPlan(workload);

  assert.equal(plan.diagnostic_untimed, true);
  assert.equal(plan.decision_timing_eligible, false);
  assert.deepEqual(command(plan, "highlight:edit-history").actions, [
    { hook: "selectMarkupAtPoint", args: [0, { x: 90, y: 330 }, 4], capture: "selection" },
    { hook: "applyMarkupMutation", args: [{ kind: "translate", markupId: "comparison:highlight:1", delta: { x: 12, y: -6 } }] },
    { hook: "applyMarkupMutation", args: [{ kind: "set-properties", markupId: "comparison:highlight:1", values: { opacity: 45 } }], capture: "committed" },
    { hook: "undoDocument", args: [], capture: "after_undo" },
    { hook: "redoDocument", args: [], capture: "after_redo" },
  ]);
  assert.deepEqual(command(plan, "text:edit-resize-history").actions.slice(0, 2), [
    { hook: "applyMarkupMutation", args: [{ kind: "replace-text", markupId: "comparison:text:1", text: "Beam B-12 / revision 4" }], capture: "pre_last_commit" },
    { hook: "applyMarkupMutation", args: [{ kind: "set-properties", markupId: "comparison:text:1", values: { x: 90, y: 390, width: 300, height: 84 } }], capture: "committed" },
  ]);
  assert.deepEqual(command(plan, "length:edit-endpoint-history").actions[0], {
    hook: "applyMarkupMutation",
    args: [{
      kind: "tool-transform",
      markupId: "comparison:length:1",
      handleId: "length.endpoint.end",
      handleBehavior: "moveEndpoint",
      startPoint: { x: 306, y: 510 },
      currentPoint: { x: 342, y: 510 },
    }],
    capture: "committed",
  });
  assert.deepEqual(command(plan, "image:resize-history").actions[0], {
    hook: "applyMarkupMutation",
    args: [{ kind: "set-properties", markupId: "comparison:image:1", values: { x: 360, y: 390, width: 180, height: 135 } }],
    capture: "committed",
  });
});

test("classifies exact maintained document and history observations without treating them as timings", () => {
  const highlightBefore = {
    id: "comparison:highlight:1",
    kind: "highlight",
    paths: [[{ x: 90, y: 330 }, { x: 150, y: 337 }, { x: 220, y: 329 }, { x: 300, y: 334 }]],
    appearance: { opacity: 1 },
  };
  const highlightMoved = {
    ...highlightBefore,
    paths: [[{ x: 102, y: 324 }, { x: 162, y: 331 }, { x: 232, y: 323 }, { x: 312, y: 328 }]],
  };
  const highlightCommitted = { ...highlightMoved, appearance: { opacity: 0.45 } };
  const observations = {
    "highlight:edit-history": {
      selected_markup_ids: ["comparison:highlight:1"],
      before_markup: highlightBefore,
      pre_last_commit_markup: highlightMoved,
      committed_markup: highlightCommitted,
      after_undo_markup: highlightMoved,
      after_redo_markup: highlightCommitted,
      history: {
        before: { past: 4, future: 0, currentRevision: 4, savedRevision: 0 },
        after_commit: { past: 6, future: 0, currentRevision: 6, savedRevision: 0 },
        after_undo: { past: 5, future: 1, currentRevision: 5, savedRevision: 0 },
        after_redo: { past: 6, future: 0, currentRevision: 6, savedRevision: 0 },
      },
      thumbnail_current: null,
    },
  };

  const report = assessElectronUntimedCorrectness(workload, observations);
  const highlight = report.commands.find((candidate) => candidate.command_id === "highlight:edit-history");

  assert.equal(report.diagnostic_untimed, true);
  assert.equal(report.decision_timing_eligible, false);
  assert.deepEqual(highlight.passed, ["hit-test-selected", "path-bounds-current", "undo-redo-exact"]);
  assert.deepEqual(highlight.failed, []);
  assert.deepEqual(highlight.blocked, [{
    milestone: "thumbnail-current",
    reason: "thumbnail paint evidence was not captured",
  }]);
});

test("routes dense-repeat outside the generic mutation plan and keeps unknown and persistence non-executable", () => {
  const plan = buildElectronUntimedCorrectnessPlan(workload);

  assert.deepEqual(command(plan, "rectangle:repeat-dense"), {
    command_id: "rectangle:repeat-dense",
    operation: "annotation.rectangle.repeat-on-dense-page",
    expected_milestones: ["spatial-index-work-recorded", "annotation-paint-work-recorded", "canonical-state-matched"],
    capture_before: ["getActiveDocument", "getDocumentHistory"],
    capture_after_each_action: ["getActiveDocument", "getDocumentHistory"],
    executable: false,
    blocked_reason: "dedicated dense live replay executes this command outside the generic mutation-plan loop",
    actions: [],
  });

  assert.equal(command(plan, "unknown:import").executable, false);
  assert.equal(command(plan, "unknown:import").blocked_reason, "unknown PDF annotation import and byte-exact preservation are not integrated");
  assert.deepEqual(command(plan, "unknown:import").actions, []);
  assert.equal(command(plan, "persistence:save-1").executable, false);
  assert.equal(command(plan, "persistence:save-1").blocked_reason, "independently validated save and reopen are not integrated");
  assert.deepEqual(command(plan, "persistence:save-1").actions, []);
  assert.equal(command(plan, "persistence:reopen-2").executable, false);
  assert.deepEqual(command(plan, "persistence:reopen-2").actions, []);

  const report = assessElectronUntimedCorrectness(workload, {});
  const unknown = report.commands.find((candidate) => candidate.command_id === "unknown:assert-cycle-1");
  const save = report.commands.find((candidate) => candidate.command_id === "persistence:save-1");

  assert.deepEqual(unknown.blocked, [
    { milestone: "unknown-dictionary-exact", reason: "raw serialized annotation dictionary evidence was not captured" },
    { milestone: "unknown-appearance-stream-exact", reason: "raw serialized appearance-stream evidence was not captured" },
  ]);
  assert.deepEqual(save.blocked, [
    { milestone: "safe-publication-complete", reason: "save completion evidence was not captured" },
    { milestone: "independent-pdf-validation-passed", reason: "independent PDF validation evidence was not captured" },
    { milestone: "native-annotations-valid", reason: "native annotation inspection evidence was not captured" },
    { milestone: "appearance-streams-valid", reason: "appearance-stream inspection evidence was not captured" },
  ]);
});

test("passes exact Highlight, Text, and Length edits but retains the Image upload blocker", () => {
  const history = (commitCount) => ({
    before: { past: 5, future: 0, currentRevision: 5, savedRevision: 0 },
    after_commit: { past: 5 + commitCount, future: 0, currentRevision: 5 + commitCount, savedRevision: 0 },
    after_undo: { past: 4 + commitCount, future: 1, currentRevision: 4 + commitCount, savedRevision: 0 },
    after_redo: { past: 5 + commitCount, future: 0, currentRevision: 5 + commitCount, savedRevision: 0 },
  });
  const highlightBefore = {
    id: "actual-highlight",
    kind: "highlight",
    paths: [[{ x: 90, y: 330 }, { x: 150, y: 337 }, { x: 220, y: 329 }, { x: 300, y: 334 }]],
    appearance: { opacity: 1 },
  };
  const highlightMoved = {
    ...highlightBefore,
    paths: highlightBefore.paths.map((path) => path.map(({ x, y }) => ({ x: x + 12, y: y - 6 }))),
  };
  const highlightCommitted = { ...highlightMoved, appearance: { opacity: 0.45 } };
  const textBefore = {
    id: "actual-text", kind: "text", text: "Beam B-12 / revision 3",
    rect: { x: 210, y: 426, width: 120, height: 20 }, fontFamily: "Helvetica", fontSizePt: 14,
  };
  const textReplaced = { ...textBefore, text: "Beam B-12 / revision 4" };
  const textCommitted = { ...textReplaced, rect: { x: 90, y: 390, width: 300, height: 84 } };
  const lengthBefore = { id: "actual-length", kind: "length", start: { x: 90, y: 510 }, end: { x: 306, y: 510 } };
  const lengthCommitted = { ...lengthBefore, end: { x: 342, y: 510 } };
  const imageBefore = {
    id: "actual-image", kind: "image", rect: { x: 294.3, y: 340.725, width: 275.4, height: 206.55 },
  };
  const imageCommitted = { ...imageBefore, rect: { x: 360, y: 390, width: 180, height: 135 } };
  const report = assessElectronUntimedCorrectness(workload, {
    "highlight:edit-history": {
      selected_markup_ids: ["comparison:highlight:1"], before_markup: highlightBefore,
      pre_last_commit_markup: highlightMoved, committed_markup: highlightCommitted,
      after_undo_markup: highlightMoved, after_redo_markup: highlightCommitted,
      history: history(2), thumbnail_current: true,
    },
    "text:edit-resize-history": {
      selected_markup_ids: ["comparison:text:1"], before_markup: textBefore,
      pre_last_commit_markup: textReplaced, committed_markup: textCommitted,
      after_undo_markup: textReplaced, after_redo_markup: textCommitted, history: history(2),
    },
    "length:edit-endpoint-history": {
      selected_markup_ids: ["comparison:length:1"], before_markup: lengthBefore,
      pre_last_commit_markup: lengthBefore, committed_markup: lengthCommitted,
      after_undo_markup: lengthBefore, after_redo_markup: lengthCommitted,
      history: history(1), derived_label: "3.50 m", thumbnail_current: true,
    },
    "image:resize-history": {
      selected_markup_ids: ["comparison:image:1"], before_markup: imageBefore,
      pre_last_commit_markup: imageBefore, committed_markup: imageCommitted,
      after_undo_markup: imageBefore, after_redo_markup: imageCommitted,
      history: history(1), upload_byte_count_recorded: null, thumbnail_current: true,
    },
  });

  assert.deepEqual(report.summary.passed, [
    "highlight:edit-history", "text:edit-resize-history", "length:edit-endpoint-history",
  ]);
  const image = report.commands.find(({ command_id: id }) => id === "image:resize-history");
  assert.deepEqual(image.passed, ["aspect-ratio-current", "undo-redo-exact", "thumbnail-current"]);
  assert.deepEqual(image.blocked, [{
    milestone: "upload-byte-count-recorded",
    reason: "decoded image upload-byte evidence was not captured",
  }]);
});
