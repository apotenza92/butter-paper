//! Settings presentation; the application owns persistence and update operations.
use crate::{
    application_shell::*,
    native_application::{UpdateMenuState, UpdateMenuStatus},
    native_update_policy::UpdateFrequency,
};
use gpui::{
    AnyElement, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScrollHandle, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, StyledExt as _,
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    scroll::ScrollableElement as _,
    sidebar::{Sidebar, SidebarMenu},
    text::TextView,
    v_flex,
};

#[derive(Clone, Default)]
pub struct ApplicationUpdatePresentation {
    pub state: UpdateMenuState,
    pub message: String,
    pub notes: String,
}
impl gpui::Global for ApplicationUpdatePresentation {}

/// Ready updates stay out of the way until installation is one restart away.
pub fn update_button(state: UpdateMenuStatus, cx: &App) -> Option<AnyElement> {
    let UpdateMenuStatus::Ready(version) = state else {
        return None;
    };
    Some(
        h_flex()
            .id("application-update-lane")
            .debug_selector(|| "application-update-lane".into())
            .self_stretch()
            .pl_3()
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("application-update-button")
                    .debug_selector(|| "application-update-button".into())
                    .accessibility_id("application-update-button")
                    .outline()
                    .label("Update ready")
                    .icon(Icon::default().path(crate::application_assets::DOWNLOAD_ICON))
                    .tooltip("The update is downloaded and ready to install")
                    .dropdown_menu(move |menu, _, _| {
                        menu.label(format!("Butter Paper {version} is ready"))
                            .item(PopupMenuItem::new("Restart to update").on_click(
                                |_, window, cx| {
                                    window.dispatch_action(Box::new(RestartToUpdate), cx);
                                },
                            ))
                            .separator()
                            .menu("View release notes", Box::new(OpenUpdateSettings))
                    }),
            )
            .into_any_element(),
    )
}

pub fn open_about(window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, |dialog, window, _| {
        dialog
            .title("About Butter Paper")
            .w(px(420.).min(window.viewport_size().width - px(32.)))
            .child(
                v_flex()
                    .id("application-about-content")
                    .debug_selector(|| "application-about-content".into())
                    .gap_3()
                    .child(format!("Butter Paper {}", env!("CARGO_PKG_VERSION")))
                    .child("A native PDF workspace for viewing, drawing and marking up documents.")
                    .child(
                        Button::new("about-releases")
                            .label("View releases")
                            .on_click(|_, _, cx| cx.open_url(APPLICATION_RELEASES_URL)),
                    ),
            )
    });
}

pub fn open_settings(window: &mut Window, cx: &mut App) {
    open_settings_category(SettingsCategory::Updates, window, cx);
}

pub fn open_update_settings(window: &mut Window, cx: &mut App) {
    open_settings_category(SettingsCategory::Updates, window, cx);
}

fn open_settings_category(category: SettingsCategory, window: &mut Window, cx: &mut App) {
    if let Some(existing) = cx.try_global::<SettingsWindow>().cloned()
        && existing
            .handle
            .update(cx, |_, window, cx| {
                existing.view.update(cx, |view, cx| {
                    view.category = category;
                    view.scroll.set_offset(gpui::point(px(0.), px(0.)));
                    cx.notify();
                });
                window.activate_window();
            })
            .is_ok()
    {
        return;
    }
    let bounds = gpui::Bounds::centered(
        None,
        gpui::size(px(960.), px(640.)).min(&window.viewport_size()),
        cx,
    );
    let opened = cx.open_window(
        gpui::WindowOptions {
            window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
            ..crate::window_title_bar::title_bar_window_options()
        },
        |window, cx| {
            window.set_window_title("Settings — Butter Paper");
            let view = cx.new(|cx| SettingsView::new(category, cx));
            view.read(cx).focus.clone().focus(window, cx);
            let root = cx.new(|cx| gpui_component::Root::new(view.clone(), window, cx));
            cx.set_global(SettingsWindow {
                handle: window
                    .window_handle()
                    .downcast::<gpui_component::Root>()
                    .unwrap(),
                view,
            });
            root
        },
    );
    if let Err(error) = opened {
        eprintln!("Couldn’t open Settings: {error}");
    }
}

#[derive(Clone)]
struct SettingsWindow {
    handle: gpui::WindowHandle<gpui_component::Root>,
    view: gpui::Entity<SettingsView>,
}
impl gpui::Global for SettingsWindow {}

#[cfg(feature = "review-driver")]
pub fn review_window(cx: &App) -> Option<gpui::AnyWindowHandle> {
    cx.try_global::<SettingsWindow>()
        .map(|state| state.handle.into())
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SettingsCategory {
    Updates,
}

struct SettingsView {
    category: SettingsCategory,
    scroll: ScrollHandle,
    _updates: Subscription,
    focus: gpui::FocusHandle,
}
impl SettingsView {
    fn new(category: SettingsCategory, cx: &mut Context<Self>) -> Self {
        Self {
            category,
            scroll: ScrollHandle::new(),
            _updates: cx.observe_global::<ApplicationUpdatePresentation>(|_, cx| cx.notify()),
            focus: cx.focus_handle(),
        }
    }

    fn render_update_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let presentation = cx
            .try_global::<ApplicationUpdatePresentation>()
            .cloned()
            .unwrap_or_default();
        let state = presentation.state;
        let frequency = state.frequency;
        let available = matches!(state.status, UpdateMenuStatus::Available(_));
        let ready = matches!(state.status, UpdateMenuStatus::Ready(_));
        let busy = matches!(
            state.status,
            UpdateMenuStatus::Checking | UpdateMenuStatus::Downloading
        );
        let message = if presentation.message.is_empty() {
            match state.status {
                UpdateMenuStatus::Unavailable => "Automatic updates are available in installed release builds. Download a release to update this copy.".into(),
                UpdateMenuStatus::Store => "The Microsoft Store keeps Butter Paper up to date.".into(),
                UpdateMenuStatus::Idle => "Check for a newer version of Butter Paper.".into(),
                UpdateMenuStatus::Checking => "Checking for updates…".into(),
                UpdateMenuStatus::Available(version) => format!("Butter Paper {version} is available."),
                UpdateMenuStatus::Downloading => "Downloading and preparing the update… You can keep working.".into(),
                UpdateMenuStatus::Ready(version) => format!("Butter Paper {version} is ready. Restart to install, or it installs when you quit. You’ll be asked to save any unsaved work."),
            }
        } else {
            presentation.message
        };
        let action = Button::new("settings-update-action")
            .debug_selector(|| "settings-update-action".into())
            .accessibility_id("settings-update-action")
            .primary()
            .label(match state.status {
                UpdateMenuStatus::Checking => "Checking…",
                UpdateMenuStatus::Downloading => "Downloading…",
                UpdateMenuStatus::Ready(_) => "Restart to update",
                UpdateMenuStatus::Available(_) => "Download update",
                _ => "Check for updates",
            })
            .disabled(busy || state.status == UpdateMenuStatus::Unavailable)
            .on_click(move |_, window, cx| {
                let action: Box<dyn gpui::Action> = if ready {
                    Box::new(RestartToUpdate)
                } else if available {
                    Box::new(DownloadUpdate)
                } else {
                    Box::new(CheckForUpdates)
                };
                window.dispatch_action(action, cx);
            });
        let schedule = Button::new("settings-update-frequency")
            .debug_selector(|| "settings-update-frequency".into())
            .outline()
            .disabled(frequency.is_none())
            .label(frequency_label(frequency))
            .dropdown_caret(true)
            .dropdown_menu(move |mut menu, _, _| {
                for (label, value, action) in frequency_options() {
                    menu = menu.menu_with_check(label, frequency == Some(value), action);
                }
                menu
            });
        let store = state.status == UpdateMenuStatus::Store;
        let version = v_flex()
            .id("settings-version-section")
            .debug_selector(|| "settings-version-section".into())
            .gap_3()
            .pb_6()
            .child(div().font_semibold().child("Version"))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Installed version {}", env!("CARGO_PKG_VERSION"))),
            )
            .child(message)
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .when(!store, |row| row.child(action))
                    .child(
                        Button::new("settings-releases")
                            .label("View releases")
                            .on_click(|_, _, cx| cx.open_url(APPLICATION_RELEASES_URL)),
                    ),
            )
            .into_any_element();
        // The Store updates its copies itself: no checks or downloads here.
        if store {
            return vec![version];
        }
        vec![
            version,
            v_flex().id("settings-automatic-section").debug_selector(|| "settings-automatic-section".into()).gap_3().pb_6()
                .child(div().font_semibold().child("Automatic checks"))
                .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Choose how often Butter Paper looks for updates."))
                .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Updates download automatically after a successful check. You choose when to restart."))
                .child(schedule).into_any_element(),
            v_flex().id("settings-notes-section").debug_selector(|| "settings-notes-section".into()).gap_3()
                .child(div().font_semibold().child("What’s new"))
                .child(TextView::markdown("update-release-notes", if presentation.notes.is_empty() { "Release notes appear here when an update is available.".to_owned() } else { presentation.notes }))
                .into_any_element(),
        ]
    }
}
impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let compact = window.viewport_size().width < window.rem_size() * 48.;
        let categories = [(
            SettingsCategory::Updates,
            Icon::default().path(crate::application_assets::DOWNLOAD_ICON),
            "Updates",
            "settings-category-updates",
        )];
        let category_buttons = categories.map(|(category, icon, label, id)| {
            let button = Button::new(id)
                .debug_selector(move || id.into())
                .ghost()
                .icon(icon)
                .tooltip(label)
                .selected(self.category == category)
                .toggled(self.category == category);
            let button = if compact {
                button.small().label(label)
            } else {
                button
            };
            crate::accessible_button::accessible_icon_button(button, label).on_click(cx.listener(
                move |view, _, _, cx| {
                    if view.category != category {
                        view.category = category;
                        view.scroll.set_offset(gpui::point(px(0.), px(0.)));
                        cx.notify();
                    }
                },
            ))
        });
        let sections = match self.category {
            SettingsCategory::Updates => self.render_update_sections(cx),
        };
        let (title, headings): (&str, &[&str]) = match self.category {
            SettingsCategory::Updates => {
                ("Updates", &["Version", "Automatic checks", "What’s new"])
            }
        };
        // Store copies have only the Version section.
        let headings = &headings[..sections.len().min(headings.len())];
        let scroll = v_flex()
            .id("application-settings-scroll")
            .debug_selector(|| "application-settings-scroll".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .pr_2()
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .children(sections)
            .vertical_scrollbar(&self.scroll);
        let body = if compact {
            let weak = cx.entity().downgrade();
            let headings = headings
                .iter()
                .map(|heading| (*heading).to_owned())
                .collect::<Vec<_>>();
            let jump = Button::new("settings-mobile-jump")
                .debug_selector(|| "settings-mobile-jump".into())
                .outline()
                .small()
                .label("Jump to section")
                .tooltip("Jump to a heading in this category")
                .dropdown_caret(true)
                .dropdown_menu(move |mut menu, _, _| {
                    for (ix, heading) in headings.iter().enumerate() {
                        let weak = weak.clone();
                        menu = menu.item(PopupMenuItem::new(heading.clone()).on_click(
                            move |_, _, cx| {
                                let _ = weak.update(cx, |view, cx| {
                                    view.scroll.scroll_to_top_of_item(ix);
                                    cx.notify();
                                });
                            },
                        ));
                    }
                    menu
                });
            v_flex()
                .id("application-settings-screen")
                .debug_selector(|| "application-settings-screen".into())
                .w_full()
                .h_full()
                .p_4()
                .gap_4()
                .child(
                    h_flex()
                        .id("settings-mobile-navigation")
                        .debug_selector(|| "settings-mobile-navigation".into())
                        .flex_shrink_0()
                        .flex_wrap()
                        .gap_2()
                        .pb_3()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .children(category_buttons)
                        .child(jump),
                )
                .child(scroll)
                .into_any_element()
        } else {
            let rail = v_flex().w_16().h_full().flex_shrink_0().child(
                Sidebar::<SidebarMenu>::new("settings-category-rail")
                    .collapsible(false)
                    .w_full()
                    .header(
                        v_flex()
                            .w_full()
                            .items_center()
                            .gap_1()
                            .children(category_buttons),
                    ),
            );
            let sidebar = v_flex()
                .id("settings-heading-sidebar")
                .debug_selector(|| "settings-heading-sidebar".into())
                .w_48()
                .h_full()
                .flex_shrink_0()
                .child(
                    Sidebar::<SidebarMenu>::new("settings-section-navigation")
                        .collapsible(false)
                        .w_full()
                        .header(
                            v_flex()
                                .w_full()
                                .gap_1()
                                .child(
                                    div()
                                        .px_2()
                                        .h_8()
                                        .flex()
                                        .items_center()
                                        .text_xs()
                                        .font_medium()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(title),
                                )
                                .children(headings.iter().enumerate().map(|(ix, heading)| {
                                    crate::accessible_button::accessible_icon_button(
                                        Button::new(("settings-heading", ix))
                                            .debug_selector(move || format!("settings-jump-{ix}"))
                                            .ghost()
                                            .w_full()
                                            .tooltip(format!("Jump to {heading}"))
                                            .child(div().w_full().text_left().child(*heading)),
                                        *heading,
                                    )
                                    .on_click(cx.listener(
                                        move |view, _, _, cx| {
                                            view.scroll.scroll_to_top_of_item(ix);
                                            cx.notify();
                                        },
                                    ))
                                })),
                        ),
                );
            h_flex()
                .id("application-settings-screen")
                .debug_selector(|| "application-settings-screen".into())
                .w_full()
                .items_start()
                .h_full()
                .child(rail)
                .child(sidebar)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .p_5()
                        .child(scroll.h_full()),
                )
                .into_any_element()
        };
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .key_context("Settings")
            .track_focus(&self.focus)
            .on_action(|_: &crate::document_workspace::CloseDocument, window, _| {
                window.remove_window()
            })
            .on_action(
                |_: &crate::application_close_workspace::RequestApplicationClose, window, _| {
                    window.remove_window()
                },
            )
            .children(
                crate::window_title_bar::uses_window_title_bar(window).then(|| {
                    crate::window_title_bar::window_title_bar(
                        "Settings — Butter Paper",
                        window.viewport_size().width,
                        cx.theme().background,
                    )
                }),
            )
            .child(div().flex_1().min_h_0().size_full().child(body))
            .children(gpui_component::Root::render_dialog_layer(window, cx))
            .children(gpui_component::Root::render_sheet_layer(window, cx))
    }
}

fn frequency_label(frequency: Option<UpdateFrequency>) -> &'static str {
    frequency_options()
        .into_iter()
        .find(|(_, value, _)| Some(*value) == frequency)
        .map(|(label, _, _)| label)
        .unwrap_or("Unavailable")
}
fn frequency_options() -> [(&'static str, UpdateFrequency, Box<dyn gpui::Action>); 8] {
    use UpdateFrequency::*;
    [
        ("Never", Never, Box::new(SetUpdateFrequencyNever)),
        ("At startup", Startup, Box::new(SetUpdateFrequencyAtStartup)),
        ("Hourly", Hourly, Box::new(SetUpdateFrequencyHourly)),
        (
            "Every 6 hours",
            SixHours,
            Box::new(SetUpdateFrequencyEverySixHours),
        ),
        (
            "Every 12 hours",
            TwelveHours,
            Box::new(SetUpdateFrequencyEveryTwelveHours),
        ),
        ("Daily", Daily, Box::new(SetUpdateFrequencyDaily)),
        ("Weekly", Weekly, Box::new(SetUpdateFrequencyWeekly)),
        ("Monthly", Monthly, Box::new(SetUpdateFrequencyMonthly)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext};
    use std::{cell::Cell, rc::Rc};

    #[gpui::test]
    fn settings_update_command_tracks_live_state_and_blocks_duplicate_work(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_component::init);
        let checks = Rc::new(Cell::new(0));
        let downloads = Rc::new(Cell::new(0));
        let restarts = Rc::new(Cell::new(0));
        cx.update(|cx| {
            let checks = checks.clone();
            cx.on_action(move |_: &CheckForUpdates, _| checks.set(checks.get() + 1));
            let downloads = downloads.clone();
            cx.on_action(move |_: &DownloadUpdate, _| downloads.set(downloads.get() + 1));
            let restarts = restarts.clone();
            cx.on_action(move |_: &RestartToUpdate, _| restarts.set(restarts.get() + 1));
        });
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| SettingsView::new(SettingsCategory::Updates, cx));
            gpui_component::Root::new(view, window, cx)
        });
        let version = crate::native_updater::ReleaseVersion::stable(0, 1, 5);
        for (status, expected) in [
            (UpdateMenuStatus::Unavailable, (0, 0, 0)),
            (UpdateMenuStatus::Idle, (1, 0, 0)),
            (UpdateMenuStatus::Checking, (1, 0, 0)),
            (UpdateMenuStatus::Available(version), (1, 1, 0)),
            (UpdateMenuStatus::Downloading, (1, 1, 0)),
            (UpdateMenuStatus::Ready(version), (1, 1, 1)),
        ] {
            cx.update(|window, cx| {
                cx.set_global(ApplicationUpdatePresentation {
                    state: UpdateMenuState {
                        status,
                        frequency: Some(UpdateFrequency::Daily),
                    },
                    ..Default::default()
                });
                window.draw(cx).clear(cx);
            });
            let bounds = cx.debug_bounds("settings-update-action").unwrap();
            cx.simulate_click(bounds.center(), Modifiers::default());
            cx.run_until_parked();
            assert_eq!((checks.get(), downloads.get(), restarts.get()), expected);
        }
    }

    #[gpui::test]
    fn store_copies_show_their_version_without_update_controls(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| SettingsView::new(SettingsCategory::Updates, cx));
            gpui_component::Root::new(view, window, cx)
        });
        cx.update(|window, cx| {
            cx.set_global(ApplicationUpdatePresentation {
                state: UpdateMenuState {
                    status: UpdateMenuStatus::Store,
                    frequency: None,
                },
                ..Default::default()
            });
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("settings-version-section").is_some());
        assert!(cx.debug_bounds("settings-update-action").is_none());
        assert!(cx.debug_bounds("settings-automatic-section").is_none());
        assert!(cx.debug_bounds("settings-notes-section").is_none());
        assert!(cx.debug_bounds("settings-jump-0").is_some());
        assert!(cx.debug_bounds("settings-jump-1").is_none());
    }

    #[gpui::test]
    fn heading_shortcuts_scroll_one_content_pane_and_restore_the_top(cx: &mut TestAppContext) {
        use std::cell::RefCell;
        cx.update(gpui_component::init);
        let retained = Rc::new(RefCell::new(None));
        let (_, cx) = cx.add_window_view({
            let retained = retained.clone();
            move |window, cx| {
                cx.set_global(ApplicationUpdatePresentation {
                    notes: "Release detail.\n\n".repeat(100),
                    ..Default::default()
                });
                let view = cx.new(|cx| SettingsView::new(SettingsCategory::Updates, cx));
                retained.replace(Some(view.clone()));
                gpui_component::Root::new(view, window, cx)
            }
        });
        let view = retained.borrow_mut().take().unwrap();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let shortcut = cx.debug_bounds("settings-jump-2").unwrap();
        cx.simulate_click(shortcut.center(), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(view.read_with(cx, |view, _| view.scroll.offset().y < px(0.)));
        assert_eq!(
            cx.debug_bounds("settings-jump-2").unwrap(),
            shortcut,
            "navigation stays fixed while content scrolls"
        );
        let top = cx.debug_bounds("settings-jump-0").unwrap();
        cx.simulate_click(top.center(), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(view.read_with(cx, |view, _| view.scroll.offset().y), px(0.));
        assert!(cx.debug_bounds("settings-category-general").is_none());
        for id in [
            "settings-interface-smaller",
            "settings-interface-reset",
            "settings-interface-bigger",
        ] {
            assert!(cx.debug_bounds(id).is_none());
        }
    }

    #[gpui::test]
    fn compact_jump_opens_from_its_label_and_keyboard_jumps_to_release_notes(
        cx: &mut TestAppContext,
    ) {
        use std::cell::RefCell;
        cx.update(gpui_component::init);
        let retained = Rc::new(RefCell::new(None));
        let (_, cx) = cx.add_window_view({
            let retained = retained.clone();
            move |window, cx| {
                cx.set_global(ApplicationUpdatePresentation {
                    notes: "Release detail.\n\n".repeat(100),
                    ..Default::default()
                });
                let view = cx.new(|cx| SettingsView::new(SettingsCategory::Updates, cx));
                retained.replace(Some(view.clone()));
                gpui_component::Root::new(view, window, cx)
            }
        });
        let view = retained.borrow_mut().take().unwrap();
        cx.simulate_resize(gpui::size(px(360.), px(640.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let trigger = cx.debug_bounds("settings-mobile-jump").unwrap();
        cx.simulate_click(trigger.center(), Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.simulate_keystrokes("down down down enter");
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let notes = cx.debug_bounds("settings-notes-section").unwrap();
        let viewport = cx.debug_bounds("application-settings-scroll").unwrap();
        assert_eq!(
            notes.top(),
            viewport.top(),
            "the release-notes heading is at the top of the content pane"
        );
        assert!(view.read_with(cx, |view, _| view.scroll.offset().y < px(0.)));
    }

    struct EmptyView;
    impl Render for EmptyView {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .children(gpui_component::Root::render_dialog_layer(window, cx))
        }
    }

    #[gpui::test]
    fn settings_switches_to_compact_navigation_and_stays_inside_small_windows(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| SettingsView::new(SettingsCategory::Updates, cx));
            gpui_component::Root::new(view, window, cx)
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        for width in [1200., 700., 320.] {
            cx.simulate_resize(gpui::size(px(width), px(520.)));
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let screen = cx
                .debug_bounds("application-settings-screen")
                .unwrap_or_else(|| panic!("missing settings at {width}"));
            assert_eq!(screen.left(), px(0.));
            assert_eq!(screen.right(), px(width));
            assert!(screen.top() >= px(0.) && screen.bottom() <= px(520.));
            assert_eq!(
                cx.debug_bounds("settings-mobile-navigation").is_some(),
                width < 768.
            );
            assert_eq!(
                cx.debug_bounds("settings-heading-sidebar").is_some(),
                width >= 768.
            );
        }
    }

    #[gpui::test]
    fn about_stays_inside_a_narrow_window(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| EmptyView);
            gpui_component::Root::new(view, window, cx)
        });
        cx.simulate_resize(gpui::size(px(320.), px(520.)));
        cx.update(|window, cx| {
            open_about(window, cx);
            window.draw(cx).clear(cx);
        });
        let about = cx.debug_bounds("application-about-content").unwrap();
        assert!(about.left() >= px(0.) && about.right() <= px(320.));
        assert!(about.top() >= px(0.) && about.bottom() <= px(520.));
    }

    #[gpui::test]
    fn settings_reuses_its_window_and_reopens_after_close(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| EmptyView);
            gpui_component::Root::new(view, window, cx)
        });
        cx.update(|window, cx| {
            open_settings(window, cx);
            let first = cx.global::<SettingsWindow>().clone();
            open_update_settings(window, cx);
            assert_eq!(first.handle, cx.global::<SettingsWindow>().handle);
            assert!(first.view.read(cx).category == SettingsCategory::Updates);
            first
                .handle
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let first = cx.global::<SettingsWindow>().handle;
            open_settings(window, cx);
            assert_ne!(first, cx.global::<SettingsWindow>().handle);
            assert!(!window.has_active_dialog(cx));
        });
    }

    #[gpui::test]
    fn quiet_update_button_is_only_present_when_ready(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let version = crate::native_updater::ReleaseVersion::stable(0, 1, 5);
            for status in [
                UpdateMenuStatus::Idle,
                UpdateMenuStatus::Unavailable,
                UpdateMenuStatus::Store,
                UpdateMenuStatus::Checking,
                UpdateMenuStatus::Available(version),
                UpdateMenuStatus::Downloading,
            ] {
                assert!(update_button(status, cx).is_none());
            }
            assert!(update_button(UpdateMenuStatus::Ready(version), cx).is_some());
        });
    }
}
