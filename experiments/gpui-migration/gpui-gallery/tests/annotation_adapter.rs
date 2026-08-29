use butter_paper_gpui_gallery::{
    annotation_adapter::{
        AnnotationAdapter, AnnotationTool, ELLIPSE_ROTATION_HANDLE_ID, HighlightPaintCapability,
        LENGTH_SCALE_REQUIRED_MESSAGE, PointerInputModifiers, PointerPhaseOutcome,
        RectangleSnapSettings, StraightLinePropertyEdit, VertexPathPropertyEdit, ellipse_resize_handle_id,
        ellipse_resize_handle_point, ellipse_rotation_handle_point, redact_resize_handle_id,
        redact_resize_handle_point, snapshot_resize_handle_id, snapshot_resize_handle_point,
    },
    annotation_model::{
        Annotation, AnnotationCommand, AnnotationDocument, AnnotationEdit, AnnotationKind,
        ArcAnnotation, ArcControlPoint, BlendMode, CloudAnnotation, DecodedRgbaAsset, DimensionAnnotation,
        EllipseAnnotation, HitTarget,
        ImageAnnotation, InkTool, LengthAnnotation, LengthCalibration, LineEndpoint, LineKind,
        MarkupId, MeasurementPathAnnotation, MeasurementPathKind, PageRotation, PageScale,
        PageTransform, PdfPoint, PdfRect, PenAnnotation, PenAppearance, PointerCancelReason,
        RectangleAnnotation, RectangleAppearance, RectangleResizeHandle, RedactAnnotation,
        ScalePrecision, ScaleSource, ScaleUnit, SnapshotAnnotation, StraightLineAnnotation,
        StraightLineAppearance, StrokeStyle, TextAlignment, TextBoxAnnotation, TextBoxStyle,
        VertexPathAnnotation, VertexPathKind,
    },
    native_editing_v5::NativeEditingV5Plan,
    selection_geometry::SelectionPoint,
    semantic_snapping::{SemanticSnapRole, SemanticSnapSettings},
};

fn point(x: f64, y: f64) -> PdfPoint {
    PdfPoint::new(x, y).unwrap()
}

#[test]
fn exact_engineering_visual_edits_are_atomic_history_safe_and_selection_bound() {
    let mut adapter = AnnotationAdapter::default();
    let arc_id = MarkupId::new("arc:visual").unwrap();
    let cloud_id = MarkupId::new("cloud:visual").unwrap();
    let snapshot_id = MarkupId::new("snapshot:visual").unwrap();
    let arc = ArcAnnotation::new(
        arc_id.clone(),
        0,
        point(10., 10.),
        point(110., 10.),
        point(60., 60.),
        RectangleAppearance::default().with_stroke_style(StrokeStyle::Dashed),
    )
    .unwrap();
    let cloud = CloudAnnotation::new(
        cloud_id.clone(),
        0,
        vec![point(140., 20.), point(220., 20.), point(180., 90.)],
        2.,
        RectangleAppearance::default().with_stroke_style(StrokeStyle::Dotted),
    )
    .unwrap();
    let snapshot_asset = DecodedRgbaAsset::new(2, 2, vec![0x80; 16]).unwrap();
    let snapshot = SnapshotAnnotation::new(
        snapshot_id.clone(),
        0,
        PdfRect::new(250., 20., 80., 60.).unwrap(),
        snapshot_asset.clone(),
        1.,
    )
    .unwrap()
    .with_rotation_degrees(33.)
    .unwrap();
    adapter
        .load_imported_annotations(
            122,
            vec![Annotation::Arc(arc), Annotation::Cloud(cloud), Annotation::Snapshot(snapshot)],
        )
        .unwrap();

    assert!(adapter.select_id(122, &arc_id));
    let arc_before = adapter.exact_selected_arc(122).unwrap().clone();
    let arc_appearance = RectangleAppearance::new("#2563eb", 4., None::<String>, 0.5)
        .unwrap()
        .with_stroke_style(arc_before.appearance.stroke_style());
    adapter
        .set_exact_selected_arc_appearance(122, arc_appearance.clone())
        .unwrap();
    assert_eq!(adapter.history_depths(122), (1, 0));
    let arc_after = adapter.exact_selected_arc(122).unwrap();
    assert_eq!(arc_after.appearance, arc_appearance);
    assert_eq!((arc_after.start, arc_after.mid, arc_after.end), (arc_before.start, arc_before.mid, arc_before.end));
    adapter
        .set_exact_selected_arc_appearance(122, arc_after.appearance.clone())
        .unwrap();
    assert_eq!(adapter.history_depths(122), (1, 0), "exact no-op must not add history");

    assert!(adapter.select_id(122, &cloud_id));
    let cloud_before = adapter.exact_selected_cloud(122).unwrap().clone();
    let cloud_appearance = RectangleAppearance::new("#16a34a", 3., None::<String>, 0.65)
        .unwrap()
        .with_stroke_style(cloud_before.appearance.stroke_style());
    adapter
        .set_exact_selected_cloud_appearance(122, cloud_appearance.clone())
        .unwrap();
    adapter.set_exact_selected_cloud_intensity(122, 3.5).unwrap();
    let cloud_after = adapter.exact_selected_cloud(122).unwrap();
    assert_eq!(cloud_after.appearance, cloud_appearance);
    assert_eq!(cloud_after.border_effect_intensity(), 3.5);
    assert_eq!(cloud_after.points(), cloud_before.points());
    assert!(adapter.set_exact_selected_cloud_intensity(122, 4.01).is_err());
    assert_eq!(adapter.history_depths(122), (3, 0));

    assert!(adapter.select_id(122, &snapshot_id));
    adapter.set_exact_selected_snapshot_opacity(122, 0.4).unwrap();
    let snapshot_after = adapter.exact_selected_snapshot(122).unwrap();
    assert_eq!(snapshot_after.opacity(), 0.4);
    assert_eq!(snapshot_after.asset(), &snapshot_asset);
    assert_eq!(snapshot_after.rotation_degrees(), 33.);
    assert_eq!(snapshot_after.rect, PdfRect::new(250., 20., 80., 60.).unwrap());
    assert_eq!(adapter.history_depths(122), (4, 0));

    adapter.undo(122).unwrap();
    assert_eq!(adapter.exact_selected_snapshot(122).unwrap().opacity(), 1.);
    adapter.redo(122).unwrap();
    assert_eq!(adapter.exact_selected_snapshot(122).unwrap().opacity(), 0.4);

    adapter.set_selected_locked(122, true).unwrap();
    assert!(adapter.set_exact_selected_snapshot_opacity(122, 0.2).is_err());
    adapter.set_selected_locked(122, false).unwrap();
    adapter.select_all_on_page(122, 0);
    assert!(adapter.exact_selected_arc(122).is_none());
    assert!(adapter.exact_selected_cloud(122).is_none());
    assert!(adapter.exact_selected_snapshot(122).is_none());
    assert!(adapter.set_exact_selected_cloud_intensity(122, 1.).is_err());
}

#[test]
fn snapshot_tool_uses_exact_two_click_capture_contract_and_stable_identity() {
    assert_eq!(AnnotationTool::Snapshot.label(), "Snapshot");
    assert_eq!(AnnotationTool::Snapshot.shortcut(), Some("G"));
    assert_eq!(AnnotationTool::Snapshot.tooltip_label(), "Snapshot (G)");
    assert_eq!(AnnotationTool::Snapshot.toolbar_id(), "tool-snapshot");
    assert_eq!(
        AnnotationTool::from_plain_shortcut("g"),
        Some(AnnotationTool::Snapshot)
    );
    assert_eq!(
        AnnotationTool::from_toolbar_id("tool-snapshot"),
        Some(AnnotationTool::Snapshot)
    );

    let mut adapter = AnnotationAdapter::default();
    adapter
        .set_rectangle_snap_settings(RectangleSnapSettings::new(true, 18., 50.).unwrap())
        .unwrap();
    let id = MarkupId::new("snapshot:adapter:two-click").unwrap();
    adapter.queue_next_annotation_id(id.clone());
    adapter.set_tool(AnnotationTool::Snapshot).unwrap();
    assert_eq!(
        adapter
            .pointer_down(120, 0, 17, point(10., 20.), 2.)
            .unwrap(),
        PointerPhaseOutcome::PlacementPending,
    );
    assert!(adapter.snapshot_placement_pending(120));
    assert!(adapter.snapshot(120).unwrap().snapshots.is_empty());

    assert_eq!(
        adapter.pointer_move(17, point(90., 70.)).unwrap(),
        PointerPhaseOutcome::PlacementPending,
    );
    assert_eq!(
        adapter.snapshot_pending_rect(120, 0),
        Some(PdfRect::new(10., 20., 80., 50.).unwrap()),
    );
    assert_eq!(
        adapter.snapshot_pending_rect_to(120, 0, point(100., 80.)),
        Some(PdfRect::new(10., 20., 90., 60.).unwrap()),
        "the caller can synchronously crop the exact pending second-click rectangle",
    );
    let uncaptured_preview = adapter.document_scene(120, 0);
    assert_eq!(uncaptured_preview.snapshots.len(), 1);
    assert_eq!(uncaptured_preview.snapshots[0].body_id, "snapshot.body");
    assert!(uncaptured_preview.snapshots[0].draft);
    assert_eq!(
        (
            uncaptured_preview.snapshots[0].width_px,
            uncaptured_preview.snapshots[0].height_px
        ),
        (1, 1),
        "the pre-capture preview uses only a transparent scene marker, never a retained asset",
    );
    assert_eq!(
        adapter.pointer_up(17, point(90., 70.)).unwrap(),
        PointerPhaseOutcome::PlacementPending,
        "dragging and releasing may preview but must never create a Snapshot",
    );

    let missing_capture = adapter
        .pointer_down(120, 0, 17, point(90., 70.), 2.)
        .unwrap_err();
    assert!(
        missing_capture
            .to_string()
            .contains("requires a synchronous decoded page capture")
    );
    assert!(adapter.snapshot_placement_pending(120));

    let capture = DecodedRgbaAsset::new(8, 5, vec![0x60; 8 * 5 * 4]).unwrap();
    let capture_id = capture.id().clone();
    adapter.set_snapshot_capture_asset(capture);
    let preview = adapter.document_scene(120, 0);
    assert_eq!(preview.snapshots.len(), 1);
    assert_eq!(preview.snapshots[0].id, id);
    assert_eq!(preview.snapshots[0].body_id, "snapshot.body");
    assert!(preview.snapshots[0].draft);
    assert_eq!(preview.snapshots[0].asset_id, capture_id);

    assert_eq!(
        adapter
            .pointer_down(120, 0, 17, point(90., 70.), 2.)
            .unwrap(),
        PointerPhaseOutcome::AnnotationCreated(id.clone()),
    );
    assert_eq!(adapter.tool(), AnnotationTool::Select);
    assert!(!adapter.snapshot_placement_pending(120));
    let snapshot = adapter.snapshot(120).unwrap();
    assert_eq!(snapshot.snapshots.len(), 1);
    assert_eq!(snapshot.snapshots[0].id, id);
    assert_eq!(
        snapshot.snapshots[0].rect,
        PdfRect::new(10., 20., 80., 50.).unwrap()
    );
    assert_eq!(snapshot.snapshots[0].asset().id(), &capture_id);
    assert_eq!(adapter.history_depths(120), (1, 0));

    adapter.set_tool(AnnotationTool::Snapshot).unwrap();
    adapter.pointer_down(120, 0, 18, point(0., 0.), 2.).unwrap();
    adapter.set_snapshot_capture_asset(DecodedRgbaAsset::new(2, 2, vec![0x30; 2 * 2 * 4]).unwrap());
    assert_eq!(
        adapter.pointer_down(120, 0, 18, point(2., 3.), 2.).unwrap(),
        PointerPhaseOutcome::Ignored,
        "Snapshot width must be strictly greater than two PDF points",
    );
    assert!(adapter.snapshot_placement_pending(120));
}

#[test]
fn snapshot_selection_moves_resizes_rotates_locks_and_cancels_without_image_identity() {
    let mut adapter = AnnotationAdapter::default();
    let id = MarkupId::new("snapshot:adapter:editing").unwrap();
    let capture = DecodedRgbaAsset::new(12, 8, vec![0xa0; 12 * 8 * 4]).unwrap();
    let annotation = SnapshotAnnotation::new(
        id.clone(),
        0,
        PdfRect::new(20., 30., 120., 80.).unwrap(),
        capture,
        0.8,
    )
    .unwrap();
    adapter
        .load_imported_annotations(121, vec![Annotation::Snapshot(annotation)])
        .unwrap();
    assert!(adapter.select_id(121, &id));
    assert!(adapter.snapshot(121).unwrap().images.is_empty());

    let center = point(80., 70.);
    adapter.pointer_down(121, 0, 1, center, 3.).unwrap();
    adapter.pointer_move(1, point(90., 75.)).unwrap();
    assert_eq!(
        adapter.document_scene(121, 0).snapshots[0].rect,
        PdfRect::new(30., 35., 120., 80.).unwrap(),
    );
    assert_eq!(
        adapter.pointer_up(1, point(90., 75.)).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(id.clone()),
    );

    let moved = adapter.snapshot(121).unwrap().snapshots[0].clone();
    assert_eq!(
        RectangleResizeHandle::ALL.map(snapshot_resize_handle_id),
        [
            "snapshot.resize.nw",
            "snapshot.resize.n",
            "snapshot.resize.ne",
            "snapshot.resize.e",
            "snapshot.resize.se",
            "snapshot.resize.s",
            "snapshot.resize.sw",
            "snapshot.resize.w",
        ],
        "all eight Snapshot handles keep identities separate from Image",
    );
    assert_eq!(
        snapshot_resize_handle_id(RectangleResizeHandle::East),
        "snapshot.resize.e"
    );
    let east = snapshot_resize_handle_point(&moved, RectangleResizeHandle::East);
    adapter.pointer_down(121, 0, 2, east, 3.).unwrap();
    adapter
        .pointer_move(2, point(east.x + 20., east.y))
        .unwrap();
    assert_eq!(
        adapter.pointer_up(2, point(east.x + 20., east.y)).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(id.clone()),
    );
    assert_eq!(adapter.snapshot(121).unwrap().snapshots[0].rect.width, 140.);

    adapter.set_selected_snapshot_rotation(121, 31.).unwrap();
    adapter.set_selected_snapshot_opacity(121, 0.45).unwrap();
    assert_eq!(
        adapter.snapshot(121).unwrap().snapshots[0].rotation_degrees(),
        31.
    );
    assert_eq!(adapter.snapshot(121).unwrap().snapshots[0].opacity(), 0.45);
    adapter.set_selected_snapshot_rotation(121, 0.).unwrap();
    assert_eq!(
        adapter.snapshot(121).unwrap().snapshots[0].rotation_degrees(),
        0.
    );

    let history_before_lock = adapter.history_depths(121).0;
    adapter.set_selected_locked(121, true).unwrap();
    assert_eq!(adapter.history_depths(121).0, history_before_lock + 1);
    assert_eq!(
        adapter
            .pointer_down(121, 0, 3, point(80., 70.), 3.)
            .unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(id.clone())),
    );
    adapter.undo(121).unwrap();
    assert!(!adapter.snapshot(121).unwrap().snapshots[0].locked);

    adapter.set_tool(AnnotationTool::Snapshot).unwrap();
    adapter
        .pointer_down(121, 0, 4, point(200., 200.), 3.)
        .unwrap();
    adapter.set_tool(AnnotationTool::Rectangle).unwrap();
    assert!(!adapter.snapshot_placement_pending(121));
    assert_eq!(adapter.snapshot(121).unwrap().snapshots.len(), 1);
}

#[test]
fn redact_tool_uses_pending_only_click_or_drag_geometry_and_stable_parts() {
    assert_eq!(AnnotationTool::Redact.label(), "Redact");
    assert_eq!(AnnotationTool::Redact.shortcut(), None);
    assert_eq!(AnnotationTool::Redact.tooltip_label(), "Redact");
    assert_eq!(AnnotationTool::Redact.toolbar_id(), "tool-redact");
    assert_eq!(
        AnnotationTool::from_toolbar_id("tool-redact"),
        Some(AnnotationTool::Redact)
    );
    assert_eq!(
        AnnotationTool::from_plain_shortcut("r"),
        Some(AnnotationTool::Rectangle)
    );

    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Redact).unwrap();
    adapter.queue_next_annotation_id(MarkupId::new("redact:adapter:ignored").unwrap());
    assert_eq!(
        adapter
            .pointer_down_with_input(94, 0, 1, 1, point(10., 10.), 2., false)
            .unwrap(),
        PointerPhaseOutcome::Ignored,
        "only the primary pointer button may start a pending redaction mark",
    );

    adapter.queue_next_annotation_id(MarkupId::new("redact:adapter:below-threshold").unwrap());
    assert_eq!(
        adapter
            .pointer_down_with_viewport_input(
                94,
                0,
                2,
                0,
                point(10., 10.),
                selection_point(10., 10.),
                2.,
                PointerInputModifiers::default(),
            )
            .unwrap(),
        PointerPhaseOutcome::GestureStarted,
    );
    assert_eq!(
        adapter
            .pointer_up_with_viewport_input(
                2,
                point(40., 30.),
                selection_point(12.999, 10.),
                PointerInputModifiers::default(),
            )
            .unwrap(),
        PointerPhaseOutcome::PlacementPending,
        "movement below three CSS pixels must retain the click-placement draft",
    );
    assert!(adapter.is_click_placement_pending());
    adapter.cancel(PointerCancelReason::ToolChanged).unwrap();

    let id = MarkupId::new("redact:adapter:pending-1").unwrap();
    adapter.set_tool(AnnotationTool::Redact).unwrap();
    adapter.queue_next_annotation_id(id.clone());
    adapter
        .pointer_down_with_viewport_input(
            94,
            0,
            3,
            0,
            point(10., 10.),
            selection_point(10., 10.),
            2.,
            PointerInputModifiers::default(),
        )
        .unwrap();
    assert_eq!(
        adapter
            .pointer_up_with_viewport_input(
                3,
                point(40., 30.),
                selection_point(13., 10.),
                PointerInputModifiers::default(),
            )
            .unwrap(),
        PointerPhaseOutcome::AnnotationCreated(id.clone()),
        "movement at exactly three CSS pixels must commit when both PDF dimensions exceed two points",
    );
    let snapshot = adapter.snapshot(94).unwrap();
    assert_eq!(snapshot.redacts.len(), 1);
    let redact: &RedactAnnotation = &snapshot.redacts[0];
    assert_eq!(redact.id, id);
    assert_eq!(redact.rect, PdfRect::new(10., 10., 30., 20.).unwrap());
    assert_eq!(redact.redaction_color(), "#000000");
    assert_eq!(
        redact_resize_handle_id(RectangleResizeHandle::NorthWest),
        "redact.resize.nw"
    );
    assert_eq!(
        redact_resize_handle_id(RectangleResizeHandle::SouthEast),
        "redact.resize.se"
    );
    assert_eq!(
        redact_resize_handle_point(redact, RectangleResizeHandle::NorthEast),
        point(40., 30.),
    );
    let scene = adapter.document_scene(94, 0);
    assert_eq!(scene.redacts.len(), 1);
    assert_eq!(scene.redacts[0].body_id, "redact.body");
    assert!(!scene.redacts[0].draft);
    assert_eq!(scene.redacts[0].appearance.stroke_color(), "#ff0000");
    assert_eq!(scene.redacts[0].appearance.fill_color(), Some("#000000"));
    assert_eq!(scene.redacts[0].appearance.opacity(), 0.35);
    assert_eq!(adapter.history_depths(94), (1, 0));
}

fn selection_point(x: f64, y: f64) -> SelectionPoint {
    SelectionPoint::new(x, y)
}

#[test]
fn ellipse_tool_uses_exact_three_pixel_drag_threshold_and_shift_circle_constraint() {
    assert_eq!(AnnotationTool::Ellipse.label(), "Ellipse");
    assert_eq!(AnnotationTool::Ellipse.shortcut(), Some("E"));
    assert_eq!(
        AnnotationTool::from_plain_shortcut("e"),
        Some(AnnotationTool::Ellipse)
    );
    assert_eq!(AnnotationTool::Ellipse.toolbar_id(), "draw-ellipse");

    let mut adapter = AnnotationAdapter::default();
    let id = MarkupId::new("ellipse:adapter:shift-circle").unwrap();
    adapter.set_tool(AnnotationTool::Ellipse).unwrap();
    adapter.queue_next_annotation_id(id.clone());
    assert_eq!(
        adapter
            .pointer_down_with_viewport_input(
                91,
                0,
                7,
                0,
                point(10., 10.),
                selection_point(10., 10.),
                2.,
                PointerInputModifiers::default(),
            )
            .unwrap(),
        PointerPhaseOutcome::GestureStarted,
    );
    assert_eq!(
        adapter
            .pointer_up_with_viewport_input(
                7,
                point(13., 10.),
                selection_point(13., 10.),
                PointerInputModifiers {
                    shift: true,
                    alt: false,
                },
            )
            .unwrap(),
        PointerPhaseOutcome::AnnotationCreated(id.clone()),
        "the Electron contract activates ellipse drag at exactly three viewport pixels",
    );
    let snapshot = adapter.snapshot(91).unwrap();
    assert_eq!(snapshot.revision, 1);
    assert_eq!(snapshot.ellipses.len(), 1);
    assert_eq!(snapshot.ellipses[0].id, id);
    assert_eq!(
        snapshot.ellipses[0].rect,
        PdfRect::new(10., 10., 3., 3.).unwrap()
    );
    assert_eq!(adapter.tool(), AnnotationTool::Select);
}

#[test]
fn ellipse_pointer_edit_journey_moves_resizes_rotates_and_resets_through_stable_handles() {
    let id = MarkupId::new("ellipse:adapter:pointer-edit").unwrap();
    let appearance = RectangleAppearance::new("#2563eb", 2., Some("#dbeafe"), 0.8).unwrap();
    let ellipse = EllipseAnnotation::new(
        id.clone(),
        0,
        PdfRect::new(10., 10., 100., 50.).unwrap(),
        appearance,
    )
    .unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(92, vec![Annotation::Ellipse(ellipse)])
        .unwrap();
    assert!(adapter.select_id(92, &id));

    assert_eq!(
        ellipse_resize_handle_id(RectangleResizeHandle::NorthWest),
        "ellipse.resize.nw"
    );
    assert_eq!(ELLIPSE_ROTATION_HANDLE_ID, "ellipse.rotate");
    let selected = &adapter.snapshot(92).unwrap().ellipses[0];
    let north_west = ellipse_resize_handle_point(selected, RectangleResizeHandle::NorthWest);
    assert!((north_west.x - 24.644_660_94).abs() < 1e-6);
    assert!((north_west.y - 52.677_669_53).abs() < 1e-6);

    let center = point(60., 35.);
    assert_eq!(
        adapter.pointer_down(92, 0, 11, center, 2.).unwrap(),
        PointerPhaseOutcome::GestureStarted
    );
    assert_eq!(
        adapter.pointer_up(11, point(62.999, 35.)).unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(id.clone()))
    );
    assert_eq!(
        adapter.snapshot(92).unwrap().ellipses[0].rect,
        PdfRect::new(10., 10., 100., 50.).unwrap()
    );

    adapter.pointer_down(92, 0, 12, center, 2.).unwrap();
    assert_eq!(
        adapter.pointer_up(12, point(63., 35.)).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(id.clone())
    );
    assert_eq!(
        adapter.snapshot(92).unwrap().ellipses[0].rect,
        PdfRect::new(13., 10., 100., 50.).unwrap()
    );

    let east = point(113., 35.);
    assert_eq!(
        adapter.pointer_down(92, 0, 13, east, 2.).unwrap(),
        PointerPhaseOutcome::GestureStarted
    );
    adapter.pointer_move(13, point(133., 35.)).unwrap();
    assert_eq!(
        adapter.document_scene(92, 0).ellipses[0].rect,
        PdfRect::new(13., 10., 120., 50.).unwrap(),
        "resize preview must use the same retained pointer seam as commit"
    );
    assert_eq!(
        adapter.pointer_up(13, point(133., 35.)).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(id.clone())
    );

    let selected = &adapter.snapshot(92).unwrap().ellipses[0];
    let rotate = ellipse_rotation_handle_point(selected, 1.).unwrap();
    assert_eq!(
        adapter.pointer_down(92, 0, 14, rotate, 2.).unwrap(),
        PointerPhaseOutcome::GestureStarted
    );
    let right = point(
        selected.rect.x + selected.rect.width,
        selected.rect.y + selected.rect.height / 2.,
    );
    assert_eq!(
        adapter.pointer_up(14, right).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(id.clone())
    );
    assert!((adapter.snapshot(92).unwrap().ellipses[0].rotation_degrees - 90.).abs() < 1e-6);

    let rotated = &adapter.snapshot(92).unwrap().ellipses[0];
    let rotate = ellipse_rotation_handle_point(rotated, 1.).unwrap();
    assert_eq!(
        adapter.pointer_double_click(92, 0, rotate, 2.).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(id.clone())
    );
    assert_eq!(
        adapter.snapshot(92).unwrap().ellipses[0].rotation_degrees,
        0.
    );

    adapter.set_selected_locked(92, true).unwrap();
    assert_eq!(
        adapter
            .pointer_down(92, 0, 15, point(70., 35.), 2.)
            .unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(id.clone()))
    );
    assert_eq!(
        adapter
            .pointer_double_click(92, 0, point(73., 72.), 2.)
            .unwrap(),
        PointerPhaseOutcome::Ignored
    );
}

#[test]
fn arc_three_click_creation_snaps_the_bulge_and_retains_stable_control_points() {
    assert_eq!(AnnotationTool::Arc.label(), "Arc");
    assert_eq!(AnnotationTool::Arc.shortcut(), Some("Shift+C"));
    assert_eq!(AnnotationTool::Arc.toolbar_id(), "tool-arc");
    assert_eq!(
        AnnotationTool::from_toolbar_id("tool-arc"),
        Some(AnnotationTool::Arc)
    );

    let mut adapter = AnnotationAdapter::default();
    adapter.set_observed_pixels_per_point(1.).unwrap();
    adapter.set_tool(AnnotationTool::Arc).unwrap();
    let id = MarkupId::new("arc:adapter:quarter-turn").unwrap();
    adapter.queue_next_annotation_id(id.clone());

    assert_eq!(
        adapter
            .pointer_down_with_input(93, 0, 8, 0, point(0., 0.), 2., false)
            .unwrap(),
        PointerPhaseOutcome::PlacementPending
    );
    assert_eq!(
        adapter
            .pointer_down_with_input(93, 0, 8, 0, point(2., 0.), 2., false)
            .unwrap(),
        PointerPhaseOutcome::PlacementPending,
        "the frozen Electron contract requires a chord longer than two PDF points"
    );
    assert!(adapter.document_scene(93, 0).arcs.is_empty());

    assert_eq!(
        adapter
            .pointer_down_with_input(93, 0, 8, 0, point(100., 0.), 2., false)
            .unwrap(),
        PointerPhaseOutcome::PlacementPending
    );
    adapter
        .update_arc_hover(93, 0, point(50., 10.), true)
        .unwrap();
    let preview = &adapter.document_scene(93, 0).arcs[0];
    assert!(preview.draft);
    assert_eq!(preview.id, id);
    assert_eq!(preview.start, point(0., 0.));
    assert_eq!(preview.end, point(100., 0.));
    assert!((preview.mid.x - 50.).abs() < 1e-6);
    assert!((preview.mid.y - 20.710_678).abs() < 1e-6);
    assert_eq!(preview.sampled_path.len(), 65);

    assert_eq!(
        adapter
            .pointer_down_with_input(93, 0, 8, 0, point(50., 10.), 2., true)
            .unwrap(),
        PointerPhaseOutcome::AnnotationCreated(id.clone())
    );
    let snapshot = adapter.snapshot(93).unwrap();
    assert_eq!(snapshot.arcs.len(), 1);
    assert_eq!(snapshot.arcs[0].id, id);
    assert!((snapshot.arcs[0].sweep_degrees().abs() - 90.).abs() < 1e-6);
    assert_eq!(snapshot.annotation_order, vec![id]);
    assert_eq!(adapter.tool(), AnnotationTool::Select);
}

#[test]
fn arc_pointer_preview_is_coherent_css_scaled_and_revalidated_at_release() {
    let document_id = 193;
    let arc_id = MarkupId::new("arc:adapter:manipulation").unwrap();
    let rectangle_id = MarkupId::new("rectangle:adapter:other").unwrap();
    let appearance = RectangleAppearance::new("#2563eb", 2., None::<String>, 0.65).unwrap();
    let arc = ArcAnnotation::new(
        arc_id.clone(),
        0,
        point(0., 0.),
        point(100., 0.),
        point(50., 50.),
        appearance.clone(),
    )
    .unwrap();
    let rectangle = RectangleAnnotation {
        id: rectangle_id.clone(),
        page_index: 0,
        rect: PdfRect::new(200., 200., 40., 30.).unwrap(),
        rotation_degrees: 0.,
        appearance: RectangleAppearance::default(),
        locked: false,
    };
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(
            document_id,
            vec![Annotation::Arc(arc.clone()), Annotation::Rectangle(rectangle)],
        )
        .unwrap();
    adapter.set_observed_pixels_per_point(2.).unwrap();
    adapter.set_tool(AnnotationTool::Select).unwrap();
    assert!(adapter.select_id(document_id, &arc_id));

    assert_eq!(
        adapter.pointer_down(document_id, 0, 31, point(4., 0.), 2.).unwrap(),
        PointerPhaseOutcome::GestureStarted,
        "a point eight CSS pixels from Start must hit the handle at 2 px/pt",
    );
    adapter.pointer_move(31, point(-10., 12.)).unwrap();
    let start_preview = &adapter.document_scene(document_id, 0).arcs[0];
    assert!(start_preview.draft);
    assert_eq!(start_preview.start, point(-10., 12.));
    assert_eq!(start_preview.mid, point(50., 50.));
    assert_eq!(start_preview.end, point(100., 0.));
    adapter.cancel(PointerCancelReason::AdapterError).unwrap();

    let body_point = arc.sampled_path(64)[4];
    assert!(body_point.x.hypot(body_point.y) * 2. > 9.);
    adapter
        .pointer_down(document_id, 0, 32, body_point, 2.)
        .unwrap();
    adapter
        .pointer_move(32, point(body_point.x + 10., body_point.y + 6.))
        .unwrap();
    let body_preview = &adapter.document_scene(document_id, 0).arcs[0];
    assert_eq!(body_preview.start, point(10., 6.));
    assert_eq!(body_preview.mid, point(60., 56.));
    assert_eq!(body_preview.end, point(110., 6.));
    adapter.cancel(PointerCancelReason::AdapterError).unwrap();

    adapter
        .pointer_down(document_id, 0, 33, point(50., 50.), 2.)
        .unwrap();
    adapter
        .pointer_move_with_constraint(33, point(70., 30.), false)
        .unwrap();
    let free_preview = &adapter.document_scene(document_id, 0).arcs[0];
    assert!(free_preview.draft);
    assert_eq!(free_preview.mid, point(70., 30.), "unshifted Mid follows the raw pointer");
    assert_eq!(free_preview.start, point(0., 0.));
    assert_eq!(free_preview.end, point(100., 0.));
    assert_eq!(
        adapter
            .pointer_up_with_constraint(33, point(70., 30.), true)
            .unwrap(),
        PointerPhaseOutcome::AnnotationEdited(arc_id.clone()),
    );
    let shifted_release = &adapter.snapshot(document_id).unwrap().arcs[0];
    assert_eq!(shifted_release.start, point(0., 0.));
    assert_eq!(shifted_release.mid, point(50., 20.710_678));
    assert_eq!(shifted_release.end, point(100., 0.));
    assert!((shifted_release.sweep_degrees().abs() - 90.).abs() < 0.000_01);

    adapter.undo(document_id).unwrap();
    assert!(adapter.select_id(document_id, &arc_id));
    adapter
        .pointer_down(document_id, 0, 34, point(0., 0.), 2.)
        .unwrap();
    adapter.pointer_move(34, point(100., 0.)).unwrap();
    let invalid_preview = &adapter.document_scene(document_id, 0).arcs[0];
    assert_eq!((invalid_preview.start, invalid_preview.mid, invalid_preview.end), (arc.start, arc.mid, arc.end));
    assert_eq!(invalid_preview.sampled_path, arc.sampled_path(64));
    assert!(!invalid_preview.draft, "invalid controls must not pair with the old path");
    adapter.cancel(PointerCancelReason::AdapterError).unwrap();

    let body_point = arc.sampled_path(64)[16];
    adapter
        .pointer_down(document_id, 0, 35, body_point, 2.)
        .unwrap();
    adapter
        .pointer_move(35, point(body_point.x + 12., body_point.y + 8.))
        .unwrap();
    assert!(adapter.select_id(document_id, &rectangle_id));
    let selection_changed = &adapter.document_scene(document_id, 0).arcs[0];
    assert_eq!((selection_changed.start, selection_changed.mid, selection_changed.end), (arc.start, arc.mid, arc.end));
    assert!(!selection_changed.selected);
    assert!(!selection_changed.draft);
    adapter.cancel(PointerCancelReason::AdapterError).unwrap();

    assert!(adapter.select_id(document_id, &arc_id));
    adapter
        .pointer_down(document_id, 0, 36, body_point, 2.)
        .unwrap();
    adapter
        .pointer_move(36, point(body_point.x + 12., body_point.y + 8.))
        .unwrap();
    adapter
        .set_selected_arc_control_point(document_id, ArcControlPoint::Mid, point(50., 60.), false)
        .unwrap();
    let geometry_changed = &adapter.document_scene(document_id, 0).arcs[0];
    assert_eq!(geometry_changed.mid, point(50., 60.));
    assert_eq!(geometry_changed.start, point(0., 0.));
    assert_eq!(geometry_changed.end, point(100., 0.));
    assert!(!geometry_changed.draft);
    adapter.cancel(PointerCancelReason::AdapterError).unwrap();

    adapter
        .pointer_down(document_id, 0, 37, point(50., 60.), 2.)
        .unwrap();
    adapter.pointer_move(37, point(60., 75.)).unwrap();
    adapter.set_selected_locked(document_id, true).unwrap();
    let locked = &adapter.document_scene(document_id, 0).arcs[0];
    assert_eq!(locked.mid, point(50., 60.));
    assert!(locked.locked);
    assert!(!locked.draft);
}

#[test]
fn arc_midpoint_direct_edit_is_free_until_quarter_turn_snap_is_requested() {
    let document_id = 194;
    let arc_id = MarkupId::new("arc:adapter:direct-free-mid").unwrap();
    let arc = ArcAnnotation::new(
        arc_id.clone(),
        0,
        point(0., 0.),
        point(100., 0.),
        point(50., 50.),
        RectangleAppearance::default(),
    )
    .unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(document_id, vec![Annotation::Arc(arc)])
        .unwrap();
    assert!(adapter.select_id(document_id, &arc_id));

    adapter
        .set_selected_arc_control_point(
            document_id,
            ArcControlPoint::Mid,
            point(70., 30.),
            false,
        )
        .unwrap();

    let edited = adapter.snapshot(document_id).unwrap();
    assert_eq!(edited.arcs[0].mid, point(70., 30.));
    assert_eq!((edited.revision, edited.undo_depth), (1, 1));
}

#[test]
fn arc_control_hit_radius_stays_nine_css_pixels_at_high_zoom() {
    let document_id = 195;
    let arc_id = MarkupId::new("arc:adapter:css-handle-radius").unwrap();
    let arc = ArcAnnotation::new(
        arc_id.clone(),
        0,
        point(0., 0.),
        point(100., 0.),
        point(50., 50.),
        RectangleAppearance::default(),
    )
    .unwrap();
    let body_point = arc.sampled_path(64)[3];
    let distance_from_start_css = body_point.x.hypot(body_point.y) * 2.;
    assert!((10. ..=17.).contains(&distance_from_start_css));
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(document_id, vec![Annotation::Arc(arc)])
        .unwrap();
    adapter.set_observed_pixels_per_point(2.).unwrap();
    adapter.set_tool(AnnotationTool::Select).unwrap();
    assert!(adapter.select_id(document_id, &arc_id));

    assert_eq!(
        adapter
            .pointer_down(document_id, 0, 41, body_point, 2.)
            .unwrap(),
        PointerPhaseOutcome::GestureStarted,
    );
    adapter
        .pointer_move(41, point(body_point.x + 10., body_point.y + 6.))
        .unwrap();

    let preview = &adapter.document_scene(document_id, 0).arcs[0];
    for (actual, expected) in [
        (preview.start, point(10., 6.)),
        (preview.mid, point(60., 56.)),
        (preview.end, point(110., 6.)),
    ] {
        assert!((actual.x - expected.x).abs() < 1e-9);
        assert!((actual.y - expected.y).abs() < 1e-9);
    }
}

#[test]
fn straight_line_model_shares_history_scene_edit_and_segment_hit_contracts() {
    let line_id = MarkupId::new("line:model-contract").unwrap();
    let arrow_id = MarkupId::new("arrow:model-contract").unwrap();
    let mut document = AnnotationDocument::default();

    let line = StraightLineAnnotation::new(
        line_id.clone(),
        0,
        point(72.0, 144.0),
        point(252.0, 240.0),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let arrow = StraightLineAnnotation::new(
        arrow_id.clone(),
        0,
        point(90.0, 300.0),
        point(306.0, 300.0),
        LineKind::Arrow,
        StraightLineAppearance::default_for(LineKind::Arrow),
    )
    .unwrap();
    assert_eq!(line.appearance.stroke_color(), "#ff0000");
    assert_eq!(line.appearance.stroke_width_pt(), 1.0);
    assert_eq!(arrow.appearance.stroke_width_pt(), 0.5);

    for annotation in [line, arrow] {
        document
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::StraightLine(annotation),
            ))
            .unwrap();
    }
    let scene = document.document_scene(0);
    assert_eq!(scene.straight_lines.len(), 2);
    assert_eq!(scene.straight_lines[0].kind, LineKind::Line);
    assert_eq!(scene.straight_lines[1].kind, LineKind::Arrow);
    assert_eq!(document.snapshot().straight_lines.len(), 2);
    assert_eq!(document.history_depths(), (2, 0));

    assert_eq!(
        document.hit_test(0, point(162.0, 192.0), 2.0).unwrap(),
        Some(HitTarget::Body(line_id.clone())),
    );
    assert_eq!(document.hit_test(0, point(72.0, 240.0), 2.0).unwrap(), None);

    assert!(document.select(&line_id));
    assert_eq!(
        document.hit_test(0, point(252.0, 240.0), 2.0).unwrap(),
        Some(HitTarget::LineEndpoint {
            id: line_id.clone(),
            endpoint: LineEndpoint::End,
        }),
    );
    document
        .apply_command(AnnotationCommand::EditAnnotation {
            id: line_id.clone(),
            edit: AnnotationEdit::SetStraightLineEndpoint {
                endpoint: LineEndpoint::End,
                point: point(270.0, 252.0),
            },
        })
        .unwrap();
    document
        .apply_command(AnnotationCommand::EditAnnotation {
            id: line_id.clone(),
            edit: AnnotationEdit::TranslateStraightLine {
                delta_x: 6.0,
                delta_y: -12.0,
            },
        })
        .unwrap();
    assert_eq!(document.straight_lines()[0].start, point(78.0, 132.0));
    assert_eq!(document.straight_lines()[0].end, point(276.0, 240.0));

    assert!(document.set_locked(&line_id, true).unwrap());
    assert!(matches!(
        document.apply_command(AnnotationCommand::EditAnnotation {
            id: line_id.clone(),
            edit: AnnotationEdit::TranslateStraightLine {
                delta_x: 1.0,
                delta_y: 0.0,
            },
        }),
        Err(butter_paper_gpui_gallery::annotation_model::AnnotationError::LockedMarkup(_))
    ));
    assert!(matches!(
        document.apply_command(AnnotationCommand::DeleteSelected),
        Err(butter_paper_gpui_gallery::annotation_model::AnnotationError::LockedMarkup(_))
    ));
    document.set_locked(&line_id, false).unwrap();
    let delete = document
        .apply_command(AnnotationCommand::DeleteSelected)
        .unwrap();
    assert!(matches!(
        delete,
        butter_paper_gpui_gallery::annotation_model::CommandOutcome::Deleted { .. }
    ));
    assert_eq!(document.straight_lines().len(), 1);
    document.undo().unwrap();
    assert_eq!(document.straight_lines().len(), 2);
    document.redo().unwrap();
    assert_eq!(document.straight_lines().len(), 1);

    assert!(
        document.canonical_json_snapshot()["markups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|markup| markup["kind"] == "arrow")
    );
    assert_eq!(
        AnnotationKind::Line,
        Annotation::StraightLine(
            StraightLineAnnotation::new(
                MarkupId::new("line:kind").unwrap(),
                0,
                point(0.0, 0.0),
                point(3.0, 0.0),
                LineKind::Line,
                StraightLineAppearance::default_for(LineKind::Line),
            )
            .unwrap()
        )
        .kind()
    );
}

#[test]
fn straight_line_model_rejects_non_finite_and_two_point_geometry() {
    let appearance = StraightLineAppearance::default_for(LineKind::Line);
    assert!(
        StraightLineAnnotation::new(
            MarkupId::new("line:too-short").unwrap(),
            0,
            point(0.0, 0.0),
            point(2.0, 0.0),
            LineKind::Line,
            appearance.clone(),
        )
        .is_err()
    );
    assert!(
        StraightLineAnnotation::new(
            MarkupId::new("line:not-finite").unwrap(),
            0,
            point(0.0, 0.0),
            PdfPoint {
                x: f64::INFINITY,
                y: 0.0
            },
            LineKind::Line,
            appearance,
        )
        .is_err()
    );
}

#[test]
fn line_and_arrow_tools_expose_the_frozen_electron_identity() {
    assert_eq!(AnnotationTool::Line.label(), "Line");
    assert_eq!(AnnotationTool::Line.shortcut(), Some("L"));
    assert_eq!(AnnotationTool::Line.tooltip_label(), "Line (L)");
    assert_eq!(AnnotationTool::Line.toolbar_id(), "markup-line");
    assert_eq!(
        AnnotationTool::from_plain_shortcut("l"),
        Some(AnnotationTool::Line)
    );
    assert_eq!(
        AnnotationTool::from_toolbar_id("markup-line"),
        Some(AnnotationTool::Line)
    );

    assert_eq!(AnnotationTool::Arrow.label(), "Arrow");
    assert_eq!(AnnotationTool::Arrow.shortcut(), Some("A"));
    assert_eq!(AnnotationTool::Arrow.tooltip_label(), "Arrow (A)");
    assert_eq!(AnnotationTool::Arrow.toolbar_id(), "markup-arrow");
    assert_eq!(
        AnnotationTool::from_plain_shortcut("A"),
        Some(AnnotationTool::Arrow)
    );
    assert_eq!(
        AnnotationTool::from_toolbar_id("markup-arrow"),
        Some(AnnotationTool::Arrow)
    );
    assert_eq!(AnnotationTool::from_plain_shortcut("ctrl-l"), None);
}

#[test]
fn straight_line_pointer_creation_proves_primary_threshold_click_and_shift_contracts() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Line).unwrap();
    adapter.queue_next_annotation_id(MarkupId::new("line:pointer-drag").unwrap());
    assert_eq!(
        adapter
            .pointer_down_with_input(71, 0, 1, 1, point(10.0, 10.0), 4.0, false)
            .unwrap(),
        PointerPhaseOutcome::Ignored,
    );
    assert!(adapter.snapshot(71).is_none());

    adapter
        .pointer_down_with_input(71, 0, 2, 0, point(10.0, 10.0), 4.0, false)
        .unwrap();
    assert_eq!(
        adapter
            .pointer_up_with_constraint(2, point(12.999, 10.0), false)
            .unwrap(),
        PointerPhaseOutcome::PlacementPending,
    );
    assert!(adapter.is_click_placement_pending());
    adapter
        .cancel(butter_paper_gpui_gallery::annotation_model::PointerCancelReason::ToolChanged)
        .unwrap();

    adapter.queue_next_annotation_id(MarkupId::new("line:pointer-drag").unwrap());
    adapter
        .pointer_down_with_input(71, 0, 3, 0, point(10.0, 10.0), 4.0, false)
        .unwrap();
    assert_eq!(
        adapter
            .pointer_up_with_constraint(3, point(13.0, 10.0), false)
            .unwrap(),
        PointerPhaseOutcome::AnnotationCreated(MarkupId::new("line:pointer-drag").unwrap()),
    );
    assert_eq!(adapter.tool(), AnnotationTool::Select);
    let line = &adapter.snapshot(71).unwrap().straight_lines[0];
    assert_eq!(
        (line.start, line.end),
        (point(10.0, 10.0), point(13.0, 10.0))
    );
    assert_eq!(line.appearance.stroke_width_pt(), 1.0);

    adapter.set_tool(AnnotationTool::Arrow).unwrap();
    adapter.queue_next_annotation_id(MarkupId::new("arrow:click-click").unwrap());
    adapter
        .pointer_down_with_input(71, 0, 4, 0, point(100.0, 100.0), 4.0, false)
        .unwrap();
    assert_eq!(
        adapter
            .pointer_up_with_constraint(4, point(100.0, 100.0), false)
            .unwrap(),
        PointerPhaseOutcome::PlacementPending,
    );
    adapter
        .pointer_move_with_constraint(4, point(150.0, 130.0), true)
        .unwrap();
    let preview = &adapter.document_scene(71, 0).straight_lines[1];
    assert!(preview.draft);
    assert_eq!(preview.end, point(150.0, 100.0));
    assert_eq!(adapter.thumbnail_scene(71, 0).straight_lines.len(), 1);
    assert_eq!(
        adapter
            .pointer_down_with_input(71, 0, 4, 0, point(150.0, 130.0), 4.0, true)
            .unwrap(),
        PointerPhaseOutcome::AnnotationCreated(MarkupId::new("arrow:click-click").unwrap()),
    );
    let arrow = &adapter.snapshot(71).unwrap().straight_lines[1];
    assert_eq!(arrow.end, point(150.0, 100.0));
    assert_eq!(arrow.kind, LineKind::Arrow);
    assert_eq!(arrow.appearance.stroke_width_pt(), 0.5);
    assert_eq!(adapter.history_depths(71), (2, 0));
}

#[test]
fn semantic_snapping_line_creation_uses_retained_markup_and_one_history_entry() {
    let source_id = MarkupId::new("semantic-snap:source-line").unwrap();
    let source = StraightLineAnnotation::new(
        source_id.clone(),
        0,
        point(10., 10.),
        point(30., 10.),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let second_source = StraightLineAnnotation::new(
        MarkupId::new("semantic-snap:second-source").unwrap(),
        0,
        point(50., 0.),
        point(50., 20.),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(
            170,
            vec![
                Annotation::StraightLine(source),
                Annotation::StraightLine(second_source),
            ],
        )
        .unwrap();
    adapter
        .set_semantic_snap_settings(SemanticSnapSettings::default())
        .unwrap();
    adapter.set_observed_pixels_per_point(2.).unwrap();
    adapter.set_tool(AnnotationTool::Line).unwrap();
    let created_id = MarkupId::new("semantic-snap:created-line").unwrap();
    adapter.queue_next_annotation_id(created_id.clone());

    assert_eq!(
        adapter
            .pointer_down_with_input(170, 0, 1, 0, point(34., 10.), 3., false)
            .unwrap(),
        PointerPhaseOutcome::GestureStarted,
    );
    let first = adapter
        .semantic_snap_decision()
        .expect("the first click exposes transient guide evidence");
    assert_eq!(first.point, point(30., 10.));
    assert_eq!(first.owner_id.as_ref(), Some(&source_id));
    assert_eq!(first.role, SemanticSnapRole::Endpoint);

    assert_eq!(
        adapter
            .pointer_up_with_constraint(1, point(54., 10.), false)
            .unwrap(),
        PointerPhaseOutcome::AnnotationCreated(created_id.clone()),
    );
    let snapshot = adapter.snapshot(170).unwrap();
    let created = snapshot
        .straight_lines
        .iter()
        .find(|line| line.id == created_id)
        .unwrap();
    assert_eq!(
        (created.start, created.end),
        (point(30., 10.), point(50., 10.))
    );
    assert_eq!(adapter.history_depths(170), (1, 0));
}

#[test]
fn semantic_snapping_length_creation_uses_the_shared_engine_and_one_history_entry() {
    let endpoint_source_id = MarkupId::new("semantic-snap:length-endpoint-source").unwrap();
    let endpoint_source = StraightLineAnnotation::new(
        endpoint_source_id.clone(),
        0,
        point(10., 10.),
        point(30., 10.),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let midpoint_source = StraightLineAnnotation::new(
        MarkupId::new("semantic-snap:length-midpoint-source").unwrap(),
        0,
        point(50., 0.),
        point(50., 20.),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(
            171,
            vec![
                Annotation::StraightLine(endpoint_source),
                Annotation::StraightLine(midpoint_source),
            ],
        )
        .unwrap();
    adapter
        .set_document_page_length_calibration(
            171,
            0,
            LengthCalibration::new(1., "m", "Length", true).unwrap(),
        )
        .unwrap();
    adapter
        .set_semantic_snap_settings(SemanticSnapSettings::default())
        .unwrap();
    adapter.set_observed_pixels_per_point(2.).unwrap();
    adapter.set_tool(AnnotationTool::Length).unwrap();
    let created_id = MarkupId::new("semantic-snap:created-length").unwrap();
    let history_before = adapter.history_depths(171);

    assert_eq!(
        adapter
            .begin_length_placement(171, 0, created_id.clone(), point(34., 10.))
            .unwrap(),
        PointerPhaseOutcome::PlacementPending,
    );
    let first = adapter
        .semantic_snap_decision()
        .expect("the first Length point exposes transient guide evidence");
    assert_eq!(first.point, point(30., 10.));
    assert_eq!(first.owner_id.as_ref(), Some(&endpoint_source_id));
    assert_eq!(first.role, SemanticSnapRole::Endpoint);

    assert_eq!(
        adapter
            .commit_length_placement(171, 0, point(54., 10.), false)
            .unwrap(),
        PointerPhaseOutcome::AnnotationCreated(created_id.clone()),
    );
    let snapshot = adapter.snapshot(171).unwrap();
    let created = snapshot
        .lengths
        .iter()
        .find(|length| length.id == created_id)
        .unwrap();
    assert_eq!(
        (created.start, created.end),
        (point(30., 10.), point(50., 10.))
    );
    assert_eq!(adapter.history_depths(171), (history_before.0 + 1, 0));
}

#[test]
fn dimension_two_click_creation_keeps_caption_geometry_and_history_coherent() {
    assert_eq!(AnnotationTool::Dimension.label(), "Dimension");
    assert_eq!(AnnotationTool::Dimension.shortcut(), Some("Shift+L"));
    assert_eq!(
        AnnotationTool::Dimension.tooltip_label(),
        "Dimension (Shift+L)"
    );
    assert_eq!(AnnotationTool::Dimension.toolbar_id(), "tool-dimension");
    assert_eq!(
        AnnotationTool::from_plain_shortcut("l"),
        Some(AnnotationTool::Line)
    );
    assert_eq!(
        AnnotationTool::from_toolbar_id("tool-dimension"),
        Some(AnnotationTool::Dimension)
    );

    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Dimension).unwrap();
    let id = MarkupId::new("dimension:adapter").unwrap();
    assert_eq!(
        adapter
            .begin_dimension_placement(88, 0, id.clone(), point(20., 50.))
            .unwrap(),
        PointerPhaseOutcome::PlacementPending
    );
    adapter
        .update_dimension_placement(point(120., 50.), false)
        .unwrap();
    let draft = &adapter.document_scene(88, 0).dimensions[0];
    assert!(draft.draft);
    assert_eq!(draft.content, "Dimension");
    assert_eq!(draft.dimension_line_offset, 24.);
    assert_eq!(draft.start, point(20., 50.));
    assert_eq!(draft.end, point(120., 50.));

    assert_eq!(
        adapter
            .commit_dimension_placement(88, 0, point(120., 50.), false)
            .unwrap(),
        PointerPhaseOutcome::AnnotationCreated(id.clone())
    );
    adapter
        .replace_dimension_content_in_create_transaction(88, &id, "Door opening")
        .unwrap();
    assert_eq!(adapter.history_depths(88), (1, 0));

    adapter
        .edit_selected_dimension_endpoint(88, LineEndpoint::End, point(130., 55.))
        .unwrap();
    adapter.set_selected_dimension_offset(88, 40.).unwrap();
    adapter.move_selected_dimension(88, 5., -2.).unwrap();
    let snapshot = adapter.snapshot(88).unwrap();
    assert_eq!(snapshot.dimensions.len(), 1);
    let dimension = &snapshot.dimensions[0];
    assert_eq!(dimension.id, id);
    assert_eq!(dimension.start, point(25., 48.));
    assert_eq!(dimension.end, point(135., 53.));
    assert_eq!(dimension.dimension_line_offset(), 40.);
    assert_eq!(dimension.content(), "Door opening");
    assert_eq!(snapshot.annotation_order, vec![dimension.id.clone()]);
    assert_eq!(adapter.thumbnail_scene(88, 0).dimensions.len(), 1);

    adapter.set_selected_locked(88, true).unwrap();
    assert!(adapter.set_selected_dimension_offset(88, 48.).is_err());

    let short_id = MarkupId::new("dimension:short").unwrap();
    adapter.set_tool(AnnotationTool::Dimension).unwrap();
    adapter
        .begin_dimension_placement(89, 0, short_id, point(0., 0.))
        .unwrap();
    assert_eq!(
        adapter
            .commit_dimension_placement(89, 0, point(2., 0.), false)
            .unwrap(),
        PointerPhaseOutcome::Ignored
    );
    assert!(adapter.snapshot(89).unwrap().dimensions.is_empty());

    assert_eq!(
        DimensionAnnotation::default_offset(point(120., 50.), point(20., 50.)),
        -24.
    );
}

#[test]
fn dimension_pointer_selection_hits_offset_line() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Dimension).unwrap();
    let id = MarkupId::new("dimension:offset-line-selection").unwrap();
    adapter
        .begin_dimension_placement(118, 0, id.clone(), point(20., 50.))
        .unwrap();
    adapter
        .commit_dimension_placement(118, 0, point(120., 50.), false)
        .unwrap();
    adapter.set_tool(AnnotationTool::Select).unwrap();
    adapter.clear_selection(118);
    assert_eq!(adapter.snapshot(118).unwrap().selected_id, None);

    assert_eq!(
        adapter
            .pointer_down(118, 0, 41, point(70., 74.), 4.)
            .unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(id.clone()))
    );
    assert_eq!(adapter.snapshot(118).unwrap().selected_id, Some(id));
}

#[test]
fn callout_two_click_creation_preserves_frozen_composite_geometry() {
    assert_eq!(AnnotationTool::Callout.label(), "Callout");
    assert_eq!(AnnotationTool::Callout.shortcut(), Some("Q"));
    assert_eq!(AnnotationTool::Callout.toolbar_id(), "tool-callout");
    assert_eq!(
        AnnotationTool::from_plain_shortcut("q"),
        Some(AnnotationTool::Callout)
    );

    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Callout).unwrap();
    adapter.queue_next_annotation_id(MarkupId::new("callout:two-click").unwrap());
    adapter
        .pointer_down_with_input(91, 0, 1, 0, point(20., 20.), 2., false)
        .unwrap();
    assert_eq!(
        adapter.pointer_up(1, point(20., 20.)).unwrap(),
        PointerPhaseOutcome::PlacementPending,
    );
    adapter.pointer_move(1, point(100., 80.)).unwrap();
    let draft = adapter.document_scene(91, 0).callouts[0].clone();
    assert!(draft.draft);
    assert_eq!(draft.text_box, PdfRect::new(100., 58., 150., 44.).unwrap());
    assert_eq!(
        draft.leader_points,
        vec![point(20., 20.), point(60., 80.), point(100., 80.)]
    );
    assert!(adapter.thumbnail_scene(91, 0).callouts.is_empty());

    assert_eq!(
        adapter
            .pointer_down_with_input(91, 0, 1, 0, point(100., 80.), 2., false)
            .unwrap(),
        PointerPhaseOutcome::AnnotationCreated(MarkupId::new("callout:two-click").unwrap()),
    );
    let snapshot = adapter.snapshot(91).unwrap();
    assert_eq!(snapshot.callouts.len(), 1);
    assert_eq!(snapshot.callouts[0].content(), "Callout");
    assert_eq!(snapshot.callouts[0].leader_points(), draft.leader_points);
    assert_eq!(adapter.tool(), AnnotationTool::Select);
    assert_eq!(adapter.history_depths(91), (1, 0));
}

#[test]
fn straight_line_selection_body_endpoint_lock_and_history_flow_through_the_adapter() {
    let id = MarkupId::new("line:edit-journey").unwrap();
    let annotation = StraightLineAnnotation::new(
        id.clone(),
        0,
        point(10.0, 10.0),
        point(100.0, 10.0),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(72, vec![Annotation::StraightLine(annotation)])
        .unwrap();

    assert_eq!(
        adapter
            .pointer_down(72, 0, 1, point(50.0, 10.0), 2.0)
            .unwrap(),
        PointerPhaseOutcome::GestureStarted,
    );
    assert_eq!(
        adapter.pointer_up(1, point(52.999, 10.0)).unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(id.clone())),
    );
    assert_eq!(adapter.history_depths(72), (0, 0));

    adapter
        .pointer_down(72, 0, 2, point(50.0, 10.0), 2.0)
        .unwrap();
    assert_eq!(
        adapter.pointer_up(2, point(53.0, 10.0)).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(id.clone()),
    );
    assert_eq!(
        adapter.snapshot(72).unwrap().straight_lines[0].start,
        point(13.0, 10.0)
    );

    adapter
        .pointer_down(72, 0, 3, point(103.0, 10.0), 2.0)
        .unwrap();
    assert_eq!(
        adapter.pointer_up(3, point(103.0, 14.0)).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(id.clone()),
    );
    assert_eq!(
        adapter.snapshot(72).unwrap().straight_lines[0].end,
        point(103.0, 14.0)
    );

    adapter.set_selected_locked(72, true).unwrap();
    assert_eq!(adapter.selected_kind(72), Some(AnnotationKind::Line));
    assert_eq!(
        adapter
            .pointer_down(72, 0, 4, point(50.0, 10.0), 2.0)
            .unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(id.clone())),
    );
    assert!(adapter.delete_selected(72).is_err());
    adapter.set_selected_locked(72, false).unwrap();
    adapter.delete_selected(72).unwrap();
    assert!(adapter.snapshot(72).unwrap().straight_lines.is_empty());
    adapter.undo(72).unwrap();
    assert_eq!(adapter.snapshot(72).unwrap().straight_lines.len(), 1);
    adapter.redo(72).unwrap();
    assert!(adapter.snapshot(72).unwrap().straight_lines.is_empty());
}

#[test]
fn selected_straight_line_properties_are_controlled_independent_and_lock_aware() {
    let line_id = MarkupId::new("line:properties").unwrap();
    let arrow_id = MarkupId::new("arrow:properties").unwrap();
    let line = StraightLineAnnotation::new(
        line_id.clone(),
        0,
        point(10.0, 10.0),
        point(100.0, 10.0),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let arrow = StraightLineAnnotation::new(
        arrow_id.clone(),
        0,
        point(10.0, 30.0),
        point(100.0, 30.0),
        LineKind::Arrow,
        StraightLineAppearance::default_for(LineKind::Arrow),
    )
    .unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(
            73,
            vec![
                Annotation::StraightLine(line),
                Annotation::StraightLine(arrow),
            ],
        )
        .unwrap();

    assert!(adapter.select_id(73, &line_id));
    for edit in [
        StraightLinePropertyEdit::StrokeColor("#2563eb".into()),
        StraightLinePropertyEdit::StrokeWidthPt(4.0),
        StraightLinePropertyEdit::Opacity(0.5),
    ] {
        adapter
            .edit_selected_straight_line_property(73, edit)
            .unwrap();
    }
    assert_eq!(adapter.history_depths(73), (3, 0));
    let snapshot = adapter.snapshot(73).unwrap();
    let edited_line = snapshot
        .straight_lines
        .iter()
        .find(|annotation| annotation.id == line_id)
        .unwrap();
    assert_eq!(edited_line.appearance.stroke_color(), "#2563eb");
    assert_eq!(edited_line.appearance.stroke_width_pt(), 4.0);
    assert_eq!(edited_line.appearance.opacity(), 0.5);
    assert_eq!(
        snapshot
            .straight_lines
            .iter()
            .find(|annotation| annotation.id == arrow_id)
            .unwrap()
            .appearance,
        StraightLineAppearance::default_for(LineKind::Arrow),
    );

    adapter
        .edit_selected_straight_line_property(
            73,
            StraightLinePropertyEdit::StrokeColor("#2563eb".into()),
        )
        .unwrap();
    assert_eq!(
        adapter.history_depths(73),
        (3, 0),
        "a no-op must stay out of history"
    );
    assert!(
        adapter
            .edit_selected_straight_line_property(
                73,
                StraightLinePropertyEdit::StrokeWidthPt(24.25),
            )
            .is_err()
    );
    assert_eq!(adapter.history_depths(73), (3, 0));

    adapter.set_selected_locked(73, true).unwrap();
    assert!(
        adapter
            .edit_selected_straight_line_property(73, StraightLinePropertyEdit::Opacity(0.75),)
            .is_err()
    );
    assert_eq!(adapter.history_depths(73), (4, 0));
    assert_eq!(
        adapter
            .snapshot(73)
            .unwrap()
            .straight_lines
            .iter()
            .find(|annotation| annotation.id == line_id)
            .unwrap()
            .appearance
            .opacity(),
        0.5,
    );
}

#[test]
fn selected_vertex_path_properties_preserve_hidden_appearance_and_are_exact() {
    let polyline_id = MarkupId::new("polyline:properties").unwrap();
    let polygon_id = MarkupId::new("polygon:properties").unwrap();
    let hidden = RectangleAppearance::new("#112233", 2., Some("#445566"), 0.8)
        .unwrap().with_fill_opacity(0.35).unwrap().with_stroke_style(StrokeStyle::Dashed);
    let polyline = VertexPathAnnotation::new(polyline_id.clone(), 0, vec![point(0., 0.), point(20., 20.)], VertexPathKind::Polyline, hidden.clone()).unwrap();
    let polygon = VertexPathAnnotation::new(polygon_id.clone(), 0, vec![point(0., 0.), point(20., 0.), point(10., 20.)], VertexPathKind::Polygon, hidden.clone()).unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter.load_imported_annotations(174, vec![Annotation::VertexPath(polyline), Annotation::VertexPath(polygon)]).unwrap();
    assert!(adapter.select_id(174, &polygon_id));
    assert_eq!(adapter.selected_vertex_path(174).unwrap().id, polygon_id);
    adapter.edit_selected_vertex_path_property(174, VertexPathPropertyEdit::StrokeColor("#abcdef80".into())).unwrap();
    adapter.edit_selected_vertex_path_property(174, VertexPathPropertyEdit::StrokeWidthPt(4.25)).unwrap();
    adapter.edit_selected_vertex_path_property(174, VertexPathPropertyEdit::Opacity(0.5)).unwrap();
    adapter.edit_selected_vertex_path_property(174, VertexPathPropertyEdit::FillColor(None)).unwrap();
    let appearance = &adapter.selected_vertex_path(174).unwrap().appearance;
    assert_eq!(appearance.stroke_color(), "#abcdef");
    assert_eq!(appearance.stroke_width_pt(), 4.25);
    assert_eq!(appearance.opacity(), 0.5);
    assert_eq!(appearance.fill_color(), None);
    assert_eq!(appearance.fill_opacity(), 0.35);
    assert_eq!(appearance.stroke_style(), StrokeStyle::Dashed);
    assert_eq!(adapter.history_depths(174), (4, 0));
    for width in [0.25, 24.0] {
        adapter.edit_selected_vertex_path_property(174, VertexPathPropertyEdit::StrokeWidthPt(width)).unwrap();
    }
    for opacity in [0.0, 1.0] {
        adapter.edit_selected_vertex_path_property(174, VertexPathPropertyEdit::Opacity(opacity)).unwrap();
    }
    let accepted_depth = adapter.history_depths(174);
    for edit in [
        VertexPathPropertyEdit::StrokeWidthPt(0.249),
        VertexPathPropertyEdit::StrokeWidthPt(24.001),
        VertexPathPropertyEdit::StrokeWidthPt(f64::NAN),
        VertexPathPropertyEdit::Opacity(-0.001),
        VertexPathPropertyEdit::Opacity(1.001),
        VertexPathPropertyEdit::Opacity(f64::INFINITY),
        VertexPathPropertyEdit::StrokeColor("#abcdefzz".into()),
        VertexPathPropertyEdit::FillColor(Some("#123456g0".into())),
    ] {
        assert!(adapter.edit_selected_vertex_path_property(174, edit).is_err());
        assert_eq!(adapter.history_depths(174), accepted_depth);
    }
    adapter.edit_selected_vertex_path_property(174, VertexPathPropertyEdit::FillColor(None)).unwrap();
    assert_eq!(adapter.history_depths(174), accepted_depth);
    adapter.set_selected_locked(174, true).unwrap();
    assert!(adapter.edit_selected_vertex_path_property(174, VertexPathPropertyEdit::Opacity(0.4)).is_err());
    assert!(adapter.select_id(174, &polyline_id));
    assert_eq!(adapter.selected_vertex_path(174).unwrap().appearance, hidden);
    assert!(adapter.edit_selected_vertex_path_property(174, VertexPathPropertyEdit::FillColor(Some("#ffffff80".into()))).is_err());
}

#[test]
fn shift_click_group_move_preserves_order_focus_locked_member_and_one_history_transaction() {
    let rectangle_a_id = MarkupId::new("rectangle:group-a").unwrap();
    let line_b_id = MarkupId::new("line:group-b").unwrap();
    let rectangle_c_id = MarkupId::new("rectangle:group-c-locked").unwrap();
    let rectangle_a = RectangleAnnotation {
        id: rectangle_a_id.clone(),
        page_index: 0,
        rect: PdfRect::new(10., 10., 20., 20.).unwrap(),
        rotation_degrees: 0.,
        appearance: RectangleAppearance::default(),
        locked: false,
    };
    let line_b = StraightLineAnnotation::new(
        line_b_id.clone(),
        0,
        point(50., 20.),
        point(90., 20.),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let rectangle_c = RectangleAnnotation {
        id: rectangle_c_id.clone(),
        page_index: 0,
        rect: PdfRect::new(110., 10., 20., 20.).unwrap(),
        rotation_degrees: 0.,
        appearance: RectangleAppearance::default(),
        locked: true,
    };
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(
            74,
            vec![
                Annotation::Rectangle(rectangle_a),
                Annotation::StraightLine(line_b),
                Annotation::Rectangle(rectangle_c),
            ],
        )
        .unwrap();

    adapter
        .pointer_down_with_input(74, 0, 1, 0, point(20., 20.), 2., false)
        .unwrap();
    adapter.pointer_up(1, point(20., 20.)).unwrap();
    assert_eq!(adapter.selected_ids(74), &[rectangle_a_id.clone()]);
    assert_eq!(
        adapter
            .pointer_down_with_input(74, 0, 2, 0, point(70., 20.), 2., true)
            .unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(rectangle_a_id.clone())),
    );
    assert_eq!(
        adapter
            .pointer_down_with_input(74, 0, 3, 0, point(120., 20.), 2., true)
            .unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(rectangle_a_id.clone())),
    );
    assert_eq!(
        adapter.selected_ids(74),
        &[
            rectangle_a_id.clone(),
            line_b_id.clone(),
            rectangle_c_id.clone(),
        ],
    );
    let before = adapter.snapshot(74).unwrap();

    assert_eq!(
        adapter
            .pointer_down_with_input(74, 0, 4, 0, point(70., 20.), 2., false)
            .unwrap(),
        PointerPhaseOutcome::GestureStarted,
    );
    adapter.pointer_move(4, point(75., 27.)).unwrap();
    assert_eq!(
        adapter.pointer_up(4, point(75., 27.)).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(rectangle_a_id.clone()),
    );

    let moved = adapter.snapshot(74).unwrap();
    assert_eq!(moved.revision, before.revision + 1);
    assert_eq!(moved.undo_depth, before.undo_depth + 1);
    assert_eq!(
        adapter.selected_ids(74),
        &[
            rectangle_a_id.clone(),
            line_b_id.clone(),
            rectangle_c_id.clone(),
        ],
    );
    let moved_a = moved
        .rectangles
        .iter()
        .find(|annotation| annotation.id == rectangle_a_id)
        .unwrap();
    assert_eq!((moved_a.rect.x, moved_a.rect.y), (15., 17.));
    let moved_b = moved
        .straight_lines
        .iter()
        .find(|annotation| annotation.id == line_b_id)
        .unwrap();
    assert_eq!(
        (moved_b.start, moved_b.end),
        (point(55., 27.), point(95., 27.))
    );
    let retained_c = moved
        .rectangles
        .iter()
        .find(|annotation| annotation.id == rectangle_c_id)
        .unwrap();
    assert_eq!((retained_c.rect.x, retained_c.rect.y), (110., 10.));

    adapter.undo(74).unwrap();
    assert_eq!(adapter.snapshot(74).unwrap().rectangles, before.rectangles);
    assert_eq!(
        adapter.snapshot(74).unwrap().straight_lines,
        before.straight_lines
    );
    assert_eq!(
        adapter.selected_ids(74),
        &[
            rectangle_a_id.clone(),
            line_b_id.clone(),
            rectangle_c_id.clone(),
        ],
    );
    adapter.redo(74).unwrap();
    assert_eq!(adapter.snapshot(74).unwrap().rectangles, moved.rectangles);
    assert_eq!(
        adapter.snapshot(74).unwrap().straight_lines,
        moved.straight_lines
    );

    let copied = adapter.selected_annotations_in_document_order(74);
    assert_eq!(
        copied
            .iter()
            .map(|annotation| annotation.id().clone())
            .collect::<Vec<_>>(),
        vec![
            rectangle_a_id.clone(),
            line_b_id.clone(),
            rectangle_c_id.clone(),
        ],
    );
    let pasted_ids = [
        MarkupId::new("paste:rectangle:a").unwrap(),
        MarkupId::new("paste:line:b").unwrap(),
        MarkupId::new("paste:rectangle:c-locked").unwrap(),
    ];
    let pasted = copied
        .iter()
        .zip(&pasted_ids)
        .map(|(annotation, id)| {
            annotation
                .translated_copy(id.clone(), 0, 12., -12.)
                .unwrap()
        })
        .collect();
    let before_paste = adapter.snapshot(74).unwrap();
    assert_eq!(adapter.insert_annotations(74, pasted).unwrap(), pasted_ids);
    let after_paste = adapter.snapshot(74).unwrap();
    assert_eq!(after_paste.revision, before_paste.revision + 1);
    assert_eq!(after_paste.undo_depth, before_paste.undo_depth + 1);
    assert_eq!(adapter.selected_ids(74), &pasted_ids);
    let pasted_a = after_paste
        .rectangles
        .iter()
        .find(|annotation| annotation.id == pasted_ids[0])
        .unwrap();
    assert_eq!((pasted_a.rect.x, pasted_a.rect.y), (27., 5.));
    assert!(
        after_paste
            .rectangles
            .iter()
            .find(|annotation| annotation.id == pasted_ids[2])
            .unwrap()
            .locked
    );

    let before_delete = after_paste.clone();
    assert_eq!(
        adapter.delete_selected_unlocked(74).unwrap(),
        vec![pasted_ids[0].clone(), pasted_ids[1].clone()],
    );
    let deleted = adapter.snapshot(74).unwrap();
    assert_eq!(deleted.revision, before_delete.revision + 1);
    assert_eq!(deleted.undo_depth, before_delete.undo_depth + 1);
    assert_eq!(adapter.selected_ids(74), &[pasted_ids[2].clone()]);
    assert!(
        deleted
            .rectangles
            .iter()
            .any(|annotation| annotation.id == pasted_ids[2])
    );
    assert!(
        deleted
            .straight_lines
            .iter()
            .all(|annotation| annotation.id != pasted_ids[1])
    );
    adapter.undo(74).unwrap();
    assert_eq!(
        adapter.snapshot(74).unwrap().rectangles,
        before_delete.rectangles
    );
    assert_eq!(adapter.selected_ids(74), &[pasted_ids[2].clone()]);
    adapter.redo(74).unwrap();
    assert_eq!(adapter.snapshot(74).unwrap().rectangles, deleted.rectangles);
    assert_eq!(adapter.selected_ids(74), &[pasted_ids[2].clone()]);

    assert!(adapter.select_id(74, &rectangle_c_id));
    assert!(adapter.toggle_selection(74, &pasted_ids[2]));
    let all_locked_before_drag = adapter.snapshot(74).unwrap();
    assert_eq!(
        adapter
            .pointer_down_with_input(74, 0, 5, 0, point(120., 20.), 2., false)
            .unwrap(),
        PointerPhaseOutcome::GestureStarted,
    );
    adapter.pointer_move(5, point(128., 28.)).unwrap();
    assert_eq!(
        adapter.pointer_up(5, point(128., 28.)).unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(rectangle_c_id.clone())),
        "an all-locked group drag is consumed without reporting a document edit",
    );
    assert_eq!(adapter.snapshot(74).unwrap(), all_locked_before_drag);
    assert_eq!(
        adapter.selected_ids(74),
        &[rectangle_c_id, pasted_ids[2].clone()]
    );
}

#[test]
fn shift_click_and_group_move_cover_every_maintained_annotation_family() {
    let rectangle = RectangleAnnotation {
        id: MarkupId::new("family:rectangle").unwrap(),
        page_index: 0,
        rect: PdfRect::new(10., 10., 20., 20.).unwrap(),
        rotation_degrees: 0.,
        appearance: RectangleAppearance::default(),
        locked: false,
    };
    let line = StraightLineAnnotation::new(
        MarkupId::new("family:line").unwrap(),
        0,
        point(10., 50.),
        point(30., 50.),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let pen = PenAnnotation::new(
        MarkupId::new("family:pen").unwrap(),
        0,
        vec![point(10., 80.), point(30., 80.)],
        PenAppearance::new("#ff0000", 2., 1.).unwrap(),
    )
    .unwrap();
    let text = TextBoxAnnotation::new(
        MarkupId::new("family:text").unwrap(),
        0,
        PdfRect::new(10., 110., 30., 20.).unwrap(),
        "Text",
        TextBoxStyle::new("Helvetica", 12., "#000000", 1.).unwrap(),
    )
    .unwrap();
    let length = LengthAnnotation::new(
        MarkupId::new("family:length").unwrap(),
        0,
        point(10., 150.),
        point(30., 150.),
        LengthCalibration::from_scale(72., 1., "m", 2, true).unwrap(),
    )
    .unwrap();
    let image = ImageAnnotation::new(
        MarkupId::new("family:image").unwrap(),
        0,
        PdfRect::new(10., 180., 40., 20.).unwrap(),
        DecodedRgbaAsset::new(4, 2, vec![0x80; 4 * 2 * 4]).unwrap(),
        false,
    )
    .unwrap();
    let ids = [
        rectangle.id.clone(),
        line.id.clone(),
        pen.id.clone(),
        text.id.clone(),
        length.id.clone(),
        image.id.clone(),
    ];
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(
            75,
            vec![
                Annotation::Rectangle(rectangle),
                Annotation::StraightLine(line),
                Annotation::Pen(pen),
                Annotation::TextBox(text),
                Annotation::Length(length),
                Annotation::Image(image),
            ],
        )
        .unwrap();

    adapter
        .pointer_down_with_input(75, 0, 1, 0, point(20., 20.), 2., false)
        .unwrap();
    adapter.pointer_up(1, point(20., 20.)).unwrap();
    for (pointer_id, target) in [
        (2, point(20., 50.)),
        (3, point(20., 80.)),
        (4, point(20., 120.)),
        (5, point(20., 150.)),
        (6, point(20., 190.)),
    ] {
        adapter
            .pointer_down_with_input(75, 0, pointer_id, 0, target, 2., true)
            .unwrap();
    }
    assert_eq!(adapter.selected_ids(75), &ids);
    let before = adapter.snapshot(75).unwrap();

    assert_eq!(
        adapter
            .pointer_down_with_input(75, 0, 7, 0, point(20., 190.), 2., false)
            .unwrap(),
        PointerPhaseOutcome::GestureStarted,
    );
    adapter.pointer_move(7, point(28., 196.)).unwrap();
    let preview = adapter.document_scene(75, 0);
    assert_eq!(preview.rectangles[0].rect.x, 18.);
    assert_eq!(preview.straight_lines[0].start, point(18., 56.));
    assert_eq!(preview.pens[0].paths[0][0], point(18., 86.));
    assert_eq!(preview.text_boxes[0].layout_rect.x, 18.);
    assert_eq!(preview.lengths[0].start, point(18., 156.));
    assert_eq!(preview.images[0].rect.x, 18.);
    assert_eq!(
        adapter.pointer_up(7, point(28., 196.)).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(ids[0].clone()),
    );
    let moved = adapter.snapshot(75).unwrap();
    assert_eq!((moved.revision, moved.undo_depth), (1, 1));
    assert_eq!(moved.rectangles[0].rect.x, 18.);
    assert_eq!(moved.straight_lines[0].start, point(18., 56.));
    assert_eq!(moved.pens[0].paths().next().unwrap()[0], point(18., 86.));
    assert_eq!(moved.text_boxes[0].layout_rect.x, 18.);
    assert_eq!(moved.lengths[0].start, point(18., 156.));
    assert_eq!(moved.images[0].rect.x, 18.);
    adapter.undo(75).unwrap();
    assert_eq!(adapter.snapshot(75).unwrap().rectangles, before.rectangles);
    adapter.redo(75).unwrap();
    assert_eq!(adapter.snapshot(75).unwrap(), moved);
}

#[test]
fn selected_text_box_body_move_and_resize_handles_commit_once_and_respect_lock() {
    let id = MarkupId::new("text-box:pointer-edit").unwrap();
    let original = PdfRect::new(100., 120., 80., 32.).unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(
            751,
            vec![Annotation::TextBox(
                TextBoxAnnotation::new(
                    id.clone(),
                    0,
                    original,
                    "Existing text",
                    TextBoxStyle::new("Helvetica", 12., "#000000", 1.).unwrap(),
                )
                .unwrap(),
            )],
        )
        .unwrap();

    assert_eq!(
        adapter
            .pointer_down_with_input(751, 0, 1, 0, point(120., 130.), 2., false)
            .unwrap(),
        PointerPhaseOutcome::GestureStarted,
    );
    adapter.pointer_move(1, point(132., 138.)).unwrap();
    assert_eq!(
        adapter.document_scene(751, 0).text_boxes[0].layout_rect,
        PdfRect::new(112., 128., 80., 32.).unwrap(),
    );
    assert_eq!(
        adapter.pointer_up(1, point(132., 138.)).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(id.clone()),
    );
    assert_eq!(
        (
            adapter.snapshot(751).unwrap().revision,
            adapter.snapshot(751).unwrap().undo_depth
        ),
        (1, 1),
    );

    let south_east = point(192., 128.);
    assert_eq!(
        adapter
            .pointer_down_with_input(751, 0, 2, 0, south_east, 2., false)
            .unwrap(),
        PointerPhaseOutcome::GestureStarted,
    );
    adapter.pointer_move(2, point(216., 112.)).unwrap();
    assert_eq!(
        adapter.document_scene(751, 0).text_boxes[0].layout_rect,
        PdfRect::new(112., 112., 104., 48.).unwrap(),
    );
    assert_eq!(
        adapter.pointer_up(2, point(216., 112.)).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(id.clone()),
    );
    assert_eq!(
        (
            adapter.snapshot(751).unwrap().revision,
            adapter.snapshot(751).unwrap().undo_depth
        ),
        (2, 2),
    );

    assert_eq!(
        adapter
            .pointer_double_click(751, 0, point(130., 140.), 2.)
            .unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(id.clone())),
        "double-click identifies the selected unlocked Text Box without mutating it",
    );
    assert_eq!(adapter.snapshot(751).unwrap().undo_depth, 2);

    adapter.set_selected_locked(751, true).unwrap();
    let locked = adapter.snapshot(751).unwrap();
    assert_eq!(
        adapter
            .pointer_down_with_input(751, 0, 3, 0, point(130., 140.), 2., false)
            .unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(id)),
    );
    assert_eq!(adapter.snapshot(751).unwrap(), locked);
}

#[test]
fn text_box_all_eight_resize_handles_clamp_through_rotated_zoomed_page_two_transform() {
    let transform = PageTransform::new_rotated(612., 792., 1.75, PageRotation::Degrees90).unwrap();
    let original = PdfRect::new(100., 120., 80., 40.).unwrap();
    for (ix, handle) in RectangleResizeHandle::ALL.into_iter().enumerate() {
        let id = MarkupId::new(format!("text-box:all-handles:{ix}")).unwrap();
        let mut adapter = AnnotationAdapter::default();
        adapter
            .load_imported_annotations(
                752,
                vec![Annotation::TextBox(
                    TextBoxAnnotation::new(
                        id.clone(),
                        2,
                        original,
                        "handle table",
                        TextBoxStyle::new("Helvetica", 12., "#000000", 1.).unwrap(),
                    )
                    .unwrap(),
                )],
            )
            .unwrap();
        assert!(adapter.select_id(752, &id));
        let start_pixels = transform.point_to_local_pixels(handle.point(original));
        let start = transform
            .point_from_local_pixels(start_pixels.x, start_pixels.y)
            .unwrap();
        let collapse = match handle {
            RectangleResizeHandle::NorthWest => point(230., 40.),
            RectangleResizeHandle::North => point(140., 40.),
            RectangleResizeHandle::NorthEast => point(50., 40.),
            RectangleResizeHandle::East => point(50., 140.),
            RectangleResizeHandle::SouthEast => point(50., 220.),
            RectangleResizeHandle::South => point(140., 220.),
            RectangleResizeHandle::SouthWest => point(230., 220.),
            RectangleResizeHandle::West => point(230., 140.),
        };
        let collapse_pixels = transform.point_to_local_pixels(collapse);
        let collapse = transform
            .point_from_local_pixels(collapse_pixels.x, collapse_pixels.y)
            .unwrap();
        assert_eq!(
            adapter
                .pointer_down_with_input(752, 2, 100 + ix as u64, 0, start, 1.75, false)
                .unwrap(),
            PointerPhaseOutcome::GestureStarted,
        );
        adapter.pointer_move(100 + ix as u64, collapse).unwrap();
        let preview = adapter.document_scene(752, 2).text_boxes[0].layout_rect;
        assert!(
            preview.width >= 2. && preview.height >= 2.,
            "{handle:?} must clamp"
        );
        assert_eq!(
            adapter.pointer_up(100 + ix as u64, collapse).unwrap(),
            PointerPhaseOutcome::AnnotationEdited(id),
        );
        let snapshot = adapter.snapshot(752).unwrap();
        assert_eq!((snapshot.revision, snapshot.undo_depth), (1, 1));
        assert_eq!(snapshot.text_boxes[0].page_index, 2);
    }
}

#[test]
fn empty_canvas_marquee_uses_viewport_threshold_and_replace_add_remove_modifiers() {
    let rectangle_id = MarkupId::new("marquee:rectangle").unwrap();
    let line_id = MarkupId::new("marquee:line").unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(
            76,
            vec![
                Annotation::Rectangle(RectangleAnnotation {
                    id: rectangle_id.clone(),
                    page_index: 0,
                    rect: PdfRect::new(10., 10., 10., 10.).unwrap(),
                    rotation_degrees: 0.,
                    appearance: RectangleAppearance::default(),
                    locked: false,
                }),
                Annotation::StraightLine(
                    StraightLineAnnotation::new(
                        line_id.clone(),
                        0,
                        point(60., 10.),
                        point(70., 20.),
                        LineKind::Line,
                        StraightLineAppearance::default_for(LineKind::Line),
                    )
                    .unwrap(),
                ),
            ],
        )
        .unwrap();

    let no_modifiers = PointerInputModifiers::default();
    assert_eq!(
        adapter
            .pointer_down_with_viewport_input(
                76,
                0,
                1,
                0,
                point(0., 0.),
                selection_point(0., 0.),
                2.,
                no_modifiers,
            )
            .unwrap(),
        PointerPhaseOutcome::GestureStarted,
    );
    adapter
        .pointer_move_with_viewport_input(1, point(6., 0.), selection_point(6., 0.), no_modifiers)
        .unwrap();
    assert_eq!(
        adapter
        .pointer_up_with_viewport_input(
            1,
            point(6., 0.),
            selection_point(6., 0.),
            no_modifiers,
        )
        .unwrap(),
        PointerPhaseOutcome::PlacementPending,
    );
    assert!(
        adapter.selected_ids(76).is_empty(),
        "exactly six CSS pixels must arm the box form without selecting"
    );
    assert!(adapter.is_click_placement_pending());
    assert_eq!(
        adapter
            .pointer_down_with_viewport_input(
                76,
                0,
                1,
                0,
                point(40., 40.),
                selection_point(40., 40.),
                2.,
                no_modifiers,
            )
            .unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(rectangle_id.clone())),
    );
    assert!(!adapter.is_click_placement_pending());
    adapter.clear_selection(76);

    adapter
        .pointer_down_with_viewport_input(
            76,
            0,
            9,
            0,
            point(0., 0.),
            selection_point(0., 0.),
            2.,
            no_modifiers,
        )
        .unwrap();
    adapter
        .pointer_move_with_viewport_input(
            9,
            point(40., 40.),
            selection_point(40., 40.),
            no_modifiers,
        )
        .unwrap();
    adapter.cancel(PointerCancelReason::CaptureLost).unwrap();
    assert!(adapter.active_selection_marquee(76).is_none());
    assert!(
        adapter.selected_ids(76).is_empty(),
        "cancellation must not apply a transient marquee"
    );

    adapter
        .pointer_down_with_viewport_input(
            76,
            0,
            2,
            0,
            point(0., 0.),
            selection_point(0., 0.),
            2.,
            no_modifiers,
        )
        .unwrap();
    for (pdf, viewport) in [
        (point(40., 0.), selection_point(40., 0.)),
        (point(40., 40.), selection_point(40., 40.)),
    ] {
        adapter
            .pointer_move_with_viewport_input(2, pdf, viewport, no_modifiers)
            .unwrap();
    }
    adapter
        .pointer_up_with_viewport_input(2, point(0., 40.), selection_point(0., 40.), no_modifiers)
        .unwrap();
    assert_eq!(adapter.selected_ids(76), &[rectangle_id.clone()]);

    let add = PointerInputModifiers {
        shift: true,
        alt: false,
    };
    adapter
        .pointer_down_with_viewport_input(
            76,
            0,
            3,
            0,
            point(50., 0.),
            selection_point(50., 0.),
            2.,
            add,
        )
        .unwrap();
    for (pdf, viewport) in [
        (point(80., 0.), selection_point(80., 0.)),
        (point(80., 30.), selection_point(80., 30.)),
    ] {
        adapter
            .pointer_move_with_viewport_input(3, pdf, viewport, add)
            .unwrap();
    }
    adapter
        .pointer_up_with_viewport_input(3, point(50., 30.), selection_point(50., 30.), add)
        .unwrap();
    assert_eq!(adapter.selected_ids(76), &[rectangle_id.clone(), line_id]);

    let remove = PointerInputModifiers {
        shift: true,
        alt: true,
    };
    adapter
        .pointer_down_with_viewport_input(
            76,
            0,
            4,
            0,
            point(0., 0.),
            selection_point(0., 0.),
            2.,
            remove,
        )
        .unwrap();
    for (pdf, viewport) in [
        (point(40., 0.), selection_point(40., 0.)),
        (point(40., 40.), selection_point(40., 40.)),
    ] {
        adapter
            .pointer_move_with_viewport_input(4, pdf, viewport, remove)
            .unwrap();
    }
    adapter
        .pointer_up_with_viewport_input(4, point(0., 40.), selection_point(0., 40.), remove)
        .unwrap();
    assert_eq!(
        adapter.selected_ids(76),
        &[MarkupId::new("marquee:line").unwrap()]
    );
}

#[test]
fn rectangle_tool_presentation_matches_the_electron_contract() {
    assert_eq!(AnnotationTool::Rectangle.label(), "Rectangle");
    assert_eq!(AnnotationTool::Rectangle.shortcut(), Some("R"));
    assert_eq!(AnnotationTool::Rectangle.tooltip_label(), "Rectangle (R)");
    assert!(AnnotationTool::Rectangle.uses_crosshair());
}

#[test]
fn plain_rectangle_and_select_shortcuts_resolve_without_stealing_modified_keys() {
    assert_eq!(
        AnnotationTool::from_plain_shortcut("r"),
        Some(AnnotationTool::Rectangle)
    );
    assert_eq!(
        AnnotationTool::from_plain_shortcut("R"),
        Some(AnnotationTool::Rectangle)
    );
    assert_eq!(
        AnnotationTool::from_plain_shortcut("v"),
        Some(AnnotationTool::Select)
    );
    assert_eq!(AnnotationTool::from_plain_shortcut("ctrl-r"), None);
    assert_eq!(AnnotationTool::from_plain_shortcut(""), None);
}

#[test]
fn rectangle_click_or_drag_keeps_a_live_draft_until_the_second_click() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Rectangle).unwrap();
    adapter.set_observed_pixels_per_point(2.0).unwrap();

    assert_eq!(
        adapter
            .pointer_down(31, 0, 7, point(100.0, 100.0), 4.0)
            .unwrap(),
        PointerPhaseOutcome::GestureStarted
    );
    assert_eq!(
        adapter.pointer_up(7, point(101.0, 101.0)).unwrap(),
        PointerPhaseOutcome::PlacementPending
    );
    assert!(adapter.is_click_placement_pending());
    assert_eq!(adapter.history_depths(31), (0, 0));

    assert_eq!(
        adapter.pointer_up(7, point(101.0, 101.0)).unwrap(),
        PointerPhaseOutcome::PlacementPending
    );
    assert!(adapter.is_click_placement_pending());
    assert_eq!(adapter.history_depths(31), (0, 0));

    adapter.pointer_move(7, point(140.0, 130.0)).unwrap();
    let preview = &adapter.document_scene(31, 0).rectangles[0];
    assert!(preview.preview);
    assert_eq!((preview.rect.x, preview.rect.y), (100.0, 100.0));
    assert_eq!((preview.rect.width, preview.rect.height), (40.0, 30.0));
    assert!(adapter.thumbnail_scene(31, 0).rectangles.is_empty());

    assert!(matches!(
        adapter
            .pointer_down(31, 0, 7, point(140.0, 130.0), 4.0)
            .unwrap(),
        PointerPhaseOutcome::AnnotationCreated(_)
    ));
    assert!(!adapter.is_click_placement_pending());
    assert_eq!(adapter.history_depths(31), (1, 0));
    let committed = &adapter.document_scene(31, 0).rectangles[0];
    assert!(!committed.preview);
    assert_eq!((committed.rect.width, committed.rect.height), (40.0, 30.0));
}

#[test]
fn cancelling_a_pending_rectangle_click_does_not_change_history() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Rectangle).unwrap();
    adapter
        .pointer_down(32, 0, 9, point(20.0, 30.0), 4.0)
        .unwrap();
    assert_eq!(
        adapter.pointer_up(9, point(20.0, 30.0)).unwrap(),
        PointerPhaseOutcome::PlacementPending
    );

    adapter
        .cancel(butter_paper_gpui_gallery::annotation_model::PointerCancelReason::ToolChanged)
        .unwrap();

    assert!(!adapter.is_click_placement_pending());
    assert!(adapter.document_scene(32, 0).rectangles.is_empty());
    assert_eq!(adapter.history_depths(32), (0, 0));
}

#[test]
fn rectangle_drag_requires_both_dimensions_to_exceed_two_pdf_points() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Rectangle).unwrap();

    adapter
        .pointer_down(33, 0, 11, point(10.0, 10.0), 4.0)
        .unwrap();
    assert_eq!(
        adapter.pointer_up(11, point(12.0, 50.0)).unwrap(),
        PointerPhaseOutcome::Ignored
    );

    assert!(adapter.document_scene(33, 0).rectangles.is_empty());
    assert_eq!(adapter.history_depths(33), (0, 0));
}

#[test]
fn selecting_and_moving_a_rectangle_previews_then_commits_one_edit() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Rectangle).unwrap();
    adapter
        .pointer_down(34, 0, 13, point(100.0, 100.0), 4.0)
        .unwrap();
    adapter.pointer_up(13, point(140.0, 130.0)).unwrap();
    let id = MarkupId::new("comparison:rectangle:1").unwrap();

    adapter.set_tool(AnnotationTool::Select).unwrap();
    assert_eq!(
        adapter
            .pointer_down(34, 0, 14, point(120.0, 115.0), 4.0)
            .unwrap(),
        PointerPhaseOutcome::GestureStarted
    );
    adapter.pointer_move(14, point(150.0, 135.0)).unwrap();

    let preview = &adapter.document_scene(34, 0).rectangles[0];
    assert!(preview.preview);
    assert_eq!((preview.rect.x, preview.rect.y), (130.0, 120.0));
    let thumbnail = &adapter.thumbnail_scene(34, 0).rectangles[0];
    assert_eq!((thumbnail.rect.x, thumbnail.rect.y), (100.0, 100.0));
    assert_eq!(adapter.history_depths(34), (1, 0));

    assert_eq!(
        adapter.pointer_up(14, point(150.0, 135.0)).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(id)
    );
    assert_eq!(adapter.history_depths(34), (2, 0));
    let committed = &adapter.document_scene(34, 0).rectangles[0];
    assert!(!committed.preview);
    assert_eq!((committed.rect.x, committed.rect.y), (130.0, 120.0));
}

#[test]
fn north_west_resize_flows_through_the_public_pointer_adapter() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Rectangle).unwrap();
    adapter
        .pointer_down(35, 0, 15, point(100.0, 100.0), 4.0)
        .unwrap();
    adapter.pointer_up(15, point(140.0, 130.0)).unwrap();
    adapter.set_tool(AnnotationTool::Select).unwrap();

    assert_eq!(
        adapter
            .pointer_down(35, 0, 16, point(100.0, 130.0), 4.0)
            .unwrap(),
        PointerPhaseOutcome::GestureStarted
    );
    adapter.pointer_move(16, point(90.0, 140.0)).unwrap();
    let preview = &adapter.document_scene(35, 0).rectangles[0];
    assert!(preview.preview);
    assert_eq!(
        (
            preview.rect.x,
            preview.rect.y,
            preview.rect.width,
            preview.rect.height
        ),
        (90.0, 100.0, 50.0, 40.0)
    );

    assert!(matches!(
        adapter.pointer_up(16, point(90.0, 140.0)).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(_)
    ));
    assert_eq!(adapter.history_depths(35), (2, 0));
}

#[test]
fn rotation_handle_keeps_a_twelve_css_pixel_offset_at_two_hundred_percent_zoom() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Rectangle).unwrap();
    adapter
        .pointer_down(36, 0, 17, point(100.0, 100.0), 2.0)
        .unwrap();
    adapter.pointer_up(17, point(140.0, 130.0)).unwrap();
    adapter.set_tool(AnnotationTool::Select).unwrap();
    adapter.set_observed_pixels_per_point(2.0).unwrap();

    assert_eq!(
        adapter
            .pointer_down(36, 0, 18, point(120.0, 136.0), 2.0)
            .unwrap(),
        PointerPhaseOutcome::GestureStarted
    );
}

fn rectangle_adapter(document_id: u64) -> AnnotationAdapter {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Rectangle).unwrap();
    adapter
        .pointer_down(document_id, 0, 1, point(100.0, 100.0), 4.0)
        .unwrap();
    adapter.pointer_up(1, point(140.0, 140.0)).unwrap();
    adapter.set_tool(AnnotationTool::Select).unwrap();
    adapter
}

#[test]
fn enabled_rectangle_translation_snap_uses_the_nearest_grid_delta() {
    let mut adapter = rectangle_adapter(21);
    adapter
        .set_rectangle_snap_settings(RectangleSnapSettings::new(true, 10.0, 2.0).unwrap())
        .unwrap();
    adapter.set_observed_pixels_per_point(1.0).unwrap();

    adapter
        .pointer_down(21, 0, 2, point(120.0, 120.0), 4.0)
        .unwrap();
    adapter.pointer_move(2, point(128.5, 131.5)).unwrap();
    adapter.pointer_up(2, point(128.5, 131.5)).unwrap();

    let rect = adapter.document_scene(21, 0).rectangles[0].rect;
    assert_eq!((rect.x, rect.y), (110.0, 110.0));
}

#[test]
fn disabled_rectangle_translation_snap_preserves_raw_translation() {
    let mut adapter = rectangle_adapter(22);
    adapter
        .set_rectangle_snap_settings(RectangleSnapSettings::new(false, 10.0, 100.0).unwrap())
        .unwrap();

    adapter
        .pointer_down(22, 0, 2, point(120.0, 120.0), 4.0)
        .unwrap();
    adapter.pointer_move(2, point(128.5, 131.5)).unwrap();
    adapter.pointer_up(2, point(128.5, 131.5)).unwrap();

    let rect = adapter.document_scene(22, 0).rectangles[0].rect;
    assert_eq!((rect.x, rect.y), (108.5, 111.5));
}

#[test]
fn rectangle_translation_snap_threshold_is_inclusive_l_infinity() {
    let settings = RectangleSnapSettings::new(true, 10.0, 4.0).unwrap();
    let mut boundary = rectangle_adapter(23);
    boundary.set_rectangle_snap_settings(settings).unwrap();
    boundary.set_observed_pixels_per_point(2.0).unwrap();
    boundary
        .pointer_down(23, 0, 2, point(120.0, 120.0), 4.0)
        .unwrap();
    boundary.pointer_up(2, point(128.0, 132.0)).unwrap();
    let boundary_rect = boundary.document_scene(23, 0).rectangles[0].rect;
    assert_eq!((boundary_rect.x, boundary_rect.y), (110.0, 110.0));

    let mut outside = rectangle_adapter(24);
    outside.set_rectangle_snap_settings(settings).unwrap();
    outside.set_observed_pixels_per_point(2.0).unwrap();
    outside
        .pointer_down(24, 0, 2, point(120.0, 120.0), 4.0)
        .unwrap();
    outside.pointer_up(2, point(127.9, 132.0)).unwrap();
    let outside_rect = outside.document_scene(24, 0).rectangles[0].rect;
    assert_eq!((outside_rect.x, outside_rect.y), (107.9, 112.0));
}

#[test]
fn rectangle_translation_snap_updates_preview_then_commits_one_history_entry() {
    let mut adapter = rectangle_adapter(25);
    adapter
        .set_rectangle_snap_settings(RectangleSnapSettings::new(true, 10.0, 2.0).unwrap())
        .unwrap();
    let history_before = adapter.history_depths(25);

    adapter
        .pointer_down(25, 0, 2, point(120.0, 120.0), 4.0)
        .unwrap();
    adapter.pointer_move(2, point(128.5, 131.5)).unwrap();
    assert_eq!(adapter.history_depths(25), history_before);
    assert_eq!(adapter.document_scene(25, 0).rectangles[0].rect.x, 110.0);

    adapter.pointer_up(2, point(128.5, 131.5)).unwrap();
    assert_eq!(adapter.history_depths(25), (history_before.0 + 1, 0));
}

#[test]
fn native_snap_replay_uses_the_ordinary_product_pointer_transaction() {
    let plan = NativeEditingV5Plan::embedded().unwrap().snap_transform;
    let mut adapter = AnnotationAdapter::default();
    let history_before = adapter.prepare_native_v5_snap(26, &plan).unwrap();

    adapter
        .begin_native_v5_snap(26, &plan, 7, 4.0, 8.0 / 7.0)
        .unwrap();
    assert_eq!(adapter.active_surface(), Some((26, plan.page_index)));
    assert_eq!(adapter.rectangle_snap_settings().enabled(), true);

    let observed_sample_indexes = [0, plan.sample_count / 2, plan.sample_count - 1];
    for index in observed_sample_indexes {
        let t = index as f64 / (plan.sample_count - 1) as f64;
        let point = point(
            plan.start.x + (plan.unsnapped_end.x - plan.start.x) * t,
            plan.start.y + (plan.unsnapped_end.y - plan.start.y) * t,
        );
        adapter.update_native_v5_snap(26, point).unwrap();
        assert_eq!(adapter.history_depths(26), history_before);
    }

    let receipt = adapter
        .commit_native_v5_snap(26, plan.unsnapped_end)
        .unwrap();
    assert_eq!(adapter.active_surface(), None);
    assert_eq!(receipt.sample_count, observed_sample_indexes.len());
    assert!(receipt.resolution.acquired);
    assert_eq!(receipt.final_rect, plan.expected_final_rect);
    assert_eq!(adapter.history_depths(26), (history_before.0 + 1, 0));
}

#[test]
fn text_length_and_image_workflows_use_frozen_comparison_defaults_and_shared_history() {
    let mut adapter = AnnotationAdapter::default();

    adapter.set_tool(AnnotationTool::TextBox).unwrap();
    adapter
        .pointer_down(9, 0, 1, point(90.0, 390.0), 4.0)
        .unwrap();
    let text = &adapter.document_scene(9, 0).text_boxes[0];
    assert_eq!(text.content, "Beam B-12 / revision 3");
    assert_eq!(
        (text.layout_rect.width, text.layout_rect.height),
        (240.0, 72.0)
    );
    adapter
        .replace_selected_text(9, "Beam B-12 / revision 4")
        .unwrap();
    adapter.resize_selected_text(9, 300.0, 84.0).unwrap();

    adapter.set_tool(AnnotationTool::Length).unwrap();
    adapter
        .set_document_page_length_calibration(
            9,
            0,
            LengthCalibration::from_scale(72., 1., "m", 2, true).unwrap(),
        )
        .unwrap();
    adapter
        .begin_length_placement(
            9,
            0,
            MarkupId::new("length:frozen-workflow").unwrap(),
            point(90.0, 510.0),
        )
        .unwrap();
    adapter
        .commit_length_placement(9, 0, point(306.0, 510.0), false)
        .unwrap();
    assert_eq!(adapter.document_scene(9, 0).lengths[0].caption, "3.00 m");
    adapter.set_tool(AnnotationTool::Select).unwrap();
    adapter
        .pointer_down(9, 0, 3, point(306.0, 510.0), 4.0)
        .unwrap();
    adapter.pointer_up(3, point(342.0, 510.0)).unwrap();
    assert_eq!(adapter.document_scene(9, 0).lengths[0].caption, "3.50 m");

    let checker = DecodedRgbaAsset::new(4, 3, vec![0x80; 4 * 3 * 4]).unwrap();
    let checker_id = checker.id().as_str().to_string();
    adapter.set_image_asset(checker);
    adapter
        .set_image_placement_page(612.0, 792.0, 0.45)
        .unwrap();
    adapter.set_tool(AnnotationTool::Image).unwrap();
    adapter
        .pointer_down(9, 0, 4, point(360.0, 390.0), 4.0)
        .unwrap();
    adapter.resize_selected_image(9, 180.0, 135.0).unwrap();
    let image = &adapter.document_scene(9, 0).images[0];
    assert_eq!(image.asset_id.as_str(), checker_id);
    assert_eq!((image.rect.width, image.rect.height), (180.0, 135.0));

    let thumbnail = adapter.thumbnail_scene(9, 0);
    assert_eq!(thumbnail.text_boxes.len(), 1);
    assert_eq!(thumbnail.lengths.len(), 1);
    assert_eq!(thumbnail.images.len(), 1);
    assert_eq!(
        adapter.history_depths(9),
        (8, 0),
        "the authoritative page scale is one document history entry"
    );
    assert!(adapter.is_dirty(9));
}

#[test]
fn exact_selected_measurement_caption_edit_handles_length_path_noop_lock_and_multi_selection() {
    let calibration = LengthCalibration::from_scale(72., 1., "m", 2, true).unwrap();
    let length_id = MarkupId::new("measurement-caption:length").unwrap();
    let path_id = MarkupId::new("measurement-caption:path").unwrap();
    let length = LengthAnnotation::new(
        length_id.clone(),
        0,
        point(0., 0.),
        point(72., 0.),
        calibration.clone(),
    )
    .unwrap();
    let path = MeasurementPathAnnotation::new(
        path_id.clone(),
        0,
        vec![point(0., 0.), point(72., 0.)],
        MeasurementPathKind::Polylength,
        calibration,
        RectangleAppearance::default(),
    )
    .unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(
            1091,
            vec![
                Annotation::Length(length),
                Annotation::MeasurementPath(path),
            ],
        )
        .unwrap();

    assert!(adapter.select_id(1091, &length_id));
    adapter
        .set_exact_selected_measurement_show_caption(1091, false)
        .unwrap();
    let after_length = adapter.snapshot(1091).unwrap();
    assert_eq!((after_length.revision, after_length.undo_depth), (1, 1));
    assert!(!after_length.lengths[0].calibration().show_caption());
    adapter
        .set_exact_selected_measurement_show_caption(1091, false)
        .unwrap();
    assert_eq!(adapter.history_depths(1091), (1, 0));

    assert!(adapter.select_id(1091, &path_id));
    adapter
        .set_exact_selected_measurement_show_caption(1091, false)
        .unwrap();
    let after_path = adapter.snapshot(1091).unwrap();
    assert_eq!((after_path.revision, after_path.undo_depth), (2, 2));
    assert!(!after_path.measurement_paths[0].calibration().show_caption());

    adapter.set_selected_locked(1091, true).unwrap();
    let locked_history = adapter.history_depths(1091);
    assert!(
        adapter
            .set_exact_selected_measurement_show_caption(1091, true)
            .is_err()
    );
    assert_eq!(adapter.history_depths(1091), locked_history);

    adapter.set_selected_locked(1091, false).unwrap();
    assert!(adapter.select_id(1091, &length_id));
    assert!(adapter.toggle_selection(1091, &path_id));
    let multi_history = adapter.history_depths(1091);
    assert!(
        adapter
            .set_exact_selected_measurement_show_caption(1091, true)
            .is_err()
    );
    assert_eq!(adapter.history_depths(1091), multi_history);
}

#[test]
fn exact_selected_text_box_style_is_atomic_noop_safe_locked_and_undoable() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::TextBox).unwrap();
    adapter
        .pointer_down(1090, 0, 1, point(90., 390.), 4.)
        .unwrap();
    let original = adapter.exact_selected_text_box(1090).unwrap().clone();
    let edited = TextBoxStyle::new(original.style().font_family(), 18., "#2563eb", 0.6)
        .unwrap()
        .with_weight_and_alignment(original.style().weight(), TextAlignment::Center)
        .unwrap();

    let before = adapter.history_depths(1090);
    adapter
        .set_exact_selected_text_box_style(1090, edited.clone())
        .unwrap();
    assert_eq!(adapter.history_depths(1090), (before.0 + 1, 0));
    let current = adapter.exact_selected_text_box(1090).unwrap();
    assert_eq!(current.style(), &edited);
    assert_eq!(current.content(), original.content());
    assert_eq!(current.layout_rect, original.layout_rect);
    assert_eq!(current.id, original.id);

    adapter
        .set_exact_selected_text_box_style(1090, edited)
        .unwrap();
    assert_eq!(adapter.history_depths(1090), (before.0 + 1, 0));
    adapter.undo(1090).unwrap();
    assert_eq!(adapter.exact_selected_text_box(1090).unwrap(), &original);
    adapter.redo(1090).unwrap();

    adapter.set_selected_locked(1090, true).unwrap();
    let locked_history = adapter.history_depths(1090);
    let rejected = TextBoxStyle::new("Helvetica", 20., "#ff0000", 1.).unwrap();
    assert!(
        adapter
            .set_exact_selected_text_box_style(1090, rejected)
            .is_err()
    );
    assert_eq!(adapter.history_depths(1090), locked_history);
}

#[test]
fn length_endpoint_drag_previews_without_committing_history() {
    let mut adapter = AnnotationAdapter::default();
    adapter
        .set_document_page_length_calibration(
            109,
            0,
            LengthCalibration::from_scale(72., 2., "m", 3, true).unwrap(),
        )
        .unwrap();
    let length_id = MarkupId::new("length:endpoint-preview").unwrap();
    adapter
        .begin_length_placement(109, 0, length_id.clone(), point(72., 192.))
        .unwrap();
    adapter
        .commit_length_placement(109, 0, point(216., 192.), false)
        .unwrap();
    adapter.set_tool(AnnotationTool::Select).unwrap();

    assert_eq!(
        adapter
            .pointer_down(109, 0, 37, point(216., 192.), 4.)
            .unwrap(),
        PointerPhaseOutcome::GestureStarted
    );
    let retained_before = adapter.snapshot(109).unwrap();
    let history_before = adapter.history_depths(109);
    assert_eq!(retained_before.selected_id.as_ref(), Some(&length_id));
    assert_eq!(retained_before.lengths[0].end, point(216., 192.));

    assert_eq!(
        adapter.pointer_move(37, point(180., 192.)).unwrap(),
        PointerPhaseOutcome::GestureStarted
    );
    let preview = &adapter.document_scene(109, 0).lengths[0];
    assert_eq!(preview.end, point(180., 192.));
    assert_eq!(preview.caption, "3.000 m");
    let retained_during_preview = adapter.snapshot(109).unwrap();
    assert_eq!(retained_during_preview.revision, retained_before.revision);
    assert_eq!(retained_during_preview.lengths[0].end, point(216., 192.));
    assert_eq!(adapter.history_depths(109), history_before);

    assert_eq!(
        adapter.pointer_up(37, point(180., 192.)).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(length_id)
    );
    let committed = adapter.snapshot(109).unwrap();
    assert_eq!(committed.lengths[0].end, point(180., 192.));
    assert_eq!(committed.lengths[0].caption(), "3.000 m");
    assert_eq!(committed.revision, retained_before.revision + 1);
    assert_eq!(adapter.history_depths(109), (history_before.0 + 1, 0));
}

#[test]
fn image_placement_preserves_natural_aspect_ratio_centers_and_clamps_to_the_page() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_image_asset(DecodedRgbaAsset::new(512, 384, vec![0x80; 512 * 384 * 4]).unwrap());
    adapter
        .set_image_placement_page(612.0, 792.0, 0.45)
        .unwrap();
    adapter.set_tool(AnnotationTool::Image).unwrap();

    adapter
        .pointer_down(13, 0, 1, point(432.0, 444.0), 4.0)
        .unwrap();
    let centered = adapter.document_scene(13, 0).images[0].rect;
    assert!(
        !adapter.document_scene(13, 0).images[0].aspect_locked,
        "the Electron regular-image contract permits free resize after natural-aspect placement",
    );
    assert_eq!(
        centered,
        butter_paper_gpui_gallery::annotation_model::PdfRect::new(294.3, 340.725, 275.4, 206.55)
            .unwrap()
    );

    adapter.queue_next_annotation_id(
        butter_paper_gpui_gallery::annotation_model::MarkupId::new("image-clamped").unwrap(),
    );
    adapter
        .pointer_down(13, 0, 2, point(600.0, 780.0), 4.0)
        .unwrap();
    let clamped = adapter
        .document_scene(13, 0)
        .images
        .into_iter()
        .find(|image| image.id.as_str() == "image-clamped")
        .unwrap()
        .rect;
    assert_eq!(
        clamped,
        butter_paper_gpui_gallery::annotation_model::PdfRect::new(336.6, 585.45, 275.4, 206.55)
            .unwrap()
    );
}

#[test]
fn regular_image_pointer_move_and_east_resize_use_the_three_pixel_threshold() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_image_asset(DecodedRgbaAsset::new(512, 384, vec![0x80; 512 * 384 * 4]).unwrap());
    adapter
        .set_image_placement_page(612.0, 792.0, 0.45)
        .unwrap();
    adapter.queue_next_annotation_id(MarkupId::new("image:pointer-journey").unwrap());
    adapter.set_tool(AnnotationTool::Image).unwrap();
    adapter
        .pointer_down(31, 0, 1, point(432.0, 444.0), 4.0)
        .unwrap();
    let original = adapter.document_scene(31, 0).images[0].rect;
    assert!(!adapter.document_scene(31, 0).images[0].aspect_locked);

    adapter.set_tool(AnnotationTool::Select).unwrap();
    let center = point(
        original.x + original.width / 2.0,
        original.y + original.height / 2.0,
    );
    assert_eq!(
        adapter.pointer_down(31, 0, 2, center, 4.0).unwrap(),
        PointerPhaseOutcome::GestureStarted,
    );
    assert_eq!(
        adapter
            .pointer_up(2, point(center.x + 2.999, center.y))
            .unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(
            MarkupId::new("image:pointer-journey").unwrap()
        )),
    );
    assert_eq!(adapter.document_scene(31, 0).images[0].rect, original);

    adapter.pointer_down(31, 0, 3, center, 4.0).unwrap();
    assert!(matches!(
        adapter
            .pointer_up(3, point(center.x + 3.0, center.y))
            .unwrap(),
        PointerPhaseOutcome::AnnotationEdited(_)
    ));
    let moved = adapter.document_scene(31, 0).images[0].rect;
    assert_eq!(moved.x, original.x + 3.0);
    assert_eq!(
        (moved.width, moved.height),
        (original.width, original.height)
    );

    let east = point(moved.x + moved.width, moved.y + moved.height / 2.0);
    assert_eq!(
        adapter.pointer_down(31, 0, 4, east, 4.0).unwrap(),
        PointerPhaseOutcome::GestureStarted,
    );
    adapter
        .pointer_move(4, point(east.x + 30.0, east.y))
        .unwrap();
    let preview = adapter.document_scene(31, 0).images[0].rect;
    assert_eq!(preview.width, moved.width + 30.0);
    assert_eq!(
        (preview.x, preview.y, preview.height),
        (moved.x, moved.y, moved.height)
    );
    assert!(matches!(
        adapter.pointer_up(4, point(east.x + 30.0, east.y)).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(_)
    ));
    assert_eq!(adapter.document_scene(31, 0).images[0].rect, preview);

    let resized = preview;
    let east = point(resized.x + resized.width, resized.y + resized.height / 2.0);
    adapter.pointer_down(31, 0, 5, east, 4.0).unwrap();
    adapter
        .pointer_move(5, point(east.x + 40.0, east.y))
        .unwrap();
    adapter
        .cancel(butter_paper_gpui_gallery::annotation_model::PointerCancelReason::CaptureLost)
        .unwrap();
    assert_eq!(adapter.document_scene(31, 0).images[0].rect, resized);

    adapter.set_selected_locked(31, true).unwrap();
    assert_eq!(
        adapter.pointer_down(31, 0, 6, east, 4.0).unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(
            MarkupId::new("image:pointer-journey").unwrap()
        )),
    );
}

#[test]
fn queued_text_seed_and_key_edits_commit_as_one_undoable_create_transaction() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::TextBox).unwrap();
    adapter.queue_next_text_content(" ");
    adapter
        .pointer_down(19, 0, 1, point(210.0, 426.0), 4.0)
        .unwrap();

    let content_before = adapter.selected_text(19).unwrap().to_owned();
    let history_before_typing = adapter.history_depths(19).0;
    assert_eq!(content_before, " ");
    assert_ne!(content_before, "Beam B-12 / revision 3");

    let expected = "Beam B-12 / revision 3";
    for byte_count in 1..=expected.len() {
        adapter
            .replace_selected_text_in_create_transaction(19, &expected[..byte_count])
            .unwrap();
    }

    assert_eq!(adapter.selected_text(19), Some(expected));
    assert_eq!(adapter.history_depths(19).0, history_before_typing);
    adapter.undo(19).unwrap();
    assert!(adapter.document_scene(19, 0).text_boxes.is_empty());
    adapter.redo(19).unwrap();
    assert_eq!(
        adapter.document_scene(19, 0).text_boxes[0].content,
        expected
    );
}

#[test]
fn all_annotation_types_share_lock_delete_undo_and_redo_commands() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Rectangle).unwrap();
    adapter
        .pointer_down(11, 0, 1, point(72.0, 144.0), 4.0)
        .unwrap();
    adapter.pointer_up(1, point(252.0, 240.0)).unwrap();

    adapter.set_selected_locked(11, true).unwrap();
    assert!(adapter.selected_is_locked(11));
    assert!(adapter.delete_selected(11).is_err());
    adapter.set_selected_locked(11, false).unwrap();
    adapter.delete_selected(11).unwrap();
    assert!(adapter.document_scene(11, 0).rectangles.is_empty());

    adapter.undo(11).unwrap();
    assert_eq!(adapter.document_scene(11, 0).rectangles.len(), 1);
    adapter.redo(11).unwrap();
    assert!(adapter.document_scene(11, 0).rectangles.is_empty());
}

#[test]
fn typed_rectangle_and_streamed_highlight_tools_commit_real_document_gestures() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Rectangle).unwrap();
    assert_eq!(adapter.tool(), AnnotationTool::Rectangle);
    assert!(matches!(
        adapter
            .pointer_down(7, 0, 1, point(72.0, 144.0), 4.0)
            .unwrap(),
        PointerPhaseOutcome::GestureStarted
    ));
    adapter.pointer_move(1, point(252.0, 240.0)).unwrap();
    adapter.pointer_up(1, point(252.0, 240.0)).unwrap();

    let rectangle_scene = adapter.document_scene(7, 0);
    assert_eq!(rectangle_scene.rectangles.len(), 1);
    assert_eq!(rectangle_scene.rectangles[0].rect.x, 72.0);
    assert_eq!(rectangle_scene.rectangles[0].rect.width, 180.0);

    adapter.set_tool(AnnotationTool::Highlight).unwrap();
    adapter
        .pointer_down(7, 0, 2, point(90.0, 330.0), 4.0)
        .unwrap();
    adapter.pointer_move(2, point(150.0, 337.0)).unwrap();
    adapter.pointer_move(2, point(220.0, 329.0)).unwrap();
    adapter.pointer_up(2, point(300.0, 334.0)).unwrap();

    let scene = adapter.document_scene(7, 0);
    assert_eq!(scene.pens.len(), 1);
    assert_eq!(scene.pens[0].tool, InkTool::Highlight);
    assert_eq!(scene.pens[0].blend_mode, BlendMode::Multiply);
    assert_eq!(
        adapter.highlight_paint_capability(),
        HighlightPaintCapability::SourceAlphaFallback
    );
    assert_eq!(scene.pens[0].appearance.color(), "#ffff00");
    assert_eq!(scene.pens[0].appearance.width_pt(), 12.0);
    assert_eq!(scene.pens[0].appearance.opacity(), 1.0);
    assert_eq!(scene.pens[0].points.len(), 4);
    assert_eq!(scene.pens[0].points.first(), Some(&point(90.0, 330.0)));
    assert_eq!(scene.pens[0].points.last(), Some(&point(300.0, 334.0)));
    assert_eq!(adapter.history_depths(7), (2, 0));
    assert!(adapter.is_dirty(7));
    assert_eq!(adapter.thumbnail_scene(7, 0).pens.len(), 1);
}

#[test]
fn exact_single_selected_ink_appearance_edit_preserves_ink_identity_and_history() {
    let pen_id = MarkupId::new("ink-properties:pen").unwrap();
    let highlight_id = MarkupId::new("ink-properties:highlight").unwrap();
    let pen = PenAnnotation::new_paths(
        pen_id.clone(),
        0,
        vec![
            vec![point(10., 10.), point(20., 20.)],
            vec![point(30., 30.), point(40., 40.)],
        ],
        PenAppearance::new("#ff0000", 1., 1.).unwrap(),
        false,
    )
    .unwrap();
    let highlight = PenAnnotation::new_highlight(
        highlight_id.clone(),
        0,
        vec![point(50., 50.), point(80., 55.)],
        PenAppearance::new("#ffff00", 12., 1.).unwrap(),
    )
    .unwrap();
    let pen_paths = pen.paths().map(<[_]>::to_vec).collect::<Vec<_>>();

    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(91, vec![Annotation::Pen(pen), Annotation::Pen(highlight)])
        .unwrap();
    assert!(adapter.select_id(91, &pen_id));
    let selected = adapter
        .exact_selected_ink(91)
        .expect("one selected Pen must be exposed");
    assert_eq!(
        (selected.id.clone(), selected.tool()),
        (pen_id.clone(), InkTool::Pen)
    );

    let edited = PenAppearance::new("#ABCDEF", 3.25, 0.4).unwrap();
    adapter
        .set_exact_selected_ink_appearance(91, edited.clone())
        .unwrap();
    let snapshot = adapter.snapshot(91).unwrap();
    let pen = snapshot.pens.iter().find(|pen| pen.id == pen_id).unwrap();
    assert_eq!(pen.appearance, edited);
    assert_eq!(
        pen.paths().map(<[_]>::to_vec).collect::<Vec<_>>(),
        pen_paths
    );
    assert!(!pen.smooth_curves);
    assert_eq!(
        (pen.tool(), pen.blend_mode(), pen.locked),
        (InkTool::Pen, BlendMode::Normal, false)
    );
    assert_eq!((snapshot.revision, snapshot.undo_depth), (1, 1));

    adapter
        .set_exact_selected_ink_appearance(91, pen.appearance.clone())
        .unwrap();
    assert_eq!(
        adapter.history_depths(91),
        (1, 0),
        "a no-op must not add history"
    );

    assert!(adapter.toggle_selection(91, &highlight_id));
    assert!(adapter.exact_selected_ink(91).is_none());
    assert!(
        adapter
            .set_exact_selected_ink_appearance(91, PenAppearance::new("#000000", 2., 0.5).unwrap(),)
            .is_err()
    );
    assert_eq!(adapter.history_depths(91), (1, 0));
}

#[test]
fn highlight_defaults_are_application_owned_and_threshold_uses_observed_css_scale() {
    let mut adapter = AnnotationAdapter::default();
    let defaults = PenAppearance::new("#00ff00", 18., 0.5).unwrap();
    adapter.set_highlight_appearance(defaults.clone()).unwrap();
    assert_eq!(adapter.highlight_appearance(), defaults);
    adapter.set_observed_pixels_per_point(2.).unwrap();
    adapter.set_tool(AnnotationTool::Highlight).unwrap();

    adapter.queue_next_annotation_id(MarkupId::new("highlight:below-scaled-threshold").unwrap());
    adapter.pointer_down(88, 0, 1, point(10., 10.), 4.).unwrap();
    assert_eq!(
        adapter.pointer_up(1, point(11.499, 10.)).unwrap(),
        PointerPhaseOutcome::Ignored
    );
    assert!(adapter.snapshot(88).unwrap().pens.is_empty());

    adapter.queue_next_annotation_id(MarkupId::new("highlight:exact-scaled-threshold").unwrap());
    adapter.pointer_down(88, 0, 2, point(10., 10.), 4.).unwrap();
    assert!(matches!(
        adapter.pointer_up(2, point(11.5, 10.)).unwrap(),
        PointerPhaseOutcome::AnnotationCreated(_)
    ));
    assert_eq!(adapter.snapshot(88).unwrap().pens[0].appearance, defaults);
}

#[test]
fn vertex_paths_preserve_click_threshold_closure_pointer_edit_lock_and_history_contract() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_observed_pixels_per_point(2.).unwrap();

    let polyline_id = MarkupId::new("vertex-path:polyline:pointer-journey").unwrap();
    adapter.set_tool(AnnotationTool::Polyline).unwrap();
    adapter.queue_next_annotation_id(polyline_id.clone());
    assert_eq!(
        adapter
            .pointer_down(73, 0, 1, point(72.125, 144.375), 4.)
            .unwrap(),
        PointerPhaseOutcome::PlacementPending,
    );
    adapter
        .pointer_down(73, 0, 2, point(72.624, 144.375), 4.)
        .unwrap();
    assert_eq!(
        adapter.document_scene(73, 0).vertex_paths[0].points.len(),
        1,
        "adjacent movement below one-half PDF point must not add a vertex",
    );
    for (pointer_id, vertex) in [(3, point(180.25, 220.625)), (4, point(300.875, 160.125))] {
        adapter.pointer_down(73, 0, pointer_id, vertex, 4.).unwrap();
    }
    assert_eq!(
        adapter.finish_vertex_path(73).unwrap(),
        PointerPhaseOutcome::AnnotationCreated(polyline_id.clone()),
    );
    let created = adapter.snapshot(73).unwrap();
    assert_eq!(created.vertex_paths.len(), 1);
    assert_eq!(created.vertex_paths[0].kind, VertexPathKind::Polyline);
    assert_eq!(created.vertex_paths[0].points().len(), 3);
    assert_eq!(created.selected_id.as_ref(), Some(&polyline_id));

    adapter.set_tool(AnnotationTool::Select).unwrap();
    let original_middle = created.vertex_paths[0].points()[1];
    assert_eq!(
        adapter.pointer_down(73, 0, 5, original_middle, 4.).unwrap(),
        PointerPhaseOutcome::GestureStarted,
    );
    assert_eq!(
        adapter
            .pointer_up(5, point(original_middle.x + 1.49, original_middle.y))
            .unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(polyline_id.clone())),
        "movement below three CSS pixels must not edit a vertex",
    );
    assert_eq!(
        adapter.snapshot(73).unwrap().vertex_paths[0].points()[1],
        original_middle,
    );

    adapter.pointer_down(73, 0, 6, original_middle, 4.).unwrap();
    let edited_middle = point(original_middle.x + 1.5, original_middle.y);
    assert_eq!(
        adapter.pointer_up(6, edited_middle).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(polyline_id.clone()),
        "movement at exactly three CSS pixels must edit the stable vertex",
    );
    assert_eq!(
        adapter.snapshot(73).unwrap().vertex_paths[0].points()[1],
        edited_middle,
    );

    let segment_hit = point(
        (created.vertex_paths[0].points()[0].x + edited_middle.x) / 2.,
        (created.vertex_paths[0].points()[0].y + edited_middle.y) / 2.,
    );
    adapter.pointer_down(73, 0, 7, segment_hit, 4.).unwrap();
    let body_delta = point(segment_hit.x + 12., segment_hit.y - 6.);
    assert_eq!(
        adapter.pointer_up(7, body_delta).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(polyline_id.clone()),
    );
    let moved = adapter.snapshot(73).unwrap();
    assert_eq!(
        moved.vertex_paths[0].points()[0],
        point(
            created.vertex_paths[0].points()[0].x + 12.,
            created.vertex_paths[0].points()[0].y - 6.
        ),
    );

    adapter.set_selected_locked(73, true).unwrap();
    let locked = adapter.snapshot(73).unwrap();
    let locked_vertex = locked.vertex_paths[0].points()[0];
    assert_eq!(
        adapter.pointer_down(73, 0, 8, locked_vertex, 4.).unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(polyline_id.clone())),
    );
    assert_eq!(adapter.snapshot(73).unwrap(), locked);
    adapter.undo(73).unwrap();
    assert!(!adapter.snapshot(73).unwrap().vertex_paths[0].locked);
    adapter.redo(73).unwrap();
    assert!(adapter.snapshot(73).unwrap().vertex_paths[0].locked);

    let polygon_id = MarkupId::new("vertex-path:polygon:pointer-journey").unwrap();
    adapter.set_tool(AnnotationTool::Polygon).unwrap();
    adapter.queue_next_annotation_id(polygon_id.clone());
    for (pointer_id, vertex) in [
        (9, point(90., 360.)),
        (10, point(220., 500.)),
        (11, point(360., 340.)),
    ] {
        assert_eq!(
            adapter.pointer_down(73, 0, pointer_id, vertex, 4.).unwrap(),
            PointerPhaseOutcome::PlacementPending,
        );
    }
    assert_eq!(
        adapter
            .pointer_down(73, 0, 12, point(94.999, 360.), 4.)
            .unwrap(),
        PointerPhaseOutcome::AnnotationCreated(polygon_id.clone()),
        "a click within ten CSS pixels of the first vertex must close the Polygon",
    );
    let polygon = adapter
        .snapshot(73)
        .unwrap()
        .vertex_paths
        .into_iter()
        .find(|path| path.id == polygon_id)
        .unwrap();
    assert_eq!(polygon.kind, VertexPathKind::Polygon);
    assert_eq!(
        polygon.points().len(),
        3,
        "closure must not duplicate the first point"
    );
    assert!(
        VertexPathAnnotation::new(
            MarkupId::new("vertex-path:polygon:collapsed-closing-edge").unwrap(),
            0,
            vec![point(10., 10.), point(60., 10.), point(10.499, 10.)],
            VertexPathKind::Polygon,
            RectangleAppearance::default(),
        )
        .is_err(),
        "Polygon validation must apply the half-point spacing rule to its closing edge",
    );
}

#[test]
fn cloud_paths_preserve_ten_pixel_closure_intensity_and_stable_vertex_identity() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_observed_pixels_per_point(2.).unwrap();
    let cloud_id = MarkupId::new("cloud:pointer-journey").unwrap();
    adapter.set_tool(AnnotationTool::Cloud).unwrap();
    adapter.queue_next_annotation_id(cloud_id.clone());
    for (pointer_id, vertex) in [
        (1, point(90., 360.)),
        (2, point(220., 500.)),
        (3, point(360., 340.)),
    ] {
        assert_eq!(
            adapter.pointer_down(74, 0, pointer_id, vertex, 4.).unwrap(),
            PointerPhaseOutcome::PlacementPending,
        );
    }
    assert_eq!(
        adapter
            .pointer_down(74, 0, 4, point(94.999, 360.), 4.)
            .unwrap(),
        PointerPhaseOutcome::AnnotationCreated(cloud_id.clone()),
        "a click within ten CSS pixels of the first node must close the Cloud",
    );
    let created = adapter.snapshot(74).unwrap();
    assert_eq!(created.clouds.len(), 1);
    assert_eq!(created.clouds[0].id, cloud_id);
    assert_eq!(created.clouds[0].border_effect_intensity(), 2.);
    assert_eq!(created.clouds[0].points().len(), 3);
    assert!(created.clouds[0].scallop_path().len() > created.clouds[0].points().len());

    adapter.set_tool(AnnotationTool::Select).unwrap();
    adapter.select_id(74, &cloud_id);
    let moved_vertex = point(240., 480.);
    adapter
        .set_selected_cloud_point(74, 1, moved_vertex)
        .unwrap();
    adapter.translate_selected_cloud(74, 12., -6.).unwrap();
    let edited = adapter.snapshot(74).unwrap();
    assert_eq!(edited.clouds[0].id, cloud_id);
    assert_eq!(edited.clouds[0].points()[1], point(252., 474.));
    assert_eq!(edited.annotation_order, vec![cloud_id.clone()]);
    adapter.set_selected_locked(74, true).unwrap();
    assert!(
        adapter
            .set_selected_cloud_point(74, 0, point(100., 100.))
            .is_err()
    );
    adapter.undo(74).unwrap();
    assert!(!adapter.snapshot(74).unwrap().clouds[0].locked);
    adapter.redo(74).unwrap();
    assert!(adapter.snapshot(74).unwrap().clouds[0].locked);
}

#[test]
fn cloud_plus_node_and_drag_creation_keep_one_composite_identity_and_reroute() {
    assert_eq!(AnnotationTool::CloudPlus.label(), "Cloud+");
    assert_eq!(AnnotationTool::CloudPlus.shortcut(), Some("K"));
    assert_eq!(AnnotationTool::CloudPlus.toolbar_id(), "tool-cloud-plus");
    assert_eq!(
        AnnotationTool::from_plain_shortcut("k"),
        Some(AnnotationTool::CloudPlus)
    );

    let mut adapter = AnnotationAdapter::default();
    adapter.set_observed_pixels_per_point(2.).unwrap();
    adapter.set_tool(AnnotationTool::CloudPlus).unwrap();
    let node_id = MarkupId::new("cloud-plus:nodes").unwrap();
    adapter.queue_next_annotation_id(node_id.clone());
    assert_eq!(
        adapter
            .pointer_down(75, 0, 1, point(90., 360.), 4.)
            .unwrap(),
        PointerPhaseOutcome::GestureStarted
    );
    assert_eq!(
        adapter.pointer_up(1, point(90., 360.)).unwrap(),
        PointerPhaseOutcome::PlacementPending
    );
    for (pointer_id, vertex) in [(2, point(220., 500.)), (3, point(360., 340.))] {
        assert_eq!(
            adapter.pointer_down(75, 0, pointer_id, vertex, 4.).unwrap(),
            PointerPhaseOutcome::PlacementPending,
        );
    }
    assert_eq!(
        adapter
            .pointer_down(75, 0, 4, point(94.999, 360.), 4.)
            .unwrap(),
        PointerPhaseOutcome::AnnotationCreated(node_id.clone()),
    );
    let created = adapter.snapshot(75).unwrap();
    assert_eq!(created.cloud_pluses.len(), 1);
    assert!(created.clouds.is_empty());
    assert!(created.callouts.is_empty());
    let cloud_plus = &created.cloud_pluses[0];
    assert_eq!(cloud_plus.id, node_id);
    assert_eq!(cloud_plus.cloud_points().len(), 3);
    assert_eq!(cloud_plus.leader_points().len(), 3);
    assert_eq!(cloud_plus.text_box.width, 150.);
    assert_eq!(cloud_plus.text_box.height, 44.);
    assert_eq!(cloud_plus.content(), "Cloud+");

    adapter.select_id(75, &node_id);
    let old_leader = cloud_plus.leader_points().to_vec();
    adapter
        .set_selected_cloud_plus_cloud_point(75, 1, point(250., 480.))
        .unwrap();
    let edited = adapter.snapshot(75).unwrap();
    assert_eq!(edited.cloud_pluses[0].id, node_id);
    assert_ne!(edited.cloud_pluses[0].leader_points(), old_leader);
    assert_eq!(edited.annotation_order, vec![node_id.clone()]);

    let rectangle_id = MarkupId::new("cloud-plus:rectangle").unwrap();
    adapter.clear_selection(75);
    adapter.queue_next_annotation_id(rectangle_id.clone());
    assert_eq!(
        adapter.pointer_down(75, 0, 9, point(20., 20.), 4.).unwrap(),
        PointerPhaseOutcome::GestureStarted
    );
    adapter.pointer_move(9, point(100., 80.)).unwrap();
    assert_eq!(
        adapter.pointer_up(9, point(100., 80.)).unwrap(),
        PointerPhaseOutcome::AnnotationCreated(rectangle_id.clone())
    );
    let snapshot = adapter.snapshot(75).unwrap();
    assert_eq!(snapshot.cloud_pluses.len(), 2);
    let rectangle = snapshot
        .cloud_pluses
        .iter()
        .find(|annotation| annotation.id == rectangle_id)
        .unwrap();
    assert_eq!(
        rectangle.cloud_points(),
        &[
            point(20., 20.),
            point(100., 20.),
            point(100., 80.),
            point(20., 80.)
        ]
    );
}

#[test]
fn measurement_paths_require_scale_and_preserve_enter_escape_and_identity_contract() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_tool(AnnotationTool::Polylength).unwrap();
    let error = adapter
        .pointer_down(94, 0, 1, point(10., 10.), 4.)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        format!("invalid geometry: {LENGTH_SCALE_REQUIRED_MESSAGE}")
    );
    assert!(!adapter.measurement_path_pending(94));
    assert!(adapter.snapshot(94).is_none());

    let scale = PageScale::from_factors(
        0,
        ScaleSource::Custom,
        "1 in = 2 ft",
        ScaleUnit::In,
        ScaleUnit::Ft,
        2. / 72.,
        2. / 72.,
        ScalePrecision::decimal(0.01).unwrap(),
    )
    .unwrap();
    adapter
        .load_imported_annotations_with_page_scale_state(
            94,
            Vec::new(),
            vec![(0, scale.clone())],
            Vec::new(),
            Vec::new(),
        )
        .unwrap();

    let polylength_id = MarkupId::new("measurement:polylength:pointer").unwrap();
    adapter.queue_next_annotation_id(polylength_id.clone());
    for (pointer_id, vertex) in [
        (2, point(72., 72.)),
        (3, point(144., 72.)),
        (4, point(144., 144.)),
    ] {
        assert_eq!(
            adapter.pointer_down(94, 0, pointer_id, vertex, 4.).unwrap(),
            PointerPhaseOutcome::PlacementPending,
        );
    }
    assert_eq!(
        adapter.finish_measurement_path(94).unwrap(),
        PointerPhaseOutcome::AnnotationCreated(polylength_id.clone()),
    );
    let after_polylength = adapter.snapshot(94).unwrap();
    assert_eq!(after_polylength.measurement_paths.len(), 1);
    assert_eq!(
        after_polylength.measurement_paths[0].kind,
        MeasurementPathKind::Polylength
    );
    assert_eq!(after_polylength.measurement_paths[0].caption(), "4.00 ft");
    assert_eq!(after_polylength.selected_id.as_ref(), Some(&polylength_id));

    let cancelled_area_id = MarkupId::new("measurement:area:cancelled").unwrap();
    adapter.set_tool(AnnotationTool::Area).unwrap();
    adapter.queue_next_annotation_id(cancelled_area_id);
    adapter
        .pointer_down(94, 0, 5, point(200., 200.), 4.)
        .unwrap();
    adapter
        .pointer_down(94, 0, 6, point(260., 200.), 4.)
        .unwrap();
    assert_eq!(
        adapter.finish_measurement_path(94).unwrap(),
        PointerPhaseOutcome::Ignored,
        "insufficient Enter must leave the Area draft active",
    );
    assert!(adapter.measurement_path_pending(94));
    assert_eq!(
        adapter.cancel_measurement_path(94).unwrap(),
        PointerPhaseOutcome::Ignored,
    );
    assert!(!adapter.measurement_path_pending(94));
    assert_eq!(adapter.snapshot(94).unwrap().measurement_paths.len(), 1);

    let area_id = MarkupId::new("measurement:area:pointer").unwrap();
    adapter.queue_next_annotation_id(area_id.clone());
    for (pointer_id, vertex) in [
        (7, point(200., 200.)),
        (8, point(272., 200.)),
        (9, point(272., 272.)),
    ] {
        adapter.pointer_down(94, 0, pointer_id, vertex, 4.).unwrap();
    }
    assert_eq!(
        adapter.finish_measurement_path(94).unwrap(),
        PointerPhaseOutcome::AnnotationCreated(area_id.clone()),
    );
    let created = adapter.snapshot(94).unwrap();
    assert_eq!(created.measurement_paths.len(), 2);
    assert_eq!(created.measurement_paths[0].id, polylength_id);
    assert_eq!(created.measurement_paths[1].id, area_id);
    assert_eq!(created.measurement_paths[1].kind, MeasurementPathKind::Area);
    assert_eq!(created.measurement_paths[1].caption(), "2.00 ft^2");
}

#[test]
fn measurement_paths_preserve_double_click_selection_edit_lock_and_history_contract() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_observed_pixels_per_point(2.).unwrap();
    let scale = PageScale::from_factors(
        0,
        ScaleSource::Custom,
        "1 in = 2 ft",
        ScaleUnit::In,
        ScaleUnit::Ft,
        2. / 72.,
        2. / 72.,
        ScalePrecision::decimal(0.01).unwrap(),
    )
    .unwrap();
    adapter
        .load_imported_annotations_with_page_scale_state(
            95,
            Vec::new(),
            vec![(0, scale)],
            Vec::new(),
            Vec::new(),
        )
        .unwrap();

    let polylength_id = MarkupId::new("measurement:polylength:double-click").unwrap();
    adapter.set_tool(AnnotationTool::Polylength).unwrap();
    adapter.queue_next_annotation_id(polylength_id.clone());
    adapter.pointer_down(95, 0, 1, point(72., 72.), 4.).unwrap();
    adapter
        .pointer_down(95, 0, 2, point(144., 72.), 4.)
        .unwrap();
    assert_eq!(
        adapter
            .pointer_double_click(95, 0, point(144., 144.), 4.)
            .unwrap(),
        PointerPhaseOutcome::AnnotationCreated(polylength_id.clone()),
    );
    let created = adapter.snapshot(95).unwrap();
    assert_eq!(created.measurement_paths[0].points().len(), 3);
    assert_eq!(created.measurement_paths[0].caption(), "4.00 ft");

    adapter.set_tool(AnnotationTool::Select).unwrap();
    assert_eq!(adapter.selected_kind(95), Some(AnnotationKind::Polylength));
    let original_middle = created.measurement_paths[0].points()[1];
    assert_eq!(
        adapter.pointer_down(95, 0, 3, original_middle, 4.).unwrap(),
        PointerPhaseOutcome::GestureStarted,
    );
    let edited_middle = point(original_middle.x + 1.5, original_middle.y);
    assert_eq!(
        adapter.pointer_up(3, edited_middle).unwrap(),
        PointerPhaseOutcome::AnnotationEdited(polylength_id.clone()),
    );
    assert_eq!(
        adapter.snapshot(95).unwrap().measurement_paths[0].points()[1],
        edited_middle,
    );

    let segment_hit = point((72. + edited_middle.x) / 2., 72.);
    adapter.pointer_down(95, 0, 4, segment_hit, 4.).unwrap();
    assert_eq!(
        adapter
            .pointer_up(4, point(segment_hit.x + 12., segment_hit.y - 6.))
            .unwrap(),
        PointerPhaseOutcome::AnnotationEdited(polylength_id.clone()),
    );
    let moved = adapter.snapshot(95).unwrap();
    assert_eq!(moved.measurement_paths[0].points()[0], point(84., 66.));

    adapter.set_selected_locked(95, true).unwrap();
    assert!(adapter.selected_is_locked(95));
    let locked = adapter.snapshot(95).unwrap();
    assert_eq!(
        adapter
            .pointer_down(95, 0, 5, locked.measurement_paths[0].points()[0], 4.)
            .unwrap(),
        PointerPhaseOutcome::SelectionChanged(Some(polylength_id)),
    );
    assert_eq!(adapter.snapshot(95).unwrap(), locked);
    adapter.undo(95).unwrap();
    assert!(!adapter.snapshot(95).unwrap().measurement_paths[0].locked);
    adapter.redo(95).unwrap();
    assert!(adapter.snapshot(95).unwrap().measurement_paths[0].locked);

    let area_id = MarkupId::new("measurement:area:interior-hit").unwrap();
    adapter.set_tool(AnnotationTool::Area).unwrap();
    adapter.queue_next_annotation_id(area_id.clone());
    for (pointer_id, vertex) in [
        (6, point(240., 240.)),
        (7, point(312., 240.)),
        (8, point(312., 312.)),
    ] {
        adapter.pointer_down(95, 0, pointer_id, vertex, 4.).unwrap();
    }
    adapter.finish_measurement_path(95).unwrap();
    adapter.set_tool(AnnotationTool::Select).unwrap();
    let interior = point(294., 258.);
    assert_eq!(
        adapter.pointer_down(95, 0, 9, interior, 4.).unwrap(),
        PointerPhaseOutcome::GestureStarted,
        "Area interior must remain a body hit even without a fill color",
    );
    assert_eq!(
        adapter
            .pointer_up(9, point(interior.x + 6., interior.y + 6.))
            .unwrap(),
        PointerPhaseOutcome::AnnotationEdited(area_id.clone()),
    );
    assert_eq!(adapter.selected_kind(95), Some(AnnotationKind::Area));
    assert_eq!(adapter.snapshot(95).unwrap().selected_id, Some(area_id));
}
