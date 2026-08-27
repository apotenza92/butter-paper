use std::{
    fs,
    path::{Path, PathBuf},
    process::{self, Command},
    time::{SystemTime, UNIX_EPOCH},
};

use butter_paper_gpui_gallery::{
    annotation_adapter::ellipse_resize_handle_point_for_rect,
    annotation_model::{
        Annotation, ArcAnnotation, CalloutAnnotation, CalloutAppearance, CloudAnnotation,
        CloudPlusAnnotation, CloudPlusAppearance, DecodedRgbaAsset, DimensionAnnotation,
        DimensionAppearance, EllipseAnnotation, ImageAnnotation, LengthAnnotation,
        LengthCalibration, LineKind, MarkupId, MeasurementPathAnnotation, MeasurementPathKind,
        PageScale, PdfPoint, PdfRect, PenAnnotation, PenAppearance, RectangleAnnotation,
        RectangleAppearance, RectangleResizeHandle, RedactAnnotation, ScalePrecision,
        ScaleSource, ScaleUnit, SnapshotAnnotation, StraightLineAnnotation,
        StraightLineAppearance, StrokeStyle, TextBoxAnnotation, TextBoxStyle, VertexPathKind,
    },
    pdf_engine::{PdfPersistenceError, PdfPersistenceSession},
};

#[test]
fn snapshot_create_edit_delete_reopens_and_preserves_external_stamps_and_owned_resources() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("snapshot-source.pdf");
    let first = scratch.path().join("snapshot-first.pdf");
    let second = scratch.path().join("snapshot-second.pdf");
    let removed = scratch.path().join("snapshot-removed.pdf");
    snapshot_compatibility_fixture(&source);
    let source_bytes = fs::read(&source).unwrap();
    let source_document = original_document_oracle(&source);
    let source_probes = snapshot_preservation_oracle(&source);
    let asset = DecodedRgbaAsset::new(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 192, 0, 0, 255, 128, 255, 255, 255, 0,
        ],
    )
    .unwrap();
    let id = MarkupId::new("snapshot:persistence-1").unwrap();
    let mut created = SnapshotAnnotation::new(
        id.clone(),
        0,
        PdfRect::new(144., 216., 180., 72.).unwrap(),
        asset.clone(),
        0.65,
    )
    .unwrap()
    .with_rotation_degrees(22.)
    .unwrap();
    created.locked = true;

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    assert!(session.snapshots().is_empty());
    assert!(session.untouched_annotations().iter().any(|annotation| {
        annotation.name == "vendor-snapshot" && annotation.subtype == "Stamp"
    }));
    assert!(session.untouched_annotations().iter().any(|annotation| {
        annotation.name == "bp:malformed-snapshot" && annotation.subtype == "Stamp"
    }));
    session.add_snapshot(created.clone()).unwrap();
    assert!(session.snapshot_has_canonical_native_identity(&id));
    session.save_as(&first).unwrap();
    validate_independently(&first);
    assert_eq!(fs::read(&source).unwrap(), source_bytes);
    assert_eq!(original_document_oracle(&first), source_document);
    assert_eq!(snapshot_preservation_oracle(&first), source_probes);

    let first_document = Document::load(&first).unwrap();
    let raw = annotation_dictionary(&first_document, "bp:snapshot:persistence-1");
    assert_eq!(raw.get(b"Type").unwrap().as_name().unwrap(), b"Annot");
    assert_eq!(raw.get(b"Subtype").unwrap().as_name().unwrap(), b"Stamp");
    assert_eq!(raw.get(b"IT").unwrap().as_name().unwrap(), b"StampSnapshot");
    assert_eq!(raw.get(b"Subj").unwrap().as_str().unwrap(), b"Snapshot");
    assert_eq!(raw.get(b"Contents").unwrap().as_str().unwrap(), b"");
    assert_eq!(raw.get(b"F").unwrap().as_i64().unwrap(), 132);
    assert!((raw.get(b"CA").unwrap().as_float().unwrap() - 0.65).abs() < 0.000_01);
    assert!((raw.get(b"ca").unwrap().as_float().unwrap() - 0.65).abs() < 0.000_01);
    assert!((raw.get(b"Rotation").unwrap().as_float().unwrap() - 22.).abs() < 0.000_01);
    assert_eq!(raw.get(b"Rect").unwrap().as_array().unwrap().len(), 4);
    let first_graph = image_appearance_graph_ids(&first_document, raw);
    let first_form = first_document
        .get_object(first_graph[0])
        .unwrap()
        .as_stream()
        .unwrap();
    assert_eq!(
        first_form.dict.get(b"Type").unwrap().as_name().unwrap(),
        b"XObject"
    );
    assert_eq!(
        first_form.dict.get(b"Subtype").unwrap().as_name().unwrap(),
        b"Form"
    );
    let first_image = first_document
        .get_object(first_graph[1])
        .unwrap()
        .as_stream()
        .unwrap();
    assert_eq!(
        first_image.dict.get(b"Subtype").unwrap().as_name().unwrap(),
        b"Image"
    );
    assert_eq!(
        first_image
            .dict
            .get(b"ColorSpace")
            .unwrap()
            .as_name()
            .unwrap(),
        b"DeviceRGB"
    );
    assert_eq!(
        first_image.content,
        vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]
    );
    let first_alpha = first_document
        .get_object(first_graph[2])
        .unwrap()
        .as_stream()
        .unwrap();
    assert_eq!(
        first_alpha.dict.get(b"Subtype").unwrap().as_name().unwrap(),
        b"Image"
    );
    assert_eq!(
        first_alpha
            .dict
            .get(b"ColorSpace")
            .unwrap()
            .as_name()
            .unwrap(),
        b"DeviceGray"
    );
    assert_eq!(first_alpha.content, vec![255, 192, 128, 0]);

    let mut first_reopen = PdfPersistenceSession::open(&first).unwrap();
    assert_eq!(first_reopen.snapshots().len(), 1);
    assert_eq!(first_reopen.snapshots()[0].id, id);
    assert_eq!(first_reopen.snapshots()[0].asset(), &asset);
    assert!((first_reopen.snapshots()[0].opacity() - 0.65).abs() < 0.000_01);
    assert!((first_reopen.snapshots()[0].rotation_degrees() - 22.).abs() < 0.000_01);
    assert!(first_reopen.snapshots()[0].locked);
    assert!(first_reopen.snapshot_has_canonical_native_identity(&id));
    let edited = SnapshotAnnotation::new(
        id.clone(),
        0,
        PdfRect::new(126., 198., 216., 108.).unwrap(),
        asset.clone(),
        0.4,
    )
    .unwrap()
    .with_rotation_degrees(315.)
    .unwrap();
    first_reopen.replace_snapshot(edited).unwrap();
    first_reopen.save_as(&second).unwrap();
    validate_independently(&second);
    let second_document = Document::load(&second).unwrap();
    assert_eq!(snapshot_preservation_oracle(&second), source_probes);
    let second_raw = annotation_dictionary(&second_document, "bp:snapshot:persistence-1");
    let second_graph = image_appearance_graph_ids(&second_document, second_raw);
    assert!(!second_document.objects.contains_key(&first_graph[0]));
    assert_eq!(second_graph[1..], first_graph[1..]);
    assert_eq!(second_raw.get(b"F").unwrap().as_i64().unwrap(), 4);
    assert!((second_raw.get(b"CA").unwrap().as_float().unwrap() - 0.4).abs() < 0.000_01);
    assert!((second_raw.get(b"Rotation").unwrap().as_float().unwrap() - 315.).abs() < 0.000_01);

    let mut second_reopen = PdfPersistenceSession::open(&second).unwrap();
    assert!(second_reopen.snapshot_has_canonical_native_identity(&id));
    assert_eq!(second_reopen.snapshots()[0].rect.width, 216.);
    assert!((second_reopen.snapshots()[0].opacity() - 0.4).abs() < 0.000_01);
    assert!((second_reopen.snapshots()[0].rotation_degrees() - 315.).abs() < 0.000_01);
    assert!(!second_reopen.snapshots()[0].locked);
    second_reopen.remove_snapshot(&id).unwrap();
    second_reopen.save_as(&removed).unwrap();
    validate_independently(&removed);
    let removed_document = Document::load(&removed).unwrap();
    assert!(
        second_graph
            .iter()
            .all(|object_id| !removed_document.objects.contains_key(object_id))
    );
    assert!(
        PdfPersistenceSession::open(&removed)
            .unwrap()
            .snapshots()
            .is_empty()
    );
    assert_eq!(snapshot_preservation_oracle(&removed), source_probes);
    assert_eq!(original_document_oracle(&removed), source_document);
}

#[test]
fn redact_create_edit_delete_round_trips_pending_dictionary_without_touching_page_content() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("redact-source.pdf");
    let first = scratch.path().join("redact-first.pdf");
    let second = scratch.path().join("redact-second.pdf");
    let removed = scratch.path().join("redact-removed.pdf");
    redact_compatibility_fixture(&source);
    let source_bytes = fs::read(&source).unwrap();
    let source_document = original_document_oracle(&source);
    let vendor_redact = vendor_redact_oracle(&source);

    let appearance = RectangleAppearance::new("#ff0000", 1., Some("#000000"), 0.35)
        .unwrap()
        .with_fill_opacity(0.35)
        .unwrap();
    let id = MarkupId::new("redact:persistence-1").unwrap();
    let created = RedactAnnotation::new(
        id.clone(),
        0,
        PdfRect::new(144., 216., 180., 72.).unwrap(),
        "#000000",
        Some("Pending review"),
        appearance.clone(),
    )
    .unwrap();
    let mut session = PdfPersistenceSession::open(&source).unwrap();
    assert!(session.redacts().is_empty());
    assert!(session.untouched_annotations().iter().any(|annotation| {
        annotation.name == "vendor-redact" && annotation.subtype == "Redact"
    }));
    session.add_redact(created.clone()).unwrap();
    assert!(session.redact_has_canonical_native_identity(&id));
    session.save_as(&first).unwrap();
    validate_independently(&first);
    assert_eq!(
        fs::read(&source).unwrap(),
        source_bytes,
        "Save As must not mutate its source PDF"
    );
    assert_eq!(original_document_oracle(&first), source_document);
    assert_eq!(vendor_redact_oracle(&first), vendor_redact);

    let document = Document::load(&first).unwrap();
    let raw = annotation_dictionary(&document, "bp:redact:persistence-1");
    assert_eq!(raw.get(b"Type").unwrap().as_name().unwrap(), b"Annot");
    assert_eq!(raw.get(b"Subtype").unwrap().as_name().unwrap(), b"Redact");
    assert_eq!(raw.get(b"Rect").unwrap().as_array().unwrap().len(), 4);
    assert_eq!(raw.get(b"QuadPoints").unwrap().as_array().unwrap().len(), 8);
    assert_eq!(raw.get(b"IC").unwrap().as_array().unwrap().len(), 3);
    assert_eq!(
        raw.get(b"OverlayText").unwrap().as_str().unwrap(),
        b"Pending review"
    );
    assert_eq!(raw.get(b"Subj").unwrap().as_str().unwrap(), b"Redaction");
    assert_eq!(
        raw.get(b"Contents").unwrap().as_str().unwrap(),
        b"Marked for redaction"
    );
    assert_eq!(raw.get(b"F").unwrap().as_i64().unwrap(), 4);
    assert!(raw.get(b"BPAppearance").is_ok());
    assert!(raw.get(b"CA").is_ok());
    assert!(raw.get(b"ca").is_ok());
    assert!(
        raw.get(b"AP").is_err(),
        "a pending Redact must not invent an appearance stream"
    );

    let mut reopened = PdfPersistenceSession::open(&first).unwrap();
    assert_eq!(reopened.redacts().len(), 1);
    assert!(reopened.redacts()[0].same_persisted_state_as(&created));
    assert!(reopened.redact_has_canonical_native_identity(&id));
    let edited = RedactAnnotation::new(
        id.clone(),
        0,
        PdfRect::new(168., 228., 192., 84.).unwrap(),
        "#101010",
        None::<String>,
        appearance,
    )
    .unwrap();
    reopened.replace_redact(edited.clone()).unwrap();
    reopened.save_as(&second).unwrap();
    assert_eq!(original_document_oracle(&second), source_document);
    assert_eq!(vendor_redact_oracle(&second), vendor_redact);
    let mut second_reopen = PdfPersistenceSession::open(&second).unwrap();
    assert!(second_reopen.redacts()[0].same_persisted_state_as(&edited));
    second_reopen.remove_redact(&id).unwrap();
    second_reopen.save_as(&removed).unwrap();
    let removed_reopen = PdfPersistenceSession::open(&removed).unwrap();
    assert!(removed_reopen.redacts().is_empty());
    assert!(!removed_reopen.has_canonical_raw_annotation_name(&id));
    assert_eq!(original_document_oracle(&removed), source_document);
    assert_eq!(vendor_redact_oracle(&removed), vendor_redact);
}

#[test]
fn arc_create_edit_delete_round_trips_circle_arc_identity_appearance_and_ap() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("arc-source.pdf");
    let created_path = scratch.path().join("arc-created.pdf");
    let edited_path = scratch.path().join("arc-edited.pdf");
    let deleted_path = scratch.path().join("arc-deleted.pdf");
    public_annotation_fixture(&source);
    let id = MarkupId::new("arc:persistence-1").unwrap();
    let created = ArcAnnotation::new(
        id.clone(),
        0,
        point(72., 240.),
        point(288., 240.),
        point(180., 348.),
        RectangleAppearance::new("#ff0000", 1., None::<String>, 0.75).unwrap(),
    )
    .unwrap();

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_arc(created.clone()).unwrap();
    session.save_as(&created_path).unwrap();
    validate_independently(&created_path);

    let document = Document::load(&created_path).unwrap();
    let raw = annotation_dictionary(&document, "bp:arc:persistence-1");
    assert_eq!(raw.get(b"Subtype").unwrap().as_name().unwrap(), b"Circle");
    assert_eq!(raw.get(b"IT").unwrap().as_name().unwrap(), b"CircleArc");
    assert_eq!(raw.get(b"Subj").unwrap().as_str().unwrap(), b"Arc");
    assert_eq!(raw.get(b"RD").unwrap().as_array().unwrap().len(), 4);
    assert!(raw.get(b"Angle1").unwrap().as_float().unwrap().is_finite());
    assert!(raw.get(b"Angle2").unwrap().as_float().unwrap().is_finite());
    assert!(raw.get(b"AP").is_ok());

    let mut created_reopen = PdfPersistenceSession::open(&created_path).unwrap();
    assert_eq!(created_reopen.arcs().len(), 1);
    assert!(created_reopen.arcs()[0].same_persisted_state_as(&created));
    assert!(created_reopen.arc_has_canonical_native_identity(&id));

    let edited = ArcAnnotation::new(
        id.clone(),
        0,
        created.start,
        created.end,
        point(180., 284.735_065),
        RectangleAppearance::new("#2563eb", 3., None::<String>, 0.5).unwrap(),
    )
    .unwrap();
    created_reopen.replace_arc(edited.clone()).unwrap();
    created_reopen.save_as(&edited_path).unwrap();
    let mut edited_reopen = PdfPersistenceSession::open(&edited_path).unwrap();
    assert!(edited_reopen.arcs()[0].same_persisted_state_as(&edited));
    assert!(edited_reopen.arc_has_canonical_native_identity(&id));

    edited_reopen.remove_arc(&id).unwrap();
    edited_reopen.save_as(&deleted_path).unwrap();
    let deleted_reopen = PdfPersistenceSession::open(&deleted_path).unwrap();
    assert!(deleted_reopen.arcs().is_empty());
    assert!(!deleted_reopen.has_canonical_raw_annotation_name(&id));
}

#[test]
fn cloud_plus_pair_round_trip_is_one_logical_annotation_and_never_standalone_parts() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("cloud-plus-source.pdf");
    let output = scratch.path().join("cloud-plus-output.pdf");
    public_annotation_fixture(&source);
    let cloud_plus = CloudPlusAnnotation::new(
        MarkupId::new("cloud-plus-persistence").unwrap(),
        0,
        vec![
            point(30., 30.),
            point(140., 30.),
            point(140., 110.),
            point(30., 110.),
        ],
        2.,
        vec![point(140., 70.), point(165., 92.), point(190., 92.)],
        PdfRect::new(190., 70., 150., 44.).unwrap(),
        "Cloud plus\nBeam B-12",
        CloudPlusAppearance::new(
            RectangleAppearance::new("#ff0000", 1., None::<String>, 1.).unwrap(),
            StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap(),
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_cloud_plus(cloud_plus.clone()).unwrap();
    assert!(session.cloud_plus_has_canonical_native_identity(&cloud_plus.id));
    assert_eq!(
        session.annotation_order().last(),
        Some(&MarkupId::new("cloud-plus-persistence").unwrap())
    );
    session.save_as(&output).unwrap();
    validate_independently(&output);

    let document = Document::load(&output).unwrap();
    let cloud = annotation_dictionary(&document, "bp:cloud-plus-persistence:cloud");
    let text = annotation_dictionary(&document, "bp:cloud-plus-persistence:text");
    assert_eq!(
        cloud.get(b"Subtype").unwrap().as_name().unwrap(),
        b"Polygon"
    );
    assert_eq!(
        cloud.get(b"IT").unwrap().as_name().unwrap(),
        b"PolygonCloud"
    );
    assert_eq!(cloud.get(b"ITEx").unwrap().as_name().unwrap(), b"PolyText");
    assert_eq!(cloud.get(b"Subj").unwrap().as_str().unwrap(), b"Cloud+");
    assert!(cloud.get(b"Vertices").is_ok());
    assert!(cloud.get(b"BE").is_ok());
    assert!(cloud.get(b"AP").is_ok());
    assert_eq!(
        text.get(b"Subtype").unwrap().as_name().unwrap(),
        b"FreeText"
    );
    assert_eq!(
        text.get(b"IT").unwrap().as_name().unwrap(),
        b"FreeTextCallout"
    );
    assert_eq!(text.get(b"ITEx").unwrap().as_name().unwrap(), b"PolyText");
    assert_eq!(text.get(b"Subj").unwrap().as_str().unwrap(), b"Cloud+");
    assert_eq!(text.get(b"CL").unwrap().as_array().unwrap().len(), 6);
    assert!(text.get(b"GroupNesting").is_ok());
    assert!(text.get(b"AP").is_ok());

    let reopened = PdfPersistenceSession::open(&output).unwrap();
    assert_eq!(reopened.cloud_pluses().len(), 1);
    assert!(reopened.clouds().is_empty());
    assert!(reopened.callouts().is_empty());
    let imported = &reopened.cloud_pluses()[0];
    assert_eq!(imported.id, cloud_plus.id);
    assert_eq!(imported.cloud_points(), cloud_plus.cloud_points());
    assert_eq!(imported.leader_points(), cloud_plus.leader_points());
    assert_eq!(imported.text_box, cloud_plus.text_box);
    assert_eq!(imported.content(), cloud_plus.content());
    assert!(
        imported.same_persisted_state_as(&cloud_plus),
        "the paired native reopen must preserve the complete logical Cloud+ state: expected={cloud_plus:?} actual={imported:?}"
    );
    assert!(reopened.cloud_plus_has_canonical_native_identity(&cloud_plus.id));
}

#[test]
fn dimension_round_trip_uses_one_canonical_unmeasured_line_dimension_identity() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("dimension-source.pdf");
    let first = scratch.path().join("dimension-first.pdf");
    let second = scratch.path().join("dimension-second.pdf");
    let removed = scratch.path().join("dimension-removed.pdf");
    public_annotation_fixture(&source);
    let appearance = DimensionAppearance::new(
        StraightLineAppearance::new("#ff0000", 1., 0.75, StrokeStyle::Solid).unwrap(),
        TextBoxStyle::new("Helvetica", 12., "#112233", 0.75).unwrap(),
    )
    .unwrap();
    let dimension = DimensionAnnotation::new(
        MarkupId::new("dimension-persistence").unwrap(),
        0,
        point(72., 320.),
        point(288., 320.),
        24.,
        "Door opening",
        appearance.clone(),
    )
    .unwrap();

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_dimension(dimension.clone()).unwrap();
    assert_eq!(session.dimensions(), &[dimension.clone()]);
    assert!(session.dimension_has_canonical_native_identity(&dimension.id));
    session.save_as(&first).unwrap();
    validate_independently(&first);

    let first_document = Document::load(&first).unwrap();
    let raw = annotation_dictionary(&first_document, "bp:dimension-persistence");
    assert_eq!(raw.get(b"Subtype").unwrap().as_name().unwrap(), b"Line");
    assert_eq!(raw.get(b"IT").unwrap().as_name().unwrap(), b"LineDimension");
    assert_eq!(raw.get(b"Subj").unwrap().as_str().unwrap(), b"Dimension");
    assert_eq!(raw.get(b"L").unwrap().as_array().unwrap().len(), 4);
    assert_eq!(raw.get(b"LE").unwrap().as_array().unwrap().len(), 2);
    assert_eq!(raw.get(b"LL").unwrap().as_float().unwrap(), 24.);
    assert_eq!(raw.get(b"LLE").unwrap().as_float().unwrap(), 4.);
    assert!(raw.get(b"Cap").unwrap().as_bool().unwrap());
    assert_eq!(
        raw.get(b"Contents").unwrap().as_str().unwrap(),
        b"Door opening"
    );
    assert!(raw.get(b"AP").is_ok());
    assert!(raw.get(b"Measure").is_err());

    let mut reopened = PdfPersistenceSession::open(&first).unwrap();
    assert_eq!(reopened.dimensions().len(), 1);
    assert!(reopened.dimensions()[0].same_persisted_state_as(&dimension));
    assert!(reopened.dimension_has_canonical_native_identity(&dimension.id));
    let edited = DimensionAnnotation::new(
        dimension.id.clone(),
        0,
        dimension.start,
        point(324., 330.),
        40.,
        "Clear width",
        appearance,
    )
    .unwrap();
    reopened.replace_dimension(edited.clone()).unwrap();
    reopened.save_as(&second).unwrap();
    let mut second_reopen = PdfPersistenceSession::open(&second).unwrap();
    assert!(second_reopen.dimensions()[0].same_persisted_state_as(&edited));
    assert!(second_reopen.dimension_has_canonical_native_identity(&edited.id));

    second_reopen.remove_dimension(&edited.id).unwrap();
    second_reopen.save_as(&removed).unwrap();
    assert!(
        PdfPersistenceSession::open(&removed)
            .unwrap()
            .dimensions()
            .is_empty()
    );
}

#[test]
fn cloud_plus_pair_stays_adjacent_through_reorder_replace_and_remove() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("cloud-plus-lifecycle-source.pdf");
    let first = scratch.path().join("cloud-plus-lifecycle-first.pdf");
    let second = scratch.path().join("cloud-plus-lifecycle-second.pdf");
    let removed = scratch.path().join("cloud-plus-lifecycle-removed.pdf");
    public_annotation_fixture(&source);
    let cloud_plus_id = MarkupId::new("cloud-plus-lifecycle").unwrap();
    let annotation = CloudPlusAnnotation::new(
        cloud_plus_id.clone(),
        0,
        vec![
            point(30., 30.),
            point(140., 30.),
            point(140., 110.),
            point(30., 110.),
        ],
        2.,
        vec![point(140., 70.), point(165., 92.), point(190., 92.)],
        PdfRect::new(190., 70., 150., 44.).unwrap(),
        "Initial",
        CloudPlusAppearance::new(
            RectangleAppearance::new("#ff0000", 1., None::<String>, 1.).unwrap(),
            StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap(),
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_cloud_plus(annotation.clone()).unwrap();
    session
        .reorder_managed_annotations(&[
            cloud_plus_id.clone(),
            MarkupId::new("source-rectangle").unwrap(),
        ])
        .unwrap();
    session.save_as(&first).unwrap();
    assert_eq!(
        page_annotation_names(&first),
        vec![
            "bp:cloud-plus-lifecycle:cloud",
            "bp:cloud-plus-lifecycle:text",
            "unknown-1",
            "source-rectangle",
        ]
    );

    let mut reopened = PdfPersistenceSession::open(&first).unwrap();
    let replacement = CloudPlusAnnotation::new(
        cloud_plus_id.clone(),
        0,
        annotation.cloud_points().to_vec(),
        annotation.border_effect_intensity(),
        vec![point(145., 72.), point(175., 100.), point(205., 100.)],
        PdfRect::new(205., 78., 150., 60.).unwrap(),
        "Replaced\ncontent",
        annotation.appearance.clone(),
    )
    .unwrap();
    reopened.replace_cloud_plus(replacement).unwrap();
    reopened.save_as(&second).unwrap();
    let mut reopened = PdfPersistenceSession::open(&second).unwrap();
    assert_eq!(reopened.cloud_pluses()[0].content(), "Replaced\ncontent");
    assert_eq!(
        page_annotation_names(&second)[..2],
        [
            "bp:cloud-plus-lifecycle:cloud",
            "bp:cloud-plus-lifecycle:text",
        ]
    );

    reopened.remove_cloud_plus(&cloud_plus_id).unwrap();
    reopened.save_as(&removed).unwrap();
    assert!(
        PdfPersistenceSession::open(&removed)
            .unwrap()
            .cloud_pluses()
            .is_empty()
    );
    assert_eq!(
        page_annotation_names(&removed),
        vec!["unknown-1", "source-rectangle"]
    );
}

#[test]
fn cloud_plus_import_pairs_reversed_halves_and_quarantines_a_damaged_pair() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("cloud-plus-order-source.pdf");
    let normal = scratch.path().join("cloud-plus-order-normal.pdf");
    let reversed = scratch.path().join("cloud-plus-order-reversed.pdf");
    let damaged = scratch.path().join("cloud-plus-order-damaged.pdf");
    public_annotation_fixture(&source);
    let cloud_plus_id = MarkupId::new("cloud-plus-order").unwrap();
    let cloud_plus = CloudPlusAnnotation::new(
        cloud_plus_id.clone(),
        0,
        vec![
            point(30., 30.),
            point(140., 30.),
            point(140., 110.),
            point(30., 110.),
        ],
        2.,
        Vec::new(),
        PdfRect::new(55., 48., 80., 44.).unwrap(),
        "Inline Cloud+",
        CloudPlusAppearance::new(
            RectangleAppearance::new("#ff0000", 1., None::<String>, 1.).unwrap(),
            StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap(),
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_cloud_plus(cloud_plus).unwrap();
    session.save_as(&normal).unwrap();

    let mut reversed_document = Document::load(&normal).unwrap();
    let page_id = reversed_document.get_pages()[&1];
    reversed_document
        .get_object_mut(page_id)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .get_mut(b"Annots")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .reverse();
    reversed_document.save(&reversed).unwrap();
    let reversed_session = PdfPersistenceSession::open(&reversed).unwrap();
    assert_eq!(reversed_session.cloud_pluses().len(), 1);
    assert_eq!(reversed_session.cloud_pluses()[0].id, cloud_plus_id);
    assert!(reversed_session.clouds().is_empty());
    assert!(reversed_session.callouts().is_empty());

    let mut damaged_document = Document::load(&reversed).unwrap();
    let page_id = damaged_document.get_pages()[&1];
    let annotation_references = damaged_document
        .get_object(page_id)
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Annots")
        .unwrap()
        .as_array()
        .unwrap()
        .clone();
    let text_object_id = annotation_references
        .iter()
        .filter_map(|value| value.as_reference().ok())
        .find(|object_id| {
            damaged_document
                .get_object(*object_id)
                .ok()
                .and_then(|value| value.as_dict().ok())
                .and_then(|dictionary| dictionary.get(b"NM").ok())
                .and_then(|value| value.as_str().ok())
                == Some(b"bp:cloud-plus-order:text")
        })
        .unwrap();
    damaged_document
        .get_object_mut(page_id)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .get_mut(b"Annots")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .retain(|value| value.as_reference().ok() != Some(text_object_id));
    damaged_document.save(&damaged).unwrap();
    let damaged_session = PdfPersistenceSession::open(&damaged).unwrap();
    assert!(damaged_session.cloud_pluses().is_empty());
    assert!(damaged_session.clouds().is_empty());
    assert!(damaged_session.callouts().is_empty());
    assert!(
        damaged_session
            .untouched_annotations()
            .iter()
            .any(|annotation| {
                annotation.name == "bp:cloud-plus-order:cloud" && annotation.subtype == "Polygon"
            })
    );
}

#[test]
fn cloud_plus_import_uses_exact_group_members_when_native_names_have_no_role_suffix() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("cloud-plus-group-source.pdf");
    let managed = scratch.path().join("cloud-plus-group-managed.pdf");
    let grouped = scratch.path().join("cloud-plus-group-external.pdf");
    public_annotation_fixture(&source);
    let cloud_plus = CloudPlusAnnotation::new(
        MarkupId::new("cloud-plus-group").unwrap(),
        0,
        vec![
            point(30., 30.),
            point(140., 30.),
            point(140., 110.),
            point(30., 110.),
        ],
        2.,
        vec![point(140., 70.), point(165., 92.), point(190., 92.)],
        PdfRect::new(190., 70., 150., 44.).unwrap(),
        "External group",
        CloudPlusAppearance::new(
            RectangleAppearance::new("#ff0000", 1., None::<String>, 1.).unwrap(),
            StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap(),
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_cloud_plus(cloud_plus).unwrap();
    session.save_as(&managed).unwrap();

    let mut document = Document::load(&managed).unwrap();
    let page_id = document.get_pages()[&1];
    let references = document
        .get_object(page_id)
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Annots")
        .unwrap()
        .as_array()
        .unwrap()
        .clone();
    let named_object = |name: &[u8]| {
        references
            .iter()
            .filter_map(|value| value.as_reference().ok())
            .find(|object_id| {
                document
                    .get_object(*object_id)
                    .ok()
                    .and_then(|value| value.as_dict().ok())
                    .and_then(|dictionary| dictionary.get(b"NM").ok())
                    .and_then(|value| value.as_str().ok())
                    == Some(name)
            })
            .unwrap()
    };
    let cloud_object_id = named_object(b"bp:cloud-plus-group:cloud");
    let text_object_id = named_object(b"bp:cloud-plus-group:text");
    document
        .get_object_mut(cloud_object_id)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("NM", pdf_string("legacy-cloud-name"));
    let text = document
        .get_object_mut(text_object_id)
        .unwrap()
        .as_dict_mut()
        .unwrap();
    text.set("NM", pdf_string("legacy-text-name"));
    text.set(
        "GroupNesting",
        vec![
            pdf_string("Cloud+"),
            Object::Name(b"legacy-text-name".to_vec()),
            Object::Name(b"legacy-cloud-name".to_vec()),
        ],
    );
    document.save(&grouped).unwrap();

    let imported = PdfPersistenceSession::open(&grouped).unwrap();
    assert_eq!(imported.cloud_pluses().len(), 1);
    assert_eq!(imported.cloud_pluses()[0].id.as_str(), "legacy-cloud-name");
    assert!(imported.clouds().is_empty());
    assert!(imported.callouts().is_empty());
}

#[test]
fn callout_round_trip_keeps_native_composite_identity_and_text_box_geometry() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("callout-source.pdf");
    let output = scratch.path().join("callout-output.pdf");
    public_annotation_fixture(&source);
    let callout = CalloutAnnotation::new(
        MarkupId::new("callout-persistence").unwrap(),
        0,
        vec![point(30., 30.), point(60., 60.), point(100., 60.)],
        PdfRect::new(100., 38., 150., 44.).unwrap(),
        "Need to check\nBeam B-12",
        CalloutAppearance::new(
            StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap(),
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_callout(callout.clone()).unwrap();
    assert!(session.callout_has_canonical_native_identity(&callout.id));
    session.save_as(&output).unwrap();
    validate_independently(&output);

    let document = Document::load(&output).unwrap();
    let dictionary = annotation_dictionary(&document, "bp:callout-persistence");
    assert_eq!(
        dictionary.get(b"Subtype").unwrap().as_name().unwrap(),
        b"FreeText"
    );
    assert_eq!(
        dictionary.get(b"IT").unwrap().as_name().unwrap(),
        b"FreeTextCallout"
    );
    assert_eq!(
        dictionary.get(b"Subj").unwrap().as_str().unwrap(),
        b"Callout"
    );
    assert_eq!(dictionary.get(b"CL").unwrap().as_array().unwrap().len(), 6);
    assert_eq!(dictionary.get(b"LE").unwrap().as_array().unwrap().len(), 2);
    assert!(dictionary.get(b"RD").is_ok());
    assert!(dictionary.get(b"DA").is_ok());
    assert!(dictionary.get(b"DS").is_ok());
    assert!(dictionary.get(b"RC").is_ok());
    assert!(dictionary.get(b"DR").is_ok());
    assert!(dictionary.get(b"AP").is_ok());

    let reopened = PdfPersistenceSession::open(&output).unwrap();
    assert_eq!(reopened.callouts().len(), 1);
    assert_eq!(reopened.callouts()[0].id, callout.id);
    assert_eq!(
        reopened.callouts()[0].leader_points(),
        callout.leader_points()
    );
    assert_eq!(reopened.callouts()[0].text_box, callout.text_box);
    assert_eq!(reopened.callouts()[0].content(), callout.content());
    assert!(reopened.callout_has_canonical_native_identity(&callout.id));
}
use lopdf::{Dictionary, Document, Object, Stream, StringFormat, dictionary};

struct ScratchDirectory(PathBuf);

impl ScratchDirectory {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must follow the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "butter-paper-pdf-persistence-{}-{nonce}",
            process::id(),
        ));
        fs::create_dir(&path).expect("scratch directory must be creatable");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

#[test]
fn measurement_paths_keep_native_intent_scale_caption_and_stable_identity_across_reopens() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("measurement-path-source.pdf");
    let first = scratch.path().join("measurement-path-first.pdf");
    let second = scratch.path().join("measurement-path-second.pdf");
    public_annotation_fixture(&source);
    let calibration = LengthCalibration::from_scale(72., 2., "ft", 2, true).unwrap();
    let polylength = MeasurementPathAnnotation::new(
        MarkupId::new("measurement-polylength").unwrap(),
        0,
        vec![point(72., 72.), point(144., 72.), point(144., 144.)],
        MeasurementPathKind::Polylength,
        calibration.clone(),
        RectangleAppearance::default(),
    )
    .unwrap();
    let area = MeasurementPathAnnotation::new(
        MarkupId::new("measurement-area").unwrap(),
        0,
        vec![point(216., 216.), point(288., 216.), point(288., 288.)],
        MeasurementPathKind::Area,
        calibration.clone(),
        RectangleAppearance::default(),
    )
    .unwrap();

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_measurement_path(polylength.clone()).unwrap();
    session.add_measurement_path(area.clone()).unwrap();
    session.save_as(&first).unwrap();
    validate_independently(&first);

    let first_document = Document::load(&first).unwrap();
    for (name, subtype, intent, subject, measurement_types, caption) in [
        (
            "bp:measurement-polylength",
            b"PolyLine".as_slice(),
            b"PolyLineDimension".as_slice(),
            b"Polylength Measurement".as_slice(),
            130,
            "4.00 ft",
        ),
        (
            "bp:measurement-area",
            b"Polygon".as_slice(),
            b"PolygonDimension".as_slice(),
            b"Area Measurement".as_slice(),
            129,
            "2.00 ft^2",
        ),
    ] {
        let annotation = annotation_dictionary(&first_document, name);
        assert_eq!(
            annotation.get(b"Subtype").unwrap().as_name().unwrap(),
            subtype
        );
        assert_eq!(annotation.get(b"IT").unwrap().as_name().unwrap(), intent);
        assert_eq!(annotation.get(b"Subj").unwrap().as_str().unwrap(), subject);
        assert_eq!(
            annotation
                .get(b"MeasurementTypes")
                .unwrap()
                .as_i64()
                .unwrap(),
            measurement_types,
        );
        assert_eq!(
            annotation
                .get(b"Measure")
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Type")
                .unwrap()
                .as_name()
                .unwrap(),
            b"Measure",
        );
        assert_eq!(
            annotation.get(b"Contents").unwrap().as_str().unwrap(),
            caption.as_bytes(),
        );
        let appearance = normal_appearance(&first_document, annotation);
        assert!(
            String::from_utf8_lossy(&appearance.content).contains(caption),
            "the native appearance must paint the current measurement caption",
        );
    }

    let mut first_reopen = PdfPersistenceSession::open(&first).unwrap();
    assert!(first_reopen.vertex_paths().is_empty());
    assert_eq!(first_reopen.measurement_paths().len(), 2);
    assert!(first_reopen.measurement_path_has_canonical_native_identity(&polylength.id));
    assert!(first_reopen.measurement_path_has_canonical_native_identity(&area.id));
    assert_eq!(
        first_reopen.measurement_paths()[0].kind,
        MeasurementPathKind::Polylength
    );
    assert_eq!(first_reopen.measurement_paths()[0].caption(), "4.00 ft");
    assert_eq!(
        first_reopen.measurement_paths()[1].kind,
        MeasurementPathKind::Area
    );
    assert_eq!(first_reopen.measurement_paths()[1].caption(), "2.00 ft^2");

    let edited = MeasurementPathAnnotation::new(
        polylength.id.clone(),
        0,
        vec![point(72., 72.), point(180., 72.), point(180., 144.)],
        MeasurementPathKind::Polylength,
        calibration,
        RectangleAppearance::default(),
    )
    .unwrap();
    first_reopen.replace_measurement_path(edited).unwrap();
    first_reopen.save_as(&second).unwrap();
    validate_independently(&second);
    let second_reopen = PdfPersistenceSession::open(&second).unwrap();
    assert_eq!(second_reopen.measurement_paths().len(), 2);
    assert_eq!(second_reopen.measurement_paths()[0].id, polylength.id);
    assert_eq!(second_reopen.measurement_paths()[0].caption(), "5.00 ft");
    assert_eq!(second_reopen.measurement_paths()[1].id, area.id);
}

#[test]
fn measurement_and_cloud_import_classify_without_absorbing_ordinary_paths() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("measurement-import-classification.pdf");
    let target = scratch
        .path()
        .join("measurement-import-classification-saved.pdf");
    measurement_import_classification_fixture(&source);

    let mut session = PdfPersistenceSession::open(&source).unwrap();

    assert_eq!(session.measurement_paths().len(), 4);
    let measurement = |id: &str| {
        session
            .measurement_paths()
            .iter()
            .find(|annotation| annotation.id.as_str() == id)
            .unwrap_or_else(|| panic!("measurement {id} must import by stable identity"))
    };
    assert_eq!(
        measurement("legacy-polylength").kind,
        MeasurementPathKind::Polylength
    );
    assert_eq!(measurement("legacy-polylength").caption(), "2.00 ft");
    assert_eq!(measurement("legacy-area").kind, MeasurementPathKind::Area);
    assert_eq!(measurement("legacy-area").caption(), "0.50 ft^2");
    assert_eq!(
        measurement("standard-polylength").kind,
        MeasurementPathKind::Polylength
    );
    assert_eq!(measurement("standard-polylength").caption(), "1.00 ft");
    assert_eq!(measurement("standard-area").kind, MeasurementPathKind::Area);
    assert_eq!(measurement("standard-area").caption(), "0.50 ft^2");

    assert_eq!(session.vertex_paths().len(), 2);
    assert_eq!(session.vertex_paths()[0].id.as_str(), "ordinary-polyline");
    assert_eq!(session.vertex_paths()[0].kind, VertexPathKind::Polyline);
    assert_eq!(session.vertex_paths()[1].id.as_str(), "ordinary-polygon");
    assert_eq!(session.vertex_paths()[1].kind, VertexPathKind::Polygon);
    assert!(session.untouched_annotations().iter().any(|annotation| {
        annotation.name == "direct-legacy-polylength" && annotation.subtype == "PolyLine"
    }));
    assert_eq!(session.clouds().len(), 1);
    assert_eq!(session.clouds()[0].id.as_str(), "cloud-not-area");
    assert_eq!(session.clouds()[0].border_effect_intensity(), 2.);
    assert_eq!(session.clouds()[0].points().len(), 3);
    assert!(!session.cloud_has_canonical_native_identity(&session.clouds()[0].id));
    let imported_cloud = session.clouds()[0].clone();
    let mut edited_points = imported_cloud.points().to_vec();
    edited_points[1] = point(330., 186.);
    let mut edited_cloud = CloudAnnotation::new(
        imported_cloud.id.clone(),
        imported_cloud.page_index,
        edited_points,
        imported_cloud.border_effect_intensity(),
        imported_cloud.appearance.clone(),
    )
    .unwrap();
    edited_cloud.locked = imported_cloud.locked;
    session.replace_cloud(edited_cloud).unwrap();
    assert!(session.cloud_has_canonical_native_identity(&session.clouds()[0].id));

    session.save_as(&target).unwrap();
    let saved = Document::load(&target).unwrap();
    assert_eq!(
        annotation_dictionary(&saved, "direct-legacy-polylength")
            .get(b"BPDirectProbe")
            .unwrap()
            .as_str()
            .unwrap(),
        b"preserve-unmanaged-direct-dictionary",
    );
    let reopened = PdfPersistenceSession::open(&target).unwrap();
    assert_eq!(reopened.measurement_paths(), session.measurement_paths());
    assert_eq!(reopened.vertex_paths(), session.vertex_paths());
    assert_eq!(reopened.clouds(), session.clouds());
    assert!(reopened.cloud_has_canonical_native_identity(&reopened.clouds()[0].id));
}

#[test]
fn full_page_scale_json_survives_two_reopens_without_losing_axes_or_fraction_precision() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("source.pdf");
    let first = scratch.path().join("first.pdf");
    let second = scratch.path().join("second.pdf");
    public_annotation_fixture(&source);
    let scale = PageScale::from_factors(
        0,
        ScaleSource::Custom,
        "1 in = 2 ft / 9 ft Y",
        ScaleUnit::In,
        ScaleUnit::Ft,
        2. / 72.,
        9. / 144.,
        ScalePrecision::fraction(16).unwrap(),
    )
    .unwrap();

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.set_page_scale(scale.clone()).unwrap();
    session.save_as(&first).unwrap();
    let first_reopen = PdfPersistenceSession::open(&first).unwrap();
    assert_eq!(first_reopen.page_scales(), &[scale.clone()]);
    first_reopen.save_as(&second).unwrap();
    let second_reopen = PdfPersistenceSession::open(&second).unwrap();
    assert_eq!(second_reopen.page_scales(), &[scale]);
}

#[test]
fn replacing_page_scales_removes_stale_page_metadata_on_reopen() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("source.pdf");
    let scaled = scratch.path().join("scaled.pdf");
    let cleared = scratch.path().join("cleared.pdf");
    public_annotation_fixture(&source);
    let scale = PageScale::from_factors(
        0,
        ScaleSource::Custom,
        "1 cm = 1 m",
        ScaleUnit::Cm,
        ScaleUnit::M,
        1.,
        1.,
        ScalePrecision::decimal(0.001).unwrap(),
    )
    .unwrap();

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.replace_page_scales(&[scale.clone()]).unwrap();
    session.save_as(&scaled).unwrap();
    let mut reopened = PdfPersistenceSession::open(&scaled).unwrap();
    assert_eq!(reopened.page_scales(), &[scale]);

    reopened.replace_page_scales(&[]).unwrap();
    reopened.save_as(&cleared).unwrap();
    let cleared_reopen = PdfPersistenceSession::open(&cleared).unwrap();
    assert!(cleared_reopen.page_scales().is_empty());
    assert!(cleared_reopen.page_length_calibrations().is_empty());
}

#[test]
fn ellipse_create_edit_delete_round_trips_circle_identity_and_appearance() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("source.pdf");
    let created_path = scratch.path().join("ellipse-created.pdf");
    let edited_path = scratch.path().join("ellipse-edited.pdf");
    let deleted_path = scratch.path().join("ellipse-deleted.pdf");
    public_annotation_fixture(&source);
    let id = MarkupId::new("ellipse:persistence-1").unwrap();
    let created = EllipseAnnotation::new(
        id.clone(),
        0,
        PdfRect::new(72., 144., 180., 96.).unwrap(),
        RectangleAppearance::new("#ff0000", 1., None::<String>, 1.).unwrap(),
    )
    .unwrap();

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_ellipse(created.clone()).unwrap();
    session.save_as(&created_path).unwrap();
    let mut created_reopen = PdfPersistenceSession::open(&created_path).unwrap();
    assert_eq!(created_reopen.ellipses(), &[created.clone()]);
    assert!(created_reopen.ellipse_has_canonical_native_identity(&id));

    let mut edited = created.clone();
    edited.rect = PdfRect::new(96., 168., 216., 120.).unwrap();
    edited.rotation_degrees = 30.;
    edited.appearance = RectangleAppearance::new("#2563eb", 3., Some("#dbeafe"), 0.75)
        .unwrap()
        .with_fill_opacity(0.5)
        .unwrap();
    created_reopen.replace_ellipse(edited.clone()).unwrap();
    created_reopen.save_as(&edited_path).unwrap();
    let mut edited_reopen = PdfPersistenceSession::open(&edited_path).unwrap();
    assert_eq!(edited_reopen.ellipses(), &[edited]);
    assert!(edited_reopen.ellipse_has_canonical_native_identity(&id));

    edited_reopen.remove_ellipse(&id).unwrap();
    edited_reopen.save_as(&deleted_path).unwrap();
    let deleted_reopen = PdfPersistenceSession::open(&deleted_path).unwrap();
    assert!(deleted_reopen.ellipses().is_empty());
    assert!(!deleted_reopen.has_canonical_raw_annotation_name(&id));
}

#[test]
fn ellipse_property_writes_standard_dash_and_normal_appearance_stream() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("ellipse-property-source.pdf");
    let output = scratch.path().join("ellipse-property-output.pdf");
    public_annotation_fixture(&source);
    let id = MarkupId::new("ellipse:property-appearance-1").unwrap();
    let mut ellipse = EllipseAnnotation::new(
        id.clone(),
        0,
        PdfRect::new(72., 144., 180., 96.).unwrap(),
        RectangleAppearance::new("#2563eb", 3., Some("#fef3c7"), 0.72)
            .unwrap()
            .with_fill_opacity(0.4)
            .unwrap()
            .with_stroke_style(StrokeStyle::Dashed),
    )
    .unwrap();
    ellipse.rotation_degrees = 15.;
    ellipse.locked = true;

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_ellipse(ellipse.clone()).unwrap();
    session.save_as(&output).unwrap();

    let document = Document::load(&output).unwrap();
    let raw = annotation_dictionary(&document, "bp:ellipse:property-appearance-1");
    assert_eq!(raw.get(b"Subtype").unwrap().as_name().unwrap(), b"Circle");
    let border_style = resolve(&document, raw.get(b"BS").unwrap())
        .as_dict()
        .unwrap();
    assert_eq!(border_style.get(b"S").unwrap().as_name().unwrap(), b"D");
    assert_eq!(border_style.get(b"D").unwrap().as_array().unwrap().len(), 2);
    let normal = resolve(
        &document,
        resolve(&document, raw.get(b"AP").unwrap())
            .as_dict()
            .unwrap()
            .get(b"N")
            .unwrap(),
    )
    .as_stream()
    .unwrap();
    let content = String::from_utf8_lossy(&normal.content);
    assert!(content.contains(" c\n"), "Ellipse appearance must use cubic curves: {content}");
    assert!(content.contains(" d\n"), "Ellipse appearance must paint the dash: {content}");
    assert!(content.contains("B\n"), "Ellipse appearance must stroke and fill: {content}");
    let resources = resolve(&document, normal.dict.get(b"Resources").unwrap())
        .as_dict()
        .unwrap();
    assert!(resources.get(b"ExtGState").is_ok());

    let reopened = PdfPersistenceSession::open(&output).unwrap();
    assert_eq!(reopened.ellipses(), &[ellipse]);
    assert!(reopened.ellipse_has_canonical_native_identity(&id));
}

#[test]
fn ellipse_property_rotation_and_stroke_fit_the_normal_appearance_bounds() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("ellipse-rotation-source.pdf");
    let output = scratch.path().join("ellipse-rotation-output.pdf");
    public_annotation_fixture(&source);
    let id = MarkupId::new("ellipse:rotation-appearance-1").unwrap();
    let rect = PdfRect::new(72., 144., 180., 96.).unwrap();
    let mut ellipse = EllipseAnnotation::new(
        id,
        0,
        rect,
        RectangleAppearance::new("#2563eb", 10., None::<String>, 1.).unwrap(),
    )
    .unwrap();
    ellipse.rotation_degrees = 30.;

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_ellipse(ellipse.clone()).unwrap();
    session.save_as(&output).unwrap();

    let document = Document::load(&output).unwrap();
    let raw = annotation_dictionary(&document, "bp:ellipse:rotation-appearance-1");
    let annotation_rect = raw
        .get(b"Rect")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|value| f64::from(value.as_float().unwrap()))
        .collect::<Vec<_>>();
    let normal = resolve(
        &document,
        resolve(&document, raw.get(b"AP").unwrap())
            .as_dict()
            .unwrap()
            .get(b"N")
            .unwrap(),
    )
    .as_stream()
    .unwrap();
    let bbox = normal
        .dict
        .get(b"BBox")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|value| f64::from(value.as_float().unwrap()))
        .collect::<Vec<_>>();
    let content = String::from_utf8_lossy(&normal.content);
    let tokens = content.split_whitespace().collect::<Vec<_>>();
    let move_index = tokens.iter().position(|token| *token == "m").unwrap();
    let move_x = tokens[move_index - 2].parse::<f64>().unwrap();
    let move_y = tokens[move_index - 1].parse::<f64>().unwrap();
    let east = ellipse_resize_handle_point_for_rect(
        rect,
        ellipse.rotation_degrees,
        RectangleResizeHandle::East,
    );
    assert!((annotation_rect[0] + move_x - east.x).abs() < 0.000_01);
    assert!((annotation_rect[1] + move_y - east.y).abs() < 0.000_01);

    let radians = ellipse.rotation_degrees.to_radians();
    let radius_x = rect.width * 0.5;
    let radius_y = rect.height * 0.5;
    let geometric_width = 2.
        * ((radius_x * radians.cos()).powi(2) + (radius_y * radians.sin()).powi(2))
            .sqrt();
    let geometric_height = 2.
        * ((radius_x * radians.sin()).powi(2) + (radius_y * radians.cos()).powi(2))
            .sqrt();
    let expected_width = geometric_width + ellipse.appearance.stroke_width_pt();
    let expected_height = geometric_height + ellipse.appearance.stroke_width_pt();
    assert!((annotation_rect[2] - annotation_rect[0] - expected_width).abs() < 0.000_1);
    assert!((annotation_rect[3] - annotation_rect[1] - expected_height).abs() < 0.000_1);
    assert!((bbox[2] - expected_width).abs() < 0.000_1);
    assert!((bbox[3] - expected_height).abs() < 0.000_1);
}

#[test]
fn ellipse_property_decimal_geometry_uses_pdf_representable_reopen_equivalence() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("ellipse-decimal-source.pdf");
    let output = scratch.path().join("ellipse-decimal-output.pdf");
    public_annotation_fixture(&source);
    let id = MarkupId::new("ellipse:decimal-property-1").unwrap();
    let mut ellipse = EllipseAnnotation::new(
        id,
        0,
        PdfRect::new(12.345_679, 23.456_789, 90.123_457, 45.765_431).unwrap(),
        RectangleAppearance::new("#2563eb", 3.25, Some("#fef3c7"), 0.72)
            .unwrap()
            .with_fill_opacity(0.4)
            .unwrap(),
    )
    .unwrap();
    ellipse.rotation_degrees = 17.125;

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session.add_ellipse(ellipse.clone()).unwrap();
    session.save_as(&output).unwrap();
    let reopened = PdfPersistenceSession::open(&output).unwrap();
    assert!(reopened.ellipses()[0].same_persisted_state_as(&ellipse));
}

impl Drop for ScratchDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn pdf_string(value: &str) -> Object {
    Object::String(value.as_bytes().to_vec(), StringFormat::Literal)
}

fn point(x: f64, y: f64) -> PdfPoint {
    PdfPoint::new(x, y).unwrap()
}

fn public_annotation_fixture(path: &Path) {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(
        dictionary! {},
        b"q 0.8 G 0.5 w 36 36 540 720 re S Q\n".to_vec(),
    ));
    let rectangle_appearance_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 180.into(), 96.into()],
            "Resources" => dictionary! {},
        },
        b"q 0.113725 0.431373 0.847059 RG 2 w 1 1 178 94 re S Q\n".to_vec(),
    ));
    let rectangle_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Square",
        "Rect" => vec![72.into(), 144.into(), 252.into(), 240.into()],
        "NM" => pdf_string("source-rectangle"),
        "C" => vec![Object::Real(0.113725), Object::Real(0.431373), Object::Real(0.847059)],
        "BS" => dictionary! { "Type" => "Border", "W" => 2, "S" => "S" },
        "AP" => dictionary! { "N" => rectangle_appearance_id },
        "BPProbe" => pdf_string("replace-on-edit"),
    });
    let unknown_appearance_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 24.into(), 24.into()],
            "Resources" => dictionary! {},
            "BPStreamProbe" => pdf_string("unknown-stream-preserve-me"),
        },
        b"q 0.7 0.2 0.8 rg 0 0 24 24 re f Q\n".to_vec(),
    ));
    let unknown_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Text",
        "Rect" => vec![300.into(), 144.into(), 324.into(), 168.into()],
        "NM" => pdf_string("unknown-1"),
        "Contents" => pdf_string("Untouched proprietary annotation"),
        "BPUnknown" => pdf_string("unknown-dictionary-preserve-me"),
        "AP" => dictionary! { "N" => unknown_appearance_id },
    });
    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "CropBox" => vec![18.into(), 18.into(), 594.into(), 774.into()],
            "Resources" => dictionary! {},
            "Contents" => content_id,
            "Annots" => vec![rectangle_id.into(), unknown_id.into()],
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    let info_id = document.add_object(dictionary! {
        "Title" => pdf_string("Butter Paper persistence fixture"),
        "Author" => pdf_string("Butter Paper"),
        "BPInfoProbe" => pdf_string("preserve-document-metadata"),
    });
    document.trailer.set("Root", catalog_id);
    document.trailer.set("Info", info_id);
    document
        .save(path)
        .expect("public annotation fixture must save");
}

fn snapshot_compatibility_fixture(path: &Path) {
    public_annotation_fixture(path);
    let mut document = Document::load(path).expect("base Snapshot fixture must load");
    let vendor_appearance_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 72.into(), 36.into()],
            "Resources" => dictionary! {},
            "BPVendorAppearance" => pdf_string("preserve-vendor-appearance"),
        },
        b"q 0.2 0.4 0.8 rg 0 0 72 36 re f Q\n".to_vec(),
    ));
    let vendor_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Stamp",
        "Rect" => vec![360.into(), 216.into(), 432.into(), 252.into()],
        "NM" => pdf_string("vendor-snapshot"),
        "Subj" => pdf_string("Snapshot"),
        "IT" => "StampSnapshot",
        "Contents" => pdf_string("External snapshot"),
        "BPVendorSnapshot" => pdf_string("preserve-vendor-dictionary"),
        "AP" => dictionary! { "N" => vendor_appearance_id },
    });
    let malformed_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Stamp",
        "Rect" => vec![450.into(), 216.into(), 522.into(), 252.into()],
        "NM" => pdf_string("bp:malformed-snapshot"),
        "Subj" => pdf_string("Snapshot"),
        "IT" => "StampSnapshot",
        "Contents" => pdf_string("Canonical-looking but missing an image appearance"),
        "BPMalformedSnapshot" => pdf_string("preserve-malformed-dictionary"),
    });
    let page_id = document.get_pages()[&1];
    let annotations = document
        .get_object_mut(page_id)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .get_mut(b"Annots")
        .unwrap()
        .as_array_mut()
        .unwrap();
    annotations.push(vendor_id.into());
    annotations.push(malformed_id.into());
    document
        .save(path)
        .expect("Snapshot compatibility fixture must save");
}

fn redact_compatibility_fixture(path: &Path) {
    public_annotation_fixture(path);
    let mut document = Document::load(path).expect("base redact fixture must load");
    let appearance_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 72.into(), 36.into()],
            "Resources" => dictionary! {},
        },
        b"q 0.2 0.2 0.2 rg 0 0 72 36 re f Q\n".to_vec(),
    ));
    let rollover_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 72.into(), 36.into()],
            "Resources" => dictionary! {},
        },
        b"q 0.7 0.1 0.1 rg 0 0 72 36 re f Q\n".to_vec(),
    ));
    let redact_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Redact",
        "Rect" => vec![360.into(), 216.into(), 432.into(), 252.into()],
        "QuadPoints" => vec![360.into(), 252.into(), 432.into(), 252.into(), 360.into(), 216.into(), 432.into(), 216.into()],
        "IC" => vec![Object::Real(0.1), Object::Real(0.2), Object::Real(0.3)],
        "OverlayText" => pdf_string("VENDOR PENDING"),
        "Repeat" => true,
        "DA" => pdf_string("0 0 0 rg /Helv 9 Tf"),
        "Q" => 1,
        "NM" => pdf_string("vendor-redact"),
        "Contents" => pdf_string("External pending redaction"),
        "AP" => dictionary! { "N" => appearance_id },
        "RO" => rollover_id,
        "BPVendorRedact" => pdf_string("preserve-verbatim"),
    });
    let page_id = document.get_pages()[&1];
    document
        .get_object_mut(page_id)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .get_mut(b"Annots")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(redact_id.into());
    document
        .save(path)
        .expect("redact compatibility fixture must save");
}

fn measurement_import_classification_fixture(path: &Path) {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let standard_number_format = || {
        Object::Dictionary(dictionary! {
            "Type" => "NumberFormat",
            "U" => pdf_string("ft"),
            "C" => Object::Real((1. / 36.) as f32),
            "D" => 100,
            "SS" => pdf_string(""),
        })
    };
    let standard_measure = || {
        Object::Dictionary(dictionary! {
            "Type" => "Measure",
            "Subtype" => "RL",
            "R" => pdf_string("1 ft = 36 pt"),
            "X" => vec![standard_number_format()],
            "D" => vec![standard_number_format()],
            "A" => vec![standard_number_format()],
            "TargetUnitConversion" => Object::Real((1. / 36.) as f32),
        })
    };
    let legacy_polylength_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "PolyLine",
        "Subj" => pdf_string("Polylength Measurement"),
        "NM" => pdf_string("bp:legacy-polylength"),
        "Vertices" => vec![72.into(), 72.into(), 108.into(), 72.into(), 108.into(), 90.into()],
    });
    let legacy_area_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Polygon",
        "Subj" => pdf_string("Area Measurement"),
        "NM" => pdf_string("bp:legacy-area"),
        "Vertices" => vec![144.into(), 72.into(), 180.into(), 72.into(), 180.into(), 90.into()],
    });
    let standard_polylength_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "PolyLine",
        "IT" => "PolyLineDimension",
        "NM" => pdf_string("bp:standard-polylength"),
        "Vertices" => vec![216.into(), 72.into(), 252.into(), 72.into()],
        "Measure" => standard_measure(),
    });
    let standard_area_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Polygon",
        "IT" => "PolygonDimension",
        "NM" => pdf_string("bp:standard-area"),
        "Vertices" => vec![288.into(), 72.into(), 324.into(), 72.into(), 324.into(), 108.into()],
        "Measure" => standard_measure(),
    });
    let ordinary_polyline_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "PolyLine",
        "Subj" => pdf_string("PolyLine"),
        "NM" => pdf_string("bp:ordinary-polyline"),
        "Vertices" => vec![72.into(), 180.into(), 108.into(), 180.into()],
    });
    let ordinary_polygon_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Polygon",
        "Subj" => pdf_string("Polygon"),
        "NM" => pdf_string("bp:ordinary-polygon"),
        "Vertices" => vec![144.into(), 180.into(), 180.into(), 180.into(), 180.into(), 216.into()],
    });
    let cloud_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Polygon",
        "IT" => "PolygonCloud",
        "Subj" => pdf_string("Cloud"),
        "NM" => pdf_string("bp:cloud-not-area"),
        "Vertices" => vec![288.into(), 180.into(), 324.into(), 180.into(), 324.into(), 216.into()],
        "BE" => dictionary! { "S" => "C", "I" => 2 },
    });
    let direct_legacy_measurement = Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "PolyLine",
        "Subj" => pdf_string("Polylength"),
        "NM" => pdf_string("direct-legacy-polylength"),
        "Vertices" => vec![216.into(), 180.into(), 252.into(), 180.into()],
        "BPDirectProbe" => pdf_string("preserve-unmanaged-direct-dictionary"),
    });
    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {},
            "Contents" => content_id,
            "BPPageScale" => pdf_string(r#"{"pageIndex":0,"source":"custom","name":"1 ft = 36 pt / 1 ft = 18 pt Y","pdfUnits":"in","realUnits":"ft","scaleX":0.027777777777777776,"scaleY":0.05555555555555555,"precision":{"mode":"decimal","value":0.01}}"#),
            "Annots" => vec![
                legacy_polylength_id.into(),
                legacy_area_id.into(),
                standard_polylength_id.into(),
                standard_area_id.into(),
                ordinary_polyline_id.into(),
                ordinary_polygon_id.into(),
                cloud_id.into(),
                direct_legacy_measurement,
            ],
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog_id);
    document.save(path).unwrap();
}

fn straight_line_annotation_fixture(path: &Path) {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let line_appearance_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 148.into(), 48.into()],
            "Resources" => dictionary! {},
            "BPStreamProbe" => pdf_string("line-appearance-preserve-me"),
        },
        b"q 0 0 1 RG 2 w 4 4 m 144 44 l S Q\n".to_vec(),
    ));
    let line_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "Rect" => vec![68.into(), 116.into(), 220.into(), 168.into()],
        "L" => vec![72.into(), 120.into(), 216.into(), 164.into()],
        "NM" => pdf_string("source-line"),
        "Subj" => pdf_string("Imported line subject"),
        "Contents" => pdf_string("Imported line contents"),
        "F" => 32,
        "C" => vec![Object::Real(0.), Object::Real(0.), Object::Real(1.)],
        "BS" => dictionary! { "Type" => "Border", "W" => 2, "S" => "D", "D" => vec![6.into(), 4.into()] },
        "CA" => Object::Real(0.75),
        "AP" => dictionary! { "N" => line_appearance_id },
        "BPProbe" => pdf_string("line-dictionary-preserve-until-edit"),
    });
    let arrow_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "IT" => "linearrow",
        "Rect" => vec![248.into(), 196.into(), 404.into(), 244.into()],
        "L" => vec![252.into(), 200.into(), 400.into(), 240.into()],
        "NM" => pdf_string("bp:source-arrow"),
        "C" => vec![Object::Real(1.), Object::Real(0.), Object::Real(0.)],
        "IC" => vec![Object::Real(1.), Object::Real(0.), Object::Real(0.)],
        "Border" => vec![0.into(), 0.into(), Object::Real(0.5)],
        "ca" => Object::Real(0.8),
        "LE" => vec![Object::Name(b"None".to_vec()), Object::Name(b"ClosedArrow".to_vec())],
    });
    let rect_fallback_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "Rect" => vec![80.into(), 300.into(), 200.into(), 360.into()],
        "NM" => pdf_string("rect-fallback-line"),
        "C" => vec![Object::Real(0.), Object::Real(0.5), Object::Real(0.)],
        "BS" => dictionary! { "Type" => "Border", "W" => 1, "S" => "S" },
    });
    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {},
            "Contents" => content_id,
            "Annots" => vec![line_id.into(), arrow_id.into(), rect_fallback_id.into()],
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document
        .save(path)
        .expect("straight-line annotation fixture must save");
}

fn straight_line_identity_fixture(path: &Path, ambiguous: bool) {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let direct_line = Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "Rect" => vec![36.into(), 36.into(), 108.into(), 72.into()],
        "L" => vec![40.into(), 40.into(), 104.into(), 68.into()],
        "NM" => pdf_string("direct-line"),
        "BPDirectProbe" => pdf_string("preserve-direct-line"),
    });
    let mut annotations = vec![direct_line];
    if ambiguous {
        for name in ["same-line", "bp:same-line"] {
            let object_id = document.add_object(dictionary! {
                "Type" => "Annot",
                "Subtype" => "Line",
                "Rect" => vec![140.into(), 80.into(), 220.into(), 120.into()],
                "L" => vec![144.into(), 84.into(), 216.into(), 116.into()],
                "NM" => pdf_string(name),
            });
            annotations.push(object_id.into());
        }
    }
    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {},
            "Contents" => content_id,
            "Annots" => annotations,
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document.save(path).unwrap();
}

fn production_length_and_dimension_fixture(path: &Path) {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let dimension_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "IT" => "LineDimension",
        "Subj" => pdf_string("Dimension"),
        "Rect" => vec![72.into(), 300.into(), 288.into(), 340.into()],
        "L" => vec![72.into(), 320.into(), 288.into(), 320.into()],
        "NM" => pdf_string("bp:electron-dimension"),
        "Contents" => pdf_string("Existing dimension"),
    });
    let number_format = dictionary! {
        "Type" => "NumberFormat",
        "U" => pdf_string("m"),
        "C" => Object::Real((1. / 72.) as f32),
        "D" => 100,
        "SS" => pdf_string(""),
    };
    let length_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "IT" => "LineDimension",
        "Subj" => pdf_string("Length Measurement"),
        "Rect" => vec![54.into(), 402.into(), 306.into(), 438.into()],
        "L" => vec![72.into(), 420.into(), 288.into(), 420.into()],
        "NM" => pdf_string("bp:electron-length"),
        "Contents" => pdf_string("3.00 m"),
        "Cap" => Object::Boolean(true),
        "Measure" => dictionary! {
            "Type" => "Measure",
            "Subtype" => "RL",
            "R" => pdf_string("1 m = 72 pt"),
            "X" => vec![Object::Dictionary(number_format)],
            "TargetUnitConversion" => Object::Real((1. / 72.) as f32),
        },
    });
    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {},
            "Contents" => content_id,
            "BPPageScale" => pdf_string(r#"{"pageIndex":0,"source":"custom","name":"1 m = 72 pt","pdfUnits":"in","realUnits":"m","scaleX":0.013888888888888888,"scaleY":0.013888888888888888,"precision":{"mode":"decimal","value":0.01}}"#),
            "Annots" => vec![dimension_id.into(), length_id.into()],
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog_id);
    document.save(path).unwrap();
}

fn ink_identity_fixture(
    path: &Path,
    names: &[Option<&str>],
    direct: bool,
    subject: &str,
    blend: &str,
) {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let mut annotations = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let y = 100 + index as i64 * 24;
        let mut annotation = dictionary! {
            "Type" => "Annot",
            "Subtype" => "Ink",
            "Subj" => pdf_string(subject),
            "Rect" => vec![72.into(), (y - 6).into(), 180.into(), (y + 6).into()],
            "InkList" => vec![Object::Array(vec![72.into(), y.into(), 180.into(), y.into()])],
            "C" => vec![1.into(), 1.into(), 0.into()],
            "CA" => Object::Real(1.),
            "BS" => dictionary! { "Type" => "Border", "W" => 12, "S" => "S" },
            "BM" => blend,
        };
        if let Some(name) = name {
            annotation.set("NM", pdf_string(name));
        }
        annotations.push(if direct {
            Object::Dictionary(annotation)
        } else {
            document.add_object(annotation).into()
        });
    }
    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {},
            "Contents" => content_id,
            "Annots" => annotations,
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog_id);
    document.save(path).unwrap();
}

#[test]
fn ink_import_rejects_ambiguous_names_and_retains_exact_object_identity() {
    let scratch = ScratchDirectory::new();
    let ambiguous = scratch.path().join("ambiguous-ink.pdf");
    ink_identity_fixture(
        &ambiguous,
        &[Some("ink-1"), Some("bp:ink-1")],
        false,
        "Highlight",
        "Multiply",
    );
    let error = match PdfPersistenceSession::open(&ambiguous) {
        Ok(_) => panic!("native names that normalize to one stable id must fail closed"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("ambiguous Ink identity ink-1"));

    let unnamed = scratch.path().join("unnamed-ink.pdf");
    let saved = scratch.path().join("unnamed-ink-saved.pdf");
    ink_identity_fixture(&unnamed, &[None], false, "Highlight", "Multiply");
    let mut session = PdfPersistenceSession::open(&unnamed).unwrap();
    assert_eq!(session.pens()[0].id.as_str(), "page-0-annotation-0");
    let mut edited = session.pens()[0].clone();
    edited.appearance = PenAppearance::new("#ffff00", 10., 0.75).unwrap();
    session.replace_pen(edited.clone()).unwrap();
    session.save_as(&saved).unwrap();
    let reopened = PdfPersistenceSession::open(&saved).unwrap();
    assert_eq!(reopened.pens(), &[edited]);
    assert!(
        reopened.pen_has_canonical_native_identity(&MarkupId::new("page-0-annotation-0").unwrap())
    );

    let direct = scratch.path().join("direct-ink.pdf");
    ink_identity_fixture(
        &direct,
        &[Some("direct-ink")],
        true,
        "Highlight",
        "Multiply",
    );
    let direct_session = PdfPersistenceSession::open(&direct).unwrap();
    assert!(direct_session.pens().is_empty());
    assert!(
        direct_session
            .untouched_annotations()
            .iter()
            .any(|annotation| { annotation.name == "direct-ink" && annotation.subtype == "Ink" })
    );
}

#[test]
fn ink_import_matches_electron_subject_classification_case_insensitively() {
    let scratch = ScratchDirectory::new();
    let lowercase = scratch.path().join("lowercase-highlight.pdf");
    ink_identity_fixture(
        &lowercase,
        &[Some("lowercase-highlight")],
        false,
        "hIgHlIgHt",
        "Normal",
    );
    let highlight = PdfPersistenceSession::open(&lowercase).unwrap();
    assert_eq!(
        highlight.pens()[0].tool(),
        butter_paper_gpui_gallery::annotation_model::InkTool::Highlight
    );

    let multiply_pen = scratch.path().join("multiply-pen.pdf");
    ink_identity_fixture(
        &multiply_pen,
        &[Some("multiply-pen")],
        false,
        "Pen",
        "Multiply",
    );
    let pen = PdfPersistenceSession::open(&multiply_pen).unwrap();
    assert_eq!(
        pen.pens()[0].tool(),
        butter_paper_gpui_gallery::annotation_model::InkTool::Pen
    );
}

#[test]
fn production_measure_import_distinguishes_dimension_and_preserves_managed_length_identity() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("production-length-source.pdf");
    let target = scratch.path().join("production-length-edited.pdf");
    production_length_and_dimension_fixture(&source);

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    assert_eq!(session.lengths().len(), 1);
    assert_eq!(session.dimensions().len(), 1);
    assert!(session.straight_lines().is_empty());
    assert_eq!(session.lengths()[0].id.as_str(), "electron-length");
    assert_eq!(session.lengths()[0].caption(), "3.00 m");
    assert_eq!(session.dimensions()[0].id.as_str(), "electron-dimension");
    assert_eq!(session.dimensions()[0].content(), "Existing dimension");
    let imported_dimension = session.dimensions()[0].clone();
    assert!(!session.untouched_annotations().iter().any(|annotation| {
        annotation.name == "bp:electron-dimension" && annotation.subtype == "Line"
    }));

    let imported = session.lengths()[0].clone();
    let edited = LengthAnnotation::new(
        imported.id.clone(),
        imported.page_index,
        imported.start,
        point(324., 420.),
        imported.calibration().clone(),
    )
    .unwrap();
    assert_eq!(edited.caption(), "3.50 m");
    session.replace_length(edited.clone()).unwrap();
    session.save_as(&target).unwrap();

    let reopened = PdfPersistenceSession::open(&target).unwrap();
    assert_eq!(reopened.lengths(), &[edited]);
    assert!(reopened.dimensions()[0].same_persisted_state_as(&imported_dimension));
    assert!(reopened.straight_lines().is_empty());
    assert!(!reopened.untouched_annotations().iter().any(|annotation| {
        annotation.name == "bp:electron-dimension" && annotation.subtype == "Line"
    }));
    let document = Document::load(&target).unwrap();
    let raw = annotation_dictionary(&document, "bp:electron-length");
    assert_eq!(
        raw.get(b"NM").unwrap().as_str().unwrap(),
        b"bp:electron-length"
    );
    assert_eq!(raw.get(b"MeasurementTypes").unwrap().as_i64().unwrap(), 130);
    assert_eq!(raw.get(b"LE").unwrap().as_array().unwrap().len(), 2);
    assert_eq!(
        raw.get(b"DA").unwrap().as_str().unwrap(),
        b"1 0 0 rg /Helv 12 Tf"
    );
}

#[derive(Debug, Eq, PartialEq)]
struct UnknownAnnotationOracle {
    contents: Vec<u8>,
    dictionary_probe: Vec<u8>,
    appearance_probe: Vec<u8>,
    appearance_bytes: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
struct StraightLineAnnotationOracle {
    dictionary: String,
    appearance_dictionary: String,
    appearance_bytes: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
struct OriginalDocumentOracle {
    page_content: Vec<u8>,
    media_box: Vec<String>,
    crop_box: Vec<String>,
    title: Vec<u8>,
    author: Vec<u8>,
    metadata_probe: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
struct VendorRedactOracle {
    rect: String,
    quad_points: String,
    interior_color: String,
    overlay_text: Vec<u8>,
    repeat: bool,
    default_appearance: Vec<u8>,
    justification: i64,
    vendor_probe: Vec<u8>,
    appearance_bytes: Vec<u8>,
    rollover_bytes: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
struct SnapshotPreservationOracle {
    vendor_contents: Vec<u8>,
    vendor_probe: Vec<u8>,
    vendor_appearance_probe: Vec<u8>,
    vendor_appearance_bytes: Vec<u8>,
    malformed_contents: Vec<u8>,
    malformed_probe: Vec<u8>,
}

fn resolve<'a>(document: &'a Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => document
            .get_object(*id)
            .expect("referenced object must exist"),
        _ => object,
    }
}

fn annotation_dictionary<'a>(document: &'a Document, name: &str) -> &'a Dictionary {
    let page_id = document.get_pages()[&1];
    let page = document
        .get_object(page_id)
        .expect("page must exist")
        .as_dict()
        .expect("page must be a dictionary");
    let annotations = resolve(
        document,
        page.get(b"Annots").expect("page must have annotations"),
    )
    .as_array()
    .expect("annotations must be an array");
    annotations
        .iter()
        .map(|annotation| {
            resolve(document, annotation)
                .as_dict()
                .expect("annotation must be a dictionary")
        })
        .find(|annotation| {
            annotation
                .get(b"NM")
                .ok()
                .and_then(|value| value.as_str().ok())
                == Some(name.as_bytes())
        })
        .expect("named annotation must exist")
}

fn vendor_redact_oracle(path: &Path) -> VendorRedactOracle {
    let document = Document::load(path).expect("vendor Redact oracle must load");
    let annotation = annotation_dictionary(&document, "vendor-redact");
    let appearance = resolve(&document, annotation.get(b"AP").unwrap())
        .as_dict()
        .unwrap();
    let normal = resolve(&document, appearance.get(b"N").unwrap())
        .as_stream()
        .unwrap();
    let rollover = resolve(&document, annotation.get(b"RO").unwrap())
        .as_stream()
        .unwrap();
    VendorRedactOracle {
        rect: format!("{:?}", annotation.get(b"Rect").unwrap()),
        quad_points: format!("{:?}", annotation.get(b"QuadPoints").unwrap()),
        interior_color: format!("{:?}", annotation.get(b"IC").unwrap()),
        overlay_text: annotation
            .get(b"OverlayText")
            .unwrap()
            .as_str()
            .unwrap()
            .to_vec(),
        repeat: annotation.get(b"Repeat").unwrap().as_bool().unwrap(),
        default_appearance: annotation.get(b"DA").unwrap().as_str().unwrap().to_vec(),
        justification: annotation.get(b"Q").unwrap().as_i64().unwrap(),
        vendor_probe: annotation
            .get(b"BPVendorRedact")
            .unwrap()
            .as_str()
            .unwrap()
            .to_vec(),
        appearance_bytes: normal.content.clone(),
        rollover_bytes: rollover.content.clone(),
    }
}

fn snapshot_preservation_oracle(path: &Path) -> SnapshotPreservationOracle {
    let document = Document::load(path).expect("Snapshot preservation oracle must load");
    let vendor = annotation_dictionary(&document, "vendor-snapshot");
    let vendor_appearance = normal_appearance(&document, vendor);
    let malformed = annotation_dictionary(&document, "bp:malformed-snapshot");
    SnapshotPreservationOracle {
        vendor_contents: vendor.get(b"Contents").unwrap().as_str().unwrap().to_vec(),
        vendor_probe: vendor
            .get(b"BPVendorSnapshot")
            .unwrap()
            .as_str()
            .unwrap()
            .to_vec(),
        vendor_appearance_probe: vendor_appearance
            .dict
            .get(b"BPVendorAppearance")
            .unwrap()
            .as_str()
            .unwrap()
            .to_vec(),
        vendor_appearance_bytes: vendor_appearance.content.clone(),
        malformed_contents: malformed
            .get(b"Contents")
            .unwrap()
            .as_str()
            .unwrap()
            .to_vec(),
        malformed_probe: malformed
            .get(b"BPMalformedSnapshot")
            .unwrap()
            .as_str()
            .unwrap()
            .to_vec(),
    }
}

fn page_annotation_names(path: &Path) -> Vec<String> {
    let document = Document::load(path).expect("saved PDF must reopen independently");
    let page_id = document.get_pages()[&1];
    let page = document.get_object(page_id).unwrap().as_dict().unwrap();
    resolve(&document, page.get(b"Annots").unwrap())
        .as_array()
        .unwrap()
        .iter()
        .map(|annotation| {
            resolve(&document, annotation)
                .as_dict()
                .unwrap()
                .get(b"NM")
                .unwrap()
                .as_str()
                .unwrap()
        })
        .map(|name| String::from_utf8(name.to_vec()).unwrap())
        .collect()
}

fn unknown_annotation_oracle(path: &Path) -> UnknownAnnotationOracle {
    let document = Document::load(path).expect("saved PDF must reopen independently");
    let annotation = annotation_dictionary(&document, "unknown-1");
    let normal_appearance = resolve(
        &document,
        annotation
            .get(b"AP")
            .expect("unknown annotation must keep AP")
            .as_dict()
            .expect("AP must be a dictionary")
            .get(b"N")
            .expect("AP must keep its normal appearance"),
    )
    .as_stream()
    .expect("normal appearance must be a stream");
    UnknownAnnotationOracle {
        contents: annotation
            .get(b"Contents")
            .expect("unknown annotation must keep Contents")
            .as_str()
            .expect("Contents must be a string")
            .to_vec(),
        dictionary_probe: annotation
            .get(b"BPUnknown")
            .expect("unknown dictionary probe must survive")
            .as_str()
            .expect("unknown dictionary probe must be a string")
            .to_vec(),
        appearance_probe: normal_appearance
            .dict
            .get(b"BPStreamProbe")
            .expect("unknown appearance probe must survive")
            .as_str()
            .expect("unknown appearance probe must be a string")
            .to_vec(),
        appearance_bytes: normal_appearance.content.clone(),
    }
}

fn straight_line_annotation_oracle(path: &Path, name: &str) -> StraightLineAnnotationOracle {
    let document = Document::load(path).expect("straight-line oracle must load");
    let annotation = annotation_dictionary(&document, name);
    let appearance = normal_appearance(&document, annotation);
    StraightLineAnnotationOracle {
        dictionary: format!("{annotation:?}"),
        appearance_dictionary: format!("{:?}", appearance.dict),
        appearance_bytes: appearance.content.clone(),
    }
}

fn original_document_oracle(path: &Path) -> OriginalDocumentOracle {
    let document = Document::load(path).expect("document oracle must load");
    let page = document
        .get_object(document.get_pages()[&1])
        .unwrap()
        .as_dict()
        .unwrap();
    let content = resolve(&document, page.get(b"Contents").unwrap())
        .as_stream()
        .unwrap();
    let array_strings = |key: &[u8]| {
        page.get(key)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|object| format!("{object:?}"))
            .collect::<Vec<_>>()
    };
    let info = resolve(&document, document.trailer.get(b"Info").unwrap())
        .as_dict()
        .unwrap();
    let string = |key: &[u8]| info.get(key).unwrap().as_str().unwrap().to_vec();
    OriginalDocumentOracle {
        page_content: content.content.clone(),
        media_box: array_strings(b"MediaBox"),
        crop_box: array_strings(b"CropBox"),
        title: string(b"Title"),
        author: string(b"Author"),
        metadata_probe: string(b"BPInfoProbe"),
    }
}

fn normal_appearance<'a>(document: &'a Document, annotation: &'a Dictionary) -> &'a Stream {
    resolve(
        document,
        annotation
            .get(b"AP")
            .expect("native annotation must have AP")
            .as_dict()
            .expect("AP must be a dictionary")
            .get(b"N")
            .expect("AP must have a normal appearance"),
    )
    .as_stream()
    .expect("normal appearance must be a stream")
}

fn image_appearance_graph_ids(document: &Document, annotation: &Dictionary) -> [(u32, u16); 3] {
    let form_id = annotation
        .get(b"AP")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"N")
        .unwrap()
        .as_reference()
        .unwrap();
    let form = document.get_object(form_id).unwrap().as_stream().unwrap();
    let image_id = form
        .dict
        .get(b"Resources")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"XObject")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Im0")
        .unwrap()
        .as_reference()
        .unwrap();
    let alpha_id = document
        .get_object(image_id)
        .unwrap()
        .as_stream()
        .unwrap()
        .dict
        .get(b"SMask")
        .unwrap()
        .as_reference()
        .unwrap();
    [form_id, image_id, alpha_id]
}

fn assert_native_representative_contracts(path: &Path, edited: bool) {
    let document = Document::load(path).expect("native contract PDF must load");
    let highlight = annotation_dictionary(&document, "bp:highlight-1");
    assert_eq!(
        highlight.get(b"Subtype").unwrap().as_name().unwrap(),
        b"Ink"
    );
    assert_eq!(
        highlight.get(b"Subj").unwrap().as_str().unwrap(),
        b"Highlight"
    );
    assert_eq!(
        highlight.get(b"BM").unwrap().as_name().unwrap(),
        b"Multiply"
    );
    let ink_path = highlight.get(b"InkList").unwrap().as_array().unwrap()[0]
        .as_array()
        .unwrap();
    assert_eq!(
        highlight.get(b"InkList").unwrap().as_array().unwrap().len(),
        2
    );
    assert_eq!(ink_path.len(), if edited { 10 } else { 8 });
    normal_appearance(&document, highlight);

    let text = annotation_dictionary(&document, "text-1");
    assert_eq!(
        text.get(b"Subtype").unwrap().as_name().unwrap(),
        b"FreeText"
    );
    assert_eq!(
        text.get(b"Contents").unwrap().as_str().unwrap(),
        if edited {
            b"Beam B-12 / revision 5"
        } else {
            b"Beam B-12 / revision 4"
        },
    );
    assert!(
        text.get(b"DA")
            .unwrap()
            .as_str()
            .unwrap()
            .starts_with(b"/Helv 14")
    );
    normal_appearance(&document, text);

    let length = annotation_dictionary(&document, "bp:length-1");
    assert_eq!(length.get(b"Subtype").unwrap().as_name().unwrap(), b"Line");
    assert_eq!(
        length.get(b"IT").unwrap().as_name().unwrap(),
        b"LineDimension"
    );
    assert_eq!(
        length.get(b"Subj").unwrap().as_str().unwrap(),
        b"Length Measurement"
    );
    assert_eq!(length.get(b"L").unwrap().as_array().unwrap().len(), 4);
    assert_eq!(
        length.get(b"Contents").unwrap().as_str().unwrap(),
        if edited {
            b"Span: 3.50 m"
        } else {
            b"Span: 3.00 m"
        },
    );
    assert!(!length.get(b"Cap").unwrap().as_bool().unwrap());
    assert!(length.get(b"BPScale").unwrap().as_dict().is_ok());
    assert!(
        !normal_appearance(&document, length)
            .content
            .windows(2)
            .any(|window| window == b"BT"),
        "a hidden Length caption must not be painted into the appearance stream",
    );

    let image = annotation_dictionary(&document, "bp:image-1");
    assert_eq!(image.get(b"Subtype").unwrap().as_name().unwrap(), b"Square");
    assert_eq!(image.get(b"IT").unwrap().as_name().unwrap(), b"SquareImage");
    assert_eq!(image.get(b"Subj").unwrap().as_str().unwrap(), b"Image");
    let appearance = normal_appearance(&document, image);
    let image_stream = resolve(
        &document,
        appearance
            .dict
            .get(b"Resources")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"XObject")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Im0")
            .unwrap(),
    )
    .as_stream()
    .unwrap();
    assert_eq!(
        image_stream
            .dict
            .get(b"Subtype")
            .unwrap()
            .as_name()
            .unwrap(),
        b"Image"
    );
    assert_eq!(
        image_stream.dict.get(b"Width").unwrap().as_i64().unwrap(),
        2
    );
    assert_eq!(
        image_stream.dict.get(b"Height").unwrap().as_i64().unwrap(),
        2
    );
    assert_eq!(
        image_stream.content,
        vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]
    );
    let alpha = resolve(&document, image_stream.dict.get(b"SMask").unwrap())
        .as_stream()
        .unwrap();
    assert_eq!(alpha.content, vec![255, 255, 255, 255]);
}

fn validate_independently(path: &Path) {
    let qpdf = Command::new("qpdf")
        .arg("--check")
        .arg(path)
        .output()
        .expect("qpdf must be installed for the persistence gate");
    assert!(
        qpdf.status.success(),
        "qpdf rejected {}:\n{}\n{}",
        path.display(),
        String::from_utf8_lossy(&qpdf.stdout),
        String::from_utf8_lossy(&qpdf.stderr),
    );
    let pdfinfo = Command::new("pdfinfo")
        .arg(path)
        .output()
        .expect("pdfinfo must be installed for the persistence gate");
    assert!(
        pdfinfo.status.success(),
        "pdfinfo rejected {}:\n{}",
        path.display(),
        String::from_utf8_lossy(&pdfinfo.stderr),
    );
    assert!(String::from_utf8_lossy(&pdfinfo.stdout).contains("Pages:           1"));
}

#[test]
fn imports_rectangle_and_records_untouched_unknown_annotation() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("public-annotation-source.pdf");
    public_annotation_fixture(&source);

    let session = PdfPersistenceSession::open(&source).expect("fixture must open");

    assert_eq!(session.page_count(), 1);
    assert_eq!(session.rectangles().len(), 1);
    let rectangle = &session.rectangles()[0];
    assert_eq!(rectangle.id.as_str(), "source-rectangle");
    assert_eq!(rectangle.page_index, 0);
    assert_eq!(rectangle.rect.x, 72.0);
    assert_eq!(rectangle.rect.y, 144.0);
    assert_eq!(rectangle.rect.width, 180.0);
    assert_eq!(rectangle.rect.height, 96.0);
    assert_eq!(rectangle.appearance.stroke_color(), "#1d6ed8");
    assert_eq!(rectangle.appearance.stroke_width_pt(), 2.0);
    assert_eq!(
        session
            .untouched_annotations()
            .iter()
            .map(|annotation| (annotation.name.as_str(), annotation.subtype.as_str()))
            .collect::<Vec<_>>(),
        [("unknown-1", "Text")],
    );
}

#[test]
fn imports_plain_line_arrow_and_rect_fallback_as_typed_straight_lines() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("straight-line-source.pdf");
    straight_line_annotation_fixture(&source);

    let session = PdfPersistenceSession::open(&source).expect("fixture must open");

    assert_eq!(session.straight_lines().len(), 3);
    let line = session
        .straight_lines()
        .iter()
        .find(|annotation| annotation.id.as_str() == "source-line")
        .unwrap();
    assert_eq!(line.kind, LineKind::Line);
    assert_eq!(
        (line.start, line.end),
        (point(72., 120.), point(216., 164.))
    );
    assert_eq!(line.appearance.stroke_color(), "#0000ff");
    assert_eq!(line.appearance.stroke_width_pt(), 2.);
    assert_eq!(line.appearance.opacity(), 0.75);
    assert_eq!(line.appearance.stroke_style(), StrokeStyle::Dashed);

    let arrow = session
        .straight_lines()
        .iter()
        .find(|annotation| annotation.id.as_str() == "source-arrow")
        .unwrap();
    assert_eq!(arrow.kind, LineKind::Arrow);
    assert_eq!(arrow.appearance.stroke_width_pt(), 0.5);
    assert_eq!(arrow.appearance.opacity(), 0.8);
    assert!(session.straight_line_has_canonical_native_identity(&arrow.id));

    let fallback = session
        .straight_lines()
        .iter()
        .find(|annotation| annotation.id.as_str() == "rect-fallback-line")
        .unwrap();
    assert_eq!(fallback.kind, LineKind::Line);
    assert_eq!(
        (fallback.start, fallback.end),
        (point(80., 300.), point(200., 360.))
    );
    assert!(
        session
            .untouched_annotations()
            .iter()
            .all(|annotation| annotation.subtype != "Line")
    );
}

#[test]
fn unchanged_straight_line_preserves_custom_dictionary_and_appearance_graph_exactly() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("straight-line-source.pdf");
    let output = scratch.path().join("straight-line-unchanged.pdf");
    straight_line_annotation_fixture(&source);
    let source_oracle = straight_line_annotation_oracle(&source, "source-line");

    PdfPersistenceSession::open(&source)
        .unwrap()
        .save_as(&output)
        .unwrap();

    assert_eq!(
        straight_line_annotation_oracle(&output, "source-line"),
        source_oracle,
        "an unchanged typed line must keep its source dictionary and /AP graph byte-for-byte",
    );
}

#[test]
fn edited_created_and_deleted_straight_lines_rebuild_only_the_owned_safe_dictionary() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("straight-line-source.pdf");
    let first_output = scratch.path().join("straight-line-edited.pdf");
    let second_output = scratch.path().join("straight-line-reopened.pdf");
    straight_line_annotation_fixture(&source);

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    let mut edited = session
        .straight_lines()
        .iter()
        .find(|annotation| annotation.id.as_str() == "source-line")
        .unwrap()
        .clone();
    edited.start = point(90., 130.);
    edited.end = point(240., 180.);
    edited.appearance =
        StraightLineAppearance::new("#16a34a", 3., 0.6, StrokeStyle::Solid).unwrap();
    edited.locked = true;
    session.replace_straight_line(edited.clone()).unwrap();
    session
        .remove_straight_line(&MarkupId::new("source-arrow").unwrap())
        .unwrap();
    let created = StraightLineAnnotation::new(
        MarkupId::new("created-arrow").unwrap(),
        0,
        point(300., 320.),
        point(420., 380.),
        LineKind::Arrow,
        StraightLineAppearance::new("#dc2626", 0.5, 0.8, StrokeStyle::Solid).unwrap(),
    )
    .unwrap();
    session.add_straight_line(created.clone()).unwrap();
    let mut locked_created = StraightLineAnnotation::new(
        MarkupId::new("created-locked-arrow").unwrap(),
        0,
        point(320., 440.),
        point(460., 440.),
        LineKind::Arrow,
        StraightLineAppearance::default_for(LineKind::Arrow),
    )
    .unwrap();
    locked_created.locked = true;
    session.add_straight_line(locked_created.clone()).unwrap();
    session.save_as(&first_output).unwrap();
    validate_independently(&first_output);

    let reopened = PdfPersistenceSession::open(&first_output).unwrap();
    assert!(reopened.straight_lines().contains(&edited));
    assert!(reopened.straight_lines().contains(&created));
    assert!(reopened.straight_lines().contains(&locked_created));
    assert!(
        reopened
            .straight_lines()
            .iter()
            .all(|annotation| annotation.id.as_str() != "source-arrow")
    );
    assert!(reopened.straight_line_has_canonical_native_identity(&edited.id));
    assert!(reopened.straight_line_has_canonical_native_identity(&created.id));
    assert!(reopened.straight_line_has_canonical_native_identity(&locked_created.id));

    let saved = Document::load(&first_output).unwrap();
    let saved_line = annotation_dictionary(&saved, "bp:source-line");
    assert!(saved_line.get(b"AP").is_err());
    assert!(saved_line.get(b"BPProbe").is_err());
    assert!(saved_line.get(b"IT").is_err());
    assert!(saved_line.get(b"LE").is_err());
    assert!(saved_line.get(b"IC").is_err());
    assert_eq!(
        saved_line.get(b"F").unwrap().as_i64().unwrap(),
        32 | 128,
        "an edited Line must preserve imported flags while toggling only Locked",
    );
    assert_eq!(
        saved_line.get(b"Subj").unwrap().as_str().unwrap(),
        b"Imported line subject"
    );
    assert_eq!(
        saved_line.get(b"Contents").unwrap().as_str().unwrap(),
        b"Imported line contents"
    );
    assert_eq!(
        saved_line.get(b"ca").unwrap(),
        saved_line.get(b"CA").unwrap()
    );
    assert_eq!(
        saved_line
            .get(b"Rect")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|value| f64::from(value.as_float().unwrap()))
            .collect::<Vec<_>>(),
        [86., 126., 244., 184.],
    );

    let saved_arrow = annotation_dictionary(&saved, "bp:created-arrow");
    assert_eq!(
        saved_arrow.get(b"IT").unwrap().as_name().unwrap(),
        b"LineArrow"
    );
    assert_eq!(
        saved_arrow
            .get(b"LE")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_name().unwrap())
            .collect::<Vec<_>>(),
        [b"None".as_slice(), b"ClosedArrow".as_slice()],
    );
    assert_eq!(
        saved_arrow.get(b"IC").unwrap(),
        saved_arrow.get(b"C").unwrap()
    );
    assert_eq!(
        saved_arrow.get(b"ca").unwrap(),
        saved_arrow.get(b"CA").unwrap()
    );
    let saved_arrow_border_style = saved_arrow.get(b"BS").unwrap().as_dict().unwrap();
    assert_eq!(
        saved_arrow_border_style
            .get(b"S")
            .unwrap()
            .as_name()
            .unwrap(),
        b"S"
    );
    assert!(saved_arrow_border_style.get(b"D").is_err());
    assert_eq!(
        saved_arrow.get(b"F").unwrap().as_i64().unwrap(),
        4,
        "a newly created Arrow must use Electron's printable annotation flag",
    );
    assert_eq!(
        saved_arrow.get(b"Subj").unwrap().as_str().unwrap(),
        b"Arrow"
    );
    assert_eq!(saved_arrow.get(b"Contents").unwrap().as_str().unwrap(), b"");
    assert!(saved_arrow.get(b"AP").is_err());
    let saved_locked_arrow = annotation_dictionary(&saved, "bp:created-locked-arrow");
    assert_eq!(
        saved_locked_arrow.get(b"F").unwrap().as_i64().unwrap(),
        128,
        "a newly created locked Arrow must replace the default Print flag exactly as Electron does",
    );

    reopened.save_as(&second_output).unwrap();
    assert_eq!(
        PdfPersistenceSession::open(&second_output)
            .unwrap()
            .straight_lines(),
        reopened.straight_lines(),
    );
}

#[test]
fn straight_line_import_fails_closed_for_direct_and_ambiguous_native_identity() {
    let scratch = ScratchDirectory::new();
    let direct_source = scratch.path().join("direct-line-source.pdf");
    let direct_output = scratch.path().join("direct-line-output.pdf");
    straight_line_identity_fixture(&direct_source, false);
    let original_direct = format!(
        "{:?}",
        annotation_dictionary(&Document::load(&direct_source).unwrap(), "direct-line")
    );

    let direct_session = PdfPersistenceSession::open(&direct_source).unwrap();
    assert!(direct_session.straight_lines().is_empty());
    assert_eq!(
        direct_session.untouched_annotations(),
        &[butter_paper_gpui_gallery::pdf_engine::UntouchedAnnotation {
            name: "direct-line".into(),
            subtype: "Line".into(),
        }]
    );
    direct_session.save_as(&direct_output).unwrap();
    assert_eq!(
        format!(
            "{:?}",
            annotation_dictionary(&Document::load(&direct_output).unwrap(), "direct-line")
        ),
        original_direct,
    );

    let ambiguous_source = scratch.path().join("ambiguous-line-source.pdf");
    straight_line_identity_fixture(&ambiguous_source, true);
    let error = match PdfPersistenceSession::open(&ambiguous_source) {
        Ok(_) => panic!("normalized duplicate straight-line identities must fail closed"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        PdfPersistenceError::InvalidDocument(ref message)
            if message.contains("ambiguous straight-line identity same-line")
    ));
}

#[test]
fn edits_native_rectangle_and_safely_round_trips_twice_without_touching_unknown_annotation() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("public-annotation-source.pdf");
    let first_output = scratch.path().join("first-save.pdf");
    let second_output = scratch.path().join("second-save.pdf");
    public_annotation_fixture(&source);
    let source_unknown = unknown_annotation_oracle(&source);

    let mut source_session = PdfPersistenceSession::open(&source).expect("fixture must open");
    let mut edited = source_session.rectangles()[0].clone();
    edited.rect = PdfRect::new(90.0, 132.0, 210.0, 96.0).unwrap();
    edited.appearance = RectangleAppearance::new("#dc2626", 3.0, Some("#dc2626"), 0.12).unwrap();
    source_session
        .replace_rectangle(edited)
        .expect("imported rectangle must be replaceable by stable identity");
    source_session
        .save_as(&first_output)
        .expect("first safe save must succeed");
    validate_independently(&first_output);
    assert_eq!(unknown_annotation_oracle(&first_output), source_unknown);

    let first_reopen =
        PdfPersistenceSession::open(&first_output).expect("first output must reopen");
    let rectangle = &first_reopen.rectangles()[0];
    assert_eq!(rectangle.id.as_str(), "source-rectangle");
    assert_eq!(
        rectangle.rect,
        PdfRect::new(90.0, 132.0, 210.0, 96.0).unwrap()
    );
    assert_eq!(rectangle.appearance.stroke_color(), "#dc2626");
    assert_eq!(rectangle.appearance.fill_color(), Some("#dc2626"));
    assert_eq!(rectangle.appearance.stroke_width_pt(), 3.0);
    assert!((rectangle.appearance.opacity() - 0.12).abs() < 0.0001);
    assert!(
        annotation_dictionary(&Document::load(&first_output).unwrap(), "source-rectangle")
            .get(b"BPProbe")
            .is_err(),
        "editing must replace the source rectangle dictionary instead of retaining unsafe keys",
    );

    first_reopen
        .save_as(&second_output)
        .expect("second safe save must succeed");
    validate_independently(&second_output);
    assert_eq!(unknown_annotation_oracle(&second_output), source_unknown);
    let second_reopen =
        PdfPersistenceSession::open(&second_output).expect("second output must reopen");
    assert_eq!(second_reopen.rectangles(), first_reopen.rectangles());
}

#[test]
fn appends_a_created_rectangle_and_reopens_it_twice() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("public-annotation-source.pdf");
    let first_output = scratch.path().join("first-created-save.pdf");
    let second_output = scratch.path().join("second-created-save.pdf");
    public_annotation_fixture(&source);
    let source_unknown = unknown_annotation_oracle(&source);

    let mut session = PdfPersistenceSession::open(&source).expect("fixture must open");
    session
        .add_rectangle(RectangleAnnotation {
            id: MarkupId::new("created-rectangle").unwrap(),
            page_index: 0,
            rect: PdfRect::new(320.0, 300.0, 144.0, 72.0).unwrap(),
            rotation_degrees: 30.0,
            appearance: RectangleAppearance::new("#16a34a", 4.0, Some("#86efac"), 0.4).unwrap(),
            locked: false,
        })
        .expect("a new stable rectangle must be appendable");
    session
        .save_as(&first_output)
        .expect("created rectangle must save");
    validate_independently(&first_output);
    assert_eq!(unknown_annotation_oracle(&first_output), source_unknown);

    let first_reopen =
        PdfPersistenceSession::open(&first_output).expect("first output must reopen");
    let created = first_reopen
        .rectangles()
        .iter()
        .find(|rectangle| rectangle.id.as_str() == "created-rectangle")
        .expect("created rectangle must import by stable identity");
    assert_eq!(
        created.rect,
        PdfRect::new(320.0, 300.0, 144.0, 72.0).unwrap()
    );
    assert_eq!(created.rotation_degrees, 30.0);
    assert_eq!(created.appearance.stroke_color(), "#16a34a");
    assert_eq!(created.appearance.fill_color(), Some("#86efac"));
    assert_eq!(created.appearance.stroke_width_pt(), 4.0);
    assert!((created.appearance.opacity() - 0.4).abs() < 0.0001);

    first_reopen
        .save_as(&second_output)
        .expect("second save must succeed");
    validate_independently(&second_output);
    assert_eq!(unknown_annotation_oracle(&second_output), source_unknown);
    let second_reopen =
        PdfPersistenceSession::open(&second_output).expect("second output must reopen");
    assert_eq!(second_reopen.rectangles(), first_reopen.rectangles());
}

#[test]
fn safe_save_refuses_to_replace_an_existing_target_and_removes_its_temporary_file() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("public-annotation-source.pdf");
    let target = scratch.path().join("existing.pdf");
    public_annotation_fixture(&source);
    fs::write(&target, b"keep this exact file").unwrap();
    let session = PdfPersistenceSession::open(&source).expect("fixture must open");

    let error = session
        .save_as(&target)
        .expect_err("safe save must not replace an existing target");

    assert!(matches!(
        error,
        PdfPersistenceError::Io(ref error)
            if error.kind() == std::io::ErrorKind::AlreadyExists
    ));
    assert_eq!(fs::read(&target).unwrap(), b"keep this exact file");
    assert!(
        fs::read_dir(scratch.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")),
        "a refused save must not leave a same-directory temporary file",
    );
}

#[test]
fn cross_family_managed_order_survives_two_reopens_without_moving_untouched_slots() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("ordered-source.pdf");
    let first = scratch.path().join("ordered-first.pdf");
    let second = scratch.path().join("ordered-second.pdf");
    public_annotation_fixture(&source);
    let mut session = PdfPersistenceSession::open(&source).unwrap();

    let line = StraightLineAnnotation::new(
        MarkupId::new("order-line").unwrap(),
        0,
        point(280., 120.),
        point(360., 160.),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let pen = PenAnnotation::new(
        MarkupId::new("order-pen").unwrap(),
        0,
        vec![point(80., 300.), point(180., 310.)],
        PenAppearance::new("#ff0000", 2., 1.).unwrap(),
    )
    .unwrap();
    let text = TextBoxAnnotation::new(
        MarkupId::new("order-text").unwrap(),
        0,
        PdfRect::new(72., 360., 180., 60.).unwrap(),
        "Order",
        TextBoxStyle::new("Helvetica", 12., "#111827", 1.).unwrap(),
    )
    .unwrap();
    let length = LengthAnnotation::new(
        MarkupId::new("order-length").unwrap(),
        0,
        point(90., 500.),
        point(300., 500.),
        LengthCalibration::from_scale(72., 1., "m", 2, true).unwrap(),
    )
    .unwrap();
    let image = ImageAnnotation::new(
        MarkupId::new("order-image").unwrap(),
        0,
        PdfRect::new(380., 360., 80., 60.).unwrap(),
        DecodedRgbaAsset::new(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).unwrap(),
        false,
    )
    .unwrap();
    session.add_straight_line(line.clone()).unwrap();
    session.add_pen(pen.clone()).unwrap();
    session.add_text_box(text.clone()).unwrap();
    session.add_length(length.clone()).unwrap();
    session.add_image(image.clone()).unwrap();
    let expected = vec![
        text.id.clone(),
        MarkupId::new("source-rectangle").unwrap(),
        image.id.clone(),
        line.id.clone(),
        pen.id.clone(),
        length.id.clone(),
    ];
    session.reorder_managed_annotations(&expected).unwrap();
    assert_eq!(session.annotation_order(), expected);
    assert_eq!(
        session
            .annotations_in_document_order()
            .iter()
            .map(Annotation::id)
            .cloned()
            .collect::<Vec<_>>(),
        expected,
    );
    session.save_as(&first).unwrap();
    assert_eq!(
        page_annotation_names(&first),
        vec![
            "order-text",
            "unknown-1",
            "source-rectangle",
            "bp:order-image",
            "bp:order-line",
            "bp:order-pen",
            "bp:order-length",
        ],
    );
    let reopened = PdfPersistenceSession::open(&first).unwrap();
    assert_eq!(reopened.annotation_order(), expected);
    reopened.save_as(&second).unwrap();
    assert_eq!(
        PdfPersistenceSession::open(&second)
            .unwrap()
            .annotation_order(),
        expected
    );
    assert_eq!(
        page_annotation_names(&second),
        page_annotation_names(&first)
    );
}

#[test]
fn all_representative_native_annotations_add_replace_and_round_trip_twice() {
    let scratch = ScratchDirectory::new();
    let source = scratch.path().join("bp-annotation-all-v1-source.pdf");
    let first_output = scratch.path().join("bp-annotation-all-v1-first.pdf");
    let second_output = scratch.path().join("bp-annotation-all-v1-second.pdf");
    let third_output = scratch
        .path()
        .join("bp-annotation-all-v1-without-image.pdf");
    public_annotation_fixture(&source);
    let source_unknown = unknown_annotation_oracle(&source);
    let source_document = original_document_oracle(&source);

    let mut session = PdfPersistenceSession::open(&source).unwrap();
    session
        .add_pen(
            PenAnnotation::new_highlight_paths(
                MarkupId::new("highlight-1").unwrap(),
                0,
                vec![
                    vec![
                        point(90.0, 312.0),
                        point(150.0, 319.0),
                        point(220.0, 311.0),
                        point(330.0, 316.0),
                    ],
                    vec![point(90.0, 336.0), point(330.0, 336.0)],
                ],
                PenAppearance::new("#facc15", 16.0, 0.35).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_text_box(
            TextBoxAnnotation::new(
                MarkupId::new("text-1").unwrap(),
                0,
                PdfRect::new(72.0, 360.0, 252.0, 72.0).unwrap(),
                "Beam B-12 / revision 4",
                TextBoxStyle::new("Helvetica", 14.0, "#111827", 1.0).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .add_length(
            LengthAnnotation::new(
                MarkupId::new("length-1").unwrap(),
                0,
                point(90.0, 498.0),
                point(306.0, 498.0),
                LengthCalibration::from_scale(72.0, 1.0, "m", 2, false)
                    .unwrap()
                    .with_label("Span")
                    .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let asset = DecodedRgbaAsset::new(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ],
    )
    .unwrap();
    session
        .add_image(
            ImageAnnotation::new(
                MarkupId::new("image-1").unwrap(),
                0,
                PdfRect::new(360.0, 360.0, 144.0, 144.0).unwrap(),
                asset.clone(),
                true,
            )
            .unwrap(),
        )
        .unwrap();
    session.save_as(&first_output).unwrap();
    validate_independently(&first_output);
    assert_native_representative_contracts(&first_output, false);
    let first_document = Document::load(&first_output).unwrap();
    let original_highlight_appearance = normal_appearance(
        &first_document,
        annotation_dictionary(&first_document, "bp:highlight-1"),
    )
    .content
    .clone();
    let original_image_graph = image_appearance_graph_ids(
        &first_document,
        annotation_dictionary(&first_document, "bp:image-1"),
    );
    assert_eq!(unknown_annotation_oracle(&first_output), source_unknown);
    assert_eq!(original_document_oracle(&first_output), source_document);

    let mut first_reopen = PdfPersistenceSession::open(&first_output).unwrap();
    assert_eq!(first_reopen.pens().len(), 1);
    assert_eq!(
        first_reopen.text_boxes()[0].content(),
        "Beam B-12 / revision 4"
    );
    assert_eq!(first_reopen.lengths()[0].caption(), "Span: 3.00 m");
    assert_eq!(first_reopen.images()[0].asset(), &asset);

    let mut edited_rectangle = first_reopen.rectangles()[0].clone();
    edited_rectangle.appearance = RectangleAppearance::new("#dc2626", 3.0, Some("#dc2626"), 0.88)
        .unwrap()
        .with_fill_opacity(31.0 / 255.0)
        .unwrap()
        .with_stroke_style(StrokeStyle::Dashed);
    first_reopen.replace_rectangle(edited_rectangle).unwrap();

    let mut edited_pen = first_reopen.pens()[0].clone();
    edited_pen = PenAnnotation::new_highlight_paths(
        edited_pen.id,
        edited_pen.page_index,
        vec![
            vec![
                point(102.0, 306.0),
                point(162.0, 313.0),
                point(232.0, 305.0),
                point(312.0, 310.0),
                point(342.0, 312.0),
            ],
            vec![point(102.0, 336.0), point(342.0, 336.0)],
        ],
        PenAppearance::new("#facc15", 16.0, 0.45).unwrap(),
    )
    .unwrap();
    first_reopen.replace_pen(edited_pen).unwrap();
    let mut edited_text = first_reopen.text_boxes()[0].clone();
    edited_text = TextBoxAnnotation::new(
        edited_text.id.clone(),
        edited_text.page_index,
        PdfRect::new(72.0, 360.0, 300.0, 84.0).unwrap(),
        "Beam B-12 / revision 5",
        edited_text.style().clone(),
    )
    .unwrap();
    first_reopen.replace_text_box(edited_text).unwrap();
    let edited_length = LengthAnnotation::new(
        first_reopen.lengths()[0].id.clone(),
        0,
        point(90.0, 498.0),
        point(342.0, 498.0),
        first_reopen.lengths()[0].calibration().clone(),
    )
    .unwrap();
    first_reopen.replace_length(edited_length).unwrap();
    let edited_image = ImageAnnotation::new(
        first_reopen.images()[0].id.clone(),
        0,
        PdfRect::new(360.0, 360.0, 180.0, 180.0).unwrap(),
        first_reopen.images()[0].asset().clone(),
        true,
    )
    .unwrap();
    first_reopen.replace_image(edited_image).unwrap();
    first_reopen.save_as(&second_output).unwrap();
    validate_independently(&second_output);
    assert_native_representative_contracts(&second_output, true);
    let second_document = Document::load(&second_output).unwrap();
    assert!(
        second_document.objects.values().all(|object| {
            object.as_stream().map_or(true, |stream| {
                stream.content != original_highlight_appearance
            })
        }),
        "replacing a Highlight must remove its unreferenced prior appearance geometry",
    );
    assert!(
        original_image_graph
            .iter()
            .all(|object_id| !second_document.objects.contains_key(object_id)),
        "replacing an Image must remove its unreferenced Form, Image, and SMask objects",
    );
    assert_eq!(unknown_annotation_oracle(&second_output), source_unknown);
    assert_eq!(original_document_oracle(&second_output), source_document);

    let mut second_reopen = PdfPersistenceSession::open(&second_output).unwrap();
    assert_eq!(second_reopen.rectangles()[0].appearance.opacity(), 0.88);
    assert!(
        (second_reopen.rectangles()[0].appearance.fill_opacity() - 31.0 / 255.0).abs() < 0.000_001
    );
    assert_eq!(
        second_reopen.rectangles()[0].appearance.stroke_style(),
        StrokeStyle::Dashed
    );
    assert_eq!(second_reopen.pens()[0].points().len(), 5);
    assert_eq!(second_reopen.pens()[0].appearance.opacity(), 0.45);
    assert_eq!(
        second_reopen.text_boxes()[0].content(),
        "Beam B-12 / revision 5"
    );
    assert_eq!(second_reopen.text_boxes()[0].layout_rect.width, 300.0);
    assert_eq!(second_reopen.lengths()[0].caption(), "Span: 3.50 m");
    assert_eq!(second_reopen.images()[0].rect.width, 180.0);
    assert_eq!(second_reopen.images()[0].asset(), &asset);
    let second_image_graph = image_appearance_graph_ids(
        &second_document,
        annotation_dictionary(&second_document, "bp:image-1"),
    );
    second_reopen
        .remove_image(&MarkupId::new("image-1").unwrap())
        .unwrap();
    second_reopen.save_as(&third_output).unwrap();
    validate_independently(&third_output);
    let third_document = Document::load(&third_output).unwrap();
    assert!(
        second_image_graph
            .iter()
            .all(|object_id| !third_document.objects.contains_key(object_id)),
        "deleting an Image must remove its unreferenced Form, Image, and SMask objects",
    );
    assert!(
        PdfPersistenceSession::open(&third_output)
            .unwrap()
            .images()
            .is_empty()
    );
}
