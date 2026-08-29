use butter_paper_gpui_gallery::annotation_model::PageTransform;
use butter_paper_gpui_gallery::page_geometry::{PageCoordinateSpace, PdfPoint, PdfRect, Rotation};
use lopdf::{Document, Object, dictionary};

fn crop_space(rotation: Rotation) -> PageCoordinateSpace {
    PageCoordinateSpace::new(
        PdfRect::new(0.0, 0.0, 612.0, 792.0).unwrap(),
        PdfRect::new(36.0, 72.0, 540.0, 720.0).unwrap(),
        rotation,
        1.0,
    )
    .unwrap()
}

#[test]
fn crop_box_origin_and_all_rotations_match_electron_oracle() {
    let cases = [
        (Rotation::Degrees0, PdfPoint::new(36.0, 684.0)),
        (Rotation::Degrees90, PdfPoint::new(36.0, 36.0)),
        (Rotation::Degrees180, PdfPoint::new(504.0, 36.0)),
        (Rotation::Degrees270, PdfPoint::new(684.0, 504.0)),
    ];
    for (rotation, expected) in cases {
        assert_eq!(
            crop_space(rotation).pdf_to_viewport(PdfPoint::new(72.0, 108.0)),
            expected
        );
    }
}

#[test]
fn user_unit_scales_viewport_but_preserves_raw_pdf_coordinates() {
    let space = PageCoordinateSpace::new(
        PdfRect::new(0.0, 0.0, 612.0, 792.0).unwrap(),
        PdfRect::new(36.0, 72.0, 540.0, 720.0).unwrap(),
        Rotation::Degrees0,
        2.0,
    )
    .unwrap();
    let pdf = PdfPoint::new(72.0, 108.0);
    let viewport = space.pdf_to_viewport(pdf);
    assert_eq!(viewport, PdfPoint::new(72.0, 1368.0));
    assert_eq!(space.viewport_to_pdf(viewport), pdf);
    assert_eq!(space.display_size_points(), (1080.0, 1440.0));
}

#[test]
fn rectangles_round_trip_through_each_rotation() {
    let rect = PdfRect::new(72.0, 108.0, 72.0, 36.0).unwrap();
    for rotation in [
        Rotation::Degrees0,
        Rotation::Degrees90,
        Rotation::Degrees180,
        Rotation::Degrees270,
    ] {
        let space = crop_space(rotation);
        let viewport = space.pdf_rect_to_viewport(rect);
        assert_eq!(space.viewport_rect_to_pdf(viewport), rect);
    }
}

#[test]
fn invalid_boxes_units_and_rotations_are_rejected() {
    assert!(
        PageCoordinateSpace::new(
            PdfRect::new(0.0, 0.0, 10.0, 10.0).unwrap(),
            PdfRect::new(0.0, 0.0, 0.0, 10.0).unwrap(),
            Rotation::Degrees0,
            1.0,
        )
        .is_err()
    );
    assert!(
        PageCoordinateSpace::new(
            PdfRect::new(0.0, 0.0, 10.0, 10.0).unwrap(),
            PdfRect::new(0.0, 0.0, 10.0, 10.0).unwrap(),
            Rotation::Degrees0,
            0.0,
        )
        .is_err()
    );
    assert!(Rotation::from_degrees(45).is_err());
}

#[test]
fn inherited_page_dictionary_values_are_resolved_once_at_the_page_boundary() {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    document.objects.insert(
        pages_id,
        dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "CropBox" => vec![36.into(), 72.into(), 576.into(), 792.into()],
            "Rotate" => 270,
            "UserUnit" => Object::Real(2.0),
        }
        .into(),
    );
    document.objects.insert(
        page_id,
        dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
        }
        .into(),
    );

    let space = PageCoordinateSpace::from_lopdf_page(&document, page_id).unwrap();
    assert_eq!(
        space.media_box(),
        PdfRect::new(0.0, 0.0, 612.0, 792.0).unwrap()
    );
    assert_eq!(
        space.view_box(),
        PdfRect::new(36.0, 72.0, 540.0, 720.0).unwrap()
    );
    assert_eq!(space.rotation(), Rotation::Degrees270);
    assert_eq!(space.user_unit(), 2.0);
}

#[test]
fn page_dictionary_values_override_inherited_values_and_invalid_units_fail() {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    document.objects.insert(
        pages_id,
        dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Rotate" => 90,
            "UserUnit" => Object::Real(2.0),
        }
        .into(),
    );
    document.objects.insert(
        page_id,
        dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "CropBox" => vec![36.into(), 72.into(), 576.into(), 792.into()],
            "Rotate" => -90,
            "UserUnit" => Object::Real(0.0),
        }
        .into(),
    );

    assert!(PageCoordinateSpace::from_lopdf_page(&document, page_id).is_err());
    document
        .get_object_mut(page_id)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("UserUnit", Object::Real(1.5));
    let space = PageCoordinateSpace::from_lopdf_page(&document, page_id).unwrap();
    assert_eq!(space.rotation(), Rotation::Degrees270);
    assert_eq!(space.user_unit(), 1.5);
}

#[test]
fn annotation_transform_consumes_the_same_crop_origin_and_user_unit() {
    let space = crop_space(Rotation::Degrees90);
    let transform = PageTransform::from_page_coordinate_space(space, 1.0).unwrap();
    let point = butter_paper_gpui_gallery::annotation_model::PdfPoint::new(72.0, 108.0).unwrap();
    let local = transform.point_to_local_pixels(point);
    assert_eq!(local.x, 36.0);
    assert_eq!(local.y, 36.0);
    assert_eq!(
        transform.point_from_local_pixels(local.x, local.y).unwrap(),
        butter_paper_gpui_gallery::annotation_model::PdfPoint::new(72.0, 108.0).unwrap()
    );
    assert_eq!(transform.tolerance_points(8.0).unwrap(), 8.0);
}

#[test]
fn effective_rotation_can_change_without_losing_source_boxes_or_user_unit() {
    let source = PageCoordinateSpace::new(
        PdfRect::new(0.0, 0.0, 612.0, 792.0).unwrap(),
        PdfRect::new(36.0, 72.0, 540.0, 720.0).unwrap(),
        Rotation::Degrees0,
        2.0,
    )
    .unwrap();
    let effective = source.with_rotation(Rotation::Degrees90);
    assert_eq!(effective.media_box(), source.media_box());
    assert_eq!(effective.view_box(), source.view_box());
    assert_eq!(effective.user_unit(), 2.0);
    assert_eq!(effective.display_size_points(), (1_440.0, 1_080.0));
}
