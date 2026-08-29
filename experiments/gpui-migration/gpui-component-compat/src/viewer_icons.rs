use gpui::{
    InteractiveElement as _, IntoElement, ParentElement as _, PathBuilder, Styled as _, Window,
    canvas, div, point, px,
};

pub const FIT_WIDTH_ICON_ID: &str = "viewer-fit-width-icon";
pub const FIT_PAGE_ICON_ID: &str = "viewer-fit-page-icon";
pub const CONTINUOUS_ICON_ID: &str = "viewer-continuous-icon";
pub const SINGLE_PAGE_ICON_ID: &str = "viewer-single-page-icon";
pub const ZOOM_OUT_ICON_ID: &str = "viewer-zoom-out-icon";
pub const ZOOM_IN_ICON_ID: &str = "viewer-zoom-in-icon";

type Segment = ((f32, f32), (f32, f32));

const FIT_WIDTH_SEGMENTS: [Segment; 5] = [
    ((18., 8.), (22., 12.)),
    ((22., 12.), (18., 16.)),
    ((2., 12.), (22., 12.)),
    ((6., 8.), (2., 12.)),
    ((2., 12.), (6., 16.)),
];
const FIT_PAGE_SEGMENTS: [Segment; 12] = [
    ((15., 15.), (21., 21.)),
    ((15., 9.), (21., 3.)),
    ((21., 16.), (21., 21.)),
    ((21., 21.), (16., 21.)),
    ((21., 8.), (21., 3.)),
    ((21., 3.), (16., 3.)),
    ((3., 16.), (3., 21.)),
    ((3., 21.), (8., 21.)),
    ((3., 21.), (9., 15.)),
    ((3., 8.), (3., 3.)),
    ((3., 3.), (8., 3.)),
    ((9., 9.), (3., 3.)),
];
const RECTANGLE_VERTICAL: (f32, f32, f32, f32, f32) = (6., 2., 12., 20., 2.);
const ZOOM_LENS: (f32, f32, f32) = (11., 11., 8.);

#[derive(Clone, Copy)]
enum ViewerIconKind {
    FitWidth,
    FitPage,
    Continuous,
    SinglePage,
    ZoomOut,
    ZoomIn,
}

pub fn fit_width_icon() -> impl IntoElement {
    viewer_icon(FIT_WIDTH_ICON_ID, ViewerIconKind::FitWidth)
}

pub fn fit_page_icon() -> impl IntoElement {
    viewer_icon(FIT_PAGE_ICON_ID, ViewerIconKind::FitPage)
}

pub fn continuous_icon() -> impl IntoElement {
    viewer_icon(CONTINUOUS_ICON_ID, ViewerIconKind::Continuous)
}

pub fn single_page_icon() -> impl IntoElement {
    viewer_icon(SINGLE_PAGE_ICON_ID, ViewerIconKind::SinglePage)
}

pub fn zoom_out_icon() -> impl IntoElement {
    viewer_icon(ZOOM_OUT_ICON_ID, ViewerIconKind::ZoomOut)
}

pub fn zoom_in_icon() -> impl IntoElement {
    viewer_icon(ZOOM_IN_ICON_ID, ViewerIconKind::ZoomIn)
}

fn viewer_icon(id: &'static str, kind: ViewerIconKind) -> impl IntoElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .relative()
        .size(px(16.))
        .flex_none()
        .child(canvas(
            |bounds, _, _| bounds,
            move |bounds, _, window, _| paint_icon(kind, bounds, window),
        ))
}

fn paint_icon(kind: ViewerIconKind, bounds: gpui::Bounds<gpui::Pixels>, window: &mut Window) {
    let color = window.text_style().color;
    let scale = f32::from(bounds.size.width.min(bounds.size.height)) / 24.;
    let origin = bounds.origin;
    let at = |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px(y * scale));
    let mut path = PathBuilder::stroke(px((2. * scale).max(1.)));
    match kind {
        ViewerIconKind::FitWidth => {
            for (from, to) in FIT_WIDTH_SEGMENTS {
                path.move_to(at(from.0, from.1));
                path.line_to(at(to.0, to.1));
            }
        }
        ViewerIconKind::FitPage => {
            for (from, to) in FIT_PAGE_SEGMENTS {
                path.move_to(at(from.0, from.1));
                path.line_to(at(to.0, to.1));
            }
        }
        ViewerIconKind::Continuous | ViewerIconKind::SinglePage => {
            let (x, y, width, height, radius) = RECTANGLE_VERTICAL;
            let radii = point(px(radius * scale), px(radius * scale));
            path.move_to(at(x + radius, y));
            path.line_to(at(x + width - radius, y));
            path.arc_to(radii, px(0.), false, true, at(18., 4.));
            path.line_to(at(x + width, y + height - radius));
            path.arc_to(radii, px(0.), false, true, at(16., 22.));
            path.line_to(at(x + radius, y + height));
            path.arc_to(radii, px(0.), false, true, at(6., 20.));
            path.line_to(at(x, y + radius));
            path.arc_to(radii, px(0.), false, true, at(8., 2.));
            path.close();
            if matches!(kind, ViewerIconKind::Continuous) {
                path.move_to(at(8., 12.));
                path.line_to(at(16., 12.));
            }
        }
        ViewerIconKind::ZoomOut | ViewerIconKind::ZoomIn => {
            // Exact Lucide ZoomIn/ZoomOut 24 px geometry used by the Electron
            // shell. The separate lucide-react 1.8.0 product-icon provenance
            // is checksum-bound in the experiment policy and notice file.
            let (_, _, radius) = ZOOM_LENS;
            let radii = point(px(radius * scale), px(radius * scale));
            path.move_to(at(19., 11.));
            path.arc_to(radii, px(0.), false, true, at(3., 11.));
            path.arc_to(radii, px(0.), false, true, at(19., 11.));
            path.move_to(at(16.65, 16.65));
            path.line_to(at(21., 21.));
            path.move_to(at(8., 11.));
            path.line_to(at(14., 11.));
            if matches!(kind, ViewerIconKind::ZoomIn) {
                path.move_to(at(11., 8.));
                path.line_to(at(11., 14.));
            }
        }
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_quality_icon_geometry_matches_the_electron_lucide_contract() {
        assert_eq!(FIT_WIDTH_SEGMENTS.len(), 5);
        assert_eq!(FIT_WIDTH_SEGMENTS[2], ((2., 12.), (22., 12.)));
        assert_eq!(FIT_PAGE_SEGMENTS.len(), 12);
        assert_eq!(FIT_PAGE_SEGMENTS[0], ((15., 15.), (21., 21.)));
        assert_eq!(FIT_PAGE_SEGMENTS[11], ((9., 9.), (3., 3.)));
        assert_eq!(RECTANGLE_VERTICAL, (6., 2., 12., 20., 2.));
        assert_eq!(ZOOM_LENS, (11., 11., 8.));
    }
}
