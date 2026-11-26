use std::{cell::Cell, rc::Rc};

use gpui::{App, Pixels, Subscription, WeakEntity, Window, WindowAppearance};
use gpui_component::Theme;

use crate::app_appearance::AppAppearancePreference;
use crate::application_shell::{PageAppearancePreference, application_ui_zoom_font_size};
use crate::document_workspace::DocumentWorkspace;
use crate::paper_appearance::PaperAppearance;

/// Whether the system's appearance is dark: what Match System follows.
pub fn is_dark_appearance(appearance: WindowAppearance) -> bool {
    matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

/// The appearance the application theme takes when the system's is
/// `system`: the App Appearance setting decides. Light and Dark ignore the
/// system. Match System, the default, follows it.
pub fn app_theme_appearance(system: WindowAppearance, cx: &App) -> WindowAppearance {
    AppAppearancePreference::current(cx).resolve(system)
}

/// Whether the app theme is dark when the system's appearance is `system`:
/// what Match App follows.
pub fn app_is_dark(system: WindowAppearance, cx: &App) -> bool {
    is_dark_appearance(app_theme_appearance(system, cx))
}

/// The paper pages draw with when the system's appearance is `system`: the
/// Page Appearance setting, with Match App following the app theme.
pub fn paper_for_system(system: WindowAppearance, cx: &App) -> PaperAppearance {
    PageAppearancePreference::current(cx).resolve(app_is_dark(system, cx))
}

/// Gives `workspace` the paper its settings resolve to when the system's
/// appearance is `system`.
pub fn apply_paper_for_system(
    workspace: &WeakEntity<DocumentWorkspace>,
    system: WindowAppearance,
    cx: &mut App,
) {
    let paper = paper_for_system(system, cx);
    let _ = workspace.update(cx, |workspace, cx| {
        workspace.set_paper_appearance(paper, cx)
    });
}

/// Keeps `workspace`'s paper in step with the settings and the system
/// appearance of `window`: now, and each time the system's appearance
/// changes (Match App follows the app theme, which Match System takes from
/// the system). A change of a setting calls [`apply_paper_for_system`].
pub fn follow_window_paper(
    workspace: WeakEntity<DocumentWorkspace>,
    window: &mut Window,
    cx: &mut App,
) {
    apply_paper_for_system(&workspace, window.appearance(), cx);
    window
        .observe_window_appearance(move |window, cx| {
            apply_paper_for_system(&workspace, window.appearance(), cx);
        })
        .detach();
}

/// Applies the application theme for the system appearance `system`, through
/// GPUI Component's complete theme transition, including its gpui-base
/// projection. A forced App Appearance (Light or Dark) keeps its theme.
pub fn apply_window_appearance(system: WindowAppearance, window: &mut Window, cx: &mut App) {
    Theme::change(app_theme_appearance(system, cx), Some(window), cx);
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

/// Applies the application theme for the system appearance `system` and
/// keeps the shell's zoom on top of the new theme's font size. Use it when
/// the App Appearance setting changes, in any window or none: the theme is
/// global, so one call changes every window.
pub fn apply_window_appearance_with_zoom_base(
    system: WindowAppearance,
    window: Option<&mut Window>,
    cx: &mut App,
    zoom_level: i8,
    zoom_base_font_size: &Cell<Pixels>,
) {
    // Theme configs may omit `font_size`; reset the previous zoom first so an
    // omitted value cannot make the current zoomed size become the new base.
    Theme::global_mut(cx).font_size = zoom_base_font_size.get();
    Theme::sync_base(cx);
    Theme::change(app_theme_appearance(system, cx), None, cx);
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
