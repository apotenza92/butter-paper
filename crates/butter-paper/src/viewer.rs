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
    /// Pages side by side in pairs, scrolled as rows (Two Pages view).
    TwoPages {
        cover_alone: bool,
    },
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
    /// Real colours or Dark pages. Part of the tile's identity: a Dark tile
    /// never stands in for a Real one.
    pub paper: crate::paper_appearance::PaperAppearance,
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
    paper: crate::paper_appearance::PaperAppearance,
}

impl From<TileRequest> for TileIdentity {
    fn from(request: TileRequest) -> Self {
        Self {
            source: request.source,
            page: request.page,
            zoom_tenths: request.zoom_tenths,
            device_scale_millis: request.device_scale_millis,
            crop: request.crop,
            paper: request.paper,
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
        self.insert_evicting(key, value, bytes, &mut Vec::new())
    }

    /// `insert`, moving the entries it evicts (or replaces) to `evicted`.
    pub fn insert_evicting(
        &mut self,
        key: TileRequest,
        value: T,
        bytes: usize,
        evicted: &mut Vec<T>,
    ) -> bool {
        let key = TileIdentity::from(key);
        if bytes > self.policy.max_bytes {
            return false;
        }
        if let Some(previous) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(previous.bytes);
            self.order.retain(|candidate| *candidate != key);
            evicted.push(previous.value);
        }
        while self.bytes + bytes > self.policy.max_bytes {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(removed.bytes);
                evicted.push(removed.value);
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

    /// `clear`, moving every entry to `evicted`.
    pub fn drain_into(&mut self, evicted: &mut Vec<T>) {
        evicted.extend(self.entries.drain().map(|(_, tile)| tile.value));
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
    /// Distances from the viewport centre. Vertical comes first: a view
    /// panned sideways over blank space is still reading the page level with
    /// its centre.
    distance_y: f32,
    distance_x: f32,
    intersection_area: f32,
    source_index: usize,
}

impl FocusScore {
    fn compare(self, other: Self) -> Ordering {
        self.distance_y
            .total_cmp(&other.distance_y)
            .then_with(|| self.distance_x.total_cmp(&other.distance_x))
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
            paper: Default::default(),
        })
    }
}

#[derive(Default)]
pub struct RenderPlanner {
    generation: u64,
    policy: CachePolicy,
    last_tile_identities: Option<Vec<TileIdentity>>,
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
            finite_or_zero(input.viewport.scroll_x) + local_visible.x,
            finite_or_zero(input.viewport.scroll_y) + local_visible.y,
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

/// The row and column of page `page` in Two Pages view. Without the cover
/// alone, row r holds pages 2r and 2r + 1; with it, page 0 is alone in row
/// 0, in the right column as a book's cover, and row r holds pages 2r - 1
/// and 2r. A last page with no partner stays in the left column.
pub const fn two_page_slot(page: usize, cover_alone: bool) -> (usize, usize) {
    if cover_alone {
        if page == 0 {
            (0, 1)
        } else {
            (page.div_ceil(2), (page + 1) % 2)
        }
    } else {
        (page / 2, page % 2)
    }
}

/// The pages of row `row` in Two Pages view, of a document of `page_count`
/// pages: the first and, when it has one, the second.
pub fn two_page_row(row: usize, cover_alone: bool, page_count: usize) -> (usize, Option<usize>) {
    let first = match (cover_alone, row) {
        (true, 0) => return (0, None),
        (true, row) => row * 2 - 1,
        (false, row) => row * 2,
    };
    let second = first + 1;
    (first, (second < page_count).then_some(second))
}

/// The first page of the row holding `page` in Two Pages view.
pub const fn two_page_row_first(page: usize, cover_alone: bool) -> usize {
    let (row, _) = two_page_slot(page, cover_alone);
    match (cover_alone, row) {
        (true, 0) => 0,
        (true, row) => row * 2 - 1,
        (false, row) => row * 2,
    }
}

/// The first page of the row before (`forward` false) or after the row
/// holding `page`, in a document of `page_count` pages: Previous and Next
/// in Two Pages view move a spread. `None` at either end.
pub fn two_page_spread_step(
    page: usize,
    cover_alone: bool,
    page_count: usize,
    forward: bool,
) -> Option<usize> {
    if page_count == 0 {
        return None;
    }
    let (row, _) = two_page_slot(page.min(page_count - 1), cover_alone);
    let last_row = two_page_slot(page_count - 1, cover_alone).0;
    let target = if forward {
        (row < last_row).then_some(row + 1)?
    } else {
        row.checked_sub(1)?
    };
    Some(two_page_row(target, cover_alone, page_count).0)
}

/// The size of the row holding `page` in Two Pages view, at 100%: two
/// column slots with `gap` between them, as tall as the taller page. A page
/// alone in its row keeps an empty slot as wide as itself. A fit in Two
/// Pages fits this.
pub fn two_page_row_size(
    page_sizes: &[(f32, f32)],
    page: usize,
    cover_alone: bool,
    gap: f32,
) -> Option<(f32, f32)> {
    let (row, _) = two_page_slot(page, cover_alone);
    let (first, second) = two_page_row(row, cover_alone, page_sizes.len());
    let first = *page_sizes.get(first)?;
    Some(match second.and_then(|second| page_sizes.get(second)) {
        Some(second) => (first.0 + gap + second.0, first.1.max(second.1)),
        None => (first.0 * 2. + gap, first.1),
    })
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
            // The pages meet the viewport's edges; only the space between
            // them is a gap.
            let total_width = viewport_width.max(widest_page);
            let mut layouts = Vec::with_capacity(pages.len());
            let mut page_y = 0.0;
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
            let total_height = if layouts.is_empty() {
                0.0
            } else {
                page_y - gap
            };
            (layouts, total_width, total_height)
        }
        RenderLayout::TwoPages { cover_alone } => {
            // Each row's pages meet at their tops and the row is centred
            // as two column slots: a page alone in its row (the cover, an
            // odd last page) keeps its column beside an empty slot as wide
            // as itself, as a book shows it.
            let scaled = pages
                .iter()
                .enumerate()
                .map(|(position, page)| {
                    let (row, column) = two_page_slot(position, cover_alone);
                    let width = finite_positive(page.width_points, 1.) * zoom;
                    let height = finite_positive(page.height_points, 1.) * zoom;
                    (page, width, height, column, row)
                })
                .collect::<Vec<_>>();
            let row_count = scaled.iter().map(|(.., row)| row + 1).max().unwrap_or(0);
            let mut row_widths = vec![0_f32; row_count];
            let mut row_heights = vec![0_f32; row_count];
            let mut row_pages = vec![0_usize; row_count];
            for (_, width, height, _, row) in &scaled {
                row_widths[*row] += if row_widths[*row] > 0. {
                    gap + width
                } else {
                    *width
                };
                row_heights[*row] = row_heights[*row].max(*height);
                row_pages[*row] += 1;
            }
            // A lone page's empty slot.
            for (width, pages) in row_widths.iter_mut().zip(&row_pages) {
                if *pages == 1 {
                    *width = *width * 2. + gap;
                }
            }
            let total_width = row_widths.iter().copied().fold(viewport_width, f32::max);
            let mut row_tops = Vec::with_capacity(row_count);
            let mut top = 0.;
            for height in &row_heights {
                row_tops.push(top);
                top += height + gap;
            }
            let mut row_lefts = row_widths
                .iter()
                .map(|width| (total_width - width) / 2.)
                .collect::<Vec<_>>();
            let layouts = scaled
                .into_iter()
                .map(|(page, width, height, column_index, row_index)| {
                    // A lone page in the right column sits past its empty
                    // left slot.
                    if row_pages[row_index] == 1 && column_index == 1 {
                        row_lefts[row_index] += width + gap;
                    }
                    let left = row_lefts[row_index];
                    row_lefts[row_index] += width + gap;
                    PageLayout {
                        page: page.page,
                        logical_rect: Rect::new(left, row_tops[row_index], width, height),
                        device_width: (width * device_scale).ceil() as usize,
                        device_height: (height * device_scale).ceil() as usize,
                        column_index,
                        row_index,
                    }
                })
                .collect();
            let total_height = if row_count == 0 { 0. } else { top - gap };
            (layouts, total_width, total_height)
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
        distance_y,
        distance_x,
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

/// Scroll may be negative while the viewport is panned past the first page.
fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(x: usize) -> TileRequest {
        TileRequest {
            source: RenderSource {
                document_id: 1,
                revision: 1,
            },
            generation: 1,
            page: 0,
            zoom_tenths: 1_000,
            device_scale_millis: 1_000,
            rotation_quarter_turns: 0,
            crop: PixelRect {
                x,
                y: 0,
                width: 16,
                height: 16,
            },
            paper: Default::default(),
        }
    }

    #[test]
    fn a_tile_cache_hands_back_what_it_evicts_or_clears() {
        let mut cache = TileCache::new(CachePolicy {
            max_bytes: 20,
            ..CachePolicy::default()
        });
        let mut evicted = Vec::new();
        assert!(cache.insert_evicting(tile(0), "first", 10, &mut evicted));
        assert!(cache.insert_evicting(tile(16), "second", 10, &mut evicted));
        assert!(evicted.is_empty());
        assert!(cache.insert_evicting(tile(32), "third", 10, &mut evicted));
        assert_eq!(evicted, ["first"], "the least recently used goes");
        assert!(cache.insert_evicting(tile(32), "third again", 10, &mut evicted));
        assert_eq!(evicted, ["first", "third"], "a replaced entry goes too");
        cache.drain_into(&mut evicted);
        evicted[2..].sort();
        assert_eq!(evicted, ["first", "third", "second", "third again"]);
        assert!(cache.is_empty());
        assert_eq!(cache.bytes(), 0);
    }

    fn two_pages(
        pages: &[(f32, f32)],
        cover_alone: bool,
        viewport: f32,
    ) -> (Vec<PageLayout>, f32, f32) {
        let pages = pages
            .iter()
            .enumerate()
            .map(|(page, (width, height))| PageGeometry::new(page, *width, *height))
            .collect::<Vec<_>>();
        build_page_layouts(
            &pages,
            1.,
            1.,
            24.,
            viewport,
            RenderLayout::TwoPages { cover_alone },
        )
    }

    fn rects(layouts: &[PageLayout]) -> Vec<(usize, f32, f32, f32, f32)> {
        layouts
            .iter()
            .map(|layout| {
                let rect = layout.logical_rect;
                (layout.page, rect.x, rect.y, rect.width, rect.height)
            })
            .collect()
    }

    #[test]
    fn two_pages_pairs_odd_and_even_pages_or_puts_the_cover_alone() {
        assert_eq!(
            (0..5)
                .map(|page| two_page_slot(page, false))
                .collect::<Vec<_>>(),
            [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0)]
        );
        assert_eq!(
            (0..5)
                .map(|page| two_page_slot(page, true))
                .collect::<Vec<_>>(),
            [(0, 1), (1, 0), (1, 1), (2, 0), (2, 1)]
        );
        assert_eq!(two_page_row(0, false, 5), (0, Some(1)));
        assert_eq!(two_page_row(2, false, 5), (4, None));
        assert_eq!(two_page_row(0, true, 5), (0, None));
        assert_eq!(two_page_row(1, true, 5), (1, Some(2)));
        assert_eq!(two_page_row(2, true, 4), (3, None));

        // Five letter pages in a 1,400 pt view: pairs side by side, the
        // last alone in the left column, every row centred as two slots,
        // rows 24 pt apart.
        let letter = (612., 792.);
        let (layouts, width, height) = two_pages(&[letter; 5], false, 1_400.);
        assert_eq!(width, 1_400.);
        assert_eq!(height, 792. * 3. + 48.);
        let left = (1_400. - 1_248.) / 2.;
        assert_eq!(
            rects(&layouts),
            [
                (0, left, 0., 612., 792.),
                (1, left + 636., 0., 612., 792.),
                (2, left, 816., 612., 792.),
                (3, left + 636., 816., 612., 792.),
                (4, left, 1_632., 612., 792.),
            ]
        );
        assert_eq!(
            layouts
                .iter()
                .map(|layout| (layout.row_index, layout.column_index))
                .collect::<Vec<_>>(),
            [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0)]
        );

        // With the cover alone: page 1 alone in the right column, then
        // 2–3, then 4 alone in the left column.
        let (layouts, _, height) = two_pages(&[letter; 4], true, 1_400.);
        assert_eq!(height, 792. * 3. + 48.);
        assert_eq!(
            rects(&layouts),
            [
                (0, left + 636., 0., 612., 792.),
                (1, left, 816., 612., 792.),
                (2, left + 636., 816., 612., 792.),
                (3, left, 1_632., 612., 792.),
            ]
        );
    }

    #[test]
    fn two_pages_of_mixed_sizes_meet_at_their_tops_in_a_centred_row() {
        // A letter page beside a tabloid landscape, then a turned page
        // (its rotated size) beside a letter page, in a view narrower than
        // the widest row: the content widens to that row.
        let (layouts, width, height) = two_pages(
            &[(612., 792.), (1_224., 792.), (792., 612.), (612., 792.)],
            false,
            1_000.,
        );
        assert_eq!(width, 612. + 24. + 1_224.);
        assert_eq!(height, 792. + 24. + 792.);
        assert_eq!(
            rects(&layouts),
            [
                (0, 0., 0., 612., 792.),
                (1, 636., 0., 1_224., 792.),
                (2, (1_860. - 1_428.) / 2., 816., 792., 612.),
                (3, (1_860. - 1_428.) / 2. + 816., 816., 612., 792.),
            ]
        );
        assert_eq!(
            two_page_row_size(&[(612., 792.), (1_224., 900.)], 1, false, 24.),
            Some((1_860., 900.))
        );
        assert_eq!(
            two_page_row_size(&[(612., 792.)], 0, false, 24.),
            Some((1_248., 792.)),
            "a lone page fits as a two-column row"
        );
        assert_eq!(
            two_page_row_size(&[(612., 792.), (500., 700.), (600., 800.)], 0, true, 24.),
            Some((1_248., 792.))
        );
    }

    #[test]
    fn two_pages_steps_a_spread_to_the_first_page_of_the_next_or_previous_row() {
        // Nine pages, no cover: rows 1–2, 3–4, 5–6, 7–8, 9.
        assert_eq!(two_page_spread_step(0, false, 9, true), Some(2));
        assert_eq!(two_page_spread_step(1, false, 9, true), Some(2));
        assert_eq!(two_page_spread_step(7, false, 9, true), Some(8));
        assert_eq!(two_page_spread_step(8, false, 9, true), None);
        assert_eq!(two_page_spread_step(3, false, 9, false), Some(0));
        assert_eq!(two_page_spread_step(1, false, 9, false), None);
        // The cover alone: rows 1, 2–3, 4–5, 6–7, 8–9.
        assert_eq!(two_page_spread_step(0, true, 9, true), Some(1));
        assert_eq!(two_page_spread_step(2, true, 9, true), Some(3));
        assert_eq!(two_page_spread_step(2, true, 9, false), Some(0));
        assert_eq!(two_page_spread_step(8, true, 9, true), None);
        assert_eq!(two_page_row_first(8, true), 7);
        assert_eq!(two_page_row_first(8, false), 8);
        assert_eq!(two_page_spread_step(0, false, 0, true), None);
    }
}
