const gateCommandIds = Object.freeze([
  "rectangle:repeat-dense",
  "highlight:edit-history",
  "text:edit-resize-history",
  "length:edit-endpoint-history",
  "image:resize-history",
  "unknown:import",
  "unknown:assert-cycle-1",
  "unknown:assert-cycle-2",
  "persistence:apply-fixed-state",
  "persistence:save-1",
  "persistence:reopen-1",
  "persistence:save-2",
  "persistence:reopen-2",
]);

const executableCommandIds = new Set([
  "highlight:edit-history",
  "text:edit-resize-history",
  "length:edit-endpoint-history",
  "image:resize-history",
]);

function blockedExecutionReason(id) {
  if (id === "rectangle:repeat-dense") {
    return "dedicated dense live replay executes this command outside the generic mutation-plan loop";
  }
  if (id.startsWith("unknown:")) {
    return "unknown PDF annotation import and byte-exact preservation are not integrated";
  }
  if (id.startsWith("persistence:")) {
    return "independently validated save and reopen are not integrated";
  }
  return null;
}

function commandsById(workload) {
  return new Map((workload?.journeys ?? []).flatMap((journey) => journey.commands ?? [])
    .map((command) => [command.id, command]));
}

function requiredCommand(commands, id) {
  const command = commands.get(id);
  if (!command) throw new Error(`Comparison workload command is missing: ${id}`);
  return command;
}

function point([x, y]) {
  return { x, y };
}

function historyActions() {
  return [
    { hook: "undoDocument", args: [], capture: "after_undo" },
    { hook: "redoDocument", args: [], capture: "after_redo" },
  ];
}

function actionsForCommand(command, commands) {
  if (command.id === "rectangle:repeat-dense") {
    return [];
  }
  if (command.id === "highlight:edit-history") {
    const create = requiredCommand(commands, "highlight:create");
    return [
      { hook: "selectMarkupAtPoint", args: [0, point(create.pointer_path.control_points[0]), 4], capture: "selection" },
      { hook: "applyMarkupMutation", args: [{ kind: "translate", markupId: command.annotation_id, delta: command.move_delta }] },
      { hook: "applyMarkupMutation", args: [{ kind: "set-properties", markupId: command.annotation_id, values: { opacity: 45 } }], capture: "committed" },
      ...historyActions(),
    ];
  }
  if (command.id === "text:edit-resize-history") {
    return [
      { hook: "applyMarkupMutation", args: [{ kind: "replace-text", markupId: command.annotation_id, text: command.replacement_text }], capture: "pre_last_commit" },
      { hook: "applyMarkupMutation", args: [{ kind: "set-properties", markupId: command.annotation_id, values: command.resize_bounds }], capture: "committed" },
      ...historyActions(),
    ];
  }
  if (command.id === "length:edit-endpoint-history") {
    const create = requiredCommand(commands, "length:create");
    const startPoint = command.endpoint === "start" ? create.start : create.finish;
    return [
      {
        hook: "applyMarkupMutation",
        args: [{
          kind: "tool-transform",
          markupId: command.annotation_id,
          handleId: `length.endpoint.${command.endpoint === "start" ? "start" : "end"}`,
          handleBehavior: "moveEndpoint",
          startPoint,
          currentPoint: command.replacement,
        }],
        capture: "committed",
      },
      ...historyActions(),
    ];
  }
  if (command.id === "image:resize-history") {
    return [
      { hook: "applyMarkupMutation", args: [{ kind: "set-properties", markupId: command.annotation_id, values: command.replacement_bounds }], capture: "committed" },
      ...historyActions(),
    ];
  }
  if (command.operation === "persistence.safe-save") {
    return [{ hook: "saveCurrentDocumentAs", args: [{ output: command.output }], capture: "saved" }];
  }
  if (command.operation === "persistence.reopen-and-compare") {
    return [
      { hook: "closeTab", args: [0] },
      { hook: "openDocumentPath", args: [{ output: `cycle-${command.cycle}.pdf` }], capture: "reopened" },
    ];
  }
  if (command.id === "persistence:apply-fixed-state") {
    return [{ hook: "getActiveDocument", args: [], capture: "canonical_document" }];
  }
  return [];
}

export function buildElectronUntimedCorrectnessPlan(workload) {
  const commands = commandsById(workload);
  return {
    schema_version: "bp-electron-untimed-correctness-plan-v1",
    manifest_id: workload?.manifest_id ?? null,
    diagnostic_untimed: true,
    decision_timing_eligible: false,
    required_hook_methods: [
      "selectMarkupAtPoint",
      "applyMarkupMutation",
      "undoDocument",
      "redoDocument",
      "getDocumentHistory",
      "getActiveDocument",
      "getDiagnostics",
      "getPerfSnapshot",
    ],
    commands: gateCommandIds.map((id) => {
      const command = requiredCommand(commands, id);
      const blockedReason = blockedExecutionReason(id);
      return {
        command_id: id,
        operation: command.operation,
        expected_milestones: [...command.expected_milestones],
        capture_before: ["getActiveDocument", "getDocumentHistory"],
        capture_after_each_action: ["getActiveDocument", "getDocumentHistory"],
        ...(blockedReason ? { executable: false, blocked_reason: blockedReason } : {}),
        actions: executableCommandIds.has(id) ? actionsForCommand(command, commands) : [],
      };
    }),
  };
}

function exact(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function expectedAnnotation(workload, id) {
  return workload?.expected?.final_state?.annotations?.find((annotation) => annotation.id === id) ?? null;
}

function markupById(document, id) {
  return document?.markups?.find((markup) => markup.id === id) ?? null;
}

function boundsMatch(markup, expected) {
  return exact(markup?.rect, expected?.bounds);
}

function historyRoundTripExact(observation, commitCount) {
  const history = observation?.history;
  if (!history?.before || !history?.after_commit || !history?.after_undo || !history?.after_redo) return null;
  return exact(observation.after_undo_markup, observation.pre_last_commit_markup)
    && exact(observation.after_redo_markup, observation.committed_markup)
    && history.after_commit.past === history.before.past + commitCount
    && history.after_commit.future === 0
    && history.after_undo.past === history.after_commit.past - 1
    && history.after_undo.future === 1
    && history.after_redo.past === history.after_commit.past
    && history.after_redo.future === 0
    && history.after_redo.currentRevision === history.after_commit.currentRevision
    && history.after_redo.savedRevision === history.after_commit.savedRevision;
}

const missingEvidenceReasons = Object.freeze({
  "spatial-index-work-recorded": "spatial-index diagnostic evidence was not captured",
  "annotation-paint-work-recorded": "annotation paint-work evidence was not captured",
  "canonical-state-matched": "canonical maintained-document evidence was not captured",
  "hit-test-selected": "maintained hit-test selection evidence was not captured",
  "path-bounds-current": "maintained Highlight geometry evidence was not captured",
  "selection-current": "maintained selection evidence was not captured",
  "layout-current": "maintained text layout evidence was not captured",
  "font-persistence-recorded": "maintained text font evidence was not captured",
  "control-point-current": "maintained Length endpoint evidence was not captured",
  "derived-length-exact": "maintained derived Length label evidence was not captured",
  "aspect-ratio-current": "maintained Image bounds evidence was not captured",
  "upload-byte-count-recorded": "decoded image upload-byte evidence was not captured",
  "undo-redo-exact": "maintained history round-trip evidence was not captured",
  "thumbnail-current": "thumbnail paint evidence was not captured",
  "unknown-dictionary-snapshotted": "raw serialized annotation dictionary evidence was not captured",
  "unknown-appearance-stream-snapshotted": "raw serialized appearance-stream evidence was not captured",
  "unknown-dictionary-exact": "raw serialized annotation dictionary evidence was not captured",
  "unknown-appearance-stream-exact": "raw serialized appearance-stream evidence was not captured",
  "dirty-current": "maintained dirty-state evidence was not captured",
  "safe-publication-complete": "save completion evidence was not captured",
  "independent-pdf-validation-passed": "independent PDF validation evidence was not captured",
  "native-annotations-valid": "native annotation inspection evidence was not captured",
  "appearance-streams-valid": "appearance-stream inspection evidence was not captured",
  "document-reopened": "reopen completion evidence was not captured",
  "page-content-preserved": "independent page-content evidence was not captured",
  "page-boxes-rotation-metadata-preserved": "independent page metadata evidence was not captured",
  "fixed-crops-matched": "fixed-crop visual evidence was not captured",
  "dirty-published": "published dirty-state evidence was not captured",
});

function checksForCommand(workload, command, observation) {
  const checks = {};
  if (command.id === "rectangle:repeat-dense") {
    checks["spatial-index-work-recorded"] = observation?.spatial_index_work_recorded ?? null;
    checks["annotation-paint-work-recorded"] = observation?.annotation_paint_work_recorded ?? null;
    checks["canonical-state-matched"] = observation?.canonical_state_matched ?? null;
  } else if (command.id === "highlight:edit-history") {
    const expected = expectedAnnotation(workload, command.annotation_id);
    const expectedPaths = observation?.before_markup?.paths?.map((path) => path.map((candidate) => ({
      x: candidate.x + command.move_delta.x,
      y: candidate.y + command.move_delta.y,
    }))) ?? null;
    checks["hit-test-selected"] = observation
      ? exact(observation.selected_markup_ids, [command.annotation_id])
      : null;
    checks["path-bounds-current"] = observation?.after_redo_markup
      ? expectedPaths !== null
        && exact(observation.after_redo_markup.paths, expectedPaths)
        && observation.after_redo_markup.appearance?.opacity === expected?.style?.opacity
      : null;
    checks["undo-redo-exact"] = historyRoundTripExact(observation, 2);
    checks["thumbnail-current"] = observation?.thumbnail_current ?? null;
  } else if (command.id === "text:edit-resize-history") {
    const expected = expectedAnnotation(workload, command.annotation_id);
    const markup = observation?.after_redo_markup;
    checks["selection-current"] = observation
      ? exact(observation.selected_markup_ids, [command.annotation_id])
      : null;
    checks["layout-current"] = markup
      ? markup.text === expected?.text && boundsMatch(markup, expected)
      : null;
    checks["font-persistence-recorded"] = markup
      ? markup.fontFamily === expected?.font?.family && markup.fontSizePt === expected?.font?.size_pt
      : null;
    checks["undo-redo-exact"] = historyRoundTripExact(observation, 2);
  } else if (command.id === "length:edit-endpoint-history") {
    const expected = expectedAnnotation(workload, command.annotation_id);
    const markup = observation?.after_redo_markup;
    checks["control-point-current"] = markup
      ? exact(markup.start, expected?.start) && exact(markup.end, expected?.finish)
      : null;
    checks["derived-length-exact"] = observation?.derived_label === expected?.label
      ? true
      : observation?.derived_label == null ? null : false;
    checks["undo-redo-exact"] = historyRoundTripExact(observation, 1);
    checks["thumbnail-current"] = observation?.thumbnail_current ?? null;
  } else if (command.id === "image:resize-history") {
    const expected = expectedAnnotation(workload, command.annotation_id);
    const markup = observation?.after_redo_markup;
    checks["aspect-ratio-current"] = markup
      ? boundsMatch(markup, expected)
        && markup.rect.width / markup.rect.height === expected.bounds.width / expected.bounds.height
      : null;
    checks["upload-byte-count-recorded"] = observation?.upload_byte_count_recorded ?? null;
    checks["undo-redo-exact"] = historyRoundTripExact(observation, 1);
    checks["thumbnail-current"] = observation?.thumbnail_current ?? null;
  } else if (command.id === "unknown:import") {
    checks["unknown-dictionary-snapshotted"] = observation?.dictionary_snapshotted ?? null;
    checks["unknown-appearance-stream-snapshotted"] = observation?.appearance_stream_snapshotted ?? null;
  } else if (command.operation === "annotation.unknown.assert-preserved") {
    checks["unknown-dictionary-exact"] = observation?.dictionary_byte_exact ?? null;
    checks["unknown-appearance-stream-exact"] = observation?.appearance_stream_byte_exact ?? null;
  } else if (command.id === "persistence:apply-fixed-state") {
    checks["canonical-state-matched"] = observation?.canonical_state_matched ?? null;
    checks["dirty-current"] = observation?.dirty_current ?? null;
  } else if (command.operation === "persistence.safe-save") {
    checks["safe-publication-complete"] = observation?.save_completed ?? null;
    checks["independent-pdf-validation-passed"] = observation?.independent_pdf_validation_passed ?? null;
    checks["native-annotations-valid"] = observation?.native_annotations_valid ?? null;
    checks["appearance-streams-valid"] = observation?.appearance_streams_valid ?? null;
  } else if (command.operation === "persistence.reopen-and-compare") {
    checks["document-reopened"] = observation?.document_reopened ?? null;
    checks["canonical-state-matched"] = observation?.canonical_state_matched ?? null;
    checks["page-content-preserved"] = observation?.page_content_preserved ?? null;
    checks["page-boxes-rotation-metadata-preserved"] = observation?.page_metadata_preserved ?? null;
    checks["fixed-crops-matched"] = observation?.fixed_crops_matched ?? null;
    if (command.expected_milestones.includes("dirty-published")) {
      checks["dirty-published"] = observation?.dirty_published ?? null;
    }
  }
  return checks;
}

function classifyCommand(workload, command, observation) {
  const checks = checksForCommand(workload, command, observation);
  const passed = [];
  const failed = [];
  const blocked = [];
  for (const milestone of command.expected_milestones) {
    if (checks[milestone] === true) {
      passed.push(milestone);
    } else if (checks[milestone] === false) {
      failed.push({ milestone, reason: `captured evidence did not satisfy ${milestone}` });
    } else {
      blocked.push({
        milestone,
        reason: missingEvidenceReasons[milestone] ?? `evidence was not captured for ${milestone}`,
      });
    }
  }
  return {
    command_id: command.id,
    status: failed.length > 0 ? "failed" : blocked.length > 0 ? "blocked" : "passed",
    passed,
    failed,
    blocked,
  };
}

export function assessElectronUntimedCorrectness(workload, observations = {}) {
  const commands = commandsById(workload);
  const results = gateCommandIds.map((id) => {
    const command = requiredCommand(commands, id);
    return classifyCommand(workload, command, observations[id]);
  });
  return {
    schema_version: "bp-electron-untimed-correctness-report-v1",
    manifest_id: workload?.manifest_id ?? null,
    diagnostic_untimed: true,
    decision_timing_eligible: false,
    commands: results,
    summary: {
      passed: results.filter((result) => result.status === "passed").map((result) => result.command_id),
      failed: results.filter((result) => result.status === "failed").map((result) => result.command_id),
      blocked: results.filter((result) => result.status === "blocked").map((result) => result.command_id),
    },
  };
}
