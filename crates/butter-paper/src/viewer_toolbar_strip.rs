use std::{cell::Cell, rc::Rc};

use gpui::{
    App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, ScrollHandle, StatefulInteractiveElement as _,
    Styled as _, Window, div, prelude::FluentBuilder as _, rems,
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
/// A signed document's signatures button, at the trailing edge.
pub const VIEWER_SIGNATURES_ID: &str = "viewer-signatures";
/// The zoom, fit and page-view controls, centred in the toolbar.
pub const VIEWER_TOOLBAR_CONTROLS_ID: &str = "viewer-toolbar-controls";
/// The least width of the Edit PDF switch's slot, in rems. The view
/// controls stay centred while the space opposite is as wide; in a narrower
/// toolbar that space gives way first, and the controls move right.
pub const EDIT_PDF_SLOT_MIN_REMS: f32 = 8.125;
/// The slot of the compact Edit PDF switch, an icon in place of its label,
/// in rems.
pub const EDIT_PDF_COMPACT_SLOT_REMS: f32 = 4.25;
/// The slot of the signatures button, a shield alone, in rems.
pub const TOOLBAR_ICON_SLOT_REMS: f32 = 2.5;
/// The zoom, fit and page-view controls other than the page navigator.
const VIEW_CONTROLS_REMS: f32 = 24.75;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewerToolbarStripEvent {
    FitPresetChanged(FitPreset),
    /// The Edit PDF switch was pressed.
    EditPdfToggled,
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

        // The Edit PDF switch sits at the leading edge, and the signatures
        // button at the trailing edge; matching flexible spaces keep the
        // view controls centred. All scroll with the controls when the strip
        // is narrow.
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
                ViewerToolbarFit::Full => switch.label("Edit PDF").into_any_element(),
                // Narrow, the same switch with an icon where its label was;
                // the icon toggles it as the label does, and the group
                // carries the name.
                ViewerToolbarFit::Compact => h_flex()
                    .id("viewer-edit-pdf-compact")
                    .role(gpui::Role::Group)
                    .aria_label("Edit PDF")
                    .gap_2()
                    .items_center()
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
            // Padded inside: the slot and the trailing space grow alike. The
            // slot's least width is stated, so it always holds the control
            // and its padding.
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
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .items_center()
                        .child(
                            div()
                                .id(EDIT_PDF_ID)
                                .debug_selector(|| EDIT_PDF_ID.into())
                                .child(control),
                        ),
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
                                        toolbar.child(h_flex().flex_shrink_0().child(control))
                                    })
                                    .when_some(self.page_navigator.clone(), |toolbar, navigator| {
                                        toolbar.child(h_flex().flex_shrink_0().child(navigator))
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
