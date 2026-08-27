use butter_paper_gpui_gallery::annotation_model::{
    Annotation, AnnotationCommand, AnnotationDocument, AnnotationEdit, ArcAnnotation,
    ArcControlPoint, MarkupId, PdfPoint, PdfRect, RectangleAppearance,
};

fn point(x: f64, y: f64) -> PdfPoint {
    PdfPoint::new(x, y).unwrap()
}

fn assert_point_close(actual: PdfPoint, expected: PdfPoint) {
    assert!((actual.x - expected.x).abs() < 0.000_001);
    assert!((actual.y - expected.y).abs() < 0.000_001);
}

#[test]
fn arc_model_keeps_three_point_geometry_stable_through_edit_history_and_copy() {
    let id = MarkupId::new("arc:model:three-point").unwrap();
    let arc = ArcAnnotation::new(
        id.clone(),
        0,
        point(0., 0.),
        point(100., 0.),
        point(50., 50.),
        RectangleAppearance::new("#ff0000", 1., None::<String>, 1.).unwrap(),
    )
    .unwrap();

    assert_eq!(arc.rect(), PdfRect::new(0., -50., 100., 100.).unwrap());
    assert_eq!(arc.angle1_degrees(), 180.);
    assert_eq!(arc.angle2_degrees(), 0.);
    assert_eq!(arc.sampled_path(64).len(), 65);
    assert_eq!(arc.sampled_path(64).first(), Some(&point(0., 0.)));
    assert_eq!(arc.sampled_path(64).last(), Some(&point(100., 0.)));

    let mut document = AnnotationDocument::default();
    document
        .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Arc(
            arc.clone(),
        )))
        .unwrap();
    document.select(&id);
    document
        .apply_command(AnnotationCommand::EditAnnotation {
            id: id.clone(),
            edit: AnnotationEdit::SetArcControlPoint {
                control: ArcControlPoint::Mid,
                point: point(50., 20.710_678),
                snap_quarter_turn: true,
            },
        })
        .unwrap();

    let edited = document.snapshot();
    assert_eq!(edited.arcs.len(), 1);
    assert_eq!(edited.arcs[0].id, id);
    assert_eq!(edited.arcs[0].mid, point(50., 20.710_678));
    assert_eq!(edited.arcs[0].sweep_degrees().abs(), 90.);
    assert_eq!((edited.undo_depth, edited.redo_depth), (2, 0));

    document.apply_command(AnnotationCommand::Undo).unwrap();
    assert!(document.snapshot().arcs[0].same_persisted_state_as(&arc));
    document.apply_command(AnnotationCommand::Redo).unwrap();
    assert_eq!(document.snapshot().arcs[0].sweep_degrees().abs(), 90.);

    let copy_id = MarkupId::new("arc:model:copy").unwrap();
    let copy = Annotation::Arc(document.snapshot().arcs[0].clone())
        .translated_copy(copy_id.clone(), 0, 12., -8.)
        .unwrap();
    let Annotation::Arc(copy) = copy else {
        panic!("an Arc translated copy must remain an Arc");
    };
    assert_eq!(copy.id, copy_id);
    assert_eq!(copy.start, point(12., -8.));
    assert_eq!(copy.end, point(112., -8.));
    assert_point_close(copy.mid, point(62., 12.710_678));
}
