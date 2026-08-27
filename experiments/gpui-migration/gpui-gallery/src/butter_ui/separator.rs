use gpui::{IntoElement, RenderOnce, SharedString, Styled as _, div, prelude::*, px};

use super::ButterTheme;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SeparatorOrientation {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(IntoElement)]
pub struct Separator {
    id: SharedString,
    orientation: SeparatorOrientation,
    theme: ButterTheme,
}

impl Separator {
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            orientation: SeparatorOrientation::Horizontal,
            theme: ButterTheme::light(),
        }
    }

    pub fn orientation(mut self, orientation: SeparatorOrientation) -> Self {
        self.orientation = orientation;
        self
    }
}

impl RenderOnce for Separator {
    fn render(self, _window: &mut gpui::Window, _cx: &mut gpui::App) -> impl IntoElement {
        div()
            .id(self.id)
            .flex_none()
            .bg(self.theme.border)
            .when(
                self.orientation == SeparatorOrientation::Horizontal,
                |element| element.h(px(1.0)).w_full(),
            )
            .when(
                self.orientation == SeparatorOrientation::Vertical,
                |element| element.w(px(1.0)).h_full(),
            )
    }
}
