//! Keeps the document point under the pointer fixed while zooming.
//!
//! Page gaps and centring do not scale with zoom, so the anchor is a position
//! within a page rather than a scaled scroll offset. It is resolved against
//! the page layout of the new zoom level before that layout is drawn.

use crate::viewer::PageLayout;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomAnchor {
    pub page: usize,
    /// Position within the page as a fraction of its width and height.
    pub page_fraction: (f32, f32),
    /// Pointer position relative to the viewport's top-left corner.
    pub viewport_point: (f32, f32),
}

/// Anchor the zoom at the page under the pointer, or the nearest page.
pub fn zoom_anchor_at(
    layouts: &[PageLayout],
    scroll: (f32, f32),
    viewport_point: (f32, f32),
) -> Option<ZoomAnchor> {
    let content = (scroll.0 + viewport_point.0, scroll.1 + viewport_point.1);
    let distance = |layout: &PageLayout| {
        let rect = layout.logical_rect;
        let dx = (rect.x - content.0).max(content.0 - (rect.x + rect.width)).max(0.);
        let dy = (rect.y - content.1).max(content.1 - (rect.y + rect.height)).max(0.);
        dx * dx + dy * dy
    };
    let layout = layouts
        .iter()
        .filter(|layout| layout.logical_rect.width > 0. && layout.logical_rect.height > 0.)
        .min_by(|a, b| distance(a).total_cmp(&distance(b)))?;
    let rect = layout.logical_rect;
    // Not clamped to the page: a pointer over the blank space beside it keeps
    // its distance from the page while zooming.
    Some(ZoomAnchor {
        page: layout.page,
        page_fraction: (
            (content.0 - rect.x) / rect.width,
            (content.1 - rect.y) / rect.height,
        ),
        viewport_point,
    })
}

/// Scroll offset that returns the anchored page point to the pointer, within
/// the scroll range widened by the viewer's pan margin on every side.
pub fn anchored_scroll(
    anchor: ZoomAnchor,
    layouts: &[PageLayout],
    content_size: (f32, f32),
    viewport_size: (f32, f32),
    pan_margin: (f32, f32),
) -> Option<(f32, f32)> {
    let rect = layouts.iter().find(|layout| layout.page == anchor.page)?.logical_rect;
    let point = (
        rect.x + anchor.page_fraction.0 * rect.width,
        rect.y + anchor.page_fraction.1 * rect.height,
    );
    let max = (
        (content_size.0 - viewport_size.0).max(0.),
        (content_size.1 - viewport_size.1).max(0.),
    );
    Some((
        (point.0 - anchor.viewport_point.0).clamp(-pan_margin.0, max.0 + pan_margin.0),
        (point.1 - anchor.viewport_point.1).clamp(-pan_margin.1, max.1 + pan_margin.1),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::Rect;

    fn page(page: usize, x: f32, y: f32, width: f32, height: f32) -> PageLayout {
        PageLayout {
            page,
            logical_rect: Rect { x, y, width, height },
            device_width: width as usize,
            device_height: height as usize,
            column_index: 0,
            row_index: page,
        }
    }

    #[test]
    fn keeps_the_pointed_page_point_under_the_pointer_across_zoom() {
        // 100% zoom: two 600x800 pages centred in a 1000px-wide canvas with a 20px gap.
        let before = [page(0, 200., 20., 600., 800.), page(1, 200., 840., 600., 800.)];
        let anchor = zoom_anchor_at(&before, (0., 500.), (350., 450.)).unwrap();
        assert_eq!(anchor.page, 1);
        assert_eq!(anchor.page_fraction, (0.25, 0.1375));

        // 200%: pages double but gaps and centring do not.
        let after = [page(0, 20., 20., 1200., 1600.), page(1, 20., 1640., 1200., 1600.)];
        let scroll = anchored_scroll(anchor, &after, (1240., 3260.), (1000., 700.), (0., 0.)).unwrap();
        assert_eq!(scroll, (0., 1410.));
        // The page point is back under the pointer.
        assert_eq!(scroll.1 + 450., 1640. + 0.1375 * 1600.);
    }

    #[test]
    fn anchors_to_the_nearest_page_from_a_gap_and_clamps_to_the_pan_range() {
        let layouts = [page(0, 0., 0., 100., 100.), page(1, 0., 120., 100., 100.)];
        let anchor = zoom_anchor_at(&layouts, (0., 0.), (50., 225.)).unwrap();
        assert_eq!(anchor.page, 1);
        assert_eq!(anchor.page_fraction, (0.5, 1.05));
        let scroll = anchored_scroll(anchor, &layouts, (100., 220.), (100., 100.), (0., 0.)).unwrap();
        assert_eq!(scroll, (0., 0.));
        assert!(anchored_scroll(ZoomAnchor { page: 9, ..anchor }, &layouts, (1., 1.), (1., 1.), (0., 0.)).is_none());
        assert!(zoom_anchor_at(&[], (0., 0.), (0., 0.)).is_none());
    }

    #[test]
    fn zooming_over_blank_space_keeps_the_page_where_it_was_panned() {
        // A 100pt page panned 300px right of the viewport's left edge.
        let before = [page(0, 0., 0., 100., 100.)];
        let anchor = zoom_anchor_at(&before, (-300., 0.), (50., 50.)).unwrap();
        assert_eq!(anchor.page_fraction, (-2.5, 0.5));
        // Doubling the page keeps the pointed blank point under the pointer,
        // which needs a scroll left of the page.
        let after = [page(0, 0., 0., 200., 200.)];
        let scroll = anchored_scroll(anchor, &after, (200., 200.), (800., 600.), (752., 552.)).unwrap();
        assert_eq!(scroll, (-550., 50.));
    }
}
