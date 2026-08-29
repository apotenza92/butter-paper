use crate::{
    document_viewer::{ViewerFitPreset, resolve_fit_zoom_percent},
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

/// The core reader view persisted for one durable PDF path.
///
/// Page indices are zero-based. Page, zoom, and scroll values are clamped
/// against the successfully opened document before they become live state.
#[derive(Clone, Copy, Debug)]
pub struct RestartView {
    current_page: u32,
    mode: PageViewMode,
    zoom: RestartZoom,
    scroll_x: f32,
    scroll_y: f32,
}

impl PartialEq for RestartView {
    fn eq(&self, other: &Self) -> bool {
        self.current_page == other.current_page
            && self.mode == other.mode
            && self.zoom == other.zoom
            && self.scroll_x.to_bits() == other.scroll_x.to_bits()
            && self.scroll_y.to_bits() == other.scroll_y.to_bits()
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
        }
    }

    pub const fn current_page(self) -> u32 {
        self.current_page
    }

    pub const fn mode(self) -> PageViewMode {
        self.mode
    }

    pub const fn zoom(self) -> RestartZoom {
        self.zoom
    }

    pub const fn scroll(self) -> (f32, f32) {
        (self.scroll_x, self.scroll_y)
    }

    pub fn is_finite(self) -> bool {
        let zoom_is_finite = match self.zoom {
            RestartZoom::FitWidth | RestartZoom::FitPage => true,
            RestartZoom::Manual(percent) => percent.is_finite(),
        };
        zoom_is_finite && self.scroll_x.is_finite() && self.scroll_y.is_finite()
    }
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

#[derive(Clone, Debug, PartialEq)]
pub struct NativeDocumentViewState {
    mode: PageViewMode,
    zoom_preset: ViewerZoomPreset,
    zoom_percent: f32,
    continuous_wheel: WheelBehavior,
    single_page_wheel: WheelBehavior,
    viewport_size: Option<(f32, f32)>,
    scroll: (f32, f32),
    single_page_wheel_delta: f32,
    cad_view_active: bool,
    cad_organisation: CadViewOrganisation,
    pages_per_lane: usize,
}

impl Default for NativeDocumentViewState {
    fn default() -> Self {
        Self {
            mode: PageViewMode::Continuous,
            zoom_preset: ViewerZoomPreset::FitWidth,
            zoom_percent: 100.,
            continuous_wheel: WheelBehavior::Scroll,
            single_page_wheel: WheelBehavior::Zoom,
            viewport_size: None,
            scroll: (0., 0.),
            single_page_wheel_delta: 0.,
            cad_view_active: false,
            cad_organisation: CadViewOrganisation::Columns,
            pages_per_lane: 10,
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
        let zoom = match self.zoom_preset {
            ViewerZoomPreset::Manual => RestartZoom::Manual(self.zoom_percent),
            ViewerZoomPreset::FitWidth => RestartZoom::FitWidth,
            ViewerZoomPreset::FitPage => RestartZoom::FitPage,
        };
        RestartView::new(current_page, self.mode, zoom, self.scroll.0, self.scroll.1)
    }

    pub fn apply_restart_view(&mut self, view: RestartView, page_count: usize) -> u32 {
        self.set_mode(view.mode());
        match view.zoom() {
            RestartZoom::FitWidth => self.set_fit_preset(ViewerFitPreset::Width),
            RestartZoom::FitPage => self.set_fit_preset(ViewerFitPreset::Page),
            RestartZoom::Manual(percent) => self.set_manual_zoom(percent),
        }
        let (scroll_x, scroll_y) = view.scroll();
        self.set_scroll(scroll_x, scroll_y);
        view.current_page().min(page_count.saturating_sub(1) as u32)
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
        self.single_page_wheel_delta = 0.;
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
        }
    }

    pub fn set_mode(&mut self, mode: PageViewMode) {
        if self.mode != mode {
            self.mode = mode;
            self.single_page_wheel_delta = 0.;
        }
        if mode != PageViewMode::Continuous {
            self.cad_view_active = false;
        }
    }

    pub fn set_wheel_behavior(&mut self, mode: PageViewMode, behavior: WheelBehavior) {
        match mode {
            PageViewMode::Continuous => self.continuous_wheel = behavior,
            PageViewMode::SinglePage => self.single_page_wheel = behavior,
        }
        self.single_page_wheel_delta = 0.;
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
        self.zoom_percent =
            resolve_fit_zoom_percent(preset, width, height, page_size.0, page_size.1);
    }

    pub fn set_scroll(&mut self, x: f32, y: f32) {
        self.scroll = (x.max(0.), y.max(0.));
    }

    pub fn wheel(
        &mut self,
        page_count: usize,
        current_page: usize,
        delta_x: f32,
        delta_y: f32,
        control: bool,
    ) -> WheelOutcome {
        let behavior = self.wheel_behavior(self.mode);
        let should_scroll = !self.cad_view_active
            && match behavior {
                WheelBehavior::Scroll => !control,
                WheelBehavior::Zoom => control,
            };
        if should_scroll {
            if self.mode == PageViewMode::Continuous {
                self.single_page_wheel_delta = 0.;
                return WheelOutcome::NativeScroll;
            }
            if page_count <= 1 {
                self.single_page_wheel_delta = 0.;
                return WheelOutcome::Consumed;
            }
            let dominant = if delta_y.abs() >= delta_x.abs() {
                delta_y
            } else {
                delta_x
            };
            if !dominant.is_finite() || dominant.abs() <= f32::EPSILON {
                return WheelOutcome::Consumed;
            }
            self.single_page_wheel_delta += dominant;
            if self.single_page_wheel_delta.abs() < 80. {
                return WheelOutcome::Consumed;
            }
            let direction = self.single_page_wheel_delta.signum() as isize;
            self.single_page_wheel_delta = 0.;
            let target = current_page
                .saturating_add_signed(direction)
                .min(page_count.saturating_sub(1));
            return if target == current_page {
                WheelOutcome::Consumed
            } else {
                WheelOutcome::Page(target)
            };
        }

        self.single_page_wheel_delta = 0.;
        let delta = delta_y.clamp(-120., 120.);
        let next = self.zoom_percent / 100. * (-delta * 0.00165).exp();
        self.set_manual_zoom(next * 100.);
        WheelOutcome::Zoom(self.zoom_percent)
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

fn clamp_zoom_percent(zoom_percent: f32) -> f32 {
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
        assert_eq!(view.zoom_percent(), 150.);
        view.set_fit_preset(ViewerFitPreset::Page);
        view.update_viewport(1_000., 560., (612., 792.));
        assert_eq!(view.zoom_percent(), 66.);
        view.update_viewport(840., 560., (612., 792.));
        assert_eq!(view.zoom_percent(), 66.);
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

    #[test]
    fn wheel_inversion_threshold_zoom_and_edges_match_the_frozen_contract() {
        let mut view = NativeDocumentViewState::default();
        assert_eq!(view.wheel(4, 1, 0., 20., false), WheelOutcome::NativeScroll);
        assert!(
            matches!(view.wheel(4, 1, 0., -120., true), WheelOutcome::Zoom(zoom) if (zoom - 121.9).abs() < 0.1)
        );
        view.set_mode(PageViewMode::SinglePage);
        view.set_wheel_behavior(PageViewMode::SinglePage, WheelBehavior::Scroll);
        assert_eq!(view.wheel(4, 1, 0., 79., false), WheelOutcome::Consumed);
        assert_eq!(view.wheel(4, 1, 0., 1., false), WheelOutcome::Page(2));
        assert_eq!(view.wheel(4, 3, 0., 80., false), WheelOutcome::Consumed);
        assert!(matches!(
            view.wheel(4, 2, 0., 120., true),
            WheelOutcome::Zoom(_)
        ));
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
