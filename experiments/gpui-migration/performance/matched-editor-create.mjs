const editorCreateCommandIds = Object.freeze([
  "text:create",
  "length:set-scale",
  "length:create",
  "image:create",
]);

export const matchedEditorCreateMinimums = Object.freeze({
  electron: Object.freeze({
    "text:create": Object.freeze([
      "Replay Text Box selection, page-space placement, and text entry through the maintained Electron UI.",
      "Record the exact committed text, bounds, font, history delta, shaped glyph layout, and painted annotation from the live renderer.",
    ]),
    "length:set-scale": Object.freeze([
      "Apply the frozen 72 PDF points = 1 m, precision 2 scale through the maintained page-scale UI and read it back from the document model.",
    ]),
    "length:create": Object.freeze([
      "Replay the two frozen endpoints through the maintained Length tool.",
      "Record the exact endpoints, derived 3.00 m label, one history entry, live label layout, and painted annotation.",
    ]),
    "image:create": Object.freeze([
      "Load the locked bp-image-checker-v1 bytes through the maintained image-file input and place the image at the frozen page-space bounds.",
      "Instrument decoded dimensions and renderer upload bytes, then prove one history entry and a painted annotation.",
    ]),
  }),
  gpui: Object.freeze({
    "text:create": Object.freeze([
      "Replay native text input and placement in the GPUI window, not only the annotation adapter.",
      "Record shaped glyph layout and presented annotation pixels while retaining the exact semantic state proof.",
    ]),
    "length:set-scale": Object.freeze([
      "Replay the scale change through the GPUI control and bind its completion timestamp to the exact semantic scale readback.",
    ]),
    "length:create": Object.freeze([
      "Replay native endpoint input in the GPUI window and record the presented 3.00 m label layout.",
    ]),
    "image:create": Object.freeze([
      "Replay deterministic checker-asset selection and placement through the GPUI window.",
      "Record the actual GPU upload byte count and presented image annotation; prepared RGBA bytes are not an upload measurement.",
    ]),
  }),
});

function commandMap(workload) {
  return new Map(workload.journeys
    .flatMap(({ commands }) => commands)
    .map((command) => [command.id, command]));
}

function equalRecord(actual, expected) {
  return actual != null
    && expected != null
    && Object.entries(expected).every(([key, value]) => typeof value === "number"
      ? Number.isFinite(actual[key]) && Math.abs(actual[key] - value) <= 0.000001
      : actual[key] === value);
}

function containsPoint(bounds, point) {
  return bounds != null && point != null
    && bounds.width > 0 && bounds.height > 0
    && point.x >= bounds.x && point.x <= bounds.x + bounds.width
    && point.y >= bounds.y && point.y <= bounds.y + bounds.height;
}

function naturalImageBounds(command, pageSize) {
  pageSize ??= command?.placement?.fixture_page_size_points;
  if (!pageSize) return null;
  const sourceWidth = 512;
  const sourceHeight = 384;
  const maxWidth = pageSize.width * command.placement.max_page_fraction;
  const maxHeight = pageSize.height * command.placement.max_page_fraction;
  const scale = Math.min(1, maxWidth / sourceWidth, maxHeight / sourceHeight);
  const width = sourceWidth * scale;
  const height = sourceHeight * scale;
  return {
    x: Math.min(Math.max(0, command.placement.point.x - width / 2), pageSize.width - width),
    y: Math.min(Math.max(0, command.placement.point.y - height / 2), pageSize.height - height),
    width,
    height,
  };
}

function semanticStateCheck(command, facts) {
  if (!facts || typeof facts !== "object") {
    return { status: "blocked", reason: "no-command-facts" };
  }
  if (command.id === "text:create") {
    const placementPoint = facts.placement_point ?? facts.input?.pdf_point;
    const passed = facts.content === command.text
      && facts.history_delta === 1
      && equalRecord(placementPoint, command.placement.point)
      && containsPoint(facts.layout_bounds ?? facts.markup?.rect, command.placement.point);
    return {
      status: passed ? "passed" : "failed",
      expected: {
        content: command.text,
        history_delta: 1,
        placement: command.placement,
        layout: "nonblank bounds containing the placement point",
      },
      observed: facts,
    };
  }
  if (command.id === "length:set-scale") {
    const passed = equalRecord(facts, command.scale);
    return { status: passed ? "passed" : "failed", expected: command.scale, observed: facts };
  }
  if (command.id === "length:create") {
    const passed = equalRecord(facts.start, command.start)
      && equalRecord(facts.end, command.finish)
      && facts.caption === command.expected_label
      && facts.history_delta === 1;
    return {
      status: passed ? "passed" : "failed",
      expected: {
        start: command.start,
        end: command.finish,
        caption: command.expected_label,
        history_delta: 1,
      },
      observed: facts,
    };
  }
  const placementPoint = facts.placement_point ?? facts.input?.pdf_point;
  const bounds = facts.bounds ?? facts.markup?.rect;
  const expectedBounds = naturalImageBounds(command, facts.page_size);
  const sourceWidth = facts.source_width_px ?? facts.presentation?.decoded_width;
  const sourceHeight = facts.source_height_px ?? facts.presentation?.decoded_height;
  const passed = equalRecord(placementPoint, command.placement.point)
    && equalRecord(bounds, expectedBounds)
    && sourceWidth === 512
    && sourceHeight === 384
    && facts.history_delta === 1;
  return {
    status: passed ? "passed" : "failed",
    expected: {
      placement: command.placement,
      bounds: expectedBounds,
      source_width_px: 512,
      source_height_px: 384,
      history_delta: 1,
    },
    observed: facts,
  };
}

function latestSuccessfulEvidence(report) {
  const iteration = [...(report?.iterations ?? [])]
    .reverse()
    .find((candidate) => candidate.success === true && candidate.application_success === true);
  if (!iteration) return new Map();
  return new Map(iteration.events
    .filter(({ event }) => event === "comparison-command-evidence")
    .map((event) => [event.command_id, event]));
}

export function assessEditorCreateImplementation(workload, implementation, report = null) {
  if (!(implementation in matchedEditorCreateMinimums)) {
    throw new Error(`unknown implementation ${implementation}`);
  }
  const commands = commandMap(workload);
  const evidence = latestSuccessfulEvidence(report);
  const results = editorCreateCommandIds.map((commandId) => {
    const command = commands.get(commandId);
    if (!command) throw new Error(`workload is missing ${commandId}`);
    const event = evidence.get(commandId);
    if (!event) {
      return {
        command_id: commandId,
        status: "blocked",
        semantic_state: { status: "blocked", reason: "no-command-evidence-from-successful-iteration" },
        proven_milestones: [],
        blocked_milestones: command.expected_milestones.map((milestone) => ({
          milestone,
          reason: implementation === "electron"
            ? "Electron has no maintained editor-create evidence lane"
            : "GPUI report did not contain semantic evidence for this command",
        })),
        minimum_to_unblock: matchedEditorCreateMinimums[implementation][commandId],
      };
    }
    const payload = event.evidence ?? {};
    const proven = new Set(payload.proven_manifest_milestones ?? []);
    const blocked = command.expected_milestones
      .filter((milestone) => !proven.has(milestone))
      .map((milestone) => payload.blocked_manifest_milestones
        ?.find((candidate) => candidate.milestone === milestone)
        ?? { milestone, reason: "milestone-not-proved-by-report" });
    const semanticState = semanticStateCheck(command, payload.facts);
    const liveEvidence = event.evidence_scope === "native-presented"
      && event.decision_timing_eligible === true;
    return {
      command_id: commandId,
      status: semanticState.status === "passed" && blocked.length === 0 && liveEvidence
        ? "ready"
        : semanticState.status === "failed" ? "failed" : "blocked",
      evidence_scope: event.evidence_scope ?? null,
      decision_timing_eligible: event.decision_timing_eligible === true,
      semantic_state: semanticState,
      proven_milestones: [...proven],
      blocked_milestones: blocked,
      minimum_to_unblock: matchedEditorCreateMinimums[implementation][commandId],
    };
  });
  return {
    implementation,
    status: results.every(({ status }) => status === "ready") ? "ready"
      : results.some(({ status }) => status === "failed") ? "failed" : "blocked",
    commands: results,
  };
}

export function assessMatchedEditorCreate(workload, reports = {}) {
  const implementations = ["electron", "gpui"].map((implementation) =>
    assessEditorCreateImplementation(workload, implementation, reports[implementation]));
  return {
    schema_version: 1,
    manifest_id: workload.manifest_id,
    scope: "matched-text-length-image-create",
    status: implementations.every(({ status }) => status === "ready") ? "ready"
      : implementations.some(({ status }) => status === "failed") ? "failed" : "blocked",
    implementations,
  };
}
