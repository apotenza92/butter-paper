use gpui::{InteractiveElement, SharedString, StatefulInteractiveElement};
use gpui_component::{
    IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
};

/// Applies a name to a real GPUI Component icon Button without adding a
/// visible label. The pinned Button exposes its interactivity but does not
/// implement `StatefulInteractiveElement`, so its existing accessibility
/// metadata cannot otherwise receive an independent name.
pub(crate) fn accessible_icon_button(button: Button, label: impl Into<SharedString>) -> Button {
    AccessibleButton(button).aria_label(label).0
}

/// As [`accessible_icon_button`], with a description that a screen reader
/// reads after the name: the second line of the button's tooltip.
pub(crate) fn accessible_described_icon_button(
    button: Button,
    label: impl Into<SharedString>,
    description: impl Into<SharedString>,
) -> Button {
    AccessibleButton(button)
        .aria_label(label)
        .aria_description(description)
        .0
}

struct AccessibleButton(Button);

/// The top-right X that dismisses a panel, popover or dialog: the stock small
/// ghost Close button, as GPUI Component's own sheets and dialogs draw it,
/// named for accessibility and tooltip by `label`. Callers add the action.
pub(crate) fn close_icon_button(id: &'static str, label: impl Into<SharedString>) -> Button {
    let label = label.into();
    accessible_icon_button(
        Button::new(id)
            .debug_selector(move || id.into())
            .accessibility_id(id)
            .ghost()
            .small()
            .icon(IconName::Close)
            .tooltip(label.clone()),
        label,
    )
}

/// Names a stock disclosure control and exposes the panel's current state.
pub(crate) fn accessible_disclosure_button(
    button: Button,
    label: impl Into<SharedString>,
    expanded: bool,
) -> Button {
    AccessibleButton(button)
        .aria_label(label)
        .aria_expanded(expanded)
        .0
}

impl InteractiveElement for AccessibleButton {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.0.interactivity()
    }
}

impl StatefulInteractiveElement for AccessibleButton {}

/// A stock Switch with the stock focus ring: the pinned Switch takes Tab
/// focus but draws none. A wrapper tracks a focus handle of its own, which is
/// no tab stop, so it holds the switch's focus within it, and draws the
/// ring the framework's own controls use while the switch has keyboard
/// focus. A press still focuses the switch: it stops focus passing to the
/// wrapper.
/// Marks the switch whose focus ring shows.
pub const SWITCH_FOCUS_RING_ID: &str = "switch-focus-ring";

#[derive(gpui::IntoElement)]
pub(crate) struct FocusRingSwitch {
    id: gpui::ElementId,
    switch: gpui_component::switch::Switch,
}

/// Wraps `switch` so it shows a focus ring; `id` names the wrapper, and
/// must be unique where it is rendered.
pub(crate) fn focus_ring_switch(
    id: impl Into<gpui::ElementId>,
    switch: gpui_component::switch::Switch,
) -> FocusRingSwitch {
    FocusRingSwitch {
        id: id.into(),
        switch,
    }
}

impl gpui::RenderOnce for FocusRingSwitch {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl gpui::IntoElement {
        use gpui::{ParentElement as _, Styled as _, prelude::FluentBuilder as _};
        use gpui_component::{ActiveTheme as _, ThemeStyled as _};
        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let ring = focus_handle.contains_focused(window, cx) && window.last_input_was_keyboard();
        gpui::div()
            .id(self.id)
            .relative()
            .track_focus(&focus_handle)
            .rounded(cx.theme().radius)
            .when(ring, |wrapper| {
                wrapper.focus_ring_style(window, cx).child(
                    // Where the ring is drawn, for tests to find.
                    gpui::div()
                        .debug_selector(|| SWITCH_FOCUS_RING_ID.into())
                        .absolute()
                        .inset_0(),
                )
            })
            .child(self.switch)
    }
}
