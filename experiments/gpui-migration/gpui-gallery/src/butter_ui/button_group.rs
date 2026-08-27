use gpui::{
    AnyElement, IntoElement, ParentElement as _, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, div, prelude::*,
};

#[derive(IntoElement)]
pub struct ButtonGroup {
    id: SharedString,
    label: SharedString,
    children: Vec<AnyElement>,
}

impl ButtonGroup {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            children: Vec::new(),
        }
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }
}

impl RenderOnce for ButtonGroup {
    fn render(self, _window: &mut gpui::Window, _cx: &mut gpui::App) -> impl IntoElement {
        div()
            .id(self.id.clone())
            .role(Role::Group)
            .aria_label(self.label)
            .flex()
            .items_stretch()
            .children(self.children)
    }
}
