use gpui::{
    div, px, InteractiveElement as _, IntoElement, ParentElement as _, SharedString, Styled as _,
    WindowOptions,
};
use gpui_component::{h_flex, TitleBar};

pub const APPLICATION_TITLE: &str = "GPUI Migration";
#[cfg(target_os = "macos")]
// The pinned TitleBar reserves this leading lane for native traffic lights but
// does not expose a centred-title slot. Mirror that platform inset on the
// trailing edge so child content is centred in the window, not the free lane.
pub const MACOS_TITLE_BAR_CONTROL_INSET: gpui::Pixels = px(80.);

/// Formats the window title from the active document and the number of open
/// documents. The document tab remains the owner of dirty-state presentation.
pub fn format_window_title(active_document_name: Option<&str>, document_count: usize) -> String {
    let Some(active_document_name) = active_document_name else {
        return APPLICATION_TITLE.to_owned();
    };
    let other_document_count = document_count.saturating_sub(1);
    if other_document_count == 0 {
        format!("{active_document_name} — {APPLICATION_TITLE}")
    } else {
        format!("{active_document_name} (+{other_document_count}) — {APPLICATION_TITLE}")
    }
}

/// Returns the uncustomised GPUI Component window configuration required by
/// the title bar's native controls, dragging, and double-click behaviour.
pub fn title_bar_window_options() -> WindowOptions {
    TitleBar::window_options()
}

pub fn window_title_bar(
    title: impl Into<SharedString>,
    window_width: gpui::Pixels,
    background: gpui::Hsla,
) -> TitleBar {
    // Match the menu surface while retaining the stock separator, draggable
    // row and native controls.
    TitleBar::new()
        .bg(background)
        .child(window_title_content(title, window_width))
}

fn window_title_content(
    title: impl Into<SharedString>,
    window_width: gpui::Pixels,
) -> impl IntoElement {
    let content = h_flex()
        .id("window-title-lane")
        .min_w_0()
        .overflow_hidden()
        .justify_center();
    #[cfg(target_os = "macos")]
    let content = content
        .w(window_width - MACOS_TITLE_BAR_CONTROL_INSET)
        .pr(MACOS_TITLE_BAR_CONTROL_INSET);
    #[cfg(not(target_os = "macos"))]
    let content = content.w_full();
    content.child(
        div()
            .id("window-title-text")
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .text_xs()
            .text_center()
            .text_ellipsis()
            .whitespace_nowrap()
            .child(title.into()),
    )
}
