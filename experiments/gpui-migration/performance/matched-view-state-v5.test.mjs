import assert from "node:assert/strict";
import test from "node:test";

import {
  buildViewStateReceiptV5,
  compareBundleViewStatesV5,
  compareViewStateReceiptsV5,
} from "./matched-view-state-v5.mjs";

function stateEvent(checkpoint, overrides = {}) {
  return {
    event: "comparison-view-state",
    component: "zoom",
    checkpoint,
    observation_source: "live-application-render-state",
    live: true,
    window_bounds_window_logical: {
      x: 10,
      y: 20,
      width: 1280,
      height: 800,
    },
    viewport_bounds_window_logical: {
      x: 310,
      y: 140,
      width: 970,
      height: 660,
    },
    display_scale_factor: 1,
    layout_mode: "single-page",
    zoom_mode: "fit-width",
    zoom_percent: 111.234,
    left_sidebar_visible: true,
    left_sidebar_width_logical: 240,
    right_sidebar_visible: false,
    right_sidebar_width_logical: 0,
    active_fixture_id: "engineering",
    active_document_index: 0,
    open_document_count: 1,
    ...overrides,
  };
}

function buildReceipt(implementation, events) {
  return buildViewStateReceiptV5(
    { iterations: [{ events }] },
    {
      implementation,
      journey: "engineering-sheet",
      component: "zoom",
      fixture_ids: ["engineering"],
    },
  );
}

test("builds a hashed receipt only from two complete live observations", () => {
  const assessment = buildReceipt("electron", [
    stateEvent("measurement-start", {
      active_fixture_id: null,
      active_document_index: null,
      open_document_count: 0,
    }),
    stateEvent("measurement-end", { zoom_percent: 100 }),
  ]);
  assert.equal(assessment.passed, true);
  assert.match(assessment.receipt.evidence_sha256, /^[0-9a-f]{64}$/);
  assert.deepEqual(
    assessment.receipt.snapshots.map(({ checkpoint }) => checkpoint),
    ["measurement-start", "measurement-end"],
  );
});

test("fails closed for missing, duplicated, declared, or invalid view state", () => {
  const missing = buildReceipt("electron", [stateEvent("measurement-start")]);
  assert.equal(missing.passed, false);
  assert(
    missing.failures.some((failure) => failure.includes("measurement-end")),
  );

  const duplicate = buildReceipt("electron", [
    stateEvent("measurement-start"),
    stateEvent("measurement-start"),
    stateEvent("measurement-end"),
  ]);
  assert.equal(duplicate.passed, false);

  const declared = buildReceipt("electron", [
    stateEvent("measurement-start", { live: false }),
    stateEvent("measurement-end"),
  ]);
  assert.equal(declared.passed, false);

  const invalid = buildReceipt("electron", [
    stateEvent("measurement-start", {
      viewport_bounds_window_logical: {
        x: 1200,
        y: 700,
        width: 500,
        height: 500,
      },
      left_sidebar_visible: false,
      left_sidebar_width_logical: 240,
      active_fixture_id: "not-launched",
    }),
    stateEvent("measurement-end"),
  ]);
  assert.equal(invalid.passed, false);
  assert(invalid.failures.some((failure) => failure.includes("outside")));
  assert(invalid.failures.some((failure) => failure.includes("disagree")));
  assert(
    invalid.failures.some((failure) => failure.includes("active fixture")),
  );
});

test("accepts subpixel observation noise and rejects material layout drift", () => {
  const electron = buildReceipt("electron", [
    stateEvent("measurement-start"),
    stateEvent("measurement-end"),
  ]).receipt;
  const gpui = buildReceipt("gpui", [
    stateEvent("measurement-start", {
      viewport_bounds_window_logical: {
        x: 310.25,
        y: 140,
        width: 969.75,
        height: 660,
      },
      zoom_percent: 111.25,
    }),
    stateEvent("measurement-end", { zoom_percent: 111.25 }),
  ]).receipt;
  assert.equal(compareViewStateReceiptsV5(electron, gpui).passed, true);

  gpui.snapshots[1].layout_mode = "continuous";
  gpui.snapshots[1].left_sidebar.width_logical = 220;
  const mismatch = compareViewStateReceiptsV5(electron, gpui);
  assert.equal(mismatch.passed, false);
  assert(mismatch.failures.some((failure) => failure.includes("layout_mode")));
  assert(
    mismatch.failures.some((failure) =>
      failure.includes("left_sidebar.width_logical"),
    ),
  );
});

test("pairs bundle components and skips only explicit benefit-ineligible results", () => {
  const electronReceipt = buildReceipt("electron", [
    stateEvent("measurement-start"),
    stateEvent("measurement-end"),
  ]).receipt;
  const gpuiReceipt = buildReceipt("gpui", [
    stateEvent("measurement-start"),
    stateEvent("measurement-end"),
  ]).receipt;
  const bundle = (implementation, receipt) => ({
    phase: "final",
    journey: "engineering-sheet",
    pair: 1,
    implementation,
    components: [
      {
        component: "zoom",
        benefit_metrics_eligible: true,
        view_state_receipt: receipt,
      },
      {
        component: "known-defect",
        benefit_metrics_eligible: implementation === "gpui",
        view_state_receipt: null,
      },
    ],
  });
  const paired = compareBundleViewStatesV5(
    bundle("electron", electronReceipt),
    bundle("gpui", gpuiReceipt),
  );
  assert.equal(paired.passed, true);
  assert.equal(paired.component_matches[0].applicable, true);
  assert.equal(paired.component_matches[1].applicable, false);
});
