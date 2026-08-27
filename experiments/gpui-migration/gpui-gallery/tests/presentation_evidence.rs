use butter_paper_gpui_gallery::{
    annotation_adapter::{AnnotationAdapter, AnnotationTool},
    annotation_model::{DecodedRgbaAsset, MarkupId, PdfPoint},
    editor_comparison_scenario::{EditorComparisonScenario, RecordingEditorObserver},
    presentation_evidence::{
        AnnotationOverlayPaintObservation, GpuiFinalFrameObservation, GpuiSubmissionObservation,
        IMAGE_CREATE_ID, ImageDecodeObservation, LENGTH_CREATE_ID, TEXT_CREATE_ID,
        TextShapeObservation, build_dense_rectangle_live_report, build_editor_final_live_report,
        build_representative_live_report, build_representative_semantic_report,
        prepare_representative_create_scene, qualify_representative_create,
    },
};

#[test]
fn overlay_submission_names_the_rectangle_drawn_in_that_scene_revision() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Rectangle).unwrap();
    adapter.queue_next_annotation_id(MarkupId::new("comparison:rectangle:sparse:1").unwrap());
    adapter
        .pointer_down(51, 0, 1, PdfPoint::new(72.0, 144.0).unwrap(), 4.0)
        .unwrap();
    adapter
        .pointer_up(1, PdfPoint::new(252.0, 240.0).unwrap())
        .unwrap();
    let scene = adapter.document_scene(51, 0);
    let paint = AnnotationOverlayPaintObservation::from_scene(51, 0, &scene, vec![]);

    assert_eq!(paint.rectangle_ids, ["comparison:rectangle:sparse:1"]);
}

fn representative_scene() -> butter_paper_gpui_gallery::annotation_model::AnnotationScene {
    let mut adapter = AnnotationAdapter::default();
    prepare_representative_create_scene(
        42,
        DecodedRgbaAsset::new(512, 384, vec![0x80; 512 * 384 * 4]).unwrap(),
        &mut adapter,
    )
    .unwrap()
}

#[test]
fn records_shape_decode_and_overlay_submission_without_claiming_gpu_present_or_upload() {
    let scene = representative_scene();
    let paint =
        AnnotationOverlayPaintObservation::from_scene(42, 0, &scene, vec![LENGTH_CREATE_ID.into()]);
    let text_shape = TextShapeObservation {
        annotation_id: TEXT_CREATE_ID.into(),
        text: "Beam B-12 / revision 3".into(),
        font_family: "Helvetica".into(),
        font_size_px: 14.0,
        shaped_utf8_bytes: "Beam B-12 / revision 3".len(),
        shaped_width_px: 155.0,
    };
    let length_shape = TextShapeObservation {
        annotation_id: LENGTH_CREATE_ID.into(),
        text: "3.00 m".into(),
        font_family: "Geist".into(),
        font_size_px: 12.0,
        shaped_utf8_bytes: "3.00 m".len(),
        shaped_width_px: 38.0,
    };
    let image_decode = ImageDecodeObservation {
        annotation_id: IMAGE_CREATE_ID.into(),
        render_image_id: 7,
        width_px: 512,
        height_px: 384,
        decoded_bgra_bytes: 512 * 384 * 4,
    };
    let report = build_representative_live_report(
        &scene,
        &paint,
        &text_shape,
        &length_shape,
        &image_decode,
        true,
        false,
        None,
    )
    .unwrap();

    assert!(!report.gpu_present_observed);
    assert_eq!(report.gpu_upload_bytes, None);
    assert_eq!(
        report.commands[0].proven_manifest_milestones,
        ["text-shaped"]
    );
    assert_eq!(
        report.commands[1].proven_manifest_milestones,
        ["label-layout-current"]
    );
    assert_eq!(
        report.commands[2].proven_manifest_milestones,
        ["bitmap-decoded"]
    );
    assert!(
        report
            .commands
            .iter()
            .all(|evidence| !evidence.native_input_completed)
    );
    assert_eq!(
        report.commands[2].blocked_manifest_milestones[0].milestone,
        "bitmap-upload-recorded"
    );
}

#[test]
fn rejects_a_stale_paint_observation() {
    let scene = representative_scene();
    let mut paint =
        AnnotationOverlayPaintObservation::from_scene(42, 0, &scene, vec![LENGTH_CREATE_ID.into()]);
    paint.scene_revision = paint.scene_revision.saturating_sub(1);
    let empty_shape = TextShapeObservation {
        annotation_id: TEXT_CREATE_ID.into(),
        text: String::new(),
        font_family: String::new(),
        font_size_px: 0.0,
        shaped_utf8_bytes: 0,
        shaped_width_px: 0.0,
    };
    let image = ImageDecodeObservation {
        annotation_id: IMAGE_CREATE_ID.into(),
        render_image_id: 0,
        width_px: 0,
        height_px: 0,
        decoded_bgra_bytes: 0,
    };
    assert_eq!(
        build_representative_live_report(
            &scene,
            &paint,
            &empty_shape,
            &empty_shape,
            &image,
            true,
            false,
            None,
        )
        .unwrap_err(),
        "annotation overlay paint observation is stale"
    );
}

#[test]
fn records_gpui_present_submission_and_exact_atlas_upload_without_claiming_scanout_or_bus_bytes() {
    let scene = representative_scene();
    let paint =
        AnnotationOverlayPaintObservation::from_scene(42, 0, &scene, vec![LENGTH_CREATE_ID.into()]);
    let text_shape = TextShapeObservation {
        annotation_id: TEXT_CREATE_ID.into(),
        text: "Beam B-12 / revision 3".into(),
        font_family: "Helvetica".into(),
        font_size_px: 14.0,
        shaped_utf8_bytes: "Beam B-12 / revision 3".len(),
        shaped_width_px: 155.0,
    };
    let length_shape = TextShapeObservation {
        annotation_id: LENGTH_CREATE_ID.into(),
        text: "3.00 m".into(),
        font_family: "Geist".into(),
        font_size_px: 12.0,
        shaped_utf8_bytes: "3.00 m".len(),
        shaped_width_px: 38.0,
    };
    let image_decode = ImageDecodeObservation {
        annotation_id: IMAGE_CREATE_ID.into(),
        render_image_id: 7,
        width_px: 512,
        height_px: 384,
        decoded_bgra_bytes: 512 * 384 * 4,
    };
    let report = build_representative_live_report(
        &scene,
        &paint,
        &text_shape,
        &length_shape,
        &image_decode,
        true,
        true,
        Some(GpuiSubmissionObservation {
            input_latency_samples_before: 4,
            input_latency_samples_after: 8,
            input_to_present_p50_ns: 1_200_000,
            input_to_present_p95_ns: 2_300_000,
            image_atlas_entry_observed: true,
            atlas_upload_bytes: image_decode.decoded_bgra_bytes,
        }),
    )
    .unwrap();

    assert!(report.gpui_present_submission_observed);
    assert!(report.gpui_image_atlas_entry_observed);
    assert_eq!(report.gpui_atlas_upload_bytes, Some(512 * 384 * 4));
    assert!(!report.gpu_present_observed);
    assert_eq!(report.gpu_upload_bytes, None);
    assert!(
        report.commands[0]
            .proven_manifest_milestones
            .contains(&"annotation-painted")
    );
    assert!(
        report.commands[2]
            .proven_manifest_milestones
            .contains(&"annotation-painted")
    );
    assert!(
        report.commands[2]
            .proven_manifest_milestones
            .contains(&"bitmap-upload-recorded")
    );
    assert!(report.commands[0].blocked_manifest_milestones.is_empty());
    assert!(report.commands[2].blocked_manifest_milestones.is_empty());
}

#[test]
fn combines_exact_semantics_with_native_gpui_presentation_without_stale_blockers() {
    let mut adapter = AnnotationAdapter::default();
    let scene = prepare_representative_create_scene(
        42,
        DecodedRgbaAsset::new(512, 384, vec![0x80; 512 * 384 * 4]).unwrap(),
        &mut adapter,
    )
    .unwrap();
    let semantic = build_representative_semantic_report(42, &scene, &adapter).unwrap();
    let paint =
        AnnotationOverlayPaintObservation::from_scene(42, 0, &scene, vec![LENGTH_CREATE_ID.into()]);
    let text_shape = TextShapeObservation {
        annotation_id: TEXT_CREATE_ID.into(),
        text: "Beam B-12 / revision 3".into(),
        font_family: "Helvetica".into(),
        font_size_px: 14.0,
        shaped_utf8_bytes: "Beam B-12 / revision 3".len(),
        shaped_width_px: 155.0,
    };
    let length_shape = TextShapeObservation {
        annotation_id: LENGTH_CREATE_ID.into(),
        text: "3.00 m".into(),
        font_family: "Geist".into(),
        font_size_px: 12.0,
        shaped_utf8_bytes: "3.00 m".len(),
        shaped_width_px: 38.0,
    };
    let image_decode = ImageDecodeObservation {
        annotation_id: IMAGE_CREATE_ID.into(),
        render_image_id: 7,
        width_px: 512,
        height_px: 384,
        decoded_bgra_bytes: 512 * 384 * 4,
    };
    let live = build_representative_live_report(
        &scene,
        &paint,
        &text_shape,
        &length_shape,
        &image_decode,
        true,
        true,
        Some(GpuiSubmissionObservation {
            input_latency_samples_before: 4,
            input_latency_samples_after: 8,
            input_to_present_p50_ns: 1_200_000,
            input_to_present_p95_ns: 2_300_000,
            image_atlas_entry_observed: true,
            atlas_upload_bytes: image_decode.decoded_bgra_bytes,
        }),
    )
    .unwrap();

    let qualified = qualify_representative_create(&semantic, &live).unwrap();

    assert!(qualified.native_input_completed);
    assert!(qualified.missing_requirements.is_empty());
    assert_eq!(qualified.commands.len(), 4);
    assert!(
        qualified
            .commands
            .iter()
            .all(|command| command.blocked_manifest_milestones.is_empty())
    );
}

#[test]
fn decision_3_semantic_report_proves_exact_natural_page_contained_image_placement() {
    let mut adapter = AnnotationAdapter::default();
    let scene = prepare_representative_create_scene(
        42,
        DecodedRgbaAsset::new(512, 384, vec![0x80; 512 * 384 * 4]).unwrap(),
        &mut adapter,
    )
    .unwrap();
    let report = build_representative_semantic_report(42, &scene, &adapter).unwrap();
    let image = report
        .commands
        .iter()
        .find(|command| command.command_id == "image:create")
        .unwrap();
    assert_eq!(
        image.facts["bounds"],
        serde_json::json!({
            "x": 294.3,
            "y": 340.725,
            "width": 275.4,
            "height": 206.55
        })
    );
}

#[test]
fn final_editor_frame_proves_edit_thumbnails_and_resized_image_upload_bytes() {
    let mut adapter = AnnotationAdapter::default();
    EditorComparisonScenario::embedded()
        .unwrap()
        .execute(
            42,
            DecodedRgbaAsset::new(512, 384, vec![0x80; 512 * 384 * 4]).unwrap(),
            &mut adapter,
            &mut RecordingEditorObserver::default(),
        )
        .unwrap();
    let scene = adapter.document_scene(42, 0);
    let thumbnail = adapter.thumbnail_scene(42, 0);
    let paint = AnnotationOverlayPaintObservation::from_scene(
        42,
        0,
        &scene,
        vec!["comparison:length:1".into()],
    );
    let image_decode = ImageDecodeObservation {
        annotation_id: IMAGE_CREATE_ID.into(),
        render_image_id: 7,
        width_px: 512,
        height_px: 384,
        decoded_bgra_bytes: 512 * 384 * 4,
    };

    let report = build_editor_final_live_report(
        &scene,
        &thumbnail,
        &paint,
        &image_decode,
        Some(GpuiFinalFrameObservation {
            frame_callback_after_submission: true,
            image_atlas_entry_observed: true,
            atlas_upload_bytes: image_decode.decoded_bgra_bytes,
        }),
    )
    .unwrap();

    assert_eq!(report.scene_revision, scene.revision);
    assert!(report.thumbnail_current);
    assert_eq!(report.gpui_atlas_upload_bytes, Some(512 * 384 * 4));
    assert!(report.commands.iter().all(|command| {
        command.blocked_manifest_milestones.is_empty()
            && !command.proven_manifest_milestones.is_empty()
    }));
}

#[test]
fn dense_rectangle_frame_records_exact_overlay_and_thumbnail_work() {
    let mut adapter = AnnotationAdapter::default();
    EditorComparisonScenario::embedded()
        .unwrap()
        .execute(
            42,
            DecodedRgbaAsset::new(512, 384, vec![0x80; 512 * 384 * 4]).unwrap(),
            &mut adapter,
            &mut RecordingEditorObserver::default(),
        )
        .unwrap();
    let scene = adapter.document_scene(43, 0);
    let thumbnail = adapter.thumbnail_scene(43, 0);
    let paint = AnnotationOverlayPaintObservation::from_scene(43, 0, &scene, vec![]);

    let report = build_dense_rectangle_live_report(43, &scene, &thumbnail, &paint, true).unwrap();

    assert_eq!(report.visible_rectangle_count, 101);
    assert_eq!(report.overlay_rectangle_count, 101);
    assert_eq!(report.thumbnail_rectangle_count, 101);
    assert_eq!(
        report.evidence.proven_manifest_milestones,
        ["annotation-paint-work-recorded"]
    );
    assert!(report.evidence.blocked_manifest_milestones.is_empty());
}
