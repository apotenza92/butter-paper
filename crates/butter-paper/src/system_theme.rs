use std::{cell::Cell, rc::Rc};

use gpui::{App, Pixels, Subscription, Window, WindowAppearance};
use gpui_component::Theme;

use crate::application_shell::application_ui_zoom_font_size;

/// Applies one window's native appearance through GPUI Component's complete
/// theme transition, including its gpui-base projection.
pub fn apply_window_appearance(appearance: WindowAppearance, window: &mut Window, cx: &mut App) {
    Theme::change(appearance, Some(window), cx);
}

/// Keeps the application theme synchronized with this window's live native
/// appearance for exactly as long as the returned subscription is retained.
pub fn follow_window_appearance(window: &mut Window, cx: &mut App) -> Subscription {
    let subscription = window.observe_window_appearance(|window, cx| {
        apply_window_appearance(window.appearance(), window, cx);
    });
    apply_window_appearance(window.appearance(), window, cx);
    subscription
}

/// Keeps the application theme and the shell's rem-based zoom synchronized
/// when the native appearance changes.
pub fn follow_window_appearance_with_application_zoom(
    window: &mut Window,
    cx: &mut App,
    zoom_level: Rc<Cell<i8>>,
    zoom_base_font_size: Rc<Cell<Pixels>>,
) -> Subscription {
    let subscription = window.observe_window_appearance({
        let zoom_level = zoom_level.clone();
        let zoom_base_font_size = zoom_base_font_size.clone();
        move |window, cx| {
            apply_window_appearance_with_zoom_base(
                window.appearance(),
                Some(window),
                cx,
                zoom_level.get(),
                &zoom_base_font_size,
            );
        }
    });
    apply_window_appearance_with_zoom_base(
        window.appearance(),
        Some(window),
        cx,
        zoom_level.get(),
        &zoom_base_font_size,
    );
    subscription
}

fn apply_window_appearance_with_zoom_base(
    appearance: WindowAppearance,
    window: Option<&mut Window>,
    cx: &mut App,
    zoom_level: i8,
    zoom_base_font_size: &Cell<Pixels>,
) {
    // Theme configs may omit `font_size`; reset the previous zoom first so an
    // omitted value cannot make the current zoomed size become the new base.
    Theme::global_mut(cx).font_size = zoom_base_font_size.get();
    Theme::sync_base(cx);
    Theme::change(appearance, None, cx);
    zoom_base_font_size.set(Theme::global(cx).font_size);
    Theme::global_mut(cx).font_size =
        application_ui_zoom_font_size(zoom_base_font_size.get(), zoom_level);
    Theme::sync_base(cx);
    if let Some(window) = window {
        window.refresh();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application_shell::{
        APPLICATION_UI_ZOOM_DEFAULT_LEVEL, application_ui_zoom_font_size,
    };
    use gpui::{TestAppContext, px};

    #[gpui::test]
    fn theme_transitions_do_not_compound_application_zoom(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let zoom_level = 1;
        let zoom_base_font_size = Cell::new(px(16.));

        for appearance in [
            WindowAppearance::Dark,
            WindowAppearance::Light,
            WindowAppearance::Dark,
            WindowAppearance::Light,
        ] {
            cx.update(|cx| {
                apply_window_appearance_with_zoom_base(
                    appearance,
                    None,
                    cx,
                    zoom_level,
                    &zoom_base_font_size,
                );
                assert_eq!(zoom_base_font_size.get(), px(16.));
                assert_eq!(
                    Theme::global(cx).font_size,
                    application_ui_zoom_font_size(px(16.), zoom_level),
                    "the zoomed font must remain one level above the unzoomed base"
                );
            });
        }
        assert_eq!(zoom_level, 1);
        assert_ne!(zoom_level, APPLICATION_UI_ZOOM_DEFAULT_LEVEL);
    }
}
