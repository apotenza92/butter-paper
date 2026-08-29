import assert from "node:assert/strict";
import test from "node:test";

import {
  classifyElectronEngineeringZoomBaselineDefectV6,
  electronEngineeringZoomBaselineDefectIdV6,
} from "./electron-v6-baseline-defect.mjs";

const zoomPercents = [
  100, 200, 400, 800, 1600, 400, 100, 800, 200, 100, 1200, 100,
];

function exactCase() {
  return {
    implementation: "electron",
    journey: "engineering-sheet",
    component: "zoom",
    receipts: [
      {
        command_id: "engineering:zoom-sequence",
        source_command_id: "viewer:zoom-sequence",
        live: true,
        passed: false,
        mapping_status: "exact-semantic-map",
        component_execution_passed: false,
        proven_milestones: [
          "zoom-state-current",
          "stale-generations-presented-zero",
        ],
        missing_milestones: [
          "visible-tiles-bounded",
          "settled-density-at-least-1",
        ],
      },
    ],
    source_command_results: [
      {
        command_id: "viewer:zoom-sequence",
        observations: {
          zoom_results: zoomPercents.map((percent) => ({
            diagnostics: { zoom: percent / 100 },
            presentation_current: true,
            render_page: { errors: 0 },
          })),
          generation_probe: {
            frame_count: 12,
            stale_visible_surface_frames: 0,
          },
        },
      },
    ],
  };
}

test("classifies only the exact retained Electron engineering zoom density defect", () => {
  assert.equal(
    classifyElectronEngineeringZoomBaselineDefectV6(exactCase()),
    electronEngineeringZoomBaselineDefectIdV6,
  );
});

test("rejects every near miss of the retained Electron engineering zoom defect", () => {
  const cases = [
    { label: "wrong journey", mutate: (value) => (value.journey = "nasa-long-document") },
    { label: "extra receipt", mutate: (value) => value.receipts.push(structuredClone(value.receipts[0])) },
    { label: "receipt passed", mutate: (value) => (value.receipts[0].passed = true) },
    { label: "mapping not exact", mutate: (value) => (value.receipts[0].mapping_status = "unmapped") },
    { label: "extra proven milestone", mutate: (value) => value.receipts[0].proven_milestones.push("visible-tiles-bounded") },
    { label: "wrong missing milestone", mutate: (value) => (value.receipts[0].missing_milestones[1] = "preview-current-generation") },
    { label: "reordered zoom", mutate: (value) => value.source_command_results[0].observations.zoom_results.reverse() },
    { label: "presentation not current", mutate: (value) => (value.source_command_results[0].observations.zoom_results[4].presentation_current = false) },
    { label: "stale visible frame", mutate: (value) => (value.source_command_results[0].observations.generation_probe.stale_visible_surface_frames = 1) },
    { label: "no observed generation frame", mutate: (value) => (value.source_command_results[0].observations.generation_probe.frame_count = 0) },
    { label: "render error", mutate: (value) => (value.source_command_results[0].observations.zoom_results[8].render_page.errors = 1) },
  ];

  for (const nearMiss of cases) {
    const value = exactCase();
    nearMiss.mutate(value);
    assert.equal(
      classifyElectronEngineeringZoomBaselineDefectV6(value),
      null,
      nearMiss.label,
    );
  }
});
