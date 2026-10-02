//! Page previews for templates, shared by the template picker and manager.
//!
//! Mirrors Electron's `TemplatePreviewCard`: a muted frame holding the page at
//! its true aspect ratio with the pattern drawn at its real spacing, followed
//! by the template name, its size summary and a page-grid badge.

use gpui::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Rgba, SharedString,
    Styled as _, div,
    prelude::FluentBuilder as _, px, rgb, svg,
};
use gpui_component::{ActiveTheme as _, h_flex, v_flex};

use crate::generated_document::{GeneratedDocumentRequest, GeneratedPattern};

#[derive(Clone, Debug, PartialEq)]
pub enum TemplatePreview {
    Generated(GeneratedDocumentRequest),
    ImportedPdf { page_count: usize },
}

impl TemplatePreview {
    /// Preview for a built-in template id, or blank A3 landscape.
    pub fn for_built_in(id: &str) -> Self {
        Self::Generated(
            crate::template_library::built_in_request(id)
                .unwrap_or_else(GeneratedDocumentRequest::a3_landscape_blank),
        )
    }

    /// "420 × 297 mm · Landscape" or "3 pages · Imported PDF".
    pub fn summary(&self) -> String {
        match self {
            Self::Generated(request) => format!(
                "{} × {} mm · {}",
                format_mm(request.width_mm),
                format_mm(request.height_mm),
                if request.width_mm >= request.height_mm { "Landscape" } else { "Portrait" }
            ),
            Self::ImportedPdf { page_count } => format!(
                "{page_count} {} · Imported PDF",
                if *page_count == 1 { "page" } else { "pages" }
            ),
        }
    }

    /// "No page grid", "Page grid · 10 mm" or "Page grid not defined".
    pub fn grid_summary(&self) -> String {
        match self {
            Self::Generated(request) => match request.pattern.as_ref() {
                None => "No page grid".into(),
                Some(pattern) => format!("Page grid · {} mm", format_mm(pattern_style(pattern).1)),
            },
            Self::ImportedPdf { .. } => "Page grid not defined".into(),
        }
    }
}

fn format_mm(value: f64) -> String {
    let rounded = (value * 10.).round() / 10.;
    if rounded.fract() == 0. {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    }
}

fn pattern_style(pattern: &GeneratedPattern) -> (&'static str, f64, &str) {
    match pattern {
        GeneratedPattern::Dots { spacing_mm, color } => ("dots", *spacing_mm, color),
        GeneratedPattern::SquareGrid { spacing_mm, color } => ("grid", *spacing_mm, color),
        GeneratedPattern::Ruled { spacing_mm, color } => ("lined", *spacing_mm, color),
        GeneratedPattern::Isometric { spacing_mm, color } => ("isometric", *spacing_mm, color),
        GeneratedPattern::Triangle { spacing_mm, color } => ("triangle", *spacing_mm, color),
    }
}

fn parse_hex(color: &str) -> Option<Rgba> {
    let hex = color.strip_prefix('#')?;
    (hex.len() == 6).then(|| u32::from_str_radix(hex, 16).ok()).flatten().map(rgb)
}

/// Pattern artwork in page millimetres, drawn at the page's real spacing.
/// GPUI paints SVGs as a single-colour mask, so the colour is applied by the
/// caller; the attributes record it for tests and tooling.
pub fn template_preview_svg(request: &GeneratedDocumentRequest) -> String {
    let (width, height) = (request.width_mm.max(1.), request.height_mm.max(1.));
    let Some(pattern) = request.pattern.as_ref() else {
        return format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" data-pattern=\"blank\"/>"
        );
    };
    let (kind, spacing_mm, color) = pattern_style(pattern);
    // Keep at most ~60 marks across the short side so previews stay legible.
    let step = spacing_mm.max(width.min(height) / 60.).max(0.5);
    // About one device pixel at typical preview sizes.
    let stroke = width.max(height) / 260.;
    let mut marks = String::new();
    let line = |marks: &mut String, x1: f64, y1: f64, x2: f64, y2: f64| {
        marks.push_str(&format!("M{x1:.2} {y1:.2}L{x2:.2} {y2:.2}"));
    };
    // Parallel lines at `angle` degrees from horizontal, `gap` apart, covering the page.
    let family = |marks: &mut String, angle: f64, gap: f64| {
        let (sin, cos) = angle.to_radians().sin_cos();
        let (nx, ny) = (-sin, cos);
        let reach = width.hypot(height);
        let corners = [(0., 0.), (width, 0.), (0., height), (width, height)];
        let (low, high) = corners.iter().fold((f64::MAX, f64::MIN), |(low, high), (x, y)| {
            let d = x * nx + y * ny;
            (low.min(d), high.max(d))
        });
        let mut offset = (low / gap).ceil() * gap;
        while offset <= high {
            let (cx, cy) = (nx * offset, ny * offset);
            line(marks, cx - cos * reach, cy - sin * reach, cx + cos * reach, cy + sin * reach);
            offset += gap;
        }
    };
    let mut dots = String::new();
    match kind {
        "dots" => {
            let radius = (step / 10.).max(stroke * 1.1);
            let mut y = step / 2.;
            while y < height {
                let mut x = step / 2.;
                while x < width {
                    dots.push_str(&format!("<circle cx=\"{x:.2}\" cy=\"{y:.2}\" r=\"{radius:.2}\"/>"));
                    x += step;
                }
                y += step;
            }
        }
        "lined" => family(&mut marks, 0., step),
        "grid" => {
            family(&mut marks, 0., step);
            family(&mut marks, 90., step);
        }
        "isometric" => {
            family(&mut marks, 90., step);
            family(&mut marks, 30., step * 3f64.sqrt() / 2.);
            family(&mut marks, -30., step * 3f64.sqrt() / 2.);
        }
        _ => {
            family(&mut marks, 0., step * 3f64.sqrt() / 2.);
            family(&mut marks, 60., step * 3f64.sqrt() / 2.);
            family(&mut marks, -60., step * 3f64.sqrt() / 2.);
        }
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" preserveAspectRatio=\"xMidYMid meet\" data-pattern=\"{kind}\" data-spacing-mm=\"{spacing_mm}\" data-color=\"{color}\"><g fill=\"{color}\" stroke=\"none\">{dots}</g><path d=\"{marks}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{stroke:.3}\"/></svg>"
    )
}

/// Page sized to fit `frame` (width, height) at the template's aspect ratio.
fn fitted_page(width_mm: f64, height_mm: f64, frame: (f32, f32)) -> (f32, f32) {
    let aspect = (width_mm / height_mm).clamp(0.2, 5.) as f32;
    if frame.0 / frame.1 > aspect {
        (frame.1 * aspect, frame.1)
    } else {
        (frame.0, frame.0 / aspect)
    }
}

/// The page on a muted frame, like Electron's `BlankPdfPagePreview`.
pub fn template_preview_page(
    preview: &TemplatePreview,
    page_id: &'static str,
    compact: bool,
    frame_width: f32,
    cx: &App,
) -> AnyElement {
    let frame_height = if compact { 128. } else { 192. };
    let padding = if compact { 8. } else { 12. };
    let inner = (frame_width - padding * 2., frame_height - padding * 2.);
    let page = match preview {
        TemplatePreview::Generated(request) => {
            let (width, height) = fitted_page(request.width_mm, request.height_mm, inner);
            let color = request
                .pattern
                .as_ref()
                .and_then(|pattern| parse_hex(pattern_style(pattern).2))
                .unwrap_or(rgb(0xd1d5db));
            div()
                .id(page_id)
                .debug_selector(move || page_id.into())
                .relative()
                .flex_none()
                .w(px(width))
                .h(px(height))
                .overflow_hidden()
                .border_1()
                .border_color(cx.theme().border)
                .bg(rgb(0xffffff))
                .shadow_xs()
                .child(
                    svg()
                        .absolute()
                        .inset_0()
                        .size_full()
                        .text_color(color)
                        .data(SharedString::from(template_preview_svg(request)).as_bytes()),
                )
                .into_any_element()
        }
        TemplatePreview::ImportedPdf { .. } => {
            let (width, height) = fitted_page(210., 297., (inner.0, inner.1 * 0.85));
            div()
                .id(page_id)
                .debug_selector(move || page_id.into())
                .flex_none()
                .w(px(width))
                .h(px(height))
                .flex()
                .items_center()
                .justify_center()
                .border_1()
                .border_color(cx.theme().border)
                .bg(rgb(0xffffff))
                .shadow_xs()
                .text_xs()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(0x000000))
                .child("PDF")
                .into_any_element()
        }
    };
    div()
        .w(px(frame_width))
        .h(px(frame_height))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_lg()
        .bg(cx.theme().muted.opacity(0.5))
        .child(page)
        .into_any_element()
}

/// Electron's `TemplatePreviewCard`: page preview, name, summary and grid badge.
pub fn template_preview_card(
    preview: &TemplatePreview,
    name: impl Into<SharedString>,
    page_id: &'static str,
    compact: bool,
    width: f32,
    cx: &App,
) -> AnyElement {
    let patterned = matches!(preview, TemplatePreview::Generated(request) if request.pattern.is_some());
    v_flex()
        .w(px(width))
        .flex_none()
        .gap_2()
        .child(template_preview_page(preview, page_id, compact, width, cx))
        .child(
            h_flex()
                .min_w_0()
                .when(compact, |row| row.flex_col())
                .items_start()
                .justify_between()
                .gap_2()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(div().truncate().text_sm().font_weight(gpui::FontWeight::MEDIUM).child(name.into()))
                        .child(
                            div()
                                .text_xs()
                                .whitespace_nowrap()
                                .truncate()
                                .text_color(cx.theme().muted_foreground)
                                .child(preview.summary()),
                        ),
                )
                .when(patterned, |row| {
                    row.child(
                        div()
                            .flex_none()
                            .rounded_md()
                            .px_2()
                            .py_0p5()
                            .text_xs()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .bg(cx.theme().secondary)
                            .text_color(cx.theme().secondary_foreground)
                            .child(preview.grid_summary()),
                    )
                }),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_match_the_electron_template_card() {
        let dots = TemplatePreview::for_built_in("built-in-dots");
        assert_eq!(dots.summary(), "420 × 297 mm · Landscape");
        assert_eq!(dots.grid_summary(), "Page grid · 10 mm");
        let blank = TemplatePreview::for_built_in("built-in-blank");
        assert_eq!(blank.grid_summary(), "No page grid");
        let imported = TemplatePreview::ImportedPdf { page_count: 1 };
        assert_eq!(imported.summary(), "1 page · Imported PDF");
        assert_eq!(imported.grid_summary(), "Page grid not defined");
        let portrait = GeneratedDocumentRequest { width_mm: 210., height_mm: 297.5, ..GeneratedDocumentRequest::a3_landscape_blank() };
        assert_eq!(TemplatePreview::Generated(portrait).summary(), "210 × 297.5 mm · Portrait");
    }

    #[test]
    fn preview_artwork_uses_page_millimetres_and_real_spacing() {
        for id in ["built-in-dots", "built-in-grid", "built-in-lined", "built-in-isometric", "built-in-triangle"] {
            let request = crate::template_library::built_in_request(id).unwrap();
            let svg = template_preview_svg(&request);
            assert!(svg.contains("viewBox=\"0 0 420 297\""), "{id}: {svg}");
            assert!(svg.contains("data-spacing-mm=\"10\""), "{id}");
            assert!(svg.contains("M") || svg.contains("<circle"), "{id}");
        }
        let grid = template_preview_svg(&crate::template_library::built_in_request("built-in-grid").unwrap());
        // 10 mm across a 297 mm page: 29 horizontal and 42 vertical lines plus edges.
        assert!(grid.matches('M').count() >= 29 + 42);
        assert!(template_preview_svg(&GeneratedDocumentRequest::a3_landscape_blank()).contains("data-pattern=\"blank\""));
    }

    #[test]
    fn pages_fit_the_frame_at_their_aspect_ratio() {
        assert_eq!(fitted_page(420., 297., (200., 100.)), (100. * 420. / 297., 100.));
        assert_eq!(fitted_page(210., 297., (100., 200.)), (100., 100. * 297. / 210.));
    }
}
