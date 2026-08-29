//! Render planning for a zoomable, scrollable PDF viewer.

use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};

const BYTES_PER_PIXEL: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn right(self) -> f32 {
        self.x + self.width
    }

    fn bottom(self) -> f32 {
        self.y + self.height
    }

    fn intersection(self, other: Self) -> Option<Self> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        (right > left && bottom > top).then(|| Self::new(left, top, right - left, bottom - top))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageGeometry {
    pub page: usize,
    pub width_points: f32,
    pub height_points: f32,
}

impl PageGeometry {
    pub const fn new(page: usize, width_points: f32, height_points: f32) -> Self {
        Self {
            page,
            width_points,
            height_points,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportGeometry {
    pub width: f32,
    pub height: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub visible_rect: Rect,
}

pub struct RenderInput<'a> {
    pub source: RenderSource,
    pub pages: &'a [PageGeometry],
    pub zoom_percent: f32,
    pub device_scale: f32,
    pub page_gap: f32,
    pub viewport: ViewportGeometry,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RenderSource {
    pub document_id: u64,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PixelRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageLayout {
    pub page: usize,
    pub logical_rect: Rect,
    pub device_width: usize,
    pub device_height: usize,
    pub column_index: usize,
    pub row_index: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CadOrganisation {
    Columns,
    Rows,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderLayout {
    Continuous,
    Cad {
        organisation: CadOrganisation,
        pages_per_lane: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TileRequest {
    pub source: RenderSource,
    pub generation: u64,
    pub page: usize,
    pub zoom_tenths: u32,
    pub device_scale_millis: u32,
    pub rotation_quarter_turns: u8,
    pub crop: PixelRect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CachePolicy {
    pub tile_edge: usize,
    pub max_tiles_per_plan: usize,
    pub max_bytes: usize,
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            tile_edge: 1_024,
            max_tiles_per_plan: 32,
            max_bytes: 256 * 1_024 * 1_024,
        }
    }
}

struct CachedTile<T> {
    value: T,
    bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct TileIdentity {
    source: RenderSource,
    page: usize,
    zoom_tenths: u32,
    device_scale_millis: u32,
    crop: PixelRect,
}

impl From<TileRequest> for TileIdentity {
    fn from(request: TileRequest) -> Self {
        Self {
            source: request.source,
            page: request.page,
            zoom_tenths: request.zoom_tenths,
            device_scale_millis: request.device_scale_millis,
            crop: request.crop,
        }
    }
}

/// A byte-accounted least-recently-used cache for decoded tile resources.
pub struct TileCache<T> {
    policy: CachePolicy,
    entries: HashMap<TileIdentity, CachedTile<T>>,
    order: VecDeque<TileIdentity>,
    bytes: usize,
}

impl<T> TileCache<T> {
    pub fn new(policy: CachePolicy) -> Self {
        Self {
            policy,
            entries: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
        }
    }

    pub fn insert(&mut self, key: TileRequest, value: T, bytes: usize) -> bool {
        let key = TileIdentity::from(key);
        if bytes > self.policy.max_bytes {
            return false;
        }
        if let Some(previous) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(previous.bytes);
            self.order.retain(|candidate| *candidate != key);
        }
        while self.bytes + bytes > self.policy.max_bytes {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(removed.bytes);
            }
        }
        self.entries.insert(key, CachedTile { value, bytes });
        self.order.push_back(key);
        self.bytes += bytes;
        true
    }

    pub fn get(&mut self, key: TileRequest) -> Option<&T> {
        let key = TileIdentity::from(key);
        if !self.entries.contains_key(&key) {
            return None;
        }
        self.order.retain(|candidate| *candidate != key);
        self.order.push_back(key);
        self.entries.get(&key).map(|tile| &tile.value)
    }

    pub fn contains(&self, key: TileRequest) -> bool {
        self.entries.contains_key(&TileIdentity::from(key))
    }

    pub fn peek(&self, key: TileRequest) -> Option<&T> {
        self.entries
            .get(&TileIdentity::from(key))
            .map(|tile| &tile.value)
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.bytes = 0;
    }
}

#[derive(Debug)]
pub struct RenderPlan {
    pub generation: u64,
    pub page_layouts: Vec<PageLayout>,
    pub visible_pages: Vec<usize>,
    pub current_page: Option<usize>,
    pub total_height: f32,
    pub total_width: f32,
    pub tiles: Vec<TileRequest>,
    pub requested_bytes: usize,
    pub cache: CachePolicy,
}

#[derive(Clone, Copy)]
struct TileContext {
    source: RenderSource,
    generation: u64,
    zoom_tenths: u32,
    device_scale_millis: u32,
}

#[derive(Clone, Copy)]
struct FocusScore {
    distance_squared: f32,
    intersection_area: f32,
    source_index: usize,
}

impl FocusScore {
    fn compare(self, other: Self) -> Ordering {
        self.distance_squared
            .total_cmp(&other.distance_squared)
            .then_with(|| other.intersection_area.total_cmp(&self.intersection_area))
            .then_with(|| self.source_index.cmp(&other.source_index))
    }
}

struct VisibleLayout {
    layout: PageLayout,
    local_visible: Rect,
    focus: FocusScore,
}

struct TileCursor {
    context: TileContext,
    layout: PageLayout,
    edge: usize,
    right: usize,
    bottom: usize,
    next_x: usize,
    next_y: usize,
    first_x: usize,
}

impl TileCursor {
    fn new(
        context: TileContext,
        layout: PageLayout,
        local_visible: Rect,
        device_scale: f32,
        edge: usize,
    ) -> Self {
        let left = ((local_visible.x * device_scale).floor() as usize / edge) * edge;
        let top = ((local_visible.y * device_scale).floor() as usize / edge) * edge;
        let right =
            ((local_visible.right() * device_scale).ceil() as usize).min(layout.device_width);
        let bottom =
            ((local_visible.bottom() * device_scale).ceil() as usize).min(layout.device_height);
        Self {
            context,
            layout,
            edge,
            right,
            bottom,
            next_x: left,
            next_y: top,
            first_x: left,
        }
    }
}

impl Iterator for TileCursor {
    type Item = TileRequest;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_y >= self.bottom || self.next_x >= self.right {
            return None;
        }
        let x = self.next_x;
        let y = self.next_y;
        self.next_x += self.edge;
        if self.next_x >= self.right {
            self.next_x = self.first_x;
            self.next_y += self.edge;
        }
        Some(TileRequest {
            source: self.context.source,
            generation: self.context.generation,
            page: self.layout.page,
            zoom_tenths: self.context.zoom_tenths,
            device_scale_millis: self.context.device_scale_millis,
            rotation_quarter_turns: 0,
            crop: PixelRect {
                x,
                y,
                width: self.edge.min(self.layout.device_width - x),
                height: self.edge.min(self.layout.device_height - y),
            },
        })
    }
}

pub struct RenderPlanner {
    generation: u64,
    policy: CachePolicy,
    last_tile_identities: Option<Vec<TileIdentity>>,
}

impl Default for RenderPlanner {
    fn default() -> Self {
        Self {
            generation: 0,
            policy: CachePolicy::default(),
            last_tile_identities: None,
        }
    }
}

impl RenderPlanner {
    /// Returns true only while a result belongs to the latest plan.
    pub fn accepts(&self, generation: u64) -> bool {
        generation != 0 && generation == self.generation
    }

    /// Invalidates queued and in-flight work without needing to enumerate it.
    pub fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.last_tile_identities = None;
    }

    pub fn plan(&mut self, input: RenderInput<'_>) -> RenderPlan {
        self.plan_with_layout(input, RenderLayout::Continuous)
    }

    pub fn plan_with_layout(
        &mut self,
        input: RenderInput<'_>,
        layout_mode: RenderLayout,
    ) -> RenderPlan {
        let candidate_generation = self.generation.wrapping_add(1).max(1);
        let zoom = finite_positive(input.zoom_percent, 100.0) / 100.0;
        let device_scale = finite_positive(input.device_scale, 1.0);
        let zoom_tenths = (zoom * 1_000.0).round() as u32;
        let device_scale_millis = (device_scale * 1_000.0).round() as u32;
        let tile_context = TileContext {
            source: input.source,
            generation: candidate_generation,
            zoom_tenths,
            device_scale_millis,
        };
        let gap = finite_nonnegative(input.page_gap);
        let (page_layouts, total_width, total_height) = build_page_layouts(
            input.pages,
            zoom,
            device_scale,
            gap,
            input.viewport.width,
            layout_mode,
        );

        let local_visible = input.viewport.visible_rect;
        let visible = Rect::new(
            finite_nonnegative(input.viewport.scroll_x) + local_visible.x,
            finite_nonnegative(input.viewport.scroll_y) + local_visible.y,
            local_visible.width.min(input.viewport.width).max(0.0),
            local_visible.height.min(input.viewport.height).max(0.0),
        );
        let viewport_center = (
            visible.x + visible.width / 2.0,
            visible.y + visible.height / 2.0,
        );
        let focus_scores = page_layouts
            .iter()
            .enumerate()
            .map(|(source_index, layout)| {
                focus_score(*layout, visible, viewport_center, source_index)
            })
            .collect::<Vec<_>>();
        let current_page = page_layouts
            .iter()
            .zip(&focus_scores)
            .min_by(|(_, left), (_, right)| left.compare(**right))
            .map(|(layout, _)| layout.page);

        let mut visible_layouts = Vec::new();
        for (source_index, layout) in page_layouts.iter().enumerate() {
            let Some(intersection) = layout.logical_rect.intersection(visible) else {
                continue;
            };
            let local = Rect::new(
                intersection.x - layout.logical_rect.x,
                intersection.y - layout.logical_rect.y,
                intersection.width,
                intersection.height,
            );
            visible_layouts.push(VisibleLayout {
                layout: *layout,
                local_visible: local,
                focus: focus_scores[source_index],
            });
        }
        let visible_pages = visible_layouts
            .iter()
            .map(|visible| visible.layout.page)
            .collect::<Vec<_>>();
        visible_layouts.sort_by(|left, right| left.focus.compare(right.focus));
        let mut cursors = visible_layouts
            .into_iter()
            .map(|visible| {
                TileCursor::new(
                    tile_context,
                    visible.layout,
                    visible.local_visible,
                    device_scale,
                    self.policy.tile_edge,
                )
            })
            .collect::<Vec<_>>();
        let mut tiles = Vec::with_capacity(self.policy.max_tiles_per_plan);
        let mut requested_bytes = 0;
        while tiles.len() < self.policy.max_tiles_per_plan {
            let mut added_tile = false;
            for cursor in &mut cursors {
                if tiles.len() >= self.policy.max_tiles_per_plan {
                    break;
                }
                let Some(tile) = cursor.next() else {
                    continue;
                };
                let tile_bytes = tile.crop.width * tile.crop.height * BYTES_PER_PIXEL;
                if requested_bytes + tile_bytes <= self.policy.max_bytes {
                    requested_bytes += tile_bytes;
                    tiles.push(tile);
                    added_tile = true;
                }
            }
            if !added_tile {
                break;
            }
        }
        let tile_identities = tiles
            .iter()
            .copied()
            .map(TileIdentity::from)
            .collect::<Vec<_>>();
        let generation = if self.last_tile_identities.as_ref() == Some(&tile_identities)
            && self.generation != 0
        {
            for tile in &mut tiles {
                tile.generation = self.generation;
            }
            self.generation
        } else {
            self.generation = candidate_generation;
            self.last_tile_identities = Some(tile_identities);
            candidate_generation
        };
        RenderPlan {
            generation,
            page_layouts,
            visible_pages,
            current_page,
            total_height,
            total_width,
            tiles,
            requested_bytes,
            cache: self.policy,
        }
    }
}

fn build_page_layouts(
    pages: &[PageGeometry],
    zoom: f32,
    device_scale: f32,
    gap: f32,
    viewport_width: f32,
    layout_mode: RenderLayout,
) -> (Vec<PageLayout>, f32, f32) {
    match layout_mode {
        RenderLayout::Continuous => {
            let widest_page = pages
                .iter()
                .map(|page| finite_positive(page.width_points, 1.0) * zoom)
                .fold(0.0, f32::max);
            let total_width = viewport_width.max(widest_page + gap * 2.0);
            let mut layouts = Vec::with_capacity(pages.len());
            let mut page_y = gap;
            for (row_index, page) in pages.iter().enumerate() {
                let width = finite_positive(page.width_points, 1.0) * zoom;
                let height = finite_positive(page.height_points, 1.0) * zoom;
                layouts.push(PageLayout {
                    page: page.page,
                    logical_rect: Rect::new((total_width - width) / 2.0, page_y, width, height),
                    device_width: (width * device_scale).ceil() as usize,
                    device_height: (height * device_scale).ceil() as usize,
                    column_index: 0,
                    row_index,
                });
                page_y += height + gap;
            }
            (layouts, total_width, page_y)
        }
        RenderLayout::Cad {
            organisation,
            pages_per_lane,
        } => {
            let pages_per_lane = pages_per_lane.clamp(1, 100);
            let column_count = match organisation {
                CadOrganisation::Columns => pages.len().div_ceil(pages_per_lane).max(1),
                CadOrganisation::Rows => pages.len().clamp(1, pages_per_lane),
            };
            let row_count = match organisation {
                CadOrganisation::Columns => pages.len().clamp(1, pages_per_lane),
                CadOrganisation::Rows => pages.len().div_ceil(pages_per_lane).max(1),
            };
            let mut column_widths = vec![1_f32; column_count];
            let mut row_heights = vec![1_f32; row_count];
            let scaled = pages
                .iter()
                .enumerate()
                .map(|(position, page)| {
                    let (column, row) = match organisation {
                        CadOrganisation::Columns => {
                            (position / pages_per_lane, position % pages_per_lane)
                        }
                        CadOrganisation::Rows => {
                            (position % pages_per_lane, position / pages_per_lane)
                        }
                    };
                    let width = finite_positive(page.width_points, 1.) * zoom;
                    let height = finite_positive(page.height_points, 1.) * zoom;
                    column_widths[column] = column_widths[column].max(width);
                    row_heights[row] = row_heights[row].max(height);
                    (page, width, height, column, row)
                })
                .collect::<Vec<_>>();
            let grid_width = column_widths.iter().sum::<f32>() + gap * (column_count + 1) as f32;
            let grid_height = row_heights.iter().sum::<f32>() + gap * (row_count + 1) as f32;
            let base_left = ((viewport_width - grid_width) / 2.).max(0.);
            let mut column_lefts = Vec::with_capacity(column_count);
            let mut left = base_left + gap;
            for width in &column_widths {
                column_lefts.push(left);
                left += *width + gap;
            }
            let mut row_tops = Vec::with_capacity(row_count);
            let mut top = gap;
            for height in &row_heights {
                row_tops.push(top);
                top += *height + gap;
            }
            let layouts = scaled
                .into_iter()
                .map(
                    |(page, width, height, column_index, row_index)| PageLayout {
                        page: page.page,
                        logical_rect: Rect::new(
                            column_lefts[column_index] + (column_widths[column_index] - width) / 2.,
                            row_tops[row_index],
                            width,
                            height,
                        ),
                        device_width: (width * device_scale).ceil() as usize,
                        device_height: (height * device_scale).ceil() as usize,
                        column_index,
                        row_index,
                    },
                )
                .collect();
            (
                layouts,
                viewport_width.max(base_left + grid_width),
                grid_height,
            )
        }
    }
}

fn focus_score(
    layout: PageLayout,
    visible: Rect,
    viewport_center: (f32, f32),
    source_index: usize,
) -> FocusScore {
    let rect = layout.logical_rect;
    let distance_x = if viewport_center.0 < rect.x {
        rect.x - viewport_center.0
    } else if viewport_center.0 > rect.right() {
        viewport_center.0 - rect.right()
    } else {
        0.0
    };
    let distance_y = if viewport_center.1 < rect.y {
        rect.y - viewport_center.1
    } else if viewport_center.1 > rect.bottom() {
        viewport_center.1 - rect.bottom()
    } else {
        0.0
    };
    let intersection_area = rect
        .intersection(visible)
        .map(|intersection| intersection.width * intersection.height)
        .unwrap_or(0.0);
    FocusScore {
        distance_squared: distance_x * distance_x + distance_y * distance_y,
        intersection_area,
        source_index,
    }
}

fn finite_positive(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}
