use std::time::{Duration, Instant};

use crate::{
    document_viewer::{ViewerFitPreset, resolve_fit_zoom_percent},
    mouse_wheel::{NotchAction, PageTurning, WheelTurns, ZoomSpeed},
    page_view_control::{PageViewMode, WheelBehavior},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewerZoomPreset {
    Manual,
    FitWidth,
    FitPage,
}

/// The durable zoom intent restored after a PDF opens.
///
/// Manual percentages must be finite in a persisted manifest and are clamped
/// to the reader's supported zoom range when applied.
#[derive(Clone, Copy, Debug)]
pub enum RestartZoom {
    FitWidth,
    FitPage,
    Manual(f32),
}

impl PartialEq for RestartZoom {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::FitWidth, Self::FitWidth) | (Self::FitPage, Self::FitPage) => true,
            (Self::Manual(left), Self::Manual(right)) => left.to_bits() == right.to_bits(),
            _ => false,
        }
    }
}

impl Eq for RestartZoom {}

/// The room Fit Page in Two Pages view leaves on each side of the row, in
/// pixels: the current page's outline (2 px outside the paper) and a dark
/// page's 1 px ring stay in view.
pub const FIT_PAGE_EDGE_ROOM: f32 = 2.;

/// The most pages whose Single Page view one document remembers. The oldest
/// visit is forgotten first.
pub const MAX_REMEMBERED_PAGE_VIEWS: usize = 100;

/// The zoom and scroll Single Page view left a page at, restored when the
/// page shows again.
#[derive(Clone, Copy, Debug)]
pub struct RememberedPageView {
    zoom: RestartZoom,
    scroll_x: f32,
    scroll_y: f32,
}

impl PartialEq for RememberedPageView {
    fn eq(&self, other: &Self) -> bool {
        self.zoom == other.zoom
            && self.scroll_x.to_bits() == other.scroll_x.to_bits()
            && self.scroll_y.to_bits() == other.scroll_y.to_bits()
    }
}

impl Eq for RememberedPageView {}

impl RememberedPageView {
    pub const fn new(zoom: RestartZoom, scroll_x: f32, scroll_y: f32) -> Self {
        Self {
            zoom,
            scroll_x,
            scroll_y,
        }
    }

    pub const fn zoom(self) -> RestartZoom {
        self.zoom
    }

    pub const fn scroll(self) -> (f32, f32) {
        (self.scroll_x, self.scroll_y)
    }

    pub fn is_finite(self) -> bool {
        zoom_is_finite(self.zoom) && self.scroll_x.is_finite() && self.scroll_y.is_finite()
    }
}

fn zoom_is_finite(zoom: RestartZoom) -> bool {
    match zoom {
        RestartZoom::FitWidth | RestartZoom::FitPage => true,
        RestartZoom::Manual(percent) => percent.is_finite(),
    }
}

/// The core reader view persisted for one durable PDF path.
///
/// Page indices are zero-based. Page, zoom, and scroll values are clamped
/// against the successfully opened document before they become live state.
#[derive(Clone, Debug)]
pub struct RestartView {
    current_page: u32,
    mode: PageViewMode,
    zoom: RestartZoom,
    scroll_x: f32,
    scroll_y: f32,
    /// Single Page view's remembered pages, the oldest visit first.
    page_views: Vec<(u32, RememberedPageView)>,
    /// Two Pages view shows the first page alone.
    cover_alone: bool,
    /// The document shows in two panes side by side.
    split_view: bool,
}

impl PartialEq for RestartView {
    fn eq(&self, other: &Self) -> bool {
        self.current_page == other.current_page
            && self.mode == other.mode
            && self.zoom == other.zoom
            && self.scroll_x.to_bits() == other.scroll_x.to_bits()
            && self.scroll_y.to_bits() == other.scroll_y.to_bits()
            && self.page_views == other.page_views
            && self.cover_alone == other.cover_alone
            && self.split_view == other.split_view
    }
}

impl Eq for RestartView {}

impl Default for RestartView {
    fn default() -> Self {
        Self {
            current_page: 0,
            mode: PageViewMode::Continuous,
            zoom: RestartZoom::FitWidth,
            scroll_x: 0.,
            scroll_y: 0.,
            page_views: Vec::new(),
            cover_alone: false,
            split_view: false,
        }
    }
}

impl RestartView {
    pub const fn new(
        current_page: u32,
        mode: PageViewMode,
        zoom: RestartZoom,
        scroll_x: f32,
        scroll_y: f32,
    ) -> Self {
        Self {
            current_page,
            mode,
            zoom,
            scroll_x,
            scroll_y,
            page_views: Vec::new(),
            cover_alone: false,
            split_view: false,
        }
    }

    /// Two Pages view's "Cover page alone".
    pub const fn with_cover_alone(mut self, cover_alone: bool) -> Self {
        self.cover_alone = cover_alone;
        self
    }

    /// Whether the document shows in two panes.
    pub const fn with_split_view(mut self, split_view: bool) -> Self {
        self.split_view = split_view;
        self
    }

    pub const fn cover_alone(&self) -> bool {
        self.cover_alone
    }

    pub const fn split_view(&self) -> bool {
        self.split_view
    }

    /// Single Page view's remembered pages, the oldest visit first. Only the
    /// last [`MAX_REMEMBERED_PAGE_VIEWS`] are kept, one for each page.
    pub fn with_page_views(mut self, page_views: Vec<(u32, RememberedPageView)>) -> Self {
        self.page_views = bounded_page_views(page_views);
        self
    }

    pub const fn current_page(&self) -> u32 {
        self.current_page
    }

    pub const fn mode(&self) -> PageViewMode {
        self.mode
    }

    pub const fn zoom(&self) -> RestartZoom {
        self.zoom
    }

    pub const fn scroll(&self) -> (f32, f32) {
        (self.scroll_x, self.scroll_y)
    }

    pub fn page_views(&self) -> &[(u32, RememberedPageView)] {
        &self.page_views
    }

    pub fn is_finite(&self) -> bool {
        zoom_is_finite(self.zoom)
            && self.scroll_x.is_finite()
            && self.scroll_y.is_finite()
            && self.page_views.iter().all(|(_, view)| view.is_finite())
    }
}

/// One entry for each page, its latest, and the last
/// [`MAX_REMEMBERED_PAGE_VIEWS`] of them.
fn bounded_page_views(
    page_views: Vec<(u32, RememberedPageView)>,
) -> Vec<(u32, RememberedPageView)> {
    let mut bounded: Vec<(u32, RememberedPageView)> = Vec::with_capacity(page_views.len());
    for (page, view) in page_views {
        bounded.retain(|(kept, _)| *kept != page);
        bounded.push((page, view));
    }
    let excess = bounded.len().saturating_sub(MAX_REMEMBERED_PAGE_VIEWS);
    bounded.drain(..excess);
    bounded
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CadViewOrganisation {
    Columns,
    Rows,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentNavigationAction {
    Home,
    End,
    PreviousPage,
    NextPage,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    PageUp,
    PageDown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DocumentNavigationOutcome {
    Page(usize),
    Scroll { x: f32, y: f32 },
    None,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WheelOutcome {
    NativeScroll,
    Page(usize),
    Zoom(f32),
    Consumed,
}

/// How far a two-finger swipe travels past a page's edge to turn the page in
/// Single Page view: enough that resting fingers do not turn it.
const SINGLE_PAGE_SWIPE_STEP: f32 = 40.;

/// Scroll events this far apart belong to different swipes. Not every
/// platform says when fingers land, and momentum keeps events coming for as
/// long as a swipe carries on.
const SINGLE_PAGE_SWIPE_IDLE: Duration = Duration::from_millis(150);

/// One event of a trackpad or touch mouse scroll.
#[derive(Clone, Copy, Debug)]
pub struct PreciseScroll {
    pub page_count: usize,
    pub current_page: usize,
    pub delta: (f32, f32),
    /// The page goes on past the view in the direction of `delta`.
    pub page_continues: bool,
    /// Fingers have just landed, which also stops any momentum.
    pub starts_gesture: bool,
    pub control: bool,
    pub reverse_zoom: bool,
    pub at: Instant,
}

/// One event of a discrete mouse wheel.
#[derive(Clone, Copy, Debug)]
pub struct DiscreteWheel {
    pub page_count: usize,
    pub current_page: usize,
    /// In pixels, a line each [`crate::mouse_wheel::WHEEL_LINE`].
    pub delta: (f32, f32),
    /// The page goes on past the view in the direction of `delta`.
    pub page_continues: bool,
    pub control: bool,
    pub reverse_zoom: bool,
    pub at: Instant,
    pub page_turning: PageTurning,
    pub zoom_speed: ZoomSpeed,
}

/// Progress of a two-finger swipe in Single Page view. A heading is the axis
/// (vertical or not) and whether the delta is negative.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct SinglePageSwipe {
    last_event: Option<Instant>,
    heading: Option<(bool, bool)>,
    travel: f32,
    /// The way this swipe last scrolled the page: run into the page's edge,
    /// it stops there rather than carrying on to turn the page.
    scrolled: Option<(bool, bool)>,
    /// A swipe turns one page, however long it and its momentum go on.
    turned: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativeDocumentViewState {
    mode: PageViewMode,
    zoom_preset: ViewerZoomPreset,
    zoom_percent: f32,
    continuous_wheel: WheelBehavior,
    single_page_wheel: WheelBehavior,
    two_pages_wheel: WheelBehavior,
    /// Two Pages view shows the first page alone, as a book's cover.
    cover_alone: bool,
    viewport_size: Option<(f32, f32)>,
    scroll: (f32, f32),
    wheel_turns: WheelTurns,
    single_page_swipe: SinglePageSwipe,
    cad_view_active: bool,
    cad_organisation: CadViewOrganisation,
    pages_per_lane: usize,
    /// Single Page view's remembered pages, the oldest visit first.
    page_views: Vec<(u32, RememberedPageView)>,
}

impl Default for NativeDocumentViewState {
    fn default() -> Self {
        Self {
            mode: PageViewMode::Continuous,
            zoom_preset: ViewerZoomPreset::FitWidth,
            zoom_percent: 100.,
            continuous_wheel: WheelBehavior::Scroll,
            single_page_wheel: WheelBehavior::Zoom,
            two_pages_wheel: WheelBehavior::Scroll,
            cover_alone: false,
            viewport_size: None,
            scroll: (0., 0.),
            wheel_turns: WheelTurns::default(),
            single_page_swipe: SinglePageSwipe::default(),
            cad_view_active: false,
            cad_organisation: CadViewOrganisation::Columns,
            pages_per_lane: 10,
            page_views: Vec::new(),
        }
    }
}

impl NativeDocumentViewState {
    pub const fn mode(&self) -> PageViewMode {
        self.mode
    }
    pub const fn zoom_preset(&self) -> ViewerZoomPreset {
        self.zoom_preset
    }
    pub const fn zoom_percent(&self) -> f32 {
        self.zoom_percent
    }
    pub const fn viewport_size(&self) -> Option<(f32, f32)> {
        self.viewport_size
    }
    pub const fn scroll(&self) -> (f32, f32) {
        self.scroll
    }

    pub fn restart_view(&self, current_page: u32) -> RestartView {
        let (scroll_x, scroll_y) = self.restart_scroll();
        // The page shown keeps its view in the restart view itself; it is
        // remembered again when Single Page view leaves it.
        RestartView::new(
            current_page,
            self.mode,
            self.zoom_intent(),
            scroll_x,
            scroll_y,
        )
        .with_page_views(self.page_views.clone())
        .with_cover_alone(self.cover_alone)
    }

    fn zoom_intent(&self) -> RestartZoom {
        match self.zoom_preset {
            ViewerZoomPreset::Manual => RestartZoom::Manual(self.zoom_percent),
            ViewerZoomPreset::FitWidth => RestartZoom::FitWidth,
            ViewerZoomPreset::FitPage => RestartZoom::FitPage,
        }
    }

    /// Adding zero turns a negative zero, which centring can leave, into
    /// zero: the stored view compares scroll bit for bit.
    fn restart_scroll(&self) -> (f32, f32) {
        (self.scroll.0 + 0., self.scroll.1 + 0.)
    }

    pub fn apply_restart_view(&mut self, view: &RestartView, page_count: usize) -> u32 {
        self.set_mode(view.mode());
        self.cover_alone = view.cover_alone();
        self.apply_zoom_intent(view.zoom());
        let (scroll_x, scroll_y) = view.scroll();
        self.set_scroll(scroll_x, scroll_y);
        self.page_views = view
            .page_views()
            .iter()
            .filter(|(page, view)| (*page as usize) < page_count && view.is_finite())
            .copied()
            .collect();
        view.current_page().min(page_count.saturating_sub(1) as u32)
    }

    fn apply_zoom_intent(&mut self, zoom: RestartZoom) {
        match zoom {
            RestartZoom::FitWidth => self.set_fit_preset(ViewerFitPreset::Width),
            RestartZoom::FitPage => self.set_fit_preset(ViewerFitPreset::Page),
            RestartZoom::Manual(percent) => self.set_manual_zoom(percent),
        }
    }

    /// Single Page view, outside CAD View, remembers each page's view.
    fn remembers_pages(&self) -> bool {
        self.mode == PageViewMode::SinglePage && !self.cad_view_active
    }

    fn shown_page_view(&self) -> RememberedPageView {
        let (scroll_x, scroll_y) = self.restart_scroll();
        RememberedPageView::new(self.zoom_intent(), scroll_x, scroll_y)
    }

    /// Single Page view leaves `page`: remembers its zoom and scroll.
    pub fn remember_page_view(&mut self, page: u32) {
        if !self.remembers_pages() {
            return;
        }
        let view = self.shown_page_view();
        self.page_views.retain(|(kept, _)| *kept != page);
        self.page_views.push((page, view));
        let excess = self
            .page_views
            .len()
            .saturating_sub(MAX_REMEMBERED_PAGE_VIEWS);
        self.page_views.drain(..excess);
    }

    /// The pages to keep for this file after its tab closes: the pages
    /// remembered, and the page shown, which is remembered only when Single
    /// Page view leaves it.
    pub fn page_views_for_memory(&self, current_page: u32) -> Vec<(u32, RememberedPageView)> {
        let mut state = self.clone();
        state.remember_page_view(current_page);
        state.page_views
    }

    /// A file's pages remembered from an earlier tab. Pages past the
    /// document's last, and views that are not finite, are left out.
    pub fn restore_page_views(
        &mut self,
        page_views: &[(u32, RememberedPageView)],
        page_count: usize,
    ) {
        self.page_views = bounded_page_views(
            page_views
                .iter()
                .filter(|(page, view)| (*page as usize) < page_count && view.is_finite())
                .copied()
                .collect(),
        );
    }

    pub fn page_view(&self, page: u32) -> Option<RememberedPageView> {
        self.page_views
            .iter()
            .find(|(kept, _)| *kept == page)
            .map(|(_, view)| *view)
    }

    pub fn remembered_pages(&self) -> impl Iterator<Item = u32> + '_ {
        self.page_views.iter().map(|(page, _)| *page)
    }

    /// Fit Width or Fit Page chosen for `page`, or the page turned: its
    /// view is no longer the one remembered.
    pub fn forget_page_view(&mut self, page: u32) {
        self.page_views.retain(|(kept, _)| *kept != page);
    }

    /// Pages moved, turned or went: `moved` gives where each remembered
    /// page is now, or `None` to forget it.
    pub fn remap_page_views(&mut self, moved: impl Fn(u32) -> Option<u32>) {
        let page_views = std::mem::take(&mut self.page_views)
            .into_iter()
            .filter_map(|(page, view)| moved(page).map(|page| (page, view)))
            .collect();
        self.page_views = bounded_page_views(page_views);
    }

    /// Single Page view shows `page`: takes its remembered zoom and returns
    /// its remembered scroll. A page not remembered keeps the zoom in force,
    /// fitted again to its size when a fit is in force, and returns `None`.
    /// `fit_to` is the viewport and the page's size.
    pub fn enter_page(
        &mut self,
        page: u32,
        fit_to: Option<((f32, f32), (f32, f32))>,
    ) -> Option<(f32, f32)> {
        let remembered = self.page_view(page);
        if let Some(view) = remembered {
            self.apply_zoom_intent(view.zoom());
        }
        if let Some(((width, height), page_size)) = fit_to {
            self.update_viewport(width, height, page_size);
        }
        remembered.map(RememberedPageView::scroll)
    }

    pub const fn cad_view_active(&self) -> bool {
        self.cad_view_active
    }

    pub const fn cad_organisation(&self) -> CadViewOrganisation {
        self.cad_organisation
    }

    pub const fn pages_per_lane(&self) -> usize {
        self.pages_per_lane
    }

    pub fn activate_cad_view(&mut self) {
        self.mode = PageViewMode::Continuous;
        self.zoom_preset = ViewerZoomPreset::Manual;
        self.cad_view_active = true;
        self.wheel_turns.reset();
    }

    pub fn set_cad_organisation(&mut self, organisation: CadViewOrganisation) {
        self.cad_organisation = organisation;
    }

    pub fn set_pages_per_lane(&mut self, count: f64) {
        self.pages_per_lane = if count.is_finite() {
            (count.round() as isize).clamp(1, 100) as usize
        } else {
            10
        };
    }

    pub const fn wheel_behavior(&self, mode: PageViewMode) -> WheelBehavior {
        match mode {
            PageViewMode::Continuous => self.continuous_wheel,
            PageViewMode::SinglePage => self.single_page_wheel,
            PageViewMode::TwoPages => self.two_pages_wheel,
        }
    }

    /// Two Pages view shows the first page alone.
    pub const fn cover_alone(&self) -> bool {
        self.cover_alone
    }

    /// Where Previous (`forward` false) or Next page goes from `page`: one
    /// page on, or in Two Pages view the first page of the row before or
    /// after. `None` at either end.
    pub fn page_step(&self, page: u32, page_count: usize, forward: bool) -> Option<u32> {
        if self.mode == PageViewMode::TwoPages && !self.cad_view_active {
            return crate::viewer::two_page_spread_step(
                page as usize,
                self.cover_alone,
                page_count,
                forward,
            )
            .and_then(|page| u32::try_from(page).ok());
        }
        if forward {
            let next = page.checked_add(1)?;
            ((next as usize) < page_count).then_some(next)
        } else {
            page.checked_sub(1)
        }
    }

    /// Where Last page goes: the last page, or in Two Pages view the first
    /// page of the last row.
    pub fn last_page_target(&self, page_count: usize) -> u32 {
        let last = page_count.saturating_sub(1);
        if self.mode == PageViewMode::TwoPages && !self.cad_view_active {
            crate::viewer::two_page_row_first(last, self.cover_alone) as u32
        } else {
            last as u32
        }
    }

    pub fn set_cover_alone(&mut self, cover_alone: bool) {
        self.cover_alone = cover_alone;
    }

    pub fn set_mode(&mut self, mode: PageViewMode) {
        if self.mode != mode {
            self.mode = mode;
            self.wheel_turns.reset();
        }
        if mode != PageViewMode::Continuous {
            self.cad_view_active = false;
        }
    }

    pub fn set_wheel_behavior(&mut self, mode: PageViewMode, behavior: WheelBehavior) {
        match mode {
            PageViewMode::Continuous => self.continuous_wheel = behavior,
            PageViewMode::SinglePage => self.single_page_wheel = behavior,
            PageViewMode::TwoPages => self.two_pages_wheel = behavior,
        }
        self.wheel_turns.reset();
    }

    pub fn set_manual_zoom(&mut self, zoom_percent: f32) {
        self.zoom_preset = ViewerZoomPreset::Manual;
        self.zoom_percent = clamp_zoom_percent(zoom_percent);
    }

    pub fn set_fit_preset(&mut self, preset: ViewerFitPreset) {
        self.zoom_preset = match preset {
            ViewerFitPreset::Width => ViewerZoomPreset::FitWidth,
            ViewerFitPreset::Page => ViewerZoomPreset::FitPage,
        };
    }

    pub fn update_viewport(&mut self, width: f32, height: f32, page_size: (f32, f32)) {
        self.viewport_size = Some((width, height));
        let preset = match self.zoom_preset {
            ViewerZoomPreset::Manual => return,
            ViewerZoomPreset::FitWidth => ViewerFitPreset::Width,
            ViewerZoomPreset::FitPage => ViewerFitPreset::Page,
        };
        // Fit Page in Two Pages keeps the row's paper edges in view: the
        // current page's outline and a dark page's ring lie outside them.
        let room = if self.mode == PageViewMode::TwoPages && preset == ViewerFitPreset::Page {
            2. * FIT_PAGE_EDGE_ROOM
        } else {
            0.
        };
        self.zoom_percent = resolve_fit_zoom_percent(
            preset,
            (width - room).max(1.),
            (height - room).max(1.),
            page_size.0,
            page_size.1,
        );
    }

    /// Scroll is negative while the viewport is panned past the first page.
    pub fn set_scroll(&mut self, x: f32, y: f32) {
        let finite = |value: f32| if value.is_finite() { value } else { 0. };
        self.scroll = (finite(x), finite(y));
    }

    pub fn wheel(&mut self, wheel: DiscreteWheel) -> WheelOutcome {
        let DiscreteWheel {
            page_count,
            current_page,
            delta: (delta_x, delta_y),
            page_continues,
            control,
            reverse_zoom,
            at,
            page_turning,
            zoom_speed,
        } = wheel;
        let behavior = self.wheel_behavior(self.mode);
        let should_scroll = !self.cad_view_active
            && match behavior {
                WheelBehavior::Scroll => !control,
                WheelBehavior::Zoom => control,
            };
        if !should_scroll {
            self.wheel_turns.reset();
            // A wheel movement that would scroll up zooms in (Maps, Revu), so
            // zoom follows whatever direction LinearMouse or natural
            // scrolling gives the scroll. Reverse Scroll Zoom flips it.
            let delta = if reverse_zoom { -delta_y } else { delta_y };
            let next = self.zoom_percent * zoom_speed.zoom_ratio(delta);
            self.set_manual_zoom(next);
            return WheelOutcome::Zoom(self.zoom_percent);
        }
        if self.mode.scrolls() {
            self.wheel_turns.reset();
            return WheelOutcome::NativeScroll;
        }
        let dominant = if delta_y.abs() >= delta_x.abs() {
            delta_y
        } else {
            delta_x
        };
        // Page turning decides when a notch turns: see `mouse_wheel`. Turns
        // count from the page last asked for, and no turn waits for a later
        // frame, so the wheel never goes on turning after it stops.
        match self
            .wheel_turns
            .event(dominant, at, page_continues, page_turning)
        {
            NotchAction::Ignore => WheelOutcome::Consumed,
            NotchAction::Scroll => WheelOutcome::NativeScroll,
            NotchAction::Turn(direction) => {
                let target = current_page
                    .saturating_add_signed(direction)
                    .min(page_count.saturating_sub(1));
                if target == current_page {
                    WheelOutcome::Consumed
                } else {
                    WheelOutcome::Page(target)
                }
            }
        }
    }

    /// Precise scrolling (trackpads and touch mice) pans in every view mode,
    /// and in Single Page a swipe past the page's edge turns the page. Ctrl
    /// zooms. None of it depends on the mouse-wheel preference.
    pub fn precise_scroll(&mut self, scroll: PreciseScroll) -> WheelOutcome {
        self.wheel_turns.reset();
        let (delta_x, delta_y) = scroll.delta;
        if scroll.control {
            self.single_page_swipe = SinglePageSwipe::default();
            return self.zoom_from_scroll(delta_y, scroll.reverse_zoom);
        }
        if self.mode != PageViewMode::SinglePage || self.cad_view_active {
            return WheelOutcome::NativeScroll;
        }
        let swipe = &mut self.single_page_swipe;
        let continues_swipe = swipe
            .last_event
            .is_some_and(|last| scroll.at.duration_since(last) < SINGLE_PAGE_SWIPE_IDLE);
        if scroll.starts_gesture || !continues_swipe {
            *swipe = SinglePageSwipe::default();
        }
        swipe.last_event = Some(scroll.at);
        if swipe.turned {
            return WheelOutcome::Consumed;
        }
        let vertical = delta_y.abs() >= delta_x.abs();
        let dominant = if vertical { delta_y } else { delta_x };
        if !dominant.is_finite() || dominant.abs() <= f32::EPSILON {
            return WheelOutcome::Consumed;
        }
        let heading = (vertical, dominant < 0.);
        if scroll.page_continues {
            swipe.scrolled = Some(heading);
            swipe.travel = 0.;
            return WheelOutcome::NativeScroll;
        }
        if swipe.scrolled == Some(heading) {
            return WheelOutcome::Consumed;
        }
        if swipe.heading != Some(heading) {
            swipe.heading = Some(heading);
            swipe.travel = 0.;
        }
        swipe.travel += dominant.abs();
        if swipe.travel < SINGLE_PAGE_SWIPE_STEP {
            return WheelOutcome::Consumed;
        }
        swipe.turned = true;
        // Like the scroll it stands in for: a delta that would scroll down
        // (negative) turns to the next page.
        let target = if dominant < 0. {
            scroll
                .current_page
                .saturating_add(1)
                .min(scroll.page_count.saturating_sub(1))
        } else {
            scroll.current_page.saturating_sub(1)
        };
        if target == scroll.current_page {
            WheelOutcome::Consumed
        } else {
            WheelOutcome::Page(target)
        }
    }

    /// Ctrl and a trackpad scroll: zooms continuously with the fingers,
    /// whatever the Zoom speed, as a pinch does.
    fn zoom_from_scroll(&mut self, delta_y: f32, reverse_zoom: bool) -> WheelOutcome {
        self.wheel_turns.reset();
        // A scroll up zooms in; Reverse Scroll Zoom flips it.
        let delta = if reverse_zoom { -delta_y } else { delta_y }.clamp(-120., 120.);
        let next = self.zoom_percent / 100. * (delta * 0.00165).exp();
        self.set_manual_zoom(next * 100.);
        WheelOutcome::Zoom(self.zoom_percent)
    }

    /// A trackpad pinch: `delta` is the gesture's magnification step
    /// (0.1 = 10% larger). Returns the new zoom percentage.
    pub fn pinch(&mut self, delta: f32) -> f32 {
        if delta.is_finite() {
            self.wheel_turns.reset();
            self.set_manual_zoom(self.zoom_percent * (1. + delta.clamp(-0.5, 1.)));
        }
        self.zoom_percent
    }

    pub fn keyboard(
        &self,
        action: DocumentNavigationAction,
        page_count: usize,
        current_page: usize,
        viewport_height: f32,
    ) -> DocumentNavigationOutcome {
        if page_count == 0 {
            return DocumentNavigationOutcome::None;
        }
        // Two Pages view turns a spread: to the first page of the row
        // before or after, and End to the first page of the last row.
        if self.mode == PageViewMode::TwoPages && !self.cad_view_active {
            let spread = |forward| {
                crate::viewer::two_page_spread_step(
                    current_page,
                    self.cover_alone,
                    page_count,
                    forward,
                )
                .map_or(
                    DocumentNavigationOutcome::None,
                    DocumentNavigationOutcome::Page,
                )
            };
            match action {
                DocumentNavigationAction::PreviousPage => return spread(false),
                DocumentNavigationAction::NextPage => return spread(true),
                DocumentNavigationAction::End => {
                    return DocumentNavigationOutcome::Page(crate::viewer::two_page_row_first(
                        page_count - 1,
                        self.cover_alone,
                    ));
                }
                _ => {}
            }
        }
        match action {
            DocumentNavigationAction::Home => DocumentNavigationOutcome::Page(0),
            DocumentNavigationAction::End => DocumentNavigationOutcome::Page(page_count - 1),
            DocumentNavigationAction::PreviousPage => current_page.checked_sub(1).map_or(
                DocumentNavigationOutcome::None,
                DocumentNavigationOutcome::Page,
            ),
            DocumentNavigationAction::NextPage => {
                let next = current_page.saturating_add(1);
                if next < page_count {
                    DocumentNavigationOutcome::Page(next)
                } else {
                    DocumentNavigationOutcome::None
                }
            }
            DocumentNavigationAction::ArrowUp => {
                DocumentNavigationOutcome::Scroll { x: 0., y: -48. }
            }
            DocumentNavigationAction::ArrowDown => {
                DocumentNavigationOutcome::Scroll { x: 0., y: 48. }
            }
            DocumentNavigationAction::ArrowLeft => {
                DocumentNavigationOutcome::Scroll { x: -48., y: 0. }
            }
            DocumentNavigationAction::ArrowRight => {
                DocumentNavigationOutcome::Scroll { x: 48., y: 0. }
            }
            DocumentNavigationAction::PageUp => DocumentNavigationOutcome::Scroll {
                x: 0.,
                y: -viewport_height.mul_add(0.9, 0.).max(48.),
            },
            DocumentNavigationAction::PageDown => DocumentNavigationOutcome::Scroll {
                x: 0.,
                y: viewport_height.mul_add(0.9, 0.).max(48.),
            },
        }
    }
}

pub(crate) fn clamp_zoom_percent(zoom_percent: f32) -> f32 {
    if !zoom_percent.is_finite() {
        return 6.25;
    }
    ((zoom_percent.clamp(6.25, 6_400.) * 10.).round()) / 10.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_resize_manual_zoom_and_per_mode_wheel_settings_are_retained() {
        let mut view = NativeDocumentViewState::default();
        view.update_viewport(1_000., 560., (612., 792.));
        assert_eq!(view.zoom_percent(), 163.3);
        view.set_fit_preset(ViewerFitPreset::Page);
        view.update_viewport(1_000., 560., (612., 792.));
        assert_eq!(view.zoom_percent(), 70.7);
        view.update_viewport(840., 560., (612., 792.));
        assert_eq!(view.zoom_percent(), 70.7);
        view.set_manual_zoom(125.);
        view.update_viewport(400., 300., (612., 792.));
        assert_eq!(view.zoom_percent(), 125.);
        assert_eq!(
            view.wheel_behavior(PageViewMode::Continuous),
            WheelBehavior::Scroll
        );
        assert_eq!(
            view.wheel_behavior(PageViewMode::SinglePage),
            WheelBehavior::Zoom
        );
    }

    /// A discrete wheel event at `at`, on a page that fits the view.
    fn notch(current_page: usize, delta_y: f32, control: bool, at: Instant) -> DiscreteWheel {
        DiscreteWheel {
            page_count: 4,
            current_page,
            delta: (0., delta_y),
            page_continues: false,
            control,
            reverse_zoom: false,
            at,
            page_turning: PageTurning::Responsive,
            zoom_speed: ZoomSpeed::Standard,
        }
    }

    const NOTCH: f32 = crate::mouse_wheel::WHEEL_LINE * crate::mouse_wheel::NOTCH_LINES;

    #[test]
    fn wheel_inversion_threshold_zoom_and_edges_match_the_frozen_contract() {
        let mut view = NativeDocumentViewState::default();
        let at = Instant::now();
        assert_eq!(
            view.wheel(notch(1, 20., false, at)),
            WheelOutcome::NativeScroll
        );
        // Ctrl and one notch up zooms in by the Zoom speed's ratio.
        assert!(
            matches!(view.wheel(notch(1, NOTCH, true, at)), WheelOutcome::Zoom(zoom) if (zoom - 120.).abs() < 0.1)
        );
        view.set_mode(PageViewMode::SinglePage);
        view.set_wheel_behavior(PageViewMode::SinglePage, WheelBehavior::Scroll);
        // Turning the wheel down (a negative delta, as it scrolls down)
        // goes to the next page; part of a notch waits for the rest.
        let later = |ms: u64| at + Duration::from_millis(ms);
        assert_eq!(
            view.wheel(notch(1, -NOTCH / 2., false, later(100))),
            WheelOutcome::Consumed
        );
        assert_eq!(
            view.wheel(notch(1, -NOTCH / 2., false, later(110))),
            WheelOutcome::Page(2)
        );
        // One notch turns one page, however hard the wheel is spun.
        assert_eq!(
            view.wheel(notch(1, -NOTCH * 3., false, later(200))),
            WheelOutcome::Page(2)
        );
        assert_eq!(
            view.wheel(notch(2, NOTCH, false, later(300))),
            WheelOutcome::Page(1)
        );
        // Part of a notch one way does not hold back a turn the other way.
        assert_eq!(
            view.wheel(notch(2, -NOTCH / 2., false, later(400))),
            WheelOutcome::Consumed
        );
        assert_eq!(
            view.wheel(notch(2, NOTCH, false, later(500))),
            WheelOutcome::Page(1)
        );
        // No page past the first or the last.
        assert_eq!(
            view.wheel(notch(3, -NOTCH, false, later(600))),
            WheelOutcome::Consumed
        );
        assert_eq!(
            view.wheel(notch(0, NOTCH, false, later(700))),
            WheelOutcome::Consumed
        );
        // A page larger than the view scrolls first.
        assert_eq!(
            view.wheel(DiscreteWheel {
                page_continues: true,
                ..notch(2, -NOTCH, false, later(800))
            }),
            WheelOutcome::NativeScroll
        );
        assert!(matches!(
            view.wheel(notch(2, NOTCH, true, later(900))),
            WheelOutcome::Zoom(_)
        ));
    }

    #[test]
    fn every_wheel_zoom_follows_the_zoom_speed() {
        let at = Instant::now();
        for (speed, factor) in [
            (ZoomSpeed::Fine, 1.10),
            (ZoomSpeed::Standard, 1.20),
            (ZoomSpeed::Coarse, 1.35),
        ] {
            // Ctrl and the wheel in Continuous view, and the plain wheel
            // where it zooms (Single Page set to Zoom, and CAD View).
            let mut continuous = NativeDocumentViewState::default();
            let mut single_page = NativeDocumentViewState::default();
            single_page.set_mode(PageViewMode::SinglePage);
            let mut cad = NativeDocumentViewState::default();
            cad.activate_cad_view();
            for (view, control) in [
                (&mut continuous, true),
                (&mut single_page, false),
                (&mut cad, false),
            ] {
                view.set_manual_zoom(100.);
                let outcome = view.wheel(DiscreteWheel {
                    zoom_speed: speed,
                    ..notch(1, NOTCH, control, at)
                });
                assert!(
                    matches!(outcome, WheelOutcome::Zoom(zoom) if (zoom - factor * 100.).abs() < 0.1),
                    "{speed:?} {outcome:?}"
                );
                let outcome = view.wheel(DiscreteWheel {
                    zoom_speed: speed,
                    ..notch(1, -NOTCH, control, at)
                });
                assert!(
                    matches!(outcome, WheelOutcome::Zoom(zoom) if (zoom - 100.).abs() < 0.1),
                    "{speed:?} {outcome:?}"
                );
            }
        }
    }

    fn precise(delta_y: f32, control: bool, at: Instant) -> PreciseScroll {
        PreciseScroll {
            page_count: 5,
            current_page: 2,
            delta: (0., delta_y),
            page_continues: true,
            starts_gesture: false,
            control,
            reverse_zoom: false,
            at,
        }
    }

    #[test]
    fn precise_scroll_is_independent_of_mouse_preferences_and_cad() {
        let mut view = NativeDocumentViewState::default();
        let at = Instant::now();
        for mode in [PageViewMode::Continuous, PageViewMode::SinglePage] {
            view.set_mode(mode);
            for behavior in [WheelBehavior::Scroll, WheelBehavior::Zoom] {
                view.set_wheel_behavior(mode, behavior);
                let before = view.zoom_percent();
                assert_eq!(
                    view.precise_scroll(precise(-120., false, at)),
                    WheelOutcome::NativeScroll
                );
                assert_eq!(view.zoom_percent(), before);
                assert!(
                    matches!(view.precise_scroll(precise(120., true, at)), WheelOutcome::Zoom(zoom) if zoom > before)
                );
            }
        }
        view.activate_cad_view();
        assert_eq!(
            view.precise_scroll(precise(-120., false, at)),
            WheelOutcome::NativeScroll
        );
        assert!(matches!(
            view.wheel(DiscreteWheel {
                page_count: 5,
                ..notch(2, NOTCH, false, at)
            }),
            WheelOutcome::Zoom(_)
        ));
    }

    /// A scroll in Single Page at the page's edge.
    fn at_edge(delta_y: f32, at: Instant) -> PreciseScroll {
        PreciseScroll {
            page_continues: false,
            ..precise(delta_y, false, at)
        }
    }

    fn single_page_view() -> NativeDocumentViewState {
        let mut view = NativeDocumentViewState::default();
        view.set_mode(PageViewMode::SinglePage);
        view
    }

    #[test]
    fn a_swipe_turns_one_page_once_it_has_travelled_far_enough() {
        let mut view = single_page_view();
        let at = Instant::now();
        assert_eq!(
            view.precise_scroll(at_edge(-30., at)),
            WheelOutcome::Consumed
        );
        assert_eq!(
            view.precise_scroll(at_edge(-30., at)),
            WheelOutcome::Page(3)
        );

        // Fingers still moving, then momentum: the one turn is all it gets.
        assert_eq!(
            view.precise_scroll(at_edge(-200., at)),
            WheelOutcome::Consumed
        );
        // A page the momentum could scroll is left alone too.
        assert_eq!(
            view.precise_scroll(precise(-200., false, at)),
            WheelOutcome::Consumed
        );

        // New fingers on the trackpad are a new swipe, the other way here.
        assert_eq!(
            view.precise_scroll(PreciseScroll {
                starts_gesture: true,
                ..at_edge(0., at)
            }),
            WheelOutcome::Consumed
        );
        assert_eq!(view.precise_scroll(at_edge(60., at)), WheelOutcome::Page(1));
    }

    #[test]
    fn a_pause_ends_a_swipe_where_the_platform_does_not_report_fingers() {
        let mut view = single_page_view();
        let at = Instant::now();
        assert_eq!(
            view.precise_scroll(at_edge(-60., at)),
            WheelOutcome::Page(3)
        );
        let during = at + SINGLE_PAGE_SWIPE_IDLE / 2;
        assert_eq!(
            view.precise_scroll(at_edge(-60., during)),
            WheelOutcome::Consumed
        );
        let later = during + SINGLE_PAGE_SWIPE_IDLE;
        assert_eq!(
            view.precise_scroll(at_edge(-60., later)),
            WheelOutcome::Page(3)
        );

        // A nudge that never got far enough is forgotten after a pause.
        let later = later + SINGLE_PAGE_SWIPE_IDLE;
        view.precise_scroll(at_edge(-30., later));
        let later = later + SINGLE_PAGE_SWIPE_IDLE;
        assert_eq!(
            view.precise_scroll(at_edge(-30., later)),
            WheelOutcome::Consumed
        );
    }

    #[test]
    fn a_swipe_that_scrolls_to_the_page_edge_stops_there() {
        let mut view = single_page_view();
        let at = Instant::now();
        assert_eq!(
            view.precise_scroll(precise(-200., false, at)),
            WheelOutcome::NativeScroll
        );
        assert_eq!(
            view.precise_scroll(at_edge(-200., at)),
            WheelOutcome::Consumed
        );
        assert_eq!(
            view.precise_scroll(PreciseScroll {
                starts_gesture: true,
                ..at_edge(-60., at)
            }),
            WheelOutcome::Page(3)
        );

        // A stray movement sideways does not count as scrolling to the edge.
        let mut view = single_page_view();
        assert_eq!(
            view.precise_scroll(PreciseScroll {
                delta: (1., 0.),
                ..precise(0., false, at)
            }),
            WheelOutcome::NativeScroll
        );
        assert_eq!(
            view.precise_scroll(at_edge(-60., at)),
            WheelOutcome::Page(3)
        );
    }

    #[test]
    fn a_swipe_stops_at_the_first_and_last_pages_and_goes_across() {
        let mut view = single_page_view();
        let at = Instant::now();
        assert_eq!(
            view.precise_scroll(PreciseScroll {
                current_page: 4,
                ..at_edge(-60., at)
            }),
            WheelOutcome::Consumed
        );
        assert_eq!(
            view.precise_scroll(PreciseScroll {
                current_page: 0,
                starts_gesture: true,
                ..at_edge(60., at)
            }),
            WheelOutcome::Consumed
        );
        assert_eq!(
            view.precise_scroll(PreciseScroll {
                delta: (-60., 2.),
                starts_gesture: true,
                ..at_edge(0., at)
            }),
            WheelOutcome::Page(3)
        );
    }

    #[test]
    fn reverse_scroll_zoom_flips_only_the_zoom_direction() {
        let mut view = NativeDocumentViewState::default();
        let at = Instant::now();
        assert!(
            matches!(view.wheel(DiscreteWheel { reverse_zoom: true, ..notch(1, NOTCH, true, at) }), WheelOutcome::Zoom(zoom) if (zoom - 83.3).abs() < 0.1)
        );
        assert_eq!(
            view.wheel(DiscreteWheel {
                reverse_zoom: true,
                ..notch(1, 20., false, at)
            }),
            WheelOutcome::NativeScroll
        );
    }

    fn remembered(view: &NativeDocumentViewState, page: u32) -> Option<(RestartZoom, (f32, f32))> {
        view.page_view(page)
            .map(|view| (view.zoom(), view.scroll()))
    }

    #[test]
    fn single_page_view_remembers_each_page_and_restores_it() {
        let mut view = single_page_view();
        view.update_viewport(800., 600., (612., 792.));
        // Page 0 at 250% scrolled down and across.
        view.set_manual_zoom(250.);
        view.set_scroll(120., 900.);
        view.remember_page_view(0);
        // Page 1, not seen before, keeps the zoom; the caller shows it
        // from its edge.
        assert_eq!(view.enter_page(1, Some(((800., 600.), (612., 792.)))), None);
        assert_eq!(view.zoom_percent(), 250.);
        view.set_manual_zoom(80.);
        view.set_scroll(0., 0.);
        view.remember_page_view(1);
        // Back to page 0: its zoom and scroll.
        assert_eq!(
            view.enter_page(0, Some(((800., 600.), (612., 792.)))),
            Some((120., 900.))
        );
        assert_eq!(view.zoom_percent(), 250.);
        assert_eq!(view.zoom_preset(), ViewerZoomPreset::Manual);
        // A page seen under Fit Page comes back fitted to the view now.
        view.set_fit_preset(ViewerFitPreset::Page);
        view.update_viewport(800., 600., (612., 792.));
        view.set_scroll(-10., 0.);
        view.remember_page_view(0);
        assert_eq!(
            view.enter_page(1, Some(((1_000., 600.), (612., 792.)))),
            Some((0., 0.))
        );
        assert_eq!(view.zoom_percent(), 80.);
        assert_eq!(
            view.enter_page(0, Some(((1_000., 600.), (612., 792.)))),
            Some((-10., 0.))
        );
        assert_eq!(view.zoom_preset(), ViewerZoomPreset::FitPage);
        assert_eq!(view.zoom_percent(), 75.7);
        // A fit in force fits a page not seen before to its own size.
        assert_eq!(
            view.enter_page(2, Some(((1_000., 600.), (1_224., 792.)))),
            None
        );
        assert_eq!(view.zoom_percent(), 75.7);
        assert_eq!(
            view.enter_page(3, Some(((1_000., 600.), (2_448., 792.)))),
            None
        );
        assert_eq!(view.zoom_percent(), 40.8);
        // Choosing a fit forgets the page's view.
        view.forget_page_view(1);
        assert_eq!(remembered(&view, 1), None);
    }

    #[test]
    fn continuous_and_cad_views_remember_no_page_and_keep_the_memory() {
        let mut view = single_page_view();
        view.set_manual_zoom(300.);
        view.set_scroll(5., 6.);
        view.remember_page_view(2);
        view.set_mode(PageViewMode::Continuous);
        view.set_manual_zoom(100.);
        view.remember_page_view(3);
        assert_eq!(remembered(&view, 3), None);
        view.activate_cad_view();
        view.remember_page_view(3);
        assert_eq!(remembered(&view, 3), None);
        view.set_mode(PageViewMode::SinglePage);
        assert_eq!(
            remembered(&view, 2),
            Some((RestartZoom::Manual(300.), (5., 6.)))
        );
    }

    #[test]
    fn page_memory_is_bounded_follows_moved_pages_and_forgets_turned_ones() {
        let mut view = single_page_view();
        for page in 0..(MAX_REMEMBERED_PAGE_VIEWS as u32 + 10) {
            view.set_scroll(page as f32, 0.);
            view.remember_page_view(page);
        }
        assert_eq!(view.remembered_pages().count(), MAX_REMEMBERED_PAGE_VIEWS);
        // The oldest visits are forgotten first.
        assert_eq!(remembered(&view, 9), None);
        assert!(remembered(&view, 10).is_some());
        // A page seen again is the newest visit.
        view.remember_page_view(10);
        assert_eq!(view.remembered_pages().last(), Some(10));

        // Page 10 deleted, 11 turned: the rest move up one.
        view.remap_page_views(|page| match page {
            10 | 11 => None,
            page if page > 11 => Some(page - 1),
            page => Some(page),
        });
        assert_eq!(remembered(&view, 10), None);
        assert_eq!(
            remembered(&view, 11),
            Some((RestartZoom::FitWidth, (12., 0.)))
        );
        assert_eq!(
            view.remembered_pages().count(),
            MAX_REMEMBERED_PAGE_VIEWS - 2
        );
    }

    #[test]
    fn page_memory_survives_a_restart_view_with_the_page_shown() {
        let mut view = single_page_view();
        view.set_manual_zoom(200.);
        view.set_scroll(10., 20.);
        view.remember_page_view(4);
        view.set_fit_preset(ViewerFitPreset::Width);
        view.set_scroll(0., 300.);
        let restart = view.restart_view(1);
        assert_eq!(
            restart.page_views(),
            &[(
                4,
                RememberedPageView::new(RestartZoom::Manual(200.), 10., 20.)
            )]
        );
        assert_eq!(restart.scroll(), (0., 300.));
        let mut reopened = NativeDocumentViewState::default();
        assert_eq!(reopened.apply_restart_view(&restart, 5), 1);
        assert_eq!(
            remembered(&reopened, 4),
            Some((RestartZoom::Manual(200.), (10., 20.)))
        );
        assert_eq!(reopened.restart_view(1), restart);
        // Page 4 is past the end of a three-page document.
        let mut shorter = NativeDocumentViewState::default();
        shorter.apply_restart_view(&restart, 3);
        assert_eq!(remembered(&shorter, 4), None);
    }

    #[test]
    fn pinching_scales_the_zoom_by_its_magnification() {
        let mut view = NativeDocumentViewState::default();
        let start = view.zoom_percent();
        assert!((view.pinch(0.1) - start * 1.1).abs() < 0.01);
        assert!((view.pinch(-0.1) - start * 1.1 * 0.9).abs() < 0.01);
        let before = view.zoom_percent();
        assert_eq!(view.pinch(f32::NAN), before);
    }

    #[test]
    fn keyboard_navigation_does_not_wrap_and_uses_exact_scroll_distances() {
        let view = NativeDocumentViewState::default();
        assert_eq!(
            view.keyboard(DocumentNavigationAction::Home, 4, 2, 600.),
            DocumentNavigationOutcome::Page(0)
        );
        assert_eq!(
            view.keyboard(DocumentNavigationAction::End, 4, 2, 600.),
            DocumentNavigationOutcome::Page(3)
        );
        assert_eq!(
            view.keyboard(DocumentNavigationAction::PreviousPage, 4, 0, 600.),
            DocumentNavigationOutcome::None
        );
        assert_eq!(
            view.keyboard(DocumentNavigationAction::NextPage, 4, 3, 600.),
            DocumentNavigationOutcome::None
        );
        assert_eq!(
            view.keyboard(DocumentNavigationAction::ArrowRight, 4, 2, 600.),
            DocumentNavigationOutcome::Scroll { x: 48., y: 0. }
        );
        assert_eq!(
            view.keyboard(DocumentNavigationAction::PageDown, 4, 2, 600.),
            DocumentNavigationOutcome::Scroll { x: 0., y: 540. }
        );
    }

    #[test]
    fn cad_view_state_is_document_owned_clamped_and_forces_continuous_manual_zoom() {
        let mut view = NativeDocumentViewState::default();
        view.set_mode(PageViewMode::SinglePage);
        view.set_fit_preset(ViewerFitPreset::Page);

        view.activate_cad_view();
        assert!(view.cad_view_active());
        assert_eq!(view.mode(), PageViewMode::Continuous);
        assert_eq!(view.zoom_preset(), ViewerZoomPreset::Manual);
        assert_eq!(view.cad_organisation(), CadViewOrganisation::Columns);
        assert_eq!(view.pages_per_lane(), 10);

        view.set_cad_organisation(CadViewOrganisation::Rows);
        view.set_pages_per_lane(0.);
        assert_eq!(view.cad_organisation(), CadViewOrganisation::Rows);
        assert_eq!(view.pages_per_lane(), 1);
        view.set_pages_per_lane(101.);
        assert_eq!(view.pages_per_lane(), 100);

        view.set_mode(PageViewMode::SinglePage);
        assert!(!view.cad_view_active());
    }
}
