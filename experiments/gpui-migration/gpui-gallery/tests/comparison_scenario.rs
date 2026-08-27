use butter_paper_gpui_gallery::comparison_scenario::{
    ComparisonScenarioKind, ComparisonScenarioPlan, MilestoneGate, compare_highlight_geometry,
};

#[test]
fn plans_the_exact_manifest_backed_annotation_create_subset() {
    let plan = ComparisonScenarioPlan::embedded(ComparisonScenarioKind::AnnotationCreate)
        .expect("the checked-in comparison workload should parse");

    assert_eq!(plan.manifest_id(), "bp-perf-v3-decision-3");
    let annotation = plan
        .annotation_create()
        .expect("annotation-create should carry annotation commands");
    assert_eq!(annotation.rectangle.command_id, "rectangle:create-sparse");
    assert_eq!(
        annotation.rectangle.annotation_id,
        "comparison:rectangle:sparse:1"
    );
    assert_eq!(annotation.rectangle.sample_count, 361);
    assert_eq!(annotation.rectangle.start, [72.0, 144.0]);
    assert_eq!(annotation.rectangle.finish, [252.0, 240.0]);
    assert_eq!(annotation.highlight.command_id, "highlight:create");
    assert_eq!(annotation.highlight.annotation_id, "comparison:highlight:1");
    assert_eq!(annotation.highlight.sample_count, 361);
    assert_eq!(annotation.highlight.control_points.len(), 4);
    assert_eq!(annotation.highlight.control_points[0], [90.0, 330.0]);
    assert_eq!(annotation.highlight.control_points[3], [300.0, 334.0]);
}

#[test]
fn plans_the_exact_manifest_backed_continuous_scroll_subset() {
    let plan = ComparisonScenarioPlan::embedded(ComparisonScenarioKind::ContinuousScroll)
        .expect("the checked-in comparison workload should parse");
    let scroll = plan
        .continuous_scroll()
        .expect("continuous-scroll should carry its path");

    assert_eq!(scroll.command_id, "viewer:continuous-scroll");
    assert_eq!(scroll.input_rate_hz, 120);
    assert_eq!(scroll.forward_duration_ms, 20_000);
    assert_eq!(scroll.forward_viewport_heights, 50.0);
    assert_eq!(scroll.pause_duration_ms, 2_000);
    assert_eq!(scroll.reverse_duration_ms, 10_000);
    assert_eq!(scroll.finish_page, 1);
}

#[test]
fn plans_only_the_exact_shared_rectangle_transform_command() {
    let plan = ComparisonScenarioPlan::embedded(ComparisonScenarioKind::AnnotationTransform)
        .expect("the checked-in rectangle transform should parse");
    let commands = plan.annotation_transform().unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].command_id, "rectangle:select-move-resize");
    assert_eq!(
        commands[0].expected_milestones,
        [
            "hit-test-selected",
            "move-committed-once",
            "resize-committed-once"
        ]
    );
}

#[test]
fn plans_only_the_exact_gpui_rectangle_properties_history_command() {
    let plan =
        ComparisonScenarioPlan::embedded(ComparisonScenarioKind::AnnotationPropertiesHistory)
            .expect("the checked-in rectangle properties/history command should parse");
    let commands = plan.annotation_properties_history().unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].command_id, "rectangle:properties-history");
    assert_eq!(
        commands[0].expected_milestones,
        [
            "properties-current",
            "locked-edit-rejected",
            "undo-redo-exact",
            "dirty-current"
        ]
    );
}

#[test]
fn milestone_gate_requires_every_declared_manifest_milestone() {
    let plan = ComparisonScenarioPlan::embedded(ComparisonScenarioKind::AnnotationCreate)
        .expect("the checked-in comparison workload should parse");
    let annotation = plan.annotation_create().unwrap();
    let mut gate = MilestoneGate::new(&plan);

    for milestone in &annotation.rectangle.expected_milestones {
        gate.record(&annotation.rectangle.command_id, milestone)
            .expect("manifest milestone should be accepted");
    }
    assert!(!gate.is_complete());
    assert_eq!(
        gate.missing().len(),
        annotation.highlight.expected_milestones.len()
    );

    for milestone in &annotation.highlight.expected_milestones {
        gate.record(&annotation.highlight.command_id, milestone)
            .expect("manifest milestone should be accepted");
    }
    assert!(gate.is_complete());
    assert!(gate.missing().is_empty());
}

#[test]
fn milestone_gate_rejects_fabricated_or_cross_command_milestones() {
    let plan = ComparisonScenarioPlan::embedded(ComparisonScenarioKind::AnnotationCreate)
        .expect("the checked-in comparison workload should parse");
    let mut gate = MilestoneGate::new(&plan);

    let error = gate
        .record("rectangle:create-sparse", "path-smoothed")
        .expect_err("a highlight milestone must not satisfy the rectangle command");
    assert!(error.to_string().contains("not declared"));

    let error = gate
        .record("rectangle:create-sparse", "made-up-ready")
        .expect_err("an invented milestone must not be accepted");
    assert!(error.to_string().contains("not declared"));
}

#[test]
fn scenario_names_are_stable_runner_contracts() {
    assert_eq!(
        ComparisonScenarioKind::AnnotationCreate.as_str(),
        "annotation-create"
    );
    assert_eq!(
        ComparisonScenarioKind::ContinuousScroll.as_str(),
        "continuous-scroll"
    );
    assert_eq!(
        ComparisonScenarioKind::AnnotationTransform.as_str(),
        "annotation-transform"
    );
    assert_eq!(
        ComparisonScenarioKind::AnnotationPropertiesHistory.as_str(),
        "annotation-properties-history"
    );
    assert_eq!(
        "annotation-create"
            .parse::<ComparisonScenarioKind>()
            .unwrap(),
        ComparisonScenarioKind::AnnotationCreate,
    );
    assert!("annotation".parse::<ComparisonScenarioKind>().is_err());
}

#[test]
fn highlight_geometry_uses_the_frozen_native_pixel_tolerance() {
    let expected = (0..361)
        .map(|index| [f64::from(index), 0.0])
        .collect::<Vec<_>>();
    let within_tolerance = (0..100)
        .map(|index| [f64::from(index) * 360.0 / 99.0, 1.0])
        .collect::<Vec<_>>();
    let outside_tolerance = within_tolerance
        .iter()
        .map(|point| [point[0], 3.0])
        .collect::<Vec<_>>();

    let matched = compare_highlight_geometry(&expected, &within_tolerance, 1.0)
        .expect("valid paths should compare");
    assert!(matched.matched);
    assert_eq!(matched.canonical_resample_count, 64);
    assert_eq!(matched.maximum_centerline_deviation_pdf_points, 1.0);
    assert_eq!(matched.smoothing_tolerance_pdf_points, 2.0);
    assert_eq!(
        matched.coordinate_quantization_allowance_pdf_points,
        std::f64::consts::FRAC_1_SQRT_2
    );
    assert_eq!(
        matched.tolerance_pdf_points,
        2.0 + std::f64::consts::FRAC_1_SQRT_2
    );
    assert_eq!(
        matched.canonicalization,
        "arc-length 64-point centerline; two native pixels after maintained smoothing plus one half-pixel-per-axis XTEST quantization diagonal"
    );

    let at_quantized_boundary = within_tolerance
        .iter()
        .map(|point| [point[0], 2.7])
        .collect::<Vec<_>>();
    assert!(
        compare_highlight_geometry(&expected, &at_quantized_boundary, 1.0)
            .unwrap()
            .matched
    );

    let mismatched = compare_highlight_geometry(&expected, &outside_tolerance, 1.0)
        .expect("valid paths should compare");
    assert!(!mismatched.matched);
    assert_eq!(mismatched.maximum_centerline_deviation_pdf_points, 3.0);
}

#[test]
fn highlight_geometry_fails_closed_without_a_verifiable_surface_or_path() {
    let expected = [[0.0, 0.0], [1.0, 1.0]];
    assert!(compare_highlight_geometry(&expected, &expected, 0.0).is_err());
    assert!(compare_highlight_geometry(&expected, &[[0.0, 0.0]], 1.0).is_err());
}
