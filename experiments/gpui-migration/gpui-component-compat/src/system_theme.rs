use gpui::{App, Subscription, Window, WindowAppearance};
use gpui_component::Theme;

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
