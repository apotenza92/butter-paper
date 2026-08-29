import { zoomSequence } from "./scenario-contract.mjs";

export const electronEngineeringZoomBaselineDefectIdV6 =
  "electron-engineering-zoom-density-and-raster-bound-v1";

const exactProvenMilestones = Object.freeze([
  "zoom-state-current",
  "stale-generations-presented-zero",
]);
const exactMissingMilestones = Object.freeze([
  "visible-tiles-bounded",
  "settled-density-at-least-1",
]);

function exactValues(actual, expected) {
  return (
    Array.isArray(actual) &&
    actual.length === expected.length &&
    actual.every((value, index) => value === expected[index])
  );
}

export function classifyElectronEngineeringZoomBaselineDefectV6({
  implementation,
  journey,
  component,
  receipts,
  source_command_results: sourceCommandResults,
}) {
  if (
    implementation !== "electron" ||
    journey !== "engineering-sheet" ||
    component !== "zoom" ||
    !Array.isArray(receipts) ||
    receipts.length !== 1 ||
    !Array.isArray(sourceCommandResults) ||
    sourceCommandResults.length !== 1
  ) {
    return null;
  }

  const receipt = receipts[0];
  const source = sourceCommandResults[0];
  const observations = source?.observations;
  const zoomResults = observations?.zoom_results;
  const generationProbe = observations?.generation_probe;
  const zoomStatesExact =
    Array.isArray(zoomResults) &&
    zoomResults.length === zoomSequence.length &&
    zoomResults.every(
      (observation, index) =>
        observation?.diagnostics?.zoom === zoomSequence[index] / 100,
    );
  const presentationExact =
    zoomStatesExact &&
    zoomResults.every(
      (observation) => observation?.presentation_current === true,
    );
  const noRenderErrors =
    zoomStatesExact &&
    zoomResults.every(
      (observation) =>
        observation?.render_page?.errors === 0 &&
        observation?.render_error == null &&
        observation?.error == null,
    );

  return receipt?.command_id === "engineering:zoom-sequence" &&
    receipt?.source_command_id === "viewer:zoom-sequence" &&
    receipt?.live === true &&
    receipt?.passed === false &&
    receipt?.mapping_status === "exact-semantic-map" &&
    receipt?.component_execution_passed === false &&
    exactValues(receipt?.proven_milestones, exactProvenMilestones) &&
    exactValues(receipt?.missing_milestones, exactMissingMilestones) &&
    source?.command_id === "viewer:zoom-sequence" &&
    presentationExact &&
    noRenderErrors &&
    Number.isInteger(generationProbe?.frame_count) &&
    generationProbe.frame_count > 0 &&
    generationProbe?.stale_visible_surface_frames === 0
    ? electronEngineeringZoomBaselineDefectIdV6
    : null;
}
