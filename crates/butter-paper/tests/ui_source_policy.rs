//! Bounded source checks for UI conventions (see AGENTS.md, "UI lint policy").
//! These are pattern checks over the Rust sources, not a parser or a rendered
//! colour test; native interaction tests and visual review remain required.

use std::path::PathBuf;

use regex::Regex;

fn source(file: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src").join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let from = source.find(start).unwrap_or_else(|| panic!("missing {start}"));
    let to = from + source[from..].find(end).unwrap_or_else(|| panic!("missing {end}"));
    &source[from..to]
}

fn matches(pattern: &str, text: &str) -> bool {
    Regex::new(pattern).unwrap().is_match(text)
}

fn count(pattern: &str, text: &str) -> usize {
    Regex::new(pattern).unwrap().find_iter(text).count()
}

/// Direct numeric `rgb`/`rgba`/`hsla` constructor calls. Domain colour
/// conversions stay valid; literals belong in a reviewed token layer.
fn literal_colour_calls(source: &str) -> Vec<String> {
    Regex::new(r"\b(?:rgb|rgba|hsla)\s*\(\s*(?:0x[\da-fA-F]+|\d+(?:\.\d*)?)")
        .unwrap()
        .find_iter(source)
        .map(|found| found.as_str().to_string())
        .collect()
}

fn embedded_measurement_sections(appearance: &str, measurement: &str) -> bool {
    appearance.contains(".content_only(self.embedded && !snapshot.show_offset)")
        && measurement.contains(".content_only(self.embedded)")
}

#[test]
fn colour_check_detects_literal_constructors_but_allows_tokens_and_document_colours() {
    assert_eq!(literal_colour_calls("div().bg(gpui::rgb(0x000000))").len(), 1);
    assert_eq!(literal_colour_calls("div().border_color(rgba(0xff0000ff))").len(), 1);
    assert_eq!(literal_colour_calls("div().bg(hsla(0., 0., 0., 1.))").len(), 1);
    assert!(
        literal_colour_calls(
            "div().bg(cx.theme().background); gpui::Rgba::from(annotation_colour); rgb(document_colour)"
        )
        .is_empty()
    );
}

#[test]
fn inspectors_panels_viewer_toolbar_and_system_theme_use_tokens_or_domain_colours() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let pattern = Regex::new(r"(?:inspector|panel|toolbar_strip|system_theme)\.rs$").unwrap();
    let files: Vec<String> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| pattern.is_match(name))
        .collect();
    assert!(files.iter().any(|file| file == "dimension_property_inspector.rs"));
    assert!(files.iter().any(|file| file == "viewer_toolbar_strip.rs"));
    for file in files {
        assert_eq!(
            literal_colour_calls(&source(&file)),
            Vec::<String>::new(),
            "{file}: use semantic theme tokens for control chrome; keep annotation colours as domain values"
        );
    }
}

#[test]
fn shared_interaction_chrome_is_the_single_authored_canvas_colour_source() {
    // Item controls are unpainted, so the shared painter has no handle colours.
    let allowed = [
        "rgb(0x2563eb", "rgb(0xffffff", "rgb(0x94a3b8", "rgb(0x93c5fd", "rgb(0x1d4ed8",
        "rgb(0x0f766e", "rgb(0x22c55e",
    ];
    // Each interaction colour owns one semantic role, so each appears once.
    assert_eq!(literal_colour_calls(&source("interaction_chrome.rs")), allowed);
}

#[test]
fn embedded_appearance_and_measurement_sections_retain_one_scroll_owner() {
    let appearance = source("dimension_property_inspector.rs");
    let measurement = source("measurement_property_inspector.rs");
    assert!(
        embedded_measurement_sections(&appearance, &measurement),
        "embedded measurement sections must contribute content to the outer inspector"
    );
    let detached = appearance.replace(
        ".content_only(self.embedded && !snapshot.show_offset)",
        ".content_only(false)",
    );
    assert!(!embedded_measurement_sections(&detached, &measurement));
    let detached = measurement.replace(".content_only(self.embedded)", ".content_only(false)");
    assert!(!embedded_measurement_sections(&appearance, &detached));
}

#[test]
fn quit_transaction_replaces_the_stock_footer_rather_than_adding_another_ok() {
    let source = source("application_close_workspace.rs");
    let dialog = between(&source, "pub fn open_dialog(", "fn reconcile_dialog_window(");
    assert!(dialog.contains(".footer(gpui::Empty)"));
    assert!(dialog.contains("for action in dialog.actions"));
    assert!(dialog.contains(".child(footer)"));
    // Enter still follows the safe Save All transaction.
    assert!(matches(r"\.on_ok\((?s:.)*?ApplicationCloseAction::SaveAll", dialog));
}

#[test]
fn quit_shortcuts_remain_platform_correct() {
    let source = source("application_shell.rs");
    let bindings = between(&source, "pub fn init_application_shell_actions(", "#[cfg(test)]");
    assert!(matches(
        r#"#\[cfg\(target_os = "macos"\)\](?s:.)*?KeyBinding::new\(\s*"cmd-q",\s*crate::application_close_workspace::RequestApplicationQuit"#,
        bindings
    ));
    // Windows and Linux close the app with its last window, not a shortcut.
    assert!(!bindings.contains("\"ctrl-q\""));
}

#[test]
fn active_workspace_has_zoom_and_page_modes_without_cad_activation() {
    let source = source("document_workspace.rs");
    assert!(source.contains("ViewerToolbarStrip::new_with_zoom("));
    assert!(!matches(
        r"ViewerToolbarStrip::new_with_cad_view\(|CadViewControl::|handle_cad_view_control_event",
        &source
    ));
}

#[test]
fn both_workspace_tab_paths_use_stock_outline_without_visual_overrides() {
    let source = source("document_workspace.rs");
    let bars: Vec<String> =
        Regex::new(r#"TabBar::new\("document-workspace-session-tabs-component"\)((?s:.)*?)\.children\("#)
            .unwrap()
            .captures_iter(&source)
            .map(|capture| capture[1].to_string())
            .collect();
    assert_eq!(bars.len(), 2, "cover loaded and empty/loading workspace paths");
    for style in bars {
        assert!(style.contains(".outline()"));
        assert!(style.contains(".menu(false)"), "keep the measured external overflow control");
        assert!(style.contains(".max_width(px(190.))"), "retain document label truncation");
        assert!(!matches(r"\.(pill|segmented|bg|p_0)\(", &style));
    }
}

#[test]
fn close_controls_overlay_naturally_sized_labels_without_a_reserved_suffix() {
    let source = source("document_workspace.rs");
    assert_eq!(count(r"\.child\(\s*session_tab_close_lane\(\)", &source), 2);
    assert_eq!(count(r"\.child\(session_tab_overlay_label\(", &source), 2);
    let label = between(&source, "fn session_tab_overlay_label", "impl Render for DocumentWorkspace");
    assert!(matches(r"\.opacity\(0\.\)\s*\.child\(label\.clone\(\)\)", label));
    // Revealing the close button keeps the label's measured start and
    // truncates against the close lane instead of re-centring.
    assert!(label.contains(".when(revealed, |this| this.pl(text_offset.get()).pr_6())"));
    assert!(label.contains(".when(!revealed, |this| this.text_center())"));
    assert!(!label.contains(".group_hover("));
}

#[test]
fn close_affordances_reveal_on_tab_hover_and_keyboard_focus_without_removing_layout() {
    let source = source("document_workspace.rs");
    assert_eq!(
        count(
            r"\.opacity\(if reveal_close \{ 1\. \} else \{ 0\. \}\)\s*\.focus\(\|style\| style\.opacity\(1\.\)\)",
            &source
        ),
        2
    );
}

#[test]
fn close_fills_blend_with_their_tab_until_the_close_target_is_hovered() {
    let source = source("document_workspace.rs");
    let closes: Vec<String> = Regex::new(r"Button::new\(close_id\)((?s:.)*?)\.small\(\)")
        .unwrap()
        .captures_iter(&source)
        .map(|capture| capture[1].to_string())
        .collect();
    assert_eq!(closes.len(), 2);
    for style in closes {
        assert!(matches(
            r"\.color\(cx.theme\(\).transparent\)\s*\.hover\(cx.theme\(\).background\)",
            &style
        ));
        assert!(style.contains(".bg(cx.theme().transparent)"));
        assert!(style.contains(".border_color(cx.theme().transparent)"));
    }
}
