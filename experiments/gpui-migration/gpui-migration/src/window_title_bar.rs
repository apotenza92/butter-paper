use gpui::{
    InteractiveElement as _, IntoElement, ParentElement as _, SharedString, Styled as _,
    WindowOptions, div, px,
};
use gpui_component::{TitleBar, h_flex};

pub const APPLICATION_TITLE: &str = "Butter Paper";
#[cfg(target_os = "macos")]
// The pinned TitleBar reserves this leading lane for native traffic lights but
// does not expose a centred-title slot. Mirror that platform inset on the
// trailing edge so child content is centred in the window, not the free lane.
pub const MACOS_TITLE_BAR_CONTROL_INSET: gpui::Pixels = px(80.);

/// Formats the window title from the active document and the number of open
/// documents. The document tab remains the owner of dirty-state presentation.
/// Longest list of tab names before the rest are counted ("+3").
pub const WINDOW_TITLE_MAX_CHARS: usize = 100;
const WINDOW_TITLE_SEPARATOR: &str = " | ";

/// The window title lists its tabs without extensions, the active tab first
/// and the rest in tab order: "second | review | third". With no tabs it is
/// the application title. A long list ends with a count of the tabs left out.
pub fn format_window_title(tab_names: &[&str], active_index: Option<usize>) -> String {
    format_window_title_for_application(tab_names, active_index, APPLICATION_TITLE)
}

pub fn format_window_title_for_application(
    tab_names: &[&str],
    active_index: Option<usize>,
    application_title: &str,
) -> String {
    if tab_names.is_empty() {
        return application_title.to_owned();
    }
    let active = active_index.filter(|index| *index < tab_names.len());
    let ordered = active
        .into_iter()
        .chain((0..tab_names.len()).filter(|index| Some(*index) != active))
        .map(|index| crate::document_tab_bar::format_document_tab_label(tab_names[index]));
    let mut title = String::new();
    let mut shown = 0;
    for name in ordered {
        let separator = if shown == 0 { "" } else { WINDOW_TITLE_SEPARATOR };
        let remaining_after = tab_names.len() - shown - 1;
        let suffix_reserve = if remaining_after > 0 { 6 } else { 0 };
        if shown > 0
            && title.chars().count() + separator.len() + name.chars().count() + suffix_reserve
                > WINDOW_TITLE_MAX_CHARS
        {
            break;
        }
        title.push_str(separator);
        title.push_str(name);
        shown += 1;
    }
    let hidden = tab_names.len() - shown;
    if hidden > 0 {
        title.push_str(&format!("{WINDOW_TITLE_SEPARATOR}+{hidden}"));
    }
    title
}

/// AppKit owns the macOS title bar, including the user's double-click action
/// (Fill, Zoom, Minimise or Do Nothing). Other platforms use GPUI Component.
pub fn title_bar_window_options() -> WindowOptions {
    #[cfg(target_os = "macos")]
    {
        WindowOptions::default()
    }
    #[cfg(not(target_os = "macos"))]
    {
        TitleBar::window_options()
    }
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
