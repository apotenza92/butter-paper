//! The page navigator in the view toolbar, as in Revu's navigation bar:
//! First page, Previous page, the current page of the total ("3 / 12"),
//! Next page and Last page. Clicking the page number makes it a field to
//! type a page into: Enter goes there, Escape or leaving the field keeps the
//! page shown.

use gpui::{
    AppContext as _, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, Window, div, prelude::FluentBuilder as _, rems,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Escape, Input, InputEvent, InputState},
};

use crate::accessible_button::accessible_icon_button;
use crate::viewer_icons::{first_page_icon, last_page_icon, next_page_icon, previous_page_icon};

pub const PAGE_NAVIGATOR_ID: &str = "viewer-page-navigator";
pub const PAGE_FIRST_ID: &str = "viewer-page-first";
pub const PAGE_PREVIOUS_ID: &str = "viewer-page-previous";
pub const PAGE_NEXT_ID: &str = "viewer-page-next";
pub const PAGE_LAST_ID: &str = "viewer-page-last";
/// The "3 / 12" button that becomes the page field when clicked.
pub const PAGE_INDICATOR_ID: &str = "viewer-page-indicator";
/// The field a page number is typed into.
pub const PAGE_INPUT_ID: &str = "viewer-page-input";

/// How much of the navigator the toolbar has room for. A compact toolbar
/// drops the page buttons; the page field always shows (Home, End and the
/// arrow keys still turn pages).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageNavigatorFit {
    Full,
    Indicator,
}

/// A page the navigator asks for. Pages are counted from 0.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageNavigatorEvent {
    First,
    Previous,
    Next,
    Last,
    GoTo(u32),
}

/// The page a typed entry asks for, counted from 0: a whole number, held to
/// the document's pages. Anything else asks for none.
pub fn parse_page_entry(entry: &str, page_count: u32) -> Option<u32> {
    if page_count == 0 {
        return None;
    }
    let number = entry.trim().parse::<u64>().ok()?;
    Some((number.clamp(1, u64::from(page_count)) - 1) as u32)
}

/// The page field's least width, in rems, for a page count of `digits`
/// digits: room for its widest label, so it keeps its width as pages turn.
pub const fn indicator_min_rems(digits: u32) -> f32 {
    2.5 + digits as f32
}

/// The toolbar room the page field takes, with the gap before it.
pub const fn indicator_room_rems(digits: u32) -> f32 {
    indicator_min_rems(digits) + 0.375
}

/// The most page-count digits the toolbar's least width is planned for.
pub const PAGE_NAVIGATOR_MAX_DIGITS: u32 = 4;
/// The room Previous and Next page take, and again First and Last page.
pub const PAGE_NAVIGATOR_BUTTON_PAIR_REMS: f32 = 4.;

/// The digits of a page count, for sizing the page field.
pub fn page_count_digits(page_count: u32) -> u32 {
    page_count.max(1).ilog10() + 1
}

/// "3 / 12" for the third of twelve pages; dashes without a document.
pub fn format_page_indicator(current: u32, page_count: u32) -> String {
    if page_count == 0 {
        "– / –".to_owned()
    } else {
        format!("{} / {page_count}", current.min(page_count - 1) + 1)
    }
}

struct PageEntry {
    input: Entity<InputState>,
    /// Where focus returns when the entry ends.
    restore_focus: Option<FocusHandle>,
    _subscription: Subscription,
}

/// Holds the page shown and the page count from the active document; the
/// workspace moves to the pages it asks for.
pub struct PageNavigator {
    current: u32,
    page_count: u32,
    disabled: bool,
    fit: PageNavigatorFit,
    entry: Option<PageEntry>,
}

impl PageNavigator {
    pub fn new() -> Self {
        Self {
            current: 0,
            page_count: 0,
            disabled: true,
            fit: PageNavigatorFit::Full,
            entry: None,
        }
    }

    /// The page shown, counted from 0.
    pub fn current_page(&self) -> u32 {
        self.current
    }

    pub fn page_count(&self) -> u32 {
        self.page_count
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Whether the page field is open for typing.
    pub fn is_editing(&self) -> bool {
        self.entry.is_some()
    }

    pub fn indicator_label(&self) -> String {
        format_page_indicator(self.current, self.page_count)
    }

    /// Follows the active document. A page typed but not yet entered is
    /// kept.
    pub fn set_pages(&mut self, current: u32, page_count: u32, cx: &mut Context<Self>) {
        let current = current.min(page_count.saturating_sub(1));
        if self.current == current && self.page_count == page_count {
            return;
        }
        self.current = current;
        self.page_count = page_count;
        cx.notify();
    }

    pub fn fit(&self) -> PageNavigatorFit {
        self.fit
    }

    pub fn set_fit(&mut self, fit: PageNavigatorFit, cx: &mut Context<Self>) {
        if self.fit != fit {
            self.fit = fit;
            cx.notify();
        }
    }

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if self.disabled == disabled {
            return;
        }
        self.disabled = disabled;
        // A field left open would take typing for a document no longer
        // there.
        if disabled {
            self.entry = None;
        }
        cx.notify();
    }

    /// Whether First and Previous page are available.
    pub fn can_go_back(&self) -> bool {
        !self.disabled && self.page_count > 0 && self.current > 0
    }

    /// Whether Next and Last page are available.
    pub fn can_go_forward(&self) -> bool {
        !self.disabled && self.current + 1 < self.page_count
    }

    fn request(&mut self, event: PageNavigatorEvent, cx: &mut Context<Self>) {
        if self.disabled || self.page_count == 0 {
            return;
        }
        cx.emit(event);
    }

    /// Opens the page field with the page shown, selected, for typing over.
    pub fn begin_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled || self.page_count == 0 || self.entry.is_some() {
            return;
        }
        let restore_focus = window.focused(cx);
        let page = (self.current + 1).to_string();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(page));
        let subscription = cx.subscribe_in(
            &input,
            window,
            |navigator, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => navigator.commit_entry(window, cx),
                InputEvent::Blur => navigator.cancel_entry(window, cx),
                _ => {}
            },
        );
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        self.entry = Some(PageEntry {
            input,
            restore_focus,
            _subscription: subscription,
        });
        cx.notify();
    }

    /// Goes to the page typed, if it is one, and closes the field.
    pub fn commit_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.entry.as_ref() else {
            return;
        };
        let typed = entry.input.read(cx).value();
        let page = parse_page_entry(&typed, self.page_count);
        self.end_entry(window, cx);
        if let Some(page) = page {
            self.request(PageNavigatorEvent::GoTo(page), cx);
        }
    }

    /// Closes the field, keeping the page shown.
    pub fn cancel_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_entry(window, cx);
    }

    fn end_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.entry.take() else {
            return;
        };
        // Back to the document (or wherever focus was), so tool and page
        // shortcuts work again, unless focus already moved on elsewhere.
        if entry.input.focus_handle(cx).is_focused(window)
            && let Some(focus) = entry.restore_focus
        {
            focus.focus(window, cx);
        }
        cx.notify();
    }
}

impl Default for PageNavigator {
    fn default() -> Self {
        Self::new()
    }
}

impl EventEmitter<PageNavigatorEvent> for PageNavigator {}

impl Render for PageNavigator {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let enabled_colour = cx.theme().button_foreground;
        let muted_colour = cx.theme().muted_foreground;
        let disabled_colour = muted_colour.opacity(0.5);
        let back = self.can_go_back();
        let forward = self.can_go_forward();
        let colour = |enabled: bool| {
            if enabled {
                enabled_colour
            } else {
                disabled_colour
            }
        };
        let nav_button = |id: &'static str,
                          label: &'static str,
                          icon: gpui::AnyElement,
                          enabled: bool,
                          event: PageNavigatorEvent,
                          cx: &mut Context<Self>| {
            accessible_icon_button(
                Button::new(id)
                    .ghost()
                    .compact()
                    .debug_selector(move || id.into())
                    .accessibility_id(id)
                    .child(icon)
                    .tooltip(label)
                    .disabled(!enabled)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.request(event, cx);
                    })),
                label,
            )
        };
        let first = nav_button(
            PAGE_FIRST_ID,
            "First Page",
            first_page_icon(colour(back)).into_any_element(),
            back,
            PageNavigatorEvent::First,
            cx,
        );
        let previous = nav_button(
            PAGE_PREVIOUS_ID,
            "Previous Page",
            previous_page_icon(colour(back)).into_any_element(),
            back,
            PageNavigatorEvent::Previous,
            cx,
        );
        let next = nav_button(
            PAGE_NEXT_ID,
            "Next Page",
            next_page_icon(colour(forward)).into_any_element(),
            forward,
            PageNavigatorEvent::Next,
            cx,
        );
        let last = nav_button(
            PAGE_LAST_ID,
            "Last Page",
            last_page_icon(colour(forward)).into_any_element(),
            forward,
            PageNavigatorEvent::Last,
            cx,
        );

        let label: SharedString = self.indicator_label().into();
        // Room for the longest label this document can show, so the
        // buttons beside it stay put as pages turn.
        let digits = page_count_digits(self.page_count);
        let indicator_width = rems(indicator_min_rems(digits));
        let indicator = match &self.entry {
            Some(entry) => h_flex()
                .id(PAGE_INPUT_ID)
                .debug_selector(|| PAGE_INPUT_ID.into())
                .min_w(indicator_width)
                .justify_center()
                .gap_1()
                .on_action(cx.listener(|this, _: &Escape, window, cx| {
                    this.cancel_entry(window, cx);
                }))
                .child(
                    div()
                        .w(rems(0.8 + digits as f32 * 0.6))
                        .child(Input::new(&entry.input).small()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted_colour)
                        .whitespace_nowrap()
                        .child(format!("/ {}", self.page_count)),
                )
                .into_any_element(),
            None => Button::new(PAGE_INDICATOR_ID)
                .ghost()
                .compact()
                .min_w(indicator_width)
                .debug_selector(|| PAGE_INDICATOR_ID.into())
                .accessibility_id(PAGE_INDICATOR_ID)
                .label(label.clone())
                .tooltip(if self.page_count == 0 {
                    "Page".to_owned()
                } else {
                    format!(
                        "Page {} of {}. Click to type a page to go to.",
                        self.current + 1,
                        self.page_count
                    )
                })
                .disabled(self.disabled || self.page_count == 0)
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.begin_entry(window, cx);
                }))
                .into_any_element(),
        };

        let buttons = self.fit == PageNavigatorFit::Full;
        h_flex()
            .id(PAGE_NAVIGATOR_ID)
            .debug_selector(|| PAGE_NAVIGATOR_ID.into())
            .tab_group()
            .flex_shrink_0()
            .items_center()
            .when(buttons, |navigator| navigator.child(first))
            .when(buttons, |navigator| navigator.child(previous))
            .child(indicator)
            .when(buttons, |navigator| navigator.child(next))
            .when(buttons, |navigator| navigator.child(last))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_pages_are_held_to_the_document() {
        assert_eq!(parse_page_entry("3", 12), Some(2));
        assert_eq!(parse_page_entry(" 12 ", 12), Some(11));
        assert_eq!(parse_page_entry("40", 12), Some(11));
        assert_eq!(parse_page_entry("0", 12), Some(0));
        assert_eq!(parse_page_entry("", 12), None);
        assert_eq!(parse_page_entry("two", 12), None);
        assert_eq!(parse_page_entry("-1", 12), None);
        assert_eq!(parse_page_entry("3", 0), None);
    }

    #[test]
    fn page_counts_are_sized_by_their_digits() {
        assert_eq!(page_count_digits(0), 1);
        assert_eq!(page_count_digits(9), 1);
        assert_eq!(page_count_digits(10), 2);
        assert_eq!(page_count_digits(999), 3);
        assert_eq!(page_count_digits(1_000), 4);
    }

    #[test]
    fn the_indicator_counts_from_one() {
        assert_eq!(format_page_indicator(0, 12), "1 / 12");
        assert_eq!(format_page_indicator(11, 12), "12 / 12");
        assert_eq!(format_page_indicator(0, 0), "– / –");
    }
}
