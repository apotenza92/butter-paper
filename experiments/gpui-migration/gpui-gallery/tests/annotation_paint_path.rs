use butter_paper_gpui_gallery::{
    annotation_model::PdfPoint,
    annotation_paint_path::{InkPaintPathSegment, build_ink_paint_path},
};

fn point(x: f64, y: f64) -> PdfPoint {
    PdfPoint::new(x, y).unwrap()
}

#[test]
fn smooth_path_matches_the_electron_interpolating_cubic_contract() {
    let path = build_ink_paint_path(&[point(0.0, 0.0), point(6.0, 6.0), point(12.0, 0.0)], true);
    assert_eq!(
        path,
        [
            InkPaintPathSegment::MoveTo(point(0.0, 0.0)),
            InkPaintPathSegment::CubicTo {
                control_a: point(1.0, 1.0),
                control_b: point(4.0, 6.0),
                to: point(6.0, 6.0),
            },
            InkPaintPathSegment::CubicTo {
                control_a: point(8.0, 6.0),
                control_b: point(11.0, 1.0),
                to: point(12.0, 0.0),
            },
        ]
    );
}

#[test]
fn raw_points_remain_a_polyline_when_smoothing_is_disabled() {
    let path = build_ink_paint_path(&[point(0.0, 0.0), point(6.0, 6.0), point(12.0, 0.0)], false);
    assert_eq!(
        path,
        [
            InkPaintPathSegment::MoveTo(point(0.0, 0.0)),
            InkPaintPathSegment::LineTo(point(6.0, 6.0)),
            InkPaintPathSegment::LineTo(point(12.0, 0.0)),
        ]
    );
}
