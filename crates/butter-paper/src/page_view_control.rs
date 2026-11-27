use gpui::{
    ClickEvent, Context, EventEmitter, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, Window, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, Selectable as _,
    button::{Button, DropdownButton},
    h_flex,
    menu::PopupMenuItem,
};

use crate::accessible_button::accessible_icon_button;

pub const CONTINUOUS_SPLIT_ID: &str = "continuous-view-split";
pub const CONTINUOUS_PRIMARY_ID: &str = "continuous-view-primary";
pub const SINGLE_PAGE_SPLIT_ID: &str = "single-page-view-split";
pub const SINGLE_PAGE_PRIMARY_ID: &str = "single-page-view-primary";
pub const TWO_PAGES_SPLIT_ID: &str = "two-pages-view-split";
pub const TWO_PAGES_PRIMARY_ID: &str = "two-pages-view-primary";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageViewMode {
    Continuous,
    SinglePage,
    /// Pages side by side in pairs, scrolled continuously as rows.
    TwoPages,
}

impl PageViewMode {
    /// Every mode, in the toolbar's order.
    pub const ALL: [Self; 3] = [Self::Continuous, Self::SinglePage, Self::TwoPages];

    /// Whether the view scrolls through every page (Continuous and Two
    /// Pages), rather than showing one page at a time.
    pub const fn scrolls(self) -> bool {
        !matches!(self, Self::SinglePage)
    }

    /// The command's name, as the toolbar and menus give it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Continuous => "Continuous View",
            Self::SinglePage => "Single Page View",
            Self::TwoPages => "Two Pages View",
        }
    }
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
    /// Two Pages view's "Cover page alone" was turned on or off.
    CoverAloneChanged(bool),
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
    /// Two Pages view: whether the first page shows alone.
    cover_alone: bool,
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
            cover_alone: false,
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
            cover_alone: false,
        }
    }

    pub fn two_pages() -> Self {
        Self {
            mode: PageViewMode::TwoPages,
            primary_activations: 0,
            fit_activations: 0,
            wheel_behavior: WheelBehavior::Scroll,
            selected: false,
            disabled: false,
            cover_alone: false,
        }
    }

    pub fn cover_alone(&self) -> bool {
        self.cover_alone
    }

    /// Synchronizes the document's cover setting without reporting it back.
    pub fn sync_cover_alone(&mut self, cover_alone: bool, cx: &mut Context<Self>) {
        if self.cover_alone != cover_alone {
            self.cover_alone = cover_alone;
            cx.notify();
        }
    }

    /// "Cover page alone" chosen in the menu.
    pub fn set_cover_alone(&mut self, cover_alone: bool, cx: &mut Context<Self>) {
        if self.disabled || self.cover_alone == cover_alone {
            return;
        }
        self.cover_alone = cover_alone;
        cx.emit(PageViewControlEvent::CoverAloneChanged(cover_alone));
        cx.notify();
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
        if self.mode != PageViewMode::Continuous {
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

    pub fn activate(&mut self, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        self.primary_activations = self.primary_activations.saturating_add(1);
        self.selected = true;
        cx.emit(PageViewControlEvent::Activated(self.mode));
        cx.notify();
    }

    pub fn activate_fit(&mut self, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        self.fit_activations = self.fit_activations.saturating_add(1);
        self.selected = true;
        cx.emit(PageViewControlEvent::FitActivated(self.mode));
        cx.notify();
    }

    pub fn set_wheel_behavior(&mut self, behavior: WheelBehavior, cx: &mut Context<Self>) {
        if self.disabled || self.wheel_behavior == behavior {
            return;
        }
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
            PageViewMode::TwoPages => TWO_PAGES_SPLIT_ID,
        }
    }

    fn primary_id(&self) -> &'static str {
        match self.mode {
            PageViewMode::Continuous => CONTINUOUS_PRIMARY_ID,
            PageViewMode::SinglePage => SINGLE_PAGE_PRIMARY_ID,
            PageViewMode::TwoPages => TWO_PAGES_PRIMARY_ID,
        }
    }

    fn label(&self) -> &'static str {
        self.mode.label()
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
        let icon_color = if disabled {
            cx.theme().muted_foreground.opacity(0.5)
        } else {
            cx.theme().button_foreground
        };
        let selected = self.selected && !disabled;
        let split_id = self.split_id();
        let primary_id = self.primary_id();
        let probe_id = match self.mode {
            PageViewMode::Continuous => "continuous-view-split-probe",
            PageViewMode::SinglePage => "single-page-view-split-probe",
            PageViewMode::TwoPages => "two-pages-view-split-probe",
        };
        let label = self.label();
        let icon = match self.mode {
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
        let zoom_control = cx.entity().clone();
        let scroll_control = cx.entity().clone();
        let cover_control = cx.entity().clone();
        let mode = self.mode;
        let cover_alone = self.cover_alone;

        h_flex()
            .id(probe_id)
            .debug_selector(move || split_id.into())
            .relative()
            .child(
                DropdownButton::new(split_id)
                    .outline()
                    .button(accessible_icon_button(
                        Button::new(primary_id)
                            .debug_selector(move || primary_id.into())
                            .accessibility_id(primary_id)
                            .tooltip(match self.mode {
                                PageViewMode::Continuous => {
                                    "Continuous View. Double-click to fit width."
                                }
                                PageViewMode::SinglePage => {
                                    "Single Page View. Double-click to fit page."
                                }
                                PageViewMode::TwoPages => {
                                    "Two Pages View. Double-click to fit page."
                                }
                            })
                            .child(icon)
                            .selected(selected)
                            .toggled(selected)
                            .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                                if event.click_count() == 2 {
                                    this.activate_fit(cx);
                                } else {
                                    this.activate(cx);
                                }
                            })),
                        label,
                    ))
                    .selected(selected)
                    .disabled(disabled)
                    .dropdown_menu(move |menu, _, _| {
                        let zoom_control = zoom_control.clone();
                        let scroll_control = scroll_control.clone();
                        let cover_control = cover_control.clone();
                        menu.check_side(gpui_component::Side::Right)
                            .when(mode == PageViewMode::TwoPages, |menu| {
                                menu.item(
                                    PopupMenuItem::new("Cover page alone")
                                        .checked(cover_alone)
                                        .on_click(move |_, _, cx| {
                                            cover_control.update(cx, |control, cx| {
                                                control.set_cover_alone(!cover_alone, cx);
                                            });
                                        }),
                                )
                                .separator()
                            })
                            .label("Mouse wheel")
                            .item(
                                PopupMenuItem::new("Zoom")
                                    .icon(IconName::Search)
                                    .checked(current_behavior == WheelBehavior::Zoom)
                                    .on_click(move |_, _, cx| {
                                        zoom_control.update(cx, |control, cx| {
                                            control.set_wheel_behavior(WheelBehavior::Zoom, cx);
                                        });
                                    }),
                            )
                            .item(
                                PopupMenuItem::new("Scroll")
                                    .icon(IconName::ChevronsUpDown)
                                    .checked(current_behavior == WheelBehavior::Scroll)
                                    .on_click(move |_, _, cx| {
                                        scroll_control.update(cx, |control, cx| {
                                            control.set_wheel_behavior(WheelBehavior::Scroll, cx);
                                        });
                                    }),
                            )
                    }),
            )
            // Both halves are selected, so the divider between them is
            // drawn in a visible colour, over the buttons.
            .when(selected, |split| {
                split.child(crate::toolbar_group::selected_seam(cx))
            })
    }
}
