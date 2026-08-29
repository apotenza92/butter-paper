import assert from "node:assert/strict";
import test from "node:test";

import { loadComparisonWorkload } from "./comparison-workload.mjs";
import {
  assessEditorCreateImplementation,
  assessMatchedEditorCreate,
  matchedEditorCreateMinimums,
} from "./matched-editor-create.mjs";

function evidence(commandId, proven, facts, blocked = []) {
  return {
    event: "comparison-command-evidence",
    command_id: commandId,
    evidence_scope: "domain-semantic",
    all_manifest_milestones_proven: blocked.length === 0,
    decision_timing_eligible: false,
    evidence: {
      command_id: commandId,
      proven_manifest_milestones: proven,
      blocked_manifest_milestones: blocked,
      facts,
    },
  };
}

function gpuiSemanticReport() {
  return {
    iterations: [{
      success: true,
      application_success: true,
      events: [
        evidence("text:create", ["text-input-committed", "gesture-committed-once"], {
          content: "Beam B-12 / revision 3",
          history_delta: 1,
          placement_point: { x: 210, y: 426 },
          layout_bounds: { x: 90, y: 390, width: 240, height: 72 },
        }, [
          { milestone: "text-shaped", reason: "requires observed GPUI paint, presentation, or GPU evidence" },
          { milestone: "annotation-painted", reason: "requires observed GPUI paint, presentation, or GPU evidence" },
        ]),
        evidence("length:set-scale", ["measurement-scale-current"], {
          paper_points: 72, real_world_value: 1, unit: "m", precision: 2,
        }),
        evidence("length:create", ["derived-length-exact", "gesture-committed-once"], {
          start: { x: 90, y: 510 }, end: { x: 306, y: 510 }, caption: "3.00 m", history_delta: 1,
        }, [{ milestone: "label-layout-current", reason: "requires observed GPUI paint, presentation, or GPU evidence" }]),
        evidence("image:create", ["bitmap-decoded", "gesture-committed-once"], {
          placement_point: { x: 432, y: 444 },
          page_size: { width: 612, height: 792 },
          bounds: { x: 294.3, y: 340.725, width: 275.4, height: 206.55 },
          source_width_px: 512, source_height_px: 384, rgba_bytes: 786432, history_delta: 1,
        }, [
          { milestone: "bitmap-upload-recorded", reason: "requires observed GPUI paint, presentation, or GPU evidence" },
          { milestone: "annotation-painted", reason: "requires observed GPUI paint, presentation, or GPU evidence" },
        ]),
      ],
    }],
  };
}

test("GPUI semantic evidence passes exact final state without pretending to prove presentation", async () => {
  const report = assessEditorCreateImplementation(
    await loadComparisonWorkload(),
    "gpui",
    gpuiSemanticReport(),
  );

  assert.equal(report.status, "blocked");
  assert.ok(report.commands.every(({ semantic_state }) => semantic_state.status === "passed"));
  assert.deepEqual(
    report.commands.find(({ command_id }) => command_id === "length:set-scale").blocked_milestones,
    [],
  );
  assert.equal(report.commands.find(({ command_id }) => command_id === "length:set-scale").status, "blocked");
  assert.deepEqual(
    report.commands.find(({ command_id }) => command_id === "image:create").blocked_milestones.map(({ milestone }) => milestone),
    ["bitmap-upload-recorded", "annotation-painted"],
  );
});

test("missing Electron evidence produces explicit implementation work instead of readiness", async () => {
  const report = assessEditorCreateImplementation(await loadComparisonWorkload(), "electron");

  assert.equal(report.status, "blocked");
  assert.ok(report.commands.every(({ status }) => status === "blocked"));
  assert.match(report.commands[0].blocked_milestones[0].reason, /no maintained editor-create evidence lane/);
  assert.match(matchedEditorCreateMinimums.electron["image:create"][0], /locked bp-image-checker-v1/);
});

test("matched readiness remains blocked when only GPUI semantic state exists", async () => {
  const report = assessMatchedEditorCreate(await loadComparisonWorkload(), {
    gpui: gpuiSemanticReport(),
  });

  assert.equal(report.status, "blocked");
  assert.equal(report.implementations.find(({ implementation }) => implementation === "electron").status, "blocked");
  assert.equal(report.implementations.find(({ implementation }) => implementation === "gpui").status, "blocked");
});

test("failed final state is distinct from blocked visual evidence", async () => {
  const report = gpuiSemanticReport();
  report.iterations[0].events.find(({ command_id }) => command_id === "length:create")
    .evidence.facts.caption = "3.01 m";

  const assessed = assessEditorCreateImplementation(await loadComparisonWorkload(), "gpui", report);
  assert.equal(assessed.status, "failed");
  assert.equal(
    assessed.commands.find(({ command_id }) => command_id === "length:create").semantic_state.status,
    "failed",
  );
});
