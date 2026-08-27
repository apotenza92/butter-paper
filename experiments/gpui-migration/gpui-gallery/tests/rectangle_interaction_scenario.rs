use butter_paper_gpui_gallery::{
    annotation_adapter::AnnotationAdapter,
    annotation_model::StrokeStyle,
    rectangle_interaction_scenario::{
        NativeRectangleTransformObservation, RectangleInteractionScenario,
    },
};

#[test]
fn exposes_the_decision_3_native_edge_move_and_east_resize_plan() {
    let plan = RectangleInteractionScenario::embedded()
        .unwrap()
        .native_transform_plan()
        .unwrap();

    assert_eq!(plan.annotation_id, "comparison:rectangle:sparse:1");
    assert_eq!(plan.create_start, [72.0, 144.0]);
    assert_eq!(plan.create_finish, [252.0, 240.0]);
    assert_eq!(plan.create_sample_count, 361);
    assert_eq!(plan.select_point, [117.0, 240.0]);
    assert_eq!(plan.move_delta, [18.0, -12.0]);
    assert_eq!(plan.move_sample_count, 361);
    assert_eq!(plan.resize_handle, "east");
    assert_eq!(plan.resize_delta, [30.0, 0.0]);
    assert_eq!(plan.resize_sample_count, 361);
    assert_eq!(plan.expected_final_rect, [90.0, 132.0, 210.0, 96.0]);
    assert_eq!(plan.maximum_geometry_error_device_px, 1.0);
}

#[test]
fn assesses_native_transform_history_geometry_and_platform_draw_fail_closed() {
    let plan = RectangleInteractionScenario::embedded()
        .unwrap()
        .native_transform_plan()
        .unwrap();
    let evidence = plan
        .assess_native_observation(NativeRectangleTransformObservation {
            hit_test_selected: true,
            create_history_delta: 1,
            move_history_delta: 1,
            resize_history_delta: 1,
            observed_final_rect: [90.5, 132.0, 210.0, 96.0],
            pixels_per_point: 2.0,
            gpui_platform_draw_submitted: true,
        })
        .unwrap();
    assert_eq!(evidence.maximum_geometry_error_device_px, 1.0);
    assert_eq!(evidence.geometry_tolerance_device_px, 1.0);
    assert_eq!(evidence.select_semantics, "no-fill-edge-or-stroked-body");

    let error = plan
        .assess_native_observation(NativeRectangleTransformObservation {
            move_history_delta: 2,
            ..evidence.observation
        })
        .unwrap_err();
    assert!(error.to_string().contains("one history transaction"));
}

#[test]
fn executes_exact_rectangle_transform_properties_lock_and_history_commands() {
    let mut adapter = AnnotationAdapter::default();
    let report = RectangleInteractionScenario::embedded()
        .unwrap()
        .execute(71, &mut adapter)
        .unwrap();

    assert_eq!(report.final_rect, [90.0, 132.0, 210.0, 96.0]);
    assert_eq!(report.command_evidence.len(), 2);
    assert_eq!(
        report.command_evidence[0].command_id,
        "rectangle:select-move-resize"
    );
    assert_eq!(
        report.command_evidence[0].proven_milestones,
        [
            "hit-test-selected",
            "move-committed-once",
            "resize-committed-once"
        ]
    );
    assert_eq!(
        report.command_evidence[1].proven_milestones,
        [
            "properties-current",
            "locked-edit-rejected",
            "undo-redo-exact",
            "dirty-current"
        ]
    );
    assert_eq!(report.history_depths.1, 0);
    assert!(report.dirty);

    let rectangle = &adapter.document_scene(71, 0).rectangles[0];
    assert_eq!(rectangle.appearance.stroke_color(), "#dc2626");
    assert_eq!(rectangle.appearance.fill_color(), Some("#dc2626"));
    assert_eq!(rectangle.appearance.stroke_width_pt(), 3.0);
    assert_eq!(rectangle.appearance.stroke_style(), StrokeStyle::Dashed);
    assert!((rectangle.appearance.opacity() - 0.88).abs() < f64::EPSILON);
    assert!((rectangle.appearance.fill_opacity() - 31.0 / 255.0).abs() < 0.000_001);
    assert!(!rectangle.locked);
}

#[test]
fn executes_the_transform_without_unrelated_editor_commands() {
    let mut adapter = AnnotationAdapter::default();
    let report = RectangleInteractionScenario::embedded()
        .unwrap()
        .execute_transform(73, &mut adapter)
        .unwrap();
    assert_eq!(report.final_rect, [90.0, 132.0, 210.0, 96.0]);
    assert_eq!(report.command_evidence.len(), 1);
    assert_eq!(report.history_depths, (3, 0));
    let scene = adapter.document_scene(73, 0);
    assert_eq!(scene.rectangles.len(), 1);
    assert!(scene.pens.is_empty());
    assert!(scene.text_boxes.is_empty());
    assert!(scene.lengths.is_empty());
    assert!(scene.images.is_empty());
}

#[test]
fn executes_only_the_properties_history_evidence_after_exact_prerequisites() {
    let mut adapter = AnnotationAdapter::default();
    let report = RectangleInteractionScenario::embedded()
        .unwrap()
        .execute_properties_history(74, &mut adapter)
        .unwrap();

    assert_eq!(report.command_evidence.len(), 1);
    assert_eq!(
        report.command_evidence[0].command_id,
        "rectangle:properties-history"
    );
    assert_eq!(
        report.command_evidence[0].proven_milestones,
        [
            "properties-current",
            "locked-edit-rejected",
            "undo-redo-exact",
            "dirty-current"
        ]
    );
    assert!(report.dirty);
    assert_eq!(report.history_depths.1, 0);
}

#[test]
fn rejects_manifest_interaction_drift_before_replay() {
    let manifest = include_str!("../../performance/comparison-workload.json")
        .replace("\"handle\": \"east\"", "\"handle\": \"west\"");
    let mut adapter = AnnotationAdapter::default();
    let error = RectangleInteractionScenario::from_json(&manifest)
        .unwrap()
        .execute(72, &mut adapter)
        .unwrap_err();
    assert!(error.to_string().contains("handle must be east"));
}
