use butter_paper_gpui_gallery::{
    annotation_model::{MarkupId, PdfPoint, PenAnnotation, PenAppearance},
    highlight_compositor::{
        HighlightRasterMapping, precompose_highlights_multiply_rgba,
        precompose_highlights_multiply_rgba_mapped,
    },
    page_geometry::{PageCoordinateSpace, PdfRect as CoordinateRect, Rotation},
};

fn white_surface(width: u32, height: u32) -> Vec<u8> {
    vec![255; width as usize * height as usize * 4]
}

fn pixel(surface: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let start = ((y * width + x) * 4) as usize;
    surface[start..start + 4].try_into().unwrap()
}

#[test]
fn cpu_precomposition_applies_true_multiply_without_changing_alpha_or_outside_pixels() {
    let highlight = PenAnnotation::new_highlight(
        MarkupId::new("highlight:oracle").unwrap(),
        0,
        vec![
            PdfPoint::new(1., 3.5).unwrap(),
            PdfPoint::new(5., 3.5).unwrap(),
        ],
        PenAppearance::new("#ffff00", 2., 1.).unwrap(),
    )
    .unwrap();
    let mut surface = white_surface(7, 7);

    let changed = precompose_highlights_multiply_rgba(
        &mut surface,
        7,
        7,
        7.,
        7.,
        std::slice::from_ref(&highlight),
    )
    .unwrap();

    assert!(changed > 0);
    assert_eq!(pixel(&surface, 7, 3, 3), [255, 255, 0, 255]);
    assert_eq!(pixel(&surface, 7, 3, 0), [255, 255, 255, 255]);
    assert!(surface.chunks_exact(4).all(|pixel| pixel[3] == 255));
}

#[test]
fn cpu_precomposition_uses_source_color_and_opacity_and_ignores_pen_paths() {
    let highlight = PenAnnotation::new_highlight(
        MarkupId::new("highlight:opacity").unwrap(),
        0,
        vec![
            PdfPoint::new(1., 3.5).unwrap(),
            PdfPoint::new(5., 3.5).unwrap(),
        ],
        PenAppearance::new("#808000", 2., 0.5).unwrap(),
    )
    .unwrap();
    let pen = PenAnnotation::new(
        MarkupId::new("pen:ignored").unwrap(),
        0,
        vec![
            PdfPoint::new(1., 1.5).unwrap(),
            PdfPoint::new(5., 1.5).unwrap(),
        ],
        PenAppearance::new("#ff0000", 2., 1.).unwrap(),
    )
    .unwrap();
    let mut surface = vec![200, 100, 50, 231].repeat(49);

    precompose_highlights_multiply_rgba(&mut surface, 7, 7, 7., 7., &[highlight, pen]).unwrap();

    assert_eq!(pixel(&surface, 7, 3, 3), [150, 75, 25, 231]);
    assert_eq!(pixel(&surface, 7, 3, 5), [200, 100, 50, 231]);
}

#[test]
fn cpu_precomposition_rejects_invalid_raster_or_page_geometry() {
    assert!(precompose_highlights_multiply_rgba(&mut [0; 4], 0, 1, 1., 1., &[]).is_err());
    assert!(precompose_highlights_multiply_rgba(&mut [0; 3], 1, 1, 1., 1., &[]).is_err());
    assert!(precompose_highlights_multiply_rgba(&mut [0; 4], 1, 1, f64::NAN, 1., &[]).is_err());
}

#[test]
fn crop_mapping_places_only_the_intersecting_page_space_highlight_in_a_tile() {
    let page_zero = PenAnnotation::new_highlight(
        MarkupId::new("highlight:tile-page-zero").unwrap(),
        0,
        vec![
            PdfPoint::new(4., 5.).unwrap(),
            PdfPoint::new(8., 5.).unwrap(),
        ],
        PenAppearance::new("#ffff00", 2., 1.).unwrap(),
    )
    .unwrap();
    let other_page = PenAnnotation::new_highlight(
        MarkupId::new("highlight:tile-other-page").unwrap(),
        1,
        vec![
            PdfPoint::new(4., 5.).unwrap(),
            PdfPoint::new(8., 5.).unwrap(),
        ],
        PenAppearance::new("#00ffff", 2., 1.).unwrap(),
    )
    .unwrap();
    let mut surface = white_surface(4, 4);

    let changed = precompose_highlights_multiply_rgba_mapped(
        &mut surface,
        4,
        4,
        HighlightRasterMapping::new(0, 10., 10., 2., 2., 8., 6.).unwrap(),
        &[page_zero, other_page],
    )
    .unwrap();

    assert!(changed > 0);
    assert_eq!(pixel(&surface, 4, 2, 2), [255, 255, 0, 255]);
    assert_eq!(pixel(&surface, 4, 0, 0), [255, 255, 255, 255]);
}

#[test]
fn crop_rotation_and_user_unit_share_the_canonical_page_transform() {
    let coordinate_space = PageCoordinateSpace::new(
        CoordinateRect::new(0., 0., 360., 240.).unwrap(),
        CoordinateRect::new(18., 24., 324., 192.).unwrap(),
        Rotation::Degrees90,
        2.,
    )
    .unwrap();
    let highlight = PenAnnotation::new_highlight(
        MarkupId::new("highlight:coordinate-space").unwrap(),
        0,
        vec![
            PdfPoint::new(72., 60.).unwrap(),
            PdfPoint::new(144., 60.).unwrap(),
        ],
        PenAppearance::new("#ffff00", 2., 1.).unwrap(),
    )
    .unwrap();
    let mut surface = white_surface(384, 648);

    let changed = precompose_highlights_multiply_rgba_mapped(
        &mut surface,
        384,
        648,
        HighlightRasterMapping::from_coordinate_space(0, coordinate_space, 1., 1., 0., 0.).unwrap(),
        &[highlight],
    )
    .unwrap();

    assert!(changed > 0);
    assert_eq!(pixel(&surface, 384, 72, 180), [255, 255, 0, 255]);
    assert_eq!(pixel(&surface, 384, 180, 72), [255, 255, 255, 255]);
}
