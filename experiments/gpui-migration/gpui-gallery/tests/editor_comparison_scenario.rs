use butter_paper_gpui_gallery::{
    annotation_adapter::AnnotationAdapter,
    annotation_model::{DecodedRgbaAsset, StrokeStyle},
    editor_comparison_scenario::{
        EditorComparisonEvent, EditorComparisonScenario, RecordingEditorObserver,
    },
};

#[test]
fn executes_the_exact_representative_editor_state_through_the_public_adapter() {
    let mut adapter = AnnotationAdapter::default();
    let checker = DecodedRgbaAsset::new(512, 384, vec![0x80; 512 * 384 * 4]).unwrap();
    let mut observer = RecordingEditorObserver::default();

    let report = EditorComparisonScenario::embedded()
        .unwrap()
        .execute(41, checker, &mut adapter, &mut observer)
        .unwrap();

    assert_eq!(report.completed_command_ids.len(), 13);
    assert_eq!(report.command_evidence.len(), 13);
    assert_eq!(report.completed_command_ids[0], "rectangle:create-sparse");
    assert_eq!(report.completed_command_ids[12], "image:resize-history");
    assert!(report.blocked_commands.is_empty());
    assert_eq!(report.history_depths.1, 0);
    assert!(report.dirty);
    let rectangle_transform = report
        .evidence_for("rectangle:select-move-resize")
        .expect("rectangle transform evidence");
    assert_eq!(
        rectangle_transform.proven_manifest_milestones,
        [
            "hit-test-selected",
            "move-committed-once",
            "resize-committed-once"
        ]
    );
    assert!(rectangle_transform.blocked_manifest_milestones.is_empty());
    assert_eq!(rectangle_transform.facts["history_delta"], 2);
    assert_eq!(
        rectangle_transform.facts["final_bounds"],
        serde_json::json!({"height": 96.0, "width": 210.0, "x": 90.0, "y": 132.0})
    );

    let text_create = report.evidence_for("text:create").unwrap();
    assert_eq!(
        text_create.proven_manifest_milestones,
        ["text-input-committed", "gesture-committed-once"]
    );
    assert_eq!(
        text_create
            .blocked_manifest_milestones
            .iter()
            .map(|blocked| blocked.milestone.as_str())
            .collect::<Vec<_>>(),
        ["text-shaped", "annotation-painted"]
    );

    let highlight_create = report.evidence_for("highlight:create").unwrap();
    assert_eq!(
        highlight_create.proven_manifest_milestones,
        [
            "pointer-stream-received",
            "path-smoothed",
            "gesture-committed-once"
        ]
    );
    assert_eq!(highlight_create.facts["submitted_samples"], 361);
    assert_eq!(highlight_create.facts["stored_input_points"], 332);
    assert_eq!(highlight_create.facts["polyline_segments"], 331);
    assert_eq!(
        highlight_create.facts["renderer_contract"],
        "straight-polyline"
    );
    assert_eq!(
        highlight_create.facts["canonical_geometry_resample_count"],
        64
    );

    let image_create = report.evidence_for("image:create").unwrap();
    assert_eq!(
        image_create.proven_manifest_milestones,
        ["bitmap-decoded", "gesture-committed-once"]
    );
    assert_eq!(
        image_create
            .blocked_manifest_milestones
            .iter()
            .map(|blocked| blocked.milestone.as_str())
            .collect::<Vec<_>>(),
        ["bitmap-upload-recorded", "annotation-painted"]
    );
    assert_eq!(image_create.facts["rgba_bytes"], 512 * 384 * 4);
    assert_eq!(
        image_create.facts["bounds"],
        serde_json::json!({
            "x": 294.3,
            "y": 340.725,
            "width": 275.4,
            "height": 206.55
        })
    );
    assert_eq!(
        image_create.facts["placement_point"],
        serde_json::json!({"x": 432.0, "y": 444.0})
    );
    assert_eq!(image_create.facts["max_page_fraction"], 0.45);
    assert_eq!(
        image_create.facts["page_size"],
        serde_json::json!({"width": 612.0, "height": 792.0})
    );

    let dense = report.evidence_for("rectangle:repeat-dense").unwrap();
    assert_eq!(
        dense.proven_manifest_milestones,
        ["spatial-index-work-recorded", "canonical-state-matched"]
    );
    assert_eq!(
        dense.blocked_manifest_milestones[0].milestone,
        "annotation-paint-work-recorded"
    );
    let scene = adapter.document_scene(41, 0);
    assert_eq!(scene.rectangles[0].rect.x, 90.0);
    assert_eq!(scene.rectangles[0].rect.y, 132.0);
    assert_eq!(scene.rectangles[0].rect.width, 210.0);
    assert_eq!(scene.rectangles[0].appearance.stroke_color(), "#dc2626");
    assert!((scene.rectangles[0].appearance.opacity() - 0.88).abs() < 0.000_001);
    assert!((scene.rectangles[0].appearance.fill_opacity() - 31.0 / 255.0).abs() < 0.000_001);
    assert_eq!(
        scene.rectangles[0].appearance.stroke_style(),
        StrokeStyle::Dashed
    );
    assert_eq!(scene.pens[0].appearance.opacity(), 0.45);
    assert_eq!(scene.pens[0].points[0].x, 102.0);
    assert_eq!(scene.pens[0].points[0].y, 324.0);
    assert_eq!(scene.text_boxes[0].content, "Beam B-12 / revision 4");
    assert_eq!(scene.text_boxes[0].layout_rect.width, 300.0);
    assert_eq!(scene.text_boxes[0].layout_rect.height, 84.0);
    assert_eq!(scene.lengths[0].caption, "3.50 m");
    assert_eq!(scene.images[0].rect.width, 180.0);
    assert_eq!(scene.images[0].rect.height, 135.0);

    assert!(observer.events.iter().any(|event| matches!(
        event,
        EditorComparisonEvent::TextInputApplied { text, .. }
            if text == "Beam B-12 / revision 4"
    )));
    assert!(observer.events.iter().any(|event| matches!(
        event,
        EditorComparisonEvent::UploadPayloadPrepared { rgba_bytes, .. }
            if *rgba_bytes == 512 * 384 * 4
    )));
    assert!(observer.events.iter().any(|event| matches!(
        event,
        EditorComparisonEvent::SpatialIndexQueried {
            candidate_count,
            total_rectangle_count,
            ..
        } if candidate_count < total_rectangle_count && *total_rectangle_count == 101
    )));
}

#[test]
fn rejects_a_manifest_whose_editor_final_state_drifts() {
    let manifest = include_str!("../../performance/comparison-workload.json")
        .replace("Beam B-12 / revision 4", "Beam B-12 / revision 5");
    let error = EditorComparisonScenario::from_json(&manifest).unwrap_err();
    assert!(error.to_string().contains("expected state hash"));
}
