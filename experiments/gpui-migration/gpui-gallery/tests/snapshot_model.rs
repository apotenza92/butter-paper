use butter_paper_gpui_gallery::annotation_model::{
    Annotation, AnnotationCommand, AnnotationDocument, AnnotationEdit, AnnotationKind,
    CommandOutcome, DecodedRgbaAsset, MarkupId, PdfRect, SnapshotAnnotation,
};

fn id(value: &str) -> MarkupId {
    MarkupId::new(value).unwrap()
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> PdfRect {
    PdfRect::new(x, y, width, height).unwrap()
}

fn asset() -> DecodedRgbaAsset {
    DecodedRgbaAsset::new(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ],
    )
    .unwrap()
}

#[test]
fn snapshot_model_retains_distinct_pixels_geometry_appearance_and_history() {
    let asset = asset();
    let snapshot = SnapshotAnnotation::new(
        id("snapshot:model:1"),
        2,
        rect(72., 144., 180., 96.),
        asset.clone(),
        0.8,
    )
    .unwrap()
    .with_rotation_degrees(15.)
    .unwrap();
    assert_eq!(
        Annotation::Snapshot(snapshot.clone()).kind(),
        AnnotationKind::Snapshot
    );
    assert_eq!(snapshot.asset(), &asset);
    assert_eq!(snapshot.opacity(), 0.8);
    assert_eq!(snapshot.rotation_degrees(), 15.);
    assert!(!snapshot.locked);

    let mut document = AnnotationDocument::default();
    document
        .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Snapshot(
            snapshot,
        )))
        .unwrap();
    document.select(&id("snapshot:model:1"));
    assert_eq!(
        document.snapshot().annotation_order,
        vec![id("snapshot:model:1")]
    );

    assert_eq!(
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: id("snapshot:model:1"),
                edit: AnnotationEdit::SetSnapshotRect(rect(90., 132., 216., 120.)),
            })
            .unwrap(),
        CommandOutcome::AnnotationEdited {
            id: id("snapshot:model:1"),
            kind: AnnotationKind::Snapshot,
            changed: true,
            revision: 2,
        }
    );
    document
        .apply_command(AnnotationCommand::EditAnnotation {
            id: id("snapshot:model:1"),
            edit: AnnotationEdit::SetSnapshotRotation(92.5),
        })
        .unwrap();
    document
        .apply_command(AnnotationCommand::EditAnnotation {
            id: id("snapshot:model:1"),
            edit: AnnotationEdit::SetSnapshotOpacity(0.35),
        })
        .unwrap();

    let retained = document.snapshot();
    assert_eq!(retained.snapshots.len(), 1);
    assert_eq!(retained.snapshots[0].rect, rect(90., 132., 216., 120.));
    assert_eq!(retained.snapshots[0].rotation_degrees(), 92.5);
    assert_eq!(retained.snapshots[0].opacity(), 0.35);
    assert_eq!(retained.annotation_order, vec![id("snapshot:model:1")]);
    let scene = document.document_scene(2);
    assert_eq!(scene.snapshots.len(), 1);
    assert_eq!(scene.snapshots[0].body_id, "snapshot.body");
    assert_eq!(scene.snapshots[0].asset_id, asset.id().clone());
    assert_eq!(scene.snapshots[0].rect, rect(90., 132., 216., 120.));
    assert_eq!(scene.snapshots[0].rotation_degrees, 92.5);
    assert_eq!(scene.snapshots[0].opacity, 0.35);
    assert!(scene.snapshots[0].selected);
    assert!(!scene.snapshots[0].draft);
    let thumbnail = document.thumbnail_scene(2);
    assert_eq!(thumbnail.snapshots.len(), 1);
    assert!(!thumbnail.snapshots[0].selected);
    assert!(!thumbnail.snapshots[0].draft);

    let copied = Annotation::Snapshot(retained.snapshots[0].clone())
        .translated_copy(id("snapshot:model:copy"), 3, 12., -6.)
        .unwrap();
    let Annotation::Snapshot(copied) = copied else {
        panic!("a Snapshot copy must retain its distinct annotation kind");
    };
    assert_eq!(copied.page_index, 3);
    assert_eq!(copied.rect, rect(102., 126., 216., 120.));
    assert_eq!(copied.asset(), &asset);
    assert_eq!(copied.opacity(), 0.35);
    assert_eq!(copied.rotation_degrees(), 92.5);

    document.apply_command(AnnotationCommand::Undo).unwrap();
    assert_eq!(document.snapshots()[0].opacity(), 0.8);
    document.apply_command(AnnotationCommand::Redo).unwrap();
    assert_eq!(document.snapshots()[0].opacity(), 0.35);
    document
        .apply_command(AnnotationCommand::SetLocked {
            id: id("snapshot:model:1"),
            locked: true,
        })
        .unwrap();
    assert!(document.snapshots()[0].locked);
    assert!(
        document
            .apply_command(AnnotationCommand::DeleteSelected)
            .is_err()
    );
    document
        .apply_command(AnnotationCommand::SetLocked {
            id: id("snapshot:model:1"),
            locked: false,
        })
        .unwrap();
    document
        .apply_command(AnnotationCommand::DeleteSelected)
        .unwrap();
    assert!(document.snapshots().is_empty());
    document.apply_command(AnnotationCommand::Undo).unwrap();
    assert_eq!(document.snapshots()[0].asset(), &asset);
}

#[test]
fn snapshot_model_rejects_invalid_opacity_rotation_and_layout_geometry() {
    let asset = asset();
    assert!(
        SnapshotAnnotation::new(
            id("snapshot:model:bad-opacity"),
            0,
            rect(0., 0., 10., 10.),
            asset.clone(),
            1.01,
        )
        .is_err()
    );
    assert!(
        SnapshotAnnotation::new(
            id("snapshot:model:bad-rotation"),
            0,
            rect(0., 0., 10., 10.),
            asset.clone(),
            1.,
        )
        .unwrap()
        .with_rotation_degrees(f64::NAN)
        .is_err()
    );
    assert!(
        SnapshotAnnotation::new(
            id("snapshot:model:too-small"),
            0,
            rect(0., 0., 1.99, 10.),
            asset,
            1.,
        )
        .is_err()
    );
}

#[test]
fn snapshot_model_inserts_and_moves_by_stable_identity_in_document_order() {
    let mut document = AnnotationDocument::default();
    let snapshot_id = id("snapshot:model:ordered");
    let snapshot = SnapshotAnnotation::new(
        snapshot_id.clone(),
        4,
        rect(20., 30., 80., 40.),
        asset(),
        1.,
    )
    .unwrap();

    assert_eq!(
        document
            .insert_annotations(vec![Annotation::Snapshot(snapshot)])
            .unwrap(),
        vec![snapshot_id.clone()]
    );
    assert!(document.select(&snapshot_id));
    assert!(document.translate_selection_on_page(4, 12., -8.).unwrap());

    assert_eq!(
        document.snapshot().annotation_order,
        vec![snapshot_id.clone()]
    );
    assert_eq!(document.snapshots()[0].id, snapshot_id);
    assert_eq!(document.snapshots()[0].rect, rect(32., 22., 80., 40.));
    assert!(matches!(
        document.selected_annotations_in_document_order().as_slice(),
        [Annotation::Snapshot(_)]
    ));
}
