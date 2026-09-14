use gpui::{
    App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, ScrollHandle, StatefulInteractiveElement as _,
    Styled as _, Window, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Selectable as _,
    button::{Button, ButtonGroup},
    h_flex,
};

use crate::accessible_button::accessible_icon_button;
use crate::cad_view_control::{CadViewControl, CadViewControlEvent};
use crate::continuous_view_control::ContinuousViewControl;
use crate::page_view_control::{
    PageViewControl, PageViewControlEvent, PageViewMode, WheelBehavior,
};
use crate::viewer_icons::{fit_page_icon, fit_width_icon};
use crate::zoom_control::ZoomControl;

pub const VIEWER_TOOLBAR_ID: &str = "viewer-toolbar";
pub const VIEWER_TOOLBAR_SCROLL_ID: &str = "viewer-toolbar-scroll";
pub const VIEWER_TOOLBAR_CONTENT_ID: &str = "viewer-toolbar-content";
pub const FIT_BUTTON_GROUP_ID: &str = "viewer-fit-controls";
pub const FIT_WIDTH_ID: &str = "viewer-fit-width";
pub const FIT_PAGE_ID: &str = "viewer-fit-page";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FitPreset {
    Width,
    Page,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewerToolbarStripEvent {
    FitPresetChanged(FitPreset),
}

/// Owns the representative viewer-toolbar feature state while GPUI Component
/// owns the presentation and interaction behavior of each control.
pub struct ViewerToolbarStrip {
    continuous_control: Entity<ContinuousViewControl>,
    single_page_control: Option<Entity<PageViewControl>>,
    zoom_control: Option<Entity<ZoomControl>>,
    cad_view_control: Option<Entity<CadViewControl>>,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    fit_preset: FitPreset,
    fit_selected: bool,
    fit_changes: usize,
    page_view_mode: PageViewMode,
    disabled: bool,
}

impl ViewerToolbarStrip {
    pub fn new(continuous_control: Entity<ContinuousViewControl>, cx: &mut Context<Self>) -> Self {
        Self {
            continuous_control,
            single_page_control: None,
            zoom_control: None,
            cad_view_control: None,
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            fit_preset: FitPreset::Width,
            fit_selected: true,
            fit_changes: 0,
            page_view_mode: PageViewMode::Continuous,
            disabled: false,
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
            cad_view_control: None,
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            fit_preset: FitPreset::Width,
            fit_selected: true,
            fit_changes: 0,
            page_view_mode: PageViewMode::Continuous,
            disabled: false,
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
        if let Some(control) = &self.cad_view_control {
            control.update(cx, |control, cx| {
                control.sync_retained_state(active, organisation, pages_per_lane, cx);
            });
        }
        cx.notify();
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

impl EventEmitter<ViewerToolbarStripEvent> for ViewerToolbarStrip {}

impl Render for ViewerToolbarStrip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

        h_flex()
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
                            .gap_2()
                            // Match the shell's standard edge inset; keep intrinsic
                            // control sizes intact at the 480px centre-column contract.
                            .px_1()
                            .py_1()
                            .when_some(self.zoom_control.clone(), |toolbar, control| {
                                toolbar.child(h_flex().flex_shrink_0().child(control))
                            })
                            .child(h_flex().flex_shrink_0().child(fit_controls))
                            .child(
                                h_flex()
                                    .flex_shrink_0()
                                    .child(self.continuous_control.clone()),
                            )
                            .when_some(self.single_page_control.clone(), |toolbar, control| {
                                toolbar.child(h_flex().flex_shrink_0().child(control))
                            })
                            .when_some(self.cad_view_control.clone(), |toolbar, control| {
                                toolbar.child(h_flex().flex_shrink_0().child(control))
                            }),
                    ),
            )
    }
}
