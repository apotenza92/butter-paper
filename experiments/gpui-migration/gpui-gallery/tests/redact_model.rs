use butter_paper_gpui_gallery::annotation_model::{
    Annotation, AnnotationCommand, AnnotationDocument, AnnotationEdit, AnnotationKind, MarkupId,
    PdfRect, RectangleAppearance, RedactAnnotation,
};

const PENDING_REDACTION_STATUS: &str = "Pending redaction mark — saving keeps the underlying PDF content; this mark does not securely remove text or graphics.";

fn pending_redact(id: &str, rect: PdfRect) -> RedactAnnotation {
    RedactAnnotation::new(
        MarkupId::new(id).unwrap(),
        0,
        rect,
        "#000000",
        None::<String>,
        RectangleAppearance::new("#ff0000", 1., Some("#000000"), 0.35)
            .unwrap()
            .with_fill_opacity(0.35)
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn redact_model_retains_a_distinct_pending_mark_with_strict_dimensions_and_no_apply_state() {
    let id = MarkupId::new("redact:model:pending-1").unwrap();
    let redact = pending_redact(
        id.as_str(),
        PdfRect::new(216., 144., 72.000_1, 2.000_1).unwrap(),
    );

    assert_eq!(
        Annotation::Redact(redact.clone()).kind(),
        AnnotationKind::Redact
    );
    assert_eq!(redact.redaction_color(), "#000000");
    assert_eq!(redact.overlay_text(), None);
    assert_eq!(redact.pending_status_text(), PENDING_REDACTION_STATUS);
    assert_eq!(redact.appearance.stroke_color(), "#ff0000");
    assert_eq!(redact.appearance.stroke_width_pt(), 1.);
    assert_eq!(redact.appearance.fill_color(), Some("#000000"));
    assert_eq!(redact.appearance.opacity(), 0.35);
    assert_eq!(redact.appearance.fill_opacity(), 0.35);
    assert!(
        RedactAnnotation::new(
            MarkupId::new("redact:model:too-small").unwrap(),
            0,
            PdfRect::new(0., 0., 2., 3.).unwrap(),
            "#000000",
            None::<String>,
            redact.appearance.clone(),
        )
        .is_err(),
        "both normalized PDF dimensions must be strictly greater than two points",
    );

    let mut document = AnnotationDocument::default();
    document
        .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Redact(
            redact.clone(),
        )))
        .unwrap();
    document.select(&id);
    document
        .apply_command(AnnotationCommand::EditAnnotation {
            id: id.clone(),
            edit: AnnotationEdit::TranslateRedact {
                delta_x: 12.,
                delta_y: -6.,
            },
        })
        .unwrap();
    document
        .apply_command(AnnotationCommand::EditAnnotation {
            id: id.clone(),
            edit: AnnotationEdit::SetRedactRect(PdfRect::new(228., 138., 96., 36.).unwrap()),
        })
        .unwrap();
    let edited = document.snapshot();
    assert_eq!(edited.redacts.len(), 1);
    assert_eq!(edited.redacts[0].id, id);
    assert_eq!(
        edited.redacts[0].rect,
        PdfRect::new(228., 138., 96., 36.).unwrap()
    );
    assert_eq!(
        (edited.revision, edited.undo_depth, edited.redo_depth),
        (3, 3, 0)
    );

    document.apply_command(AnnotationCommand::Undo).unwrap();
    assert_eq!(
        document.snapshot().redacts[0].rect,
        PdfRect::new(228., 138., 72.000_1, 2.000_1).unwrap(),
    );
    document.apply_command(AnnotationCommand::Redo).unwrap();
    assert_eq!(
        document.snapshot().redacts[0].rect,
        PdfRect::new(228., 138., 96., 36.).unwrap(),
    );

    let debug = format!("{edited:?}");
    for forbidden in ["Apply Redactions", "Applied", "Secure", "Removed"] {
        assert!(
            !debug.contains(forbidden),
            "a pending mark must not expose a destructive or completed redaction state: {forbidden}",
        );
    }
}

#[test]
fn pdf_rectangle_equivalence_uses_the_real_edge_quantization_of_persisted_numbers() {
    let retained = PdfRect::new(179.999_982, 239.999_972, 216.000_057, 96.000_061).unwrap();
    let reopened = PdfRect::new(179.999_985, 239.999_969, 216.000_046, 96.000_061).unwrap();
    assert!(
        retained.same_pdf_geometry_as(reopened),
        "the two rectangles have identical f32 left, bottom, right, and top edges in the PDF writer",
    );
    assert!(
        !retained.same_pdf_geometry_as(
            PdfRect::new(179.999_985, 239.999_969, 216.001_046, 96.000_061).unwrap(),
        ),
        "a representable one-thousandth-point width change must remain distinct",
    );
}
