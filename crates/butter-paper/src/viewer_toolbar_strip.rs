use std::{cell::Cell, rc::Rc};

use gpui::{
    App, Bounds, ClickEvent, Context, Entity, EventEmitter, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _, px,
    rems,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, ElementExt as _, Icon, Selectable as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    h_flex,
    switch::Switch,
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
pub const VIEWER_TOOLBAR_SCROLL_ID: &str = "viewer-toolbar-scroll";
pub const VIEWER_TOOLBAR_CONTENT_ID: &str = "viewer-toolbar-content";
pub const FIT_BUTTON_GROUP_ID: &str = "viewer-fit-controls";
pub const FIT_WIDTH_ID: &str = "viewer-fit-width";
pub const FIT_PAGE_ID: &str = "viewer-fit-page";
pub const EDIT_PDF_ID: &str = "viewer-edit-pdf";
/// The page's Scale: a switch and the scale's menu, beside Edit PDF.
pub const VIEWER_SCALE_ID: &str = "viewer-scale";
pub const VIEWER_SCALE_SWITCH_ID: &str = "viewer-scale-switch";
pub const VIEWER_SCALE_MENU_ID: &str = "viewer-scale-menu";
/// The width of the scale shown in the Scale menu button, in rems; a longer
/// scale is cut short, and its tooltip names it in full.
const SCALE_VALUE_REMS: f32 = 5.75;
/// A signed document's signatures button, at the trailing edge.
pub const VIEWER_SIGNATURES_ID: &str = "viewer-signatures";
/// The zoom, fit and page-view controls, centred in the toolbar.
pub const VIEWER_TOOLBAR_CONTROLS_ID: &str = "viewer-toolbar-controls";
/// The least width of the slot of the Edit PDF switch and the Scale, in
/// rems. The view controls stay centred while the space opposite is as wide;
/// in a narrower toolbar that space gives way first, and the controls move
/// right.
pub const EDIT_PDF_SLOT_MIN_REMS: f32 = 22.75;
/// The slot of the compact Edit PDF switch and Scale, icons in place of
/// their labels and no scale shown, in rems.
pub const EDIT_PDF_COMPACT_SLOT_REMS: f32 = 13.;
/// The slot of the signatures button, a shield alone, in rems.
pub const TOOLBAR_ICON_SLOT_REMS: f32 = 2.5;
/// The zoom, fit and page-view controls other than the page navigator, with
/// the frames of the zoom and page groups.
const VIEW_CONTROLS_REMS: f32 = 26.;

/// What a toolbar of a given width has room for. The whole toolbar turns
/// compact at once: the Edit PDF switch shows an icon in place of its
/// label, and the page navigator only its page field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewerToolbarFit {
    Full,
    Compact,
}

impl ViewerToolbarFit {
    fn navigator(self) -> PageNavigatorFit {
        match self {
            Self::Full => PageNavigatorFit::Full,
            Self::Compact => PageNavigatorFit::Indicator,
        }
    }
}

/// The toolbar width, in rems, that shows the toolbar as `fit` says, with
/// the signatures button for a signed document, for a document of `digits`
/// page-count digits.
pub const fn viewer_toolbar_rems(fit: ViewerToolbarFit, signed: bool, digits: u32) -> f32 {
    use crate::page_navigator::{PAGE_NAVIGATOR_BUTTON_PAIR_REMS, indicator_room_rems};
    let (edit_pdf, pairs) = match fit {
        ViewerToolbarFit::Full => (EDIT_PDF_SLOT_MIN_REMS, 2.),
        ViewerToolbarFit::Compact => (EDIT_PDF_COMPACT_SLOT_REMS, 0.),
    };
    let signatures = if signed { TOOLBAR_ICON_SLOT_REMS } else { 0. };
    edit_pdf
        + VIEW_CONTROLS_REMS
        + indicator_room_rems(digits)
        + pairs * PAGE_NAVIGATOR_BUTTON_PAIR_REMS
        + signatures
}

/// The least toolbar width, in rems: compact Edit PDF, the view controls,
/// the page field of a document of up to four-digit page counts and the
/// signatures button. Below it the strip scrolls sideways. Thumbnails yield
/// to it, and the document window's least size holds it beside both rails.
pub const VIEWER_TOOLBAR_MIN_REMS: f32 = viewer_toolbar_rems(
    ViewerToolbarFit::Compact,
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
    /// The page's scale; off, the one switching on brings back.
    pub label: Option<SharedString>,
}

/// What a press on the Scale control asks for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaleRequest {
    /// The switch: the page's scale on or off.
    Toggle,
    /// The menu button: the Page Scale panel.
    Menu,
}

/// Handles a press on the Scale control, with the window it was made in.
pub type ScaleHandler = Rc<dyn Fn(ScaleRequest, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewerToolbarStripEvent {
    FitPresetChanged(FitPreset),
    /// The Edit PDF switch was pressed.
    EditPdfToggled,
    /// The pointer entered or left the Scale control.
    ScaleHovered(bool),
    /// The signatures button was pressed.
    SignaturesRequested,
}

/// Owns the representative viewer-toolbar feature state while GPUI Component
/// owns the presentation and interaction behavior of each control.
pub struct ViewerToolbarStrip {
    continuous_control: Entity<ContinuousViewControl>,
    single_page_control: Option<Entity<PageViewControl>>,
    zoom_control: Option<Entity<ZoomControl>>,
    /// The page navigator, right of the zoom percentage.
    page_navigator: Option<Entity<PageNavigator>>,
    /// The digits of the active document's page count, which size the page
    /// field.
    page_digits: u32,
    cad_view_control: Option<Entity<CadViewControl>>,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    fit_preset: FitPreset,
    fit_selected: bool,
    fit_changes: usize,
    page_view_mode: PageViewMode,
    disabled: bool,
    /// The Edit PDF switch, at the strip's leading edge, and whether it is
    /// on; `None` hides it.
    edit_pdf: Option<bool>,
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
            zoom_control: None,
            page_navigator: None,
            page_digits: 1,
            cad_view_control: None,
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            fit_preset: FitPreset::Width,
            fit_selected: true,
            fit_changes: 0,
            page_view_mode: PageViewMode::Continuous,
            disabled: false,
            edit_pdf: None,
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
            zoom_control: None,
            page_navigator: None,
            page_digits: 1,
            cad_view_control: None,
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            fit_preset: FitPreset::Width,
            fit_selected: true,
            fit_changes: 0,
            page_view_mode: PageViewMode::Continuous,
            disabled: false,
            edit_pdf: None,
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

    /// Shows `navigator` right of the zoom percentage.
    pub fn set_page_navigator(&mut self, navigator: Entity<PageNavigator>, cx: &mut Context<Self>) {
        navigator.update(cx, |navigator, cx| {
            navigator.set_disabled(self.disabled, cx)
        });
        self.page_navigator = Some(navigator);
        cx.notify();
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
        }
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Shows the Edit PDF switch, on or off, or hides it.
    pub fn set_edit_pdf(&mut self, edit_pdf: Option<bool>, cx: &mut Context<Self>) {
        if self.edit_pdf != edit_pdf {
            self.edit_pdf = edit_pdf;
            cx.notify();
        }
    }

    pub fn edit_pdf(&self) -> Option<bool> {
        self.edit_pdf
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
        let switch_tooltip: SharedString = match (&scale.label, scale.on) {
            (Some(label), true) => {
                format!("Page {page} · {label}. Switch off to remove page {page}'s scale.")
            }
            (Some(label), false) => {
                format!("Page {page} has no scale. Switch on to bring back {label}.")
            }
            (None, _) => format!("Page {page} has no scale. Switch on to set one."),
        }
        .into();
        let menu_tooltip: SharedString = match &scale.label {
            Some(label) if scale.on => {
                format!("Page {page}'s scale: {label}. Change it… (Shift+Alt+S)")
            }
            _ => format!("Set page {page}'s scale… (Shift+Alt+S)"),
        }
        .into();
        let switch = Switch::new("viewer-scale-toggle")
            .checked(scale.on)
            .disabled(disabled)
            .tooltip(switch_tooltip)
            .on_click(cx.listener(|toolbar, _: &bool, window, cx| {
                toolbar.request_scale(ScaleRequest::Toggle, window, cx);
            }));
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let foreground = if disabled { muted } else { theme.foreground };
        let switch_part = match fit {
            ViewerToolbarFit::Full => h_flex().child(switch.label("Scale")),
            // Narrow, a ruler where the label was; it toggles the switch as
            // the label does.
            ViewerToolbarFit::Compact => h_flex().gap_2().child(switch).child(
                div()
                    .id("viewer-scale-icon")
                    .when(!disabled, |icon| {
                        icon.cursor_pointer().on_click(cx.listener(
                            |toolbar, _: &ClickEvent, window, cx| {
                                toolbar.request_scale(ScaleRequest::Toggle, window, cx);
                            },
                        ))
                    })
                    .child(
                        Icon::default()
                            .path("icons/rail/ruler-dimension-line.svg")
                            .text_color(foreground),
                    ),
            ),
        };
        let value: SharedString = scale.label.clone().unwrap_or_else(|| "Not set".into());
        let menu = Button::new(VIEWER_SCALE_MENU_ID)
            .debug_selector(|| VIEWER_SCALE_MENU_ID.into())
            .accessibility_id(VIEWER_SCALE_MENU_ID)
            .ghost()
            .compact()
            .dropdown_caret(true)
            .rounded(px(0.))
            .h_full()
            .px_2()
            .disabled(disabled)
            .tooltip(menu_tooltip)
            .when(fit == ViewerToolbarFit::Full, |button| {
                button.child(
                    div()
                        .w(rems(SCALE_VALUE_REMS))
                        .truncate()
                        // Off, the scale switching on brings back.
                        .when(!scale.on, |value| value.text_color(muted))
                        .child(value.clone()),
                )
            })
            .on_click(cx.listener(|toolbar, _: &ClickEvent, window, cx| {
                toolbar.request_scale(ScaleRequest::Menu, window, cx);
            }));
        let menu = match fit {
            ViewerToolbarFit::Full => menu.into_any_element(),
            ViewerToolbarFit::Compact => {
                accessible_icon_button(menu, format!("Page scale: {value}")).into_any_element()
            }
        };
        let anchor = self.scale_bounds.clone();
        toolbar_group(cx)
            .id(VIEWER_SCALE_ID)
            .debug_selector(|| VIEWER_SCALE_ID.into())
            .role(gpui::Role::Group)
            .aria_label("Page scale")
            .overflow_hidden()
            .on_prepaint(move |bounds, _, _| anchor.set(Some(bounds)))
            .on_hover(cx.listener(|_, hovered: &bool, _, cx| {
                cx.emit(ViewerToolbarStripEvent::ScaleHovered(*hovered));
            }))
            .child(
                switch_part
                    .id(VIEWER_SCALE_SWITCH_ID)
                    .debug_selector(|| VIEWER_SCALE_SWITCH_ID.into())
                    .px_2()
                    .h_full()
                    .items_center(),
            )
            .child(div().w(px(1.)).h_full().bg(theme.border))
            .child(menu)
            .into_any_element()
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

/// What a toolbar `width` wide has room for.
pub fn viewer_toolbar_fit(
    width: Pixels,
    rem: Pixels,
    signed: bool,
    digits: u32,
) -> ViewerToolbarFit {
    let full = ViewerToolbarFit::Full;
    if width >= rem * viewer_toolbar_rems(full, signed, digits) {
        full
    } else {
        ViewerToolbarFit::Compact
    }
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
        let fit = viewer_toolbar_fit(
            self.width.get(),
            window.rem_size(),
            signed,
            self.page_digits,
        );
        if let Some(navigator) = &self.page_navigator {
            navigator.update(cx, |navigator, cx| navigator.set_fit(fit.navigator(), cx));
        }

        // The Edit PDF switch and the page's Scale sit at the leading edge,
        // and the signatures button at the trailing edge; matching flexible
        // spaces keep the view controls centred. All scroll with the
        // controls when the strip is narrow.
        let edit_pdf = self.edit_pdf.map(|on| {
            let tooltip = if on {
                "Stop editing the PDF's text, drawings and images (Shift+T)"
            } else {
                "Edit the PDF's text, drawings and images (Shift+T)"
            };
            let switch = Switch::new("viewer-edit-pdf-switch")
                .checked(on)
                .disabled(disabled)
                .tooltip(tooltip)
                .on_click(cx.listener(|_, _: &bool, _, cx| {
                    cx.emit(ViewerToolbarStripEvent::EditPdfToggled);
                }));
            let control = match fit {
                ViewerToolbarFit::Full => toolbar_group(cx)
                    .id("viewer-edit-pdf-full")
                    .px_2()
                    .role(gpui::Role::Group)
                    .aria_label("Edit PDF")
                    .child(switch.label("Edit"))
                    .into_any_element(),
                // Narrow, the same switch with an icon where its label was;
                // the icon toggles it as the label does, and the group
                // carries the name.
                ViewerToolbarFit::Compact => toolbar_group(cx)
                    .id("viewer-edit-pdf-compact")
                    .px_2()
                    .role(gpui::Role::Group)
                    .aria_label("Edit PDF")
                    .gap_2()
                    .child(switch)
                    .child(
                        div()
                            .id("viewer-edit-pdf-icon")
                            .when(!disabled, |icon| {
                                icon.cursor_pointer().on_click(cx.listener(
                                    |_, _: &ClickEvent, _, cx| {
                                        cx.emit(ViewerToolbarStripEvent::EditPdfToggled);
                                    },
                                ))
                            })
                            .child(
                                Icon::default()
                                    .path(crate::application_assets::EDIT_PDF_ICON)
                                    .text_color(if disabled {
                                        cx.theme().muted_foreground
                                    } else {
                                        cx.theme().foreground
                                    }),
                            ),
                    )
                    .into_any_element(),
            };
            let scale = self
                .scale
                .clone()
                .map(|scale| self.render_scale_control(scale, fit, disabled, cx));
            // Padded inside: the slot and the trailing space grow alike. The
            // slot's least width is stated, so it always holds the controls
            // and their padding.
            h_flex()
                .flex_1()
                .flex_shrink_0()
                .min_w(rems(match fit {
                    ViewerToolbarFit::Full => EDIT_PDF_SLOT_MIN_REMS,
                    ViewerToolbarFit::Compact => EDIT_PDF_COMPACT_SLOT_REMS,
                }))
                .child(
                    h_flex()
                        .px_1()
                        .gap_1p5()
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .items_center()
                        .child(
                            div()
                                .id(EDIT_PDF_ID)
                                .debug_selector(|| EDIT_PDF_ID.into())
                                .child(control),
                        )
                        .children(scale),
                )
        });
        let has_edit_pdf = edit_pdf.is_some();

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
            h_flex().px_1().flex_shrink_0().child(accessible_icon_button(
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
                    viewer_toolbar_fit(width, window.rem_size(), signed, page_digits)
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
            .h_12()
            .flex_shrink_0()
            .min_w_0()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .relative()
            .track_focus(&self.focus_handle)
            .child(
                h_flex()
                    .id(VIEWER_TOOLBAR_SCROLL_ID)
                    .debug_selector(|| VIEWER_TOOLBAR_SCROLL_ID.into())
                    .w_full()
                    .min_w_0()
                    .overflow_x_scroll()
                    .track_scroll(&self.scroll_handle)
                    .child(
                        h_flex()
                            .id(VIEWER_TOOLBAR_CONTENT_ID)
                            .debug_selector(|| VIEWER_TOOLBAR_CONTENT_ID.into())
                            .w_auto()
                            .min_w_full()
                            .flex_shrink_0()
                            .justify_center()
                            .items_center()
                            .children(edit_pdf)
                            .child(
                                h_flex()
                                    .id(VIEWER_TOOLBAR_CONTROLS_ID)
                                    .debug_selector(|| VIEWER_TOOLBAR_CONTROLS_ID.into())
                                    .flex_shrink_0()
                                    .justify_center()
                                    .items_center()
                                    .gap_1p5()
                                    // Match the shell's standard edge inset; controls keep
                                    // their intrinsic sizes and scroll rather than shrink.
                                    .px_1()
                                    .py_1()
                                    .when_some(self.zoom_control.clone(), |toolbar, control| {
                                        toolbar.child(
                                            toolbar_group(cx).px_1().flex_shrink_0().child(control),
                                        )
                                    })
                                    .when_some(self.page_navigator.clone(), |toolbar, navigator| {
                                        toolbar.child(
                                            toolbar_group(cx)
                                                .px_1()
                                                .flex_shrink_0()
                                                .child(navigator),
                                        )
                                    })
                                    .child(h_flex().flex_shrink_0().child(fit_controls))
                                    .child(
                                        h_flex()
                                            .flex_shrink_0()
                                            .child(self.continuous_control.clone()),
                                    )
                                    .when_some(
                                        self.single_page_control.clone(),
                                        |toolbar, control| {
                                            toolbar.child(h_flex().flex_shrink_0().child(control))
                                        },
                                    )
                                    .when_some(
                                        self.cad_view_control.clone(),
                                        |toolbar, control| {
                                            toolbar.child(h_flex().flex_shrink_0().child(control))
                                        },
                                    ),
                            )
                            .when(has_edit_pdf || signed, |content| {
                                // Grows as the Edit PDF slot does, so the
                                // controls are centred, but gives way first
                                // when narrow, down to the signatures
                                // button it holds.
                                content.child(
                                    h_flex()
                                        .flex_1()
                                        .flex_shrink_0()
                                        .justify_end()
                                        .when(signed, |slot| {
                                            slot.min_w(rems(TOOLBAR_ICON_SLOT_REMS))
                                        })
                                        .children(signatures),
                                )
                            }),
                    ),
            )
    }
}

/// A group of toolbar controls in one frame, as the fit and page-view
/// buttons are.
fn toolbar_group(cx: &App) -> gpui::Div {
    let theme = cx.theme();
    h_flex()
        .h_8()
        .items_center()
        .border_1()
        .border_color(theme.border)
        .rounded(theme.radius)
}
