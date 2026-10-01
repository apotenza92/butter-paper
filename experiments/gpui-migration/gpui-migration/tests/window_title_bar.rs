use butter_paper_gpui_migration::window_title_bar::{
    APPLICATION_TITLE, WINDOW_TITLE_MAX_CHARS, format_window_title,
    format_window_title_for_application, title_bar_window_options,
};

#[test]
fn empty_workspace_uses_the_application_title() {
    assert_eq!(format_window_title(&[], None), APPLICATION_TITLE);
    assert_eq!(
        format_window_title_for_application(&[], None, "Butter Paper Beta"),
        "Butter Paper Beta"
    );
}

#[test]
fn tabs_are_listed_without_extensions_active_first_then_in_tab_order() {
    assert_eq!(format_window_title(&["Drawing.pdf"], Some(0)), "Drawing");
    assert_eq!(
        format_window_title(&["review.pdf", "third.PDF", "second.pdf"], Some(2)),
        "second | review | third"
    );
    assert_eq!(
        format_window_title(&["review.pdf", "third.pdf"], None),
        "review | third"
    );
}

#[test]
fn long_tab_lists_end_with_the_count_left_out() {
    let names = (0..30).map(|index| format!("Site plan {index}.pdf")).collect::<Vec<_>>();
    let names = names.iter().map(String::as_str).collect::<Vec<_>>();
    let title = format_window_title(&names, Some(0));
    assert!(title.starts_with("Site plan 0 | Site plan 1 | "));
    assert!(title.chars().count() <= WINDOW_TITLE_MAX_CHARS + 6, "{title}");
    let shown = title.matches("Site plan").count();
    assert!(title.ends_with(&format!(" | +{}", 30 - shown)), "{title}");
}

#[test]
#[cfg(not(target_os = "macos"))]
fn title_bar_window_options_preserve_component_owned_chrome_behaviour() {
    let options = title_bar_window_options();
    let titlebar = options.titlebar.expect("the title bar must be enabled");

    assert!(titlebar.appears_transparent);
    assert!(titlebar.title.is_none());
    assert!(titlebar.traffic_light_position.is_some());
    assert!(options.app_owns_titlebar_drag);
}

#[test]
#[cfg(target_os = "macos")]
fn macos_title_bar_leaves_native_gestures_and_controls_to_appkit() {
    let options = title_bar_window_options();
    let titlebar = options.titlebar.expect("a native title bar is required");
    assert!(
        !titlebar.appears_transparent,
        "content must not cover the native title bar"
    );
    assert!(
        titlebar.traffic_light_position.is_none(),
        "AppKit positions its controls"
    );
    assert!(
        !options.app_owns_titlebar_drag,
        "AppKit must receive title bar gestures"
    );
    assert!(options.is_movable);
    assert!(options.is_resizable);
    assert!(options.is_minimizable);
    assert_eq!(options.kind, gpui::WindowKind::Normal);
}

#[test]
fn title_bar_matches_the_application_surface_and_preserves_the_stock_separator() {
    use butter_paper_gpui_migration::window_title_bar::window_title_bar;
    use gpui::{Styled, px};
    for background in [gpui::Hsla::white(), gpui::Hsla::black()] {
        let mut title_bar = window_title_bar("Drawing.pdf — Butter Paper", px(1200.), background);
        let style = title_bar.style();
        assert_eq!(
            style.border_widths.bottom, None,
            "inherit the stock bottom border"
        );
        assert_eq!(style.background, Some(background.into()));
    }
}
