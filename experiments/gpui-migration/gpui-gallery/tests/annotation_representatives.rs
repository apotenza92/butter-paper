use butter_paper_gpui_gallery::annotation_model::{
    Annotation, AnnotationCommand, AnnotationDocument, AnnotationEdit, AnnotationKind, BlendMode,
    CommandOutcome, DecodedRgbaAsset, ImageAnnotation, InkTool, LengthAnnotation,
    LengthCalibration, LengthEndpoint, MAX_COALESCED_PEN_SAMPLES, MarkupId, PdfPoint, PdfRect,
    PenAnnotation, PenAppearance, TextAlignment, TextBoxAnnotation, TextBoxStyle,
};

fn id(value: &str) -> MarkupId {
    MarkupId::new(value).unwrap()
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> PdfRect {
    PdfRect::new(x, y, width, height).unwrap()
}

fn point(x: f64, y: f64) -> PdfPoint {
    PdfPoint::new(x, y).unwrap()
}

#[test]
fn pen_path_uses_normalized_commands_and_shared_document_state() {
    let mut document = AnnotationDocument::default();
    let pen = PenAnnotation::new(
        id("pen-1"),
        0,
        vec![point(10.0, 20.0), point(12.0, 23.0), point(18.0, 26.0)],
        PenAppearance::new("#2563eb", 2.5, 0.8).unwrap(),
    )
    .unwrap();

    assert_eq!(
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Pen(pen)))
            .unwrap(),
        CommandOutcome::AnnotationCreated {
            id: id("pen-1"),
            kind: AnnotationKind::Pen,
            revision: 1,
        }
    );
    assert_eq!(document.selected_id(), Some(&id("pen-1")));
    assert!(document.snapshot().dirty);

    let replacement = vec![
        point(10.0, 20.0),
        point(14.0, 24.0),
        point(20.0, 28.0),
        point(27.0, 30.0),
    ];
    assert_eq!(
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: id("pen-1"),
                edit: AnnotationEdit::ReplacePenPath(replacement.clone()),
            })
            .unwrap(),
        CommandOutcome::AnnotationEdited {
            id: id("pen-1"),
            kind: AnnotationKind::Pen,
            changed: true,
            revision: 2,
        }
    );
    assert_eq!(document.pens()[0].points(), replacement);
    assert_eq!(document.document_scene(0).pens[0].points, replacement);
    assert_eq!(document.thumbnail_scene(0).pens[0].points, replacement);
    assert!(document.undo().unwrap());
    assert_eq!(document.pens()[0].points().len(), 3);
    assert!(document.redo().unwrap());
    assert_eq!(document.pens()[0].points().len(), 4);
}

#[test]
fn text_box_content_edit_obeys_shared_lock_history_and_scene_rules() {
    let mut document = AnnotationDocument::default();
    let text_box = TextBoxAnnotation::new(
        id("text-1"),
        0,
        rect(72.0, 540.0, 216.0, 72.0),
        "Existing field note",
        TextBoxStyle::new("Helvetica", 12.0, "#111827", 1.0).unwrap(),
    )
    .unwrap();
    document
        .apply_command(AnnotationCommand::CreateAnnotation(Annotation::TextBox(
            text_box,
        )))
        .unwrap();
    document
        .apply_command(AnnotationCommand::SetLocked {
            id: id("text-1"),
            locked: true,
        })
        .unwrap();
    assert!(
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: id("text-1"),
                edit: AnnotationEdit::SetTextBoxContent("Revised field note".into()),
            })
            .is_err()
    );
    document
        .apply_command(AnnotationCommand::SetLocked {
            id: id("text-1"),
            locked: false,
        })
        .unwrap();
    assert_eq!(
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: id("text-1"),
                edit: AnnotationEdit::SetTextBoxContent("Revised field note".into()),
            })
            .unwrap(),
        CommandOutcome::AnnotationEdited {
            id: id("text-1"),
            kind: AnnotationKind::TextBox,
            changed: true,
            revision: 4,
        }
    );
    assert_eq!(document.text_boxes()[0].content(), "Revised field note");
    assert_eq!(document.text_boxes()[0].style().font_family(), "Helvetica");
    let editor = document.document_scene(0);
    assert_eq!(
        editor.text_boxes[0].layout_rect,
        rect(72.0, 540.0, 216.0, 72.0)
    );
    assert!(editor.text_boxes[0].selected);
    let thumbnail = document.thumbnail_scene(0);
    assert_eq!(thumbnail.text_boxes[0].content, "Revised field note");
    assert!(!thumbnail.text_boxes[0].selected);
    assert!(document.undo().unwrap());
    assert_eq!(document.text_boxes()[0].content(), "Existing field note");
}

#[test]
fn length_calibration_edit_updates_scale_derived_caption_and_shared_scenes() {
    let mut document = AnnotationDocument::default();
    let length = LengthAnnotation::new(
        id("length-1"),
        1,
        point(20.0, 30.0),
        point(220.0, 30.0),
        LengthCalibration::new(0.005, "m", "Span", true).unwrap(),
    )
    .unwrap();
    document
        .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Length(
            length,
        )))
        .unwrap();
    assert_eq!(document.lengths()[0].measured_value(), 1.0);
    assert_eq!(document.lengths()[0].caption(), "Span: 1 m");

    assert_eq!(
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: id("length-1"),
                edit: AnnotationEdit::SetLengthCalibration(
                    LengthCalibration::new(0.01, "m", "Span", true).unwrap(),
                ),
            })
            .unwrap(),
        CommandOutcome::AnnotationEdited {
            id: id("length-1"),
            kind: AnnotationKind::Length,
            changed: true,
            revision: 2,
        }
    );
    assert_eq!(document.lengths()[0].measured_value(), 2.0);
    assert_eq!(document.lengths()[0].caption(), "Span: 2 m");
    assert!(document.lengths()[0].calibration().show_caption());
    assert_eq!(document.document_scene(1).lengths[0].caption, "Span: 2 m");
    assert_eq!(document.thumbnail_scene(1).lengths[0].caption, "Span: 2 m");
    assert!(document.document_scene(0).lengths.is_empty());
    assert!(document.undo().unwrap());
    assert_eq!(document.lengths()[0].caption(), "Span: 1 m");
    assert!(document.redo().unwrap());
    assert_eq!(document.lengths()[0].caption(), "Span: 2 m");
}

#[test]
fn image_uses_bounded_decoded_asset_identity_aspect_lock_and_shared_delete_history() {
    let mut document = AnnotationDocument::default();
    let rgba = vec![
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let asset = DecodedRgbaAsset::new(2, 2, rgba).unwrap();
    assert_eq!(asset.width_px(), 2);
    assert_eq!(asset.height_px(), 2);
    assert_eq!(asset.rgba().len(), 16);
    assert_eq!(asset.id().as_str().len(), 64);
    let image = ImageAnnotation::new(
        id("image-1"),
        0,
        rect(36.0, 400.0, 144.0, 144.0),
        asset.clone(),
        true,
    )
    .unwrap();
    document
        .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Image(
            image,
        )))
        .unwrap();
    assert_eq!(
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: id("image-1"),
                edit: AnnotationEdit::SetImageRect(rect(36.0, 400.0, 216.0, 216.0)),
            })
            .unwrap(),
        CommandOutcome::AnnotationEdited {
            id: id("image-1"),
            kind: AnnotationKind::Image,
            changed: true,
            revision: 2,
        }
    );
    let scene = document.document_scene(0);
    assert_eq!(scene.images[0].asset_id, asset.id().clone());
    assert_eq!(
        (scene.images[0].width_px, scene.images[0].height_px),
        (2, 2)
    );
    assert!(scene.images[0].aspect_locked);
    assert_eq!(document.thumbnail_scene(0).images[0].rect.width, 216.0);

    document
        .apply_command(AnnotationCommand::SetLocked {
            id: id("image-1"),
            locked: true,
        })
        .unwrap();
    assert!(
        document
            .apply_command(AnnotationCommand::DeleteSelected)
            .is_err()
    );
    document
        .apply_command(AnnotationCommand::SetLocked {
            id: id("image-1"),
            locked: false,
        })
        .unwrap();
    document
        .apply_command(AnnotationCommand::DeleteSelected)
        .unwrap();
    assert!(document.images().is_empty());
    assert!(document.undo().unwrap());
    assert_eq!(document.images()[0].asset(), &asset);
    assert!(document.redo().unwrap());
    assert!(document.images().is_empty());
}

#[test]
fn decoded_rgba_asset_rejects_length_mismatch_and_oversize_dimensions() {
    assert!(DecodedRgbaAsset::new(2, 2, vec![0; 15]).is_err());
    assert!(DecodedRgbaAsset::new(u32::MAX, u32::MAX, vec![]).is_err());
}

#[test]
fn pen_stream_coalesces_bounded_samples_previews_draft_and_commits_once() {
    let mut document = AnnotationDocument::default();
    let appearance = PenAppearance::new("#2563eb", 2.5, 0.8).unwrap();
    assert_eq!(
        document
            .apply_command(AnnotationCommand::BeginPen {
                pointer_id: 41,
                id: id("streamed-pen"),
                page_index: 0,
                start: point(10.0, 20.0),
                appearance: appearance.clone(),
                smooth_curves: true,
            })
            .unwrap(),
        CommandOutcome::PenStarted {
            id: id("streamed-pen")
        }
    );
    assert_eq!(
        document
            .apply_command(AnnotationCommand::AppendPenSamples {
                pointer_id: 41,
                samples: vec![
                    point(10.1, 20.1),
                    point(12.0, 22.0),
                    point(12.2, 22.2),
                    point(16.0, 25.0),
                ],
                min_distance_pt: 1.0,
            })
            .unwrap(),
        CommandOutcome::PenSamplesAppended {
            id: id("streamed-pen"),
            accepted: 2,
            total: 3,
        }
    );
    assert_eq!(document.snapshot().revision, 0);
    assert!(document.pens().is_empty());
    assert_eq!(document.document_scene(0).pens[0].points.len(), 3);
    assert!(document.document_scene(0).pens[0].draft);
    assert!(document.thumbnail_scene(0).pens.is_empty());

    let before_rejected_batch = document.document_scene(0);
    assert!(
        document
            .apply_command(AnnotationCommand::AppendPenSamples {
                pointer_id: 41,
                samples: vec![point(20.0, 20.0); MAX_COALESCED_PEN_SAMPLES + 1],
                min_distance_pt: 1.0,
            })
            .is_err()
    );
    assert_eq!(document.document_scene(0), before_rejected_batch);

    assert_eq!(
        document
            .apply_command(AnnotationCommand::CommitPen { pointer_id: 41 })
            .unwrap(),
        CommandOutcome::AnnotationCreated {
            id: id("streamed-pen"),
            kind: AnnotationKind::Pen,
            revision: 1,
        }
    );
    assert_eq!(document.history_depths(), (1, 0));
    assert_eq!(document.pens()[0].points().len(), 3);
    assert!(document.pens()[0].smooth_curves);
    assert!(!document.document_scene(0).pens[0].draft);

    document
        .apply_command(AnnotationCommand::BeginPen {
            pointer_id: 42,
            id: id("cancelled-pen"),
            page_index: 0,
            start: point(30.0, 30.0),
            appearance,
            smooth_curves: false,
        })
        .unwrap();
    document
        .apply_command(AnnotationCommand::PointerCancel {
            pointer_id: 42,
            reason: butter_paper_gpui_gallery::annotation_model::PointerCancelReason::CaptureLost,
        })
        .unwrap();
    assert_eq!(document.pens().len(), 1);
    assert_eq!(document.history_depths(), (1, 0));
}

#[test]
fn frozen_highlight_uses_multiply_blend_and_remains_distinct_from_pen() {
    let highlight = PenAnnotation::new_highlight(
        id("comparison:highlight:1"),
        0,
        vec![
            point(90.0, 330.0),
            point(150.0, 337.0),
            point(220.0, 329.0),
            point(300.0, 334.0),
        ],
        PenAppearance::new("#facc15", 16.0, 0.35).unwrap(),
    )
    .unwrap();
    assert_eq!(highlight.tool(), InkTool::Highlight);
    assert_eq!(highlight.blend_mode(), BlendMode::Multiply);

    let mut document = AnnotationDocument::default();
    document
        .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Pen(
            highlight,
        )))
        .unwrap();
    document
        .apply_command(AnnotationCommand::EditAnnotation {
            id: id("comparison:highlight:1"),
            edit: AnnotationEdit::SetInkAppearance(
                PenAppearance::new("#facc15", 16.0, 0.45).unwrap(),
            ),
        })
        .unwrap();
    let scene = document.document_scene(0);
    assert_eq!(scene.pens[0].tool, InkTool::Highlight);
    assert_eq!(scene.pens[0].blend_mode, BlendMode::Multiply);
    assert_eq!(scene.pens[0].appearance.opacity(), 0.45);
    assert!(
        document
            .canonical_json_string()
            .contains(r#""tool":"highlight""#)
    );
    assert!(
        document
            .canonical_json_string()
            .contains(r#""blend":"multiply""#)
    );
}

#[test]
fn frozen_free_text_defaults_and_layout_resize_round_trip_through_history() {
    let style = TextBoxStyle::new("Helvetica", 14.0, "#111827", 1.0).unwrap();
    assert_eq!(style.weight(), 400);
    assert_eq!(style.alignment(), TextAlignment::Left);
    let mut document = AnnotationDocument::default();
    document
        .apply_command(AnnotationCommand::CreateAnnotation(Annotation::TextBox(
            TextBoxAnnotation::new(
                id("comparison:text:1"),
                0,
                rect(90.0, 390.0, 240.0, 72.0),
                "Beam B-12 / revision 4",
                style,
            )
            .unwrap(),
        )))
        .unwrap();
    document
        .apply_command(AnnotationCommand::EditAnnotation {
            id: id("comparison:text:1"),
            edit: AnnotationEdit::SetTextBoxLayoutRect(rect(90.0, 390.0, 300.0, 84.0)),
        })
        .unwrap();
    assert_eq!(
        document.text_boxes()[0].layout_rect,
        rect(90.0, 390.0, 300.0, 84.0)
    );
    assert!(document.undo().unwrap());
    assert_eq!(document.text_boxes()[0].layout_rect.width, 240.0);
    assert!(document.redo().unwrap());
    assert_eq!(
        document.thumbnail_scene(0).text_boxes[0].layout_rect.height,
        84.0
    );
}

#[test]
fn frozen_length_scale_formats_precision_and_endpoint_edit_exactly() {
    let calibration = LengthCalibration::from_scale(72.0, 1.0, "m", 2, true).unwrap();
    let mut document = AnnotationDocument::default();
    document
        .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Length(
            LengthAnnotation::new(
                id("comparison:length:1"),
                0,
                point(90.0, 510.0),
                point(306.0, 510.0),
                calibration,
            )
            .unwrap(),
        )))
        .unwrap();
    assert_eq!(document.lengths()[0].caption(), "3.00 m");
    assert_eq!(document.lengths()[0].calibration().paper_points(), 72.0);
    assert_eq!(document.lengths()[0].calibration().real_world_value(), 1.0);
    assert_eq!(document.lengths()[0].calibration().precision(), 2);

    document
        .apply_command(AnnotationCommand::EditAnnotation {
            id: id("comparison:length:1"),
            edit: AnnotationEdit::SetLengthEndpoint {
                endpoint: LengthEndpoint::End,
                point: point(342.0, 510.0),
            },
        })
        .unwrap();
    assert_eq!(document.lengths()[0].caption(), "3.50 m");
    assert!(document.undo().unwrap());
    assert_eq!(document.lengths()[0].caption(), "3.00 m");
    assert!(document.redo().unwrap());
    assert_eq!(document.thumbnail_scene(0).lengths[0].caption, "3.50 m");
}

#[test]
fn frozen_highlight_can_be_created_through_the_streaming_command_path() {
    let mut document = AnnotationDocument::default();
    document
        .apply_command(AnnotationCommand::BeginHighlight {
            pointer_id: 77,
            id: id("streamed-highlight"),
            page_index: 0,
            start: point(90.0, 330.0),
            appearance: PenAppearance::new("#facc15", 16.0, 0.35).unwrap(),
            smooth_curves: true,
        })
        .unwrap();
    document
        .apply_command(AnnotationCommand::AppendPenSamples {
            pointer_id: 77,
            samples: vec![
                point(150.0, 337.0),
                point(220.0, 329.0),
                point(300.0, 334.0),
            ],
            min_distance_pt: 0.5,
        })
        .unwrap();
    let draft = &document.document_scene(0).pens[0];
    assert!(draft.draft);
    assert_eq!(draft.tool, InkTool::Highlight);
    assert_eq!(draft.blend_mode, BlendMode::Multiply);
    document
        .apply_command(AnnotationCommand::CommitPen { pointer_id: 77 })
        .unwrap();
    assert_eq!(document.pens()[0].tool(), InkTool::Highlight);
    assert_eq!(document.history_depths(), (1, 0));
}
