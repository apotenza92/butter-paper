use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
};

use butter_paper_gpui_gallery::viewer::{
    CachePolicy, PageGeometry, PageLayout, Rect, RenderInput, RenderPlanner, RenderSource,
    TileCache, TileRequest, ViewportGeometry,
};
use gpui::{RenderImage, ScrollHandle, point, px};

use crate::page_view_control::PageViewMode;

const PAGE_GAP: f32 = 24.;
const MAX_ACTIVE_TILE_JOBS: usize = 2;
const FIT_ZOOM_STEP: f32 = 0.02;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewerFitPreset {
    Width,
    Page,
}

pub fn resolve_fit_zoom_percent(
    preset: ViewerFitPreset,
    viewport_width: f32,
    viewport_height: f32,
    page_width: f32,
    page_height: f32,
) -> f32 {
    if !viewport_width.is_finite()
        || !viewport_height.is_finite()
        || !page_width.is_finite()
        || !page_height.is_finite()
    {
        return 6.25;
    }
    let fit_width = viewport_width / (page_width.max(1.) + PAGE_GAP * 2.);
    let raw = match preset {
        ViewerFitPreset::Width => fit_width,
        ViewerFitPreset::Page => {
            fit_width.min(viewport_height / (page_height.max(1.) + PAGE_GAP * 2.))
        }
    };
    let quantized = (raw / FIT_ZOOM_STEP).floor() * FIT_ZOOM_STEP;
    ((quantized * 1_000.).round() / 1_000.).clamp(0.0625, 64.) * 100.
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ViewportKey {
    width: i32,
    height: i32,
    scroll_x: i32,
    scroll_y: i32,
    device_scale_millis: i32,
    current_page: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct ViewerPlanSnapshot {
    pub generation: u64,
    pub page_layouts: Vec<PageLayout>,
    pub visible_pages: Vec<usize>,
    pub current_page: Option<usize>,
    pub total_height: f32,
    pub tiles: Vec<TileRequest>,
    pub requested_bytes: usize,
    pub cache_max_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DocumentViewerSnapshot {
    pub mode: PageViewMode,
    pub zoom_percent: f32,
    pub generation: u64,
    pub queued_tiles: usize,
    pub active_tiles: usize,
    pub cache_entries: usize,
    pub cache_bytes: usize,
    pub cache_max_bytes: usize,
    pub rejected_stale_tiles: usize,
}

pub(crate) struct DocumentViewerState {
    mode: PageViewMode,
    zoom_percent: f32,
    device_scale: f32,
    raster_revision: u64,
    planner: RenderPlanner,
    plan: Option<ViewerPlanSnapshot>,
    queue: VecDeque<TileRequest>,
    pending: HashSet<TileRequest>,
    active: HashSet<TileRequest>,
    cache: TileCache<Arc<RenderImage>>,
    scroll_handle: ScrollHandle,
    rejected_stale_tiles: usize,
    error: Option<String>,
    viewport_key: Option<ViewportKey>,
}

impl Default for DocumentViewerState {
    fn default() -> Self {
        Self {
            mode: PageViewMode::Continuous,
            zoom_percent: 100.,
            device_scale: 1.,
            raster_revision: 1,
            planner: RenderPlanner::default(),
            plan: None,
            queue: VecDeque::new(),
            pending: HashSet::new(),
            active: HashSet::new(),
            cache: TileCache::new(CachePolicy::default()),
            scroll_handle: ScrollHandle::new(),
            rejected_stale_tiles: 0,
            error: None,
            viewport_key: None,
        }
    }
}

impl DocumentViewerState {
    pub fn configure(&mut self, mode: PageViewMode, zoom_percent: f32) {
        let zoom_percent = if zoom_percent.is_finite() {
            zoom_percent.clamp(6.25, 6_400.)
        } else {
            100.
        };
        if self.mode == mode && (self.zoom_percent - zoom_percent).abs() < 0.001 {
            return;
        }
        self.mode = mode;
        self.zoom_percent = zoom_percent;
        self.cancel_plan();
    }

    pub fn invalidate_raster(&mut self) {
        self.raster_revision = self.raster_revision.saturating_add(1).max(1);
        self.cancel_plan();
        self.cache.clear();
    }

    pub fn invalidate_layout(&mut self) {
        self.cancel_plan();
    }

    pub fn close(&mut self) {
        self.invalidate_raster();
        self.active.clear();
    }

    fn cancel_plan(&mut self) {
        self.planner.cancel();
        self.plan = None;
        self.queue.clear();
        self.pending.clear();
        self.error = None;
        self.viewport_key = None;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn plan(
        &mut self,
        document_id: u64,
        page_sizes: &[(f32, f32)],
        page_rotation_quarter_turns: &[u8],
        current_page: usize,
        viewport_width: f32,
        viewport_height: f32,
        scroll_x: f32,
        scroll_y: f32,
        device_scale: f32,
    ) -> Result<ViewerPlanSnapshot, String> {
        if !viewport_width.is_finite()
            || !viewport_height.is_finite()
            || viewport_width <= 0.
            || viewport_height <= 0.
        {
            return Err("viewer viewport must have finite positive dimensions".into());
        }
        if page_sizes.is_empty()
            || current_page >= page_sizes.len()
            || page_rotation_quarter_turns.len() != page_sizes.len()
        {
            return Err("viewer document geometry is unavailable".into());
        }
        self.device_scale = if device_scale.is_finite() && device_scale > 0. {
            device_scale
        } else {
            1.
        };
        let viewport_key = ViewportKey {
            width: viewport_width.ceil() as i32,
            height: viewport_height.ceil() as i32,
            scroll_x: scroll_x.round() as i32,
            scroll_y: scroll_y.round() as i32,
            device_scale_millis: (self.device_scale * 1_000.).round() as i32,
            current_page,
        };
        if self.viewport_key == Some(viewport_key)
            && let Some(plan) = self.plan.clone()
        {
            return Ok(plan);
        }
        let pages = match self.mode {
            PageViewMode::Continuous => page_sizes
                .iter()
                .enumerate()
                .map(|(page, (width, height))| PageGeometry::new(page, *width, *height))
                .collect::<Vec<_>>(),
            PageViewMode::SinglePage => {
                let (width, height) = page_sizes[current_page];
                vec![PageGeometry::new(current_page, width, height)]
            }
        };
        let mut plan = self.planner.plan(RenderInput {
            source: RenderSource {
                document_id,
                revision: self.raster_revision,
            },
            pages: &pages,
            zoom_percent: self.zoom_percent,
            device_scale: self.device_scale,
            page_gap: PAGE_GAP * (self.zoom_percent / 100.),
            viewport: ViewportGeometry {
                width: viewport_width,
                height: viewport_height,
                scroll_x: scroll_x.max(0.),
                scroll_y: scroll_y.max(0.),
                visible_rect: Rect::new(0., 0., viewport_width, viewport_height),
            },
        });
        for request in &mut plan.tiles {
            request.rotation_quarter_turns = page_rotation_quarter_turns[request.page] % 4;
        }
        let snapshot = ViewerPlanSnapshot {
            generation: plan.generation,
            page_layouts: plan.page_layouts,
            visible_pages: plan.visible_pages,
            current_page: plan.current_page,
            total_height: plan.total_height,
            tiles: plan.tiles,
            requested_bytes: plan.requested_bytes,
            cache_max_bytes: plan.cache.max_bytes,
        };
        self.queue.clear();
        self.pending.clear();
        for request in snapshot.tiles.iter().copied() {
            if !self.cache.contains(request) && !self.active.contains(&request) {
                self.pending.insert(request);
                self.queue.push_back(request);
            }
        }
        self.plan = Some(snapshot.clone());
        self.viewport_key = Some(viewport_key);
        self.error = None;
        Ok(snapshot)
    }

    pub fn needs_plan(
        &self,
        viewport_width: f32,
        viewport_height: f32,
        device_scale: f32,
        current_page: usize,
    ) -> bool {
        let offset = self.scroll_handle.offset();
        let key = ViewportKey {
            width: viewport_width.ceil() as i32,
            height: viewport_height.ceil() as i32,
            scroll_x: (-f32::from(offset.x)).max(0.).round() as i32,
            scroll_y: (-f32::from(offset.y)).max(0.).round() as i32,
            device_scale_millis: (device_scale * 1_000.).round() as i32,
            current_page,
        };
        self.viewport_key != Some(key)
    }

    pub fn claim_jobs(&mut self) -> Vec<TileRequest> {
        let mut claimed = Vec::new();
        while self.active.len() < MAX_ACTIVE_TILE_JOBS {
            let Some(request) = self.queue.pop_front() else {
                break;
            };
            if !self.pending.contains(&request)
                || !self.planner.accepts(request.generation)
                || self.active.contains(&request)
            {
                continue;
            }
            self.active.insert(request);
            claimed.push(request);
        }
        claimed
    }

    pub fn finish(
        &mut self,
        request: TileRequest,
        result: Result<(Arc<RenderImage>, usize), String>,
    ) -> bool {
        self.active.remove(&request);
        self.pending.remove(&request);
        if !self.planner.accepts(request.generation)
            || request.source.revision != self.raster_revision
        {
            self.rejected_stale_tiles = self.rejected_stale_tiles.saturating_add(1);
            return false;
        }
        match result {
            Ok((image, bytes)) => {
                if !self.cache.insert(request, image, bytes) {
                    self.error = Some("viewer tile exceeds the cache limit".into());
                    return false;
                }
                true
            }
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }

    pub fn visible_tiles(&self, page: usize) -> Vec<(TileRequest, Arc<RenderImage>)> {
        self.plan
            .as_ref()
            .into_iter()
            .flat_map(|plan| plan.tiles.iter().copied())
            .filter(|request| request.page == page)
            .filter_map(|request| {
                self.cache
                    .peek(request)
                    .cloned()
                    .map(|image| (request, image))
            })
            .collect()
    }

    pub fn accepts(&self, generation: u64) -> bool {
        self.planner.accepts(generation)
    }

    pub fn touch_cached(&mut self, request: TileRequest) -> bool {
        self.cache.get(request).is_some()
    }

    pub fn insert_direct(
        &mut self,
        request: TileRequest,
        image: Arc<RenderImage>,
        bytes: usize,
    ) -> bool {
        self.queue.retain(|candidate| *candidate != request);
        self.pending.remove(&request);
        self.active.remove(&request);
        self.cache.insert(request, image, bytes)
    }

    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    pub fn cache_bytes(&self) -> usize {
        self.cache.bytes()
    }

    pub fn plan_snapshot(&self) -> Option<&ViewerPlanSnapshot> {
        self.plan.as_ref()
    }

    pub fn scroll_handle(&self) -> ScrollHandle {
        self.scroll_handle.clone()
    }

    pub fn set_scroll(&self, scroll_x: f32, scroll_y: f32) {
        self.scroll_handle
            .set_offset(point(px(-scroll_x.max(0.)), px(-scroll_y.max(0.))));
    }

    pub fn snapshot(&self) -> DocumentViewerSnapshot {
        DocumentViewerSnapshot {
            mode: self.mode,
            zoom_percent: self.zoom_percent,
            generation: self.plan.as_ref().map_or(0, |plan| plan.generation),
            queued_tiles: self.queue.len(),
            active_tiles: self.active.len(),
            cache_entries: self.cache.len(),
            cache_bytes: self.cache.bytes(),
            cache_max_bytes: CachePolicy::default().max_bytes,
            rejected_stale_tiles: self.rejected_stale_tiles,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_zoom_matches_the_electron_gap_quantization_contract() {
        assert_eq!(
            resolve_fit_zoom_percent(ViewerFitPreset::Width, 1_000., 560., 612., 792.),
            150.
        );
        assert_eq!(
            resolve_fit_zoom_percent(ViewerFitPreset::Page, 1_000., 560., 612., 792.),
            66.
        );
        assert_eq!(
            resolve_fit_zoom_percent(ViewerFitPreset::Page, 0., 0., 612., 792.),
            6.25
        );
    }

    #[test]
    fn queue_is_bounded_and_a_changed_plan_rejects_old_completions() {
        let mut viewer = DocumentViewerState::default();
        viewer.configure(PageViewMode::Continuous, 1_600.);
        let first = viewer
            .plan(
                7,
                &[(612., 792.), (612., 792.)],
                &[0, 0],
                0,
                800.,
                600.,
                0.,
                0.,
                2.,
            )
            .unwrap();
        assert!(!first.tiles.is_empty());
        let active = viewer.claim_jobs();
        assert!(!active.is_empty());
        assert!(active.len() <= MAX_ACTIVE_TILE_JOBS);

        viewer.configure(PageViewMode::SinglePage, 800.);
        let changed = viewer
            .plan(
                7,
                &[(612., 792.), (612., 792.)],
                &[0, 0],
                0,
                800.,
                600.,
                0.,
                0.,
                2.,
            )
            .unwrap();
        assert_ne!(changed.generation, first.generation);
        assert!(!viewer.finish(active[0], Err("old render must be ignored".into())));
        assert_eq!(viewer.snapshot().rejected_stale_tiles, 1);
        assert!(viewer.snapshot().active_tiles <= MAX_ACTIVE_TILE_JOBS);
    }

    #[test]
    fn rotated_page_tiles_have_distinct_identity_and_stale_crops_are_rejected() {
        let mut viewer = DocumentViewerState::default();
        let first = viewer
            .plan(9, &[(612., 792.)], &[0], 0, 800., 600., 0., 0., 1.)
            .unwrap();
        let old = first.tiles[0];
        assert_eq!(old.rotation_quarter_turns, 0);
        viewer.invalidate_raster();
        let rotated = viewer
            .plan(9, &[(792., 612.)], &[1], 0, 800., 600., 0., 0., 1.)
            .unwrap();
        assert_eq!(rotated.tiles[0].rotation_quarter_turns, 1);
        assert_ne!(old, rotated.tiles[0]);
        assert!(!viewer.finish(old, Err("old orientation".into())));
    }
}
