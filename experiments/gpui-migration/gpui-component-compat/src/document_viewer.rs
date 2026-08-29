use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use butter_paper_gpui_gallery::viewer::{
    CachePolicy, CadOrganisation, PageGeometry, PageLayout, Rect, RenderInput, RenderLayout,
    RenderPlanner, RenderSource, TileCache, TileRequest, ViewportGeometry,
};
use gpui::{RenderImage, ScrollHandle, point, px};

#[cfg(test)]
use crate::adaptive_performance::{AdaptiveViewerPerformance, ViewerRenderDiagnostics};
use crate::page_view_control::PageViewMode;

const PAGE_GAP: f32 = 24.;
const MAX_ACTIVE_TILE_JOBS: usize = 2;
const FIT_ZOOM_STEP: f32 = 0.02;
const PREVIEW_CACHE_BYTES: usize = 32 * 1024 * 1024;
const FULL_CACHE_BYTES: usize = 160 * 1024 * 1024;
const DETAIL_CACHE_BYTES: usize = 64 * 1024 * 1024;
const PREVIEW_RASTER_RATIO: f32 = 0.5;
const MIN_PREVIEW_ZOOM_TENTHS: u32 = 350;
const DETAIL_RASTER_RATIO: f32 = 1.5;
const MAX_DETAIL_ZOOM_TENTHS: u32 = 24_000;
const VIEWER_MOTION_SETTLE_MS: u64 = 180;
const VIEWER_DETAIL_PROMOTION_MS: u64 = 1_200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewerFitPreset {
    Width,
    Page,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ViewerRenderQuality {
    Preview,
    Full,
    Detail,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ViewerTileJob {
    pub authority: TileRequest,
    pub raster: TileRequest,
    pub quality: ViewerRenderQuality,
}

impl From<TileRequest> for ViewerTileJob {
    fn from(request: TileRequest) -> Self {
        Self {
            authority: request,
            raster: request,
            quality: ViewerRenderQuality::Full,
        }
    }
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
    pub total_width: f32,
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
    pub preview_tiles: usize,
    pub full_tiles: usize,
    pub detail_tiles: usize,
    pub current_quality: Option<ViewerRenderQuality>,
    pub render_failed_pages: usize,
    pub adaptive_level: usize,
}

pub fn viewer_quality_promotion_delay_ms(
    adaptive_level: usize,
    is_target_page: bool,
    viewport_in_motion: bool,
    defer_high_quality_during_motion: bool,
    immediate_thumbnail_target: bool,
) -> Option<u64> {
    if immediate_thumbnail_target {
        return Some(0);
    }
    if viewport_in_motion && defer_high_quality_during_motion {
        return None;
    }
    let level = adaptive_level.min(3);
    let delays = match (is_target_page, viewport_in_motion) {
        (true, false) => [0, 8, 24, 48],
        (true, true) => [16, 32, 64, 96],
        (false, false) => [24, 40, 64, 96],
        (false, true) => [48, 72, 112, 160],
    };
    Some(delays[level])
}

pub fn viewer_motion_is_rapid(previously_rapid: bool, speed_px_per_ms: f32) -> bool {
    let speed = if speed_px_per_ms.is_finite() {
        speed_px_per_ms.max(0.)
    } else {
        0.
    };
    if previously_rapid {
        speed >= 0.75
    } else {
        speed >= 1.5
    }
}

pub(crate) struct DocumentViewerState {
    mode: PageViewMode,
    zoom_percent: f32,
    device_scale: f32,
    raster_revision: u64,
    planner: RenderPlanner,
    plan: Option<ViewerPlanSnapshot>,
    queue: VecDeque<ViewerTileJob>,
    pending: HashSet<ViewerTileJob>,
    active: HashSet<ViewerTileJob>,
    preview_cache: TileCache<Arc<RenderImage>>,
    cache: TileCache<Arc<RenderImage>>,
    detail_cache: TileCache<Arc<RenderImage>>,
    scroll_handle: ScrollHandle,
    rejected_stale_tiles: usize,
    error: Option<String>,
    page_errors: HashMap<usize, String>,
    pending_promotions: HashMap<ViewerTileJob, Instant>,
    scheduler_revision: u64,
    adaptive_level: usize,
    viewport_in_motion: bool,
    rapid_viewport_motion: bool,
    motion_settle_deadline: Option<Instant>,
    last_motion_sample: Option<(f32, f32, Instant)>,
    thumbnail_navigation_target: Option<usize>,
    viewport_key: Option<ViewportKey>,
    cad_layout: Option<(CadOrganisation, usize)>,
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
            preview_cache: TileCache::new(cache_policy(PREVIEW_CACHE_BYTES)),
            cache: TileCache::new(cache_policy(FULL_CACHE_BYTES)),
            detail_cache: TileCache::new(cache_policy(DETAIL_CACHE_BYTES)),
            scroll_handle: ScrollHandle::new(),
            rejected_stale_tiles: 0,
            error: None,
            page_errors: HashMap::new(),
            pending_promotions: HashMap::new(),
            scheduler_revision: 1,
            adaptive_level: 0,
            viewport_in_motion: false,
            rapid_viewport_motion: false,
            motion_settle_deadline: None,
            last_motion_sample: None,
            thumbnail_navigation_target: None,
            viewport_key: None,
            cad_layout: None,
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

    pub fn configure_cad(&mut self, layout: Option<(CadOrganisation, usize)>) {
        let layout = layout.map(|(organisation, count)| (organisation, count.clamp(1, 100)));
        if self.cad_layout == layout {
            return;
        }
        self.cad_layout = layout;
        self.cancel_plan();
    }

    pub fn invalidate_raster(&mut self) {
        self.raster_revision = self.raster_revision.saturating_add(1).max(1);
        self.cancel_plan();
        self.cache.clear();
        self.preview_cache.clear();
        self.detail_cache.clear();
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
        self.page_errors.clear();
        self.pending_promotions.clear();
        self.scheduler_revision = self.scheduler_revision.saturating_add(1).max(1);
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
        self.plan_at(
            document_id,
            page_sizes,
            page_rotation_quarter_turns,
            current_page,
            viewport_width,
            viewport_height,
            scroll_x,
            scroll_y,
            device_scale,
            Instant::now(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn plan_at(
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
        now: Instant,
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
        let input = RenderInput {
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
        };
        let mut plan = if let Some((organisation, pages_per_lane)) = self.cad_layout {
            self.planner.plan_with_layout(
                input,
                RenderLayout::Cad {
                    organisation,
                    pages_per_lane,
                },
            )
        } else {
            self.planner.plan(input)
        };
        for request in &mut plan.tiles {
            request.rotation_quarter_turns = page_rotation_quarter_turns[request.page] % 4;
        }
        let snapshot = ViewerPlanSnapshot {
            generation: plan.generation,
            page_layouts: plan.page_layouts,
            visible_pages: plan.visible_pages,
            current_page: plan.current_page,
            total_height: plan.total_height,
            total_width: plan.total_width,
            tiles: plan.tiles,
            requested_bytes: plan.requested_bytes,
            cache_max_bytes: plan.cache.max_bytes,
        };
        self.queue.clear();
        self.pending.clear();
        self.pending_promotions.clear();
        self.plan = Some(snapshot.clone());
        for request in snapshot.tiles.iter().copied() {
            let detail = tile_job(request, ViewerRenderQuality::Detail);
            let full = tile_job(request, ViewerRenderQuality::Full);
            let preview = tile_job(request, ViewerRenderQuality::Preview);
            if request.page == current_page && self.detail_cache.contains(detail.raster) {
                continue;
            }
            if self.cache.contains(full.raster) {
                self.schedule_promotion(detail, now);
                continue;
            }
            if self.preview_cache.contains(preview.raster) {
                self.schedule_promotion(full, now);
            } else if preview.raster == full.raster {
                self.enqueue(full, request.page == current_page);
            } else {
                self.enqueue(preview, request.page == current_page);
            }
        }
        self.viewport_key = Some(viewport_key);
        self.error = None;
        Ok(snapshot)
    }

    fn schedule_promotion(&mut self, job: ViewerTileJob, now: Instant) {
        let current_page = self.plan.as_ref().and_then(|plan| plan.current_page);
        let is_target = current_page == Some(job.authority.page);
        let immediate_thumbnail_target =
            self.thumbnail_navigation_target == Some(job.authority.page);
        let delay = match job.quality {
            ViewerRenderQuality::Preview => Some(0),
            ViewerRenderQuality::Full => viewer_quality_promotion_delay_ms(
                self.adaptive_level,
                is_target,
                self.viewport_in_motion,
                self.rapid_viewport_motion || !is_target,
                immediate_thumbnail_target,
            ),
            ViewerRenderQuality::Detail => {
                (is_target && !self.viewport_in_motion).then_some(VIEWER_DETAIL_PROMOTION_MS)
            }
        };
        if let Some(delay) = delay {
            self.pending_promotions
                .entry(job)
                .or_insert(now + Duration::from_millis(delay));
        }
    }

    fn rebuild_promotions(&mut self, now: Instant) {
        self.pending_promotions.clear();
        let requests = self
            .plan
            .as_ref()
            .map(|plan| plan.tiles.clone())
            .unwrap_or_default();
        for request in requests {
            let preview = tile_job(request, ViewerRenderQuality::Preview);
            let full = tile_job(request, ViewerRenderQuality::Full);
            let detail = tile_job(request, ViewerRenderQuality::Detail);
            if self.cache.contains(full.raster) {
                if detail.raster != full.raster && !self.detail_cache.contains(detail.raster) {
                    self.schedule_promotion(detail, now);
                }
            } else if self.preview_cache.contains(preview.raster) {
                self.schedule_promotion(full, now);
            }
        }
    }

    pub fn observe_motion(&mut self, scroll_x: f32, scroll_y: f32, now: Instant) {
        let Some((previous_x, previous_y, previous_at)) = self.last_motion_sample else {
            self.last_motion_sample = Some((scroll_x, scroll_y, now));
            return;
        };
        self.last_motion_sample = Some((scroll_x, scroll_y, now));
        let distance = (scroll_x - previous_x).hypot(scroll_y - previous_y);
        if distance <= f32::EPSILON {
            return;
        }
        let elapsed_ms = now.saturating_duration_since(previous_at).as_secs_f32() * 1_000.;
        let speed = if elapsed_ms > 0. {
            distance / elapsed_ms
        } else {
            f32::INFINITY
        };
        self.viewport_in_motion = true;
        self.rapid_viewport_motion = viewer_motion_is_rapid(self.rapid_viewport_motion, speed);
        self.motion_settle_deadline = Some(now + Duration::from_millis(VIEWER_MOTION_SETTLE_MS));
        self.scheduler_revision = self.scheduler_revision.saturating_add(1).max(1);
        self.rebuild_promotions(now);
    }

    pub fn mark_thumbnail_navigation_target(&mut self, page: usize, now: Instant) {
        self.thumbnail_navigation_target = Some(page);
        self.scheduler_revision = self.scheduler_revision.saturating_add(1).max(1);
        self.rebuild_promotions(now);
    }

    pub fn set_adaptive_level(&mut self, level: usize, now: Instant) -> bool {
        let level = level.min(3);
        if self.adaptive_level == level {
            return false;
        }
        self.adaptive_level = level;
        self.scheduler_revision = self.scheduler_revision.saturating_add(1).max(1);
        self.rebuild_promotions(now);
        true
    }

    pub fn release_due_promotions(&mut self, now: Instant) -> usize {
        if self
            .motion_settle_deadline
            .is_some_and(|deadline| deadline <= now)
        {
            self.viewport_in_motion = false;
            self.rapid_viewport_motion = false;
            self.motion_settle_deadline = None;
            self.scheduler_revision = self.scheduler_revision.saturating_add(1).max(1);
            self.rebuild_promotions(now);
        }
        let due = self
            .pending_promotions
            .iter()
            .filter_map(|(job, deadline)| (*deadline <= now).then_some(*job))
            .collect::<Vec<_>>();
        for job in &due {
            self.pending_promotions.remove(job);
            self.enqueue(
                *job,
                job.authority.page
                    == self
                        .plan
                        .as_ref()
                        .and_then(|p| p.current_page)
                        .unwrap_or(usize::MAX),
            );
        }
        if !due.is_empty() {
            self.thumbnail_navigation_target = None;
        }
        due.len()
    }

    pub fn next_promotion_deadline(&self) -> Option<Instant> {
        self.pending_promotions
            .values()
            .copied()
            .chain(self.motion_settle_deadline)
            .min()
    }

    pub fn scheduler_revision(&self) -> u64 {
        self.scheduler_revision
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

    fn enqueue(&mut self, job: ViewerTileJob, prioritize: bool) {
        if self.pending.contains(&job) || self.active.contains(&job) {
            return;
        }
        self.pending.insert(job);
        if prioritize {
            self.queue.push_front(job);
        } else {
            self.queue.push_back(job);
        }
    }

    pub fn claim_jobs(&mut self) -> Vec<ViewerTileJob> {
        let mut claimed = Vec::new();
        let max_active = if self.adaptive_level >= 3 {
            1
        } else {
            MAX_ACTIVE_TILE_JOBS
        };
        while self.active.len() < max_active {
            let Some(request) = self.queue.pop_front() else {
                break;
            };
            if !self.pending.contains(&request)
                || !self.planner.accepts(request.authority.generation)
                || self.active.contains(&request)
            {
                continue;
            }
            self.active.insert(request);
            claimed.push(request);
        }
        claimed
    }

    #[cfg(test)]
    pub fn finish<J: Into<ViewerTileJob>>(
        &mut self,
        request: J,
        result: Result<(Arc<RenderImage>, usize), String>,
    ) -> bool {
        self.finish_at(request, result, Instant::now())
    }

    pub fn finish_at<J: Into<ViewerTileJob>>(
        &mut self,
        request: J,
        result: Result<(Arc<RenderImage>, usize), String>,
        now: Instant,
    ) -> bool {
        let request = request.into();
        self.active.remove(&request);
        self.pending.remove(&request);
        if !self.planner.accepts(request.authority.generation)
            || request.authority.source.revision != self.raster_revision
        {
            self.rejected_stale_tiles = self.rejected_stale_tiles.saturating_add(1);
            return false;
        }
        match result {
            Ok((image, bytes)) => {
                let inserted = match request.quality {
                    ViewerRenderQuality::Preview => {
                        self.preview_cache.insert(request.raster, image, bytes)
                    }
                    ViewerRenderQuality::Full => self.cache.insert(request.raster, image, bytes),
                    ViewerRenderQuality::Detail => {
                        self.detail_cache.insert(request.raster, image, bytes)
                    }
                };
                if !inserted {
                    self.error = Some("viewer tile exceeds the cache limit".into());
                    return false;
                }
                self.page_errors.remove(&request.authority.page);
                match request.quality {
                    ViewerRenderQuality::Preview => {
                        self.schedule_promotion(
                            tile_job(request.authority, ViewerRenderQuality::Full),
                            now,
                        );
                    }
                    ViewerRenderQuality::Full => {
                        let is_current = self
                            .plan
                            .as_ref()
                            .is_some_and(|plan| plan.current_page == Some(request.authority.page));
                        let detail = tile_job(request.authority, ViewerRenderQuality::Detail);
                        if is_current && detail.raster != request.raster {
                            self.schedule_promotion(detail, now);
                        }
                    }
                    ViewerRenderQuality::Detail => {}
                }
                true
            }
            Err(error) => {
                self.error = Some(error.clone());
                self.page_errors.insert(request.authority.page, error);
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
                let detail = tile_job(request, ViewerRenderQuality::Detail);
                let full = tile_job(request, ViewerRenderQuality::Full);
                let preview = tile_job(request, ViewerRenderQuality::Preview);
                self.detail_cache
                    .peek(detail.raster)
                    .or_else(|| self.cache.peek(full.raster))
                    .or_else(|| self.preview_cache.peek(preview.raster))
                    .cloned()
                    .map(|image| (request, image))
            })
            .collect()
    }

    pub fn visible_tile_quality(&self, request: TileRequest) -> Option<ViewerRenderQuality> {
        let detail = tile_job(request, ViewerRenderQuality::Detail);
        if self.detail_cache.contains(detail.raster) {
            return Some(ViewerRenderQuality::Detail);
        }
        let full = tile_job(request, ViewerRenderQuality::Full);
        if self.cache.contains(full.raster) {
            return Some(ViewerRenderQuality::Full);
        }
        let preview = tile_job(request, ViewerRenderQuality::Preview);
        self.preview_cache
            .contains(preview.raster)
            .then_some(ViewerRenderQuality::Preview)
    }

    pub fn page_quality(&self, page: usize) -> Option<ViewerRenderQuality> {
        let requests = self
            .plan
            .as_ref()?
            .tiles
            .iter()
            .copied()
            .filter(|request| request.page == page)
            .collect::<Vec<_>>();
        if requests.is_empty() {
            return None;
        }
        let qualities = requests
            .into_iter()
            .map(|request| self.visible_tile_quality(request))
            .collect::<Option<Vec<_>>>()?;
        if qualities
            .iter()
            .all(|quality| *quality == ViewerRenderQuality::Detail)
        {
            Some(ViewerRenderQuality::Detail)
        } else if qualities.iter().all(|quality| {
            matches!(
                quality,
                ViewerRenderQuality::Full | ViewerRenderQuality::Detail
            )
        }) {
            Some(ViewerRenderQuality::Full)
        } else {
            Some(ViewerRenderQuality::Preview)
        }
    }

    pub fn page_error(&self, page: usize) -> Option<&str> {
        self.page_errors.get(&page).map(String::as_str)
    }

    pub fn retry_page(&mut self, page: usize) -> bool {
        let Some(plan) = self.plan.as_ref() else {
            return false;
        };
        let requests = plan
            .tiles
            .iter()
            .copied()
            .filter(|request| request.page == page)
            .collect::<Vec<_>>();
        if requests.is_empty() {
            return false;
        }
        self.page_errors.remove(&page);
        for request in requests {
            let preview = tile_job(request, ViewerRenderQuality::Preview);
            let full = tile_job(request, ViewerRenderQuality::Full);
            if self.cache.contains(full.raster) {
                let detail = tile_job(request, ViewerRenderQuality::Detail);
                if detail.raster != full.raster {
                    self.enqueue(detail, true);
                }
            } else if self.preview_cache.contains(preview.raster) {
                self.enqueue(full, true);
            } else if preview.raster == full.raster {
                self.enqueue(full, true);
            } else {
                self.enqueue(preview, true);
            }
        }
        true
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
        self.queue
            .retain(|candidate| candidate.authority != request);
        self.pending
            .retain(|candidate| candidate.authority != request);
        self.active
            .retain(|candidate| candidate.authority != request);
        self.cache.insert(request, image, bytes)
    }

    pub fn cache_len(&self) -> usize {
        self.preview_cache.len() + self.cache.len() + self.detail_cache.len()
    }

    pub fn cache_bytes(&self) -> usize {
        self.preview_cache.bytes() + self.cache.bytes() + self.detail_cache.bytes()
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
        let current_quality = self.plan.as_ref().and_then(|plan| {
            let current_page = plan.current_page?;
            self.page_quality(current_page)
        });
        DocumentViewerSnapshot {
            mode: self.mode,
            zoom_percent: self.zoom_percent,
            generation: self.plan.as_ref().map_or(0, |plan| plan.generation),
            queued_tiles: self.queue.len(),
            active_tiles: self.active.len(),
            cache_entries: self.cache_len(),
            cache_bytes: self.cache_bytes(),
            cache_max_bytes: PREVIEW_CACHE_BYTES + FULL_CACHE_BYTES + DETAIL_CACHE_BYTES,
            rejected_stale_tiles: self.rejected_stale_tiles,
            preview_tiles: self.preview_cache.len(),
            full_tiles: self.cache.len(),
            detail_tiles: self.detail_cache.len(),
            current_quality,
            render_failed_pages: self.page_errors.len(),
            adaptive_level: self.adaptive_level,
        }
    }
}

fn cache_policy(max_bytes: usize) -> CachePolicy {
    CachePolicy {
        max_bytes,
        ..CachePolicy::default()
    }
}

fn tile_job(authority: TileRequest, quality: ViewerRenderQuality) -> ViewerTileJob {
    let raster = match quality {
        ViewerRenderQuality::Full => authority,
        ViewerRenderQuality::Preview => scaled_tile_request(
            authority,
            ((authority.zoom_tenths as f32 * PREVIEW_RASTER_RATIO).round() as u32)
                .max(MIN_PREVIEW_ZOOM_TENTHS)
                .min(authority.zoom_tenths),
        ),
        ViewerRenderQuality::Detail => scaled_tile_request(
            authority,
            ((authority.zoom_tenths as f32 * DETAIL_RASTER_RATIO).round() as u32)
                .min(MAX_DETAIL_ZOOM_TENTHS)
                .max(authority.zoom_tenths),
        ),
    };
    ViewerTileJob {
        authority,
        raster,
        quality,
    }
}

fn scaled_tile_request(authority: TileRequest, zoom_tenths: u32) -> TileRequest {
    if zoom_tenths == authority.zoom_tenths || authority.zoom_tenths == 0 {
        return authority;
    }
    let ratio = zoom_tenths as f64 / authority.zoom_tenths as f64;
    TileRequest {
        zoom_tenths,
        crop: butter_paper_gpui_gallery::viewer::PixelRect {
            x: (authority.crop.x as f64 * ratio).floor() as usize,
            y: (authority.crop.y as f64 * ratio).floor() as usize,
            width: (authority.crop.width as f64 * ratio).ceil().max(1.) as usize,
            height: (authority.crop.height as f64 * ratio).ceil().max(1.) as usize,
        },
        ..authority
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Frame, ImageBuffer, Rgba};
    use smallvec::smallvec;

    fn test_image() -> Arc<RenderImage> {
        let pixels = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_pixel(2, 2, Rgba([0, 0, 0, 255]));
        Arc::new(RenderImage::new(smallvec![Frame::new(pixels)]))
    }

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

    #[test]
    fn viewer_quality_pipeline_keeps_preview_until_full_then_promotes_current_page_detail() {
        let mut viewer = DocumentViewerState::default();
        let now = Instant::now();
        viewer.configure(PageViewMode::SinglePage, 800.);
        let plan = viewer
            .plan_at(11, &[(612., 792.)], &[0], 0, 800., 600., 0., 0., 1., now)
            .unwrap();
        let preview = viewer.claim_jobs();
        assert!(!preview.is_empty());
        assert!(
            preview
                .iter()
                .all(|job| job.quality == ViewerRenderQuality::Preview)
        );
        assert!(viewer.finish_at(preview[0], Ok((test_image(), 16)), now));
        assert_eq!(
            viewer.visible_tile_quality(plan.tiles[0]),
            Some(ViewerRenderQuality::Preview),
        );

        assert_eq!(viewer.release_due_promotions(now), 1);
        let full = viewer.claim_jobs();
        assert!(full.iter().any(|job| {
            job.authority == preview[0].authority && job.quality == ViewerRenderQuality::Full
        }));
        let full = full
            .into_iter()
            .find(|job| job.authority == preview[0].authority)
            .unwrap();
        assert!(viewer.finish_at(full, Ok((test_image(), 16)), now));
        assert_eq!(
            viewer.visible_tile_quality(plan.tiles[0]),
            Some(ViewerRenderQuality::Full),
        );

        assert_eq!(
            viewer.release_due_promotions(now + Duration::from_millis(1_199)),
            0
        );
        assert_eq!(
            viewer.release_due_promotions(now + Duration::from_millis(1_200)),
            1
        );
        let detail = viewer.claim_jobs();
        let detail = detail
            .into_iter()
            .find(|job| job.authority == preview[0].authority)
            .expect("the current page must refine to one higher-detail raster");
        assert_eq!(detail.quality, ViewerRenderQuality::Detail);
        assert!(viewer.finish_at(
            detail,
            Ok((test_image(), 16)),
            now + Duration::from_millis(1_200),
        ));
        assert_eq!(
            viewer.visible_tile_quality(plan.tiles[0]),
            Some(ViewerRenderQuality::Detail),
        );
        assert_eq!(viewer.snapshot().cache_bytes, 48);
        assert_eq!(
            viewer.snapshot().current_quality,
            Some(ViewerRenderQuality::Detail)
        );
    }

    #[test]
    fn viewer_quality_policy_matches_frozen_electron_dwell_and_motion_hysteresis() {
        assert_eq!(
            (0..4)
                .map(|level| viewer_quality_promotion_delay_ms(level, true, false, false, false))
                .collect::<Vec<_>>(),
            [Some(0), Some(8), Some(24), Some(48)],
        );
        assert_eq!(
            (0..4)
                .map(|level| viewer_quality_promotion_delay_ms(level, false, true, false, false))
                .collect::<Vec<_>>(),
            [Some(48), Some(72), Some(112), Some(160)],
        );
        assert_eq!(
            viewer_quality_promotion_delay_ms(0, false, true, true, false),
            None,
        );
        assert_eq!(
            viewer_quality_promotion_delay_ms(3, true, true, true, true),
            Some(0),
        );
        assert!(!viewer_motion_is_rapid(false, 1.49));
        assert!(viewer_motion_is_rapid(false, 1.5));
        assert!(viewer_motion_is_rapid(true, 0.75));
        assert!(!viewer_motion_is_rapid(true, 0.74));
    }

    #[test]
    fn viewer_quality_adaptive_controller_owns_runtime_pressure_and_slow_recovery() {
        let mut controller = AdaptiveViewerPerformance::default();
        let start = Instant::now();
        for frame in 0..140 {
            controller.observe_frame(start + Duration::from_micros(8_330 * frame));
        }
        assert_eq!(
            controller
                .evaluate(ViewerRenderDiagnostics::default())
                .level,
            0,
        );

        let backlog = ViewerRenderDiagnostics {
            queued_page_renders: 3,
            ..Default::default()
        };
        assert_eq!(controller.evaluate(backlog).level, 1);
        assert_eq!(controller.evaluate(backlog).level, 2);

        for evaluation in 0..5 {
            assert_eq!(
                controller
                    .evaluate(ViewerRenderDiagnostics::default())
                    .level,
                2,
                "recovery evaluation {evaluation} must retain the pressured tier",
            );
        }
        assert_eq!(
            controller
                .evaluate(ViewerRenderDiagnostics::default())
                .level,
            1,
        );
    }

    #[test]
    fn viewer_quality_runtime_level_reschedules_promotions_and_limits_level_three_work() {
        let mut viewer = DocumentViewerState::default();
        let now = Instant::now();
        viewer.configure(PageViewMode::SinglePage, 800.);
        viewer
            .plan_at(15, &[(612., 792.)], &[0], 0, 800., 600., 0., 0., 1., now)
            .unwrap();
        let preview = viewer.claim_jobs()[0];
        assert!(viewer.finish_at(preview, Ok((test_image(), 16)), now));

        assert!(viewer.set_adaptive_level(3, now));
        assert_eq!(viewer.snapshot().adaptive_level, 3);
        assert_eq!(
            viewer.release_due_promotions(now + Duration::from_millis(47)),
            0,
        );
        assert_eq!(
            viewer.release_due_promotions(now + Duration::from_millis(48)),
            1,
        );
        assert_eq!(viewer.claim_jobs().len(), 1);
    }

    #[test]
    fn viewer_quality_scheduler_resets_motion_settle_and_defers_rapid_promotions() {
        let mut viewer = DocumentViewerState::default();
        let now = Instant::now();
        viewer.configure(PageViewMode::SinglePage, 800.);
        viewer
            .plan_at(14, &[(612., 792.)], &[0], 0, 800., 600., 0., 0., 1., now)
            .unwrap();
        let preview = viewer.claim_jobs()[0];
        assert!(viewer.finish_at(preview, Ok((test_image(), 16)), now));
        viewer.observe_motion(0., 0., now);
        viewer.observe_motion(32., 0., now + Duration::from_millis(20));
        assert_eq!(
            viewer.release_due_promotions(now + Duration::from_millis(179)),
            0
        );
        viewer.observe_motion(192., 0., now + Duration::from_millis(179));
        assert_eq!(
            viewer.release_due_promotions(now + Duration::from_millis(358)),
            0
        );
        assert_eq!(
            viewer.release_due_promotions(now + Duration::from_millis(359)),
            1,
            "settling re-arms the target full-quality deadline from the settle instant",
        );
        assert_eq!(viewer.claim_jobs()[0].quality, ViewerRenderQuality::Full);
    }

    #[test]
    fn viewer_quality_upgrade_failure_retains_preview_and_stale_completion_is_rejected() {
        let mut viewer = DocumentViewerState::default();
        let now = Instant::now();
        viewer.configure(PageViewMode::SinglePage, 800.);
        let first = viewer
            .plan_at(12, &[(612., 792.)], &[0], 0, 800., 600., 0., 0., 1., now)
            .unwrap();
        let preview = viewer.claim_jobs()[0];
        assert!(viewer.finish_at(preview, Ok((test_image(), 16)), now));
        assert_eq!(viewer.release_due_promotions(now), 1);
        let full = viewer
            .claim_jobs()
            .into_iter()
            .find(|job| job.authority == preview.authority)
            .unwrap();
        assert!(!viewer.finish(full, Err("full refinement failed".into())));
        assert_eq!(
            viewer.visible_tile_quality(first.tiles[0]),
            Some(ViewerRenderQuality::Preview),
        );

        viewer.configure(PageViewMode::Continuous, 400.);
        viewer
            .plan(12, &[(612., 792.)], &[0], 0, 800., 600., 0., 0., 1.)
            .unwrap();
        assert!(!viewer.finish(full, Ok((test_image(), 16))));
        assert_eq!(viewer.snapshot().rejected_stale_tiles, 1);
    }

    #[test]
    fn viewer_quality_full_cache_evicts_under_sustained_pressure_without_exceeding_total_budget() {
        let mut viewer = DocumentViewerState::default();
        let plan = viewer
            .plan(13, &[(612., 792.)], &[0], 0, 800., 600., 0., 0., 1.)
            .unwrap();
        let base = plan.tiles[0];
        for index in 0..10 {
            let request = TileRequest {
                crop: butter_paper_gpui_gallery::viewer::PixelRect {
                    x: index * 8,
                    ..base.crop
                },
                ..base
            };
            assert!(viewer.insert_direct(request, test_image(), 20 * 1024 * 1024));
        }
        assert_eq!(viewer.cache_len(), 8);
        assert_eq!(viewer.cache_bytes(), FULL_CACHE_BYTES);
        assert!(viewer.cache_bytes() <= viewer.snapshot().cache_max_bytes);
        assert!(!viewer.touch_cached(base));
    }
}
