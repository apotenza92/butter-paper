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
