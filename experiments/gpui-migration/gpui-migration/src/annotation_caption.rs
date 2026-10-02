//! Shared measured caption layout for paint and transient selection geometry.
//! Layout is rebuilt from the current scene and transform; no painted bounds cache.
use crate::annotation_model::{
    AnnotationScene, AnnotationSelectionSupplement, MeasurementPathKind, PageTransform, PdfPoint,
    PdfRect, SceneDimension, SceneLength, SceneMeasurementPath, TextAlignment, TextBoxStyle,
};
use gpui::{
    App, Bounds, FontWeight, Pixels, Point, ShapedLine, TextAlign, TextRun, Window,
    WindowTextSystem, font, point, px, size,
};
use gpui_component::try_parse_color;

pub(crate) struct CaptionLayout {
    pub bounds: Bounds<Pixels>,
    pub pdf_corners: Vec<PdfPoint>,
    pub width_pt: f64,
    lines: Vec<ShapedLine>,
    line_height: Pixels,
    baseline: Pixels,
    inset: Pixels,
    alignment: TextAlign,
}

impl CaptionLayout {
    pub fn paint(&self, page_origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
        for (index, line) in self.lines.iter().enumerate() {
            let baseline_offset =
                (self.line_height - line.ascent - line.descent) / 2. + line.ascent;
            let origin = point(
                page_origin.x + self.bounds.origin.x + self.inset,
                page_origin.y + self.baseline + self.line_height * index as f32 - baseline_offset,
            );
            let _ = line.paint(
                origin,
                self.line_height,
                self.alignment,
                Some((self.bounds.size.width - self.inset * 2.).max(px(0.))),
                window,
                cx,
            );
        }
    }
}

fn midpoint(start: PdfPoint, end: PdfPoint) -> PdfPoint {
    PdfPoint {
        x: (start.x + end.x) / 2.,
        y: (start.y + end.y) / 2.,
    }
}

fn path_midpoint(points: &[PdfPoint]) -> Option<PdfPoint> {
    let first = *points.first()?;
    let target = points
        .windows(2)
        .map(|p| (p[1].x - p[0].x).hypot(p[1].y - p[0].y))
        .sum::<f64>()
        / 2.;
    let mut travelled = 0.;
    for pair in points.windows(2) {
        let length = (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y);
        if length > 0. && travelled + length >= target {
            let t = (target - travelled) / length;
            return Some(PdfPoint {
                x: pair[0].x + (pair[1].x - pair[0].x) * t,
                y: pair[0].y + (pair[1].y - pair[0].y) * t,
            });
        }
        travelled += length;
    }
    Some(first)
}

pub(crate) fn length_caption(
    annotation: &SceneLength,
    transform: PageTransform,
    text_system: &WindowTextSystem,
) -> Option<CaptionLayout> {
    if !annotation.show_caption {
        return None;
    }
    // Revu centres a Length caption on its dimension line, `LL` above.
    let caption_center = crate::annotation_model::measurement_line_layout(
        annotation.start,
        annotation.end,
        crate::annotation_model::LENGTH_LEADER_LENGTH_PT,
        0.,
        0.,
    )?
    .caption_center;
    layout(
        &annotation.caption,
        annotation.appearance.text(),
        caption_center,
        true,
        transform,
        text_system,
    )
}

pub(crate) fn measurement_caption(
    annotation: &SceneMeasurementPath,
    transform: PageTransform,
    text_system: &WindowTextSystem,
) -> Option<CaptionLayout> {
    if !annotation.show_caption {
        return None;
    }
    let anchor = if annotation.kind == MeasurementPathKind::Area && annotation.points.len() >= 3 {
        let count = annotation.points.len() as f64;
        PdfPoint {
            x: annotation.points.iter().map(|p| p.x).sum::<f64>() / count,
            y: annotation.points.iter().map(|p| p.y).sum::<f64>() / count,
        }
    } else {
        path_midpoint(&annotation.points)?
    };
    // Revu centres an Area caption inside the area.
    layout(
        &annotation.caption,
        &annotation.text_style,
        anchor,
        annotation.kind == MeasurementPathKind::Area,
        transform,
        text_system,
    )
}

pub(crate) fn dimension_caption(
    annotation: &SceneDimension,
    transform: PageTransform,
    text_system: &WindowTextSystem,
) -> Option<CaptionLayout> {
    let dx = annotation.end.x - annotation.start.x;
    let dy = annotation.end.y - annotation.start.y;
    let length = dx.hypot(dy);
    let center = midpoint(annotation.start, annotation.end);
    let anchor = PdfPoint {
        x: center.x - dy / length * annotation.dimension_line_offset,
        y: center.y + dx / length * annotation.dimension_line_offset,
    };
    layout(
        &annotation.content,
        annotation.appearance.text(),
        anchor,
        true,
        transform,
        text_system,
    )
}

fn layout(
    text: &str,
    style: &TextBoxStyle,
    anchor: PdfPoint,
    dimension: bool,
    transform: PageTransform,
    text_system: &WindowTextSystem,
) -> Option<CaptionLayout> {
    let scale = transform.pixels_per_point() as f32;
    let font_pixels = style.font_size_pt() as f32 * scale;
    let line_pixels = style.line_height_pt() as f32 * scale;
    let inset_pixels = style.inset_pt() as f32 * scale;
    if !scale.is_finite()
        || scale <= 0.
        || !font_pixels.is_finite()
        || font_pixels <= 0.
        || !line_pixels.is_finite()
        || line_pixels <= 0.
        || !inset_pixels.is_finite()
        || !anchor.x.is_finite()
        || !anchor.y.is_finite()
    {
        return None;
    }
    let font_size = px(style.font_size_pt() as f32 * scale);
    let line_height = px(style.line_height_pt() as f32 * scale);
    let inset = px(style.inset_pt() as f32 * scale);
    let display_family = match style.font_family() {
        "Arimo" => "Arial",
        "Tinos" => "Times New Roman",
        "Roboto Mono" => "Courier New",
        family => family,
    };
    let mut caption_font = font(display_family.to_owned());
    caption_font.weight = FontWeight(style.weight() as f32);
    let colour = try_parse_color(style.color())
        .unwrap_or(gpui::black())
        .opacity(style.opacity() as f32);
    let lines = text
        .split('\n')
        .map(|text| {
            let run = TextRun {
                len: text.len(),
                font: caption_font.clone(),
                color: colour,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            text_system.shape_line(text.to_owned().into(), font_size, &[run], None)
        })
        .collect::<Vec<_>>();
    let measured_width = lines
        .iter()
        .map(|line| line.width())
        .fold(px(0.), Pixels::max);
    if !f32::from(measured_width).is_finite()
        || lines
            .iter()
            .any(|line| !f32::from(line.ascent).is_finite() || !f32::from(line.descent).is_finite())
    {
        return None;
    }
    let width =
        (measured_width + inset * 2.).max(font_size * if dimension { 1. } else { 56. / 12. });
    let height = line_height.max(font_size * if dimension { 1. } else { 1.5 });
    let width_pt = f32::from(width) as f64 / scale as f64;
    let height_pt = f32::from(height) as f64 / scale as f64;
    let rect = PdfRect {
        x: if dimension {
            anchor.x - width_pt / 2.
        } else {
            anchor.x + 6.
        },
        y: if dimension {
            anchor.y - height_pt / 2.
        } else {
            anchor.y + 6.
        },
        width: width_pt,
        height: height_pt,
    };
    let projected = transform.rect_to_local_pixels(rect);
    if ![projected.x, projected.y, projected.width, projected.height]
        .into_iter()
        .all(|v| (v as f32).is_finite())
    {
        return None;
    }
    // Glyphs remain horizontal on rotated pages. Include their actual extent,
    // rather than inheriting the reference's swapped-rectangle overflow mismatch.
    let mut bounds = Bounds::new(
        point(px(projected.x as f32), px(projected.y as f32)),
        size(
            px(projected.width as f32).max(measured_width + inset * 2.),
            px(projected.height as f32)
                .max(height + line_height * (lines.len().saturating_sub(1) as f32)),
        ),
    );
    let baseline = bounds.origin.y + font_size * (13. / 12.);
    let glyph_top = lines
        .iter()
        .map(|line| baseline - line.ascent)
        .fold(bounds.top(), Pixels::min);
    let glyph_bottom = lines
        .iter()
        .enumerate()
        .map(|(index, line)| baseline + line_height * index as f32 + line.descent)
        .fold(bounds.bottom(), Pixels::max);
    bounds.origin.y = glyph_top;
    bounds.size.height = glyph_bottom - glyph_top;
    let pdf_corners = caption_pdf_corners(bounds, transform)?;
    Some(CaptionLayout {
        bounds,
        pdf_corners,
        width_pt,
        lines,
        line_height,
        baseline,
        inset,
        alignment: match style.alignment() {
            TextAlignment::Left => TextAlign::Left,
            TextAlignment::Center => TextAlign::Center,
            TextAlignment::Right => TextAlign::Right,
        },
    })
}

fn caption_pdf_corners(bounds: Bounds<Pixels>, transform: PageTransform) -> Option<Vec<PdfPoint>> {
    let corners = [
        bounds.origin,
        point(bounds.right(), bounds.top()),
        point(bounds.right(), bounds.bottom()),
        point(bounds.left(), bounds.bottom()),
    ];
    if corners
        .iter()
        .any(|p| !f32::from(p.x).is_finite() || !f32::from(p.y).is_finite())
    {
        return None;
    }
    corners
        .into_iter()
        .map(|p| {
            transform
                .point_from_local_pixels(f32::from(p.x) as f64, f32::from(p.y) as f64)
                .ok()
        })
        .collect()
}

pub(crate) fn selection_supplement(
    scene: &AnnotationScene,
    transform: PageTransform,
    text_system: &WindowTextSystem,
) -> AnnotationSelectionSupplement {
    let mut result = AnnotationSelectionSupplement::new();
    for annotation in &scene.lengths {
        if let Some(layout) = length_caption(annotation, transform, text_system) {
            result.insert(annotation.id.clone(), layout.pdf_corners);
        }
    }
    for annotation in &scene.measurement_paths {
        if let Some(layout) = measurement_caption(annotation, transform, text_system) {
            result.insert(annotation.id.clone(), layout.pdf_corners);
        }
    }
    for annotation in &scene.dimensions {
        if let Some(layout) = dimension_caption(annotation, transform, text_system) {
            result.insert(annotation.id.clone(), layout.pdf_corners);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation_model::{
        DimensionAppearance, LineKind, MarkupId, PageRotation, StraightLineAppearance,
    };

    #[test]
    fn caption_layout_metrics_reject_values_that_round_to_zero_or_overflow() {
        for value in [1e-10, 1e308] {
            assert!(
                TextBoxStyle::new("Helvetica", 12., "#000000", 1.)
                    .unwrap()
                    .with_layout_metrics(value, 0.)
                    .is_err()
            );
        }
        assert!(
            TextBoxStyle::new("Helvetica", 12., "#000000", 1.)
                .unwrap()
                .with_layout_metrics(12., 1e308)
                .is_err()
        );
    }

    #[test]
    fn caption_layout_rejects_overflowing_right_and_bottom_edges() {
        let transform = PageTransform::new(792., 1.).unwrap();
        for bounds in [
            Bounds::new(point(px(3e38), px(0.)), size(px(3e38), px(20.))),
            Bounds::new(point(px(0.), px(3e38)), size(px(20.), px(3e38))),
        ] {
            assert!(caption_pdf_corners(bounds, transform).is_none());
        }
    }

    #[test]
    fn caption_layout_path_anchor_uses_distance_not_vertex_mean() {
        let points = [
            PdfPoint { x: 10., y: 20. },
            PdfPoint { x: 110., y: 20. },
            PdfPoint { x: 110., y: 40. },
        ];
        assert_eq!(path_midpoint(&points), Some(PdfPoint { x: 70., y: 20. }));
        assert_eq!(path_midpoint(&[]), None);
    }

    #[gpui::test]
    fn caption_layout_shapes_current_metrics_and_inverse_projects_every_corner(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let text_system = WindowTextSystem::new(cx.text_system().clone());
            let style = TextBoxStyle::new("Arimo", 20., "#000000", 1.)
                .unwrap()
                .with_layout_metrics(13.8, 3.)
                .unwrap();
            let anchor = PdfPoint { x: 200., y: 300. };
            for rotation in [
                PageRotation::Degrees0,
                PageRotation::Degrees90,
                PageRotation::Degrees180,
                PageRotation::Degrees270,
            ] {
                for zoom in [0.5, 1., 2.] {
                    let transform = PageTransform::new_rotated(612., 792., zoom, rotation).unwrap();
                    let caption = layout(
                        "A long measured caption",
                        &style,
                        anchor,
                        false,
                        transform,
                        &text_system,
                    )
                    .unwrap();
                    let shaped_width = caption.lines[0].width();
                    assert!(
                        (caption.width_pt - (f32::from(shaped_width) as f64 / zoom + 6.)).abs()
                            < 0.001
                    );
                    assert!(caption.bounds.size.width >= shaped_width + px(6. * zoom as f32));
                    assert_eq!(caption.line_height, px(13.8 * zoom as f32));
                    assert_eq!(caption.pdf_corners.len(), 4);
                    let expected = [
                        caption.bounds.origin,
                        point(caption.bounds.right(), caption.bounds.top()),
                        point(caption.bounds.right(), caption.bounds.bottom()),
                        point(caption.bounds.left(), caption.bounds.bottom()),
                    ];
                    for (pdf, viewport) in caption.pdf_corners.iter().zip(expected) {
                        let actual = transform.point_to_local_pixels(*pdf);
                        assert!((actual.x - f32::from(viewport.x) as f64).abs() < 0.001);
                        assert!((actual.y - f32::from(viewport.y) as f64).abs() < 0.001);
                    }
                }
            }
            let invalid = TextBoxStyle::new("Helvetica", 1e100, "#000000", 1.).unwrap();
            assert!(
                layout(
                    "safe rejection",
                    &invalid,
                    anchor,
                    false,
                    PageTransform::new(792., 1.).unwrap(),
                    &text_system
                )
                .is_none()
            );
            let transform = PageTransform::new(792., 1.).unwrap();
            let small = layout(
                "Measured caption",
                &TextBoxStyle::new("Helvetica", 10., "#000000", 1.).unwrap(),
                anchor,
                false,
                transform,
                &text_system,
            )
            .unwrap();
            let large = layout(
                "Measured caption",
                &style,
                anchor,
                false,
                transform,
                &text_system,
            )
            .unwrap();
            assert!(
                large.width_pt > small.width_pt,
                "layout rebuilds from current font metrics"
            );
            let multi = layout(
                "Measured caption\nSecond line",
                &style,
                anchor,
                true,
                transform,
                &text_system,
            )
            .unwrap();
            assert!(multi.bounds.size.height > large.bounds.size.height);
        });
    }

    #[gpui::test]
    fn caption_layout_hidden_length_has_no_supplement(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let text_system = WindowTextSystem::new(cx.text_system().clone());
            let mut length = SceneLength {
                id: MarkupId::new("caption:hidden").unwrap(),
                start: PdfPoint { x: 10., y: 20. },
                end: PdfPoint { x: 110., y: 20. },
                caption: "100 m".into(),
                show_caption: false,
                appearance: DimensionAppearance::new(
                    StraightLineAppearance::default_for(LineKind::Line),
                    TextBoxStyle::new("Helvetica", 12., "#000000", 1.).unwrap(),
                )
                .unwrap(),
                selected: false,
                locked: false,
                draft: false,
                feedback: crate::annotation_model::SceneInteractionFeedback::Normal,
            };
            let transform = PageTransform::new(792., 1.).unwrap();
            assert!(length_caption(&length, transform, &text_system).is_none());
            length.show_caption = true;
            let visible = length_caption(&length, transform, &text_system).unwrap();
            // Centred on the dimension line 10 pt above the points, as in Revu.
            let min_x = visible.pdf_corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
            let max_x = visible.pdf_corners.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
            let min_y = visible.pdf_corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
            let max_y = visible.pdf_corners.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
            assert!(((min_x + max_x) / 2. - 60.).abs() < 0.001);
            assert!(((min_y + max_y) / 2. - 30.).abs() < 0.001);
            assert!(
                TextBoxStyle::new("Helvetica", 12., "#000000", 1.)
                    .unwrap()
                    .with_layout_metrics(0., 0.)
                    .is_err()
            );
            assert!(
                TextBoxStyle::new("Helvetica", 12., "#000000", 1.)
                    .unwrap()
                    .with_layout_metrics(12., -1.)
                    .is_err()
            );
        });
    }
}
