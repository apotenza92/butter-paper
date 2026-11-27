//! The page navigator in the view toolbar, as in Revu's navigation bar:
//! First page, Previous page, the current page of the total ("3 / 12"),
//! Next page and Last page. The page number is a stock input at rest and
//! while typed in: only a read-only state, the focus ring, the caret and the
//! selection tell them apart, so nothing moves or changes size when it opens.
//! Clicking it (or tabbing to it) selects the number to type over: Enter
//! goes there, Escape or leaving the field keeps the page shown.

use gpui::{
    AppContext as _, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div,
    prelude::FluentBuilder as _, rems,
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
/// The page field and "/ 12" at rest: the field is a stock input holding the
/// page number, read-only until clicked.
pub const PAGE_INDICATOR_ID: &str = "viewer-page-indicator";
/// The same field and "/ 12" while it is open for typing; it has the
/// bounds of the page indicator.
pub const PAGE_INPUT_ID: &str = "viewer-page-input";
/// The field a page number is typed into.
pub const PAGE_INPUT_FIELD_ID: &str = "viewer-page-input-field";

/// How much of the navigator the toolbar has room for. A compact toolbar
/// drops the page buttons; the page field always shows (Home, End and the
/// arrow keys still turn pages).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageNavigatorFit {
    Full,
    Indicator,
    /// The page field shows the page in view alone; its tooltip, and the
    /// field while a page is typed, name the page count. Only the narrowest
    /// toolbar form needs it.
    Current,
}

/// The room one digit takes in the page field, in rems: a little wider than
/// the widest digit at the field's text size.
const PAGE_DIGIT_REMS: f32 = 0.5625;
/// The page field's frame, in rems: its 8px padding each side, its 1px
/// border each side and 2px for the caret.
const PAGE_INPUT_FRAME_REMS: f32 = 1.25;
/// The room each side of the page field, in rems: the 3px focus ring and a
/// pixel more, so the group's frame does not cut the ring.
const PAGE_INPUT_RING_REMS: f32 = 0.25;
/// The gap between the page field and "/ 12", in rems: its focus ring and
/// a clear 5px.
const PAGE_INPUT_GAP_REMS: f32 = 0.5;

/// The page field's width, in rems, for a page count of `digits` digits:
/// room for that many digits (two at least) inside its padding, so a page
/// number typed or selected is never cut.
pub const fn page_input_rems(digits: u32) -> f32 {
    let digits = if digits < 2 { 2 } else { digits };
    PAGE_INPUT_FRAME_REMS + PAGE_DIGIT_REMS * digits as f32
}

/// The room "/ 12" takes beside the page field, in rems: the slash and
/// space, then the digits.
const fn page_count_text_rems(digits: u32) -> f32 {
    1.25 + PAGE_DIGIT_REMS * digits as f32
}

/// The current-page-alone field's least width, in rems, for a page count of
/// `digits` digits: the page field and room for its focus ring.
pub const fn current_indicator_min_rems(digits: u32) -> f32 {
    page_input_rems(digits) + 2. * PAGE_INPUT_RING_REMS
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

/// The page indicator's least width, in rems, for a page count of `digits`
/// digits: room for its widest label, and for the page field with its focus
/// ring and "/ 12" when clicked, so it keeps its width as pages turn and
/// as it opens for typing.
pub const fn indicator_min_rems(digits: u32) -> f32 {
    2. * PAGE_INPUT_RING_REMS
        + page_input_rems(digits)
        + PAGE_INPUT_GAP_REMS
        + page_count_text_rems(digits)
}

/// The toolbar room the page field takes, with the gap before it.
pub const fn indicator_room_rems(digits: u32) -> f32 {
    indicator_min_rems(digits) + 0.375
}

/// The most page-count digits the toolbar's least width is planned for.
pub const PAGE_NAVIGATOR_MAX_DIGITS: u32 = 4;
/// The room Previous and Next page take, and again First and Last page, each
/// with the divider between it and the next member.
pub const PAGE_NAVIGATOR_BUTTON_PAIR_REMS: f32 = 4.125;

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

/// The page field's input, kept for the navigator's life so the page shows in
/// the very control that is typed in.
struct PageField {
    input: Entity<InputState>,
    _subscription: Subscription,
}

/// Holds the page shown and the page count from the active document; the
/// workspace moves to the pages it asks for.
pub struct PageNavigator {
    current: u32,
    page_count: u32,
    disabled: bool,
    fit: PageNavigatorFit,
    field: Option<PageField>,
    /// The field is open for typing.
    editing: bool,
    /// Where focus returns when the entry ends.
    restore_focus: Option<FocusHandle>,
    /// Two Pages view, with whether the cover shows alone: Previous and
    /// Next turn a spread, so they end at the first and last rows.
    spreads: Option<bool>,
}

impl PageNavigator {
    pub fn new() -> Self {
        Self {
            current: 0,
            page_count: 0,
            disabled: true,
            fit: PageNavigatorFit::Full,
            field: None,
            editing: false,
            restore_focus: None,
            spreads: None,
        }
    }

    /// Two Pages view (with whether the cover shows alone), or `None`.
    pub fn set_spreads(&mut self, spreads: Option<bool>, cx: &mut Context<Self>) {
        if self.spreads != spreads {
            self.spreads = spreads;
            cx.notify();
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
        self.editing
    }

    /// The field's input once the navigator has been drawn: the page at rest,
    /// and what is typed while it is open.
    pub fn page_input(&self) -> Option<Entity<InputState>> {
        self.field.as_ref().map(|field| field.input.clone())
    }

    /// The page the field holds at rest: the page shown, counted from 1.
    fn field_text(&self) -> String {
        if self.page_count == 0 {
            "–".to_owned()
        } else {
            (self.current + 1).to_string()
        }
    }

    /// The field's input, made on first use.
    fn field_input(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        if let Some(field) = &self.field {
            return field.input.clone();
        }
        let page = self.field_text();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(page));
        let subscription = cx.subscribe_in(
            &input,
            window,
            |navigator, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => navigator.commit_entry(window, cx),
                InputEvent::Blur => navigator.cancel_entry(window, cx),
                // Tabbing to the field opens it, as a click does.
                InputEvent::Focus => navigator.begin_entry(window, cx),
                _ => {}
            },
        );
        self.field = Some(PageField {
            input: input.clone(),
            _subscription: subscription,
        });
        input
    }

    pub fn indicator_label(&self) -> String {
        if self.fit == PageNavigatorFit::Current && self.page_count > 0 {
            (self.current + 1).to_string()
        } else {
            format_page_indicator(self.current, self.page_count)
        }
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
            self.editing = false;
            self.restore_focus = None;
        }
        cx.notify();
    }

    /// Whether First and Previous page are available.
    pub fn can_go_back(&self) -> bool {
        !self.disabled
            && self.page_count > 0
            && match self.spreads {
                Some(cover) => crate::viewer::two_page_slot(self.current as usize, cover).0 > 0,
                None => self.current > 0,
            }
    }

    /// Whether Next and Last page are available.
    pub fn can_go_forward(&self) -> bool {
        !self.disabled
            && match self.spreads {
                Some(cover) => {
                    self.page_count > 0
                        && crate::viewer::two_page_slot(self.current as usize, cover).0
                            < crate::viewer::two_page_slot(self.page_count as usize - 1, cover).0
                }
                None => self.current + 1 < self.page_count,
            }
    }

    fn request(&mut self, event: PageNavigatorEvent, cx: &mut Context<Self>) {
        if self.disabled || self.page_count == 0 {
            return;
        }
        cx.emit(event);
    }

    /// Opens the page field with the page shown, selected, for typing over.
    pub fn begin_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled || self.page_count == 0 || self.editing {
            return;
        }
        let input = self.field_input(window, cx);
        let focus = input.focus_handle(cx);
        self.restore_focus = window.focused(cx).filter(|focused| *focused != focus);
        self.editing = true;
        let page = self.field_text();
        input.update(cx, |input, cx| {
            if input.value().as_ref() != page.as_str() {
                input.set_value(page, window, cx);
            }
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        cx.notify();
    }

    /// Goes to the page typed, if it is one, and closes the field.
    pub fn commit_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editing {
            return;
        }
        let Some(field) = self.field.as_ref() else {
            return;
        };
        let typed = field.input.read(cx).value();
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
        if !self.editing {
            return;
        }
        self.editing = false;
        let restore_focus = self.restore_focus.take();
        // The field shows the page again: what was typed is dropped.
        let page = self.field_text();
        if let Some(field) = &self.field {
            let input = field.input.clone();
            let focused = input.focus_handle(cx).is_focused(window);
            input.update(cx, |input, cx| {
                if input.value().as_ref() != page.as_str() {
                    input.set_value(page, window, cx);
                }
            });
            // Back to the document (or wherever focus was), so tool and page
            // shortcuts work again, unless focus already moved on elsewhere.
            if focused {
                match restore_focus {
                    Some(focus) => focus.focus(window, cx),
                    None => window.blur(),
                }
            }
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

        // Room for the longest label this document can show, so the
        // buttons beside it stay put as pages turn and as the field opens.
        let digits = page_count_digits(self.page_count);
        let current_alone = self.fit == PageNavigatorFit::Current;
        let indicator_width = rems(if current_alone {
            current_indicator_min_rems(digits)
        } else {
            indicator_min_rems(digits)
        });
        // The same input, group and "/ 12" at rest and while typed in; the
        // page shown follows the document at rest.
        let input = self.field_input(window, cx);
        let editing = self.editing;
        if !editing {
            let page = self.field_text();
            if input.read(cx).value().as_ref() != page.as_str() {
                input.update(cx, |input, cx| input.set_value(page, window, cx));
            }
        }
        let unavailable = self.disabled || self.page_count == 0;
        let tooltip = if self.page_count == 0 {
            "Page".to_owned()
        } else {
            format!(
                "Page {} of {}. Click to type a page to go to.",
                self.current + 1,
                self.page_count
            )
        };
        let id = if editing {
            PAGE_INPUT_ID
        } else {
            PAGE_INDICATOR_ID
        };
        let indicator = h_flex()
            .id(id)
            .debug_selector(move || id.into())
            .min_w(indicator_width)
            .justify_center()
            // Room for the field's focus ring inside the group's frame,
            // which clips.
            .px(rems(PAGE_INPUT_RING_REMS))
            .gap(rems(PAGE_INPUT_GAP_REMS))
            .when(editing, |group| {
                group.on_action(cx.listener(|this, _: &Escape, window, cx| {
                    this.cancel_entry(window, cx);
                }))
            })
            // A press anywhere on the group opens the field, with the page
            // selected, before the input can place a caret.
            .when(!editing && !unavailable, |group| {
                group
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                    })
                    .capture_any_mouse_down(cx.listener(
                        |this, event: &MouseDownEvent, window, cx| {
                            if event.button == MouseButton::Left {
                                this.begin_entry(window, cx);
                                cx.stop_propagation();
                            }
                        },
                    ))
            })
            .child(
                div()
                    .id(PAGE_INPUT_FIELD_ID)
                    .debug_selector(|| PAGE_INPUT_FIELD_ID.into())
                    .flex_shrink_0()
                    .w(rems(page_input_rems(digits)))
                    .child(
                        Input::new(&input)
                            .accessibility_id(PAGE_INDICATOR_ID)
                            .aria_label("Page")
                            .small()
                            .text_center()
                            .readonly(!editing)
                            .disabled(unavailable),
                    ),
            )
            .when(!current_alone, |group| {
                group.child(
                    div()
                        .text_sm()
                        .text_color(muted_colour)
                        .whitespace_nowrap()
                        .child(if self.page_count == 0 {
                            "/ –".to_owned()
                        } else {
                            format!("/ {}", self.page_count)
                        }),
                )
            })
            .into_any_element();

        let buttons = self.fit == PageNavigatorFit::Full;
        h_flex()
            .id(PAGE_NAVIGATOR_ID)
            .debug_selector(|| PAGE_NAVIGATOR_ID.into())
            .tab_group()
            .flex_shrink_0()
            .items_center()
            .children(crate::toolbar_group::divided(
                PAGE_NAVIGATOR_ID,
                if buttons {
                    vec![
                        first.into_any_element(),
                        previous.into_any_element(),
                        indicator,
                        next.into_any_element(),
                        last.into_any_element(),
                    ]
                } else {
                    vec![indicator]
                },
                cx,
            ))
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
