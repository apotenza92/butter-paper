use butter_paper_gpui_gallery::{
    annotation_adapter::AnnotationAdapter,
    annotation_model::{
        Annotation, LengthAnnotation, LengthCalibration, LineKind, MarkupId, PdfPoint, PdfRect,
        RectangleAnnotation, RectangleAppearance, StraightLineAnnotation, StraightLineAppearance,
    },
    semantic_snapping::{
        SemanticSnapIndex, SemanticSnapRole, SemanticSnapSettings, SemanticSnapTarget,
    },
};

fn point(x: f64, y: f64) -> PdfPoint {
    PdfPoint::new(x, y).unwrap()
}

#[test]
fn rectangle_centers_and_midpoints_use_the_same_semantic_index() {
    let source_id = MarkupId::new("snap:source-rectangle").unwrap();
    let source = RectangleAnnotation {
        id: source_id.clone(),
        page_index: 0,
        rect: PdfRect::new(40., 40., 20., 20.).unwrap(),
        rotation_degrees: 0.,
        appearance: RectangleAppearance::default(),
        locked: false,
    };
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(3, vec![Annotation::Rectangle(source)])
        .unwrap();
    let index = SemanticSnapIndex::from_annotation_scene(&adapter.document_scene(3, 0), &[]);

    let center = index
        .resolve_point(point(53., 50.), &SemanticSnapSettings::default(), 1.)
        .unwrap();
    assert_eq!(center.point, point(50., 50.));
    assert_eq!(center.owner_id.as_ref(), Some(&source_id));
    assert_eq!(center.role, SemanticSnapRole::Center);

    let edge_midpoint = index
        .resolve_point(point(50., 43.), &SemanticSnapSettings::default(), 1.)
        .unwrap();
    assert_eq!(edge_midpoint.point, point(50., 40.));
    assert_eq!(edge_midpoint.role, SemanticSnapRole::Midpoint);
}

#[test]
fn bounded_edge_pairs_add_interior_intersections_with_highest_priority() {
    let horizontal_id = MarkupId::new("snap:horizontal").unwrap();
    let vertical_id = MarkupId::new("snap:vertical").unwrap();
    let appearance = StraightLineAppearance::default_for(LineKind::Line);
    let horizontal = StraightLineAnnotation::new(
        horizontal_id,
        0,
        point(0., 50.),
        point(100., 50.),
        LineKind::Line,
        appearance.clone(),
    )
    .unwrap();
    let vertical = StraightLineAnnotation::new(
        vertical_id,
        0,
        point(50., 0.),
        point(50., 100.),
        LineKind::Line,
        appearance,
    )
    .unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(
            4,
            vec![
                Annotation::StraightLine(horizontal),
                Annotation::StraightLine(vertical),
            ],
        )
        .unwrap();
    let index = SemanticSnapIndex::from_annotation_scene(&adapter.document_scene(4, 0), &[]);

    let result = index
        .resolve_point(point(53., 52.), &SemanticSnapSettings::default(), 1.)
        .unwrap();
    assert_eq!(result.point, point(50., 50.));
    assert_eq!(result.role, SemanticSnapRole::Intersection);
    assert_eq!(result.owner_id, None);
}

#[test]
fn target_filtering_and_owner_exclusion_are_applied_at_the_shared_seam() {
    let source_id = MarkupId::new("snap:filter-source").unwrap();
    let source = StraightLineAnnotation::new(
        source_id.clone(),
        0,
        point(0., 0.),
        point(20., 0.),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(2, vec![Annotation::StraightLine(source)])
        .unwrap();
    let scene = adapter.document_scene(2, 0);
    let index = SemanticSnapIndex::from_annotation_scene(&scene, &[]);

    let midpoint = index
        .resolve_point(point(10., 3.), &SemanticSnapSettings::default(), 1.)
        .unwrap();
    assert_eq!(midpoint.point, point(10., 0.));
    assert_eq!(midpoint.role, SemanticSnapRole::Midpoint);

    let nearest_only = SemanticSnapSettings::default()
        .with_target(SemanticSnapTarget::Endpoint, false)
        .with_target(SemanticSnapTarget::Midpoint, false)
        .with_target(SemanticSnapTarget::Center, false)
        .with_target(SemanticSnapTarget::Intersection, false)
        .with_target(SemanticSnapTarget::Nearest, true);
    let nearest = index
        .resolve_point(point(10., 3.), &nearest_only, 1.)
        .unwrap();
    assert_eq!(nearest.point, point(10., 0.));
    assert_eq!(nearest.role, SemanticSnapRole::Nearest);

    let source_disabled = nearest_only.with_annotation_source(false);
    assert!(
        index
            .resolve_point(point(10., 3.), &source_disabled, 1.)
            .is_none(),
        "the application-owned source toggle must disable markup snapping without changing targets",
    );

    let excluded = SemanticSnapIndex::from_annotation_scene(&scene, &[source_id]);
    assert!(
        excluded
            .resolve_point(point(10., 3.), &nearest_only, 1.)
            .is_none(),
        "the moving owner must never snap to its own geometry",
    );
}

#[test]
fn annotation_endpoint_snaps_at_the_exact_eight_window_pixel_boundary() {
    let source_id = MarkupId::new("snap:source-line").unwrap();
    let source = StraightLineAnnotation::new(
        source_id.clone(),
        0,
        point(10., 10.),
        point(30., 10.),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(1, vec![Annotation::StraightLine(source)])
        .unwrap();
    let index = SemanticSnapIndex::from_annotation_scene(&adapter.document_scene(1, 0), &[]);
    let settings = SemanticSnapSettings::default();

    let exact = index
        .resolve_point(point(34., 10.), &settings, 2.)
        .expect("four PDF points at 2 px/pt is the inclusive eight-pixel boundary");
    assert_eq!(exact.point, point(30., 10.));
    assert_eq!(exact.owner_id.as_ref(), Some(&source_id));
    assert_eq!(exact.role, SemanticSnapRole::Endpoint);
    assert_eq!(exact.distance_window_px, 8.);

    assert!(
        index
            .resolve_point(point(34.001, 10.), &settings, 2.)
            .is_none(),
        "a point beyond the exact eight-window-pixel boundary must not snap",
    );
}

#[test]
fn retained_lengths_supply_endpoint_and_midpoint_candidates() {
    let source_id = MarkupId::new("snap:source-length").unwrap();
    let source = LengthAnnotation::new(
        source_id.clone(),
        0,
        point(10., 30.),
        point(30., 30.),
        LengthCalibration::new(1., "m", "Length", true).unwrap(),
    )
    .unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(5, vec![Annotation::Length(source)])
        .unwrap();
    let index = SemanticSnapIndex::from_annotation_scene(&adapter.document_scene(5, 0), &[]);

    let midpoint = index
        .resolve_point(point(20., 33.), &SemanticSnapSettings::default(), 1.)
        .expect("retained Length geometry is a markup snap source");
    assert_eq!(midpoint.point, point(20., 30.));
    assert_eq!(midpoint.owner_id.as_ref(), Some(&source_id));
    assert_eq!(midpoint.role, SemanticSnapRole::Midpoint);
}

#[test]
fn shift_constraint_is_applied_before_candidate_selection() {
    let off_axis = StraightLineAnnotation::new(
        MarkupId::new("snap:off-axis").unwrap(),
        0,
        point(44., 2.),
        point(64., 2.),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let on_axis_id = MarkupId::new("snap:on-axis").unwrap();
    let on_axis = StraightLineAnnotation::new(
        on_axis_id.clone(),
        0,
        point(50., 0.),
        point(70., 0.),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(
            6,
            vec![
                Annotation::StraightLine(off_axis),
                Annotation::StraightLine(on_axis),
            ],
        )
        .unwrap();
    let index = SemanticSnapIndex::from_annotation_scene(&adapter.document_scene(6, 0), &[]);

    let result = index
        .resolve_point_with_orthogonal_anchor(
            point(44., 2.),
            &SemanticSnapSettings::default(),
            1.,
            Some(point(0., 0.)),
        )
        .expect("the eligible on-axis endpoint remains within eight window pixels");
    assert_eq!(result.point, point(50., 0.));
    assert_eq!(result.owner_id.as_ref(), Some(&on_axis_id));
    assert_eq!(result.role, SemanticSnapRole::Endpoint);
}

#[test]
fn snapped_line_and_length_persistence_comparison_accepts_pdf_number_rounding_only() {
    let expected_line = StraightLineAnnotation::new(
        MarkupId::new("snap:persisted-line").unwrap(),
        0,
        point(99.999_986, 100.000_004),
        point(149.999_979, 199.999_99),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let rounded_line = StraightLineAnnotation::new(
        expected_line.id.clone(),
        0,
        point(99.999_99, 100.),
        point(149.999_98, 199.999_99),
        LineKind::Line,
        expected_line.appearance.clone(),
    )
    .unwrap();
    assert!(expected_line.same_persisted_state_as(&rounded_line));
    let materially_moved_line = StraightLineAnnotation::new(
        expected_line.id.clone(),
        0,
        point(99.998, 100.),
        expected_line.end,
        LineKind::Line,
        expected_line.appearance.clone(),
    )
    .unwrap();
    assert!(!expected_line.same_persisted_state_as(&materially_moved_line));

    let calibration = LengthCalibration::from_scale(72., 1., "m", 2, true).unwrap();
    let expected_length = LengthAnnotation::new(
        MarkupId::new("snap:persisted-length").unwrap(),
        0,
        point(124.999_982, 149.999_997),
        expected_line.end,
        calibration.clone(),
    )
    .unwrap();
    let rounded_length = LengthAnnotation::new(
        expected_length.id.clone(),
        0,
        point(124.999_98, 150.),
        rounded_line.end,
        LengthCalibration::from_scale(
            1.,
            calibration.units_per_point(),
            calibration.unit(),
            calibration.precision(),
            calibration.show_caption(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(expected_length.same_persisted_state_as(&rounded_length));
}
