//! Application-level composition for Butter Paper property inspectors.
//!
//! The stock GPUI Component controls retain their own interaction, focus, and
//! accessibility behaviour. This module owns only the repeated inspector
//! layout and the selected-property numeric presentation policy.

use gpui::{
    AnyElement, App, Context, ElementId, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, RenderOnce, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, div, prelude::FluentBuilder as _, rems,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Selectable as _, Sizable as _,
    StyledExt as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    collapsible::Collapsible,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    h_flex,
    input::{
        Input, InputEvent, InputState, NumberInput, NumberInputEvent, StepAction, Textarea,
        TextareaState,
    },
    scroll::ScrollableElement as _,
    slider::{Slider, SliderState},
    v_flex,
};

use crate::document_workspace::DocumentId;

/// The left column every property row shares, so the controls line up.
const PROPERTY_LABEL_WIDTH_REMS: f32 = 5.25;
/// The narrowest one of a pair of numbers gets beside its row's label: room
/// for its letter and a coordinate such as "1191.06" in macOS's wider system
/// font. Narrower, the pair moves below the label (`property_number_pair_row`).
const PROPERTY_PAIR_MIN_WIDTH_REMS: f32 = 6.25;
/// The narrowest a number pair gets beside its label: two numbers and the gap.
const PROPERTY_PAIR_CONTROL_MIN_WIDTH_REMS: f32 = 2. * PROPERTY_PAIR_MIN_WIDTH_REMS + 0.75;
/// The narrowest a slider and its number get beside their label.
const PROPERTY_SLIDER_CONTROL_MIN_WIDTH_REMS: f32 = 9.75;
/// How far a setting that depends on the one above it sits in from the left.
const PROPERTY_DEPENDENT_INDENT_REMS: f32 = 1.;

/// One property: its name in the left column and its control filling the
/// right, the layout every properties panel shares.
#[derive(IntoElement)]
pub struct PropertyRow {
    label: SharedString,
    control: AnyElement,
    align_top: bool,
    /// The narrowest the control gets beside the label before it moves below
    /// it, in rems; `None` keeps it beside the label.
    stack_below_rems: Option<f32>,
    dependent: bool,
}

impl PropertyRow {
    pub fn new(label: impl Into<SharedString>, control: impl IntoElement) -> Self {
        Self {
            label: label.into(),
            control: control.into_any_element(),
            align_top: false,
            stack_below_rems: None,
            dependent: false,
        }
    }

    /// Marks a setting that depends on another in the same section, such as
    /// a fill's opacity under the fill switch. It sits indented under that
    /// setting, and its owner disables it while the setting it depends on
    /// is off: a panel never adds or removes the row.
    pub fn dependent(mut self) -> Self {
        self.dependent = true;
        self
    }

    /// For a wide control (a number pair, a slider and its number): when the
    /// panel is too narrow for it beside the label, it moves below the label
    /// at full width, rather than clipping or squeezing.
    pub fn stack_when_narrow(mut self) -> Self {
        self.stack_below_rems = Some(PROPERTY_SLIDER_CONTROL_MIN_WIDTH_REMS);
        self
    }

    /// `stack_when_narrow` for two numbers side by side, which need more room
    /// beside the label than a slider.
    fn stack_pair_when_narrow(mut self) -> Self {
        self.stack_below_rems = Some(PROPERTY_PAIR_CONTROL_MIN_WIDTH_REMS);
        self
    }

    /// Keeps the label level with the first line of a control taller than
    /// one line, such as a text field.
    pub fn align_top(mut self) -> Self {
        self.align_top = true;
        self
    }
}

impl RenderOnce for PropertyRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .w_full()
            .min_w_0()
            .gap_3()
            .when(self.dependent, |row| {
                row.pl(rems(PROPERTY_DEPENDENT_INDENT_REMS))
            })
            .when(self.stack_below_rems.is_some(), |row| {
                row.flex_wrap().gap_y_1p5()
            })
            .map(|row| {
                if self.align_top {
                    row.items_start()
                } else {
                    row.items_center()
                }
            })
            .child(
                // A narrow panel shrinks the labels, which wrap, before the
                // controls.
                div()
                    .w(rems(PROPERTY_LABEL_WIDTH_REMS))
                    .min_w(rems(3.5))
                    .flex_shrink(1.)
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .when(self.align_top, |label| label.pt_1p5())
                    .child(self.label),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .when_some(self.stack_below_rems, |control, min| {
                        control.min_w(rems(min))
                    })
                    .child(self.control),
            )
    }
}

pub fn property_row(label: impl Into<SharedString>, control: impl IntoElement) -> PropertyRow {
    PropertyRow::new(label, control)
}

/// A slider and its number: beside the label, or below it at full width
/// when the panel is too narrow for a usable slider.
pub fn property_slider_row(
    label: impl Into<SharedString>,
    slider: PropertySliderInput,
) -> PropertyRow {
    PropertyRow::new(label, slider).stack_when_narrow()
}

/// Two numbers that make one property, such as a position's X and Y, side
/// by side: beside the label, or below it at full width when the panel is
/// too narrow for both values.
pub fn property_number_pair_row(
    label: impl Into<SharedString>,
    first: impl IntoElement,
    second: impl IntoElement,
) -> PropertyRow {
    PropertyRow::new(label, property_number_pair(first, second)).stack_pair_when_narrow()
}

/// Rows that belong together, under an optional quiet heading.
#[derive(IntoElement)]
pub struct PropertyGroup {
    title: Option<SharedString>,
    rows: Vec<AnyElement>,
}

impl PropertyGroup {
    pub fn new() -> Self {
        Self {
            title: None,
            rows: Vec::new(),
        }
    }

    pub fn titled(title: impl Into<SharedString>) -> Self {
        Self {
            title: Some(title.into()),
            rows: Vec::new(),
        }
    }

    pub fn row(mut self, row: impl IntoElement) -> Self {
        self.rows.push(row.into_any_element());
        self
    }

    pub fn row_if(self, condition: bool, row: impl FnOnce() -> AnyElement) -> Self {
        if condition { self.row(row()) } else { self }
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

impl Default for PropertyGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOnce for PropertyGroup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        v_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .when_some(self.title, |group, title| {
                group.child(
                    div()
                        .text_xs()
                        .font_medium()
                        .text_color(cx.theme().muted_foreground)
                        .child(title),
                )
            })
            .children(self.rows)
    }
}

/// The ids an optional section answers to.
#[derive(Clone, Copy, Debug)]
pub struct PropertyOptionalSectionIds {
    /// The whole section.
    pub root: &'static str,
    /// The heading's checkbox.
    pub checkbox: &'static str,
    /// The heading's chevron.
    pub chevron: &'static str,
    /// The fields, drawn while the section is open.
    pub body: &'static str,
}

/// An optional group of settings, such as Measure: a collapsible section
/// whose heading holds its checkbox, then its title, with a chevron at the
/// far end.
///
/// A properties panel never adds or removes rows when a setting changes, so
/// an unchecked section keeps its heading and shows its fields disabled.
/// Checking it enables the fields; the owner opens the section too, since the
/// fields are what the person turned it on for. The chevron collapses or
/// expands the section on its own. The checkbox and the chevron are separate
/// controls, each a tab stop that Space and Enter activate.
///
/// The owner keeps the checked and open states and updates them from the
/// callbacks. `body` builds the fields and is told whether they are disabled:
/// the section is unchecked, or `disabled` (a locked markup, a save in
/// progress). Each field then takes that value, and a field that depends on
/// another in the section also stays disabled while that one is off.
#[derive(IntoElement)]
pub struct PropertyOptionalSection {
    ids: PropertyOptionalSectionIds,
    title: SharedString,
    checked: bool,
    open: bool,
    disabled: bool,
    tooltip: Option<SharedString>,
    on_checked_change: Option<std::rc::Rc<dyn Fn(bool, &mut Window, &mut App)>>,
    on_open_change: Option<std::rc::Rc<dyn Fn(bool, &mut Window, &mut App)>>,
    body: Option<Box<dyn FnOnce(bool) -> AnyElement>>,
}

impl PropertyOptionalSection {
    pub fn new(
        ids: PropertyOptionalSectionIds,
        title: impl Into<SharedString>,
        checked: bool,
        open: bool,
    ) -> Self {
        Self {
            ids,
            title: title.into(),
            checked,
            open,
            disabled: false,
            tooltip: None,
            on_checked_change: None,
            on_open_change: None,
            body: None,
        }
    }

    /// Makes the checkbox and every field inert, as a locked markup does. The
    /// chevron still works: collapsing a section changes no document.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Called with the new checked state when the checkbox is activated.
    pub fn on_checked_change(
        mut self,
        handler: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_checked_change = Some(std::rc::Rc::new(handler));
        self
    }

    /// Called with the new open state when the chevron is activated.
    pub fn on_open_change(
        mut self,
        handler: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_open_change = Some(std::rc::Rc::new(handler));
        self
    }

    /// Whether the section's fields are disabled: the section is unchecked,
    /// or it is `disabled` as a whole.
    pub fn fields_disabled(&self) -> bool {
        !self.checked || self.disabled
    }

    /// The fields: `fields` builds them when the section draws, given
    /// `fields_disabled`.
    pub fn body(mut self, fields: impl FnOnce(bool) -> AnyElement + 'static) -> Self {
        self.body = Some(Box::new(fields));
        self
    }
}

impl RenderOnce for PropertyOptionalSection {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let ids = self.ids;
        let open = self.open;
        let fields_disabled = self.fields_disabled();
        let body = self.body.map(|fields| fields(fields_disabled));
        let title = self.title;
        let checkbox = Checkbox::new(ids.checkbox)
            .debug_selector(move || ids.checkbox.into())
            .small()
            .label(title.clone())
            .checked(self.checked)
            .disabled(self.disabled)
            .when_some(self.tooltip, |checkbox, tooltip| checkbox.tooltip(tooltip))
            .when_some(self.on_checked_change, |checkbox, handler| {
                checkbox.on_click(move |checked, window, cx| handler(*checked, window, cx))
            });
        let chevron = crate::accessible_button::accessible_disclosure_button(
            Button::new(ids.chevron)
                .debug_selector(move || ids.chevron.into())
                .ghost()
                .small()
                .icon(if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .tooltip(if open {
                    format!("Collapse {title}")
                } else {
                    format!("Expand {title}")
                })
                .when_some(self.on_open_change, |button, handler| {
                    button.on_click(move |_, window, cx| handler(!open, window, cx))
                }),
            format!("{title} fields"),
            open,
        );
        div()
            .id(ids.root)
            .debug_selector(move || ids.root.into())
            .w_full()
            .min_w_0()
            .child(
                Collapsible::new()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .open(open)
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(div().min_w_0().child(checkbox))
                            .child(chevron),
                    )
                    .content(
                        div()
                            .id(ids.body)
                            .debug_selector(move || ids.body.into())
                            .w_full()
                            .min_w_0()
                            .children(body),
                    ),
            )
    }
}

/// The ids a disclosure section answers to.
#[derive(Clone, Copy, Debug)]
pub struct PropertyDisclosureIds {
    /// The whole section.
    pub root: &'static str,
    /// The heading, which opens and closes the section.
    pub toggle: &'static str,
    /// The muted summary in the closed heading.
    pub summary: &'static str,
    /// The fields, drawn while the section is open.
    pub body: &'static str,
}

/// A collapsible group of settings with no checkbox, such as the Visual
/// search's Options: the section's heading is one control that opens and
/// closes it (the optional section's heading and chevron, without the
/// checkbox). While it is closed the heading shows a short muted summary of
/// the values inside, cut to the room there is. The owner keeps the open
/// state and updates it from `on_open_change`.
#[derive(IntoElement)]
pub struct PropertyDisclosureSection {
    ids: PropertyDisclosureIds,
    title: SharedString,
    summary: SharedString,
    open: bool,
    on_open_change: Option<std::rc::Rc<dyn Fn(bool, &mut Window, &mut App)>>,
    body: Option<AnyElement>,
}

impl PropertyDisclosureSection {
    pub fn new(ids: PropertyDisclosureIds, title: impl Into<SharedString>, open: bool) -> Self {
        Self {
            ids,
            title: title.into(),
            summary: SharedString::default(),
            open,
            on_open_change: None,
            body: None,
        }
    }

    /// The summary shown in the closed heading.
    pub fn summary(mut self, summary: impl Into<SharedString>) -> Self {
        self.summary = summary.into();
        self
    }

    /// Called with the new open state when the heading is activated.
    pub fn on_open_change(
        mut self,
        handler: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_open_change = Some(std::rc::Rc::new(handler));
        self
    }

    /// The fields, drawn while the section is open.
    pub fn body(mut self, body: impl IntoElement) -> Self {
        self.body = Some(body.into_any_element());
        self
    }
}

impl RenderOnce for PropertyDisclosureSection {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ids = self.ids;
        let open = self.open;
        let muted = cx.theme().muted_foreground;
        let title = self.title;
        let summary = self.summary;
        let heading = crate::accessible_button::accessible_disclosure_button(
            Button::new(ids.toggle)
                .debug_selector(move || ids.toggle.into())
                .ghost()
                .w_full()
                .tooltip(if open {
                    format!("Collapse {title}")
                } else {
                    format!("Expand {title}")
                })
                .when_some(self.on_open_change, |button, handler| {
                    button.on_click(move |_, window, cx| handler(!open, window, cx))
                })
                .child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap_2()
                        .child(div().flex_none().child(title.clone()))
                        .child(
                            div()
                                .id(ids.summary)
                                .debug_selector(move || ids.summary.into())
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(muted)
                                .when(open, |summary| summary.hidden())
                                .child(summary),
                        )
                        .child(Icon::new(if open {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })),
                ),
            title.clone(),
            open,
        );
        div()
            .id(ids.root)
            .debug_selector(move || ids.root.into())
            .w_full()
            .min_w_0()
            .child(
                Collapsible::new()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .open(open)
                    .child(heading)
                    .content(
                        div()
                            .id(ids.body)
                            .debug_selector(move || ids.body.into())
                            .w_full()
                            .min_w_0()
                            .children(self.body),
                    ),
            )
    }
}

/// A properties panel's content: its groups, a group's space apart, inside
/// the grid's margins.
pub fn property_body() -> gpui::Div {
    v_flex()
        .w_full()
        .min_w_0()
        .p(crate::layout_metrics::scaled(crate::layout_metrics::SPACE))
        .gap(crate::layout_metrics::scaled(
            crate::layout_metrics::GROUP_SPACE,
        ))
}

/// A colour as one control: its swatch and hex value, opening the palette.
/// Picking a colour applies it (`subscribe_property_color`), so there is no
/// separate Apply step. `label` names the property for assistive technology.
#[derive(IntoElement)]
pub struct PropertyColor {
    id: &'static str,
    label: SharedString,
    state: Entity<ColorPickerState>,
    disabled: bool,
}

impl PropertyColor {
    pub fn new(
        id: &'static str,
        label: impl Into<SharedString>,
        state: &Entity<ColorPickerState>,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            state: state.clone(),
            disabled: false,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for PropertyColor {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let value = self.state.read(cx).value();
        let hex: SharedString =
            value.map_or_else(|| "None".into(), |value| hex_label(value).into());
        let id = self.id;
        // A narrow panel shortens the hex value with an ellipsis; the swatch
        // stays, and hovering the value shows all of it.
        let hex_label = {
            let tooltip = hex.clone();
            div()
                .id((id, 1usize))
                .min_w_0()
                .truncate()
                .text_sm()
                .when(self.disabled, |label| {
                    label.text_color(cx.theme().muted_foreground)
                })
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .child(hex)
        };
        let control = div()
            .id(id)
            .debug_selector(move || id.into())
            .aria_label(self.label)
            .min_w_0()
            .overflow_hidden();
        let swatch = if self.disabled {
            div()
                .size_5()
                .flex_none()
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().input)
                .when_some(value, |swatch, value| swatch.bg(value))
                .into_any_element()
        } else {
            div()
                .flex_none()
                .child(property_color_picker(&self.state).small())
                .into_any_element()
        };
        // The hex value opens the palette too, as the swatch does.
        let hex_label = if self.disabled {
            hex_label.into_any_element()
        } else {
            let state = self.state.clone();
            hex_label
                .cursor_pointer()
                .on_click(move |_, window, cx| {
                    let focus = state.read(cx).focus_handle(cx);
                    window.focus(&focus, cx);
                    state.update(cx, |state, cx| state.set_open(true, cx));
                })
                .into_any_element()
        };
        // The swatch and its value, not the whole column: a press in the
        // empty space beside them is no press on the colour.
        h_flex()
            .w_full()
            .min_w_0()
            .child(
                control.child(
                    h_flex()
                        .min_w_0()
                        .gap_2()
                        .items_center()
                        .child(swatch)
                        .child(hex_label),
                ),
            )
            .into_any_element()
    }
}

/// Applies a picked colour: at once for a palette pick or a typed hex value,
/// which close the picker, and on closing after dragging its sliders, so a
/// drag is one change rather than one per step.
pub fn subscribe_property_color<T: 'static>(
    state: &Entity<ColorPickerState>,
    cx: &mut Context<T>,
    apply: impl Fn(&mut T, &mut Context<T>) + Clone + 'static,
) -> [Subscription; 2] {
    let pending = std::rc::Rc::new(std::cell::Cell::new(false));
    let on_change = {
        let pending = pending.clone();
        let apply = apply.clone();
        cx.subscribe(state, move |this, state, _: &ColorPickerEvent, cx| {
            if state.read(cx).is_open() {
                pending.set(true);
            } else {
                pending.set(false);
                apply(this, cx);
            }
        })
    };
    let on_close = cx.observe(state, move |this, state, cx| {
        if pending.get() && !state.read(cx).is_open() {
            pending.set(false);
            apply(this, cx);
        }
    });
    [on_change, on_close]
}

/// A number on its own row, in the same right-hand column as the sizes
/// beside colours and the values beside sliders.
pub fn property_number(input: impl IntoElement) -> impl IntoElement {
    h_flex()
        .w_full()
        .justify_end()
        .child(div().w_20().flex_none().child(input))
}

/// Two numbers that make one property, such as a position's X and Y.
/// They always sit side by side; `property_number_pair_row` moves the pair
/// below its label in a narrow panel instead of clipping them.
pub fn property_number_pair(first: impl IntoElement, second: impl IntoElement) -> impl IntoElement {
    h_flex()
        .w_full()
        .min_w_0()
        .gap_3()
        .child(div().flex_1().min_w_0().child(first))
        .child(div().flex_1().min_w_0().child(second))
}

/// The panel header's lock: it locks or unlocks the selected markup, and
/// shows which it is.
pub fn property_lock_toggle(
    id: &'static str,
    locked: bool,
    disabled: bool,
    on_toggle: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label = if locked { "Unlock" } else { "Lock" };
    // Sized to the button, so its centre is the button's.
    h_flex()
        .id(id)
        .debug_selector(move || id.into())
        .size_6()
        .flex_none()
        .items_center()
        .justify_center()
        .child(crate::accessible_button::accessible_icon_button(
            Button::new((id, 0usize))
                .ghost()
                .small()
                .selected(locked)
                .icon(Icon::default().path(if locked {
                    crate::application_assets::LOCK_ICON
                } else {
                    crate::application_assets::LOCK_OPEN_ICON
                }))
                .tooltip(if locked {
                    "Locked. Unlock to change it"
                } else {
                    "Lock to stop changes"
                })
                .disabled(disabled)
                .on_click(move |_, window, cx| {
                    give_keyboard_to_page(window, cx);
                    on_toggle(!locked, window, cx)
                }),
            label,
        ))
}

/// A stock button takes no focus on a press, so a properties field keeps
/// the keyboard, and a Delete after the press edits the field instead of
/// acting on the markup. The page takes the keyboard (`FocusPage`), as a
/// press on the page does.
fn give_keyboard_to_page(window: &mut Window, cx: &mut App) {
    use gpui_component::WindowExt as _;
    if window.has_focused_input(cx) {
        window.dispatch_action(Box::new(crate::document_workspace::FocusPage), cx);
    }
}

/// Text typed in a properties panel's Text field for one markup, to replace
/// its text. The panel emits it when the field loses focus or on
/// Cmd/Ctrl+Enter; the workspace validates and records it as one change.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyTextEditEvent {
    pub document_id: DocumentId,
    pub annotation_id: crate::annotation_model::MarkupId,
    pub expected_revision: u64,
    pub content: String,
}

/// A properties panel's multi-line Text field.
#[derive(IntoElement)]
pub struct PropertyTextField {
    id: &'static str,
    state: Entity<TextareaState>,
    disabled: bool,
}

impl PropertyTextField {
    pub fn new(id: &'static str, state: &Entity<TextareaState>) -> Self {
        Self {
            id,
            state: state.clone(),
            disabled: false,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for PropertyTextField {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let id = self.id;
        div()
            .id(id)
            .debug_selector(move || id.into())
            .w_full()
            .child(
                Textarea::new(&self.state)
                    .aria_label("Text")
                    .disabled(self.disabled),
            )
    }
}

/// A colour's hex code as the panels show it.
fn hex_label(color: gpui::Hsla) -> String {
    let rgb = gpui::Rgba::from(color);
    format!(
        "#{:02X}{:02X}{:02X}",
        (rgb.r * 255.).round() as u8,
        (rgb.g * 255.).round() as u8,
        (rgb.b * 255.).round() as u8
    )
}

/// What a property inspector tells its owner while one of its sliders is
/// dragged. The value is shown on the canvas at once and committed once,
/// when the pointer is released.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyPreviewEvent<E> {
    /// Show the markup with the edit `E`, without committing it.
    Show(E),
    /// Show the committed markup again: the drag ended, or the value is the
    /// committed one.
    Clear,
    /// A field holds text that is not yet a valid value: keep what the page
    /// shows, and let Escape restore the field.
    Pending,
}

/// What a typed change in a properties field means. Each keystroke previews
/// the value; leaving the field or pressing Enter commits it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldPhase {
    Preview,
    Commit,
}

impl FieldPhase {
    /// The phase of an input's event, if it is a typed change.
    pub fn of(event: &InputEvent) -> Option<Self> {
        match event {
            InputEvent::Change => Some(Self::Preview),
            InputEvent::Blur | InputEvent::PressEnter { .. } => Some(Self::Commit),
            InputEvent::Focus => None,
        }
    }
}

/// Keep the displayed number and the canvas live while a slider is dragged,
/// without producing an undo entry per frame. `preview` shows the value on
/// the canvas. `commit` runs once, on release: it ends the preview and makes
/// the identity-checked edit. The inspector supplies its sync guard.
pub fn subscribe_property_slider<T: 'static>(
    slider: &Entity<SliderState>,
    input: &Entity<InputState>,
    window: &mut Window,
    cx: &mut gpui::Context<T>,
    enabled: impl Fn(&T) -> bool + 'static,
    preview: impl Fn(&mut T, f64, &mut gpui::Context<T>) + 'static,
    commit: impl Fn(&mut T, f64, &mut gpui::Context<T>) + 'static,
) -> gpui::Subscription {
    let input = input.clone();
    cx.subscribe_in(
        slider,
        window,
        move |this, _, event: &gpui_component::slider::SliderEvent, window, cx| {
            if !enabled(this) {
                return;
            }
            match event {
                gpui_component::slider::SliderEvent::Change(value) => {
                    input.update(cx, |input, cx| {
                        input.set_value(format_property_number(value.start().into()), window, cx);
                    });
                    preview(this, value.start().into(), cx);
                }
                gpui_component::slider::SliderEvent::Release(value) => {
                    commit(this, value.start().into(), cx)
                }
            }
        },
    )
}

/// A sidebar panel's header, shared by the properties panels and the page
/// thumbnails, with a title that stays centred when an X is placed in the
/// trailing lane.
#[derive(IntoElement)]
pub struct PropertyInspectorHeader {
    id: &'static str,
    title: SharedString,
    trailing: Option<AnyElement>,
    lock: Option<AnyElement>,
    /// The trailing lane's width, for trailing controls wider than one X:
    /// the title is then centred between the lanes, a little left of the
    /// header's centre, so that it keeps its whole name in a narrow panel.
    trailing_lane: Option<gpui::Rems>,
}

impl PropertyInspectorHeader {
    pub fn new(id: &'static str, title: impl Into<SharedString>) -> Self {
        Self {
            id,
            title: title.into(),
            trailing: None,
            lock: None,
            trailing_lane: None,
        }
    }

    /// Widens the trailing lane to `width`, for several trailing controls.
    pub fn trailing_lane(mut self, width: gpui::Rems) -> Self {
        self.trailing_lane = Some(width);
        self
    }

    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }

    /// The selected markup's lock, just before the X.
    pub fn lock(mut self, lock: impl IntoElement) -> Self {
        self.lock = Some(lock.into_any_element());
        self
    }
}

impl RenderOnce for PropertyInspectorHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let trailing = self.trailing;
        let lock = self.lock;
        let trailing_lane = self.trailing_lane;
        // A lock widens both lanes alike, keeping the title centred; the X,
        // whether placed here or over the lane, takes its end.
        let lane = |lane: gpui::Div| {
            if lock.is_some() {
                lane.w(rems(3.5))
            } else {
                lane.w_6()
            }
        };

        h_flex()
            .id(self.id)
            .debug_selector(move || self.id.into())
            .w_full()
            // A bar: 8 + 32 + 8 with the grid's margins, its line inside.
            .h(crate::layout_metrics::scaled(crate::layout_metrics::BAR))
            .flex_shrink_0()
            .relative()
            .items_center()
            .px(crate::layout_metrics::scaled(crate::layout_metrics::SPACE))
            .child(crate::layout_metrics::inside_line(
                crate::layout_metrics::Edge::Bottom,
                cx.theme().border,
            ))
            // Equal lanes keep the title centred; each fits the standard X.
            .child(lane(div()).flex_none())
            // Only the title is semibold: a trailing control's menu or
            // popover inherits the row's text style.
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_center()
                    .text_sm()
                    .font_semibold()
                    // Cut short rather than run under a lane's controls.
                    .truncate()
                    .child(self.title),
            )
            .child(
                lane(h_flex())
                    .when_some(trailing_lane, |lane, width| lane.w(width))
                    .flex_none()
                    .items_center()
                    .children(lock)
                    .children(trailing),
            )
    }
}

/// The title row of a popover panel: its title and the standard top-right X
/// (`accessible_button::close_icon_button`) that dismisses it.
pub fn popover_header(title: impl Into<SharedString>, close: impl IntoElement) -> impl IntoElement {
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap_2()
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_semibold()
                .child(title.into()),
        )
        .child(close)
}

/// The common selected-property panel surface and edge-owned scroll region.
#[derive(IntoElement)]
pub struct PropertyInspectorPanel {
    id: &'static str,
    header_id: &'static str,
    scroll_id: &'static str,
    title: SharedString,
    header_trailing: Option<AnyElement>,
    header_lock: Option<AnyElement>,
    content: Option<AnyElement>,
    content_only: bool,
}

impl PropertyInspectorPanel {
    pub fn new(
        id: &'static str,
        header_id: &'static str,
        scroll_id: &'static str,
        title: impl Into<SharedString>,
    ) -> Self {
        Self {
            id,
            header_id,
            scroll_id,
            title: title.into(),
            header_trailing: None,
            header_lock: None,
            content: None,
            content_only: false,
        }
    }

    pub fn header_trailing(mut self, trailing: impl IntoElement) -> Self {
        self.header_trailing = Some(trailing.into_any_element());
        self
    }

    /// The selected markup's lock (`property_lock_toggle`) in the header.
    pub fn header_lock(mut self, lock: impl IntoElement) -> Self {
        self.header_lock = Some(lock.into_any_element());
        self
    }

    /// Compose two domain editors beneath one sidebar header and scroll owner.
    pub fn content_only(mut self, content_only: bool) -> Self {
        self.content_only = content_only;
        self
    }

    pub fn child(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }
}

impl RenderOnce for PropertyInspectorPanel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.content_only {
            return div()
                .id(self.id)
                .debug_selector(move || self.id.into())
                .w_full()
                .min_w_0()
                // A scroll that no field took belongs to the panel.
                .on_scroll_wheel(|_, _, cx| crate::property_wheel::wheel_over_panel(cx))
                .children(self.content)
                .into_any_element();
        }
        let mut header = PropertyInspectorHeader::new(self.header_id, self.title);
        if let Some(trailing) = self.header_trailing {
            header = header.trailing(trailing);
        }
        if let Some(lock) = self.header_lock {
            header = header.lock(lock);
        }

        v_flex()
            .id(self.id)
            .debug_selector(move || self.id.into())
            .w_full()
            .h_full()
            .min_w_0()
            .min_h_0()
            .flex_none()
            .text_sm()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            // A scroll that no field took belongs to the panel.
            .on_scroll_wheel(|_, _, cx| crate::property_wheel::wheel_over_panel(cx))
            .child(header)
            .child(
                div()
                    .id(self.scroll_id)
                    .debug_selector(move || self.scroll_id.into())
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(div().w_full().children(self.content)),
            )
            .into_any_element()
    }
}

/// A stock text input used for one numeric property, without stepper buttons.
#[derive(IntoElement)]
pub struct PropertyNumericInput {
    id: &'static str,
    label: SharedString,
    state: Entity<InputState>,
    prefix: Option<SharedString>,
    suffix: Option<SharedString>,
    disabled: bool,
}

impl PropertyNumericInput {
    pub fn new(
        id: &'static str,
        label: impl Into<SharedString>,
        state: &Entity<InputState>,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            state: state.clone(),
            prefix: None,
            suffix: None,
            disabled: false,
        }
    }

    /// A quiet letter naming the value, where a row holds two (X and Y).
    pub fn prefix(mut self, prefix: impl Into<SharedString>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    pub fn suffix(mut self, suffix: impl Into<SharedString>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for PropertyNumericInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut input = Input::new(&self.state)
            .aria_label(self.label)
            .w_full()
            .disabled(self.disabled);
        if let Some(prefix) = self.prefix {
            input = input.prefix(div().text_color(cx.theme().muted_foreground).child(prefix));
        }
        if let Some(suffix) = self.suffix {
            input = input.suffix(suffix);
        }

        let id = self.id;
        let element = div()
            .id(id)
            .debug_selector(move || id.into())
            .w_full()
            .child(input);
        // The wheel steps the number, if its owner gave it steps.
        crate::property_wheel::number_wheel(element, &ElementId::from(id), &self.state, window, cx)
    }
}

/// A size with the stock number input's − and + buttons: a line's width, a
/// font size, an offset or a landing. The buttons and the Up and Down keys
/// step the value; subscribe the field to its step with
/// [`subscribe_number_steps`], and build its `InputState` without `.step(..)`,
/// so that the owner sees each step as an edit (the subscription turns the input's own stepping off).
#[derive(IntoElement)]
pub struct PropertyStepInput {
    id: &'static str,
    state: Entity<InputState>,
    suffix: Option<SharedString>,
    disabled: bool,
}

impl PropertyStepInput {
    pub fn new(id: &'static str, state: &Entity<InputState>) -> Self {
        Self {
            id,
            state: state.clone(),
            suffix: None,
            disabled: false,
        }
    }

    /// The unit, after the number.
    pub fn suffix(mut self, suffix: impl Into<SharedString>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for PropertyStepInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut input = NumberInput::new(&self.state).disabled(self.disabled);
        if let Some(suffix) = self.suffix {
            input = input.suffix(suffix);
        }
        let id = self.id;
        let element = div()
            .id(id)
            .debug_selector(move || id.into())
            .w_full()
            .child(input);
        // The wheel steps the number, with or without the keyboard focus.
        crate::property_wheel::number_wheel(element, &ElementId::from(id), &self.state, window, cx)
    }
}

/// The value one press of a number input's − or + gives: `current` moved by
/// `step` and held between `min` and `max`, rounded to the step's precision.
/// `None` when the press changes nothing (the value is at that end already).
pub fn stepped_number(
    current: f64,
    action: StepAction,
    step: f64,
    min: f64,
    max: f64,
) -> Option<f64> {
    let moved = match action {
        StepAction::Increment => current + step,
        StepAction::Decrement => current - step,
    };
    let next = ((moved.clamp(min, max)) * 1e6).round() / 1e6;
    (next != current).then_some(next)
}

/// Steps a number field by `step` for each press of its − and + buttons or
/// of Up and Down, between `min` and `max`. Each press shows its value on
/// the markup and commits it as its own undo step: the field emits the
/// `Change` and the `PressEnter` that typing the value and pressing Enter
/// does, so the owner's usual handler for that field runs.
pub fn subscribe_number_steps<T: 'static>(
    input: &Entity<InputState>,
    step: f64,
    min: f64,
    max: f64,
    window: &mut Window,
    cx: &mut gpui::Context<T>,
) -> gpui::Subscription {
    // The stock input steps silently by 1 unless told otherwise; with no step
    // of its own it reports each press to the owner.
    input.update(cx, |input, cx| input.set_step(None, window, cx));
    crate::property_wheel::register_number_wheel(input, step, min, max, cx);
    cx.subscribe_in(
        input,
        window,
        move |_, input, event: &NumberInputEvent, window, cx| {
            let NumberInputEvent::Step(action) = event;
            let current = input.read(cx).value().trim().parse::<f64>().unwrap_or(min);
            let Some(next) = stepped_number(current, *action, step, min, max) else {
                return;
            };
            input.update(cx, |input, cx| {
                input.set_value(format_property_number(next), window, cx);
                cx.emit(InputEvent::Change);
                cx.emit(InputEvent::PressEnter {
                    secondary: false,
                    shift: false,
                });
            });
        },
    )
}

/// A stock slider paired with a stepper-free numeric input and unit suffix.
/// The owning inspector retains synchronisation, validation, and commit policy.
#[derive(IntoElement)]
pub struct PropertySliderInput {
    label: SharedString,
    slider: Entity<SliderState>,
    input: Entity<InputState>,
    row_id: Option<&'static str>,
    slider_id: Option<&'static str>,
    input_id: Option<&'static str>,
    suffix: Option<SharedString>,
    disabled: bool,
    stepper: bool,
}

impl PropertySliderInput {
    pub fn new(
        label: impl Into<SharedString>,
        slider: &Entity<SliderState>,
        input: &Entity<InputState>,
    ) -> Self {
        Self {
            label: label.into(),
            slider: slider.clone(),
            input: input.clone(),
            row_id: None,
            slider_id: None,
            input_id: None,
            suffix: None,
            disabled: false,
            stepper: false,
        }
    }

    /// The number beside the slider has the stock − and + buttons, as a
    /// [`PropertyStepInput`] has: subscribe its field with
    /// [`subscribe_number_steps`].
    pub fn stepper(mut self) -> Self {
        self.stepper = true;
        self
    }

    pub fn row_id(mut self, id: &'static str) -> Self {
        self.row_id = Some(id);
        self
    }

    pub fn slider_id(mut self, id: &'static str) -> Self {
        self.slider_id = Some(id);
        self
    }

    pub fn input_id(mut self, id: &'static str) -> Self {
        self.input_id = Some(id);
        self
    }

    pub fn suffix(mut self, suffix: impl Into<SharedString>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for PropertySliderInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let stepper = self.stepper;
        let input = if stepper {
            let mut input = NumberInput::new(&self.input).disabled(self.disabled);
            if let Some(suffix) = self.suffix {
                input = input.suffix(suffix);
            }
            input.into_any_element()
        } else {
            let mut input = Input::new(&self.input)
                .aria_label(self.label)
                .w_full()
                .disabled(self.disabled);
            if let Some(suffix) = self.suffix {
                input = input.suffix(suffix);
            }
            input.into_any_element()
        };

        // The stock slider keeps its full-strength track when disabled;
        // fade it as the disabled number field beside it is faded.
        let slider = div()
            .flex_1()
            .min_w_0()
            .when(self.disabled, |slider| slider.opacity(0.5))
            .child(Slider::new(&self.slider).disabled(self.disabled));
        let slider = if let Some(id) = self.slider_id {
            slider
                .id(id)
                .debug_selector(move || id.into())
                .into_any_element()
        } else {
            slider.into_any_element()
        };

        // The − and + buttons take room the plain number does not.
        let input = div()
            .when(stepper, |input| input.w_32())
            .when(!stepper, |input| input.w_20())
            .flex_shrink_0()
            .child(input);
        let input = if let Some(id) = self.input_id {
            input
                .id(id)
                .debug_selector(move || id.into())
                .into_any_element()
        } else {
            input.into_any_element()
        };

        let row = h_flex()
            .items_center()
            // The stock thumb and hover ring project beyond the track.
            .gap_4()
            .child(slider)
            .child(input);
        // The wheel over the slider or its number steps the slider.
        let row = if let Some(id) = self.row_id {
            let row = row.id(id).debug_selector(move || id.into());
            crate::property_wheel::slider_wheel(row, &ElementId::from(id), &self.slider, window, cx)
        } else {
            let id =
                ElementId::NamedInteger("property-slider".into(), self.slider.entity_id().as_u64());
            crate::property_wheel::slider_wheel(row.id(id.clone()), &id, &self.slider, window, cx)
        };
        row.into_any_element()
    }
}

/// The supported palette-only picker configuration avoids duplicate
/// colour-derived accessibility IDs in the stock featured and full palettes.
pub fn property_color_picker(state: &Entity<ColorPickerState>) -> ColorPicker {
    ColorPicker::new(state).featured_colors(Vec::new())
}

/// Puts `text` in a number field, unless the field already shows a number
/// that reads the same. While a number is typed, the value it previews comes
/// back through the panel's sync: "1." must not become "1" under the caret.
pub fn set_number_text(
    input: &Entity<InputState>,
    text: String,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    let reads = |text: &str| text.trim().trim_end_matches('%').trim().parse::<f64>().ok();
    let current = input.read(cx).value();
    if reads(current.as_ref()).is_some_and(|current| Some(current) == reads(&text)) {
        return;
    }
    input.update(cx, |input, cx| input.set_value(text, window, cx));
}

pub fn format_property_number(value: f64) -> String {
    if value.fract().abs() <= f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.6}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .into()
    }
}

pub fn format_property_percentage(value: f64) -> String {
    format_property_number(value * 100.)
}

/// Compact point coordinates for paired fields. Callers must recognise an
/// unchanged display string before committing, preserving the canonical f64.
pub fn format_property_geometry(value: f64) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}

pub fn parse_property_percentage(value: &str) -> Option<f64> {
    let percentage = value.trim().parse::<f64>().ok()?;
    (percentage.is_finite() && (0. ..=100.).contains(&percentage)).then_some(percentage / 100.)
}

/// Keeps the canonical model value when a picker still contains its exact
/// `f32` projection, avoiding precision-only property edits.
pub fn canonical_picker_opacity(picker_alpha: f32, canonical: f64) -> f64 {
    if picker_alpha == canonical as f32 {
        canonical
    } else {
        picker_alpha as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_parses_property_numbers_without_precision_noise() {
        assert_eq!(format_property_geometry(111.500741), "111.5");
        assert_eq!(format_property_geometry(519.851703), "519.85");
        assert_eq!(format_property_geometry(-0.0001), "0");
        assert_eq!(format_property_geometry(100.), "100");
        assert_eq!(format_property_number(12.), "12");
        assert_eq!(format_property_number(12.5), "12.5");
        assert_eq!(format_property_percentage(0.625), "62.5");
        assert_eq!(parse_property_percentage("62.5"), Some(0.625));
        assert_eq!(parse_property_percentage("101"), None);
        assert_eq!(parse_property_percentage("not a number"), None);
    }

    #[test]
    fn picker_opacity_preserves_canonical_precision_until_alpha_changes() {
        assert_eq!(canonical_picker_opacity(0.8_f64 as f32, 0.8), 0.8);
        assert_eq!(canonical_picker_opacity(0.4, 0.8), f64::from(0.4_f32));
    }
}

/// The ids a panel's "Reset to defaults…" confirmation answers to.
#[derive(Clone, Copy, Debug)]
pub struct PropertyResetIds {
    /// The row holding the button.
    pub root: &'static str,
    pub button: &'static str,
    /// The popover's content.
    pub popover: &'static str,
    pub cancel: &'static str,
    pub confirm: &'static str,
}

struct PropertyResetConfirmState {
    open: bool,
    focus: gpui::FocusHandle,
    /// Where focus goes back to when the confirmation closes after the
    /// keyboard opened it (a pointer open restores focus itself).
    return_focus: Option<gpui::FocusHandle>,
}

impl PropertyResetConfirmState {
    fn close(&mut self, window: &mut Window, cx: &mut App) {
        self.open = false;
        if let Some(previous) = self.return_focus.take()
            && (self.focus.contains_focused(window, cx) || window.focused(cx).is_none())
        {
            previous.focus(window, cx);
        }
    }
}

/// A panel's "Reset to defaults…" button, confirmed in a small popover beside
/// it rather than a window-wide dialog: one line saying what changes, Cancel,
/// and Reset. Escape, Cancel or a press elsewhere closes it unchanged; Enter
/// or Reset confirms.
///
/// `scope` names what is being reset, such as the document and tool: a new
/// scope starts closed, so a confirmation never outlives what it described.
#[derive(IntoElement)]
pub struct PropertyResetConfirm {
    ids: PropertyResetIds,
    scope: SharedString,
    message: SharedString,
    disabled: bool,
    on_confirm: std::rc::Rc<dyn Fn(&mut Window, &mut App)>,
}

impl PropertyResetConfirm {
    pub fn new(
        ids: PropertyResetIds,
        scope: impl Into<SharedString>,
        message: impl Into<SharedString>,
        on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            ids,
            scope: scope.into(),
            message: message.into(),
            disabled: false,
            on_confirm: std::rc::Rc::new(on_confirm),
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for PropertyResetConfirm {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ids = self.ids;
        let key = SharedString::from(format!("{}-confirm-{}", ids.root, self.scope));
        let state = window.use_keyed_state(key.clone(), cx, |_, cx| PropertyResetConfirmState {
            open: false,
            focus: cx.focus_handle(),
            return_focus: None,
        });
        let disabled = self.disabled;
        if disabled && state.read(cx).open {
            state.update(cx, |state, cx| state.close(window, cx));
        }
        let (open, focus) = {
            let state = state.read(cx);
            (state.open, state.focus.clone())
        };
        let button = Button::new(ids.button)
            .debug_selector(move || ids.button.into())
            .accessibility_id(ids.button)
            .ghost()
            .small()
            .label("Reset to defaults…")
            .selected(open)
            .disabled(disabled)
            .on_click({
                let state = state.clone();
                let focus = focus.clone();
                move |_, window, cx| {
                    // A pointer press has already opened it; Enter or Space on
                    // the focused button opens it here.
                    let opened = state.update(cx, |state, cx| {
                        let opening = !state.open;
                        if opening {
                            state.open = true;
                            state.return_focus = window.focused(cx);
                        }
                        cx.notify();
                        opening
                    });
                    if opened {
                        focus.focus(window, cx);
                    }
                }
            });
        let row = h_flex()
            .id(ids.root)
            .debug_selector(move || ids.root.into());
        if disabled {
            return row.child(button);
        }
        let message = self.message;
        let on_confirm = self.on_confirm;
        row.child(
            gpui_component::popover::Popover::new(key)
                .anchor(gpui::Anchor::TopLeft)
                .open(open)
                .track_focus(&focus)
                .on_open_change({
                    let state = state.clone();
                    move |open, window, cx| {
                        state.update(cx, |state, cx| {
                            if *open {
                                state.open = true;
                            } else {
                                state.close(window, cx);
                            }
                            cx.notify();
                        });
                    }
                })
                .trigger(button)
                .content(move |_, window, cx| {
                    let popover = cx.entity();
                    let confirm = {
                        let on_confirm = on_confirm.clone();
                        let popover = popover.clone();
                        move |window: &mut Window, cx: &mut App| {
                            on_confirm(window, cx);
                            popover.update(cx, |popover, cx| popover.dismiss(window, cx));
                        }
                    };
                    let enter_confirm = confirm.clone();
                    let enter_focus = focus.clone();
                    let cancel_popover = popover.clone();
                    v_flex()
                        .id(ids.popover)
                        .debug_selector(move || ids.popover.into())
                        .track_focus(&focus)
                        .on_key_down(move |event: &gpui::KeyDownEvent, window, cx| {
                            let modifiers = event.keystroke.modifiers;
                            // Enter on a focused Cancel is Cancel's own.
                            if event.keystroke.key == "enter"
                                && !modifiers.modified()
                                && enter_focus.is_focused(window)
                            {
                                cx.stop_propagation();
                                enter_confirm(window, cx);
                            }
                        })
                        .w(rems(16.))
                        .max_w(window.viewport_size().width - gpui::px(32.))
                        .min_w_0()
                        .whitespace_normal()
                        .gap_3()
                        .child(div().text_sm().child(message.clone()))
                        .child(
                            h_flex()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new(ids.cancel)
                                        .debug_selector(move || ids.cancel.into())
                                        .accessibility_id(ids.cancel)
                                        .small()
                                        .outline()
                                        .label("Cancel")
                                        .on_click(move |_, window, cx| {
                                            cancel_popover.update(cx, |popover, cx| {
                                                popover.dismiss(window, cx)
                                            });
                                        }),
                                )
                                .child(
                                    Button::new(ids.confirm)
                                        .debug_selector(move || ids.confirm.into())
                                        .accessibility_id(ids.confirm)
                                        .small()
                                        .primary()
                                        .label("Reset")
                                        .on_click(move |_, window, cx| confirm(window, cx)),
                                ),
                        )
                }),
        )
    }
}
