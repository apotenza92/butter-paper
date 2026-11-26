//! App Appearance: the application theme is Light, Dark or Match System,
//! independent of Page Appearance. Match App pages follow the app theme.
//!
//! GPUI's test platform has no public way to switch the system appearance,
//! so these tests send the system's new appearance through the functions
//! that a window's appearance observer calls.

use std::{cell::Cell, rc::Rc};

use butter_paper::app_appearance::{AppAppearance, AppAppearancePreference};
use butter_paper::application_shell::PageAppearancePreference;
use butter_paper::document_workspace::DocumentWorkspace;
use butter_paper::paper_appearance::{PageAppearance, PaperAppearance};
use butter_paper::system_theme::{
    apply_paper_for_system, apply_window_appearance, apply_window_appearance_with_zoom_base,
    follow_window_paper,
};
use gpui::{AppContext as _, Entity, TestAppContext, WindowAppearance, px};
use gpui_component::{Root, Theme, ThemeMode};

fn set_app(appearance: AppAppearance, cx: &mut gpui::App) {
    cx.set_global(AppAppearancePreference(appearance));
}

fn set_page(appearance: PageAppearance, cx: &mut gpui::App) {
    cx.set_global(PageAppearancePreference(appearance));
}

/// Opens a window whose workspace follows the settings, as a document window
/// does.
fn open_window(
    cx: &mut TestAppContext,
) -> (Entity<DocumentWorkspace>, &mut gpui::VisualTestContext) {
    let slot = Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let slot = slot.clone();
        move |window, cx| {
            let workspace = cx.new(DocumentWorkspace::new);
            follow_window_paper(workspace.downgrade(), window, cx);
            slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = slot.borrow_mut().take().unwrap();
    (workspace, cx)
}

fn mode(cx: &gpui::App) -> ThemeMode {
    Theme::global(cx).mode
}

fn paper(
    workspace: &Entity<DocumentWorkspace>,
    cx: &mut gpui::VisualTestContext,
) -> PaperAppearance {
    workspace.read_with(cx, |workspace, _| workspace.paper_appearance())
}

/// A change of the setting shows in an open window at once, and the system's
/// change shows only while the setting is Match System.
#[gpui::test]
fn the_app_theme_switches_live_and_only_match_system_follows_the_system(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        cx.set_reduce_motion(true);
    });
    let (_workspace, cx) = open_window(cx);
    let zoom_level = Cell::new(0i8);
    let base = Cell::new(px(16.));
    // The setting changes with the system showing Light, then Dark.
    for (setting, system, expected) in [
        (
            AppAppearance::Dark,
            WindowAppearance::Light,
            ThemeMode::Dark,
        ),
        (
            AppAppearance::Light,
            WindowAppearance::Dark,
            ThemeMode::Light,
        ),
        (
            AppAppearance::MatchSystem,
            WindowAppearance::Dark,
            ThemeMode::Dark,
        ),
        (
            AppAppearance::MatchSystem,
            WindowAppearance::Light,
            ThemeMode::Light,
        ),
    ] {
        cx.update(|_, cx| {
            set_app(setting, cx);
            apply_window_appearance_with_zoom_base(system, None, cx, zoom_level.get(), &base);
            assert_eq!(mode(cx), expected, "{setting:?} with the system {system:?}");
        });
    }
    // Match System follows a system switch, both ways.
    cx.update(|window, cx| {
        set_app(AppAppearance::MatchSystem, cx);
        apply_window_appearance(WindowAppearance::Dark, window, cx);
        assert_eq!(mode(cx), ThemeMode::Dark);
        apply_window_appearance(WindowAppearance::VibrantLight, window, cx);
        assert_eq!(mode(cx), ThemeMode::Light);
    });
    // A forced theme ignores every system switch.
    for (setting, forced) in [
        (AppAppearance::Dark, ThemeMode::Dark),
        (AppAppearance::Light, ThemeMode::Light),
    ] {
        cx.update(|window, cx| {
            set_app(setting, cx);
            apply_window_appearance_with_zoom_base(
                WindowAppearance::Light,
                None,
                cx,
                zoom_level.get(),
                &base,
            );
            for system in [
                WindowAppearance::Dark,
                WindowAppearance::Light,
                WindowAppearance::VibrantDark,
                WindowAppearance::VibrantLight,
            ] {
                apply_window_appearance(system, window, cx);
                assert_eq!(mode(cx), forced, "{setting:?} with the system {system:?}");
            }
        });
    }
}

/// The font size keeps its zoom while the theme switches through the setting.
#[gpui::test]
fn switching_the_app_theme_keeps_the_application_zoom(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let zoom_level = Cell::new(1i8);
    let base = Cell::new(px(16.));
    let mut sizes = Vec::new();
    for setting in [
        AppAppearance::Dark,
        AppAppearance::Light,
        AppAppearance::Dark,
    ] {
        cx.update(|cx| {
            set_app(setting, cx);
            apply_window_appearance_with_zoom_base(
                WindowAppearance::Light,
                None,
                cx,
                zoom_level.get(),
                &base,
            );
            sizes.push(Theme::global(cx).font_size);
        });
    }
    assert_eq!(sizes[0], sizes[1]);
    assert_eq!(sizes[1], sizes[2]);
}

/// Original and Force Dark pages never change with the app theme, whatever
/// the app or the system shows.
#[gpui::test]
fn a_forced_app_theme_does_not_change_original_or_force_dark_pages(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        cx.set_reduce_motion(true);
    });
    let (workspace, cx) = open_window(cx);
    let zoom_level = Cell::new(0i8);
    let base = Cell::new(px(16.));
    for (page, expected) in [
        (PageAppearance::Original, PaperAppearance::Real),
        (PageAppearance::ForceDark, PaperAppearance::Dark),
    ] {
        for app in AppAppearance::ALL {
            for system in [WindowAppearance::Light, WindowAppearance::Dark] {
                cx.update(|_, cx| {
                    set_page(page, cx);
                    set_app(app, cx);
                    apply_window_appearance_with_zoom_base(
                        system,
                        None,
                        cx,
                        zoom_level.get(),
                        &base,
                    );
                    apply_paper_for_system(&workspace.downgrade(), system, cx);
                });
                assert_eq!(
                    paper(&workspace, cx),
                    expected,
                    "{page:?} with the app {app:?} and the system {system:?}"
                );
            }
        }
    }
}

/// Match App pages change with App Appearance, and, while App Appearance is
/// Match System, with the system's switch.
#[gpui::test]
fn match_app_pages_follow_the_app_theme(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        cx.set_reduce_motion(true);
    });
    let (workspace, cx) = open_window(cx);
    let zoom_level = Cell::new(0i8);
    let base = Cell::new(px(16.));
    let apply = |system: WindowAppearance, cx: &mut gpui::App| {
        apply_window_appearance_with_zoom_base(system, None, cx, zoom_level.get(), &base);
        apply_paper_for_system(&workspace.downgrade(), system, cx);
    };
    cx.update(|_, cx| set_page(PageAppearance::MatchApp, cx));

    // App Appearance changes the pages, whatever the system shows.
    for system in [WindowAppearance::Light, WindowAppearance::Dark] {
        for (app, expected) in [
            (AppAppearance::Dark, PaperAppearance::Dark),
            (AppAppearance::Light, PaperAppearance::Real),
            (AppAppearance::Dark, PaperAppearance::Dark),
        ] {
            cx.update(|_, cx| {
                set_app(app, cx);
                apply(system, cx);
            });
            assert_eq!(
                paper(&workspace, cx),
                expected,
                "the app {app:?} with the system {system:?}"
            );
        }
    }

    // With Match System, one system switch changes the app and the pages.
    cx.update(|_, cx| set_app(AppAppearance::MatchSystem, cx));
    for (system, theme, expected) in [
        (
            WindowAppearance::Dark,
            ThemeMode::Dark,
            PaperAppearance::Dark,
        ),
        (
            WindowAppearance::Light,
            ThemeMode::Light,
            PaperAppearance::Real,
        ),
        (
            WindowAppearance::VibrantDark,
            ThemeMode::Dark,
            PaperAppearance::Dark,
        ),
    ] {
        cx.update(|_, cx| {
            apply(system, cx);
            assert_eq!(mode(cx), theme);
        });
        assert_eq!(paper(&workspace, cx), expected, "the system {system:?}");
    }
}
