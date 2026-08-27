//! Exact editor workload replay behind one small, testable seam.
//!
//! This module is a semantic correctness and resource diagnostic. It owns the
//! deterministic domain commands, but it does not prove native window input,
//! GPUI presentation, bitmap upload, or decision-timing eligibility.

use std::{collections::BTreeMap, fmt};

use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    annotation_adapter::{AnnotationAdapter, AnnotationTool},
    annotation_model::{
        AnnotationError, DecodedRgbaAsset, MarkupId, PdfPoint, PdfRect, StrokeStyle,
    },
    comparison_scenario::{
        ComparisonScenarioKind, ComparisonScenarioPlan, compare_highlight_geometry,
    },
};

const WORKLOAD_JSON: &str = include_str!("../../performance/comparison-workload.json");
const EXPECTED_STATE_SHA256: &str =
    "e9b746ca9ba717129c10c6e485e5f4ecf021e7276ed99d0f9799d567e570f887";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditorComparisonEvent {
    PointerStreamReplayed {
        command_id: &'static str,
        submitted_samples: usize,
    },
    TextInputApplied {
        command_id: &'static str,
        text: String,
    },
    DecodedAssetAccepted {
        command_id: &'static str,
        width_px: u32,
        height_px: u32,
    },
    UploadPayloadPrepared {
        command_id: &'static str,
        rgba_bytes: usize,
    },
    SpatialIndexQueried {
        command_id: &'static str,
        candidate_count: usize,
        total_rectangle_count: usize,
    },
}

pub trait EditorComparisonObserver {
    fn observe(&mut self, event: EditorComparisonEvent);
}

#[derive(Default)]
pub struct RecordingEditorObserver {
    pub events: Vec<EditorComparisonEvent>,
}

impl EditorComparisonObserver for RecordingEditorObserver {
    fn observe(&mut self, event: EditorComparisonEvent) {
        self.events.push(event);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorComparisonReport {
    pub completed_command_ids: Vec<&'static str>,
    pub command_evidence: Vec<EditorCommandEvidence>,
    pub blocked_commands: Vec<&'static str>,
    pub history_depths: (usize, usize),
    pub dirty: bool,
}

impl EditorComparisonReport {
    pub fn evidence_for(&self, command_id: &str) -> Option<&EditorCommandEvidence> {
        self.command_evidence
            .iter()
            .find(|evidence| evidence.command_id == command_id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BlockedEditorMilestone {
    pub milestone: String,
    pub reason: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EditorCommandEvidence {
    pub command_id: String,
    /// Manifest milestones directly proved by deterministic domain execution.
    pub proven_manifest_milestones: Vec<String>,
    /// Manifest milestones that require evidence outside this semantic replay.
    pub blocked_manifest_milestones: Vec<BlockedEditorMilestone>,
    /// Exact observations that support the proved milestones.
    pub facts: BTreeMap<String, Value>,
}

#[derive(Clone, Debug)]
pub struct EditorComparisonScenario {
    annotation_create: ComparisonScenarioPlan,
    expected_milestones: BTreeMap<String, Vec<String>>,
    image_create: ImageCreatePlan,
    image_resize_bounds: PdfRect,
}

#[derive(Clone, Debug)]
struct ImageCreatePlan {
    point: PdfPoint,
    page_width: f64,
    page_height: f64,
    max_page_fraction: f64,
}

impl EditorComparisonScenario {
    pub fn command_ids(&self) -> Vec<String> {
        manifest_command_ids(
            WORKLOAD_JSON,
            &[
                "rectangle-v1",
                "highlight-v1",
                "text-v1",
                "length-v1",
                "image-v1",
            ],
        )
    }
    pub fn embedded() -> Result<Self, EditorComparisonError> {
        Self::from_json(WORKLOAD_JSON)
    }

    pub fn from_json(workload_json: &str) -> Result<Self, EditorComparisonError> {
        let workload: Value = serde_json::from_str(workload_json)
            .map_err(|error| EditorComparisonError::Invalid(error.to_string()))?;
        let expected = workload
            .get("expected")
            .ok_or_else(|| EditorComparisonError::Invalid("missing expected state".into()))?;
        if canonical_sha256(expected) != EXPECTED_STATE_SHA256 {
            return Err(EditorComparisonError::Invalid(
                "expected state hash does not match the frozen comparison oracle".into(),
            ));
        }
        Ok(Self {
            annotation_create: ComparisonScenarioPlan::from_json(
                ComparisonScenarioKind::AnnotationCreate,
                workload_json,
            )
            .map_err(|error| EditorComparisonError::Invalid(error.to_string()))?,
            expected_milestones: manifest_expected_milestones(&workload)?,
            image_create: image_create_plan(&workload)?,
            image_resize_bounds: image_resize_bounds(&workload)?,
        })
    }

    fn evidence(
        &self,
        command_id: &'static str,
        proven: &[&str],
        facts: BTreeMap<String, Value>,
    ) -> Result<EditorCommandEvidence, EditorComparisonError> {
        let expected = self.expected_milestones.get(command_id).ok_or_else(|| {
            EditorComparisonError::Invalid(format!(
                "editor command {command_id} is missing from the manifest"
            ))
        })?;
        for milestone in proven {
            if !expected.iter().any(|expected| expected == milestone) {
                return Err(EditorComparisonError::Invalid(format!(
                    "editor command {command_id} tried to prove undeclared milestone {milestone}"
                )));
            }
        }
        let proven_manifest_milestones = expected
            .iter()
            .filter(|milestone| proven.contains(&milestone.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let blocked_manifest_milestones = expected
            .iter()
            .filter(|milestone| !proven.contains(&milestone.as_str()))
            .map(|milestone| BlockedEditorMilestone {
                milestone: milestone.clone(),
                reason: blocked_milestone_reason(milestone),
            })
            .collect();
        Ok(EditorCommandEvidence {
            command_id: command_id.into(),
            proven_manifest_milestones,
            blocked_manifest_milestones,
            facts,
        })
    }

    pub fn execute(
        &self,
        document_id: u64,
        checker: DecodedRgbaAsset,
        adapter: &mut AnnotationAdapter,
        observer: &mut impl EditorComparisonObserver,
    ) -> Result<EditorComparisonReport, EditorComparisonError> {
        let plan = self
            .annotation_create
            .annotation_create()
            .ok_or_else(|| EditorComparisonError::Invalid("annotation plan is missing".into()))?;
        let mut completed = Vec::new();
        let mut command_evidence = Vec::new();

        let rectangle_samples = plan.rectangle.samples();
        let history_before = adapter.history_depths(document_id).0;
        adapter.set_tool(AnnotationTool::Rectangle)?;
        adapter.queue_next_annotation_id(MarkupId::new(&plan.rectangle.annotation_id)?);
        replay_pointer_stream(adapter, document_id, 1, &rectangle_samples)?;
        observer.observe(EditorComparisonEvent::PointerStreamReplayed {
            command_id: "rectangle:create-sparse",
            submitted_samples: rectangle_samples.len(),
        });
        let rectangle_scene = adapter.document_scene(document_id, 0);
        let sparse_rectangle = rectangle_scene
            .rectangles
            .iter()
            .find(|rectangle| rectangle.id.as_str() == plan.rectangle.annotation_id)
            .ok_or_else(|| {
                EditorComparisonError::Invalid("sparse rectangle was not committed".into())
            })?;
        let history_after = adapter.history_depths(document_id).0;
        if sparse_rectangle.preview || history_after != history_before + 1 {
            return Err(EditorComparisonError::Invalid(
                "sparse rectangle did not commit exactly once".into(),
            ));
        }
        command_evidence.push(self.evidence(
            "rectangle:create-sparse",
            &["pointer-stream-received", "gesture-committed-once"],
            facts([
                ("annotation_id", json!(sparse_rectangle.id.as_str())),
                ("history_delta", json!(history_after - history_before)),
                ("submitted_samples", json!(rectangle_samples.len())),
            ]),
        )?);
        completed.push("rectangle:create-sparse");

        let transform_history_before = adapter.history_depths(document_id).0;
        adapter.set_tool(AnnotationTool::Select)?;
        adapter.pointer_down(document_id, 0, 2, point(162.0, 192.0)?, 4.0)?;
        adapter.pointer_move(2, point(180.0, 180.0)?)?;
        adapter.pointer_up(2, point(180.0, 180.0)?)?;
        adapter.pointer_down(document_id, 0, 3, point(270.0, 180.0)?, 4.0)?;
        adapter.pointer_move(3, point(300.0, 180.0)?)?;
        adapter.pointer_up(3, point(300.0, 180.0)?)?;
        let transformed = adapter
            .document_scene(document_id, 0)
            .rectangles
            .into_iter()
            .find(|rectangle| rectangle.id.as_str() == plan.rectangle.annotation_id)
            .ok_or_else(|| {
                EditorComparisonError::Invalid("transformed rectangle missing".into())
            })?;
        let transform_history_after = adapter.history_depths(document_id).0;
        let expected_transformed = crate::annotation_model::PdfRect::new(90.0, 132.0, 210.0, 96.0)?;
        if !transformed.selected
            || transformed.rect != expected_transformed
            || transform_history_after != transform_history_before + 2
        {
            return Err(EditorComparisonError::Invalid(
                "rectangle transform evidence differs from the manifest".into(),
            ));
        }
        command_evidence.push(self.evidence(
            "rectangle:select-move-resize",
            &[
                "hit-test-selected",
                "move-committed-once",
                "resize-committed-once",
            ],
            facts([
                ("annotation_id", json!(transformed.id.as_str())),
                ("final_bounds", rect_json(transformed.rect)),
                (
                    "history_delta",
                    json!(transform_history_after - transform_history_before),
                ),
            ]),
        )?);
        completed.push("rectangle:select-move-resize");

        let properties_history_before = adapter.history_depths(document_id).0;
        let rectangle_appearance = crate::annotation_model::RectangleAppearance::new(
            "#dc2626",
            3.0,
            Some("#dc2626"),
            0.88,
        )?
        .with_fill_opacity(31.0 / 255.0)?
        .with_stroke_style(StrokeStyle::Dashed);
        adapter.set_selected_rectangle_appearance(document_id, rectangle_appearance.clone())?;
        adapter.set_selected_locked(document_id, true)?;
        let locked = adapter
            .pointer_down(document_id, 0, 4, point(180.0, 180.0)?, 4.0)
            .is_err();
        if !locked {
            return Err(EditorComparisonError::Invalid(
                "locked rectangle accepted a transform".into(),
            ));
        }
        adapter.set_selected_locked(document_id, false)?;
        let depth = adapter.history_depths(document_id).0;
        let exact_before_history_replay = adapter.document_scene(document_id, 0);
        for _ in 0..depth {
            adapter.undo(document_id)?;
        }
        for _ in 0..depth {
            adapter.redo(document_id)?;
        }
        let exact_after_history_replay = adapter.document_scene(document_id, 0);
        let current_rectangle = exact_after_history_replay
            .rectangles
            .iter()
            .find(|rectangle| rectangle.id.as_str() == plan.rectangle.annotation_id)
            .ok_or_else(|| EditorComparisonError::Invalid("styled rectangle missing".into()))?;
        if !scene_content_equal(&exact_before_history_replay, &exact_after_history_replay)
            || current_rectangle.appearance != rectangle_appearance
            || current_rectangle.locked
            || !adapter.is_dirty(document_id)
        {
            return Err(EditorComparisonError::Invalid(
                "rectangle properties/history did not round-trip exactly".into(),
            ));
        }
        command_evidence.push(self.evidence(
            "rectangle:properties-history",
            &[
                "properties-current",
                "locked-edit-rejected",
                "undo-redo-exact",
                "dirty-current",
            ],
            facts([
                ("full_history_replay_count", json!(depth)),
                (
                    "history_delta_before_replay",
                    json!(depth - properties_history_before),
                ),
                ("locked_transform_rejected", json!(locked)),
                ("redo_restored_exact_scene", json!(true)),
            ]),
        )?);
        completed.push("rectangle:properties-history");

        let dense_document_id = document_id.saturating_add(1);
        seed_dense_page(adapter, dense_document_id)?;
        adapter.set_tool(AnnotationTool::Rectangle)?;
        adapter.queue_next_annotation_id(MarkupId::new("comparison:rectangle:dense:1")?);
        replay_pointer_stream(adapter, dense_document_id, 10_000, &rectangle_samples)?;
        adapter.set_tool(AnnotationTool::Select)?;
        adapter.pointer_down(dense_document_id, 0, 10_001, point(162.0, 192.0)?, 4.0)?;
        adapter.pointer_move(10_001, point(180.0, 180.0)?)?;
        adapter.pointer_up(10_001, point(180.0, 180.0)?)?;
        adapter.pointer_down(dense_document_id, 0, 10_002, point(270.0, 180.0)?, 4.0)?;
        adapter.pointer_move(10_002, point(300.0, 180.0)?)?;
        adapter.pointer_up(10_002, point(300.0, 180.0)?)?;
        adapter
            .set_selected_rectangle_appearance(dense_document_id, rectangle_appearance.clone())?;
        let work = adapter.spatial_query_work(dense_document_id, 0, point(162.0, 192.0)?, 4.0)?;
        let dense_scene = adapter.document_scene(dense_document_id, 0);
        let dense_rectangle = dense_scene
            .rectangles
            .iter()
            .find(|rectangle| rectangle.id.as_str() == "comparison:rectangle:dense:1")
            .ok_or_else(|| EditorComparisonError::Invalid("dense rectangle missing".into()))?;
        if work.candidate_count >= work.total_rectangle_count
            || dense_scene.rectangles.len() != 101
            || dense_rectangle.rect != expected_transformed
            || dense_rectangle.appearance != rectangle_appearance
        {
            return Err(EditorComparisonError::Invalid(
                "dense-page spatial query was not bounded".into(),
            ));
        }
        observer.observe(EditorComparisonEvent::SpatialIndexQueried {
            command_id: "rectangle:repeat-dense",
            candidate_count: work.candidate_count,
            total_rectangle_count: work.total_rectangle_count,
        });
        command_evidence.push(self.evidence(
            "rectangle:repeat-dense",
            &["spatial-index-work-recorded", "canonical-state-matched"],
            facts([
                ("candidate_count", json!(work.candidate_count)),
                ("canonical_bounds", rect_json(dense_rectangle.rect)),
                ("total_rectangle_count", json!(work.total_rectangle_count)),
                (
                    "visible_rectangle_count",
                    json!(dense_scene.rectangles.len()),
                ),
            ]),
        )?);
        completed.push("rectangle:repeat-dense");

        let highlight_samples = plan.highlight.samples();
        let highlight_history_before = adapter.history_depths(document_id).0;
        adapter.set_tool(AnnotationTool::Highlight)?;
        adapter.queue_next_annotation_id(MarkupId::new(&plan.highlight.annotation_id)?);
        replay_pointer_stream(adapter, document_id, 5, &highlight_samples)?;
        observer.observe(EditorComparisonEvent::PointerStreamReplayed {
            command_id: "highlight:create",
            submitted_samples: highlight_samples.len(),
        });
        let highlight_scene = adapter.document_scene(document_id, 0);
        let created_highlight = highlight_scene
            .pens
            .iter()
            .find(|pen| pen.id.as_str() == plan.highlight.annotation_id)
            .ok_or_else(|| EditorComparisonError::Invalid("highlight was not committed".into()))?;
        let highlight_history_after = adapter.history_depths(document_id).0;
        let polyline_segment_count = created_highlight.points.len().saturating_sub(1);
        let observed_highlight_points = created_highlight
            .points
            .iter()
            .map(|point| [point.x, point.y])
            .collect::<Vec<_>>();
        let geometry =
            compare_highlight_geometry(&highlight_samples, &observed_highlight_points, 1.0)
                .map_err(|error| EditorComparisonError::Invalid(error.to_string()))?;
        if created_highlight.draft
            || created_highlight.smooth_curves
            || polyline_segment_count != created_highlight.points.len() - 1
            || !geometry.matched
            || highlight_history_after != highlight_history_before + 1
        {
            return Err(EditorComparisonError::Invalid(format!(
                "highlight create evidence differs from the manifest: draft={}, submitted={}, accepted={}, history_before={}, history_after={}",
                created_highlight.draft,
                highlight_samples.len(),
                created_highlight.points.len(),
                highlight_history_before,
                highlight_history_after
            )));
        }
        command_evidence.push(self.evidence(
            "highlight:create",
            &[
                "pointer-stream-received",
                "path-smoothed",
                "gesture-committed-once",
            ],
            facts([
                (
                    "canonical_geometry_resample_count",
                    json!(geometry.canonical_resample_count),
                ),
                ("polyline_segments", json!(polyline_segment_count)),
                ("renderer_contract", json!("straight-polyline")),
                (
                    "maximum_centerline_deviation_pdf_points",
                    json!(geometry.maximum_centerline_deviation_pdf_points),
                ),
                ("stored_input_points", json!(created_highlight.points.len())),
                (
                    "history_delta",
                    json!(highlight_history_after - highlight_history_before),
                ),
                ("submitted_samples", json!(highlight_samples.len())),
            ]),
        )?);
        completed.push("highlight:create");
        let highlight_id = MarkupId::new("comparison:highlight:1")?;
        adapter.set_tool(AnnotationTool::Select)?;
        adapter.pointer_down(document_id, 0, 5_001, point(90.0, 330.0)?, 4.0)?;
        if adapter.selected_kind(document_id) != Some(crate::annotation_model::AnnotationKind::Pen)
        {
            return Err(EditorComparisonError::Invalid(
                "highlight hit target missing".into(),
            ));
        }
        let highlight_edit_history_before = adapter.history_depths(document_id).0;
        adapter.pointer_move(5_001, point(102.0, 324.0)?)?;
        adapter.pointer_up(5_001, point(102.0, 324.0)?)?;
        adapter.set_selected_ink_opacity(document_id, 0.45)?;
        let edited_highlight_scene = adapter.document_scene(document_id, 0);
        adapter.undo(document_id)?;
        adapter.redo(document_id)?;
        let redone_highlight_scene = adapter.document_scene(document_id, 0);
        let redone_highlight = redone_highlight_scene
            .pens
            .iter()
            .find(|pen| pen.id == highlight_id)
            .ok_or_else(|| EditorComparisonError::Invalid("edited highlight missing".into()))?;
        let highlight_thumbnail = adapter.thumbnail_scene(document_id, 0);
        if !scene_content_equal(&edited_highlight_scene, &redone_highlight_scene)
            || redone_highlight.points.first() != Some(&point(102.0, 324.0)?)
            || redone_highlight.appearance.opacity() != 0.45
            || adapter.history_depths(document_id).0 != highlight_edit_history_before + 2
        {
            return Err(EditorComparisonError::Invalid(
                "highlight edit/history did not round-trip exactly".into(),
            ));
        }
        command_evidence.push(self.evidence(
            "highlight:edit-history",
            &[
                "hit-test-selected",
                "path-bounds-current",
                "undo-redo-exact",
            ],
            facts([
                ("first_point", point_json(redone_highlight.points[0])),
                ("history_delta", json!(2)),
                ("opacity", json!(redone_highlight.appearance.opacity())),
                ("redo_restored_exact_scene", json!(true)),
                (
                    "thumbnail_projection_revision",
                    json!(highlight_thumbnail.revision),
                ),
            ]),
        )?);
        completed.push("highlight:edit-history");

        let text_history_before = adapter.history_depths(document_id).0;
        adapter.set_tool(AnnotationTool::TextBox)?;
        adapter.queue_next_annotation_id(MarkupId::new("comparison:text:1")?);
        adapter.pointer_down(document_id, 0, 6, point(90.0, 390.0)?, 4.0)?;
        observer.observe(EditorComparisonEvent::TextInputApplied {
            command_id: "text:create",
            text: "Beam B-12 / revision 3".into(),
        });
        let text_scene = adapter.document_scene(document_id, 0);
        let created_text = text_scene
            .text_boxes
            .iter()
            .find(|text| text.id.as_str() == "comparison:text:1")
            .ok_or_else(|| EditorComparisonError::Invalid("created text missing".into()))?;
        if created_text.content != "Beam B-12 / revision 3"
            || adapter.history_depths(document_id).0 != text_history_before + 1
        {
            return Err(EditorComparisonError::Invalid(
                "text create did not commit the manifest input exactly once".into(),
            ));
        }
        command_evidence.push(self.evidence(
            "text:create",
            &["text-input-committed", "gesture-committed-once"],
            facts([
                ("content", json!(created_text.content)),
                ("history_delta", json!(1)),
                ("layout_bounds", rect_json(created_text.layout_rect)),
            ]),
        )?);
        completed.push("text:create");
        let text_edit_history_before = adapter.history_depths(document_id).0;
        let original_text_style = created_text.style.clone();
        adapter.replace_selected_text(document_id, "Beam B-12 / revision 4")?;
        observer.observe(EditorComparisonEvent::TextInputApplied {
            command_id: "text:edit-resize-history",
            text: "Beam B-12 / revision 4".into(),
        });
        adapter.resize_selected_text(document_id, 300.0, 84.0)?;
        let edited_text_scene = adapter.document_scene(document_id, 0);
        adapter.undo(document_id)?;
        adapter.redo(document_id)?;
        let redone_text_scene = adapter.document_scene(document_id, 0);
        let redone_text = redone_text_scene
            .text_boxes
            .iter()
            .find(|text| text.id.as_str() == "comparison:text:1")
            .ok_or_else(|| EditorComparisonError::Invalid("edited text missing".into()))?;
        let expected_text_rect = crate::annotation_model::PdfRect::new(90.0, 390.0, 300.0, 84.0)?;
        if !scene_content_equal(&edited_text_scene, &redone_text_scene)
            || redone_text.content != "Beam B-12 / revision 4"
            || redone_text.layout_rect != expected_text_rect
            || redone_text.style != original_text_style
            || adapter.history_depths(document_id).0 != text_edit_history_before + 2
        {
            return Err(EditorComparisonError::Invalid(
                "text edit/history did not round-trip exactly".into(),
            ));
        }
        command_evidence.push(self.evidence(
            "text:edit-resize-history",
            &[
                "selection-current",
                "layout-current",
                "font-persistence-recorded",
                "undo-redo-exact",
            ],
            facts([
                ("content", json!(redone_text.content)),
                ("history_delta", json!(2)),
                ("layout_bounds", rect_json(redone_text.layout_rect)),
                ("redo_restored_exact_scene", json!(true)),
                ("font_family", json!(redone_text.style.font_family())),
                ("font_size_pt", json!(redone_text.style.font_size_pt())),
            ]),
        )?);
        completed.push("text:edit-resize-history");

        let calibration =
            crate::annotation_model::LengthCalibration::from_scale(72.0, 1.0, "m", 2, true)?;
        adapter.set_document_page_length_calibration(document_id, 0, calibration)?;
        let current_calibration = adapter
            .document_page_length_calibration(document_id, 0)
            .ok_or_else(|| {
                EditorComparisonError::Invalid("measurement scale was not applied".into())
            })?;
        if current_calibration.paper_points() != 72.0
            || current_calibration.real_world_value() != 1.0
            || current_calibration.unit() != "m"
            || current_calibration.precision() != 2
        {
            return Err(EditorComparisonError::Invalid(
                "measurement scale differs from the manifest".into(),
            ));
        }
        command_evidence.push(self.evidence(
            "length:set-scale",
            &["measurement-scale-current"],
            facts([
                ("paper_points", json!(current_calibration.paper_points())),
                ("precision", json!(current_calibration.precision())),
                (
                    "real_world_value",
                    json!(current_calibration.real_world_value()),
                ),
                ("unit", json!(current_calibration.unit())),
            ]),
        )?);
        completed.push("length:set-scale");
        let length_history_before = adapter.history_depths(document_id).0;
        adapter.set_tool(AnnotationTool::Length)?;
        adapter.begin_length_placement(
            document_id,
            0,
            MarkupId::new("comparison:length:1")?,
            point(90.0, 510.0)?,
        )?;
        adapter.commit_length_placement(document_id, 0, point(306.0, 510.0)?, false)?;
        let length_scene = adapter.document_scene(document_id, 0);
        let created_length = length_scene
            .lengths
            .iter()
            .find(|length| length.id.as_str() == "comparison:length:1")
            .ok_or_else(|| EditorComparisonError::Invalid("created length missing".into()))?;
        if created_length.caption != "3.00 m"
            || adapter.history_depths(document_id).0 != length_history_before + 1
        {
            return Err(EditorComparisonError::Invalid(
                "length create did not derive and commit exactly once".into(),
            ));
        }
        command_evidence.push(self.evidence(
            "length:create",
            &["derived-length-exact", "gesture-committed-once"],
            facts([
                ("caption", json!(created_length.caption)),
                ("end", point_json(created_length.end)),
                ("history_delta", json!(1)),
                ("start", point_json(created_length.start)),
            ]),
        )?);
        completed.push("length:create");
        let length_edit_history_before = adapter.history_depths(document_id).0;
        adapter.set_tool(AnnotationTool::Select)?;
        adapter.pointer_down(document_id, 0, 8, point(306.0, 510.0)?, 4.0)?;
        adapter.pointer_up(8, point(342.0, 510.0)?)?;
        let edited_length_scene = adapter.document_scene(document_id, 0);
        adapter.undo(document_id)?;
        adapter.redo(document_id)?;
        let redone_length_scene = adapter.document_scene(document_id, 0);
        let redone_length = redone_length_scene
            .lengths
            .iter()
            .find(|length| length.id.as_str() == "comparison:length:1")
            .ok_or_else(|| EditorComparisonError::Invalid("edited length missing".into()))?;
        if !scene_content_equal(&edited_length_scene, &redone_length_scene)
            || redone_length.end != point(342.0, 510.0)?
            || redone_length.caption != "3.50 m"
            || adapter.history_depths(document_id).0 != length_edit_history_before + 1
        {
            return Err(EditorComparisonError::Invalid(
                "length endpoint edit/history did not round-trip exactly".into(),
            ));
        }
        command_evidence.push(self.evidence(
            "length:edit-endpoint-history",
            &[
                "control-point-current",
                "derived-length-exact",
                "undo-redo-exact",
            ],
            facts([
                ("caption", json!(redone_length.caption)),
                ("end", point_json(redone_length.end)),
                ("history_delta", json!(1)),
                ("redo_restored_exact_scene", json!(true)),
            ]),
        )?);
        completed.push("length:edit-endpoint-history");

        let rgba_bytes = checker.rgba().len();
        let source_width = checker.width_px();
        let source_height = checker.height_px();
        observer.observe(EditorComparisonEvent::DecodedAssetAccepted {
            command_id: "image:create",
            width_px: source_width,
            height_px: source_height,
        });
        let image_history_before = adapter.history_depths(document_id).0;
        adapter.set_image_asset(checker);
        adapter.set_image_placement_page(
            self.image_create.page_width,
            self.image_create.page_height,
            self.image_create.max_page_fraction,
        )?;
        adapter.set_tool(AnnotationTool::Image)?;
        adapter.queue_next_annotation_id(MarkupId::new("comparison:image:1")?);
        adapter.pointer_down(document_id, 0, 9, self.image_create.point, 4.0)?;
        observer.observe(EditorComparisonEvent::UploadPayloadPrepared {
            command_id: "image:create",
            rgba_bytes,
        });
        let image_scene = adapter.document_scene(document_id, 0);
        let created_image = image_scene
            .images
            .iter()
            .find(|image| image.id.as_str() == "comparison:image:1")
            .ok_or_else(|| EditorComparisonError::Invalid("created image missing".into()))?;
        if created_image.width_px != source_width
            || created_image.height_px != source_height
            || created_image.rect
                != natural_image_rect(source_width, source_height, &self.image_create)?
            || adapter.history_depths(document_id).0 != image_history_before + 1
        {
            return Err(EditorComparisonError::Invalid(
                "image create did not commit the decoded asset exactly once".into(),
            ));
        }
        command_evidence.push(self.evidence(
            "image:create",
            &["bitmap-decoded", "gesture-committed-once"],
            facts([
                ("bounds", rect_json(created_image.rect)),
                ("history_delta", json!(1)),
                (
                    "max_page_fraction",
                    json!(self.image_create.max_page_fraction),
                ),
                (
                    "page_size",
                    json!({
                        "width": self.image_create.page_width,
                        "height": self.image_create.page_height,
                    }),
                ),
                ("placement_point", point_json(self.image_create.point)),
                ("rgba_bytes", json!(rgba_bytes)),
                ("source_height_px", json!(source_height)),
                ("source_width_px", json!(source_width)),
            ]),
        )?);
        completed.push("image:create");
        let image_edit_history_before = adapter.history_depths(document_id).0;
        adapter.set_selected_image_rect(document_id, self.image_resize_bounds)?;
        let edited_image_scene = adapter.document_scene(document_id, 0);
        adapter.undo(document_id)?;
        adapter.redo(document_id)?;
        let redone_image_scene = adapter.document_scene(document_id, 0);
        let redone_image = redone_image_scene
            .images
            .iter()
            .find(|image| image.id.as_str() == "comparison:image:1")
            .ok_or_else(|| EditorComparisonError::Invalid("resized image missing".into()))?;
        let aspect_ratio = redone_image.rect.width / redone_image.rect.height;
        if !scene_content_equal(&edited_image_scene, &redone_image_scene)
            || (aspect_ratio - 4.0 / 3.0).abs() > f64::EPSILON
            || adapter.history_depths(document_id).0 != image_edit_history_before + 1
        {
            return Err(EditorComparisonError::Invalid(
                "image resize/history did not round-trip exactly".into(),
            ));
        }
        command_evidence.push(self.evidence(
            "image:resize-history",
            &["aspect-ratio-current", "undo-redo-exact"],
            facts([
                ("aspect_ratio", json!(aspect_ratio)),
                ("bounds", rect_json(redone_image.rect)),
                ("history_delta", json!(1)),
                ("redo_restored_exact_scene", json!(true)),
                ("rgba_bytes_prepared_not_uploaded", json!(rgba_bytes)),
            ]),
        )?);
        completed.push("image:resize-history");

        verify_final_state(adapter, document_id)?;
        Ok(EditorComparisonReport {
            completed_command_ids: completed,
            command_evidence,
            blocked_commands: Vec::new(),
            history_depths: adapter.history_depths(document_id),
            dirty: adapter.is_dirty(document_id),
        })
    }
}

fn manifest_command_ids(json: &str, journey_ids: &[&str]) -> Vec<String> {
    let workload: Value = serde_json::from_str(json).expect("embedded workload is validated");
    workload["journeys"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|journey| journey_ids.contains(&journey["id"].as_str().unwrap()))
        .flat_map(|journey| journey["commands"].as_array().unwrap())
        .map(|command| command["id"].as_str().unwrap().to_string())
        .collect()
}

fn manifest_expected_milestones(
    workload: &Value,
) -> Result<BTreeMap<String, Vec<String>>, EditorComparisonError> {
    let editor_journeys = [
        "rectangle-v1",
        "highlight-v1",
        "text-v1",
        "length-v1",
        "image-v1",
    ];
    let journeys = workload["journeys"]
        .as_array()
        .ok_or_else(|| EditorComparisonError::Invalid("manifest journeys are missing".into()))?;
    let mut milestones = BTreeMap::new();
    for journey in journeys.iter().filter(|journey| {
        journey["id"]
            .as_str()
            .is_some_and(|id| editor_journeys.contains(&id))
    }) {
        let commands = journey["commands"].as_array().ok_or_else(|| {
            EditorComparisonError::Invalid("editor journey commands are missing".into())
        })?;
        for command in commands {
            let command_id = command["id"].as_str().ok_or_else(|| {
                EditorComparisonError::Invalid("editor command id is missing".into())
            })?;
            let expected = command["expected_milestones"]
                .as_array()
                .ok_or_else(|| {
                    EditorComparisonError::Invalid(format!(
                        "expected milestones are missing for {command_id}"
                    ))
                })?
                .iter()
                .map(|milestone| {
                    milestone.as_str().map(str::to_string).ok_or_else(|| {
                        EditorComparisonError::Invalid(format!(
                            "non-string expected milestone for {command_id}"
                        ))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            milestones.insert(command_id.to_string(), expected);
        }
    }
    Ok(milestones)
}

fn manifest_command<'a>(workload: &'a Value, command_id: &str) -> Option<&'a Value> {
    workload["journeys"]
        .as_array()?
        .iter()
        .flat_map(|journey| journey["commands"].as_array().into_iter().flatten())
        .find(|command| command["id"].as_str() == Some(command_id))
}

fn finite_number(value: &Value, field: &str) -> Result<f64, EditorComparisonError> {
    value
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| EditorComparisonError::Invalid(format!("{field} must be a finite number")))
}

fn image_create_plan(workload: &Value) -> Result<ImageCreatePlan, EditorComparisonError> {
    let command = manifest_command(workload, "image:create")
        .ok_or_else(|| EditorComparisonError::Invalid("image:create is missing".into()))?;
    if command["placement"]["sizing"].as_str() != Some("natural-size-page-contained") {
        return Err(EditorComparisonError::Invalid(
            "image:create must use natural-size-page-contained placement".into(),
        ));
    }
    let placement = &command["placement"];
    Ok(ImageCreatePlan {
        point: PdfPoint::new(
            finite_number(&placement["point"]["x"], "image:create point.x")?,
            finite_number(&placement["point"]["y"], "image:create point.y")?,
        )?,
        page_width: finite_number(
            &placement["fixture_page_size_points"]["width"],
            "image:create page width",
        )?,
        page_height: finite_number(
            &placement["fixture_page_size_points"]["height"],
            "image:create page height",
        )?,
        max_page_fraction: finite_number(
            &placement["max_page_fraction"],
            "image:create max page fraction",
        )?,
    })
}

fn image_resize_bounds(workload: &Value) -> Result<PdfRect, EditorComparisonError> {
    let command = manifest_command(workload, "image:resize-history")
        .ok_or_else(|| EditorComparisonError::Invalid("image:resize-history is missing".into()))?;
    let bounds = &command["replacement_bounds"];
    Ok(PdfRect::new(
        finite_number(&bounds["x"], "image:resize-history bounds.x")?,
        finite_number(&bounds["y"], "image:resize-history bounds.y")?,
        finite_number(&bounds["width"], "image:resize-history bounds.width")?,
        finite_number(&bounds["height"], "image:resize-history bounds.height")?,
    )?)
}

fn natural_image_rect(
    source_width: u32,
    source_height: u32,
    placement: &ImageCreatePlan,
) -> Result<PdfRect, AnnotationError> {
    let source_width = f64::from(source_width);
    let source_height = f64::from(source_height);
    let scale = 1.0_f64
        .min(placement.page_width * placement.max_page_fraction / source_width)
        .min(placement.page_height * placement.max_page_fraction / source_height);
    let width = source_width * scale;
    let height = source_height * scale;
    PdfRect::new(
        (placement.point.x - width / 2.0).clamp(0.0, (placement.page_width - width).max(0.0)),
        (placement.point.y - height / 2.0).clamp(0.0, (placement.page_height - height).max(0.0)),
        width,
        height,
    )
}

fn blocked_milestone_reason(milestone: &str) -> &'static str {
    if milestone.contains("paint")
        || milestone.contains("shaped")
        || milestone.contains("layout")
        || milestone.contains("upload")
        || milestone.contains("thumbnail")
    {
        "requires observed GPUI paint, presentation, or GPU evidence"
    } else {
        "not directly proved by the deterministic semantic replay"
    }
}

fn facts<const N: usize>(entries: [(&str, Value); N]) -> BTreeMap<String, Value> {
    entries
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect()
}

fn point_json(point: PdfPoint) -> Value {
    json!({"x": point.x, "y": point.y})
}

fn rect_json(rect: crate::annotation_model::PdfRect) -> Value {
    json!({"x": rect.x, "y": rect.y, "width": rect.width, "height": rect.height})
}

fn scene_content_equal(
    left: &crate::annotation_model::AnnotationScene,
    right: &crate::annotation_model::AnnotationScene,
) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.revision = 0;
    right.revision = 0;
    for rectangle in &mut left.rectangles {
        rectangle.selected = false;
    }
    for rectangle in &mut right.rectangles {
        rectangle.selected = false;
    }
    for pen in &mut left.pens {
        pen.selected = false;
    }
    for pen in &mut right.pens {
        pen.selected = false;
    }
    for text in &mut left.text_boxes {
        text.selected = false;
    }
    for text in &mut right.text_boxes {
        text.selected = false;
    }
    for length in &mut left.lengths {
        length.selected = false;
    }
    for length in &mut right.lengths {
        length.selected = false;
    }
    for image in &mut left.images {
        image.selected = false;
    }
    for image in &mut right.images {
        image.selected = false;
    }
    left == right
}

fn seed_dense_page(
    adapter: &mut AnnotationAdapter,
    document_id: u64,
) -> Result<(), EditorComparisonError> {
    adapter.set_tool(AnnotationTool::Rectangle)?;
    for index in 0..100_u64 {
        let column = (index % 10) as f64;
        let row = (index / 10) as f64;
        let start = point(12.0 + column * 58.0, 18.0 + row * 68.0)?;
        let finish = point(start.x + 34.0, start.y + 28.0)?;
        adapter.queue_next_annotation_id(MarkupId::new(format!("density:{index:03}"))?);
        let pointer_id = 20_000 + index;
        adapter.pointer_down(document_id, 0, pointer_id, start, 1.0)?;
        adapter.pointer_up(pointer_id, finish)?;
    }
    Ok(())
}

fn replay_pointer_stream(
    adapter: &mut AnnotationAdapter,
    document_id: u64,
    pointer_id: u64,
    samples: &[[f64; 2]],
) -> Result<(), EditorComparisonError> {
    let first = samples
        .first()
        .ok_or_else(|| EditorComparisonError::Invalid("empty pointer stream".into()))?;
    let last = samples
        .last()
        .ok_or_else(|| EditorComparisonError::Invalid("empty pointer stream".into()))?;
    adapter.pointer_down(document_id, 0, pointer_id, point(first[0], first[1])?, 4.0)?;
    for sample in &samples[1..samples.len() - 1] {
        adapter.pointer_move(pointer_id, point(sample[0], sample[1])?)?;
    }
    adapter.pointer_up(pointer_id, point(last[0], last[1])?)?;
    Ok(())
}

fn verify_final_state(
    adapter: &AnnotationAdapter,
    document_id: u64,
) -> Result<(), EditorComparisonError> {
    let scene = adapter.document_scene(document_id, 0);
    if scene.rectangles.len() != 1
        || scene.pens.len() != 1
        || scene.text_boxes.len() != 1
        || scene.lengths.len() != 1
        || scene.images.len() != 1
        || scene.rectangles[0].rect
            != crate::annotation_model::PdfRect::new(90.0, 132.0, 210.0, 96.0)?
        || scene.pens[0].appearance.opacity() != 0.45
        || scene.text_boxes[0].content != "Beam B-12 / revision 4"
        || scene.text_boxes[0].layout_rect
            != crate::annotation_model::PdfRect::new(90.0, 390.0, 300.0, 84.0)?
        || scene.lengths[0].caption != "3.50 m"
        || scene.images[0].rect
            != crate::annotation_model::PdfRect::new(360.0, 390.0, 180.0, 135.0)?
    {
        return Err(EditorComparisonError::Invalid(
            "representative editor state differs from the executable oracle".into(),
        ));
    }
    Ok(())
}

fn point(x: f64, y: f64) -> Result<PdfPoint, AnnotationError> {
    PdfPoint::new(x, y)
}

fn canonical_sha256(value: &Value) -> String {
    fn canonicalize(value: &Value) -> Value {
        match value {
            Value::Array(values) => Value::Array(values.iter().map(canonicalize).collect()),
            Value::Object(values) => Value::Object(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), canonicalize(value)))
                    .collect::<BTreeMap<_, _>>()
                    .into_iter()
                    .collect(),
            ),
            value => value.clone(),
        }
    }
    let bytes = serde_json::to_vec(&canonicalize(value)).expect("JSON value must serialize");
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug)]
pub enum EditorComparisonError {
    Annotation(AnnotationError),
    Invalid(String),
}

impl From<AnnotationError> for EditorComparisonError {
    fn from(error: AnnotationError) -> Self {
        Self::Annotation(error)
    }
}

impl fmt::Display for EditorComparisonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Annotation(error) => error.fmt(formatter),
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for EditorComparisonError {}
