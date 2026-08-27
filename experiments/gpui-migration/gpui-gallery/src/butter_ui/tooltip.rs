use gpui::{Context, IntoElement, Render, Role, SharedString, Window, div, prelude::*, px};

use super::ButterTheme;

/// Butter Paper's Nova tooltip, positioned and delayed by GPUI's native
/// deferred-tooltip support.
pub struct Tooltip {
    label: SharedString,
    theme: ButterTheme,
}

impl Tooltip {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            theme: ButterTheme::light(),
        }
    }
}

impl Render for Tooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let label = self.label.clone();
        div()
            .id(SharedString::from(format!("tooltip-{label}")))
            .debug_selector({
                let label = label.clone();
                move || format!("tooltip-{label}")
            })
            .role(Role::Tooltip)
            .rounded(px(6.0))
            .bg(self.theme.primary)
            .text_color(self.theme.primary_foreground)
            .font_family("Geist")
            .text_xs()
            .px(px(12.0))
            .py(px(6.0))
            .child(label)
    }
}
