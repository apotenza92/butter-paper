//! Renderer-independent ink path construction shared by GPUI paint surfaces.
//!
//! The cubic control points intentionally match the maintained Electron
//! `interpolatingInkPath` contract. Annotation points remain unchanged for
//! persistence and hit testing; smoothing is a paint-only decision.

use crate::annotation_model::PdfPoint;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InkPaintPathSegment {
    MoveTo(PdfPoint),
    LineTo(PdfPoint),
    CubicTo {
        control_a: PdfPoint,
        control_b: PdfPoint,
        to: PdfPoint,
    },
}

pub fn build_ink_paint_path(points: &[PdfPoint], smooth_curves: bool) -> Vec<InkPaintPathSegment> {
    let Some(first) = points.first().copied() else {
        return Vec::new();
    };
    let mut segments = Vec::with_capacity(points.len());
    segments.push(InkPaintPathSegment::MoveTo(first));
    if points.len() == 1 {
        return segments;
    }
    if !smooth_curves || points.len() == 2 {
        segments.extend(
            points
                .iter()
                .skip(1)
                .copied()
                .map(InkPaintPathSegment::LineTo),
        );
        return segments;
    }
    for index in 0..points.len() - 1 {
        let p0 = points[index.saturating_sub(1)];
        let p1 = points[index];
        let p2 = points[index + 1];
        let p3 = points[(index + 2).min(points.len() - 1)];
        let control_a = PdfPoint::new(p1.x + (p2.x - p0.x) / 6.0, p1.y + (p2.y - p0.y) / 6.0)
            .expect("finite annotation points produce finite cubic controls");
        let control_b = PdfPoint::new(p2.x - (p3.x - p1.x) / 6.0, p2.y - (p3.y - p1.y) / 6.0)
            .expect("finite annotation points produce finite cubic controls");
        segments.push(InkPaintPathSegment::CubicTo {
            control_a,
            control_b,
            to: p2,
        });
    }
    segments
}
