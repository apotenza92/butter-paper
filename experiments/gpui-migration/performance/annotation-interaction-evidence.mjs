function finitePoint(point, label) {
  if (!Number.isFinite(point?.x) || !Number.isFinite(point?.y)) {
    throw new Error(`${label} must contain finite x and y`);
  }
  return point;
}

export function buildDeltaPointerStream({ start, delta, rate_hz, duration_ms }) {
  finitePoint(start, "pointer start");
  finitePoint(delta, "pointer delta");
  if (!Number.isInteger(rate_hz) || rate_hz <= 0 || !Number.isInteger(duration_ms) || duration_ms <= 0) {
    throw new Error("pointer rate and duration must be positive integers");
  }
  const intervals = duration_ms * rate_hz / 1_000;
  if (!Number.isInteger(intervals)) throw new Error("pointer duration must contain exact input intervals");
  return Array.from({ length: intervals + 1 }, (_, index) => {
    const t = index / intervals;
    return {
      x: start.x + delta.x * t,
      y: start.y + delta.y * t,
      t_ms: index * 1_000 / rate_hz,
    };
  });
}

function close(left, right, tolerance = 0.001) {
  return Number.isFinite(left) && Number.isFinite(right) && Math.abs(left - right) <= tolerance;
}

function rectangleBounds(markup) {
  return markup?.kind === "rectangle" ? markup.rect : null;
}

export function rectangleTransformPrerequisiteEvidence(creation) {
  const result = creation?.command_results?.find(
    ({ command_id }) => command_id === "rectangle:create-sparse",
  );
  const observed = new Set(result?.observed_milestones ?? []);
  const requiredMilestones = Object.fromEntries([
    "pointer-stream-received",
    "gesture-committed-once",
    "annotation-painted",
  ].map((milestone) => [milestone, observed.has(milestone)]));
  const rectangleKindMatched = result?.correctness?.kind_matched === true;
  const observedMarkupId = typeof result?.observed_markup_id === "string"
    && result.observed_markup_id.length > 0
    ? result.observed_markup_id
    : null;
  return {
    passed: observedMarkupId !== null
      && rectangleKindMatched
      && Object.values(requiredMilestones).every(Boolean),
    observed_markup_id: observedMarkupId,
    required_milestones: requiredMilestones,
    rectangle_kind_matched: rectangleKindMatched,
  };
}

export function rectanglePropertiesTransformPrerequisiteEvidence(transform) {
  const result = transform?.command_results?.find(
    ({ command_id }) => command_id === "rectangle:select-move-resize",
  );
  const observed = new Set(result?.observed_milestones ?? []);
  const semanticMilestones = Object.fromEntries([
    "hit-test-selected",
    "move-committed-once",
    "resize-committed-once",
  ].map((milestone) => [milestone, observed.has(milestone)]));
  const observedMarkupId = typeof result?.observed_markup_id === "string"
    && result.observed_markup_id.length > 0
    ? result.observed_markup_id
    : null;
  const geometryAndHistoryExact = result?.evidence?.passed === true;
  return {
    passed: observedMarkupId !== null
      && geometryAndHistoryExact
      && Object.values(semanticMilestones).every(Boolean),
    observed_markup_id: observedMarkupId,
    semantic_milestones: semanticMilestones,
    geometry_and_history_exact: geometryAndHistoryExact,
  };
}

export function rectangleTransformEvidence(command, { before, moved, resized, history_delta, geometry_tolerance_pt = 0.001 }) {
  const beforeRect = rectangleBounds(before);
  const movedRect = rectangleBounds(moved);
  const resizedRect = rectangleBounds(resized);
  if (!beforeRect || !movedRect || !resizedRect) {
    return { passed: false, blocker: "rectangle-state-missing" };
  }
  const move = command.move.delta;
  const resize = command.resize.delta;
  const hitTestSelected = moved.id === command.annotation_id;
  const moveCommittedOnce = close(movedRect.x, beforeRect.x + move.x, geometry_tolerance_pt)
    && close(movedRect.y, beforeRect.y + move.y, geometry_tolerance_pt)
    && close(movedRect.width, beforeRect.width, geometry_tolerance_pt)
    && close(movedRect.height, beforeRect.height, geometry_tolerance_pt)
    && history_delta?.move === 1;
  const resizeCommittedOnce = command.resize.handle === "east"
    && close(resizedRect.x, movedRect.x, geometry_tolerance_pt)
    && close(resizedRect.y, movedRect.y, geometry_tolerance_pt)
    && close(resizedRect.width, movedRect.width + resize.x, geometry_tolerance_pt)
    && close(resizedRect.height, movedRect.height, geometry_tolerance_pt)
    && history_delta?.resize === 1;
  return {
    passed: hitTestSelected && moveCommittedOnce && resizeCommittedOnce,
    hit_test_selected: hitTestSelected,
    move_committed_once: moveCommittedOnce,
    resize_committed_once: resizeCommittedOnce,
    before_bounds: beforeRect,
    moved_bounds: movedRect,
    resized_bounds: resizedRect,
    history_delta,
    geometry_tolerance_pt,
  };
}

function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]));
  }
  return value;
}

function rgba(value, explicitOpacity) {
  if (typeof value !== "string" || !/^#[0-9a-f]{6}([0-9a-f]{2})?$/i.test(value)) return null;
  return {
    color: value.slice(0, 7).toLowerCase(),
    opacity: explicitOpacity ?? (value.length === 9 ? Number.parseInt(value.slice(7), 16) / 255 : 1),
  };
}

function rgbaMatches(actualColor, actualOpacity, expected) {
  const actual = rgba(actualColor, actualOpacity);
  const target = rgba(expected);
  return actual !== null
    && target !== null
    && actual.color === target.color
    && close(actual.opacity, target.opacity);
}

export function rectanglePropertiesHistoryEvidence(command, {
  current,
  locked_edit_rejected,
  before_history_replay,
  after_history_replay,
  dirty,
}) {
  const properties = command.properties;
  const appearance = current?.appearance;
  const propertiesCurrent = current?.kind === "rectangle"
    && current.id === command.annotation_id
    && rgbaMatches(appearance?.stroke?.color, appearance?.stroke?.opacity, properties.stroke)
    && rgbaMatches(appearance?.fill?.color, appearance?.fill?.opacity, properties.fill)
    && close(appearance?.stroke?.widthPt, properties.width_pt)
    && appearance?.stroke?.style === properties.dash
    && close(appearance?.opacity, properties.opacity);
  const undoRedoExact = JSON.stringify(canonical(before_history_replay))
    === JSON.stringify(canonical(after_history_replay));
  return {
    passed: propertiesCurrent && locked_edit_rejected === true && undoRedoExact && dirty === true,
    properties_current: propertiesCurrent,
    locked_edit_rejected: locked_edit_rejected === true,
    undo_redo_exact: undoRedoExact,
    dirty_current: dirty === true,
  };
}

export function rectangleDenseRepeatEvidence(commands, observation) {
  const sourceIds = commands?.dense?.source_commands ?? [];
  const sourceResults = new Map((observation?.source_results ?? [])
    .map((result) => [result.command_id, result]));
  const requiredSemanticFields = new Map([
    ["rectangle:create-sparse", ["annotation_id", "geometry", "style"]],
    ["rectangle:select-move-resize", ["alias", "selected", "geometry_and_history"]],
    ["rectangle:properties-history", [
      "alias",
      "properties_current",
      "locked_edit_rejected",
      "full_history_replayed",
      "dirty_current",
    ]],
  ]);
  const sourceReplayExact = sourceIds.length === 3 && sourceIds.every((commandId) => {
    const result = sourceResults.get(commandId);
    const requiredFields = requiredSemanticFields.get(commandId);
    const sampleCountExact = (input) => Number.isInteger(input?.expected_sample_count)
      && input.expected_sample_count > 0
      && input.acknowledged_sample_count === input.expected_sample_count;
    const inputExact = commandId === "rectangle:create-sparse"
      ? sampleCountExact(result?.input)
      : commandId === "rectangle:select-move-resize"
        ? sampleCountExact(result?.input?.move) && sampleCountExact(result?.input?.resize)
        : true;
    return result?.manifest_milestones_complete === true
      && requiredFields?.length > 0
      && requiredFields.every((field) => result.exact_fields?.[field] === true)
      && inputExact;
  });

  const create = commands?.create;
  const transform = commands?.transform;
  const properties = commands?.properties?.properties;
  const finalMarkup = observation?.final_markup;
  const start = create?.pointer_path?.start;
  const finish = create?.pointer_path?.finish;
  const move = transform?.move?.delta;
  const resize = transform?.resize?.delta;
  const expectedRect = start && finish && move && resize ? {
    x: start.x + move.x,
    y: start.y + move.y,
    width: Math.abs(finish.x - start.x) + resize.x,
    height: Math.abs(finish.y - start.y) + resize.y,
  } : null;
  const geometryTolerancePt = observation?.geometry_tolerance_pt ?? 0.001;
  const canonicalStateMatched = finalMarkup?.kind === "rectangle"
    && finalMarkup.pageIndex === observation?.page_index
    && expectedRect !== null
    && Object.keys(expectedRect).every((key) => close(
      finalMarkup.rect?.[key],
      expectedRect[key],
      geometryTolerancePt,
    ))
    && rgbaMatches(finalMarkup.appearance?.stroke?.color, finalMarkup.appearance?.stroke?.opacity, properties?.stroke)
    && rgbaMatches(finalMarkup.appearance?.fill?.color, finalMarkup.appearance?.fill?.opacity, properties?.fill)
    && close(finalMarkup.appearance?.stroke?.widthPt, properties?.width_pt)
    && finalMarkup.appearance?.stroke?.style === properties?.dash
    && close(finalMarkup.appearance?.opacity, properties?.opacity)
    && finalMarkup.locked !== true
    && observation?.initial_rectangle_count === 100
    && observation?.final_rectangle_count === 101;

  const spatial = observation?.spatial_index;
  const spatialIndexWorkRecorded = spatial?.initial_indexed_count === 100
    && spatial?.final_indexed_count === 101
    && Number.isInteger(spatial?.candidate_count)
    && spatial.candidate_count > 0
    && spatial.candidate_count < spatial.final_indexed_count
    && Number.isInteger(spatial?.queried_cell_count)
    && spatial.queried_cell_count > 0
    && spatial.query_hit_target === true
    && spatial.seeded_geometry_matched === true
    && spatial.subsequent_query_receipt_matched === true;
  const paint = observation?.annotation_paint;
  const annotationPaintWorkRecorded = Number.isInteger(paint?.before_editable_layer_renders)
    && Number.isInteger(paint?.after_editable_layer_renders)
    && paint.after_editable_layer_renders > paint.before_editable_layer_renders
    && Number.isInteger(paint?.before_thumbnail_layer_renders)
    && Number.isInteger(paint?.after_thumbnail_layer_renders)
    && paint.after_thumbnail_layer_renders > paint.before_thumbnail_layer_renders;
  const history = observation?.history;
  const historyDelta = history?.after?.past - history?.before?.past;
  const exactHistory = Number.isInteger(historyDelta)
    && historyDelta > 0
    && history?.after?.future === 0
    && history.undo?.boundary_reached === true
    && history.redo?.boundary_reached === true
    && history.undo.changed === historyDelta
    && history.redo.changed === historyDelta
    && JSON.stringify(canonical(history.before_replay_markup))
      === JSON.stringify(canonical(history.after_replay_markup));
  const overlayCurrent = observation?.overlay?.rectangle_count === 101
    && observation.overlay.target_present === true;
  const thumbnailCurrent = observation?.thumbnail?.rectangle_count === 101
    && observation.thumbnail.target_present === true;
  const exact_fields = {
    source_replay: sourceReplayExact,
    spatial_index_work: spatialIndexWorkRecorded,
    annotation_paint_work: annotationPaintWorkRecorded,
    canonical_state: canonicalStateMatched,
    exact_history: exactHistory,
    overlay_current: overlayCurrent,
    thumbnail_current: thumbnailCurrent,
  };
  return {
    passed: Object.values(exact_fields).every(Boolean),
    exact_fields,
    expected_rect: expectedRect,
  };
}
