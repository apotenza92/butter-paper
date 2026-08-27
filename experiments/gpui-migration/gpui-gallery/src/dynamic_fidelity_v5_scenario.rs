use crate::viewer::{PageLayout, Rect};

pub const COMMAND_ID: &str = "viewer:dynamic-fidelity-scroll";
pub const EXPECTED_TRAJECTORY_SAMPLES: usize = 3_841;
pub const MAX_VISIBLE_PAGE_WINDOW: usize = 8;

/// Holds a state for one complete render/frame boundary before it can be
/// emitted as painted evidence. A newer render may replace the pending state,
/// but it cannot replace evidence already waiting for the frame callback.
pub fn queue_state_for_paint<T>(pending: &mut Option<T>, ready: &mut Option<T>, current: T) {
    if ready.is_none() {
        *ready = pending.take();
    }
    *pending = Some(current);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisiblePageRasterState {
    pub page: usize,
    pub visible_intersection_area_css_px2: f64,
    pub current_raster_ready_area_fraction: f64,
    pub current_raster_device_pixels_per_css_pixel: f64,
    pub page_bounds_window_logical: Rect,
}

pub fn visible_page_raster_states(
    layouts: &[PageLayout],
    scroll_x: f32,
    scroll_y: f32,
    viewport_width: f32,
    viewport_height: f32,
    viewport_window_x: f32,
    viewport_window_y: f32,
    mut current_raster: impl FnMut(usize, f32) -> (bool, f64),
) -> Vec<VisiblePageRasterState> {
    if !scroll_x.is_finite()
        || !scroll_y.is_finite()
        || !viewport_width.is_finite()
        || !viewport_height.is_finite()
        || viewport_width <= 0.0
        || viewport_height <= 0.0
    {
        return Vec::new();
    }
    let viewport_right = scroll_x + viewport_width;
    let viewport_bottom = scroll_y + viewport_height;
    layouts
        .iter()
        .filter_map(|layout| {
            let page = layout.logical_rect;
            let left = page.x.max(scroll_x);
            let top = page.y.max(scroll_y);
            let right = (page.x + page.width).min(viewport_right);
            let bottom = (page.y + page.height).min(viewport_bottom);
            let width = (right - left).max(0.0);
            let height = (bottom - top).max(0.0);
            if width <= 0.0 || height <= 0.0 {
                return None;
            }
            let (ready, density) = current_raster(layout.page, layout.logical_rect.width);
            Some(VisiblePageRasterState {
                page: layout.page,
                visible_intersection_area_css_px2: f64::from(width * height),
                current_raster_ready_area_fraction: if ready { 1.0 } else { 0.0 },
                current_raster_device_pixels_per_css_pixel: if ready {
                    density.max(0.0)
                } else {
                    0.0
                },
                page_bounds_window_logical: Rect::new(
                    viewport_window_x + page.x - scroll_x,
                    viewport_window_y + page.y - scroll_y,
                    page.width,
                    page.height,
                ),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(page: usize, y: f32) -> PageLayout {
        PageLayout {
            page,
            logical_rect: Rect::new(12.0, y, 100.0, 200.0),
            device_width: 200,
            device_height: 400,
        }
    }

    #[test]
    fn reports_only_positive_visible_intersections_and_window_bounds() {
        let states = visible_page_raster_states(
            &[layout(1, 0.0), layout(2, 224.0), layout(3, 448.0)],
            0.0,
            150.0,
            140.0,
            298.0,
            348.0,
            160.0,
            |page, _| (page == 2, 4.0),
        );
        assert_eq!(states.len(), 2);
        assert_eq!(states[0].page, 1);
        assert_eq!(states[0].visible_intersection_area_css_px2, 5_000.0);
        assert_eq!(states[0].current_raster_ready_area_fraction, 0.0);
        assert_eq!(states[0].current_raster_device_pixels_per_css_pixel, 0.0);
        assert_eq!(states[1].page_bounds_window_logical.y, 234.0);
        assert_eq!(states[1].current_raster_ready_area_fraction, 1.0);
        assert_eq!(states[1].current_raster_device_pixels_per_css_pixel, 4.0);
    }

    #[test]
    fn invalid_viewport_fails_closed_without_receipts() {
        assert!(
            visible_page_raster_states(&[layout(1, 0.0)], 0.0, 0.0, 0.0, 10.0, 0.0, 0.0, |_, _| (
                true, 1.0
            ))
            .is_empty()
        );
    }

    #[test]
    fn state_is_not_receipted_until_a_later_frame_boundary() {
        let mut pending = None;
        let mut ready = None;
        queue_state_for_paint(&mut pending, &mut ready, "render-1");
        assert_eq!(ready, None);
        queue_state_for_paint(&mut pending, &mut ready, "render-2");
        assert_eq!(ready.take(), Some("render-1"));
        queue_state_for_paint(&mut pending, &mut ready, "render-3");
        assert_eq!(ready.take(), Some("render-2"));
    }
}
