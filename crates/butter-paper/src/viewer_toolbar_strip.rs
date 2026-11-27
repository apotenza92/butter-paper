use std::{cell::Cell, rc::Rc};

use gpui::{
    App, Bounds, ClickEvent, Context, Entity, EventEmitter, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, prelude::FluentBuilder as _, rems,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, ElementExt as _, Icon, Selectable as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    h_flex,
    input::{Redo, Undo},
    menu::{DropdownMenu as _, PopupMenuItem},
};

use crate::accessible_button::accessible_icon_button;
use crate::cad_view_control::{CadViewControl, CadViewControlEvent};
use crate::continuous_view_control::ContinuousViewControl;
use crate::page_navigator::{PageNavigator, PageNavigatorFit};
use crate::page_view_control::{
    PageViewControl, PageViewControlEvent, PageViewMode, WheelBehavior,
};
use crate::pdf_signatures::{SignatureStatus, Verdict};
use crate::viewer_icons::{fit_page_icon, fit_width_icon};
use crate::zoom_control::ZoomControl;

pub const VIEWER_TOOLBAR_ID: &str = "viewer-toolbar";
/// The stock Lucide arrows for Undo and Redo, and the ruler the Scale
/// shows.
const UNDO_ICON: &str = "icons/undo-2.svg";
const REDO_ICON: &str = "icons/redo-2.svg";
const SCALE_ICON: &str = "icons/rail/ruler-dimension-line.svg";
pub const VIEWER_TOOLBAR_SCROLL_ID: &str = "viewer-toolbar-scroll";
pub const VIEWER_TOOLBAR_CONTENT_ID: &str = "viewer-toolbar-content";
pub const FIT_BUTTON_GROUP_ID: &str = "viewer-fit-controls";
pub const FIT_WIDTH_ID: &str = "viewer-fit-width";
pub const FIT_PAGE_ID: &str = "viewer-fit-page";
/// The Edit PDF toggle: an icon button, selected while Edit PDF is on.
pub const EDIT_PDF_ID: &str = "viewer-edit-pdf";
/// The page's Scale: one split button beside Edit PDF. Its icon half turns
/// the current page's scale on and off, and its caret drops down the Page
/// Scale panel.
pub const VIEWER_SCALE_ID: &str = "viewer-scale";
pub const VIEWER_SCALE_TOGGLE_ID: &str = "viewer-scale-toggle";
pub const VIEWER_SCALE_MENU_ID: &str = "viewer-scale-menu";
/// The scale shown beside the ruler while the page has one, in every form
/// but the two narrowest.
pub const VIEWER_SCALE_VALUE_ID: &str = "viewer-scale-value";
/// Undo and Redo, at the toolbar's leading edge.
pub const VIEWER_HISTORY_GROUP_ID: &str = "viewer-history";
pub const VIEWER_UNDO_ID: &str = "viewer-undo";
pub const VIEWER_REDO_ID: &str = "viewer-redo";
/// The key context whose Undo and Redo bindings the tooltips show: the
/// document workspace's, as the Edit menu's.
const HISTORY_BINDING_CONTEXT: &str = "DocumentWorkspace";
/// The greatest width of the scale shown beside the ruler, in rems; a longer
/// scale is cut short with an ellipsis, and the tooltip names it in full.
const SCALE_VALUE_REMS: f32 = 5.75;
/// A signed document's signatures button, at the trailing edge.
pub const VIEWER_SIGNATURES_ID: &str = "viewer-signatures";
/// The page navigator, zoom and page-layout controls, centred in the
/// toolbar.
pub const VIEWER_TOOLBAR_CONTROLS_ID: &str = "viewer-toolbar-controls";
/// The frames of the page navigator and of the zoom controls.
pub const VIEWER_PAGES_GROUP_ID: &str = "viewer-pages-group";
pub const VIEWER_ZOOM_GROUP_ID: &str = "viewer-zoom-group";
/// The slots Undo and Redo, and Edit PDF and the Scale, are pinned in, at
/// the toolbar's left and right edges.
pub const VIEWER_TOOLBAR_LEFT_ID: &str = "viewer-toolbar-left";
pub const VIEWER_TOOLBAR_RIGHT_ID: &str = "viewer-toolbar-right";
/// The fit and page-view controls, beside the zoom.
pub const VIEWER_PAGE_LAYOUT_ID: &str = "viewer-page-layout";
/// The left slot: its edge inset, Undo and Redo, and the gap before the
/// page navigator, in rems.
const HISTORY_SLOT_REMS: f32 = 4.5;
/// The right slot without the scale shown: the gap after the page layout,
/// Edit PDF, the Scale's ruler and caret, and the edge inset, in rems.
const EDIT_SCALE_SLOT_REMS: f32 = 7.;
/// The room the scale shown beside the ruler adds, in rems: the label at
/// its greatest width, the gap before it and the button's padding.
const SCALE_LABEL_REMS: f32 = 6.625;
/// The slot of the signatures button, a shield alone, in rems.
pub const TOOLBAR_ICON_SLOT_REMS: f32 = 2.5;
/// The zoom, fit and page-view controls other than the page navigator, with
/// the frames of the zoom and page groups and the gap before the zoom:
/// Continuous and Single Page, then Two Pages and the Split View toggle.
const VIEW_CONTROLS_REMS: f32 = 26.625 + VIEW_ADDITIONS_REMS;
/// The room Two Pages View's split button and the Split View toggle take,
/// each with its gap, in rems.
const VIEW_ADDITIONS_REMS: f32 = TWO_PAGES_SPLIT_REMS + SPLIT_TOGGLE_REMS;
const TWO_PAGES_SPLIT_REMS: f32 = 4.875;
const SPLIT_TOGGLE_REMS: f32 = 2.875;
/// The room Redo takes, which the narrowest form gives back.
const REDO_REMS: f32 = 2.;
/// The room Zoom Out and Zoom In take as buttons, in rems.
const ZOOM_STEPS_REMS: f32 = 4.625;
/// The room the Fit Width and Fit Page group takes, with its gap, in rems.
const FIT_GROUP_REMS: f32 = 5.1875;
/// The room merging the three page-view controls and the Split View toggle
/// into one menu button gives back, in rems.
const PAGE_VIEW_MERGE_REMS: f32 = 5.375 + VIEW_ADDITIONS_REMS;
/// The room the zoom menu gives back showing a magnifier for the zoom, in
/// rems.
const ZOOM_ICON_SAVING_REMS: f32 = 2.125;
/// The room halving the gaps between groups and the edge insets gives back,
/// in rems: the left and right insets, the centre controls' two insets and
/// their gaps either side of the zoom, and the gap between Edit PDF and the
/// Scale.
const HALF_GAPS_SAVING_REMS: f32 = 1.75;

/// The merged View menu button of the narrower forms.
pub const VIEWER_PAGE_VIEW_MENU_ID: &str = "viewer-page-view-menu";
/// The Split View toggle, after the page views.
pub const VIEWER_SPLIT_VIEW_ID: &str = "viewer-split-view";

/// What a toolbar of a given width has room for. The whole toolbar changes
/// form at once, from the widest to the narrowest. Each step gives up the
/// least-used control it can, and the scale shown stays until the view
/// controls are in menus; every function stays reachable by pointer and
/// keyboard:
/// - Full: the whole page navigator and the scale shown beside the ruler.
/// - Middle: the First, Previous, Next and Last page buttons go (the page
///   field and page keys turn pages).
/// - StepsInMenu: Zoom Out and Zoom In move into the zoom menu (their keys
///   still work).
/// - ViewMenu: the three page views, their mouse-wheel menus and Split
///   View merge into one View menu button.
/// - FitInMenu: Fit Width and Fit Page move into the zoom menu too.
/// - Compact: the scale shown goes (the status bar and the Scale's tooltip
///   name it).
/// - Narrowest: the zoom menu shows a magnifier for the zoom, the page
///   field the page in view alone, and the gaps are halved. Side panels
///   give way before the toolbar gets narrower than this form for the
///   widest document (signed, four digits).
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ViewerToolbarFit {
    Full,
    Middle,
    StepsInMenu,
    ViewMenu,
    FitInMenu,
    Compact,
    Narrowest,
}

impl ViewerToolbarFit {
    /// Every form, from the widest to the narrowest.
    pub const ALL: [Self; 7] = [
        Self::Full,
        Self::Middle,
        Self::StepsInMenu,
        Self::ViewMenu,
        Self::FitInMenu,
        Self::Compact,
        Self::Narrowest,
    ];

    fn navigator(self) -> PageNavigatorFit {
        match self {
            Self::Full => PageNavigatorFit::Full,
            Self::Narrowest => PageNavigatorFit::Current,
            _ => PageNavigatorFit::Indicator,
        }
    }

    /// Whether the Scale shows the page's scale beside its ruler.
    pub fn scale_shown(self) -> bool {
        self < Self::Compact
    }

    /// Whether Zoom Out and Zoom In are in the zoom menu.
    pub fn steps_in_menu(self) -> bool {
        self >= Self::StepsInMenu
    }

    /// Whether Fit Width and Fit Page are in the zoom menu.
    pub fn fit_in_menu(self) -> bool {
        self >= Self::FitInMenu
    }

    /// Whether the page-view controls are one menu button.
    pub fn view_menu(self) -> bool {
        self >= Self::ViewMenu
    }

    /// Whether the zoom menu shows a magnifier, the page field the page in
    /// view alone, and the gaps between groups are halved.
    pub fn narrowest(self) -> bool {
        self == Self::Narrowest
    }

    /// Whether Redo is hidden, and Shift+click on Undo redoes.
    pub fn redo_in_undo(self) -> bool {
        self == Self::Narrowest
    }
}

/// The right slot's room in `fit`, in rems: Edit PDF and the Scale, with
/// the scale shown where `fit` shows it.
const fn edit_scale_slot_rems(fit: ViewerToolbarFit) -> f32 {
    match fit {
        ViewerToolbarFit::Compact | ViewerToolbarFit::Narrowest => EDIT_SCALE_SLOT_REMS,
        _ => EDIT_SCALE_SLOT_REMS + SCALE_LABEL_REMS,
    }
}

/// The toolbar width, in rems, that shows the toolbar as `fit` says, with
/// the signatures button for a signed document, for a document of `digits`
/// page-count digits.
pub const fn viewer_toolbar_rems(fit: ViewerToolbarFit, signed: bool, digits: u32) -> f32 {
    use crate::page_navigator::{PAGE_NAVIGATOR_BUTTON_PAIR_REMS, indicator_room_rems};
    let (pairs, view_controls) = match fit {
        ViewerToolbarFit::Full => (2., VIEW_CONTROLS_REMS),
        ViewerToolbarFit::Middle => (0., VIEW_CONTROLS_REMS),
        ViewerToolbarFit::StepsInMenu => (0., VIEW_CONTROLS_REMS - ZOOM_STEPS_REMS),
        ViewerToolbarFit::ViewMenu => (
            0.,
            VIEW_CONTROLS_REMS - ZOOM_STEPS_REMS - PAGE_VIEW_MERGE_REMS,
        ),
        ViewerToolbarFit::FitInMenu | ViewerToolbarFit::Compact => (
            0.,
            VIEW_CONTROLS_REMS - ZOOM_STEPS_REMS - FIT_GROUP_REMS - PAGE_VIEW_MERGE_REMS,
        ),
        ViewerToolbarFit::Narrowest => (
            0.,
            VIEW_CONTROLS_REMS
                - ZOOM_STEPS_REMS
                - FIT_GROUP_REMS
                - PAGE_VIEW_MERGE_REMS
                - ZOOM_ICON_SAVING_REMS
                - HALF_GAPS_SAVING_REMS,
        ),
    };
    let signatures = if signed { TOOLBAR_ICON_SLOT_REMS } else { 0. };
    let page_field = if matches!(fit, ViewerToolbarFit::Narrowest) {
        crate::page_navigator::current_indicator_min_rems(digits) + 0.375
    } else {
        indicator_room_rems(digits)
    };
    let history = if matches!(fit, ViewerToolbarFit::Narrowest) {
        HISTORY_SLOT_REMS - REDO_REMS
    } else {
        HISTORY_SLOT_REMS
    };
    history
        + edit_scale_slot_rems(fit)
        + view_controls
        + page_field
        + pairs * PAGE_NAVIGATOR_BUTTON_PAIR_REMS
        + signatures
}

/// The toolbar width the least document window gives with no side panel
/// open, in rems. The document window's least size (918 points wide with
/// both rails) is set from it, and stays as it was before Undo, Redo and
/// the icon toggles came.
pub const VIEWER_TOOLBAR_MIN_REMS: f32 = 48.875;

/// The narrowest toolbar any document needs, in rems: the narrowest form
/// for a signed document of a four-digit page count. Side panels give way
/// before the toolbar gets narrower.
pub const VIEWER_TOOLBAR_NARROWEST_REMS: f32 = viewer_toolbar_rems(
    ViewerToolbarFit::Narrowest,
    true,
    crate::page_navigator::PAGE_NAVIGATOR_MAX_DIGITS,
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FitPreset {
    Width,
    Page,
}

/// What the Scale control shows for the current page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScaleToolbarState {
    /// The page's number, from 1.
    pub page_number: u32,
    /// Whether the page has a scale (or one is being chosen for it).
    pub on: bool,
    /// The page's scale; off, the one turning it on brings back.
    pub label: Option<SharedString>,
    /// Whether the Page Scale panel is open: the caret shows pressed.
    pub menu_open: bool,
}

/// What a press on the Scale control asks for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaleRequest {
    /// The icon half: the page's scale on or off.
    Toggle,
    /// The caret: the Page Scale panel.
    Menu,
}

// The toolbar's Undo and Redo. They act on the document's history, even
// when a text field has the focus (owner decision); Edit ▸ Undo and Redo
// and their keys go to the focused text field first.
gpui::actions!(viewer_toolbar, [UndoDocument, RedoDocument]);

/// Whether the toolbar's Undo and Redo are available: from the document's
/// history only.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HistoryState {
    pub can_undo: bool,
    pub can_redo: bool,
}

/// Handles a press on the Scale control, with the window it was made in.
pub type ScaleHandler = Rc<dyn Fn(ScaleRequest, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewerToolbarStripEvent {
    FitPresetChanged(FitPreset),
    /// The Edit PDF toggle was pressed.
    EditPdfToggled,
    /// The pointer entered or left the Scale control.
    ScaleHovered(bool),
    /// The signatures button was pressed.
    SignaturesRequested,
    /// The Split View toggle (or its item in the View menu) was pressed.
    SplitViewToggled,
}

/// Owns the representative viewer-toolbar feature state while GPUI Component
/// owns the presentation and interaction behavior of each control.
pub struct ViewerToolbarStrip {
    continuous_control: Entity<ContinuousViewControl>,
    single_page_control: Option<Entity<PageViewControl>>,
    /// Two Pages view, after Single Page.
    two_pages_control: Option<Entity<PageViewControl>>,
    /// The Split View toggle after the page views, and whether it is on;
    /// `None` hides it.
    split_view: Option<bool>,
    zoom_control: Option<Entity<ZoomControl>>,
    /// The page navigator, right of the zoom percentage.
    page_navigator: Option<Entity<PageNavigator>>,
    /// The digits of the active document's page count, which size the page
    /// field.
    page_digits: u32,
    cad_view_control: Option<Entity<CadViewControl>>,
    focus_handle: FocusHandle,
    fit_preset: FitPreset,
    fit_selected: bool,
    fit_changes: usize,
    page_view_mode: PageViewMode,
    disabled: bool,
    /// The Edit PDF toggle, pinned right, and whether it is on; `None`
    /// hides it.
    edit_pdf: Option<bool>,
    /// Whether Undo and Redo are available, as the Edit menu has them.
    history: HistoryState,
    /// A signed document's signatures, which show the signatures button.
    signatures: Option<SignatureStatus>,
    /// The current page's Scale, beside Edit PDF; `None` hides it.
    scale: Option<ScaleToolbarState>,
    /// Where the Scale control was last laid out, in the window: the Page
    /// Scale panel drops down from it.
    scale_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    scale_handler: Option<ScaleHandler>,
    /// The toolbar's width as last laid out.
    width: Rc<Cell<Pixels>>,
}

impl ViewerToolbarStrip {
    pub fn new(continuous_control: Entity<ContinuousViewControl>, cx: &mut Context<Self>) -> Self {
        Self {
            continuous_control,
            single_page_control: None,
            two_pages_control: None,
            split_view: None,
            zoom_control: None,
            page_navigator: None,
            page_digits: 1,
            cad_view_control: None,
            focus_handle: cx.focus_handle(),
            fit_preset: FitPreset::Width,
            fit_selected: true,
            fit_changes: 0,
            page_view_mode: PageViewMode::Continuous,
            disabled: false,
            edit_pdf: None,
            history: HistoryState::default(),
            signatures: None,
            scale: None,
            scale_bounds: Rc::new(Cell::new(None)),
            scale_handler: None,
            width: Rc::new(Cell::new(Pixels::MAX)),
        }
    }

    pub fn new_paired(
        continuous_control: Entity<ContinuousViewControl>,
        single_page_control: Entity<PageViewControl>,
        cx: &mut Context<Self>,
    ) -> Self {
        let single_page_for_continuous = single_page_control.clone();
        cx.subscribe(
            &continuous_control,
            move |toolbar, _, event: &PageViewControlEvent, cx| {
                match event {
                    PageViewControlEvent::Activated(PageViewMode::Continuous) => {
                        toolbar.page_view_mode = PageViewMode::Continuous;
                        single_page_for_continuous.update(cx, |control, cx| {
                            control.set_selected(false, cx);
                        });
                    }
                    PageViewControlEvent::FitActivated(PageViewMode::Continuous) => {
                        toolbar.page_view_mode = PageViewMode::Continuous;
                        toolbar.fit_preset = FitPreset::Width;
                        toolbar.fit_selected = true;
                        toolbar.fit_changes = toolbar.fit_changes.saturating_add(1);
                        single_page_for_continuous.update(cx, |control, cx| {
                            control.set_selected(false, cx);
                        });
                    }
                    PageViewControlEvent::WheelBehaviorChanged(PageViewMode::Continuous, _) => {}
                    _ => return,
                }
                cx.notify();
            },
        )
        .detach();

        let continuous_for_single_page = continuous_control.clone();
        cx.subscribe(
            &single_page_control,
            move |toolbar, _, event: &PageViewControlEvent, cx| {
                match event {
                    PageViewControlEvent::Activated(PageViewMode::SinglePage) => {
                        toolbar.page_view_mode = PageViewMode::SinglePage;
                    }
                    PageViewControlEvent::FitActivated(PageViewMode::SinglePage) => {
                        toolbar.page_view_mode = PageViewMode::SinglePage;
                        toolbar.fit_preset = FitPreset::Page;
                        toolbar.fit_selected = true;
                        toolbar.fit_changes = toolbar.fit_changes.saturating_add(1);
                    }
                    PageViewControlEvent::WheelBehaviorChanged(PageViewMode::SinglePage, _) => {}
                    _ => return,
                }
                continuous_for_single_page.update(cx, |control, cx| {
                    control.set_selected(false, cx);
                });
                cx.notify();
            },
        )
        .detach();

        Self {
            continuous_control,
            single_page_control: Some(single_page_control),
            two_pages_control: None,
            split_view: None,
            zoom_control: None,
            page_navigator: None,
            page_digits: 1,
            cad_view_control: None,
            focus_handle: cx.focus_handle(),
            fit_preset: FitPreset::Width,
            fit_selected: true,
            fit_changes: 0,
            page_view_mode: PageViewMode::Continuous,
            disabled: false,
            edit_pdf: None,
            history: HistoryState::default(),
            signatures: None,
            scale: None,
            scale_bounds: Rc::new(Cell::new(None)),
            scale_handler: None,
            width: Rc::new(Cell::new(Pixels::MAX)),
        }
    }

    pub fn new_with_zoom(
        continuous_control: Entity<ContinuousViewControl>,
        single_page_control: Entity<PageViewControl>,
        zoom_control: Entity<ZoomControl>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut toolbar = Self::new_paired(continuous_control, single_page_control, cx);
        toolbar.zoom_control = Some(zoom_control);
        toolbar
    }

    pub fn new_with_cad_view(
        continuous_control: Entity<ContinuousViewControl>,
        single_page_control: Entity<PageViewControl>,
        zoom_control: Entity<ZoomControl>,
        cad_view_control: Entity<CadViewControl>,
        cx: &mut Context<Self>,
    ) -> Self {
        let cad_for_continuous = cad_view_control.clone();
        cx.subscribe(
            &continuous_control,
            move |_, _, event: &PageViewControlEvent, cx| {
                if *event == PageViewControlEvent::Activated(PageViewMode::Continuous) {
                    cad_for_continuous.update(cx, |control, cx| {
                        control.set_active(false, cx);
                    });
                }
            },
        )
        .detach();

        let cad_for_single_page = cad_view_control.clone();
        cx.subscribe(
            &single_page_control,
            move |_, _, event: &PageViewControlEvent, cx| {
                if *event == PageViewControlEvent::Activated(PageViewMode::SinglePage) {
                    cad_for_single_page.update(cx, |control, cx| {
                        control.set_active(false, cx);
                    });
                }
            },
        )
        .detach();

        let continuous_for_cad = continuous_control.clone();
        let single_page_for_cad = single_page_control.clone();
        cx.subscribe(
            &cad_view_control,
            move |toolbar, _, event: &CadViewControlEvent, cx| {
                if *event != CadViewControlEvent::Activated {
                    return;
                }
                toolbar.page_view_mode = PageViewMode::Continuous;
                toolbar.fit_selected = false;
                continuous_for_cad.update(cx, |control, cx| {
                    control.set_selected(false, cx);
                });
                single_page_for_cad.update(cx, |control, cx| {
                    control.set_selected(false, cx);
                });
                cx.notify();
            },
        )
        .detach();

        let mut toolbar =
            Self::new_with_zoom(continuous_control, single_page_control, zoom_control, cx);
        toolbar.cad_view_control = Some(cad_view_control);
        toolbar
    }

    /// Shows Two Pages view after Single Page. The three page views select
    /// one another out.
    pub fn set_two_pages_control(
        &mut self,
        two_pages_control: Entity<PageViewControl>,
        cx: &mut Context<Self>,
    ) {
        let others = [
            Some(self.continuous_control.clone()),
            self.single_page_control.clone(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        for other in others.clone() {
            let two_pages = two_pages_control.clone();
            cx.subscribe(&other, move |_, _, event: &PageViewControlEvent, cx| {
                if matches!(
                    event,
                    PageViewControlEvent::Activated(_) | PageViewControlEvent::FitActivated(_)
                ) {
                    two_pages.update(cx, |control, cx| control.set_selected(false, cx));
                }
            })
            .detach();
        }
        cx.subscribe(
            &two_pages_control,
            move |toolbar, _, event: &PageViewControlEvent, cx| {
                match event {
                    PageViewControlEvent::Activated(PageViewMode::TwoPages) => {
                        toolbar.page_view_mode = PageViewMode::TwoPages;
                    }
                    PageViewControlEvent::FitActivated(PageViewMode::TwoPages) => {
                        toolbar.page_view_mode = PageViewMode::TwoPages;
                        toolbar.fit_preset = FitPreset::Page;
                        toolbar.fit_selected = true;
                        toolbar.fit_changes = toolbar.fit_changes.saturating_add(1);
                    }
                    _ => return,
                }
                for other in &others {
                    other.update(cx, |control, cx| control.set_selected(false, cx));
                }
                cx.notify();
            },
        )
        .detach();
        two_pages_control.update(cx, |control, cx| {
            control.set_disabled(self.disabled, cx);
            control.set_selected(
                !self.disabled && self.page_view_mode == PageViewMode::TwoPages,
                cx,
            );
        });
        self.two_pages_control = Some(two_pages_control);
        cx.notify();
    }

    pub fn two_pages_control(&self) -> Option<&Entity<PageViewControl>> {
        self.two_pages_control.as_ref()
    }

    /// Shows the Split View toggle, on or off, or hides it.
    pub fn set_split_view(&mut self, split_view: Option<bool>, cx: &mut Context<Self>) {
        if self.split_view != split_view {
            self.split_view = split_view;
            cx.notify();
        }
    }

    pub fn split_view(&self) -> Option<bool> {
        self.split_view
    }

    /// Synchronizes Two Pages view's wheel and cover settings from the
    /// document without reporting them back.
    pub fn sync_two_pages(
        &mut self,
        wheel: WheelBehavior,
        cover_alone: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(control) = &self.two_pages_control {
            control.update(cx, |control, cx| {
                control.sync_wheel_behavior(wheel, cx);
                control.sync_cover_alone(cover_alone, cx);
            });
        }
    }

    /// Shows `navigator` right of the zoom percentage.
    pub fn set_page_navigator(&mut self, navigator: Entity<PageNavigator>, cx: &mut Context<Self>) {
        navigator.update(cx, |navigator, cx| {
            navigator.set_disabled(self.disabled, cx)
        });
        self.page_navigator = Some(navigator);
        cx.notify();
    }

    /// Two Pages view (with whether the cover shows alone), or `None`: the
    /// page navigator's Previous and Next turn a spread.
    pub fn sync_spreads(&mut self, spreads: Option<bool>, cx: &mut Context<Self>) {
        if let Some(navigator) = &self.page_navigator {
            navigator.update(cx, |navigator, cx| navigator.set_spreads(spreads, cx));
        }
    }

    /// Follows the active document's page and page count.
    pub fn sync_pages(&mut self, current_page: u32, page_count: u32, cx: &mut Context<Self>) {
        let digits = crate::page_navigator::page_count_digits(page_count);
        if self.page_digits != digits {
            self.page_digits = digits;
            cx.notify();
        }
        if let Some(navigator) = &self.page_navigator {
            navigator.update(cx, |navigator, cx| {
                navigator.set_pages(current_page, page_count, cx);
            });
        }
    }

    pub fn fit_preset(&self) -> FitPreset {
        self.fit_preset
    }

    pub fn fit_changes(&self) -> usize {
        self.fit_changes
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    pub fn page_view_mode(&self) -> PageViewMode {
        self.page_view_mode
    }

    pub fn wheel_behavior(&self, mode: PageViewMode, cx: &App) -> WheelBehavior {
        match mode {
            PageViewMode::Continuous => self.continuous_control.read(cx).wheel_behavior(),
            PageViewMode::SinglePage => self
                .single_page_control
                .as_ref()
                .map_or(WheelBehavior::Zoom, |control| {
                    control.read(cx).wheel_behavior()
                }),
            PageViewMode::TwoPages => self
                .two_pages_control
                .as_ref()
                .map_or(WheelBehavior::Scroll, |control| {
                    control.read(cx).wheel_behavior()
                }),
        }
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Shows the Edit PDF toggle, on or off, or hides it.
    pub fn set_edit_pdf(&mut self, edit_pdf: Option<bool>, cx: &mut Context<Self>) {
        if self.edit_pdf != edit_pdf {
            self.edit_pdf = edit_pdf;
            cx.notify();
        }
    }

    pub fn edit_pdf(&self) -> Option<bool> {
        self.edit_pdf
    }

    /// Enables Undo and Redo as the Edit menu's items are.
    pub fn set_history(&mut self, history: HistoryState, cx: &mut Context<Self>) {
        if self.history != history {
            self.history = history;
            cx.notify();
        }
    }

    pub fn history(&self) -> HistoryState {
        self.history
    }

    /// Shows the current page's Scale, or hides it.
    pub fn set_scale(&mut self, scale: Option<ScaleToolbarState>, cx: &mut Context<Self>) {
        if self.scale != scale {
            self.scale = scale;
            cx.notify();
        }
    }

    /// What handles a press on the Scale control.
    pub fn set_scale_handler(&mut self, handler: ScaleHandler) {
        self.scale_handler = Some(handler);
    }

    fn request_scale(&self, request: ScaleRequest, window: &mut Window, cx: &mut App) {
        if let Some(handler) = self.scale_handler.clone() {
            handler(request, window, cx);
        }
    }

    pub fn scale(&self) -> Option<&ScaleToolbarState> {
        self.scale.as_ref()
    }

    /// Where the Scale control was last laid out, in the window.
    pub fn scale_bounds(&self) -> Option<Bounds<Pixels>> {
        self.scale.as_ref().and(self.scale_bounds.get())
    }

    fn render_scale_control(
        &self,
        scale: ScaleToolbarState,
        fit: ViewerToolbarFit,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let page = scale.page_number;
        let toggle_tooltip: SharedString = match (&scale.label, scale.on) {
            (Some(label), true) => {
                format!(
                    "Page Scale on: page {page} at {label}. Click to remove page {page}'s scale."
                )
            }
            (Some(label), false) => {
                format!("Page Scale off. Click to bring back {label} on page {page}.")
            }
            (None, _) => format!("Page Scale off. Click to set page {page}'s scale."),
        }
        .into();
        let toggle_label: SharedString = match (&scale.label, scale.on) {
            (Some(label), true) => format!("Page Scale, on, {label}"),
            _ => "Page Scale, off".to_owned(),
        }
        .into();
        let menu_tooltip: SharedString = match &scale.label {
            Some(label) if scale.on => format!("Page {page}'s scale: {label}. Change it…"),
            _ => format!("Set page {page}'s scale…"),
        }
        .into();
        let menu_key_binding = crate::shortcut_label::shortcut_kbd("Shift+Alt+S");
        // The scale beside the ruler while the page has one, cut short with
        // an ellipsis when long.
        let shown = scale
            .label
            .clone()
            .filter(|_| scale.on && fit.scale_shown());
        let toggle = accessible_icon_button(
            Button::new(VIEWER_SCALE_TOGGLE_ID)
                .debug_selector(|| VIEWER_SCALE_TOGGLE_ID.into())
                .accessibility_id(VIEWER_SCALE_TOGGLE_ID)
                .icon(Icon::default().path(SCALE_ICON))
                .when_some(shown, |button, label| {
                    button.child(
                        div()
                            .id(VIEWER_SCALE_VALUE_ID)
                            .debug_selector(|| VIEWER_SCALE_VALUE_ID.into())
                            .max_w(rems(SCALE_VALUE_REMS))
                            .truncate()
                            .child(label),
                    )
                })
                .selected(scale.on)
                .disabled(disabled)
                .tooltip(toggle_tooltip)
                .on_click(cx.listener(|toolbar, _: &ClickEvent, window, cx| {
                    toolbar.request_scale(ScaleRequest::Toggle, window, cx);
                })),
            toggle_label,
        );
        let menu = accessible_icon_button(
            Button::new(VIEWER_SCALE_MENU_ID)
                .debug_selector(|| VIEWER_SCALE_MENU_ID.into())
                .accessibility_id(VIEWER_SCALE_MENU_ID)
                .dropdown_caret(true)
                // As a split button's halves: both show the scale on, and
                // the caret shows pressed while its panel is open.
                .selected(scale.on || scale.menu_open)
                .disabled(disabled)
                .map(|button| match menu_key_binding {
                    Some(key_binding) => button.tooltip_with_key_binding(menu_tooltip, key_binding),
                    None => button.tooltip(menu_tooltip),
                })
                .on_click(cx.listener(|toolbar, _: &ClickEvent, window, cx| {
                    toolbar.request_scale(ScaleRequest::Menu, window, cx);
                })),
            "Page Scale menu",
        );
        let anchor = self.scale_bounds.clone();
        h_flex()
            .id(VIEWER_SCALE_ID)
            .debug_selector(|| VIEWER_SCALE_ID.into())
            .flex_shrink_0()
            .role(gpui::Role::Group)
            .aria_label("Page Scale")
            .on_prepaint(move |bounds, _, _| anchor.set(Some(bounds)))
            .on_hover(cx.listener(|_, hovered: &bool, _, cx| {
                cx.emit(ViewerToolbarStripEvent::ScaleHovered(*hovered));
            }))
            // A split button of stock outline buttons, joined as the
            // page-view controls' halves are; the group keeps each half's
            // own disabled state.
            .child(
                ButtonGroup::new("viewer-scale-split")
                    .outline()
                    .children([toggle, menu]),
            )
            .into_any_element()
    }

    /// The Edit PDF toggle: an icon button, selected while Edit PDF is on,
    /// as Fit Width is while it applies.
    fn render_edit_pdf(&self, on: bool, disabled: bool, cx: &mut Context<Self>) -> Button {
        let button = Button::new(EDIT_PDF_ID)
            .debug_selector(|| EDIT_PDF_ID.into())
            .accessibility_id(EDIT_PDF_ID)
            .outline()
            .icon(Icon::default().path(crate::application_assets::EDIT_PDF_ICON))
            .selected(on)
            .toggled(on)
            .disabled(disabled)
            .map(
                |button| match crate::shortcut_label::shortcut_kbd("Shift+T") {
                    Some(key_binding) => button.tooltip_with_key_binding("Edit PDF", key_binding),
                    None => button.tooltip("Edit PDF"),
                },
            )
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.emit(ViewerToolbarStripEvent::EditPdfToggled);
            }));
        accessible_icon_button(button, if on { "Edit PDF, on" } else { "Edit PDF, off" })
    }

    /// Undo and Redo of the document: they send [`UndoDocument`] and
    /// [`RedoDocument`] from the toolbar, so a focused text field never
    /// takes them. A toolbar button takes no focus when pressed. In the
    /// narrowest form Redo goes, and Shift+click on Undo redoes; only then
    /// does Undo's tooltip say so.
    fn render_history(&self, fit: ViewerToolbarFit) -> ButtonGroup {
        let history = self.history;
        let focus = self.focus_handle.clone();
        let redo_focus = self.focus_handle.clone();
        let redo_in_undo = fit.redo_in_undo();
        let undo = Button::new(VIEWER_UNDO_ID)
            .debug_selector(|| VIEWER_UNDO_ID.into())
            .accessibility_id(VIEWER_UNDO_ID)
            .icon(Icon::default().path(UNDO_ICON))
            .map(|button| {
                if redo_in_undo {
                    button.tooltip_with_action(
                        "Undo. Shift+click to redo.",
                        &Undo,
                        Some(HISTORY_BINDING_CONTEXT),
                    )
                } else {
                    button.tooltip_with_action("Undo", &Undo, Some(HISTORY_BINDING_CONTEXT))
                }
            })
            .disabled(!history.can_undo && !(redo_in_undo && history.can_redo))
            .on_click(move |event: &ClickEvent, window, cx| {
                if redo_in_undo && event.modifiers().shift {
                    focus.dispatch_action(&RedoDocument, window, cx);
                } else if history.can_undo {
                    focus.dispatch_action(&UndoDocument, window, cx);
                }
            });
        let undo = accessible_icon_button(
            undo,
            if redo_in_undo {
                "Undo, Shift+click to redo"
            } else {
                "Undo"
            },
        );
        let group = ButtonGroup::new("viewer-history-buttons").outline();
        if redo_in_undo {
            return group.child(undo);
        }
        // `children` keeps each button's own disabled state.
        group.children([
            undo,
            accessible_icon_button(
                Button::new(VIEWER_REDO_ID)
                    .debug_selector(|| VIEWER_REDO_ID.into())
                    .accessibility_id(VIEWER_REDO_ID)
                    .icon(Icon::default().path(REDO_ICON))
                    .tooltip_with_action("Redo", &Redo, Some(HISTORY_BINDING_CONTEXT))
                    .disabled(!history.can_redo)
                    .on_click(move |_, window, cx| {
                        redo_focus.dispatch_action(&RedoDocument, window, cx);
                    }),
                "Redo",
            ),
        ])
    }

    /// Shows the signatures button for a signed document, or hides it.
    pub fn set_signatures(&mut self, signatures: Option<SignatureStatus>, cx: &mut Context<Self>) {
        if self.signatures != signatures {
            self.signatures = signatures;
            cx.notify();
        }
    }

    pub fn signatures(&self) -> Option<SignatureStatus> {
        self.signatures
    }

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if self.disabled == disabled {
            return;
        }
        self.disabled = disabled;
        self.continuous_control.update(cx, |control, cx| {
            control.set_disabled(disabled, cx);
        });
        if let Some(single_page_control) = &self.single_page_control {
            single_page_control.update(cx, |control, cx| {
                control.set_disabled(disabled, cx);
            });
        }
        if let Some(two_pages_control) = &self.two_pages_control {
            two_pages_control.update(cx, |control, cx| {
                control.set_disabled(disabled, cx);
            });
        }
        if let Some(zoom_control) = &self.zoom_control {
            zoom_control.update(cx, |control, cx| {
                control.set_disabled(disabled, cx);
            });
        }
        if let Some(cad_view_control) = &self.cad_view_control {
            cad_view_control.update(cx, |control, cx| {
                control.set_disabled(disabled, cx);
            });
        }
        if let Some(navigator) = &self.page_navigator {
            navigator.update(cx, |navigator, cx| navigator.set_disabled(disabled, cx));
        }
        if !disabled {
            let cad_view_active = self
                .cad_view_control
                .as_ref()
                .is_some_and(|control| control.read(cx).is_active());
            self.continuous_control.update(cx, |control, cx| {
                control.set_selected(
                    self.page_view_mode == PageViewMode::Continuous && !cad_view_active,
                    cx,
                );
            });
            if let Some(single_page_control) = &self.single_page_control {
                single_page_control.update(cx, |control, cx| {
                    control.set_selected(self.page_view_mode == PageViewMode::SinglePage, cx);
                });
            }
            if let Some(two_pages_control) = &self.two_pages_control {
                two_pages_control.update(cx, |control, cx| {
                    control.set_selected(self.page_view_mode == PageViewMode::TwoPages, cx);
                });
            }
        }
        cx.notify();
    }

    /// Synchronizes the toolbar from the active document without emitting
    /// user-intent events back to the workspace.
    #[allow(clippy::too_many_arguments)]
    pub fn sync_document_state(
        &mut self,
        mode: PageViewMode,
        fit_preset: Option<FitPreset>,
        continuous_wheel: WheelBehavior,
        single_page_wheel: WheelBehavior,
        zoom: f64,
        disabled: bool,
        cx: &mut Context<Self>,
    ) {
        let fit_selected = fit_preset.is_some();
        let fit_preset = fit_preset.unwrap_or(self.fit_preset);
        let changed = self.page_view_mode != mode
            || self.fit_preset != fit_preset
            || self.fit_selected != fit_selected;
        self.page_view_mode = mode;
        self.fit_preset = fit_preset;
        self.fit_selected = fit_selected;

        self.continuous_control.update(cx, |control, cx| {
            control.set_selected(!disabled && mode == PageViewMode::Continuous, cx);
            control.sync_wheel_behavior(continuous_wheel, cx);
        });
        if let Some(single_page_control) = &self.single_page_control {
            single_page_control.update(cx, |control, cx| {
                control.set_selected(!disabled && mode == PageViewMode::SinglePage, cx);
                control.sync_wheel_behavior(single_page_wheel, cx);
            });
        }
        if let Some(two_pages_control) = &self.two_pages_control {
            two_pages_control.update(cx, |control, cx| {
                control.set_selected(!disabled && mode == PageViewMode::TwoPages, cx);
            });
        }
        if let Some(zoom_control) = &self.zoom_control {
            zoom_control.update(cx, |control, cx| control.set_zoom(zoom, cx));
        }
        self.set_disabled(disabled, cx);
        if changed {
            cx.notify();
        }
    }

    pub fn sync_cad_document_state(
        &mut self,
        active: bool,
        organisation: crate::cad_view_control::CadViewOrganisation,
        pages_per_lane: usize,
        cx: &mut Context<Self>,
    ) {
        // The control notifies on change, which re-renders this toolbar too.
        if let Some(control) = &self.cad_view_control {
            control.update(cx, |control, cx| {
                control.sync_retained_state(active, organisation, pages_per_lane, cx);
            });
        }
    }

    /// The narrow forms' one View button: the view in use, and a menu of
    /// the three page views, Cover page alone, Split View and each view's
    /// mouse-wheel behaviour.
    fn render_page_view_menu(&self, disabled: bool, cx: &mut Context<Self>) -> gpui::AnyElement {
        let continuous = self.continuous_control.clone();
        let Some(single_page) = self.single_page_control.clone() else {
            return div().into_any_element();
        };
        let two_pages = self.two_pages_control.clone();
        let mode = self.page_view_mode;
        let icon_color = if disabled {
            cx.theme().muted_foreground.opacity(0.5)
        } else {
            cx.theme().button_foreground
        };
        let icon = match mode {
            PageViewMode::Continuous => {
                crate::viewer_icons::continuous_icon(icon_color).into_any_element()
            }
            PageViewMode::SinglePage => {
                crate::viewer_icons::single_page_icon(icon_color).into_any_element()
            }
            PageViewMode::TwoPages => {
                crate::viewer_icons::two_pages_icon(icon_color).into_any_element()
            }
        };
        let label = mode.label();
        let selected = continuous.read(cx).is_selected()
            || single_page.read(cx).is_selected()
            || two_pages
                .as_ref()
                .is_some_and(|control| control.read(cx).is_selected());
        let cover_alone = two_pages
            .as_ref()
            .is_some_and(|control| control.read(cx).cover_alone());
        let split_view = self.split_view;
        let tooltip: SharedString = match split_view {
            Some(true) => format!("View: {label}, Split View on"),
            _ => format!("View: {label}"),
        }
        .into();
        let accessible: SharedString = match split_view {
            Some(true) => format!("View, {label}, Split View on"),
            _ => format!("View, {label}"),
        }
        .into();
        let toolbar = cx.entity().downgrade();
        let controls = [
            (PageViewMode::Continuous, Some(continuous)),
            (PageViewMode::SinglePage, Some(single_page)),
            (PageViewMode::TwoPages, two_pages),
        ];
        accessible_icon_button(
            Button::new(VIEWER_PAGE_VIEW_MENU_ID)
                .debug_selector(|| VIEWER_PAGE_VIEW_MENU_ID.into())
                .accessibility_id(VIEWER_PAGE_VIEW_MENU_ID)
                .outline()
                .child(icon)
                .dropdown_caret(true)
                .tooltip(tooltip)
                .disabled(disabled),
            accessible,
        )
        .dropdown_menu(move |menu, window, cx| {
            let view = |control: Entity<PageViewControl>| {
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    control.update(cx, |control, cx| control.activate(cx));
                }
            };
            let mut menu = menu.check_side(gpui_component::Side::Right);
            for (view_mode, control) in controls.iter() {
                if let Some(control) = control {
                    menu = menu.item(
                        PopupMenuItem::new(view_mode.label())
                            .checked(selected && mode == *view_mode)
                            .on_click(view(control.clone())),
                    );
                }
            }
            menu = menu.separator();
            if let Some((_, Some(two_pages))) = controls.get(2) {
                let two_pages = two_pages.clone();
                menu = menu.item(
                    PopupMenuItem::new("Cover page alone")
                        .checked(cover_alone)
                        .on_click(move |_, _, cx| {
                            two_pages.update(cx, |control, cx| {
                                control.set_cover_alone(!cover_alone, cx);
                            });
                        }),
                );
            }
            if let Some(on) = split_view {
                let toolbar = toolbar.clone();
                menu = menu.item(PopupMenuItem::new("Split View").checked(on).on_click(
                    move |_, _, cx| {
                        let _ = toolbar.update(cx, |_, cx| {
                            cx.emit(ViewerToolbarStripEvent::SplitViewToggled);
                        });
                    },
                ));
            }
            menu = menu.separator();
            for (view_mode, control) in controls.iter() {
                let Some(control) = control.clone() else {
                    continue;
                };
                let behavior = control.read(cx).wheel_behavior();
                let label = format!("Mouse wheel in {}", view_mode.label());
                menu = menu.submenu(label, window, cx, move |submenu, _, _| {
                    let wheel = |control: Entity<PageViewControl>, behavior| {
                        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                            control
                                .update(cx, |control, cx| control.set_wheel_behavior(behavior, cx));
                        }
                    };
                    submenu
                        .check_side(gpui_component::Side::Right)
                        .item(
                            PopupMenuItem::new("Zoom")
                                .checked(behavior == WheelBehavior::Zoom)
                                .on_click(wheel(control.clone(), WheelBehavior::Zoom)),
                        )
                        .item(
                            PopupMenuItem::new("Scroll")
                                .checked(behavior == WheelBehavior::Scroll)
                                .on_click(wheel(control.clone(), WheelBehavior::Scroll)),
                        )
                });
            }
            menu
        })
        .into_any_element()
    }

    /// The Split View toggle: an icon button, selected while the document
    /// shows in two panes.
    fn render_split_view(&self, on: bool, disabled: bool, cx: &mut Context<Self>) -> Button {
        let icon_color = if disabled {
            cx.theme().muted_foreground.opacity(0.5)
        } else {
            cx.theme().button_foreground
        };
        let button = Button::new(VIEWER_SPLIT_VIEW_ID)
            .debug_selector(|| VIEWER_SPLIT_VIEW_ID.into())
            .accessibility_id(VIEWER_SPLIT_VIEW_ID)
            .outline()
            .child(crate::viewer_icons::split_view_icon(icon_color))
            .selected(on)
            .toggled(on)
            .disabled(disabled)
            .tooltip(if on {
                "Split View: two panes of this document. Click for one."
            } else {
                "Split View: show this document in two panes"
            })
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.emit(ViewerToolbarStripEvent::SplitViewToggled);
            }));
        accessible_icon_button(
            button,
            if on {
                "Split View, on"
            } else {
                "Split View, off"
            },
        )
    }

    fn select_fit_preset(&mut self, preset: FitPreset, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        self.fit_preset = preset;
        self.fit_selected = true;
        self.fit_changes = self.fit_changes.saturating_add(1);
        cx.emit(ViewerToolbarStripEvent::FitPresetChanged(preset));
        cx.notify();
    }

    fn handle_fit_click(&mut self, preset: FitPreset, event: &ClickEvent, cx: &mut Context<Self>) {
        let cad_active = self
            .cad_view_control
            .as_ref()
            .is_some_and(|control| control.read(cx).is_active());
        if self.disabled || cad_active {
            return;
        }

        if event.click_count() == 2 {
            let page_control = match preset {
                FitPreset::Width => Some(self.continuous_control.clone()),
                FitPreset::Page => self.single_page_control.clone(),
            };
            if let Some(page_control) = page_control {
                page_control.update(cx, |control, cx| control.activate_fit(cx));
            } else {
                self.select_fit_preset(preset, cx);
            }
        } else {
            self.select_fit_preset(preset, cx);
        }
    }
}

/// What a toolbar `width` wide has room for; `edit_and_scale` when it
/// shows the Edit PDF toggle and the Scale (a toolbar without them needs
/// less).
pub fn viewer_toolbar_fit(
    width: Pixels,
    rem: Pixels,
    signed: bool,
    digits: u32,
    edit_and_scale: bool,
) -> ViewerToolbarFit {
    ViewerToolbarFit::ALL
        .into_iter()
        .find(|fit| {
            let slot = if edit_and_scale {
                0.
            } else {
                edit_scale_slot_rems(*fit)
            };
            width >= rem * (viewer_toolbar_rems(*fit, signed, digits) - slot)
        })
        .unwrap_or(ViewerToolbarFit::Narrowest)
}

impl EventEmitter<ViewerToolbarStripEvent> for ViewerToolbarStrip {}

impl Render for ViewerToolbarStrip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.fit_preset;
        let disabled = self.disabled;
        let cad_active = self
            .cad_view_control
            .as_ref()
            .is_some_and(|control| control.read(cx).is_active());
        let fit_disabled = disabled || cad_active;
        let fit_selected = self.fit_selected && !cad_active;
        let icon_color = if disabled || cad_active {
            cx.theme().muted_foreground.opacity(0.5)
        } else {
            cx.theme().button_foreground
        };

        let fit_controls = ButtonGroup::new(FIT_BUTTON_GROUP_ID)
            .outline()
            // This revision propagates disabled state only to children added
            // after `disabled`, so the order is part of the compatibility seam.
            .disabled(fit_disabled)
            .child(accessible_icon_button(
                Button::new(FIT_WIDTH_ID)
                    .debug_selector(|| FIT_WIDTH_ID.into())
                    .accessibility_id(FIT_WIDTH_ID)
                    .tooltip("Fit Width. Double-click to view Continuous.")
                    .child(fit_width_icon(icon_color))
                    .selected(fit_selected && selected == FitPreset::Width)
                    .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                        this.handle_fit_click(FitPreset::Width, event, cx);
                    })),
                "Fit Width",
            ))
            .child(accessible_icon_button(
                Button::new(FIT_PAGE_ID)
                    .debug_selector(|| FIT_PAGE_ID.into())
                    .accessibility_id(FIT_PAGE_ID)
                    .tooltip("Fit Page. Double-click to view Single Page.")
                    .child(fit_page_icon(icon_color))
                    .selected(fit_selected && selected == FitPreset::Page)
                    .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                        this.handle_fit_click(FitPreset::Page, event, cx);
                    })),
                "Fit Page",
            ));

        let signed = self.signatures.is_some();
        let edit_and_scale = self.edit_pdf.is_some();

        let fit = viewer_toolbar_fit(
            self.width.get(),
            window.rem_size(),
            signed,
            self.page_digits,
            edit_and_scale,
        );
        // The narrowest form halves the gaps between groups and the edge
        // insets.
        let half = fit.narrowest();
        if let Some(navigator) = &self.page_navigator {
            navigator.update(cx, |navigator, cx| navigator.set_fit(fit.navigator(), cx));
        }
        if let Some(zoom) = &self.zoom_control {
            let toolbar = cx.entity().downgrade();
            let extras = crate::zoom_control::ZoomMenuExtras {
                steps: fit.steps_in_menu(),
                icon_only: fit == ViewerToolbarFit::Narrowest,
                fit: (fit.fit_in_menu() && !fit_disabled).then(|| {
                    crate::zoom_control::ZoomMenuFit {
                        selected: fit_selected.then_some(selected),
                        on_fit: Rc::new(move |preset, cx| {
                            let _ = toolbar.update(cx, |toolbar, cx| {
                                toolbar.select_fit_preset(preset, cx);
                            });
                        }),
                    }
                }),
            };
            zoom.update(cx, |zoom, cx| zoom.set_menu_extras(extras, cx));
        }
        let page_view_menu = (fit.view_menu() && self.single_page_control.is_some())
            .then(|| self.render_page_view_menu(disabled || cad_active, cx));
        let split_toggle = self
            .split_view
            .filter(|_| page_view_menu.is_none())
            .map(|on| self.render_split_view(on, disabled, cx));

        // Edit PDF and the page's Scale sit at the trailing edge, then the
        // signatures button; Undo and Redo at the leading edge. Flexible
        // spaces either side keep the page navigator, zoom and layout
        // centred.
        let edit_pdf = self.edit_pdf.map(|on| {
            let control = self.render_edit_pdf(on, disabled, cx);
            let scale = self
                .scale
                .clone()
                .map(|scale| self.render_scale_control(scale, fit, disabled, cx));
            h_flex()
                .map(|slot| {
                    if fit.narrowest() {
                        slot.gap_1()
                    } else {
                        slot.gap_2()
                    }
                })
                .flex_shrink_0()
                .whitespace_nowrap()
                .items_center()
                .child(control)
                .children(scale)
        });
        let has_right_controls = edit_pdf.is_some() || self.signatures.is_some();
        let history = self.render_history(fit);

        // The shield alone, coloured by the verdict; its tooltip says what
        // it means, and a click lists the signatures.
        let signatures = self.signatures.map(|status| {
            let theme = cx.theme();
            let (icon, color, label) = match status {
                SignatureStatus::Checking => (
                    crate::application_assets::SIGNATURE_UNVERIFIED_ICON,
                    theme.muted_foreground,
                    "Checking signatures…",
                ),
                SignatureStatus::Checked(Verdict::Valid) => (
                    crate::application_assets::SIGNATURE_VALID_ICON,
                    theme.success,
                    "Signed: the signatures are valid. Click for details.",
                ),
                SignatureStatus::Checked(Verdict::Unverified) => (
                    crate::application_assets::SIGNATURE_UNVERIFIED_ICON,
                    theme.warning,
                    "Signer not verified: a signer's identity can't be verified. Click for details.",
                ),
                SignatureStatus::Checked(Verdict::Invalid) => (
                    crate::application_assets::SIGNATURE_INVALID_ICON,
                    theme.danger,
                    "Signature invalid: the PDF changed after signing. Click for details.",
                ),
            };
            h_flex().flex_shrink_0().child(accessible_icon_button(
                Button::new(VIEWER_SIGNATURES_ID)
                    .debug_selector(|| VIEWER_SIGNATURES_ID.into())
                    .accessibility_id(VIEWER_SIGNATURES_ID)
                    .ghost()
                    .icon(Icon::default().path(icon).text_color(color))
                    .tooltip(label)
                    .disabled(disabled)
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                        cx.emit(ViewerToolbarStripEvent::SignaturesRequested);
                    })),
                label,
            ))
        });
        let width = self.width.clone();
        let page_digits = self.page_digits;
        let toolbar = cx.entity().downgrade();

        h_flex()
            .on_prepaint(move |bounds, window, cx| {
                // Whether the toolbar is compact follows the width laid
                // out.
                let band = |width: Pixels| {
                    viewer_toolbar_fit(
                        width,
                        window.rem_size(),
                        signed,
                        page_digits,
                        edit_and_scale,
                    )
                };
                let previous = band(width.replace(bounds.size.width));
                if previous != band(bounds.size.width) {
                    let toolbar = toolbar.clone();
                    window.defer(cx, move |_, cx| {
                        toolbar.update(cx, |_, cx| cx.notify()).ok();
                    });
                }
            })
            .id(VIEWER_TOOLBAR_ID)
            .debug_selector(|| VIEWER_TOOLBAR_ID.into())
            .tab_group()
            .w_full()
            // A bar: 8 + 32 + 8, its line drawn inside.
            .h(crate::layout_metrics::scaled(crate::layout_metrics::BAR))
            .flex_shrink_0()
            .min_w_0()
            .bg(cx.theme().background)
            .relative()
            .track_focus(&self.focus_handle)
            .child(crate::layout_metrics::inside_line(
                crate::layout_metrics::Edge::Bottom,
                cx.theme().border,
            ))
            .child(
                h_flex()
                    .id(VIEWER_TOOLBAR_SCROLL_ID)
                    .debug_selector(|| VIEWER_TOOLBAR_SCROLL_ID.into())
                    .w_full()
                    .min_w_0()
                    // Some form always fits: side panels give way before
                    // the toolbar gets narrower than the narrowest form.
                    // So the toolbar never scrolls; it clips only while a
                    // panel slides in or out.
                    .overflow_hidden()
                    .child(
                        h_flex()
                            .id(VIEWER_TOOLBAR_CONTENT_ID)
                            .debug_selector(|| VIEWER_TOOLBAR_CONTENT_ID.into())
                            .w_auto()
                            .min_w_full()
                            .flex_shrink_0()
                            .justify_center()
                            .items_center()
                            // Alike in every form: Undo and Redo pinned
                            // left, Edit PDF and the Scale pinned right, and
                            // the page navigator, zoom and page layout in the
                            // toolbar's centre. The two side slots share the
                            // free space alike (each grows from nothing but
                            // never below its content), so the controls stay
                            // centred until a wider side reaches them; then
                            // they sit between the sides, which meet them at
                            // the group gap.
                            .child(
                                h_flex()
                                    .id(VIEWER_TOOLBAR_LEFT_ID)
                                    .debug_selector(|| VIEWER_TOOLBAR_LEFT_ID.into())
                                    .flex_1()
                                    .items_center()
                                    .map(|slot| if half { slot.pl_1() } else { slot.pl_2() })
                                    .child(
                                        h_flex()
                                            .id(VIEWER_HISTORY_GROUP_ID)
                                            .debug_selector(|| VIEWER_HISTORY_GROUP_ID.into())
                                            .flex_shrink_0()
                                            .child(history),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .id(VIEWER_TOOLBAR_CONTROLS_ID)
                                    .debug_selector(|| VIEWER_TOOLBAR_CONTROLS_ID.into())
                                    .flex_shrink_0()
                                    .justify_center()
                                    .items_center()
                                    // The edge inset matches the gap between groups,
                                    // the grid's unit; the narrowest form halves
                                    // both.
                                    .map(|controls| {
                                        if half {
                                            controls.gap_1().px_1()
                                        } else {
                                            controls.gap_2().px_2()
                                        }
                                    })
                                    .py_1()
                                    .when_some(self.page_navigator.clone(), |toolbar, navigator| {
                                        toolbar.child(
                                            crate::toolbar_group::toolbar_group(cx)
                                                .id(VIEWER_PAGES_GROUP_ID)
                                                .debug_selector(|| VIEWER_PAGES_GROUP_ID.into())
                                                .flex_shrink_0()
                                                .child(navigator),
                                        )
                                    })
                                    .when_some(self.zoom_control.clone(), |toolbar, control| {
                                        toolbar.child(
                                            crate::toolbar_group::toolbar_group(cx)
                                                .id(VIEWER_ZOOM_GROUP_ID)
                                                .debug_selector(|| VIEWER_ZOOM_GROUP_ID.into())
                                                .flex_shrink_0()
                                                .child(control),
                                        )
                                    })
                                    .child(
                                        h_flex()
                                            .id(VIEWER_PAGE_LAYOUT_ID)
                                            .debug_selector(|| VIEWER_PAGE_LAYOUT_ID.into())
                                            .flex_shrink_0()
                                            .items_center()
                                            .map(
                                                |layout| {
                                                    if half {
                                                        layout.gap_1()
                                                    } else {
                                                        layout.gap_2()
                                                    }
                                                },
                                            )
                                            // Narrow, the fit presets are in the
                                            // zoom menu.
                                            .when(!fit.fit_in_menu(), |layout| {
                                                layout.child(
                                                    h_flex().flex_shrink_0().child(fit_controls),
                                                )
                                            })
                                            .map(|layout| match page_view_menu {
                                                // Narrower still, one menu button
                                                // holds both page views.
                                                Some(menu) => layout
                                                    .child(h_flex().flex_shrink_0().child(menu)),
                                                None => layout
                                                    .child(
                                                        h_flex()
                                                            .flex_shrink_0()
                                                            .child(self.continuous_control.clone()),
                                                    )
                                                    .when_some(
                                                        self.single_page_control.clone(),
                                                        |toolbar, control| {
                                                            toolbar.child(
                                                                h_flex()
                                                                    .flex_shrink_0()
                                                                    .child(control),
                                                            )
                                                        },
                                                    )
                                                    .when_some(
                                                        self.two_pages_control.clone(),
                                                        |toolbar, control| {
                                                            toolbar.child(
                                                                h_flex()
                                                                    .flex_shrink_0()
                                                                    .child(control),
                                                            )
                                                        },
                                                    )
                                                    .when_some(split_toggle, |toolbar, toggle| {
                                                        toolbar.child(
                                                            h_flex().flex_shrink_0().child(toggle),
                                                        )
                                                    }),
                                            })
                                            .when_some(
                                                self.cad_view_control.clone(),
                                                |toolbar, control| {
                                                    toolbar.child(
                                                        h_flex().flex_shrink_0().child(control),
                                                    )
                                                },
                                            ),
                                    ),
                            )
                            .child(
                                // Edit PDF and the Scale pinned right, the
                                // signatures button at the trailing edge.
                                h_flex()
                                    .id(VIEWER_TOOLBAR_RIGHT_ID)
                                    .debug_selector(|| VIEWER_TOOLBAR_RIGHT_ID.into())
                                    .flex_1()
                                    .justify_end()
                                    .items_center()
                                    .map(|slot| if half { slot.gap_1() } else { slot.gap_2() })
                                    // The edge inset, unless the slot is empty
                                    // and the page layout's own inset ends the
                                    // toolbar.
                                    .when(has_right_controls, |slot| {
                                        if half { slot.pr_1() } else { slot.pr_2() }
                                    })
                                    .children(edit_pdf)
                                    .children(signatures),
                            ),
                    ),
            )
    }
}
