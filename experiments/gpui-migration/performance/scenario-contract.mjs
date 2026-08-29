export const protocolVersion = "bp-perf-v2";
export const scenarioContractVersion = "bp-perf-v2-development-subset-7";
export const allowedScenarios = new Set([
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
]);
export const zoomSequence = [
  100, 200, 400, 800, 1600, 400, 100, 800, 200, 100, 1200, 100,
];

export function normalizedPageSequence(pageCount) {
  if (!Number.isInteger(pageCount) || pageCount < 1) {
    throw new Error("page count must be a positive integer");
  }
  const candidates = [
    pageCount,
    Math.ceil(pageCount * 0.08),
    Math.ceil(pageCount * 0.72),
    Math.ceil(pageCount * 0.25),
    Math.ceil(pageCount * 0.90),
    Math.ceil(pageCount * 0.50),
    11,
    Math.ceil(pageCount * 0.958),
    Math.ceil(pageCount * 0.33),
    1,
  ];
  return [...new Set(candidates.map((page) => Math.max(1, Math.min(pageCount, page))))];
}

const comparisonScenarioDefinitions = Object.freeze({
  "viewer-layout": Object.freeze({
    fixture_id: "bp-multi-page-v1",
    fixture_sha256: "517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b",
    command_ids: Object.freeze(["viewer:layout-single", "viewer:layout-continuous"]),
  }),
  "page-navigation": Object.freeze({
    fixture_id: "nasa-apollo-summary-526-v1",
    fixture_sha256: "68d0f3bb93fd1c4e4f3adf99d483e9161f14293a311e25aa6c31241bbbb84049",
    command_ids: Object.freeze(["viewer:navigate-normalized"]),
  }),
  zoom: Object.freeze({
    fixture_id: "usgs-usa-geology-sheet-v1",
    fixture_sha256: "f058179e193ccbc15ca662feff3554102f64ff2114436a5ce6116d6fa5d2a6e2",
    command_ids: Object.freeze(["viewer:zoom-sequence"]),
  }),
  "high-zoom-pan": Object.freeze({
    fixture_id: "usgs-usa-geology-sheet-v1",
    fixture_sha256: "f058179e193ccbc15ca662feff3554102f64ff2114436a5ce6116d6fa5d2a6e2",
    command_ids: Object.freeze(["viewer:pan-usgs"]),
  }),
  "cache-pressure": Object.freeze({
    fixture_id: "bp-multi-page-v1",
    fixture_sha256: "517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b",
    command_ids: Object.freeze(["viewer:cache-pressure"]),
  }),
  "close-reopen": Object.freeze({
    fixture_id: "bp-multi-page-v1",
    fixture_sha256: "517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b",
    command_ids: Object.freeze(["viewer:close-recover-reopen"]),
  }),
  "annotation-create": Object.freeze({
    fixture_id: "bp-annotation-density-v1",
    fixture_sha256: "1dad337b97d573ff971e886eb6509e80e5e1cd07f6ac610ae4f2f9419c666682",
    command_ids: Object.freeze(["rectangle:create-sparse", "highlight:create"]),
  }),
  "annotation-transform": Object.freeze({
    require_exact_fields: true,
    fixture_id: "bp-annotation-density-v1",
    fixture_sha256: "1dad337b97d573ff971e886eb6509e80e5e1cd07f6ac610ae4f2f9419c666682",
    command_ids: Object.freeze(["rectangle:select-move-resize"]),
  }),
  "annotation-properties-history": Object.freeze({
    require_exact_fields: true,
    fixture_id: "bp-annotation-density-v1",
    fixture_sha256: "1dad337b97d573ff971e886eb6509e80e5e1cd07f6ac610ae4f2f9419c666682",
    command_ids: Object.freeze(["rectangle:properties-history"]),
  }),
  "editor-create": Object.freeze({
    require_exact_fields: true,
    fixture_id: "bp-annotation-density-v1",
    fixture_sha256: "1dad337b97d573ff971e886eb6509e80e5e1cd07f6ac610ae4f2f9419c666682",
    command_ids: Object.freeze([
      "text:create",
      "length:set-scale",
      "length:create",
      "image:create",
    ]),
  }),
  "continuous-scroll": Object.freeze({
    fixture_id: "nasa-apollo-summary-526-v1",
    fixture_sha256: "68d0f3bb93fd1c4e4f3adf99d483e9161f14293a311e25aa6c31241bbbb84049",
    command_ids: Object.freeze(["viewer:continuous-scroll"]),
  }),
  "editor-workload": Object.freeze({
    semantic_command_only: true,
    fixture_id: "bp-annotation-density-v1",
    fixture_sha256: "1dad337b97d573ff971e886eb6509e80e5e1cd07f6ac610ae4f2f9419c666682",
    command_ids: Object.freeze([
      "rectangle:create-sparse", "rectangle:select-move-resize", "rectangle:properties-history",
      "rectangle:repeat-dense", "highlight:create", "highlight:edit-history", "text:create",
      "text:edit-resize-history", "length:set-scale", "length:create",
      "length:edit-endpoint-history", "image:create", "image:resize-history",
    ]),
  }),
  "persistence-workload": Object.freeze({
    semantic_command_only: true,
    fixture_id: "bp-annotation-all-v1",
    fixture_sha256: "4a0a94cdbcc08e7ee06504914e5b84d218f2aeb01035b42d62f2275e38d02cbd",
    command_ids: Object.freeze([
      "rectangle:create-sparse", "rectangle:select-move-resize", "rectangle:properties-history",
      "rectangle:repeat-dense", "highlight:create", "highlight:edit-history", "text:create",
      "text:edit-resize-history", "length:set-scale", "length:create",
      "length:edit-endpoint-history", "image:create", "image:resize-history", "unknown:import",
      "unknown:assert-cycle-1", "unknown:assert-cycle-2", "persistence:apply-fixed-state",
      "persistence:save-1", "persistence:reopen-1", "persistence:save-2", "persistence:reopen-2",
    ]),
  }),
});

const openPdfFixture = Object.freeze({
  fixture_id: "nasa-apollo-summary-526-v1",
  fixture_sha256: "68d0f3bb93fd1c4e4f3adf99d483e9161f14293a311e25aa6c31241bbbb84049",
});

export function lockedFixtureForScenario(scenario) {
  const definition = scenario === "open-pdf" ? openPdfFixture : comparisonScenarioDefinitions[scenario];
  if (!definition) throw new Error(`scenario ${scenario} has no locked fixture mapping`);
  return {
    fixture_id: definition.fixture_id,
    fixture_sha256: definition.fixture_sha256,
  };
}

export function buildDevelopmentScenarioContract(
  workload,
  scenario,
  inputLane = "semantic-diagnostic",
) {
  if (!["semantic-diagnostic", "native-x11-xtest"].includes(inputLane)) {
    throw new Error(`unsupported input lane ${inputLane}`);
  }
  const definition = comparisonScenarioDefinitions[scenario];
  if (!definition) return null;
  const commands = workload.journeys
    .flatMap(({ commands: journeyCommands }) => journeyCommands)
    .filter(({ id }) => definition.command_ids.includes(id));
  for (const commandId of definition.command_ids) {
    if (!commands.some(({ id }) => id === commandId)) {
      throw new Error("missing comparison command " + commandId);
    }
  }
  const orderedCommands = definition.command_ids.map((commandId) =>
    commands.find(({ id }) => id === commandId));
  return {
    manifest_id: workload.manifest_id,
    input_lane: inputLane,
    fixture_id: definition.fixture_id,
    fixture_sha256: definition.fixture_sha256,
    command_ids: [...definition.command_ids],
    semantic_command_only: definition.semantic_command_only === true,
    require_exact_fields: definition.require_exact_fields === true,
    commands: orderedCommands,
  };
}
