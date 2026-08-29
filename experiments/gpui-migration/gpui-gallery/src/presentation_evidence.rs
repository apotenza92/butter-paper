//! Honest live-frame evidence for the representative editor annotations.
//!
//! The probe deliberately separates GPUI text shaping, decoded `RenderImage`
//! availability, annotation-overlay paint submission, GPUI's production
//! platform-draw receipt, and sprite-atlas residency. The latter two can prove
//! GPUI submission and exact decoded atlas bytes, but not physical scanout or
//! physical bus traffic.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    annotation_adapter::{AnnotationAdapter, AnnotationTool},
    annotation_model::{
        AnnotationError, AnnotationScene, DecodedRgbaAsset, LengthCalibration, MarkupId, PdfPoint,
    },
    editor_comparison_scenario::{BlockedEditorMilestone, EditorCommandEvidence},
};

const WORKLOAD_JSON: &str = include_str!("../../performance/comparison-workload.json");
pub const EDITOR_CREATE_COMMAND_IDS: [&str; 4] = [
    "text:create",
    "length:set-scale",
    "length:create",
    "image:create",
];
pub const TEXT_CREATE_ID: &str = "comparison:text:1";
pub const LENGTH_CREATE_ID: &str = "comparison:length:1";
pub const IMAGE_CREATE_ID: &str = "comparison:image:1";

pub fn prepare_representative_create_scene(
    document_id: u64,
    checker: DecodedRgbaAsset,
    adapter: &mut AnnotationAdapter,
) -> Result<AnnotationScene, AnnotationError> {
    let workload: Value = serde_json::from_str(WORKLOAD_JSON)
        .map_err(|error| AnnotationError::InvalidFixture(error.to_string()))?;
    let image_command = workload["journeys"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|journey| journey["commands"].as_array().into_iter().flatten())
        .find(|command| command["id"].as_str() == Some("image:create"))
        .ok_or_else(|| AnnotationError::InvalidFixture("image:create is missing".into()))?;
    let image_number = |path: &[&str]| -> Result<f64, AnnotationError> {
        let value = path.iter().fold(image_command, |value, key| &value[*key]);
        value
            .as_f64()
            .filter(|number| number.is_finite())
            .ok_or_else(|| {
                AnnotationError::InvalidFixture(format!(
                    "image:create {} must be a finite number",
                    path.join(".")
                ))
            })
    };

    let length_calibration = LengthCalibration::from_scale(72.0, 1.0, "m", 2, true)?;
    adapter.set_length_calibration(length_calibration.clone())?;
    adapter.load_imported_annotations_with_document_state(
        document_id,
        Vec::new(),
        vec![(0, length_calibration)],
        Vec::new(),
    )?;

    adapter.set_tool(AnnotationTool::TextBox)?;
    adapter.queue_next_annotation_id(MarkupId::new(TEXT_CREATE_ID)?);
    adapter.pointer_down(document_id, 0, 60_001, PdfPoint::new(90.0, 390.0)?, 4.0)?;

    adapter.set_tool(AnnotationTool::Length)?;
    let length_start = PdfPoint::new(90.0, 510.0)?;
    let length_end = PdfPoint::new(306.0, 510.0)?;
    adapter.begin_length_placement(
        document_id,
        0,
        MarkupId::new(LENGTH_CREATE_ID)?,
        length_start,
    )?;
    adapter.update_length_placement(length_end, false)?;
    adapter.commit_length_placement(document_id, 0, length_end, false)?;

    adapter.set_image_asset(checker);
    adapter.set_image_placement_page(
        image_number(&["placement", "fixture_page_size_points", "width"])?,
        image_number(&["placement", "fixture_page_size_points", "height"])?,
        image_number(&["placement", "max_page_fraction"])?,
    )?;
    adapter.set_tool(AnnotationTool::Image)?;
    adapter.queue_next_annotation_id(MarkupId::new(IMAGE_CREATE_ID)?);
    adapter.pointer_down(
        document_id,
        0,
        60_003,
        PdfPoint::new(
            image_number(&["placement", "point", "x"])?,
            image_number(&["placement", "point", "y"])?,
        )?,
        4.0,
    )?;
    Ok(adapter.document_scene(document_id, 0))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RepresentativeCreateSemanticReport {
    pub manifest_id: String,
    pub command_ids: Vec<String>,
    pub commands: Vec<EditorCommandEvidence>,
}

/// Builds exact semantic evidence from the frozen four-command contract.
///
/// This does not prove native input, GPUI presentation, or a GPU upload. Those
/// observations belong to the later live-frame report.
pub fn build_representative_semantic_report(
    document_id: u64,
    scene: &AnnotationScene,
    adapter: &AnnotationAdapter,
) -> Result<RepresentativeCreateSemanticReport, String> {
    let workload: Value = serde_json::from_str(WORKLOAD_JSON)
        .map_err(|error| format!("invalid embedded comparison workload: {error}"))?;
    let manifest_id = workload["manifest_id"]
        .as_str()
        .ok_or_else(|| "embedded comparison workload has no manifest id".to_string())?;
    let all_commands = workload["journeys"]
        .as_array()
        .ok_or_else(|| "embedded comparison workload has no journeys".to_string())?
        .iter()
        .flat_map(|journey| journey["commands"].as_array().into_iter().flatten());
    let command_by_id = all_commands
        .filter_map(|command| Some((command["id"].as_str()?.to_owned(), command.clone())))
        .collect::<BTreeMap<_, _>>();
    let commands = EDITOR_CREATE_COMMAND_IDS
        .iter()
        .map(|id| {
            command_by_id
                .get(*id)
                .cloned()
                .ok_or_else(|| format!("embedded editor-create contract is missing {id}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if scene.page_index != 0
        || scene.text_boxes.len() != 1
        || scene.lengths.len() != 1
        || scene.images.len() != 1
        || !scene.rectangles.is_empty()
        || !scene.pens.is_empty()
        || adapter.history_depths(document_id) != (3, 0)
    {
        return Err("editor-create scene is not the isolated three-annotation result".into());
    }
    let text = scene
        .text_boxes
        .iter()
        .find(|annotation| annotation.id.as_str() == TEXT_CREATE_ID)
        .ok_or_else(|| "representative text is missing".to_string())?;
    let length = scene
        .lengths
        .iter()
        .find(|annotation| annotation.id.as_str() == LENGTH_CREATE_ID)
        .ok_or_else(|| "representative length is missing".to_string())?;
    let image = scene
        .images
        .iter()
        .find(|annotation| annotation.id.as_str() == IMAGE_CREATE_ID)
        .ok_or_else(|| "representative image is missing".to_string())?;
    let calibration = adapter
        .length_calibration()
        .ok_or_else(|| "representative measurement scale is missing".to_string())?;

    let text_command = &commands[0];
    let scale_command = &commands[1];
    let length_command = &commands[2];
    let image_command = &commands[3];
    let number_matches = |value: &Value, observed: f64| value.as_f64() == Some(observed);
    let rect_matches = |value: &Value, observed: crate::annotation_model::PdfRect| {
        number_matches(&value["x"], observed.x)
            && number_matches(&value["y"], observed.y)
            && number_matches(&value["width"], observed.width)
            && number_matches(&value["height"], observed.height)
    };
    let point_matches = |value: &Value, observed: PdfPoint| {
        number_matches(&value["x"], observed.x) && number_matches(&value["y"], observed.y)
    };
    let point_inside = |value: &Value, observed: crate::annotation_model::PdfRect| {
        let Some(x) = value["x"].as_f64() else {
            return false;
        };
        let Some(y) = value["y"].as_f64() else {
            return false;
        };
        x >= observed.x
            && x <= observed.x + observed.width
            && y >= observed.y
            && y <= observed.y + observed.height
    };
    let page_width = image_command["placement"]["fixture_page_size_points"]["width"]
        .as_f64()
        .unwrap_or_default();
    let page_height = image_command["placement"]["fixture_page_size_points"]["height"]
        .as_f64()
        .unwrap_or_default();
    let max_fraction = image_command["placement"]["max_page_fraction"]
        .as_f64()
        .unwrap_or_default();
    let image_scale = 1.0_f64
        .min(page_width * max_fraction / 512.0)
        .min(page_height * max_fraction / 384.0);
    let image_width = 512.0 * image_scale;
    let image_height = 384.0 * image_scale;
    let image_point_x = image_command["placement"]["point"]["x"]
        .as_f64()
        .unwrap_or_default();
    let image_point_y = image_command["placement"]["point"]["y"]
        .as_f64()
        .unwrap_or_default();
    let canonical_point_number = |value: f64| (value * 1_000_000.0).round() / 1_000_000.0;
    let expected_image_rect = json!({
        "x": canonical_point_number((image_point_x - image_width / 2.0).clamp(0.0, page_width - image_width)),
        "y": canonical_point_number((image_point_y - image_height / 2.0).clamp(0.0, page_height - image_height)),
        "width": canonical_point_number(image_width),
        "height": canonical_point_number(image_height),
    });
    let exact = text_command["annotation_id"].as_str() == Some(TEXT_CREATE_ID)
        && text_command["text"].as_str() == Some(text.content.as_str())
        && text_command["placement"]["sizing"].as_str() == Some("shaped-text-autosize-nonblank")
        && point_inside(&text_command["placement"]["point"], text.layout_rect)
        && number_matches(
            &scale_command["scale"]["paper_points"],
            calibration.paper_points(),
        )
        && number_matches(
            &scale_command["scale"]["real_world_value"],
            calibration.real_world_value(),
        )
        && scale_command["scale"]["unit"].as_str() == Some(calibration.unit())
        && scale_command["scale"]["precision"].as_u64() == Some(u64::from(calibration.precision()))
        && length_command["annotation_id"].as_str() == Some(LENGTH_CREATE_ID)
        && point_matches(&length_command["start"], length.start)
        && point_matches(&length_command["finish"], length.end)
        && length_command["expected_label"].as_str() == Some(length.caption.as_str())
        && image_command["annotation_id"].as_str() == Some(IMAGE_CREATE_ID)
        && image_command["placement"]["sizing"].as_str() == Some("natural-size-page-contained")
        && rect_matches(&expected_image_rect, image.rect)
        && image_command["asset_id"].as_str() == Some("bp-image-checker-v1")
        && image.width_px == 512
        && image.height_px == 384;
    if !exact {
        return Err("editor-create semantic state differs from the frozen contract".into());
    }

    let evidence = |index: usize,
                    proven: &[&str],
                    facts: BTreeMap<String, Value>|
     -> Result<EditorCommandEvidence, String> {
        let command = &commands[index];
        let id = command["id"]
            .as_str()
            .ok_or_else(|| "editor-create command has no id".to_string())?;
        let expected = command["expected_milestones"]
            .as_array()
            .ok_or_else(|| format!("{id} has no expected milestones"))?
            .iter()
            .map(|milestone| {
                milestone
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{id} has a non-string milestone"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if proven
            .iter()
            .any(|milestone| !expected.iter().any(|value| value == milestone))
        {
            return Err(format!("{id} tried to prove an undeclared milestone"));
        }
        Ok(EditorCommandEvidence {
            command_id: id.to_owned(),
            proven_manifest_milestones: expected
                .iter()
                .filter(|milestone| proven.contains(&milestone.as_str()))
                .cloned()
                .collect(),
            blocked_manifest_milestones: expected
                .iter()
                .filter(|milestone| !proven.contains(&milestone.as_str()))
                .map(|milestone| BlockedEditorMilestone {
                    milestone: milestone.clone(),
                    reason: "requires native GPUI input, presentation, or GPU upload evidence",
                })
                .collect(),
            facts,
        })
    };
    let semantic_commands = vec![
        evidence(
            0,
            &["text-input-committed", "gesture-committed-once"],
            BTreeMap::from([
                ("content".into(), json!(text.content)),
                ("history_delta".into(), json!(1)),
                (
                    "placement_point".into(),
                    text_command["placement"]["point"].clone(),
                ),
                (
                    "layout_bounds".into(),
                    json!({
                        "x": text.layout_rect.x,
                        "y": text.layout_rect.y,
                        "width": text.layout_rect.width,
                        "height": text.layout_rect.height,
                    }),
                ),
            ]),
        )?,
        evidence(
            1,
            &["measurement-scale-current"],
            BTreeMap::from([
                ("paper_points".into(), json!(calibration.paper_points())),
                ("precision".into(), json!(calibration.precision())),
                (
                    "real_world_value".into(),
                    json!(calibration.real_world_value()),
                ),
                ("unit".into(), json!(calibration.unit())),
            ]),
        )?,
        evidence(
            2,
            &["derived-length-exact", "gesture-committed-once"],
            BTreeMap::from([
                ("caption".into(), json!(length.caption)),
                ("end".into(), json!({"x": length.end.x, "y": length.end.y})),
                ("history_delta".into(), json!(1)),
                (
                    "start".into(),
                    json!({"x": length.start.x, "y": length.start.y}),
                ),
            ]),
        )?,
        evidence(
            3,
            &["bitmap-decoded", "gesture-committed-once"],
            BTreeMap::from([
                (
                    "bounds".into(),
                    json!({
                        "x": image.rect.x,
                        "y": image.rect.y,
                        "width": image.rect.width,
                        "height": image.rect.height,
                    }),
                ),
                (
                    "page_size".into(),
                    image_command["placement"]["fixture_page_size_points"].clone(),
                ),
                (
                    "placement_point".into(),
                    image_command["placement"]["point"].clone(),
                ),
                ("history_delta".into(), json!(1)),
                (
                    "rgba_bytes".into(),
                    json!(image.width_px as usize * image.height_px as usize * 4),
                ),
                ("source_height_px".into(), json!(image.height_px)),
                ("source_width_px".into(), json!(image.width_px)),
            ]),
        )?,
    ];

    Ok(RepresentativeCreateSemanticReport {
        manifest_id: manifest_id.to_owned(),
        command_ids: EDITOR_CREATE_COMMAND_IDS
            .iter()
            .map(|id| (*id).to_owned())
            .collect(),
        commands: semantic_commands,
    })
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct AnnotationOverlayPaintObservation {
    pub document_id: u64,
    pub page_index: u32,
    pub scene_revision: u64,
    pub rectangle_ids: Vec<String>,
    pub text_ids: Vec<String>,
    pub length_path_ids: Vec<String>,
    pub image_ids: Vec<String>,
}

impl AnnotationOverlayPaintObservation {
    pub fn from_scene(
        document_id: u64,
        page_index: u32,
        scene: &AnnotationScene,
        submitted_length_path_ids: Vec<String>,
    ) -> Self {
        Self {
            document_id,
            page_index,
            scene_revision: scene.revision,
            rectangle_ids: scene
                .rectangles
                .iter()
                .map(|annotation| annotation.id.as_str().to_owned())
                .collect(),
            text_ids: scene
                .text_boxes
                .iter()
                .map(|annotation| annotation.id.as_str().to_owned())
                .collect(),
            length_path_ids: submitted_length_path_ids,
            image_ids: scene
                .images
                .iter()
                .map(|annotation| annotation.id.as_str().to_owned())
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TextShapeObservation {
    pub annotation_id: String,
    pub text: String,
    pub font_family: String,
    pub font_size_px: f32,
    pub shaped_utf8_bytes: usize,
    pub shaped_width_px: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ImageDecodeObservation {
    pub annotation_id: String,
    pub render_image_id: usize,
    pub width_px: u32,
    pub height_px: u32,
    pub decoded_bgra_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BlockedPresentationMilestone {
    pub milestone: &'static str,
    pub reason: &'static str,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LiveCommandPresentationEvidence {
    pub command_id: &'static str,
    pub proven_manifest_milestones: Vec<&'static str>,
    pub blocked_manifest_milestones: Vec<BlockedPresentationMilestone>,
    pub native_input_completed: bool,
    pub native_input_blocker: Option<&'static str>,
    pub facts: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RepresentativeLivePresentationReport {
    pub scene_revision: u64,
    pub gpui_frame_callback_after_submission: bool,
    pub gpui_present_submission_observed: bool,
    pub gpui_image_atlas_entry_observed: bool,
    pub gpui_atlas_upload_bytes: Option<usize>,
    pub gpu_present_observed: bool,
    pub gpu_upload_bytes: Option<usize>,
    pub commands: Vec<LiveCommandPresentationEvidence>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RepresentativeCreateQualificationReport {
    pub commands: Vec<EditorCommandEvidence>,
    pub native_input_completed: bool,
    pub missing_requirements: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EditorFinalLiveReport {
    pub scene_revision: u64,
    pub thumbnail_current: bool,
    pub gpui_present_submission_observed: bool,
    pub gpui_atlas_upload_bytes: Option<usize>,
    pub commands: Vec<LiveCommandPresentationEvidence>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GpuiFinalFrameObservation {
    pub frame_callback_after_submission: bool,
    pub image_atlas_entry_observed: bool,
    pub atlas_upload_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DenseRectangleLiveReport {
    pub document_id: u64,
    pub scene_revision: u64,
    pub visible_rectangle_count: usize,
    pub overlay_rectangle_count: usize,
    pub thumbnail_rectangle_count: usize,
    pub evidence: EditorCommandEvidence,
}

pub fn build_dense_rectangle_live_report(
    document_id: u64,
    scene: &AnnotationScene,
    thumbnail: &AnnotationScene,
    paint: &AnnotationOverlayPaintObservation,
    gpui_frame_callback_after_submission: bool,
) -> Result<DenseRectangleLiveReport, String> {
    const TARGET_ID: &str = "comparison:rectangle:dense:1";
    if paint.document_id != document_id
        || paint.page_index != scene.page_index
        || paint.scene_revision != scene.revision
    {
        return Err("dense rectangle overlay receipt is stale".into());
    }
    if scene.rectangles.len() != 101
        || !scene
            .rectangles
            .iter()
            .any(|rectangle| rectangle.id.as_str() == TARGET_ID)
    {
        return Err("dense rectangle scene does not contain the canonical 101 rectangles".into());
    }
    let scene_ids = scene
        .rectangles
        .iter()
        .map(|rectangle| rectangle.id.as_str())
        .collect::<BTreeSet<_>>();
    let paint_ids = paint
        .rectangle_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if paint_ids != scene_ids {
        return Err("dense rectangle overlay did not submit every current rectangle".into());
    }
    if !gpui_frame_callback_after_submission {
        return Err("dense rectangle overlay has no later GPUI frame callback receipt".into());
    }
    let thumbnail_current = thumbnail.page_index == scene.page_index
        && thumbnail.revision == scene.revision
        && thumbnail.rectangles.len() == scene.rectangles.len()
        && scene.rectangles.iter().all(|rectangle| {
            thumbnail.rectangles.iter().any(|candidate| {
                candidate.id == rectangle.id
                    && candidate.rect == rectangle.rect
                    && candidate.appearance == rectangle.appearance
                    && candidate.locked == rectangle.locked
                    && !candidate.preview
            })
        });
    if !thumbnail_current {
        return Err("dense rectangle thumbnail is not the exact current projection".into());
    }

    Ok(DenseRectangleLiveReport {
        document_id,
        scene_revision: scene.revision,
        visible_rectangle_count: scene.rectangles.len(),
        overlay_rectangle_count: paint.rectangle_ids.len(),
        thumbnail_rectangle_count: thumbnail.rectangles.len(),
        evidence: EditorCommandEvidence {
            command_id: "rectangle:repeat-dense".into(),
            proven_manifest_milestones: vec!["annotation-paint-work-recorded".into()],
            blocked_manifest_milestones: Vec::new(),
            facts: BTreeMap::from([
                ("document_id".into(), json!(document_id)),
                ("gpui_frame_callback_after_submission".into(), json!(true)),
                (
                    "overlay_rectangle_count".into(),
                    json!(paint.rectangle_ids.len()),
                ),
                ("scene_revision".into(), json!(scene.revision)),
                ("thumbnail_current".into(), json!(true)),
                (
                    "thumbnail_rectangle_count".into(),
                    json!(thumbnail.rectangles.len()),
                ),
                (
                    "visible_rectangle_count".into(),
                    json!(scene.rectangles.len()),
                ),
            ]),
        },
    })
}

/// Proves presentation milestones whose subject is the final edited scene.
/// The atlas byte count is the decoded source resource size; resizing changes
/// page geometry but does not resample the stored image asset.
pub fn build_editor_final_live_report(
    scene: &AnnotationScene,
    thumbnail: &AnnotationScene,
    paint: &AnnotationOverlayPaintObservation,
    image_decode: &ImageDecodeObservation,
    gpui_frame: Option<GpuiFinalFrameObservation>,
) -> Result<EditorFinalLiveReport, String> {
    if paint.page_index != scene.page_index || paint.scene_revision != scene.revision {
        return Err("final annotation overlay paint observation is stale".into());
    }

    let scene_highlight = scene
        .pens
        .iter()
        .find(|annotation| annotation.id.as_str() == "comparison:highlight:1")
        .ok_or_else(|| "final highlight is missing".to_string())?;
    let thumbnail_highlight = thumbnail
        .pens
        .iter()
        .find(|annotation| annotation.id == scene_highlight.id);
    let highlight_current = thumbnail_highlight.is_some_and(|annotation| {
        annotation.points == scene_highlight.points
            && annotation.appearance == scene_highlight.appearance
            && annotation.tool == scene_highlight.tool
            && annotation.blend_mode == scene_highlight.blend_mode
            && annotation.smooth_curves == scene_highlight.smooth_curves
            && annotation.locked == scene_highlight.locked
            && !annotation.draft
    });

    let scene_length = scene
        .lengths
        .iter()
        .find(|annotation| annotation.id.as_str() == LENGTH_CREATE_ID)
        .ok_or_else(|| "final length is missing".to_string())?;
    let thumbnail_length = thumbnail
        .lengths
        .iter()
        .find(|annotation| annotation.id == scene_length.id);
    let length_current = thumbnail_length.is_some_and(|annotation| {
        annotation.start == scene_length.start
            && annotation.end == scene_length.end
            && annotation.caption == scene_length.caption
            && annotation.show_caption == scene_length.show_caption
            && annotation.locked == scene_length.locked
    });

    let scene_image = scene
        .images
        .iter()
        .find(|annotation| annotation.id.as_str() == IMAGE_CREATE_ID)
        .ok_or_else(|| "final image is missing".to_string())?;
    let thumbnail_image = thumbnail
        .images
        .iter()
        .find(|annotation| annotation.id == scene_image.id);
    let image_current = thumbnail_image.is_some_and(|annotation| {
        annotation.rect == scene_image.rect
            && annotation.asset_id == scene_image.asset_id
            && annotation.width_px == scene_image.width_px
            && annotation.height_px == scene_image.height_px
            && annotation.aspect_locked == scene_image.aspect_locked
            && annotation.locked == scene_image.locked
    });
    let thumbnail_current = thumbnail.page_index == scene.page_index
        && thumbnail.revision == scene.revision
        && highlight_current
        && length_current
        && image_current;

    let decoded_bytes = scene_image.width_px as usize * scene_image.height_px as usize * 4;
    let decode_current = image_decode.annotation_id == IMAGE_CREATE_ID
        && image_decode.width_px == scene_image.width_px
        && image_decode.height_px == scene_image.height_px
        && image_decode.decoded_bgra_bytes == decoded_bytes
        && paint.image_ids.iter().any(|id| id == IMAGE_CREATE_ID);
    let gpui_present_submission_observed = gpui_frame
        .as_ref()
        .is_some_and(|observation| observation.frame_callback_after_submission);
    let gpui_atlas_upload_bytes = gpui_frame.as_ref().and_then(|observation| {
        (decode_current
            && gpui_present_submission_observed
            && observation.image_atlas_entry_observed
            && observation.atlas_upload_bytes == decoded_bytes)
            .then_some(observation.atlas_upload_bytes)
    });

    let command =
        |command_id: &'static str,
         milestone: &'static str,
         proven: bool,
         reason: &'static str,
         facts: BTreeMap<String, Value>| LiveCommandPresentationEvidence {
            command_id,
            proven_manifest_milestones: proven.then_some(milestone).into_iter().collect(),
            blocked_manifest_milestones: (!proven)
                .then_some(BlockedPresentationMilestone { milestone, reason })
                .into_iter()
                .collect(),
            native_input_completed: false,
            native_input_blocker: Some("editor-workload is an untimed semantic command lane"),
            facts,
        };

    Ok(EditorFinalLiveReport {
        scene_revision: scene.revision,
        thumbnail_current,
        gpui_present_submission_observed,
        gpui_atlas_upload_bytes,
        commands: vec![
            command(
                "highlight:edit-history",
                "thumbnail-current",
                thumbnail_current,
                "the final thumbnail does not contain the exact edited highlight projection",
                BTreeMap::from([
                    ("scene_revision".into(), json!(scene.revision)),
                    ("thumbnail_revision".into(), json!(thumbnail.revision)),
                ]),
            ),
            command(
                "length:edit-endpoint-history",
                "thumbnail-current",
                thumbnail_current,
                "the final thumbnail does not contain the exact edited length projection",
                BTreeMap::from([
                    ("caption".into(), json!(scene_length.caption)),
                    ("scene_revision".into(), json!(scene.revision)),
                    ("thumbnail_revision".into(), json!(thumbnail.revision)),
                ]),
            ),
            LiveCommandPresentationEvidence {
                command_id: "image:resize-history",
                proven_manifest_milestones: {
                    let mut milestones = Vec::new();
                    if gpui_atlas_upload_bytes.is_some() {
                        milestones.push("upload-byte-count-recorded");
                    }
                    if thumbnail_current {
                        milestones.push("thumbnail-current");
                    }
                    milestones
                },
                blocked_manifest_milestones: {
                    let mut blocked = Vec::new();
                    if gpui_atlas_upload_bytes.is_none() {
                        blocked.push(BlockedPresentationMilestone {
                            milestone: "upload-byte-count-recorded",
                            reason: "no matching final GPUI sprite-atlas entry and exact decoded byte count were observed",
                        });
                    }
                    if !thumbnail_current {
                        blocked.push(BlockedPresentationMilestone {
                            milestone: "thumbnail-current",
                            reason: "the final thumbnail does not contain the exact resized image projection",
                        });
                    }
                    blocked
                },
                native_input_completed: false,
                native_input_blocker: Some("editor-workload is an untimed semantic command lane"),
                facts: BTreeMap::from([
                    ("decoded_bgra_bytes".into(), json!(decoded_bytes)),
                    (
                        "gpui_atlas_upload_bytes".into(),
                        gpui_atlas_upload_bytes.map_or(Value::Null, Value::from),
                    ),
                    (
                        "gpui_atlas_upload_scope".into(),
                        json!("decoded-sprite-atlas-bytes-not-physical-bus-bytes"),
                    ),
                    ("scene_revision".into(), json!(scene.revision)),
                    ("thumbnail_revision".into(), json!(thumbnail.revision)),
                ]),
            },
        ],
    })
}

/// Combines the deterministic editor state proof with observations from the
/// native GPUI frame. A command qualifies only when every milestone declared
/// by its frozen manifest command is present and the native replay completed.
pub fn qualify_representative_create(
    semantic: &RepresentativeCreateSemanticReport,
    live: &RepresentativeLivePresentationReport,
) -> Result<RepresentativeCreateQualificationReport, String> {
    if semantic.command_ids
        != EDITOR_CREATE_COMMAND_IDS
            .iter()
            .map(|id| (*id).to_owned())
            .collect::<Vec<_>>()
    {
        return Err("semantic editor-create report does not match the frozen command set".into());
    }

    let native_input_completed = live.commands.len() == 3
        && live
            .commands
            .iter()
            .all(|command| command.native_input_completed);
    let mut missing_requirements = Vec::new();
    if !native_input_completed {
        for command_id in EDITOR_CREATE_COMMAND_IDS {
            missing_requirements.push(format!("native-input:{command_id}"));
        }
    }

    let mut commands = Vec::with_capacity(semantic.commands.len());
    for semantic_command in &semantic.commands {
        let expected = semantic_command
            .proven_manifest_milestones
            .iter()
            .chain(
                semantic_command
                    .blocked_manifest_milestones
                    .iter()
                    .map(|blocked| &blocked.milestone),
            )
            .cloned()
            .collect::<Vec<_>>();
        let live_command = live
            .commands
            .iter()
            .find(|candidate| candidate.command_id == semantic_command.command_id);
        let live_proven = live_command
            .into_iter()
            .flat_map(|command| command.proven_manifest_milestones.iter().copied())
            .collect::<BTreeSet<_>>();
        let proven = expected
            .iter()
            .filter(|milestone| {
                semantic_command
                    .proven_manifest_milestones
                    .contains(milestone)
                    || live_proven.contains(milestone.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        let blocked = expected
            .iter()
            .filter(|milestone| !proven.contains(milestone))
            .map(|milestone| BlockedEditorMilestone {
                milestone: milestone.clone(),
                reason: "not proved by the combined semantic and native GPUI presentation evidence",
            })
            .collect::<Vec<_>>();
        for milestone in &blocked {
            missing_requirements.push(format!(
                "manifest:{}:{}",
                semantic_command.command_id, milestone.milestone
            ));
        }
        let mut facts = semantic_command.facts.clone();
        if let Some(live_command) = live_command {
            facts.insert(
                "live_presentation".into(),
                serde_json::to_value(live_command)
                    .map_err(|error| format!("failed to serialize live evidence: {error}"))?,
            );
        }
        commands.push(EditorCommandEvidence {
            command_id: semantic_command.command_id.clone(),
            proven_manifest_milestones: proven,
            blocked_manifest_milestones: blocked,
            facts,
        });
    }

    Ok(RepresentativeCreateQualificationReport {
        commands,
        native_input_completed,
        missing_requirements,
    })
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GpuiSubmissionObservation {
    pub input_latency_samples_before: u64,
    pub input_latency_samples_after: u64,
    pub input_to_present_p50_ns: u64,
    pub input_to_present_p95_ns: u64,
    pub image_atlas_entry_observed: bool,
    pub atlas_upload_bytes: usize,
}

pub fn build_representative_live_report(
    scene: &AnnotationScene,
    paint: &AnnotationOverlayPaintObservation,
    text_shape: &TextShapeObservation,
    length_shape: &TextShapeObservation,
    image_decode: &ImageDecodeObservation,
    gpui_frame_callback_after_submission: bool,
    native_input_completed: bool,
    gpui_submission: Option<GpuiSubmissionObservation>,
) -> Result<RepresentativeLivePresentationReport, String> {
    if paint.page_index != scene.page_index || paint.scene_revision != scene.revision {
        return Err("annotation overlay paint observation is stale".into());
    }
    let text = scene
        .text_boxes
        .iter()
        .find(|annotation| annotation.id.as_str() == TEXT_CREATE_ID)
        .ok_or_else(|| "representative text is missing".to_string())?;
    let length = scene
        .lengths
        .iter()
        .find(|annotation| annotation.id.as_str() == LENGTH_CREATE_ID)
        .ok_or_else(|| "representative length is missing".to_string())?;
    let image = scene
        .images
        .iter()
        .find(|annotation| annotation.id.as_str() == IMAGE_CREATE_ID)
        .ok_or_else(|| "representative image is missing".to_string())?;

    let text_shaped = paint.text_ids.iter().any(|id| id == TEXT_CREATE_ID)
        && text_shape.annotation_id == TEXT_CREATE_ID
        && text_shape.text == text.content
        && (text_shape.font_family == text.style.font_family()
            || (text.style.font_family() == "Helvetica" && text_shape.font_family == "Geist"))
        && text_shape.shaped_utf8_bytes == text.content.len()
        && text_shape.shaped_width_px.is_finite()
        && text_shape.shaped_width_px > 0.0;
    let label_layout_current = paint
        .length_path_ids
        .iter()
        .any(|id| id == LENGTH_CREATE_ID)
        && length_shape.annotation_id == LENGTH_CREATE_ID
        && length_shape.text == length.caption
        && length_shape.shaped_utf8_bytes == length.caption.len()
        && length_shape.shaped_width_px.is_finite()
        && length_shape.shaped_width_px > 0.0;
    let image_decoded = paint.image_ids.iter().any(|id| id == IMAGE_CREATE_ID)
        && image_decode.annotation_id == IMAGE_CREATE_ID
        && image_decode.width_px == image.width_px
        && image_decode.height_px == image.height_px
        && image_decode.decoded_bgra_bytes
            == image.width_px as usize * image.height_px as usize * 4;
    if !text_shaped || !label_layout_current || !image_decoded {
        return Err("representative live presentation observations are incomplete".into());
    }

    let input_blocker = (!native_input_completed)
        .then_some("editor-create has no completed native keyboard and pointer replay receipt");
    let gpui_present_submission_observed = gpui_frame_callback_after_submission
        && gpui_submission.as_ref().is_some_and(|observation| {
            observation.input_latency_samples_after > observation.input_latency_samples_before
        });
    let gpui_image_atlas_entry_observed = gpui_present_submission_observed
        && gpui_submission
            .as_ref()
            .is_some_and(|observation| observation.image_atlas_entry_observed);
    let gpui_atlas_upload_bytes = gpui_submission.as_ref().and_then(|observation| {
        (gpui_image_atlas_entry_observed
            && observation.atlas_upload_bytes == image_decode.decoded_bgra_bytes)
            .then_some(observation.atlas_upload_bytes)
    });
    let common_facts = || {
        BTreeMap::from([
            ("annotation_overlay_paint_submitted".into(), json!(true)),
            (
                "gpui_frame_callback_after_submission".into(),
                json!(gpui_frame_callback_after_submission),
            ),
            (
                "gpui_present_submission_observed".into(),
                json!(gpui_present_submission_observed),
            ),
            (
                "gpui_present_receipt_scope".into(),
                json!("platform-draw-submission-not-physical-scanout"),
            ),
            ("gpu_present_observed".into(), json!(false)),
            ("gpu_upload_bytes".into(), Value::Null),
            ("scene_revision".into(), json!(scene.revision)),
        ])
    };

    let mut text_facts = common_facts();
    text_facts.insert("shape".into(), json!(text_shape));
    let mut length_facts = common_facts();
    length_facts.insert("label_shape".into(), json!(length_shape));
    length_facts.insert("path_paint_submitted".into(), json!(true));
    let mut image_facts = common_facts();
    image_facts.insert("decoded_render_image".into(), json!(image_decode));
    image_facts.insert(
        "gpui_image_atlas_entry_observed".into(),
        json!(gpui_image_atlas_entry_observed),
    );
    image_facts.insert(
        "gpui_atlas_upload_bytes".into(),
        gpui_atlas_upload_bytes.map_or(Value::Null, Value::from),
    );
    image_facts.insert(
        "gpui_atlas_upload_scope".into(),
        json!("decoded-sprite-atlas-bytes-not-physical-bus-bytes"),
    );
    if let Some(observation) = gpui_submission.as_ref() {
        for facts in [&mut text_facts, &mut length_facts, &mut image_facts] {
            facts.insert(
                "gpui_input_latency_samples_before".into(),
                json!(observation.input_latency_samples_before),
            );
            facts.insert(
                "gpui_input_latency_samples_after".into(),
                json!(observation.input_latency_samples_after),
            );
            facts.insert(
                "gpui_input_to_present_p50_ns".into(),
                json!(observation.input_to_present_p50_ns),
            );
            facts.insert(
                "gpui_input_to_present_p95_ns".into(),
                json!(observation.input_to_present_p95_ns),
            );
        }
    }

    let annotation_painted_blocker = || BlockedPresentationMilestone {
        milestone: "annotation-painted",
        reason: "the annotation overlay was submitted, but no GPUI input-to-platform-draw present receipt was observed",
    };

    Ok(RepresentativeLivePresentationReport {
        scene_revision: scene.revision,
        gpui_frame_callback_after_submission,
        gpui_present_submission_observed,
        gpui_image_atlas_entry_observed,
        gpui_atlas_upload_bytes,
        gpu_present_observed: false,
        gpu_upload_bytes: None,
        commands: vec![
            LiveCommandPresentationEvidence {
                command_id: "text:create",
                proven_manifest_milestones: if gpui_present_submission_observed {
                    vec!["text-shaped", "annotation-painted"]
                } else {
                    vec!["text-shaped"]
                },
                blocked_manifest_milestones: (!gpui_present_submission_observed)
                    .then(annotation_painted_blocker)
                    .into_iter()
                    .collect(),
                native_input_completed,
                native_input_blocker: input_blocker,
                facts: text_facts,
            },
            LiveCommandPresentationEvidence {
                command_id: "length:create",
                proven_manifest_milestones: vec!["label-layout-current"],
                blocked_manifest_milestones: Vec::new(),
                native_input_completed,
                native_input_blocker: input_blocker,
                facts: length_facts,
            },
            LiveCommandPresentationEvidence {
                command_id: "image:create",
                proven_manifest_milestones: {
                    let mut milestones = vec!["bitmap-decoded"];
                    if gpui_atlas_upload_bytes.is_some() {
                        milestones.push("bitmap-upload-recorded");
                    }
                    if gpui_present_submission_observed {
                        milestones.push("annotation-painted");
                    }
                    milestones
                },
                blocked_manifest_milestones: {
                    let mut milestones = Vec::new();
                    if gpui_atlas_upload_bytes.is_none() {
                        milestones.push(BlockedPresentationMilestone {
                        milestone: "bitmap-upload-recorded",
                            reason: "the decoded RenderImage was available, but no matching GPUI sprite-atlas entry and exact decoded upload byte count were observed",
                        });
                    }
                    if !gpui_present_submission_observed {
                        milestones.push(annotation_painted_blocker());
                    }
                    milestones
                },
                native_input_completed,
                native_input_blocker: input_blocker,
                facts: image_facts,
            },
        ],
    })
}
