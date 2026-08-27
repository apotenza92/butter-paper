//! Exact manifest-backed rectangle transform and properties/history replay.
//!
//! This is a semantic correctness lane. It exercises the same public adapter
//! used by the GPUI surface, but it does not claim operating-system input or
//! presentation timing.

use serde_json::Value;

use crate::{
    annotation_adapter::{AnnotationAdapter, AnnotationTool},
    annotation_model::{MarkupId, PdfPoint, RectangleAppearance, StrokeStyle},
    comparison_scenario::{
        ComparisonScenarioError, ComparisonScenarioKind, ComparisonScenarioPlan,
    },
};

const WORKLOAD_JSON: &str = include_str!("../../performance/comparison-workload.json");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RectangleCommandEvidence {
    pub command_id: String,
    pub proven_milestones: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RectangleInteractionReport {
    pub command_evidence: Vec<RectangleCommandEvidence>,
    pub final_rect: [f64; 4],
    pub history_depths: (usize, usize),
    pub dirty: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativeRectangleTransformPlan {
    pub command_id: String,
    pub annotation_id: String,
    pub create_start: [f64; 2],
    pub create_finish: [f64; 2],
    pub create_sample_count: usize,
    pub create_rate_hz: u32,
    pub create_duration_ms: u64,
    pub select_point: [f64; 2],
    pub move_delta: [f64; 2],
    pub move_sample_count: usize,
    pub move_rate_hz: u32,
    pub move_duration_ms: u64,
    pub resize_handle: String,
    pub resize_delta: [f64; 2],
    pub resize_sample_count: usize,
    pub resize_rate_hz: u32,
    pub resize_duration_ms: u64,
    pub expected_final_rect: [f64; 4],
    pub maximum_geometry_error_device_px: f64,
    pub expected_milestones: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeRectangleTransformObservation {
    pub hit_test_selected: bool,
    pub create_history_delta: usize,
    pub move_history_delta: usize,
    pub resize_history_delta: usize,
    pub observed_final_rect: [f64; 4],
    pub pixels_per_point: f64,
    pub gpui_platform_draw_submitted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeRectangleTransformEvidence {
    pub observation: NativeRectangleTransformObservation,
    pub expected_final_rect: [f64; 4],
    pub maximum_geometry_error_device_px: f64,
    pub geometry_tolerance_device_px: f64,
    pub select_semantics: &'static str,
}

impl NativeRectangleTransformPlan {
    pub fn assess_native_observation(
        &self,
        observation: NativeRectangleTransformObservation,
    ) -> Result<NativeRectangleTransformEvidence, ComparisonScenarioError> {
        if !observation.hit_test_selected {
            return Err(invalid(
                "decision-3 no-fill edge hit did not select the rectangle",
            ));
        }
        if observation.create_history_delta != 1
            || observation.move_history_delta != 1
            || observation.resize_history_delta != 1
        {
            return Err(invalid(
                "native create, move, and resize must commit one history transaction each",
            ));
        }
        if !observation.pixels_per_point.is_finite() || observation.pixels_per_point <= 0.0 {
            return Err(invalid(
                "native transform requires a positive verified pixel scale",
            ));
        }
        if observation
            .observed_final_rect
            .iter()
            .any(|coordinate| !coordinate.is_finite())
        {
            return Err(invalid("native transform geometry must be finite"));
        }
        let maximum_geometry_error_device_px = observation
            .observed_final_rect
            .iter()
            .zip(self.expected_final_rect)
            .map(|(observed, expected)| (observed - expected).abs() * observation.pixels_per_point)
            .fold(0.0_f64, f64::max);
        if maximum_geometry_error_device_px > self.maximum_geometry_error_device_px {
            return Err(invalid(format!(
                "native transform geometry error {maximum_geometry_error_device_px} device px exceeds tolerance {} device px",
                self.maximum_geometry_error_device_px,
            )));
        }
        if !observation.gpui_platform_draw_submitted {
            return Err(invalid(
                "native transform has no matching GPUI platform draw submission",
            ));
        }
        Ok(NativeRectangleTransformEvidence {
            observation,
            expected_final_rect: self.expected_final_rect,
            maximum_geometry_error_device_px,
            geometry_tolerance_device_px: self.maximum_geometry_error_device_px,
            select_semantics: "no-fill-edge-or-stroked-body",
        })
    }
}

#[derive(Clone, Debug)]
pub struct RectangleInteractionScenario {
    create_plan: ComparisonScenarioPlan,
    transform: Value,
    properties: Value,
}

impl RectangleInteractionScenario {
    pub fn embedded() -> Result<Self, ComparisonScenarioError> {
        Self::from_json(WORKLOAD_JSON)
    }

    pub fn from_json(workload_json: &str) -> Result<Self, ComparisonScenarioError> {
        let workload: Value = serde_json::from_str(workload_json)
            .map_err(|error| ComparisonScenarioError::Invalid(error.to_string()))?;
        Ok(Self {
            create_plan: ComparisonScenarioPlan::from_json(
                ComparisonScenarioKind::AnnotationCreate,
                workload_json,
            )?,
            transform: command(&workload, "rectangle:select-move-resize")?.clone(),
            properties: command(&workload, "rectangle:properties-history")?.clone(),
        })
    }

    pub fn native_transform_plan(
        &self,
    ) -> Result<NativeRectangleTransformPlan, ComparisonScenarioError> {
        let create = self
            .create_plan
            .annotation_create()
            .ok_or_else(|| invalid("rectangle create plan is unavailable"))?
            .rectangle
            .clone();
        let select = point_field(&self.transform, "select_point")?;
        let move_command = field(&self.transform, "move")?;
        let move_delta = point_field(move_command, "delta")?;
        let resize_command = field(&self.transform, "resize")?;
        let resize_delta = point_field(resize_command, "delta")?;
        let resize_handle = string_field(resize_command, "handle")?.to_owned();
        if resize_handle != "east" {
            return Err(invalid("handle must be east"));
        }
        let workload: Value = serde_json::from_str(WORKLOAD_JSON)
            .map_err(|error| ComparisonScenarioError::Invalid(error.to_string()))?;
        let maximum_geometry_error_device_px = number_field(
            field(field(&workload, "expected")?, "milestone_rules")?,
            "maximum_standard_scale_control_point_error_device_px",
        )?;
        let move_samples = delta_samples(select, move_delta, move_command)?;
        let moved_start = [
            create.start[0] + move_delta.x,
            create.start[1] + move_delta.y,
        ];
        let moved_size = [
            (create.finish[0] - create.start[0]).abs(),
            (create.finish[1] - create.start[1]).abs(),
        ];
        let east = PdfPoint::new(
            moved_start[0] + moved_size[0],
            moved_start[1] + moved_size[1] / 2.0,
        )?;
        let resize_samples = delta_samples(east, resize_delta, resize_command)?;
        Ok(NativeRectangleTransformPlan {
            command_id: string_field(&self.transform, "id")?.to_owned(),
            annotation_id: create.annotation_id,
            create_start: create.start,
            create_finish: create.finish,
            create_sample_count: create.sample_count,
            create_rate_hz: create.rate_hz,
            create_duration_ms: create.duration_ms,
            select_point: [select.x, select.y],
            move_delta: [move_delta.x, move_delta.y],
            move_sample_count: move_samples.len(),
            move_rate_hz: u32::try_from(u64_field(move_command, "rate_hz")?)
                .map_err(|_| invalid("move rate exceeds u32"))?,
            move_duration_ms: u64_field(move_command, "duration_ms")?,
            resize_handle,
            resize_delta: [resize_delta.x, resize_delta.y],
            resize_sample_count: resize_samples.len(),
            resize_rate_hz: u32::try_from(u64_field(resize_command, "rate_hz")?)
                .map_err(|_| invalid("resize rate exceeds u32"))?,
            resize_duration_ms: u64_field(resize_command, "duration_ms")?,
            expected_final_rect: [
                moved_start[0],
                moved_start[1],
                moved_size[0] + resize_delta.x,
                moved_size[1],
            ],
            maximum_geometry_error_device_px,
            expected_milestones: array_field(&self.transform, "expected_milestones")?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| invalid("expected milestone must be a string"))
                })
                .collect::<Result<_, _>>()?,
        })
    }

    pub fn execute(
        &self,
        document_id: u64,
        adapter: &mut AnnotationAdapter,
    ) -> Result<RectangleInteractionReport, ComparisonScenarioError> {
        let create = self
            .create_plan
            .annotation_create()
            .ok_or_else(|| invalid("rectangle create plan is unavailable"))?
            .rectangle
            .clone();
        adapter.set_tool(AnnotationTool::Rectangle)?;
        adapter.queue_next_annotation_id(MarkupId::new(&create.annotation_id)?);
        replay(adapter, document_id, 1, &create.samples())?;

        let select = point_field(&self.transform, "select_point")?;
        let move_delta = point_field(field(&self.transform, "move")?, "delta")?;
        let resize = field(&self.transform, "resize")?;
        require_string(resize, "handle", "east")?;
        let resize_delta = point_field(resize, "delta")?;
        let move_samples = delta_samples(select, move_delta, field(&self.transform, "move")?)?;

        adapter.set_tool(AnnotationTool::Select)?;
        let history_before = adapter.history_depths(document_id).0;
        replay(adapter, document_id, 2, &move_samples)?;
        let moved = rectangle(adapter, document_id, &create.annotation_id)?;
        let east = PdfPoint::new(
            moved.rect.x + moved.rect.width,
            moved.rect.y + moved.rect.height / 2.0,
        )?;
        let resize_samples = delta_samples(east, resize_delta, resize)?;
        replay(adapter, document_id, 3, &resize_samples)?;
        let transformed = rectangle(adapter, document_id, &create.annotation_id)?;
        if !transformed.selected || adapter.history_depths(document_id).0 != history_before + 2 {
            return Err(invalid(
                "rectangle move/resize did not commit exactly once each",
            ));
        }
        let expected = [
            create.start[0] + move_delta.x,
            create.start[1] + move_delta.y,
            (create.finish[0] - create.start[0]).abs() + resize_delta.x,
            (create.finish[1] - create.start[1]).abs(),
        ];
        let observed = [
            transformed.rect.x,
            transformed.rect.y,
            transformed.rect.width,
            transformed.rect.height,
        ];
        if observed != expected {
            return Err(invalid(
                "rectangle transform geometry differs from the manifest",
            ));
        }

        require_sequence(
            &self.properties,
            &[
                "set-properties",
                "lock",
                "verify-transform-rejected",
                "unlock",
                "undo-each-commit",
                "redo-each-commit",
            ],
        )?;
        let property = field(&self.properties, "properties")?;
        let (stroke, _) = rgba_color(string_field(property, "stroke")?)?;
        let (fill, fill_alpha) = rgba_color(string_field(property, "fill")?)?;
        require_string(property, "dash", "dashed")?;
        let appearance = RectangleAppearance::new(
            stroke,
            number_field(property, "width_pt")?,
            Some(fill),
            number_field(property, "opacity")?,
        )?
        .with_fill_opacity(fill_alpha)?
        .with_stroke_style(StrokeStyle::Dashed);
        adapter.set_selected_rectangle_appearance(document_id, appearance.clone())?;
        adapter.set_selected_locked(document_id, true)?;
        let locked_select = PdfPoint::new(
            transformed.rect.x + transformed.rect.width * 0.25,
            transformed.rect.y + transformed.rect.height,
        )?;
        let locked_edit_rejected = adapter
            .pointer_down(document_id, 0, 4, locked_select, 4.0)
            .is_err();
        if !locked_edit_rejected {
            return Err(invalid("locked rectangle accepted a transform"));
        }
        adapter.set_selected_locked(document_id, false)?;
        let exact_before = adapter.document_scene(document_id, 0);
        let depth = adapter.history_depths(document_id).0;
        for _ in 0..depth {
            adapter.undo(document_id)?;
        }
        for _ in 0..depth {
            adapter.redo(document_id)?;
        }
        let exact_after = adapter.document_scene(document_id, 0);
        let current = rectangle(adapter, document_id, &create.annotation_id)?;
        if !rectangle_scene_content_equal(&exact_before, &exact_after)
            || current.appearance != appearance
            || current.locked
            || !adapter.is_dirty(document_id)
        {
            return Err(invalid(
                "rectangle properties/history did not round-trip exactly",
            ));
        }

        Ok(RectangleInteractionReport {
            command_evidence: vec![evidence(&self.transform)?, evidence(&self.properties)?],
            final_rect: observed,
            history_depths: adapter.history_depths(document_id),
            dirty: adapter.is_dirty(document_id),
        })
    }

    pub fn execute_transform(
        &self,
        document_id: u64,
        adapter: &mut AnnotationAdapter,
    ) -> Result<RectangleInteractionReport, ComparisonScenarioError> {
        let create = self
            .create_plan
            .annotation_create()
            .ok_or_else(|| invalid("rectangle create plan is unavailable"))?
            .rectangle
            .clone();
        adapter.set_tool(AnnotationTool::Rectangle)?;
        adapter.queue_next_annotation_id(MarkupId::new(&create.annotation_id)?);
        replay(adapter, document_id, 1, &create.samples())?;

        let select = point_field(&self.transform, "select_point")?;
        let move_delta = point_field(field(&self.transform, "move")?, "delta")?;
        let resize = field(&self.transform, "resize")?;
        require_string(resize, "handle", "east")?;
        let resize_delta = point_field(resize, "delta")?;
        let move_samples = delta_samples(select, move_delta, field(&self.transform, "move")?)?;
        adapter.set_tool(AnnotationTool::Select)?;
        let history_before = adapter.history_depths(document_id).0;
        replay(adapter, document_id, 2, &move_samples)?;
        let moved = rectangle(adapter, document_id, &create.annotation_id)?;
        let east = PdfPoint::new(
            moved.rect.x + moved.rect.width,
            moved.rect.y + moved.rect.height / 2.0,
        )?;
        replay(
            adapter,
            document_id,
            3,
            &delta_samples(east, resize_delta, resize)?,
        )?;
        let transformed = rectangle(adapter, document_id, &create.annotation_id)?;
        let observed = [
            transformed.rect.x,
            transformed.rect.y,
            transformed.rect.width,
            transformed.rect.height,
        ];
        let expected = [
            create.start[0] + move_delta.x,
            create.start[1] + move_delta.y,
            (create.finish[0] - create.start[0]).abs() + resize_delta.x,
            (create.finish[1] - create.start[1]).abs(),
        ];
        if !transformed.selected
            || observed != expected
            || adapter.history_depths(document_id).0 != history_before + 2
        {
            return Err(invalid(
                "rectangle transform evidence differs from the manifest",
            ));
        }
        Ok(RectangleInteractionReport {
            command_evidence: vec![evidence(&self.transform)?],
            final_rect: observed,
            history_depths: adapter.history_depths(document_id),
            dirty: adapter.is_dirty(document_id),
        })
    }

    pub fn execute_properties_history(
        &self,
        document_id: u64,
        adapter: &mut AnnotationAdapter,
    ) -> Result<RectangleInteractionReport, ComparisonScenarioError> {
        let mut report = self.execute(document_id, adapter)?;
        report
            .command_evidence
            .retain(|evidence| evidence.command_id == "rectangle:properties-history");
        if report.command_evidence.len() != 1 {
            return Err(invalid(
                "rectangle properties/history evidence is missing or duplicated",
            ));
        }
        Ok(report)
    }
}

fn rectangle_scene_content_equal(
    left: &crate::annotation_model::AnnotationScene,
    right: &crate::annotation_model::AnnotationScene,
) -> bool {
    left.rectangles.len() == right.rectangles.len()
        && left
            .rectangles
            .iter()
            .zip(&right.rectangles)
            .all(|(left, right)| {
                left.id == right.id
                    && left.rect == right.rect
                    && left.appearance == right.appearance
                    && left.locked == right.locked
                    && !left.preview
                    && !right.preview
            })
}

fn evidence(command: &Value) -> Result<RectangleCommandEvidence, ComparisonScenarioError> {
    Ok(RectangleCommandEvidence {
        command_id: string_field(command, "id")?.to_string(),
        proven_milestones: array_field(command, "expected_milestones")?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| invalid("expected milestone must be a string"))
            })
            .collect::<Result<_, _>>()?,
    })
}

fn replay(
    adapter: &mut AnnotationAdapter,
    document_id: u64,
    pointer_id: u64,
    samples: &[[f64; 2]],
) -> Result<(), ComparisonScenarioError> {
    let first = samples
        .first()
        .ok_or_else(|| invalid("pointer stream is empty"))?;
    let last = samples
        .last()
        .ok_or_else(|| invalid("pointer stream is empty"))?;
    adapter.pointer_down(
        document_id,
        0,
        pointer_id,
        PdfPoint::new(first[0], first[1])?,
        4.0,
    )?;
    for sample in &samples[1..samples.len() - 1] {
        adapter.pointer_move(pointer_id, PdfPoint::new(sample[0], sample[1])?)?;
    }
    adapter.pointer_up(pointer_id, PdfPoint::new(last[0], last[1])?)?;
    Ok(())
}

fn delta_samples(
    start: PdfPoint,
    delta: PdfPoint,
    command: &Value,
) -> Result<Vec<[f64; 2]>, ComparisonScenarioError> {
    let rate = u64_field(command, "rate_hz")?;
    let duration = u64_field(command, "duration_ms")?;
    let count = duration
        .checked_mul(rate)
        .and_then(|value| value.checked_div(1_000))
        .and_then(|value| value.checked_add(1))
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| invalid("pointer sample count overflows"))?;
    Ok((0..count)
        .map(|index| {
            let t = index as f64 / (count - 1) as f64;
            [start.x + delta.x * t, start.y + delta.y * t]
        })
        .collect())
}

fn rectangle(
    adapter: &AnnotationAdapter,
    document_id: u64,
    id: &str,
) -> Result<crate::annotation_model::SceneRectangle, ComparisonScenarioError> {
    adapter
        .document_scene(document_id, 0)
        .rectangles
        .into_iter()
        .find(|rectangle| rectangle.id.as_str() == id)
        .ok_or_else(|| invalid("manifest rectangle is missing"))
}

fn command<'a>(workload: &'a Value, id: &str) -> Result<&'a Value, ComparisonScenarioError> {
    array_field(workload, "journeys")?
        .iter()
        .flat_map(|journey| journey["commands"].as_array().into_iter().flatten())
        .find(|command| command.get("id").and_then(Value::as_str) == Some(id))
        .ok_or_else(|| invalid(format!("missing command {id}")))
}

fn require_sequence(command: &Value, expected: &[&str]) -> Result<(), ComparisonScenarioError> {
    let observed = array_field(command, "sequence")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| invalid("sequence value must be a string"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if observed != expected {
        return Err(invalid("properties/history sequence drifted"));
    }
    Ok(())
}

fn rgba_color(value: &str) -> Result<(String, f64), ComparisonScenarioError> {
    if value.len() != 9 || !value.starts_with('#') {
        return Err(invalid("manifest color must use #rrggbbaa"));
    }
    let alpha = u8::from_str_radix(&value[7..9], 16)
        .map_err(|_| invalid("manifest color has invalid alpha"))?;
    Ok((value[..7].to_string(), f64::from(alpha) / 255.0))
}

fn point_field(value: &Value, key: &str) -> Result<PdfPoint, ComparisonScenarioError> {
    let point = field(value, key)?;
    Ok(PdfPoint::new(
        number_field(point, "x")?,
        number_field(point, "y")?,
    )?)
}

fn require_string(value: &Value, key: &str, expected: &str) -> Result<(), ComparisonScenarioError> {
    if string_field(value, key)? != expected {
        return Err(invalid(format!("{key} must be {expected}")));
    }
    Ok(())
}

fn string_field<'a>(value: &'a Value, key: &str) -> Result<&'a str, ComparisonScenarioError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("missing string {key}")))
}

fn number_field(value: &Value, key: &str) -> Result<f64, ComparisonScenarioError> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| invalid(format!("missing number {key}")))
}

fn u64_field(value: &Value, key: &str) -> Result<u64, ComparisonScenarioError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid(format!("missing integer {key}")))
}

fn array_field<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, ComparisonScenarioError> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| invalid(format!("missing array {key}")))
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value, ComparisonScenarioError> {
    value
        .get(key)
        .ok_or_else(|| invalid(format!("missing field {key}")))
}

fn invalid(message: impl Into<String>) -> ComparisonScenarioError {
    ComparisonScenarioError::Invalid(message.into())
}

impl From<crate::annotation_model::AnnotationError> for ComparisonScenarioError {
    fn from(error: crate::annotation_model::AnnotationError) -> Self {
        Self::Invalid(error.to_string())
    }
}
