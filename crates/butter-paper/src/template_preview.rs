//! Page previews for templates, shared by the template picker and manager.
//!
//! Mirrors Electron's `TemplatePreviewCard`: a muted frame holding the page at
//! its true aspect ratio with the pattern drawn at its real spacing, followed
//! by the template name, its size summary and a page-grid badge.

use gpui::{
    AnyElement, App, Global, InteractiveElement as _, IntoElement, ParentElement as _, RenderImage,
    Rgba, SharedString, Styled as _, div, img, prelude::FluentBuilder as _, px, rgb, svg,
};
use gpui_component::{ActiveTheme as _, h_flex, v_flex};
use std::{collections::HashMap, sync::Arc};

use crate::generated_document::{GeneratedDocumentRequest, GeneratedPattern};
use crate::page_scale_catalog::paper_size_mm;

/// The label column shared by the paper-size and orientation rows, so their
/// controls line up.
pub const TEMPLATE_CONTROL_LABEL_WIDTH: f32 = 84.;

/// A labelled row of the template controls: "Paper size" and its Select, or
/// "Orientation" and its toggle.
pub fn template_control_row(
    id: &'static str,
    label: &'static str,
    control: impl IntoElement,
) -> impl IntoElement {
    h_flex()
        .id(id)
        .debug_selector(move || id.into())
        .gap_2()
        .child(
            div()
                .flex_none()
                .w(px(TEMPLATE_CONTROL_LABEL_WIDTH))
                .text_sm()
                .child(label),
        )
        .child(div().flex_1().min_w_0().child(control))
}

/// Portrait and Landscape side by side, the current one selected. Imported
/// PDFs keep their own pages, so it is disabled for them.
pub fn orientation_toggle(
    id_prefix: &'static str,
    orientation: PaperOrientation,
    disabled: bool,
    on_change: impl Fn(PaperOrientation, &mut gpui::Window, &mut App) + Clone + 'static,
) -> impl IntoElement {
    use gpui_component::button::{Button, ButtonGroup};
    use gpui_component::{Disableable as _, Selectable as _};
    let button = |which: PaperOrientation| {
        let on_change = on_change.clone();
        let id = format!("{id_prefix}-{}", which.label().to_lowercase());
        Button::new(SharedString::from(id.clone()))
            .debug_selector(move || id.clone())
            .label(which.label())
            .selected(orientation == which)
            .toggled(orientation == which)
            .on_click(move |_, window, cx| on_change(which, window, cx))
    };
    ButtonGroup::new(SharedString::from(format!("{id_prefix}-group")))
        .outline()
        .disabled(disabled)
        .child(button(PaperOrientation::Portrait))
        .child(button(PaperOrientation::Landscape))
}

/// Which way a sheet of paper is turned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaperOrientation {
    Portrait,
    Landscape,
}

impl PaperOrientation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Portrait => "Portrait",
            Self::Landscape => "Landscape",
        }
    }

    /// How a `width_mm` × `height_mm` sheet is turned; a square counts as
    /// landscape.
    pub fn of(width_mm: f64, height_mm: f64) -> Self {
        if width_mm >= height_mm {
            Self::Landscape
        } else {
            Self::Portrait
        }
    }
}

/// `request` with its paper turned to `orientation`.
pub fn oriented_request(
    mut request: GeneratedDocumentRequest,
    orientation: PaperOrientation,
) -> GeneratedDocumentRequest {
    if PaperOrientation::of(request.width_mm, request.height_mm) != orientation {
        std::mem::swap(&mut request.width_mm, &mut request.height_mm);
    }
    request
}

#[derive(Clone, Debug, PartialEq)]
pub enum TemplatePreview {
    Generated(GeneratedDocumentRequest),
    ImportedPdf {
        page_count: usize,
        /// Key of the template's first-page image in
        /// [`ImportedTemplatePreviews`]: its identifier and source checksum.
        first_page_key: Option<String>,
    },
}

impl TemplatePreview {
    /// Preview for a built-in template id, or blank A3 landscape.
    pub fn for_built_in(id: &str) -> Self {
        Self::Generated(
            crate::template_library::built_in_request(id)
                .unwrap_or_else(GeneratedDocumentRequest::a3_landscape_blank),
        )
    }

    /// How the template's paper is turned; imported PDFs keep their own pages.
    pub fn orientation(&self) -> Option<PaperOrientation> {
        match self {
            Self::Generated(request) => {
                Some(PaperOrientation::of(request.width_mm, request.height_mm))
            }
            Self::ImportedPdf { .. } => None,
        }
    }

    /// The template with its paper turned to `orientation`.
    pub fn oriented(&self, orientation: PaperOrientation) -> Self {
        match self {
            Self::Generated(request) => {
                Self::Generated(oriented_request(request.clone(), orientation))
            }
            Self::ImportedPdf { .. } => self.clone(),
        }
    }

    /// "A3 · Landscape", "500 × 300 mm · Landscape" or "3 pages · Imported PDF".
    pub fn summary(&self) -> String {
        match self {
            Self::Generated(request) => format!(
                "{} · {}",
                self.paper_size(),
                if request.width_mm >= request.height_mm {
                    "Landscape"
                } else {
                    "Portrait"
                }
            ),
            Self::ImportedPdf { page_count, .. } => format!(
                "{page_count} {} · Imported PDF",
                if *page_count == 1 { "page" } else { "pages" }
            ),
        }
    }

    /// The paper the template is cut to: a standard sheet's name in either
    /// orientation ("A3"), otherwise its size ("500 × 300 mm"). Imported PDFs
    /// record no page size, so they share one group.
    pub fn paper_size(&self) -> String {
        match self {
            Self::Generated(request) => match paper_size_mm(request.width_mm, request.height_mm) {
                Some((_, name)) => name.into(),
                None => format!(
                    "{} × {} mm",
                    format_mm(request.width_mm),
                    format_mm(request.height_mm)
                ),
            },
            Self::ImportedPdf { .. } => IMPORTED_PDF_PAPER_SIZE.into(),
        }
    }

    /// Where the template's paper size lists: standard sheets in catalogue
    /// order, then custom sizes, then imported PDFs.
    fn paper_size_order(&self) -> (u8, usize) {
        match self {
            Self::Generated(request) => paper_size_mm(request.width_mm, request.height_mm)
                .map_or((1, 0), |(index, _)| (0, index)),
            Self::ImportedPdf { .. } => (2, 0),
        }
    }

    /// The pattern drawn on the page, if any ("dots", "grid", …).
    pub fn pattern_kind(&self) -> Option<&'static str> {
        match self {
            Self::Generated(request) => request.pattern.as_ref().map(|p| pattern_style(p).0),
            Self::ImportedPdf { .. } => None,
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

/// Debug selector of an imported template's rendered first page.
pub const TEMPLATE_PREVIEW_FIRST_PAGE_ID: &str = "template-preview-first-page";

/// Width in pixels at which imported templates' first pages are drawn.
pub const IMPORTED_FIRST_PAGE_WIDTH: u32 = 640;

/// The cache key of an imported template's first-page image: its stable
/// identifier and the checksum of its managed source.
pub fn imported_first_page_key(template_id: &str, sha256: &str) -> String {
    format!("{template_id}:{sha256}")
}

/// An imported template's first page, as far as it has been drawn.
#[derive(Clone)]
pub enum ImportedFirstPage {
    Loading,
    Ready(Arc<RenderImage>),
    Failed,
}

/// Application-wide first-page images of imported templates, keyed by
/// [`imported_first_page_key`], so every window's picker and library share one
/// render per template source.
#[derive(Default)]
pub struct ImportedTemplatePreviews(HashMap<String, ImportedFirstPage>);

impl Global for ImportedTemplatePreviews {}

impl ImportedTemplatePreviews {
    pub fn get(key: &str, cx: &App) -> Option<ImportedFirstPage> {
        cx.try_global::<Self>()?.0.get(key).cloned()
    }

    pub fn set(key: impl Into<String>, page: ImportedFirstPage, cx: &mut App) {
        cx.default_global::<Self>().0.insert(key.into(), page);
    }
}

/// The paper-size group of templates imported from PDFs.
pub const IMPORTED_PDF_PAPER_SIZE: &str = "Imported PDFs";

/// Template positions grouped under their paper size, for lists organised by
/// size. Standard sheets come first in catalogue order (A3 before A4), then
/// custom sizes in the order they first appear, then imported PDFs; each
/// group keeps its templates' order.
pub fn group_by_paper_size<'a>(
    previews: impl IntoIterator<Item = &'a TemplatePreview>,
) -> Vec<(String, Vec<usize>)> {
    let mut groups: Vec<((u8, usize), String, Vec<usize>)> = Vec::new();
    for (index, preview) in previews.into_iter().enumerate() {
        let size = preview.paper_size();
        match groups.iter_mut().find(|(_, label, _)| *label == size) {
            Some((_, _, members)) => members.push(index),
            None => groups.push((preview.paper_size_order(), size, vec![index])),
        }
    }
    groups.sort_by_key(|(order, _, _)| *order);
    groups
        .into_iter()
        .map(|(_, label, members)| (label, members))
        .collect()
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
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
        .map(rgb)
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
        let (low, high) = corners
            .iter()
            .fold((f64::MAX, f64::MIN), |(low, high), (x, y)| {
                let d = x * nx + y * ny;
                (low.min(d), high.max(d))
            });
        let mut offset = (low / gap).ceil() * gap;
        while offset <= high {
            let (cx, cy) = (nx * offset, ny * offset);
            line(
                marks,
                cx - cos * reach,
                cy - sin * reach,
                cx + cos * reach,
                cy + sin * reach,
            );
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
                    dots.push_str(&format!(
                        "<circle cx=\"{x:.2}\" cy=\"{y:.2}\" r=\"{radius:.2}\"/>"
                    ));
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
    let first_page = match preview {
        TemplatePreview::ImportedPdf {
            first_page_key: Some(key),
            ..
        } => match ImportedTemplatePreviews::get(key, cx) {
            Some(ImportedFirstPage::Ready(image)) => Some(image),
            _ => None,
        },
        _ => None,
    };
    let page = match preview {
        _ if let Some(image) = first_page => {
            let size = image.size(0);
            let (width, height) = fitted_page(
                f64::from(size.width.0.max(1)),
                f64::from(size.height.0.max(1)),
                inner,
            );
            div()
                .id(page_id)
                .debug_selector(move || page_id.into())
                .flex_none()
                .w(px(width))
                .h(px(height))
                .overflow_hidden()
                .border_1()
                .border_color(cx.theme().border)
                .bg(rgb(0xffffff))
                .shadow_xs()
                .child(
                    img(image)
                        .id(TEMPLATE_PREVIEW_FIRST_PAGE_ID)
                        .debug_selector(|| TEMPLATE_PREVIEW_FIRST_PAGE_ID.into())
                        .size_full(),
                )
                .into_any_element()
        }
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
    let patterned =
        matches!(preview, TemplatePreview::Generated(request) if request.pattern.is_some());
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
                        .child(
                            div()
                                .truncate()
                                .text_sm()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(name.into()),
                        )
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
        assert_eq!(dots.summary(), "A3 · Landscape");
        assert_eq!(dots.grid_summary(), "Page grid · 10 mm");
        let blank = TemplatePreview::for_built_in("built-in-blank");
        assert_eq!(blank.grid_summary(), "No page grid");
        let imported = TemplatePreview::ImportedPdf {
            page_count: 1,
            first_page_key: None,
        };
        assert_eq!(imported.summary(), "1 page · Imported PDF");
        assert_eq!(imported.grid_summary(), "Page grid not defined");
        let portrait = GeneratedDocumentRequest {
            width_mm: 210.,
            height_mm: 297.5,
            ..GeneratedDocumentRequest::a3_landscape_blank()
        };
        assert_eq!(
            TemplatePreview::Generated(portrait).summary(),
            "210 × 297.5 mm · Portrait"
        );
    }

    #[test]
    fn preview_artwork_uses_page_millimetres_and_real_spacing() {
        for id in [
            "built-in-dots",
            "built-in-grid",
            "built-in-lined",
            "built-in-isometric",
            "built-in-triangle",
        ] {
            let request = crate::template_library::built_in_request(id).unwrap();
            let svg = template_preview_svg(&request);
            assert!(svg.contains("viewBox=\"0 0 420 297\""), "{id}: {svg}");
            assert!(svg.contains("data-spacing-mm=\"10\""), "{id}");
            assert!(svg.contains("M") || svg.contains("<circle"), "{id}");
        }
        let grid = template_preview_svg(
            &crate::template_library::built_in_request("built-in-grid").unwrap(),
        );
        // 10 mm across a 297 mm page: 29 horizontal and 42 vertical lines plus edges.
        assert!(grid.matches('M').count() >= 29 + 42);
        assert!(
            template_preview_svg(&GeneratedDocumentRequest::a3_landscape_blank())
                .contains("data-pattern=\"blank\"")
        );
    }

    #[test]
    fn pages_fit_the_frame_at_their_aspect_ratio() {
        assert_eq!(
            fitted_page(420., 297., (200., 100.)),
            (100. * 420. / 297., 100.)
        );
        assert_eq!(
            fitted_page(210., 297., (100., 200.)),
            (100., 100. * 297. / 210.)
        );
    }

    fn sized(width_mm: f64, height_mm: f64) -> TemplatePreview {
        TemplatePreview::Generated(GeneratedDocumentRequest {
            width_mm,
            height_mm,
            ..GeneratedDocumentRequest::a3_landscape_blank()
        })
    }

    #[test]
    fn templates_name_their_paper_in_either_orientation() {
        assert_eq!(sized(420., 297.).paper_size(), "A3");
        assert_eq!(sized(297., 420.).summary(), "A3 · Portrait");
        assert_eq!(sized(210., 297.).summary(), "A4 · Portrait");
        assert_eq!(sized(297., 210.).summary(), "A4 · Landscape");
        assert_eq!(sized(500., 300.).summary(), "500 × 300 mm · Landscape");
        assert_eq!(
            TemplatePreview::for_built_in("built-in-grid-a4").summary(),
            "A4 · Portrait"
        );
        assert_eq!(
            TemplatePreview::ImportedPdf {
                page_count: 2,
                first_page_key: None
            }
            .paper_size(),
            IMPORTED_PDF_PAPER_SIZE
        );
    }

    #[test]
    fn templates_group_by_paper_size_in_catalogue_order() {
        let previews = [
            sized(500., 300.),
            TemplatePreview::ImportedPdf {
                page_count: 1,
                first_page_key: None,
            },
            sized(210., 297.),
            sized(420., 297.),
            sized(297., 210.),
            sized(100., 100.),
            sized(297., 420.),
        ];
        assert_eq!(
            group_by_paper_size(&previews),
            vec![
                ("A3".to_owned(), vec![3, 6]),
                ("A4".to_owned(), vec![2, 4]),
                ("500 × 300 mm".to_owned(), vec![0]),
                ("100 × 100 mm".to_owned(), vec![5]),
                (IMPORTED_PDF_PAPER_SIZE.to_owned(), vec![1]),
            ]
        );
    }

    #[test]
    fn turning_a_template_swaps_its_paper_and_says_so() {
        let a3 = TemplatePreview::for_built_in("built-in-dots");
        assert_eq!(a3.orientation(), Some(PaperOrientation::Landscape));
        let turned = a3.oriented(PaperOrientation::Portrait);
        assert_eq!(turned.summary(), "A3 · Portrait");
        assert_eq!(turned.paper_size(), "A3");
        assert_eq!(turned.oriented(PaperOrientation::Landscape), a3);
        let imported = TemplatePreview::ImportedPdf {
            page_count: 1,
            first_page_key: None,
        };
        assert_eq!(imported.orientation(), None);
        assert_eq!(imported.oriented(PaperOrientation::Portrait), imported);
    }
}
