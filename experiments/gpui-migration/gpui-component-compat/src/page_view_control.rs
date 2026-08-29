use gpui::{
    ClickEvent, Context, EventEmitter, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Window,
};
use gpui_component::{
    Disableable as _, Selectable as _, Sizable as _,
    button::{Button, DropdownButton},
    h_flex,
    menu::PopupMenuItem,
};

use crate::accessible_button::accessible_icon_button;

pub const CONTINUOUS_SPLIT_ID: &str = "continuous-view-split";
pub const CONTINUOUS_PRIMARY_ID: &str = "continuous-view-primary";
pub const SINGLE_PAGE_SPLIT_ID: &str = "single-page-view-split";
pub const SINGLE_PAGE_PRIMARY_ID: &str = "single-page-view-primary";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageViewMode {
    Continuous,
    SinglePage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WheelBehavior {
    Scroll,
    Zoom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageViewControlEvent {
    Activated(PageViewMode),
    FitActivated(PageViewMode),
    WheelBehaviorChanged(PageViewMode, WheelBehavior),
}

/// Retains one page-view control's presentation and interaction state. The
/// toolbar owns the exclusive page-view selection and responds to `Activated`.
pub struct PageViewControl {
    mode: PageViewMode,
    primary_activations: usize,
    fit_activations: usize,
    wheel_behavior: WheelBehavior,
    selected: bool,
    disabled: bool,
}

impl PageViewControl {
    /// Preserve the original compatibility seam: the default constructor is
    /// the Continuous control.
    pub fn new() -> Self {
        Self::continuous()
    }

    pub fn continuous() -> Self {
        Self {
            mode: PageViewMode::Continuous,
            primary_activations: 0,
            fit_activations: 0,
            wheel_behavior: WheelBehavior::Scroll,
            selected: true,
            disabled: false,
        }
    }

    pub fn single_page() -> Self {
        Self {
            mode: PageViewMode::SinglePage,
            primary_activations: 0,
            fit_activations: 0,
            wheel_behavior: WheelBehavior::Zoom,
            selected: false,
            disabled: false,
        }
    }

    pub fn mode(&self) -> PageViewMode {
        self.mode
    }

    pub fn primary_activations(&self) -> usize {
        self.primary_activations
    }

    pub fn wheel_behavior(&self) -> WheelBehavior {
        self.wheel_behavior
    }

    pub fn fit_activations(&self) -> usize {
        self.fit_activations
    }

    pub fn fit_width_activations(&self) -> usize {
        if self.mode == PageViewMode::Continuous {
            self.fit_activations
        } else {
            0
        }
    }

    pub fn fit_page_activations(&self) -> usize {
        if self.mode == PageViewMode::SinglePage {
            self.fit_activations
        } else {
            0
        }
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub fn set_selected(&mut self, selected: bool, cx: &mut Context<Self>) {
        if self.selected == selected {
            return;
        }
        self.selected = selected;
        cx.notify();
    }

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if self.disabled == disabled && (!disabled || !self.selected) {
            return;
        }
        self.disabled = disabled;
        if disabled {
            self.selected = false;
        }
        cx.notify();
    }

    /// Synchronizes application-owned wheel state without reporting a user
    /// intent back to the owner.
    pub fn sync_wheel_behavior(&mut self, behavior: WheelBehavior, cx: &mut Context<Self>) {
        if self.wheel_behavior == behavior {
            return;
        }
        self.wheel_behavior = behavior;
        cx.notify();
    }

    fn set_wheel_behavior(&mut self, behavior: WheelBehavior, cx: &mut Context<Self>) {
        self.wheel_behavior = behavior;
        cx.emit(PageViewControlEvent::WheelBehaviorChanged(
            self.mode, behavior,
        ));
        cx.notify();
    }

    fn split_id(&self) -> &'static str {
        match self.mode {
            PageViewMode::Continuous => CONTINUOUS_SPLIT_ID,
            PageViewMode::SinglePage => SINGLE_PAGE_SPLIT_ID,
        }
    }

    fn primary_id(&self) -> &'static str {
        match self.mode {
            PageViewMode::Continuous => CONTINUOUS_PRIMARY_ID,
            PageViewMode::SinglePage => SINGLE_PAGE_PRIMARY_ID,
        }
    }

    fn label(&self) -> &'static str {
        match self.mode {
            PageViewMode::Continuous => "Continuous View",
            PageViewMode::SinglePage => "Single Page View",
        }
    }
}

impl Default for PageViewControl {
    fn default() -> Self {
        Self::new()
    }
}

impl EventEmitter<PageViewControlEvent> for PageViewControl {}

impl Render for PageViewControl {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_behavior = self.wheel_behavior;
        let disabled = self.disabled;
        let selected = self.selected && !disabled;
        let split_id = self.split_id();
        let primary_id = self.primary_id();
        let probe_id = match self.mode {
            PageViewMode::Continuous => "continuous-view-split-probe",
            PageViewMode::SinglePage => "single-page-view-split-probe",
        };
        let label = self.label();
        let icon = match self.mode {
            PageViewMode::Continuous => crate::viewer_icons::continuous_icon().into_any_element(),
            PageViewMode::SinglePage => crate::viewer_icons::single_page_icon().into_any_element(),
        };
        let zoom_control = cx.entity().clone();
        let scroll_control = cx.entity().clone();

        h_flex()
            .id(probe_id)
            .debug_selector(move || split_id.into())
            .child(
                DropdownButton::new(split_id)
                    .small()
                    .button(accessible_icon_button(
                        Button::new(primary_id)
                            .debug_selector(move || primary_id.into())
                            .accessibility_id(primary_id)
                            .tooltip(label)
                            .child(icon)
                            .selected(selected)
                            .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                                if event.click_count() == 2 {
                                    this.fit_activations += 1;
                                    this.selected = true;
                                    cx.emit(PageViewControlEvent::FitActivated(this.mode));
                                } else {
                                    this.primary_activations += 1;
                                    this.selected = true;
                                    cx.emit(PageViewControlEvent::Activated(this.mode));
                                }
                                cx.notify();
                            })),
                        label,
                    ))
                    .selected(selected)
                    .disabled(disabled)
                    .dropdown_menu(move |menu, _, _| {
                        let zoom_control = zoom_control.clone();
                        let scroll_control = scroll_control.clone();
                        menu.item(
                            PopupMenuItem::new("Mousewheel Zoom")
                                .checked(current_behavior == WheelBehavior::Zoom)
                                .on_click(move |_, _, cx| {
                                    zoom_control.update(cx, |control, cx| {
                                        control.set_wheel_behavior(WheelBehavior::Zoom, cx);
                                    });
                                }),
                        )
                        .item(
                            PopupMenuItem::new("Mousewheel Scroll")
                                .checked(current_behavior == WheelBehavior::Scroll)
                                .on_click(move |_, _, cx| {
                                    scroll_control.update(cx, |control, cx| {
                                        control.set_wheel_behavior(WheelBehavior::Scroll, cx);
                                    });
                                }),
                        )
                    }),
            )
    }
}
