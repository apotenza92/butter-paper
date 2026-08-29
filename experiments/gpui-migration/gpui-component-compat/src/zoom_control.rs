use std::cell::RefCell;

use gpui::{
    Anchor, ClickEvent, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Window,
};
use gpui_component::{
    Disableable as _, Sizable as _,
    button::Button,
    h_flex,
    menu::{PopupMenu, PopupMenuItem},
    popover::Popover,
};

use crate::accessible_button::accessible_icon_button;
use crate::viewer_icons::{zoom_in_icon, zoom_out_icon};

pub const ZOOM_GROUP_ID: &str = "viewer-zoom-controls";
pub const ZOOM_OUT_ID: &str = "viewer-zoom-out";
pub const ZOOM_IN_ID: &str = "viewer-zoom-in";
pub const ZOOM_MENU_ID: &str = "viewer-zoom-menu";

pub const MIN_VIEWER_ZOOM: f64 = 0.0625;
pub const MAX_VIEWER_ZOOM: f64 = 64.;
pub const DEFAULT_VIEWER_ZOOM: f64 = 1.;
pub const ZOOM_STEP_FACTOR: f64 = 1.1;
pub const ZOOM_PRESETS: [f64; 14] = [
    0.0625, 0.1, 0.25, 0.5, 0.75, 1., 1.25, 1.5, 2., 4., 8., 16., 32., 64.,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZoomControlEvent {
    Changed(f64),
}

#[derive(Default)]
struct ZoomMenuCache {
    menu: RefCell<Option<Entity<PopupMenu>>>,
}

/// Retains application-owned zoom state while GPUI Component owns the three
/// buttons, popup lifecycle, and standard menu interaction.
pub struct ZoomControl {
    zoom: f64,
    disabled: bool,
    menu_open: bool,
    reset_activations: usize,
    focus_handle: Option<FocusHandle>,
}

impl ZoomControl {
    pub fn new() -> Self {
        Self {
            zoom: DEFAULT_VIEWER_ZOOM,
            disabled: false,
            menu_open: false,
            reset_activations: 0,
            focus_handle: None,
        }
    }

    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub fn is_menu_open(&self) -> bool {
        self.menu_open
    }

    pub fn reset_activations(&self) -> usize {
        self.reset_activations
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus_handle
            .clone()
            .expect("the zoom control must render before its focus handle is requested")
    }

    pub fn displayed_percentage(&self) -> String {
        format_zoom_percent(self.zoom)
    }

    pub fn selected_preset(&self) -> Option<f64> {
        ZOOM_PRESETS
            .iter()
            .copied()
            .find(|preset| (self.zoom - preset).abs() < 0.001)
    }

    pub fn set_zoom(&mut self, zoom: f64, cx: &mut Context<Self>) {
        let zoom = clamp_viewer_zoom(zoom);
        if (self.zoom - zoom).abs() < 0.000_001 {
            return;
        }
        self.zoom = zoom;
        cx.notify();
    }

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if self.disabled == disabled && (!disabled || !self.menu_open) {
            return;
        }
        self.disabled = disabled;
        if disabled {
            self.menu_open = false;
        }
        cx.notify();
    }

    fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.request_zoom(self.zoom / ZOOM_STEP_FACTOR, cx);
    }

    fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.request_zoom(self.zoom * ZOOM_STEP_FACTOR, cx);
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        self.menu_open = false;
        self.reset_activations += 1;
        self.request_zoom(DEFAULT_VIEWER_ZOOM, cx);
    }

    fn request_zoom(&mut self, zoom: f64, cx: &mut Context<Self>) {
        let previous = self.zoom;
        self.set_zoom(zoom, cx);
        if (self.zoom - previous).abs() >= 0.000_001 {
            cx.emit(ZoomControlEvent::Changed(self.zoom));
        }
    }
}

impl EventEmitter<ZoomControlEvent> for ZoomControl {}

impl Default for ZoomControl {
    fn default() -> Self {
        Self::new()
    }
}

pub fn clamp_viewer_zoom(zoom: f64) -> f64 {
    if !zoom.is_finite() {
        return MIN_VIEWER_ZOOM;
    }

    let rounded = (zoom * 1_000.).round() / 1_000.;
    rounded.clamp(MIN_VIEWER_ZOOM, MAX_VIEWER_ZOOM)
}

pub fn format_zoom_percent(zoom: f64) -> String {
    if zoom < 0.1 {
        let value = format!("{:.2}", zoom * 100.);
        let value = value.trim_end_matches('0').trim_end_matches('.');
        format!("{value}%")
    } else {
        format!("{}%", (zoom * 100.).round() as i64)
    }
}

impl Render for ZoomControl {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.disabled;
        let current_zoom = self.zoom;
        let focus_handle = self
            .focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        let displayed_percentage: SharedString = self.displayed_percentage().into();
        let menu_cache = window.use_keyed_state("viewer-zoom-menu-cache", cx, |_, _| {
            ZoomMenuCache::default()
        });

        let zoom_out = accessible_icon_button(
            Button::new(ZOOM_OUT_ID)
                .small()
                .debug_selector(|| ZOOM_OUT_ID.into())
                .accessibility_id(ZOOM_OUT_ID)
                .child(zoom_out_icon())
                .tooltip("Zoom Out")
                .disabled(disabled)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.zoom_out(cx))),
            "Zoom Out",
        );

        let zoom_in = accessible_icon_button(
            Button::new(ZOOM_IN_ID)
                .small()
                .debug_selector(|| ZOOM_IN_ID.into())
                .accessibility_id(ZOOM_IN_ID)
                .child(zoom_in_icon())
                .tooltip("Zoom In")
                .disabled(disabled)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.zoom_in(cx))),
            "Zoom In",
        );

        let percentage = Button::new(ZOOM_MENU_ID)
            .small()
            .debug_selector(|| ZOOM_MENU_ID.into())
            .accessibility_id(ZOOM_MENU_ID)
            .label(displayed_percentage)
            .dropdown_caret(true)
            .tooltip("Zoom percentage. Double-click to reset to 100%.")
            .disabled(disabled)
            .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                if event.click_count() == 2 {
                    this.reset(cx);
                    cx.stop_propagation();
                }
            }));

        let percentage = if disabled {
            percentage.into_any_element()
        } else {
            let control_for_open = cx.entity().downgrade();
            let control_for_menu = cx.entity().downgrade();
            Popover::new("viewer-zoom-menu-popover")
                .appearance(false)
                .overlay_closable(false)
                .anchor(Anchor::TopLeft)
                .open(self.menu_open)
                .on_open_change(move |open, _, cx| {
                    let _ = control_for_open.update(cx, |control, cx| {
                        control.menu_open = *open;
                        cx.notify();
                    });
                })
                .trigger(percentage)
                .content(move |_, window, cx| {
                    if let Some(menu) = menu_cache.read(cx).menu.borrow().clone() {
                        return menu;
                    }

                    let control_for_rows = control_for_menu.clone();
                    let menu = PopupMenu::build(window, cx, move |menu, _, _| {
                        ZOOM_PRESETS.iter().copied().fold(menu, |menu, preset| {
                            let control = control_for_rows.clone();
                            menu.item(
                                PopupMenuItem::new(format_zoom_percent(preset))
                                    .checked((current_zoom - preset).abs() < 0.001)
                                    .on_click(move |_, _, cx| {
                                        let _ = control.update(cx, |control, cx| {
                                            control.request_zoom(preset, cx);
                                        });
                                    }),
                            )
                        })
                    });
                    menu_cache.read(cx).menu.replace(Some(menu.clone()));
                    menu.focus_handle(cx).focus(window, cx);

                    let popover = cx.entity();
                    window
                        .subscribe(&menu, cx, {
                            let menu_cache = menu_cache.clone();
                            move |_, _: &DismissEvent, window, cx| {
                                popover.update(cx, |popover, cx| popover.dismiss(window, cx));
                                menu_cache.read(cx).menu.replace(None);
                            }
                        })
                        .detach();

                    menu
                })
                .into_any_element()
        };

        h_flex()
            .id(ZOOM_GROUP_ID)
            .debug_selector(|| ZOOM_GROUP_ID.into())
            .track_focus(&focus_handle)
            .tab_group()
            .flex_shrink_0()
            .gap_1()
            // Keep the exact Electron source order: Out, In, percentage menu.
            .child(zoom_out)
            .child(zoom_in)
            .child(percentage)
    }
}
