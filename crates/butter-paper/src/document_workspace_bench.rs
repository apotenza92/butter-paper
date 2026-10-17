//! Probes for the review driver's thumbnail benchmark (`thumb-bench`): what
//! the thumbnails sidebar and the current page show, read once per frame.

use super::*;
use crate::document_viewer::ViewerRenderQuality;

#[derive(Clone, Debug, Default)]
pub struct ThumbnailBenchProbe {
    pub page_count: usize,
    /// Rows the sidebar shows, from its scroll position.
    pub visible_rows: std::ops::Range<usize>,
    /// Of those, the rows with a thumbnail image.
    pub visible_ready: usize,
    pub loaded_thumbnails: usize,
    /// Thumbnail renders asked for and not yet finished or failed.
    pub pending_thumbnails: usize,
    pub current_page: u32,
    /// The current page's planned viewer tiles, how many are drawn at any
    /// quality, and how many at full quality or better.
    pub current_tiles_planned: usize,
    pub current_tiles_drawn: usize,
    pub current_tiles_sharp: usize,
    /// Of the drawn tiles, how many are at detail quality.
    pub current_tiles_detail: usize,
    pub plan_generation: u64,
    /// Every planned tile on screen (all visible pages): how many are
    /// drawn at any quality, and at each quality.
    pub visible_tiles_drawn: usize,
    pub visible_tiles_by_quality: [usize; 3],
    pub zoom_percent: f32,
    pub viewport: Option<(f32, f32)>,
    /// Whether the current page shows a whole-page image of itself (its
    /// page render, or its thumbnail standing in) under the tiles.
    pub current_base_image: bool,
    /// That image's width in pixels (a thumbnail standing in is narrower).
    pub current_base_width: i32,
    pub list_bounds: Bounds<Pixels>,
    pub row_height: f32,
    pub scroll_top: f32,
    pub max_scroll: f32,
}

impl DocumentWorkspace {
    pub fn thumbnail_bench_probe(&self, cx: &App) -> Option<ThumbnailBenchProbe> {
        let document_id = self.active_document_id?;
        let session = self.session(document_id, cx)?.read(cx);
        if !matches!(session.status, NativeDocumentStatus::Ready) {
            return None;
        }
        let state = self.thumbnail_scroll.0.borrow();
        let bounds = state.base_handle.bounds();
        let page_count = session.page_sizes.len();
        // `item` is the list's own size; `contents` spans every row.
        let row_height = state.last_item_size.map_or(0., |size| {
            f32::from(size.contents.height) / page_count.max(1) as f32
        });
        let scroll_top = -f32::from(state.base_handle.offset().y);
        let max_scroll = f32::from(state.base_handle.max_offset().y);
        let visible_rows = if row_height > 0. {
            let first = (scroll_top / row_height).floor().max(0.) as usize;
            let last = ((scroll_top + f32::from(bounds.size.height)) / row_height).ceil() as usize;
            first.min(page_count)..last.min(page_count)
        } else {
            0..0
        };
        let loaded = session
            .thumbnails
            .iter()
            .map(|thumbnail| thumbnail.page_index as usize)
            .collect::<HashSet<_>>();
        let current_page = session.current_page;
        let planned = session
            .viewer
            .plan_snapshot()
            .map(|plan| {
                plan.tiles
                    .iter()
                    .copied()
                    .filter(|tile| tile.page == current_page as usize)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let sharp = planned
            .iter()
            .filter(|tile| {
                matches!(
                    session.viewer.visible_tile_quality(**tile),
                    Some(ViewerRenderQuality::Full | ViewerRenderQuality::Detail)
                )
            })
            .count();
        let detail = planned
            .iter()
            .filter(|tile| {
                session.viewer.visible_tile_quality(**tile) == Some(ViewerRenderQuality::Detail)
            })
            .count();
        let mut by_quality = [0; 3];
        let mut drawn = 0;
        for tile in session
            .viewer
            .plan_snapshot()
            .map(|plan| plan.tiles.clone())
            .unwrap_or_default()
        {
            let quality = session.viewer.visible_tile_quality(tile);
            if let Some(quality) = quality {
                drawn += 1;
                by_quality[match quality {
                    ViewerRenderQuality::Preview => 0,
                    ViewerRenderQuality::Full => 1,
                    ViewerRenderQuality::Detail => 2,
                }] += 1;
            }
        }
        Some(ThumbnailBenchProbe {
            visible_tiles_drawn: drawn,
            visible_tiles_by_quality: by_quality,
            current_tiles_detail: detail,
            plan_generation: session
                .viewer
                .plan_snapshot()
                .map_or(0, |plan| plan.generation),
            zoom_percent: session.viewer.snapshot().zoom_percent,
            viewport: session.viewport_size,
            page_count,
            visible_ready: visible_rows
                .clone()
                .filter(|row| loaded.contains(row))
                .count(),
            visible_rows,
            loaded_thumbnails: loaded.len(),
            pending_thumbnails: session
                .thumbnail_requests
                .values()
                .filter(|request| request.error.is_none())
                .count(),
            current_page,
            current_tiles_planned: planned.len(),
            current_tiles_drawn: session.viewer.visible_tiles(current_page as usize).len(),
            current_tiles_sharp: sharp,
            current_base_image: session.current_image.is_some()
                && session.current_image_page == Some(current_page),
            current_base_width: session
                .current_image
                .as_ref()
                .filter(|_| session.current_image_page == Some(current_page))
                .map_or(0, |image| image.size(0).width.0),
            list_bounds: bounds,
            row_height,
            scroll_top,
            max_scroll,
        })
    }

    pub fn thumbnail_bench_open_pages_panel(&mut self, cx: &mut Context<Self>) {
        self.pages_panel_open = true;
        cx.notify();
    }

    /// Moves the thumbnails list as a click on its scroll bar does.
    pub fn thumbnail_bench_scroll_to(&self, fraction: f32) {
        let handle = self.thumbnail_scroll.0.borrow().base_handle.clone();
        let max = f32::from(handle.max_offset().y);
        handle.set_offset(point(px(0.), px(-(max * fraction.clamp(0., 1.)))));
    }
}
