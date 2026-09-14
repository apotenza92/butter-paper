use butter_paper_gpui_migration::window_title_bar::{
    APPLICATION_TITLE, format_window_title, title_bar_window_options,
};

#[test]
fn empty_workspace_uses_the_application_title() {
    assert_eq!(format_window_title(None, 0), APPLICATION_TITLE);
}

#[test]
fn one_document_names_the_document_and_application() {
    assert_eq!(
        format_window_title(Some("Drawing.pdf"), 1),
        "Drawing.pdf — GPUI Migration"
    );
}

#[test]
fn multiple_documents_include_the_other_document_count() {
    assert_eq!(
        format_window_title(Some("Drawing.pdf"), 4),
        "Drawing.pdf (+3) — GPUI Migration"
    );
}

#[test]
fn title_bar_window_options_preserve_component_owned_chrome_behaviour() {
    let options = title_bar_window_options();
    let titlebar = options.titlebar.expect("the title bar must be enabled");

    assert!(titlebar.appears_transparent);
    assert!(titlebar.title.is_none());
    assert!(titlebar.traffic_light_position.is_some());
    assert!(options.app_owns_titlebar_drag);
}

#[test]
fn title_bar_matches_the_application_surface_and_preserves_the_stock_separator() {
    use gpui::{Styled, px};
    use butter_paper_gpui_migration::window_title_bar::window_title_bar;
    for background in [gpui::Hsla::white(), gpui::Hsla::black()] {
        let mut title_bar = window_title_bar("Drawing.pdf — GPUI Migration", px(1200.), background);
        let style = title_bar.style();
        assert_eq!(style.border_widths.bottom, None, "inherit the stock bottom border");
        assert_eq!(style.background, Some(background.into()));
    }
}
