use gpui::{Hsla, IntoElement, RenderOnce, SharedString, Styled as _, svg};

#[derive(IntoElement)]
pub struct Icon {
    name: SharedString,
    size: f32,
    color: Hsla,
}

impl Icon {
    pub fn new(name: impl Into<SharedString>, color: Hsla) -> Self {
        Self {
            name: name.into(),
            size: 16.0,
            color,
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for Icon {
    fn render(self, _window: &mut gpui::Window, _cx: &mut gpui::App) -> impl IntoElement {
        svg()
            .path(SharedString::from(format!("icons/{}.svg", self.name)))
            .size(gpui::px(self.size))
            .text_color(self.color)
    }
}
