//! Render planning for a zoomable, scrollable PDF viewer.

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
        let widest_page = input
            .pages
            .iter()
            .map(|page| finite_positive(page.width_points, 1.0) * zoom)
            .fold(0.0, f32::max);
        let layout_width = input.viewport.width.max(widest_page + gap * 2.0);
        let mut page_layouts = Vec::with_capacity(input.pages.len());
        let mut page_y = gap;
        for page in input.pages {
            let logical_width = finite_positive(page.width_points, 1.0) * zoom;
            let logical_height = finite_positive(page.height_points, 1.0) * zoom;
            let x = (layout_width - logical_width) / 2.0;
            page_layouts.push(PageLayout {
                page: page.page,
                logical_rect: Rect::new(x, page_y, logical_width, logical_height),
                device_width: (logical_width * device_scale).ceil() as usize,
                device_height: (logical_height * device_scale).ceil() as usize,
            });
            page_y += logical_height + gap;
        }

        let local_visible = input.viewport.visible_rect;
        let visible = Rect::new(
            finite_nonnegative(input.viewport.scroll_x) + local_visible.x,
            finite_nonnegative(input.viewport.scroll_y) + local_visible.y,
            local_visible.width.min(input.viewport.width).max(0.0),
            local_visible.height.min(input.viewport.height).max(0.0),
        );
        let mut tiles = Vec::new();
        let mut visible_pages = Vec::new();
        for layout in &page_layouts {
            let Some(intersection) = layout.logical_rect.intersection(visible) else {
                continue;
            };
            visible_pages.push(layout.page);
            let local = Rect::new(
                intersection.x - layout.logical_rect.x,
                intersection.y - layout.logical_rect.y,
                intersection.width,
                intersection.height,
            );
            append_tiles(
                &mut tiles,
                tile_context,
                *layout,
                local,
                device_scale,
                self.policy,
            );
            if tiles.len() >= self.policy.max_tiles_per_plan {
                break;
            }
        }
        tiles.truncate(self.policy.max_tiles_per_plan);
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
        let requested_bytes = tiles
            .iter()
            .map(|tile| tile.crop.width * tile.crop.height * BYTES_PER_PIXEL)
            .sum();
        let viewport_center_y = visible.y + visible.height / 2.0;
        let current_page = page_layouts
            .iter()
            .min_by(|left, right| {
                let left_distance = distance_to_vertical_rect(viewport_center_y, left.logical_rect);
                let right_distance =
                    distance_to_vertical_rect(viewport_center_y, right.logical_rect);
                left_distance.total_cmp(&right_distance)
            })
            .map(|layout| layout.page);
        RenderPlan {
            generation,
            page_layouts,
            visible_pages,
            current_page,
            total_height: page_y,
            tiles,
            requested_bytes,
            cache: self.policy,
        }
    }
}

fn distance_to_vertical_rect(y: f32, rect: Rect) -> f32 {
    if y < rect.y {
        rect.y - y
    } else if y > rect.bottom() {
        y - rect.bottom()
    } else {
        0.0
    }
}

fn append_tiles(
    tiles: &mut Vec<TileRequest>,
    context: TileContext,
    layout: PageLayout,
    local_visible: Rect,
    device_scale: f32,
    policy: CachePolicy,
) {
    let edge = policy.tile_edge;
    let left = ((local_visible.x * device_scale).floor() as usize / edge) * edge;
    let top = ((local_visible.y * device_scale).floor() as usize / edge) * edge;
    let right = ((local_visible.right() * device_scale).ceil() as usize).min(layout.device_width);
    let bottom =
        ((local_visible.bottom() * device_scale).ceil() as usize).min(layout.device_height);
    for y in (top..bottom).step_by(edge) {
        for x in (left..right).step_by(edge) {
            if tiles.len() >= policy.max_tiles_per_plan {
                return;
            }
            tiles.push(TileRequest {
                source: context.source,
                generation: context.generation,
                page: layout.page,
                zoom_tenths: context.zoom_tenths,
                device_scale_millis: context.device_scale_millis,
                rotation_quarter_turns: 0,
                crop: PixelRect {
                    x,
                    y,
                    width: edge.min(layout.device_width - x),
                    height: edge.min(layout.device_height - y),
                },
            });
        }
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
