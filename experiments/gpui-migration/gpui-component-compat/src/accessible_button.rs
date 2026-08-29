use gpui::{InteractiveElement, SharedString, StatefulInteractiveElement};
use gpui_component::button::Button;

/// Applies a name to a real GPUI Component icon Button without adding a
/// visible label. The pinned Button exposes its interactivity but does not
/// implement `StatefulInteractiveElement`, so its existing accessibility
/// metadata cannot otherwise receive an independent name.
pub(crate) fn accessible_icon_button(button: Button, label: impl Into<SharedString>) -> Button {
    AccessibleButton(button).aria_label(label).0
}

struct AccessibleButton(Button);

impl InteractiveElement for AccessibleButton {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.0.interactivity()
    }
}

impl StatefulInteractiveElement for AccessibleButton {}
