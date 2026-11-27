use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use butter_paper::{
    annotation_model::{MarkupId, PdfPoint},
    application_close_workspace::{
        ApplicationCloseShell, ApplicationCloseWorkspace, RequestApplicationClose,
        RequestApplicationQuit, register_application_close_action,
    },
    application_shell::{NewWindow, ToggleApplicationFullScreen, ToggleApplicationMenuBar},
    document_workspace::{
        ActualSize, CloseDocument, ContinuousView, ConvertToWord, DOCUMENT_OPEN_PROGRESS_ID,
        DOCUMENT_OPEN_STATUS_ID, DocumentOpenBatchDisposition, DocumentOpenBatchRequest,
        DocumentOpenBatchStatus, DocumentOpenOrigin, DocumentWorkspace, FitPage, FitWidth,
        NativeDocumentOpener, NativeDocumentResource, NativeDocumentSaver, NavigateNextPage,
        NavigatePreviousPage, NewFromTemplate, OpenDocumentRequest, OpenPdf, OpenedNativeDocument,
        RasterSurface, RotatePageLeft, RotatePageRight, Save, SaveAs, SaveDocumentAsTemplate,
        SaveDocumentRequest, SavedNativeDocument, SinglePageView, ThumbnailSurface,
        ToggleCoverPageAlone, ToggleSplitView, TwoPagesView, ZoomIn, ZoomOut,
    },
    native_application::{
        ApplicationMenuShellState, NativeApplicationMenuState, NativeDocumentIngress,
        UpdateMenuState, UpdateMenuStatus, build_in_window_application_menus,
        build_native_application_menus, build_native_application_menus_with_shell,
    },
    viewer::TileRequest,
    viewer_toolbar_strip::VIEWER_TOOLBAR_CONTENT_ID,
};
use gpui::{
    Action, AppContext as _, ExternalPaths, FileDropEvent, Menu, MenuItem, TestAppContext, point,
    px,
};
use gpui_component::Root;
use gpui_component::input::{Copy, Cut, Delete, Paste, Redo, SelectAll, Undo};

#[test]
fn native_application_menu_uses_document_actions_and_never_exposes_raw_quit() {
    let menus = build_native_application_menus(NativeApplicationMenuState {
        has_active_document: true,
        save_busy: false,
        has_focused_input: false,
        can_undo: true,
        can_redo: false,
        can_cut: true,
        can_copy: false,
        can_paste: true,
        can_select_all: false,
        can_delete: true,
        can_explode_block: false,
        can_close_document: true,
        document_ready: true,
        can_previous_page: false,
        can_next_page: true,
        rotation_busy: false,
        can_zoom_out: true,
        can_zoom_in: true,
        actual_size_checked: false,
        fit_width_checked: true,
        fit_page_checked: false,
        continuous_view_checked: true,
        single_page_view_checked: false,
        two_pages_view_checked: false,
        cover_page_alone_checked: true,
        split_view_checked: true,
        can_move_document_to_new_window: true,
        can_merge_all_windows: true,
    });

    assert_eq!(
        menu_names(&menus),
        ["Butter Paper", "File", "Edit", "Document", "View", "Window",]
    );
    let app = menu(&menus, "Butter Paper");
    assert_action(item(app, "Quit Butter Paper"), |action| {
        action.as_any().is::<RequestApplicationQuit>()
    });
    let file = menu(&menus, "File");
    assert_action(item(file, "Open…"), |action| {
        action.as_any().is::<OpenPdf>()
    });
    assert_action(item(file, "New from Template…"), |action| {
        action.as_any().is::<NewFromTemplate>()
    });
    assert_action(item(file, "Save Document as Template…"), |action| {
        action.as_any().is::<SaveDocumentAsTemplate>()
    });
    assert_action(item(file, "Save"), |action| action.as_any().is::<Save>());
    assert_action(item(file, "Save As…"), |action| {
        action.as_any().is::<SaveAs>()
    });
    assert_action(item(file, "Close Document"), |action| {
        action.as_any().is::<CloseDocument>()
    });
    assert!(!item(file, "Close Document").is_disabled());
    assert!(!item(file, "Save").is_disabled());
    assert!(!item(file, "Save As…").is_disabled());
    assert!(!item(file, "Save Document as Template…").is_disabled());
    assert_action(item(file, "Convert to Word Document…"), |action| {
        action.as_any().is::<ConvertToWord>()
    });
    assert!(!item(file, "Convert to Word Document…").is_disabled());

    let edit = menu(&menus, "Edit");
    assert_eq!(
        menu_item_names(edit),
        [
            "Undo",
            "Redo",
            "<separator>",
            "Cut",
            "Copy",
            "Paste",
            "Delete",
            "<separator>",
            "Explode Block",
            "<separator>",
            "Select All",
            "<separator>",
            "Find",
            "Find in Documents",
        ]
    );
    assert_action(item(edit, "Undo"), |action| action.as_any().is::<Undo>());
    assert_action(item(edit, "Redo"), |action| action.as_any().is::<Redo>());
    assert_action(item(edit, "Cut"), |action| action.as_any().is::<Cut>());
    assert_action(item(edit, "Copy"), |action| action.as_any().is::<Copy>());
    assert_action(item(edit, "Paste"), |action| action.as_any().is::<Paste>());
    // Explode Block waits for a block selected in Edit PDF.
    assert_action(item(edit, "Explode Block"), |action| {
        action
            .as_any()
            .is::<butter_paper::document_workspace::ExplodeBlock>()
    });
    assert!(item(edit, "Explode Block").is_disabled());
    assert_action(item(edit, "Delete"), |action| {
        action.as_any().is::<Delete>()
    });
    assert_action(item(edit, "Select All"), |action| {
        action.as_any().is::<SelectAll>()
    });
    // Find opens the find bar; Find in Documents, the search panel.
    assert_action(item(edit, "Find"), |action| {
        action
            .as_any()
            .is::<butter_paper::document_workspace::FindInDocument>()
    });
    assert_action(item(edit, "Find in Documents"), |action| {
        action
            .as_any()
            .is::<butter_paper::document_workspace::FindInDocuments>()
    });
    assert!(!item(edit, "Undo").is_disabled());
    assert!(item(edit, "Redo").is_disabled());
    assert!(!item(edit, "Cut").is_disabled());
    assert!(item(edit, "Copy").is_disabled());
    assert!(!item(edit, "Paste").is_disabled());
    assert!(!item(edit, "Delete").is_disabled());
    assert!(item(edit, "Select All").is_disabled());

    let document = menu(&menus, "Document");
    assert_eq!(
        menu_item_names(document),
        [
            "Previous Page",
            "Next Page",
            "<separator>",
            "Rotate Left",
            "Rotate Right",
            "Rotate All Pages Left",
            "Rotate All Pages Right",
            "<separator>",
            "Move Document to New Window",
        ]
    );
    assert!(!item(document, "Move Document to New Window").is_disabled());
    assert_action(item(document, "Previous Page"), |action| {
        action.as_any().is::<NavigatePreviousPage>()
    });
    assert_action(item(document, "Next Page"), |action| {
        action.as_any().is::<NavigateNextPage>()
    });
    assert_action(item(document, "Rotate Left"), |action| {
        action.as_any().is::<RotatePageLeft>()
    });
    assert_action(item(document, "Rotate Right"), |action| {
        action.as_any().is::<RotatePageRight>()
    });
    assert_action(item(document, "Rotate All Pages Left"), |action| {
        action
            .as_any()
            .is::<butter_paper::document_workspace::RotateAllPagesLeft>()
    });
    assert_action(item(document, "Rotate All Pages Right"), |action| {
        action
            .as_any()
            .is::<butter_paper::document_workspace::RotateAllPagesRight>()
    });
    assert!(item(document, "Previous Page").is_disabled());
    assert!(!item(document, "Next Page").is_disabled());
    assert!(!item(document, "Rotate Left").is_disabled());
    assert!(!item(document, "Rotate Right").is_disabled());
    assert!(!item(document, "Rotate All Pages Left").is_disabled());
    assert!(!item(document, "Rotate All Pages Right").is_disabled());

    let view = menu(&menus, "View");
    assert_eq!(
        menu_item_names(view),
        [
            "Zoom In",
            "Zoom Out",
            "Actual Size",
            "<separator>",
            "Fit Width",
            "Fit Page",
            "<separator>",
            "Continuous View",
            "Single Page View",
            "Two Pages View",
            "Cover Page Alone",
            "<separator>",
            "Split View",
        ]
    );
    assert_action(item(view, "Zoom In"), |action| {
        action.as_any().is::<ZoomIn>()
    });
    assert_action(item(view, "Zoom Out"), |action| {
        action.as_any().is::<ZoomOut>()
    });
    assert_action(item(view, "Actual Size"), |action| {
        action.as_any().is::<ActualSize>()
    });
    assert_action(item(view, "Fit Width"), |action| {
        action.as_any().is::<FitWidth>()
    });
    assert_action(item(view, "Fit Page"), |action| {
        action.as_any().is::<FitPage>()
    });
    assert_action(item(view, "Continuous View"), |action| {
        action.as_any().is::<ContinuousView>()
    });
    assert_action(item(view, "Single Page View"), |action| {
        action.as_any().is::<SinglePageView>()
    });
    assert!(item(view, "Fit Width").is_checked());
    assert!(!item(view, "Fit Page").is_checked());
    assert!(item(view, "Continuous View").is_checked());
    assert!(!item(view, "Single Page View").is_checked());
    assert_action(item(view, "Two Pages View"), |action| {
        action.as_any().is::<TwoPagesView>()
    });
    assert_action(item(view, "Cover Page Alone"), |action| {
        action.as_any().is::<ToggleCoverPageAlone>()
    });
    assert_action(item(view, "Split View"), |action| {
        action.as_any().is::<ToggleSplitView>()
    });
    assert!(!item(view, "Two Pages View").is_checked());
    assert!(item(view, "Cover Page Alone").is_checked());
    assert!(item(view, "Split View").is_checked());

    let disabled = build_native_application_menus(NativeApplicationMenuState::default());
    let file = menu(&disabled, "File");
    assert!(item(file, "Save").is_disabled());
    assert!(item(file, "Save As…").is_disabled());
    assert!(item(file, "Save Document as Template…").is_disabled());
    assert!(item(file, "Close Document").is_disabled());
    let edit = menu(&disabled, "Edit");
    for name in [
        "Undo",
        "Redo",
        "Cut",
        "Copy",
        "Paste",
        "Delete",
        "Select All",
    ] {
        assert!(item(edit, name).is_disabled(), "{name} must be disabled");
    }
    for name in [
        "Previous Page",
        "Next Page",
        "Rotate Left",
        "Rotate Right",
        "Rotate All Pages Left",
        "Rotate All Pages Right",
    ] {
        assert!(
            item(menu(&disabled, "Document"), name).is_disabled(),
            "{name} must be disabled"
        );
    }
    // A page still turning holds back every rotation, as saving does.
    for busy in [
        NativeApplicationMenuState {
            document_ready: true,
            has_active_document: true,
            rotation_busy: true,
            ..Default::default()
        },
        NativeApplicationMenuState {
            document_ready: true,
            has_active_document: true,
            save_busy: true,
            ..Default::default()
        },
    ] {
        let busy = build_native_application_menus(busy);
        for name in [
            "Rotate Left",
            "Rotate Right",
            "Rotate All Pages Left",
            "Rotate All Pages Right",
        ] {
            assert!(
                item(menu(&busy, "Document"), name).is_disabled(),
                "{name} must be disabled while busy"
            );
        }
    }
    for name in [
        "Zoom In",
        "Zoom Out",
        "Actual Size",
        "Fit Width",
        "Fit Page",
        "Continuous View",
        "Single Page View",
    ] {
        assert!(
            item(menu(&disabled, "View"), name).is_disabled(),
            "{name} must be disabled"
        );
    }

    let focused_input = build_native_application_menus(NativeApplicationMenuState {
        has_focused_input: true,
        ..Default::default()
    });
    let edit = menu(&focused_input, "Edit");
    for name in [
        "Undo",
        "Redo",
        "Cut",
        "Copy",
        "Paste",
        "Delete",
        "Select All",
    ] {
        assert!(!item(edit, name).is_disabled(), "{name} must be enabled");
    }

    let busy = build_native_application_menus(NativeApplicationMenuState {
        has_active_document: true,
        save_busy: true,
        ..Default::default()
    });
    let file = menu(&busy, "File");
    assert!(item(file, "Save").is_disabled());
    assert!(item(file, "Save As…").is_disabled());
    assert!(item(file, "Save Document as Template…").is_disabled());
    assert!(item(menu(&busy, "Document"), "Rotate Left").is_disabled());
    assert!(item(menu(&busy, "Document"), "Rotate Right").is_disabled());

    let window = menu(&menus, "Window");
    assert_action(item(window, "Close Window"), |action| {
        action.as_any().is::<RequestApplicationClose>()
    });
    #[cfg(target_os = "macos")]
    {
        assert_action(item(window, "Minimise"), |action| {
            action
                .as_any()
                .is::<butter_paper::application_shell::MinimiseWindow>()
        });
        assert_action(item(window, "Zoom"), |action| {
            action
                .as_any()
                .is::<butter_paper::application_shell::ZoomWindow>()
        });
    }
    assert_action(item(file, "New Window"), |action| {
        action.as_any().is::<NewWindow>()
    });
}

#[test]
fn native_application_menu_disables_save_commands_until_the_active_document_is_ready() {
    let menus = build_native_application_menus(NativeApplicationMenuState {
        has_active_document: true,
        can_close_document: true,
        document_ready: false,
        ..Default::default()
    });
    let file = menu(&menus, "File");

    assert!(item(file, "Save").is_disabled());
    assert!(item(file, "Save As…").is_disabled());
    assert!(item(file, "Save Document as Template…").is_disabled());
    assert!(item(file, "Sign with Certificate…").is_disabled());
    assert!(
        !item(file, "Close Document").is_disabled(),
        "a loading or failed document must remain independently closable"
    );
}

#[test]
fn in_window_application_menu_is_the_four_menu_projection() {
    let menus = build_in_window_application_menus(
        NativeApplicationMenuState {
            has_active_document: true,
            document_ready: true,
            can_undo: true,
            can_copy: true,
            can_select_all: true,
            ..Default::default()
        },
        ApplicationMenuShellState {
            menu_bar_visible: false,
            menu_bar_visibility_supported: true,
            reverse_scroll_zoom: false,
            app_appearance: Default::default(),
            page_appearance: Default::default(),
            updates: Default::default(),
        },
    );

    assert_eq!(menu_names(&menus), ["Butter Paper", "File", "Edit", "View"]);
    assert_eq!(
        menu_item_names(menu(&menus, "File")),
        [
            "New Window",
            "New from Template…",
            "Open…",
            "<separator>",
            "Save",
            "Save As…",
            "Save Document as Template…",
            "<separator>",
            "Merge Documents…",
            "Convert to Word Document…",
            "Sign with Certificate…",
        ]
    );
    assert_eq!(
        menu_item_names(menu(&menus, "Edit")),
        [
            "Undo",
            "Redo",
            "<separator>",
            "Cut",
            "Copy",
            "Paste",
            "<separator>",
            "Explode Block",
            "<separator>",
            "Select All",
            "<separator>",
            "Find",
            "Find in Documents",
        ]
    );
    let mut view = vec![
        // The page views and Split View, as the native View menu has them.
        "Continuous View",
        "Single Page View",
        "Two Pages View",
        "Cover Page Alone",
        "<separator>",
        "Split View",
        "<separator>",
        "Show Menu Bar in App Windows",
        "<separator>",
        "Reverse Scroll Zoom",
        "<separator>",
        // The App Appearance and Page Appearance submenus, side by side.
        "<other>",
        "<other>",
        "<separator>",
        "Toggle Full Screen",
    ];
    if !cfg!(target_os = "macos") {
        view.extend(["<separator>", "Merge All Windows"]);
    }
    assert_eq!(menu_item_names(menu(&menus, "View")), view);
    assert!(!item(menu(&menus, "View"), "Show Menu Bar in App Windows").is_checked());
    assert!(
        !menu_item_names(menu(&menus, "View"))
            .iter()
            .any(|name| *name == "Reload" || *name == "Force Reload")
    );

    assert_action(
        item(menu(&menus, "View"), "Show Menu Bar in App Windows"),
        |action| action.as_any().is::<ToggleApplicationMenuBar>(),
    );
    assert_action(item(menu(&menus, "View"), "Toggle Full Screen"), |action| {
        action.as_any().is::<ToggleApplicationFullScreen>()
    });
}

#[test]
fn native_application_projection_retains_document_edit_commands_and_restores_the_menu_bar() {
    let menus = build_native_application_menus_with_shell(
        NativeApplicationMenuState {
            has_active_document: true,
            document_ready: true,
            can_delete: true,
            can_explode_block: false,
            ..Default::default()
        },
        ApplicationMenuShellState {
            menu_bar_visible: false,
            menu_bar_visibility_supported: true,
            reverse_scroll_zoom: false,
            app_appearance: Default::default(),
            page_appearance: Default::default(),
            updates: Default::default(),
        },
    );

    assert!(
        menu(&menus, "Edit")
            .items
            .iter()
            .any(|item| matches!(item, MenuItem::Action { name, .. } if name == "Delete"))
    );
    assert!(!item(menu(&menus, "View"), "Show Menu Bar in App Windows").is_checked());
    assert_action(
        item(menu(&menus, "View"), "Show Menu Bar in App Windows"),
        |action| action.as_any().is::<ToggleApplicationMenuBar>(),
    );
}

/// Merge All Windows is in the Window menu on macOS (AppKit adds no tab
/// commands of its own, as document windows have no tabbing identifier) and
/// in the View menu of both menu surfaces elsewhere. It needs two windows.
#[test]
fn merge_all_windows_is_in_the_window_menu_on_macos_and_the_view_menu_elsewhere() {
    use butter_paper::application_shell::MergeAllWindows;
    let shell = ApplicationMenuShellState {
        menu_bar_visibility_supported: !cfg!(target_os = "macos"),
        ..Default::default()
    };
    for can_merge in [false, true] {
        let state = NativeApplicationMenuState {
            can_merge_all_windows: can_merge,
            ..Default::default()
        };
        let native = build_native_application_menus_with_shell(state, shell);
        let in_window = build_in_window_application_menus(state, shell);
        let holders = |menus: &[Menu]| {
            menus
                .iter()
                .filter(|menu| menu_item_names(menu).contains(&"Merge All Windows"))
                .map(|menu| menu.name.to_string())
                .collect::<Vec<_>>()
        };
        if cfg!(target_os = "macos") {
            assert_eq!(holders(&native), ["Window"]);
            assert_eq!(
                menu_item_names(menu(&native, "Window")),
                [
                    "Minimise",
                    "Zoom",
                    "<separator>",
                    "Merge All Windows",
                    "<separator>",
                    "Close Window",
                ]
            );
            assert!(holders(&in_window).is_empty());
        } else {
            assert_eq!(holders(&native), ["View"]);
            assert_eq!(holders(&in_window), ["View"]);
            assert_eq!(menu_item_names(menu(&native, "Window")), ["Close Window"]);
            let item = item(menu(&in_window, "View"), "Merge All Windows");
            assert_eq!(item.is_disabled(), !can_merge);
            assert_action(item, |action| action.as_any().is::<MergeAllWindows>());
        }
        let menu_name = if cfg!(target_os = "macos") {
            "Window"
        } else {
            "View"
        };
        let item = item(menu(&native, menu_name), "Merge All Windows");
        assert_eq!(
            item.is_disabled(),
            !can_merge,
            "one window has nothing to merge"
        );
        assert_action(item, |action| action.as_any().is::<MergeAllWindows>());
    }
}

/// Users cannot change the interface size for now (decisions.md, "Interface
/// size"): no menu in either menu surface offers it.
#[test]
fn no_menu_offers_an_interface_size() {
    for menus in [
        build_native_application_menus(Default::default()),
        build_native_application_menus_with_shell(
            Default::default(),
            ApplicationMenuShellState {
                menu_bar_visibility_supported: true,
                ..Default::default()
            },
        ),
        build_in_window_application_menus(Default::default(), Default::default()),
    ] {
        for menu in &menus {
            for name in menu_item_names(menu) {
                let name = name.to_lowercase();
                assert!(
                    !name.contains("interface") && !name.contains("bigger"),
                    "{}: {name}",
                    menu.name
                );
            }
        }
    }
}

#[test]
fn about_and_settings_are_available_in_both_menu_surfaces() {
    use butter_paper::application_shell::{AboutButterPaper, CheckForUpdates, OpenSettings};
    for status in [
        UpdateMenuStatus::Unavailable,
        UpdateMenuStatus::Checking,
        UpdateMenuStatus::Available(butter_paper::native_updater::ReleaseVersion::stable(
            1, 0, 0,
        )),
        UpdateMenuStatus::Ready(butter_paper::native_updater::ReleaseVersion::stable(
            1, 0, 0,
        )),
    ] {
        let shell = ApplicationMenuShellState {
            updates: UpdateMenuState {
                status,
                frequency: None,
            },
            ..Default::default()
        };
        for menus in [
            build_native_application_menus_with_shell(Default::default(), shell),
            build_in_window_application_menus(Default::default(), shell),
        ] {
            let product = menu(&menus, "Butter Paper");
            assert_action(item(product, "About Butter Paper"), |action| {
                action.as_any().is::<AboutButterPaper>()
            });
            assert_action(item(product, "Settings…"), |action| {
                action.as_any().is::<OpenSettings>()
            });
            assert!(!item(product, "Settings…").is_disabled());
            // Every copy has Check for Updates; the rest lives in Settings.
            assert_action(item(product, "Check for Updates…"), |action| {
                action.as_any().is::<CheckForUpdates>()
            });
            assert!(
                !menu_item_names(product)
                    .iter()
                    .filter(|name| **name != "Check for Updates…")
                    .any(|name| name.contains("Update") || name.contains("Automatically"))
            );
        }
    }
}

/// View ▸ App Appearance and View ▸ Page Appearance: two submenus side by
/// side, in the native menu bar and the in-window View menu, each with its
/// setting checked. They follow Reverse Scroll Zoom, in a group of their own.
#[test]
fn the_appearance_submenus_are_side_by_side_with_the_settings_checked_in_both_menu_surfaces() {
    use butter_paper::app_appearance::AppAppearance;
    use butter_paper::application_shell::{
        SetAppAppearanceDark, SetAppAppearanceLight, SetAppAppearanceMatchSystem,
        SetPageAppearanceForceDark, SetPageAppearanceMatchApp, SetPageAppearanceOriginal,
    };
    use butter_paper::paper_appearance::PageAppearance;
    // Every pair of settings, so one submenu's check never follows the other's.
    for app_setting in AppAppearance::ALL {
        for page_setting in PageAppearance::ALL {
            let shell = ApplicationMenuShellState {
                menu_bar_visibility_supported: !cfg!(target_os = "macos"),
                app_appearance: app_setting,
                page_appearance: page_setting,
                ..Default::default()
            };
            let state = NativeApplicationMenuState::default();
            let native = build_native_application_menus_with_shell(state, shell);
            let in_window = build_in_window_application_menus(state, shell);
            for menus in [&native, &in_window] {
                let view = menu(menus, "View");
                let position = view
                    .items
                    .iter()
                    .position(|item| matches!(item, MenuItem::Submenu(submenu) if submenu.name == "App Appearance"))
                    .expect("View has an App Appearance submenu");
                // Reverse Scroll Zoom, a separator, App Appearance, Page
                // Appearance, a separator.
                assert!(matches!(
                    &view.items[position - 2],
                    MenuItem::Action { name, .. } if name == "Reverse Scroll Zoom"
                ));
                assert!(matches!(view.items[position - 1], MenuItem::Separator));
                assert!(matches!(
                    &view.items[position + 1],
                    MenuItem::Submenu(submenu) if submenu.name == "Page Appearance"
                ));
                assert!(matches!(view.items[position + 2], MenuItem::Separator));
                let (MenuItem::Submenu(app), MenuItem::Submenu(page)) =
                    (&view.items[position], &view.items[position + 1])
                else {
                    unreachable!()
                };
                assert_eq!(
                    menu_item_names(app),
                    ["Light", "Dark", "Match System"],
                    "title case in the menu bar"
                );
                assert_eq!(
                    menu_item_names(page),
                    ["Original", "Force Dark", "Match App"]
                );
                for (name, expected) in AppAppearance::ALL
                    .iter()
                    .map(|appearance| (appearance.label(), *appearance == app_setting))
                {
                    let MenuItem::Action { checked, .. } = item(app, name) else {
                        unreachable!()
                    };
                    assert_eq!(*checked, expected, "App {name} with {app_setting:?} set");
                }
                for (name, expected) in PageAppearance::ALL
                    .iter()
                    .map(|appearance| (appearance.label(), *appearance == page_setting))
                {
                    let MenuItem::Action { checked, .. } = item(page, name) else {
                        unreachable!()
                    };
                    assert_eq!(*checked, expected, "Page {name} with {page_setting:?} set");
                }
                assert_action(item(app, "Light"), |action| {
                    action.as_any().is::<SetAppAppearanceLight>()
                });
                assert_action(item(app, "Dark"), |action| {
                    action.as_any().is::<SetAppAppearanceDark>()
                });
                assert_action(item(app, "Match System"), |action| {
                    action.as_any().is::<SetAppAppearanceMatchSystem>()
                });
                assert_action(item(page, "Original"), |action| {
                    action.as_any().is::<SetPageAppearanceOriginal>()
                });
                assert_action(item(page, "Force Dark"), |action| {
                    action.as_any().is::<SetPageAppearanceForceDark>()
                });
                assert_action(item(page, "Match App"), |action| {
                    action.as_any().is::<SetPageAppearanceMatchApp>()
                });
                // Never disabled, and no shortcut shows: they are settings,
                // not document commands.
                for submenu in [app, page] {
                    for name in menu_item_names(submenu) {
                        assert!(!item(submenu, name).is_disabled(), "{name}");
                    }
                }
            }
        }
    }
}

#[test]
fn native_document_ingress_queues_file_urls_before_the_workspace_exists() {
    let ingress = NativeDocumentIngress::default();
    assert_eq!(
        ingress.enqueue_file_urls([
            "file:///tmp/Plan%20Set.PDF",
            "https://example.com/remote.pdf",
            "file:///tmp/notes.txt",
            "file:///tmp/Plan%20Set.PDF",
            "file://remote-host/share/drawing.pdf",
        ]),
        1
    );

    assert_eq!(
        ingress.take_requests(),
        [DocumentOpenBatchRequest::new(
            DocumentOpenOrigin::System,
            [PathBuf::from("/tmp/Plan Set.PDF")],
        )]
    );
    assert!(ingress.take_requests().is_empty());
}

#[test]
fn native_document_ingress_preserves_batch_order_and_drop_origin() {
    let ingress = NativeDocumentIngress::default();
    assert!(ingress.enqueue_request(DocumentOpenBatchRequest::new(
        DocumentOpenOrigin::System,
        [PathBuf::from("/tmp/first.pdf")],
    )));
    assert_eq!(
        ingress.enqueue_drop_paths([
            PathBuf::from("/tmp/first.pdf"),
            PathBuf::from("/tmp/notes.txt"),
            PathBuf::from("/tmp/second.PDF"),
        ]),
        2
    );

    assert_eq!(
        ingress.take_requests(),
        [
            DocumentOpenBatchRequest::new(
                DocumentOpenOrigin::System,
                [PathBuf::from("/tmp/first.pdf")],
            ),
            DocumentOpenBatchRequest::new(
                DocumentOpenOrigin::Drop,
                [
                    PathBuf::from("/tmp/first.pdf"),
                    PathBuf::from("/tmp/second.PDF"),
                ],
            ),
        ]
    );
}

#[test]
fn native_document_ingress_rejects_malformed_percent_encoding_and_nul() {
    let ingress = NativeDocumentIngress::default();
    assert_eq!(
        ingress.enqueue_file_urls([
            "file:///tmp/bad%2.pdf",
            "file:///tmp/bad%00name.pdf",
            "file:///tmp/good%2Epdf",
        ]),
        1
    );
    assert_eq!(
        ingress.take_requests(),
        [DocumentOpenBatchRequest::new(
            DocumentOpenOrigin::System,
            [PathBuf::from("/tmp/good.pdf")],
        )]
    );
}

#[gpui::test]
fn native_application_close_action_enters_the_dirty_close_transaction(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let backend = Arc::new(RecordingBackend::default());
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    let document_id = workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/tmp/dirty.pdf"), cx)
    });
    cx.run_until_parked();
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                document_id,
                0,
                MarkupId::new("native-menu:dirty").unwrap(),
                PdfPoint::new(10., 10.).unwrap(),
                PdfPoint::new(40., 40.).unwrap(),
                cx,
            )
        })
        .unwrap();
    let close = cx.new({
        let workspace = workspace.clone();
        let backend = backend.clone();
        move |_| ApplicationCloseWorkspace::new(workspace, backend)
    });
    cx.update(|cx| register_application_close_action(&close, cx));
    let close_for_window = close.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell =
            cx.new(|cx| ApplicationCloseShell::new_for_native_window(close_for_window, window, cx));
        Root::new(shell, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    cx.dispatch_action(RequestApplicationClose);

    assert!(close.read_with(cx, |close, _| close.dialog().is_some()));
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(document_id, cx).is_some()
    }));
    assert!(!backend.released.load(Ordering::Acquire));
}

#[gpui::test]
fn native_application_root_accepts_real_external_pdf_drops_and_preserves_drop_origin_policy(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let backend = Arc::new(RecordingBackend::default());
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/tmp/already-open.pdf"), cx)
    });
    cx.run_until_parked();
    let close = cx.new({
        let workspace = workspace.clone();
        let backend = backend.clone();
        move |_| ApplicationCloseWorkspace::new(workspace, backend)
    });
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let first = PathBuf::from("/tmp/drop-first.pdf");
    let second = PathBuf::from("/tmp/drop-second.PDF");
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.open_documents(
            DocumentOpenBatchRequest::new(
                DocumentOpenOrigin::Drop,
                [
                    first.clone(),
                    PathBuf::from("/tmp/notes.txt"),
                    second.clone(),
                ],
            ),
            cx,
        )),
        DocumentOpenBatchDisposition::Started {
            batch_id: 1,
            candidate_count: 2,
        }
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_open_batches()),
        1
    );
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.document_open_status().clone()),
        DocumentOpenBatchStatus::Opening {
            origin: DocumentOpenOrigin::Drop,
            candidate_count: 2,
            ..
        }
    ));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_OPEN_STATUS_ID).is_some());
    assert!(cx.debug_bounds(DOCUMENT_OPEN_PROGRESS_ID).is_some());

    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_open_batches()),
        0
    );
    assert!(cx.debug_bounds(DOCUMENT_OPEN_STATUS_ID).is_none());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .sessions()
                .iter()
                .map(|session| session.read(cx).path().to_owned())
                .collect::<Vec<_>>()
        }),
        [
            PathBuf::from("/tmp/already-open.pdf"),
            first.clone(),
            second,
        ]
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .session(workspace.active_document_id().unwrap(), cx)
                .unwrap()
                .read(cx)
                .path()
                .to_owned()
        }),
        first
    );

    for path in [
        PathBuf::from("/tmp/drop-first.pdf"),
        PathBuf::from("/tmp/drop-third.pdf"),
    ] {
        assert!(matches!(
            workspace.update(cx, |workspace, cx| workspace.open_documents(
                DocumentOpenBatchRequest::new(DocumentOpenOrigin::Drop, [path]),
                cx,
            )),
            DocumentOpenBatchDisposition::Started { .. }
        ));
    }
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_open_batches()),
        2,
        "overlapping drops must retain one busy claim per accepted request"
    );
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        5,
        "a separate drop of the same PDF must force a new document tab"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_open_batches()),
        0
    );

    cx.simulate_event(FileDropEvent::Entered {
        position: point(px(20.), px(20.)),
        paths: ExternalPaths(
            [
                PathBuf::from("/tmp/root-drop.pdf"),
                PathBuf::from("/tmp/root-notes.txt"),
            ]
            .into_iter()
            .collect(),
        ),
    });
    cx.simulate_event(FileDropEvent::Submit {
        position: point(px(20.), px(20.)),
    });
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        6,
        "one real Submit must deliver its accepted PDF exactly once"
    );

    cx.simulate_event(FileDropEvent::Entered {
        position: point(px(20.), px(20.)),
        paths: ExternalPaths([PathBuf::from("/tmp/notes-only.txt")].into_iter().collect()),
    });
    cx.simulate_event(FileDropEvent::Submit {
        position: point(px(20.), px(20.)),
    });
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        6,
        "a non-PDF-only drop must be a silent no-op"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_open_batches()),
        0
    );

    let ids = workspace.read_with(cx, |workspace, app| {
        workspace
            .sessions()
            .iter()
            .map(|session| session.read(app).id())
            .collect::<Vec<_>>()
    });
    for document_id in ids {
        assert!(workspace.update(cx, |workspace, cx| {
            workspace.close_document(document_id, cx)
        }));
    }
    assert!(backend.released.load(Ordering::Acquire));
}

#[gpui::test]
fn empty_workspace_strip_starts_with_its_actions(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        cx.set_reduce_motion(true);
    });
    let workspace =
        cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    let (_, cx) = cx.add_window_view(move |window, cx| Root::new(workspace, window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let strip = cx
        .debug_bounds(butter_paper::document_workspace::DOCUMENT_SESSION_TABS_ID)
        .unwrap();
    let track_actions = cx
        .debug_bounds(butter_paper::document_workspace::DOCUMENT_SESSION_TAB_TRACK_ACTIONS_ID)
        .unwrap();
    let open = cx.debug_bounds("document-tab-open").unwrap();
    // No tabs: no empty tab track and no separator before New Tab, which
    // sits the row's spacing from the strip's edge.
    assert_eq!(track_actions.left(), strip.left());
    assert_eq!(open.left() - strip.left(), px(8.));
    assert_eq!(open.top() - strip.top(), px(8.));
}

#[gpui::test]
fn workspace_tab_hover_truncates_and_restores_without_moving_tabs(cx: &mut TestAppContext) {
    use butter_paper::document_workspace::document_session_tab_id;
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let workspace =
        cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    let first = workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/tmp/first-document.pdf"), cx)
    });
    let second = workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/tmp/second-document.pdf"), cx)
    });
    let short = workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/tmp/a.pdf"), cx)
    });
    cx.run_until_parked();
    let (_, cx) = cx.add_window_view(move |window, cx| Root::new(workspace, window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let outside = point(px(600.), px(500.));
    let strip = cx
        .debug_bounds(butter_paper::document_workspace::DOCUMENT_SESSION_TABS_ID)
        .unwrap();
    let first_bounds = cx.debug_bounds("document-1-session-tab").unwrap();
    let second_bounds = cx.debug_bounds("document-2-session-tab").unwrap();
    let viewport = cx
        .debug_bounds("document-workspace-native-session-tab-bar")
        .unwrap();
    // Segmented tabs: the bar is the whole row. The row has one spacing,
    // the room above and below its 32px tabs: the first tab's inset and the
    // space between tabs.
    let gap = second_bounds.left() - first_bounds.right();
    assert_eq!(gap, px(8.), "tabs sit the row's spacing apart");
    assert_eq!(
        first_bounds.left() - strip.left(),
        px(8.),
        "the bar fills the strip, the first tab in its inset"
    );
    assert!(
        viewport.right() > second_bounds.right(),
        "unused tab space stays before pinned actions"
    );
    // The documents menu follows the last tab on the track; a separator,
    // then New Tab and New from template, follow it.
    let track_actions = cx
        .debug_bounds(butter_paper::document_workspace::DOCUMENT_SESSION_TAB_TRACK_ACTIONS_ID)
        .unwrap();
    let last_bounds = cx.debug_bounds("document-3-session-tab").unwrap();
    // One spacing across the row: after the last tab, on each side of the
    // separator, and between the buttons.
    let row_gap = px(8.);
    assert_eq!(track_actions.left() - last_bounds.right(), row_gap);
    let open = cx.debug_bounds("document-tab-open").unwrap();
    let new_pdf = cx.debug_bounds("document-tab-new-pdf").unwrap();
    let picker = cx.debug_bounds("document-tab-template-picker").unwrap();
    let menu = cx.debug_bounds("document-tabs-overflow").unwrap();
    // The documents menu starts the actions, with the tabs it lists. The
    // separator line after it takes no width of its own: New Tab follows the
    // menu by two spacings, one on each side of the line.
    assert_eq!(menu.left(), track_actions.left());
    assert_eq!(open.left() - menu.right(), row_gap * 2.);
    assert_eq!(new_pdf.left() - open.right(), row_gap);
    assert_eq!(
        picker.left(),
        new_pdf.right(),
        "the split control is one piece"
    );
    assert_eq!(track_actions.right() - picker.right(), row_gap);
    // The bar runs to the window's edge.
    assert_eq!(viewport.right(), strip.right());
    cx.simulate_mouse_move(outside, None, gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    for appearance in [gpui::WindowAppearance::Light, gpui::WindowAppearance::Dark] {
        cx.update(|window, cx| gpui_component::Theme::change(appearance, Some(window), cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        for id in [first, second] {
            // The selector API requires static strings; these eight bounded
            // fixture selectors are retained only for the test process.
            let tab_id: &'static str = Box::leak(document_session_tab_id(id).into_boxed_str());
            let label_id: &'static str =
                Box::leak(format!("{tab_id}-visible-label").into_boxed_str());
            let tab = cx.debug_bounds(tab_id).expect("tab rendered");
            let close_id: &'static str = Box::leak(
                butter_paper::document_workspace::document_session_close_id(id).into_boxed_str(),
            );
            let close = cx.debug_bounds(close_id).expect("close rendered");
            // The tab's raised area is 32px high, as the row's buttons; the
            // close button sits in a square lane at its end, equally clear
            // of it at the top, bottom and trailing edge.
            let raised = px(32.);
            assert_eq!(
                close.size.width, close.size.height,
                "close needs a square frame"
            );
            assert_eq!(close.size.width, px(20.), "stock extra-small close target");
            assert_eq!(
                tab.right() - close.right(),
                (raised - close.size.height) / 2.,
                "close has equal top/bottom/trailing inset"
            );
            assert!(
                (close.center().y - tab.center().y).abs() <= px(0.5),
                "close must share the tab's vertical centre"
            );
            assert!(
                (close.center().x - (tab.right() - raised / 2.)).abs() <= px(0.5),
                "close must sit centred in the raised area's end: tab={tab:?}, close={close:?}"
            );
            let label = cx.debug_bounds(label_id).expect("label rendered");
            cx.simulate_mouse_move(tab.center(), None, gpui::Modifiers::default());
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(
                cx.debug_bounds(tab_id).unwrap(),
                tab,
                "hover must not resize or move tab"
            );
            assert!(
                cx.debug_bounds(label_id).unwrap().size.width < label.size.width,
                "hover must truncate visible label"
            );
            cx.simulate_mouse_move(outside, None, gpui::Modifiers::default());
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(
                cx.debug_bounds(label_id).unwrap(),
                label,
                "exit must restore full label"
            );
        }
        // A short title must keep its leading anchor when the close button
        // appears: the label truncates in place rather than re-centring.
        let tab_id: &'static str = Box::leak(document_session_tab_id(short).into_boxed_str());
        let label_id: &'static str = Box::leak(format!("{tab_id}-visible-label").into_boxed_str());
        let rest = cx.debug_bounds(label_id).unwrap();
        let short_centre = cx.debug_bounds(tab_id).unwrap().center();
        cx.simulate_mouse_move(short_centre, None, gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let revealed = cx.debug_bounds(label_id).unwrap();
        assert_eq!(
            revealed.left(),
            rest.left(),
            "short label must not shift on reveal"
        );
        assert!(
            revealed.right() < rest.right(),
            "revealed label must stop before the close lane"
        );
        cx.simulate_mouse_move(outside, None, gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

#[gpui::test]
fn document_tabs_sit_on_a_segmented_track_that_fills_the_strip_and_scrolls_when_full(
    cx: &mut TestAppContext,
) {
    use butter_paper::document_workspace::{
        DOCUMENT_SESSION_TAB_TRACK_ID, document_session_close_id, document_session_tab_id,
    };
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let workspace =
        cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    let ids = (0..2)
        .map(|ix| {
            workspace.update(cx, |workspace, cx| {
                workspace.open_path(PathBuf::from(format!("/tmp/track-sheet-{ix}.pdf")), cx)
            })
        })
        .collect::<Vec<_>>();
    cx.run_until_parked();
    let observed = workspace.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| Root::new(workspace, window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let viewport = cx
        .debug_bounds("document-workspace-native-session-tab-bar")
        .unwrap();
    let track = cx.debug_bounds(DOCUMENT_SESSION_TAB_TRACK_ID).unwrap();
    let tab_bounds = |cx: &mut gpui::VisualTestContext, id| {
        let selector: &'static str = Box::leak(document_session_tab_id(id).into_boxed_str());
        cx.debug_bounds(selector).unwrap()
    };
    let first = tab_bounds(cx, ids[0]);
    let last = tab_bounds(cx, ids[1]);
    // Inset by the row's spacing. A few tabs still take the whole strip:
    // the track is the whole row, edge to edge and top to bottom, square.
    assert_eq!(first.left() - track.left(), px(8.));
    assert!(
        track.right() - last.right() > px(100.),
        "the track continues past a few tabs: track={track:?}, last={last:?}"
    );
    assert_eq!(track.left(), viewport.left());
    assert_eq!(track.right(), viewport.right(), "the track fills the strip");
    assert_eq!(track.size.height, viewport.size.height);
    // Closing stays a separate target in the tab.
    let close: &'static str = Box::leak(document_session_close_id(ids[1]).into_boxed_str());
    cx.simulate_mouse_move(last.center(), None, gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let close = cx.debug_bounds(close).unwrap();
    assert!(close.left() > last.center().x && close.right() <= last.right());

    // Many tabs: the track takes the strip and scrolls inside it.
    for ix in 2..14 {
        observed.update(cx, |workspace, cx| {
            workspace.open_path(PathBuf::from(format!("/tmp/track-sheet-{ix}.pdf")), cx)
        });
    }
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let viewport = cx
        .debug_bounds("document-workspace-native-session-tab-bar")
        .unwrap();
    let track = cx.debug_bounds(DOCUMENT_SESSION_TAB_TRACK_ID).unwrap();
    assert!(
        track.left() >= viewport.left() && track.right() <= viewport.right(),
        "the full track stays inside the strip: track={track:?}, strip={viewport:?}"
    );
    // The documents menu, New Tab and New from template stay at the track's
    // right end.
    let track_actions = cx
        .debug_bounds(butter_paper::document_workspace::DOCUMENT_SESSION_TAB_TRACK_ACTIONS_ID)
        .unwrap();
    assert_eq!(track_actions.right(), track.right());
    assert!(cx.debug_bounds("document-tabs-overflow").is_some());

    // A double-click on the track's buttons is theirs; one on the empty
    // track opens a New Tab.
    let double_click = |cx: &mut gpui::VisualTestContext, position| {
        for click_count in [1, 2] {
            cx.simulate_event(gpui::MouseDownEvent {
                position,
                button: gpui::MouseButton::Left,
                click_count,
                ..Default::default()
            });
            cx.simulate_event(gpui::MouseUpEvent {
                position,
                button: gpui::MouseButton::Left,
                click_count,
                ..Default::default()
            });
        }
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };
    for _ in 2..14 {
        let active = observed
            .read_with(cx, |workspace, _| workspace.active_document_id())
            .unwrap();
        observed.update(cx, |workspace, cx| {
            workspace.close_document(active, cx);
        });
    }
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let track = cx.debug_bounds(DOCUMENT_SESSION_TAB_TRACK_ID).unwrap();
    double_click(cx, point(track.right() - px(40.), track.center().y));
    assert!(observed.read_with(cx, |workspace, _| workspace.is_new_tab_open()));
    observed.update(cx, |workspace, cx| workspace.close_new_tab(cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    // The gap between the documents menu and New Tab belongs to them too.
    let open = cx.debug_bounds("document-tab-open").unwrap();
    let menu = cx.debug_bounds("document-tabs-overflow").unwrap();
    assert!(open.left() > menu.right());
    double_click(
        cx,
        point((menu.right() + open.left()) / 2., menu.center().y),
    );
    assert!(!observed.read_with(cx, |workspace, _| workspace.is_new_tab_open()));
}

#[gpui::test]
fn empty_workspace_retains_tab_bar_boundaries_and_pinned_actions(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let workspace =
        cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    let (_, cx) = cx.add_window_view(move |window, cx| Root::new(workspace, window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let strip = cx
        .debug_bounds(butter_paper::document_workspace::DOCUMENT_SESSION_TABS_ID)
        .unwrap();
    let divider = cx
        .debug_bounds("document-workspace-native-session-tab-bar")
        .unwrap();
    let open = cx.debug_bounds("document-tab-open").unwrap();
    let picker = cx.debug_bounds("document-tab-template-picker").unwrap();
    assert_eq!(
        strip.size.height,
        px(48.),
        "empty row is a bar: 8 + 32 + 8, its line drawn inside"
    );
    assert_eq!(divider.top(), strip.top());
    assert_eq!(
        divider.bottom(),
        strip.bottom(),
        "the line takes no layout space"
    );
    // New Tab sits on the empty track, New from template beside it; the
    // track is the whole row.
    let track = cx
        .debug_bounds(butter_paper::document_workspace::DOCUMENT_SESSION_TAB_TRACK_ID)
        .unwrap();
    assert!(open.left() > track.left() && open.right() < track.center().x);
    assert!(picker.left() > open.right() && picker.right() < track.center().x);
    assert_eq!(track.left(), strip.left());
    assert_eq!(track.right(), strip.right());
    assert!(
        cx.debug_bounds("document-tabs-overflow").is_none(),
        "no documents to list"
    );
}

#[gpui::test]
fn workspace_tabs_reorder_with_one_pointer_move_and_cancel_outside(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let workspace =
        cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    let ids = (0..3)
        .map(|ix| {
            workspace.update(cx, |workspace, cx| {
                workspace.open_path(PathBuf::from(format!("/tmp/drag-sheet-{ix}.pdf")), cx)
            })
        })
        .collect::<Vec<_>>();
    cx.run_until_parked();
    let observed = workspace.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| Root::new(workspace, window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let active = observed.read_with(cx, |workspace, _| workspace.active_document_id());
    let source = cx.debug_bounds("document-1-session-tab").unwrap().center();
    let target = cx.debug_bounds("document-3-session-tab").unwrap().center();
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_move(
        target,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        observed.read_with(cx, |workspace, cx| workspace.session_order(cx)),
        vec![ids[1], ids[2], ids[0]]
    );
    assert_eq!(
        observed.read_with(cx, |workspace, _| workspace.active_document_id()),
        active,
        "reordering must preserve active document identity"
    );
    let source = cx.debug_bounds("document-1-session-tab").unwrap().center();
    let target = cx.debug_bounds("document-2-session-tab").unwrap().center();
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_move(
        target,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let outside = point(target.x, target.y + px(150.));
    cx.simulate_mouse_up(outside, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        observed.read_with(cx, |workspace, cx| workspace.session_order(cx)),
        vec![ids[1], ids[2], ids[0]],
        "release outside must cancel, even without a final move event"
    );
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_move(
        target,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        observed.read_with(cx, |workspace, cx| workspace.session_order(cx)),
        ids,
        "reverse drag restores original order"
    );
}

#[gpui::test]
fn workspace_integration_crowded_tabs_keep_actions_pinned_for_both_wheel_axes(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let workspace =
        cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    for ix in 0..12 {
        workspace.update(cx, |workspace, cx| {
            workspace.open_path(
                PathBuf::from(format!("/tmp/integration-long-drawing-sheet-{ix}.pdf")),
                cx,
            )
        });
    }
    cx.run_until_parked();
    let (_, cx) = cx.add_window_view(move |window, cx| Root::new(workspace, window, cx));
    cx.simulate_resize(gpui::size(px(900.), px(800.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let ids = [
        "document-tab-open",
        "document-tab-new-pdf",
        "document-tab-template-picker",
        "document-tabs-overflow",
    ];
    let before = ids.map(|id| cx.debug_bounds(id).unwrap());
    assert!(before[2].right() <= px(900.));
    let tabs = cx
        .debug_bounds(butter_paper::document_workspace::DOCUMENT_SESSION_TABS_ID)
        .unwrap();
    let cut = cx
        .debug_bounds("document-workspace-native-session-tab-bar")
        .unwrap();
    assert!(cx.debug_bounds("document-tabs-actions-separator").is_none());
    let overflow = before[3];
    assert_eq!(
        overflow.size, before[0].size,
        "the documents menu uses the same stock button size as New Tab"
    );
    assert_eq!(overflow.top(), before[0].top());
    // Crowded tabs: the documents menu, New Tab and New from template stay
    // at the track's right end, which is the window's. The menu comes
    // first, beside the tabs it lists.
    let track = cx
        .debug_bounds(butter_paper::document_workspace::DOCUMENT_SESSION_TAB_TRACK_ID)
        .unwrap();
    assert!(before[2].right() < track.right() && track.right() - before[2].right() < px(12.));
    assert!(overflow.right() < before[0].left() && before[0].right() < before[1].left());

    assert_eq!(track.right(), cut.right());
    assert_eq!(cut.top(), tabs.top(), "the tab bar starts at the strip top");
    assert_eq!(
        cut.bottom(),
        tabs.bottom(),
        "the tab bar fills the strip; its line is drawn inside"
    );
    assert!(overflow.right() < cut.right());
    let target = point(tabs.left() + px(60.), tabs.center().y);
    let mut positions = Vec::new();
    for delta in [
        point(px(0.), px(2000.)),
        point(px(0.), px(-160.)),
        point(px(160.), px(0.)),
        point(px(-160.), px(0.)),
    ] {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: target,
            delta: gpui::ScrollDelta::Pixels(delta),
            modifiers: gpui::Modifiers::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        positions.push(cx.debug_bounds("document-1-session-tab").unwrap().left());
        for (id, expected) in ids.into_iter().zip(before) {
            assert_eq!(
                cx.debug_bounds(id).unwrap(),
                expected,
                "wheel scrolling moved pinned {id}"
            );
        }
    }
    assert!(
        positions[1] < positions[0],
        "vertical wheel must actually scroll tabs"
    );
    assert!(
        positions[2] > positions[1],
        "horizontal wheel must scroll back"
    );
    assert!(
        positions[3] < positions[2],
        "opposite horizontal wheel must scroll forward"
    );
}

/// A click on a tab focuses it for the arrow keys but shows no focus ring,
/// as a click on a stock button shows none; keyboard focus shows the ring,
/// inside the tab.
#[gpui::test]
fn a_tab_shows_its_focus_ring_for_keyboard_focus_only(cx: &mut TestAppContext) {
    use butter_paper::document_workspace::{
        SESSION_TAB_FOCUS_RING_ID, init_document_workspace_actions,
    };
    cx.update(|cx| {
        gpui_component::init(cx);
        cx.set_reduce_motion(true);
    });
    cx.update(init_document_workspace_actions);
    let workspace =
        cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    for name in ["first", "second", "third"] {
        workspace.update(cx, |workspace, cx| {
            workspace.open_path(PathBuf::from(format!("/tmp/{name}-document.pdf")), cx)
        });
    }
    cx.run_until_parked();
    let (_, cx) = cx.add_window_view(move |window, cx| Root::new(workspace, window, cx));
    cx.update(|window, cx| {
        window.activate_window();
        window.draw(cx).clear(cx);
    });
    let second = cx.debug_bounds("document-2-session-tab").unwrap();
    cx.simulate_click(second.center(), gpui::Modifiers::default());
    cx.simulate_mouse_move(point(px(600.), px(500.)), None, gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(SESSION_TAB_FOCUS_RING_ID).is_none(),
        "a click leaves no ring"
    );

    for key in ["right", "left"] {
        let keystroke = gpui::Keystroke::parse(key).unwrap();
        cx.simulate_event(gpui::KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(gpui::KeyUpEvent { keystroke });
        cx.run_until_parked();
    }
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let ring = cx
        .debug_bounds(SESSION_TAB_FOCUS_RING_ID)
        .expect("keyboard focus shows the ring");
    let second = cx.debug_bounds("document-2-session-tab").unwrap();
    assert!(
        ring.left() >= second.left()
            && ring.right() <= second.right()
            && ring.top() >= second.top()
            && ring.bottom() <= second.bottom(),
        "the ring lies inside the focused tab: {ring:?} in {second:?}"
    );
    assert_eq!(ring.size.height, px(32.), "around the tab's raised area");
}

#[gpui::test]
fn the_tab_strip_fades_only_the_edges_that_hide_tabs(cx: &mut TestAppContext) {
    use butter_paper::document_workspace::{
        DOCUMENT_SESSION_TAB_FADE_LEFT_ID as LEFT, DOCUMENT_SESSION_TAB_FADE_RIGHT_ID as RIGHT,
    };
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let workspace =
        cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    let observed = workspace.clone();
    for ix in 0..12 {
        workspace.update(cx, |workspace, cx| {
            workspace.open_path(PathBuf::from(format!("/tmp/fade-sheet-{ix}.pdf")), cx)
        });
    }
    cx.run_until_parked();
    let (_, cx) = cx.add_window_view(move |window, cx| Root::new(workspace, window, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..3 {
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }
    };
    settle(cx);
    let fades = |cx: &mut gpui::VisualTestContext| {
        (
            cx.debug_bounds(LEFT).is_some(),
            cx.debug_bounds(RIGHT).is_some(),
        )
    };
    // The first tab is active, so the strip shows its start: only the right
    // edge hides tabs.
    assert_eq!(
        fades(cx),
        (false, true),
        "at the start, only the right fades"
    );
    let menu = cx.debug_bounds("document-tabs-overflow").unwrap();
    let right = cx.debug_bounds(RIGHT).unwrap();
    assert_eq!(right.right(), menu.left(), "the fade ends at the menu");
    assert_eq!(
        right.size.width,
        px(24.),
        "two spacings of gradient, one solid"
    );
    let tabs = cx
        .debug_bounds(butter_paper::document_workspace::DOCUMENT_SESSION_TABS_ID)
        .unwrap();
    let wheel = |cx: &mut gpui::VisualTestContext, dx: f32| {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: point(tabs.left() + px(60.), tabs.center().y),
            delta: gpui::ScrollDelta::Pixels(point(px(dx), px(0.))),
            modifiers: gpui::Modifiers::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        settle(cx);
    };
    wheel(cx, -300.);
    assert_eq!(fades(cx), (true, true), "in the middle, both edges fade");
    let left = cx.debug_bounds(LEFT).unwrap();
    let track = cx
        .debug_bounds(butter_paper::document_workspace::DOCUMENT_SESSION_TAB_TRACK_ID)
        .unwrap();
    assert_eq!(
        left.left(),
        track.left(),
        "the left fade starts at the edge"
    );
    // The fade takes no pointer input: a click under it, on a tab and clear
    // of its close button, reaches that tab.
    let (tab_id, under) = (1..=12u64)
        .filter_map(|n| {
            let id: &'static str = Box::leak(format!("document-{n}-session-tab").into_boxed_str());
            let tab = cx.debug_bounds(id)?;
            let left = tab.left().max(right.left());
            let right_limit = (tab.right() - px(36.)).min(right.right());
            (left < right_limit).then(|| (n, point(left + px(1.), right.center().y)))
        })
        .next()
        .expect("a tab's label lies under the fade");
    cx.simulate_click(under, gpui::Modifiers::default());
    settle(cx);
    assert_eq!(
        observed.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(butter_paper::document_workspace::DocumentId::new(tab_id)),
        "the tab under the fade takes the click"
    );
    wheel(cx, -100_000.);
    assert_eq!(fades(cx), (true, false), "at the end, only the left fades");
    wheel(cx, 100_000.);
    assert_eq!(fades(cx), (false, true));

    // With room for every tab, nothing fades.
    for _ in 0..10 {
        let active = observed
            .read_with(cx, |workspace, _| workspace.active_document_id())
            .unwrap();
        observed.update(cx, |workspace, cx| workspace.close_document(active, cx));
    }
    settle(cx);
    assert_eq!(fades(cx), (false, false), "all tabs fit: no fade");
}

#[gpui::test]
fn select_default_template_stays_inside_the_narrowest_window(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let workspace =
        cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    for ix in 0..12 {
        workspace.update(cx, |workspace, cx| {
            workspace.open_path(PathBuf::from(format!("/tmp/narrow-sheet-{ix}.pdf")), cx)
        });
    }
    cx.run_until_parked();
    let (_, cx) = cx.add_window_view(move |window, cx| Root::new(workspace, window, cx));
    let width = butter_paper::window_title_bar::DOCUMENT_WINDOW_MIN_SIZE.width;
    cx.simulate_resize(gpui::size(width, px(800.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let picker = cx.debug_bounds("document-tab-template-picker").unwrap();
    cx.simulate_click(picker.center(), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    // The panel's content, and its 12px popover padding.
    let panel = cx.debug_bounds("template-picker").unwrap().dilate(px(12.));
    let close = cx.debug_bounds("template-picker-close").unwrap();
    let preview = cx.debug_bounds("template-picker-preview").unwrap();
    for (name, bounds) in [("panel", panel), ("close", close), ("preview", preview)] {
        assert!(
            bounds.left() >= px(0.) && bounds.right() <= width,
            "the {name} stays inside the {width:?} window: {bounds:?}"
        );
    }
    assert!(close.right() <= panel.right(), "the X stays in the panel");
}

#[gpui::test]
fn workspace_integration_sidebar_matrix_preserves_canvas_boundaries(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let backend = Arc::new(RecordingBackend::default());
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/tmp/integration-fixture.pdf"), cx)
    });
    cx.run_until_parked();
    let observed = workspace.clone();
    let close = cx.new(move |_| ApplicationCloseWorkspace::new(workspace, backend));
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    let id = observed.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    let revision = observed.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(id, cx).unwrap().revision
    });
    for width in [1200., 900., 1500.] {
        cx.simulate_resize(gpui::size(px(width), px(800.)));
        // Closed/closed -> left -> both -> right -> closed/closed.
        for toggle in [
            None,
            Some("document-left-rail-pages"),
            Some("document-workspace-rail-actions"),
            Some("document-left-rail-pages"),
            Some("document-workspace-rail-actions"),
        ] {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            if let Some(toggle) = toggle {
                let target = cx.debug_bounds(toggle).unwrap().center();
                cx.simulate_click(target, gpui::Modifiers::default());
            }
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let left = cx.debug_bounds("document-left-rail").unwrap();
            let right = cx.debug_bounds("document-workspace-right-rail").unwrap();
            let toolbar = cx.debug_bounds("viewer-toolbar").unwrap();
            let canvas = cx
                .debug_bounds(butter_paper::document_workspace::DOCUMENT_PAGE_ID)
                .unwrap();
            let leading = cx
                .debug_bounds("document-left-sidebar-header")
                .map_or(left.right(), |b| b.right());
            let trailing = cx
                .debug_bounds("document-workspace-properties-sidebar")
                .map_or(right.left(), |b| b.left());
            assert_eq!(
                toolbar.left(),
                leading,
                "toolbar leading boundary at width {width}"
            );
            assert_eq!(
                toolbar.right(),
                trailing,
                "toolbar trailing boundary at width {width}"
            );
            assert_eq!(toolbar.top(), left.top());
            assert_eq!(toolbar.top(), right.top());
            assert_eq!(canvas.left(), toolbar.left());
            assert_eq!(canvas.right(), toolbar.right());
            assert_eq!(canvas.top(), toolbar.bottom());
            assert_eq!(right.right(), px(width));
            let first = cx.debug_bounds("viewer-zoom-controls").unwrap();
            let last = cx
                .debug_bounds(butter_paper::viewer_toolbar_strip::VIEWER_PAGE_LAYOUT_ID)
                .unwrap();
            let content = cx.debug_bounds(VIEWER_TOOLBAR_CONTENT_ID).unwrap();
            // Undo and Redo, then the pages, zoom with the page layout, then
            // Edit PDF and the Scale. With room, the centre controls sit as
            // far from Undo and Redo as from Edit PDF. The toolbar never
            // scrolls: side panels give way before it gets too narrow.
            let edit_pdf = cx.debug_bounds("viewer-edit-pdf").unwrap();
            let layout = cx
                .debug_bounds(butter_paper::viewer_toolbar_strip::VIEWER_PAGE_LAYOUT_ID)
                .unwrap();
            assert!(
                layout.right() <= edit_pdf.left() && last.right() <= edit_pdf.left(),
                "Edit PDF overlaps at width {width}: toolbar={toolbar:?}, edit={edit_pdf:?}, layout={layout:?}"
            );
            if content.size.width <= toolbar.size.width {
                assert!(
                    edit_pdf.right() <= toolbar.right(),
                    "Edit PDF outside the toolbar at width {width}: toolbar={toolbar:?}, edit={edit_pdf:?}"
                );
                if toolbar.size.width
                    >= px(16.
                        * butter_paper::viewer_toolbar_strip::viewer_toolbar_rems(
                            butter_paper::viewer_toolbar_strip::ViewerToolbarFit::Full,
                            false,
                            4,
                        ))
                {
                    let history = cx
                        .debug_bounds(butter_paper::viewer_toolbar_strip::VIEWER_HISTORY_GROUP_ID)
                        .unwrap();
                    let zoom = cx
                        .debug_bounds(butter_paper::viewer_toolbar_strip::VIEWER_ZOOM_GROUP_ID)
                        .unwrap();
                    // Centred in the toolbar, unless a wider side would
                    // reach the controls there: then they meet that side.
                    let controls = cx
                        .debug_bounds(
                            butter_paper::viewer_toolbar_strip::VIEWER_TOOLBAR_CONTROLS_ID,
                        )
                        .unwrap();
                    let half = controls.size.width / 2.;
                    let centre = toolbar.center().x;
                    let room = centre - half >= history.right() && centre + half <= edit_pdf.left();
                    if room {
                        assert!(
                            (controls.center().x - centre).abs() <= px(1.),
                            "the centre controls not centred at width {width}: {controls:?} in {toolbar:?}, first={first:?}, zoom={zoom:?}"
                        );
                    } else {
                        assert!(
                            (controls.left() - history.right()).abs() <= px(1.)
                                || (edit_pdf.left() - controls.right()).abs() <= px(1.),
                            "the centre controls not against the wider side at width {width}: {controls:?}"
                        );
                    }
                }
            } else {
                panic!(
                    "the toolbar overflows at width {width}: toolbar={toolbar:?}, content={content:?}"
                );
            }
            assert_eq!(
                observed.read_with(cx, |workspace, _| workspace.active_document_id()),
                Some(id)
            );
            assert_eq!(
                observed.read_with(cx, |workspace, cx| workspace
                    .annotation_snapshot(id, cx)
                    .unwrap()
                    .revision),
                revision
            );
        }
    }
}

#[gpui::test]
fn workspace_sidebar_dividers_resize_independently_and_double_click_restores_defaults(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let backend = Arc::new(RecordingBackend::default());
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/tmp/sidebar-reset.pdf"), cx)
    });
    cx.run_until_parked();
    let observed = workspace.clone();
    let close = cx.new(move |_| ApplicationCloseWorkspace::new(workspace, backend));
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    cx.simulate_resize(gpui::size(px(2100.), px(800.)));
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..5 {
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }
    };
    settle(cx);
    let id = observed.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    let revision = observed.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(id, cx).unwrap().revision
    });
    for toggle in [
        "document-left-rail-pages",
        "document-workspace-rail-actions",
    ] {
        let position = cx.debug_bounds(toggle).unwrap().center();
        cx.simulate_click(position, gpui::Modifiers::default());
        settle(cx);
    }
    let left_id = "document-left-sidebar";
    let right_id = "document-workspace-properties-sidebar";
    assert_eq!(cx.debug_bounds(left_id).unwrap().size.width, px(300.));
    assert_eq!(cx.debug_bounds(right_id).unwrap().size.width, px(300.));
    for (selector, other, width) in [(left_id, right_id, 360.), (right_id, left_id, 400.)] {
        let panel = cx.debug_bounds(selector).unwrap();
        let other_width = cx.debug_bounds(other).unwrap().size.width;
        let left = selector == left_id;
        let start = point(
            if left { panel.right() } else { panel.left() },
            panel.top() + px(100.),
        );
        let end = point(
            if left {
                panel.left() + px(width)
            } else {
                panel.right() - px(width)
            },
            start.y,
        );
        cx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_move(
            start + point(px(if left { 8. } else { -8. }), px(0.)),
            Some(gpui::MouseButton::Left),
            gpui::Modifiers::default(),
        );
        settle(cx);
        cx.simulate_mouse_move(
            end,
            Some(gpui::MouseButton::Left),
            gpui::Modifiers::default(),
        );
        settle(cx);
        cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
        settle(cx);
        assert_eq!(
            cx.debug_bounds(selector).unwrap().size.width,
            px(width),
            "drag {selector}"
        );
        assert_eq!(
            cx.debug_bounds(other).unwrap().size.width,
            other_width,
            "drag must not change {other}"
        );
    }
    // Window resizing and hiding/reopening a neighbour must preserve preferences.
    for width in [1800., 2300., 2100.] {
        cx.simulate_resize(gpui::size(px(width), px(800.)));
        settle(cx);
        assert_eq!(cx.debug_bounds(left_id).unwrap().size.width, px(360.));
        assert_eq!(cx.debug_bounds(right_id).unwrap().size.width, px(400.));
    }
    // The exact fit boundary has no extra rounding margin or oscillation:
    // both rails, both sidebars and the least page between them.
    let fit = 48.
        + 88.
        + 360.
        + 400.
        + 16. * butter_paper::document_workspace::PAGE_BESIDE_SIDEBARS_MIN_REMS;
    for (width, visible) in [
        (fit - 1., false),
        (fit, true),
        (fit - 1., false),
        (1800., true),
    ] {
        cx.simulate_resize(gpui::size(px(width), px(800.)));
        settle(cx);
        assert_eq!(
            cx.debug_bounds(left_id).is_some(),
            visible,
            "thumbnail fit threshold {width}"
        );
    }
    // A temporarily hidden sidebar returns at its custom width, not the default.
    cx.simulate_resize(gpui::size(px(900.), px(800.)));
    settle(cx);
    assert!(cx.debug_bounds(left_id).is_none());
    let toggle = cx
        .debug_bounds("document-left-rail-pages")
        .unwrap()
        .center();
    cx.simulate_click(toggle, gpui::Modifiers::default()); // disabled while it cannot fit
    cx.simulate_resize(gpui::size(px(2100.), px(800.)));
    settle(cx);
    assert_eq!(cx.debug_bounds(left_id).unwrap().size.width, px(360.));
    assert_eq!(cx.debug_bounds(right_id).unwrap().size.width, px(400.));
    for toggle in [
        "document-left-rail-pages",
        "document-left-rail-pages",
        "document-workspace-rail-actions",
        "document-workspace-rail-actions",
    ] {
        let position = cx.debug_bounds(toggle).unwrap().center();
        cx.simulate_click(position, gpui::Modifiers::default());
        settle(cx);
    }
    assert_eq!(cx.debug_bounds(left_id).unwrap().size.width, px(360.));
    assert_eq!(cx.debug_bounds(right_id).unwrap().size.width, px(400.));
    // Double-clicking ordinary sidebar content must not reset its width.
    let header = cx
        .debug_bounds("document-left-sidebar-header")
        .unwrap()
        .center();
    cx.simulate_event(gpui::MouseDownEvent {
        position: header,
        button: gpui::MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    cx.simulate_event(gpui::MouseUpEvent {
        position: header,
        button: gpui::MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    settle(cx);
    assert_eq!(cx.debug_bounds(left_id).unwrap().size.width, px(360.));
    for (selector, other) in [(left_id, right_id), (right_id, left_id)] {
        let panel = cx.debug_bounds(selector).unwrap();
        let other_width = cx.debug_bounds(other).unwrap().size.width;
        let position = point(
            if selector == left_id {
                panel.right()
            } else {
                panel.left()
            },
            panel.top() + px(100.),
        );
        cx.simulate_event(gpui::MouseDownEvent {
            position,
            button: gpui::MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position,
            button: gpui::MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        settle(cx);
        assert_eq!(
            cx.debug_bounds(selector).unwrap().size.width,
            px(300.),
            "reset {selector}"
        );
        assert_eq!(
            cx.debug_bounds(other).unwrap().size.width,
            other_width,
            "reset must not change {other}"
        );
    }
    // Wide enough for both sidebars and the toolbar at the largest
    // interface size.
    cx.simulate_resize(gpui::size(px(2800.), px(1000.)));
    for level in [-3, 3, 0] {
        cx.update(|window, cx| {
            butter_paper::application_shell::apply_application_ui_zoom(level, px(16.), window, cx);
            observed.update(cx, |_, cx| cx.notify());
        });
        settle(cx);
        let rem = butter_paper::application_shell::application_ui_zoom_font_size(px(16.), level);
        for selector in [left_id, right_id] {
            assert!(
                (cx.debug_bounds(selector).unwrap().size.width - rem * 18.75).abs() < px(1.),
                "default follows interface scale {level}: {selector}"
            );
        }
        let select = cx.debug_bounds("document-workspace-select-tool").unwrap();
        let hand = cx.debug_bounds("document-workspace-pan-tool").unwrap();
        assert_eq!(
            select.top(),
            hand.top(),
            "two-column rail must not wrap at interface zoom {level}"
        );
        let toolbar = cx.debug_bounds("viewer-toolbar").unwrap();
        assert_eq!(toolbar.left(), cx.debug_bounds(left_id).unwrap().right());
        assert_eq!(toolbar.right(), cx.debug_bounds(right_id).unwrap().left());
        if level == 3 {
            // Just too narrow at this interface size for the least page
            // between thumbnails and properties (3 + 15 + 18.75 + 18.75 rem,
            // and the 5.5 rem tool rail).
            let narrow = rem * 60.5;
            cx.simulate_resize(gpui::size(narrow, px(1000.)));
            settle(cx);
            assert!(
                cx.debug_bounds(left_id).is_none(),
                "narrow/high-zoom fallback collapses thumbnails"
            );
            let toolbar = cx.debug_bounds("viewer-toolbar").unwrap();
            assert_eq!(
                toolbar.left(),
                cx.debug_bounds("document-left-rail").unwrap().right()
            );
            assert_eq!(toolbar.right(), cx.debug_bounds(right_id).unwrap().left());
            assert!(toolbar.size.width >= px(100.));
            assert!(
                cx.debug_bounds("document-workspace-right-rail")
                    .unwrap()
                    .right()
                    <= narrow
            );
            let properties_toggle = cx
                .debug_bounds("document-workspace-rail-actions")
                .unwrap()
                .center();
            cx.simulate_click(properties_toggle, gpui::Modifiers::default());
            settle(cx);
            assert!(
                cx.debug_bounds(right_id).is_none(),
                "properties toggle closes the panel at high zoom"
            );
            assert!(
                cx.debug_bounds(left_id).is_some(),
                "closing properties makes room for thumbnails"
            );
            cx.simulate_click(properties_toggle, gpui::Modifiers::default());
            settle(cx);
            assert!(cx.debug_bounds(left_id).is_none());
            cx.simulate_resize(gpui::size(px(2800.), px(1000.)));
            settle(cx);
            assert!((cx.debug_bounds(left_id).unwrap().size.width - rem * 18.75).abs() < px(1.));
        }
    }
    // Explicitly closing thumbnails overrides automatic restoration.
    let toggle = cx
        .debug_bounds("document-left-rail-pages")
        .unwrap()
        .center();
    cx.simulate_click(toggle, gpui::Modifiers::default());
    for width in [900., 1800.] {
        cx.simulate_resize(gpui::size(px(width), px(1000.)));
        settle(cx);
        assert!(cx.debug_bounds(left_id).is_none());
    }
    assert_eq!(
        observed.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(id)
    );
    assert_eq!(
        observed.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(id, cx)
            .unwrap()
            .revision),
        revision
    );
}

#[gpui::test]
fn workspace_left_rail_toggles_existing_panel_without_moving_rail(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let backend = Arc::new(RecordingBackend::default());
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/documents/rail-fixture.pdf"), cx)
    });
    cx.run_until_parked();
    let observed = workspace.clone();
    let close = cx.new(move |_| ApplicationCloseWorkspace::new(workspace, backend));
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let rail = cx
        .debug_bounds("document-left-rail")
        .expect("rail rendered");
    let toggle = cx
        .debug_bounds("document-left-rail-pages")
        .expect("toggle rendered");
    assert_eq!(rail.size.width, px(48.));
    assert_eq!(toggle.size.width, px(32.));
    assert_eq!(toggle.size.height, px(32.));
    assert!(toggle.origin.y >= rail.origin.y + px(8.));
    assert!(cx.debug_bounds("document-thumbnail-strip").is_none());
    cx.simulate_click(toggle.center(), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("document-thumbnail-strip").is_some());
    let header = cx
        .debug_bounds("document-left-sidebar-header")
        .expect("sidebar header");
    let toolbar = cx.debug_bounds("viewer-toolbar").expect("viewer toolbar");
    assert_eq!(
        header.origin.y, rail.origin.y,
        "sidebar starts directly below tabs"
    );
    assert_eq!(
        header.origin.y, toolbar.origin.y,
        "sidebar header sits beside viewer controls"
    );
    assert_eq!(
        header.size.height, toolbar.size.height,
        "header and viewer band share a height"
    );
    assert!(
        toolbar.origin.x >= header.origin.x + header.size.width,
        "viewer is in the canvas column"
    );
    let row = cx
        .debug_bounds("document-1-thumbnail-0")
        .expect("thumbnail row");
    let preview = cx
        .debug_bounds("thumbnail-preview-0")
        .expect("thumbnail preview");
    assert!(
        preview.origin.y + preview.size.height <= row.origin.y + row.size.height,
        "preview must stay inside its card"
    );
    assert!(row.origin.x > header.origin.x, "cards have a leading inset");
    assert!(
        cx.debug_bounds("thumbnail-0-scale").is_none(),
        "page scale is set from the Measure tools, not the thumbnails"
    );
    let rotation = observed.update(cx, |workspace, cx| {
        workspace
            .begin_page_rotation(
                workspace.active_document_id().unwrap(),
                0,
                butter_paper::annotation_model::PageRotationDirection::Right,
                cx,
            )
            .unwrap()
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let revision = observed.read_with(cx, |workspace, cx| {
        workspace
            .annotation_snapshot(rotation.document_id, cx)
            .unwrap()
            .revision
    });
    // Rotate in the thumbnail menu waits its turn behind a rotation under
    // way, and is refused while saving.
    let rotate = |cx: &mut gpui::VisualTestContext| {
        observed.update(cx, |workspace, cx| {
            workspace.rotate_pages(
                rotation.document_id,
                vec![0],
                butter_paper::annotation_model::PageRotationDirection::Left,
                cx,
            )
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };
    rotate(cx);
    assert_eq!(
        observed.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(rotation.document_id, cx)
            .unwrap()
            .revision),
        revision,
        "a busy rotation holds the next one back"
    );
    observed.update(cx, |workspace, cx| {
        workspace.apply_page_rotation_result(
            &rotation,
            Err("Injected rotation failure".into()),
            cx,
        );
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    // The waiting rotation goes once the failed one is done with.
    rotate(cx);
    let recovered = observed.read_with(cx, |workspace, cx| {
        workspace
            .annotation_snapshot(rotation.document_id, cx)
            .unwrap()
            .revision
    });
    assert!(recovered > revision, "rotation recovers after failure");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let save = observed.update(cx, |workspace, cx| {
        workspace.begin_save(rotation.document_id, cx).unwrap()
    });
    let save_revision = observed.read_with(cx, |workspace, cx| {
        workspace
            .annotation_snapshot(rotation.document_id, cx)
            .unwrap()
            .revision
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    rotate(cx);
    assert_eq!(
        observed.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(rotation.document_id, cx)
            .unwrap()
            .revision),
        save_revision
    );
    observed.update(cx, |workspace, cx| {
        workspace.apply_save_result(&save, Err("Injected save failure".into()), cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(cx.debug_bounds("document-left-rail").unwrap(), rail);
    cx.simulate_click(toggle.center(), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("document-thumbnail-strip").is_none());
    assert_eq!(cx.debug_bounds("document-left-rail-pages").unwrap(), toggle);
}

#[gpui::test]
fn workspace_right_rail_replaces_horizontal_tools_and_keeps_canvas_band_between_rails(
    cx: &mut TestAppContext,
) {
    use butter_paper::annotation_adapter::AnnotationTool;
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let backend = Arc::new(RecordingBackend::default());
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/documents/right-rail-fixture.pdf"), cx)
    });
    cx.run_until_parked();
    let observed = workspace.clone();
    let close = cx.new(move |_| ApplicationCloseWorkspace::new(workspace, backend));
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let rail = cx
        .debug_bounds("document-workspace-right-rail")
        .expect("right rail");
    let left = cx.debug_bounds("document-left-rail").unwrap();
    let toolbar = cx.debug_bounds("viewer-toolbar").unwrap();
    assert_eq!(rail.size.width, px(88.), "two tool columns by default");
    assert_eq!(rail.top(), left.top(), "both rails meet the tab band");
    assert_eq!(
        rail.top(),
        toolbar.top(),
        "viewer toolbar must sit beside the rail"
    );
    assert_eq!(
        toolbar.right(),
        rail.left(),
        "viewer band ends at the rail boundary"
    );
    assert_eq!(rail.bottom(), left.bottom());
    assert!(
        cx.debug_bounds("document-workspace-toolbar-scroll")
            .is_none(),
        "legacy horizontal strip is gone at rest"
    );
    let select = cx.debug_bounds("document-workspace-select-tool").unwrap();
    let hand = cx.debug_bounds("document-workspace-pan-tool").unwrap();
    assert_eq!(select.size, gpui::size(px(32.), px(32.)));
    assert_eq!(
        hand.top(),
        select.top(),
        "General tools share a two-column row"
    );
    assert_eq!(hand.left() - select.right(), px(8.));
    assert!(select.left() >= rail.left() + px(5.));
    assert!(hand.right() <= rail.right() - px(5.));
    let document_id =
        observed.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    let revision = observed.read_with(cx, |workspace, cx| {
        workspace
            .annotation_snapshot(document_id, cx)
            .unwrap()
            .revision
    });
    cx.simulate_click(hand.center(), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let viewport = cx.debug_bounds("document-native-viewport").unwrap();
    let start = viewport.center();
    let end = start - point(px(20.), px(80.));
    let before = observed.read_with(cx, |workspace, cx| {
        workspace
            .document_view_state(document_id, cx)
            .unwrap()
            .scroll()
    });
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_move(
        end,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let after = observed.read_with(cx, |workspace, cx| {
        workspace
            .document_view_state(document_id, cx)
            .unwrap()
            .scroll()
    });
    assert!(after.1 > before.1, "Hand drag scrolls the canvas");
    assert_eq!(
        observed.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(document_id, cx)
            .unwrap()
            .revision),
        revision,
        "Hand must not create or move annotations"
    );
    for (id, tool) in [
        ("document-workspace-text-box-tool", AnnotationTool::TextBox),
        ("document-workspace-arrow-tool", AnnotationTool::Arrow),
        (
            "document-workspace-rectangle-tool",
            AnnotationTool::Rectangle,
        ),
        ("document-workspace-select-tool", AnnotationTool::Select),
    ] {
        let button = cx.debug_bounds(id).expect("tool visible");
        cx.simulate_click(button.center(), gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            observed.read_with(cx, |workspace, cx| workspace
                .annotation_tool(workspace.active_document_id().unwrap(), cx)),
            Some(tool)
        );
    }
    let pages = cx.debug_bounds("document-left-rail-pages").unwrap();
    cx.simulate_click(pages.center(), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.debug_bounds("document-workspace-right-rail").unwrap(),
        rail,
        "opening thumbnails cannot displace the right rail"
    );
    let actions = cx.debug_bounds("document-workspace-rail-actions").unwrap();
    cx.simulate_click(actions.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("document-workspace-properties-sidebar")
            .is_some(),
        "the canonical properties sidebar opens beside the rail"
    );
    assert!(
        cx.debug_bounds("document-workspace-toolbar-scroll")
            .is_none(),
        "the interim document-actions stack is not a second properties pane"
    );
    assert_eq!(
        cx.debug_bounds("document-workspace-right-rail").unwrap(),
        rail
    );
    cx.simulate_click(actions.center(), gpui::Modifiers::default());
    cx.simulate_resize(gpui::size(px(900.), px(440.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let short_rail = cx.debug_bounds("document-workspace-right-rail").unwrap();
    let pinned = cx.debug_bounds("document-workspace-rail-actions").unwrap();
    assert!(pinned.top() >= short_rail.top() && pinned.bottom() <= short_rail.top() + px(48.));
    assert_eq!(
        short_rail.right(),
        px(900.),
        "rail stays on the window edge when resized"
    );
    let scroller = cx
        .debug_bounds("document-workspace-right-rail-scroll")
        .unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: scroller.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-900.))),
        modifiers: gpui::Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let offset = observed.read_with(cx, |workspace, _| workspace.right_rail_scroll_offset());
    let area = cx
        .debug_bounds(butter_paper::document_workspace::DOCUMENT_PEN_TOOL_ID)
        .unwrap_or_else(|| {
            panic!("Pen not revealed: scroll offset={offset:?}, scroller={scroller:?}")
        });
    assert!(
        area.top() >= scroller.top() && area.bottom() <= scroller.bottom(),
        "scrolling reveals the final Draw tool"
    );
    assert_eq!(
        cx.debug_bounds("document-workspace-rail-actions").unwrap(),
        pinned,
        "scrolling cannot move pinned controls"
    );
}

#[gpui::test]
fn right_rail_stays_at_two_columns_and_double_click_toggles_properties(cx: &mut TestAppContext) {
    use butter_paper::annotation_adapter::AnnotationTool;
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let backend = Arc::new(RecordingBackend::default());
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/documents/rail-interactions.pdf"), cx)
    });
    cx.run_until_parked();
    let observed = workspace.clone();
    let close = cx.new(move |_| ApplicationCloseWorkspace::new(workspace, backend));
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let initial = cx.debug_bounds("document-workspace-right-rail").unwrap();
    let cloud_plus = cx
        .debug_bounds(butter_paper::document_workspace::DOCUMENT_CLOUD_PLUS_TOOL_ID)
        .unwrap();
    let callout = cx
        .debug_bounds(butter_paper::document_workspace::DOCUMENT_CALLOUT_TOOL_ID)
        .unwrap();
    assert_eq!(
        cloud_plus.size,
        gpui::size(px(32.), px(32.)),
        "overscan must not enlarge the button"
    );
    assert_eq!(
        cloud_plus.top(),
        callout.top(),
        "composite icon cannot wrap its two-column row"
    );
    // The rail has no resize handle: a drag from its edge, with a threshold
    // move and a frame before the destination, leaves it at two columns.
    // New Tab and New PDF keep their sizes and spacing after each drag.
    let mut tab_action_layouts = Vec::new();
    for requested_width in [170., 20., 600.] {
        let expected_width = 88.;
        let rail = cx.debug_bounds("document-workspace-right-rail").unwrap();
        let start = point(rail.left(), rail.top() + px(100.));
        let end = point(rail.right() - px(requested_width), start.y);
        cx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_move(
            start - point(px(8.), px(0.)),
            Some(gpui::MouseButton::Left),
            gpui::Modifiers::default(),
        );
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_move(
            end,
            Some(gpui::MouseButton::Left),
            gpui::Modifiers::default(),
        );
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let resized = cx.debug_bounds("document-workspace-right-rail").unwrap();
        assert_eq!(
            resized.size.width,
            px(expected_width),
            "requested {requested_width}"
        );
        assert_eq!(resized.right(), initial.right(), "right edge stays pinned");
        assert_eq!(
            cx.debug_bounds("viewer-toolbar").unwrap().right(),
            resized.left()
        );
        // New Tab, then New from template, on the tab track: the rail's
        // width does not move them.
        let open = cx.debug_bounds("document-tab-open").unwrap();
        let primary = cx.debug_bounds("document-tab-new-pdf").unwrap();
        let picker = cx.debug_bounds("document-tab-template-picker").unwrap();
        assert!(open.right() < primary.left() && primary.right() <= picker.left());
        let window_right = cx.update(|window, _| window.viewport_size().width);
        tab_action_layouts.push((
            open.size,
            primary.size,
            picker.size,
            picker.left() - primary.right(),
            window_right - picker.right(),
        ));
        let select = cx.debug_bounds("document-workspace-select-tool").unwrap();
        let hand = cx.debug_bounds("document-workspace-pan-tool").unwrap();
        let actions = cx.debug_bounds("document-workspace-rail-actions").unwrap();
        let scroller = cx
            .debug_bounds("document-workspace-right-rail-scroll")
            .unwrap();
        assert!(
            actions.bottom() <= scroller.top(),
            "pinned controls cannot clip into the scroll body"
        );
        assert_eq!(select.top(), hand.top());
        // The rail's line is drawn inside, so the tools centre on the rail.
        assert_eq!((select.left() + hand.right()) / 2., resized.center().x);
        // Edit PDF is a switch in the viewer toolbar, not a rail tool.
        assert!(
            cx.debug_bounds("document-workspace-edit-pdf-content-tool")
                .is_none()
        );
    }
    assert!(
        tab_action_layouts.windows(2).all(|pair| pair[0] == pair[1]),
        "New Tab and New PDF keep their size and spacing: {tab_action_layouts:?}"
    );
    let document_id =
        observed.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    for (selector, tool) in [
        (
            "document-workspace-rectangle-tool",
            AnnotationTool::Rectangle,
        ),
        (
            "document-workspace-highlight-tool",
            AnnotationTool::Highlight,
        ),
    ] {
        for expected_open in [true, false] {
            let button = cx.debug_bounds(selector).unwrap();
            cx.simulate_click(button.center(), gpui::Modifiers::default());
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(
                cx.debug_bounds("document-workspace-properties-sidebar")
                    .is_some(),
                !expected_open,
                "single click only selects"
            );
            cx.simulate_event(gpui::MouseDownEvent {
                position: button.center(),
                button: gpui::MouseButton::Left,
                click_count: 2,
                ..Default::default()
            });
            cx.simulate_event(gpui::MouseUpEvent {
                position: button.center(),
                button: gpui::MouseButton::Left,
                click_count: 2,
                ..Default::default()
            });
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(
                cx.debug_bounds("document-workspace-properties-sidebar")
                    .is_some(),
                expected_open
            );
            assert_eq!(
                observed.read_with(cx, |workspace, cx| workspace
                    .annotation_tool(document_id, cx)),
                Some(tool)
            );
            assert_eq!(
                cx.debug_bounds("document-workspace-right-rail").unwrap(),
                initial
            );
            if tool == AnnotationTool::Highlight && expected_open {
                let sidebar = cx
                    .debug_bounds("document-workspace-properties-sidebar")
                    .unwrap();
                assert_eq!(
                    sidebar.top(),
                    initial.top(),
                    "properties begins below tabs beside the canvas toolbar"
                );
                assert_eq!(sidebar.bottom(), initial.bottom());
                assert_eq!(sidebar.right(), initial.left());
                for (requested_width, expected_width) in [(200., 240.), (500., 420.), (300., 300.)]
                {
                    let sidebar = cx
                        .debug_bounds("document-workspace-properties-sidebar")
                        .unwrap();
                    let start = point(sidebar.left(), sidebar.top() + px(100.));
                    let end = point(sidebar.right() - px(requested_width), start.y);
                    cx.simulate_mouse_down(
                        start,
                        gpui::MouseButton::Left,
                        gpui::Modifiers::default(),
                    );
                    cx.simulate_mouse_move(
                        start - point(px(8.), px(0.)),
                        Some(gpui::MouseButton::Left),
                        gpui::Modifiers::default(),
                    );
                    cx.update(|window, cx| window.draw(cx).clear(cx));
                    cx.simulate_mouse_move(
                        end,
                        Some(gpui::MouseButton::Left),
                        gpui::Modifiers::default(),
                    );
                    cx.update(|window, cx| window.draw(cx).clear(cx));
                    cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
                    cx.run_until_parked();
                    cx.update(|window, cx| window.draw(cx).clear(cx));
                    let resized = cx
                        .debug_bounds("document-workspace-properties-sidebar")
                        .unwrap();
                    assert_eq!(
                        resized.size.width,
                        px(expected_width),
                        "properties clamp {requested_width}"
                    );
                    assert_eq!(
                        resized.right(),
                        initial.left(),
                        "properties resize keeps rail pinned"
                    );
                    assert_eq!(
                        cx.debug_bounds("highlight-defaults-panel-opacity-input")
                            .unwrap()
                            .size
                            .width,
                        px(80.)
                    );
                }
                observed.update(cx, |workspace, cx| {
                    workspace
                        .set_highlight_defaults(document_id, "#00ff00", 18., 0.5, cx)
                        .unwrap()
                });
                cx.update(|window, cx| window.draw(cx).clear(cx));
                let before = observed.read_with(cx, |workspace, cx| {
                    workspace.highlight_defaults(document_id, cx).unwrap()
                });
                let accordion = cx
                    .debug_bounds("highlight-defaults-panel-accordion")
                    .unwrap();
                cx.simulate_click(
                    gpui::point(accordion.center().x, accordion.bottom() - px(16.)),
                    gpui::Modifiers::default(),
                );
                cx.executor()
                    .advance_clock(std::time::Duration::from_millis(250));
                cx.update(|window, cx| window.draw(cx).clear(cx));
                for confirm in [false, true] {
                    let reset = cx
                        .debug_bounds("highlight-defaults-panel-reset-button")
                        .unwrap();
                    cx.simulate_click(reset.center(), gpui::Modifiers::default());
                    cx.executor()
                        .advance_clock(std::time::Duration::from_millis(250));
                    cx.update(|window, cx| window.draw(cx).clear(cx));
                    let action = cx
                        .debug_bounds(if confirm {
                            "highlight-defaults-panel-reset-confirm"
                        } else {
                            "highlight-defaults-panel-reset-cancel"
                        })
                        .unwrap();
                    cx.simulate_click(action.center(), gpui::Modifiers::default());
                    cx.update(|window, cx| window.draw(cx).clear(cx));
                    let after = observed.read_with(cx, |workspace, cx| {
                        workspace.highlight_defaults(document_id, cx).unwrap()
                    });
                    if confirm {
                        assert_eq!(after.color, "#ffff00");
                        assert_eq!(after.width_pt, 12.);
                        assert_eq!(after.opacity, 1.);
                    } else {
                        assert_eq!(after, before, "cancelling reset preserves defaults");
                    }
                }
            }
        }
    }
    for level in [2, -2, 0] {
        cx.update(|window, cx| {
            butter_paper::application_shell::apply_application_ui_zoom(level, px(16.), window, cx);
            observed.update(cx, |_, cx| cx.notify());
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let rem = butter_paper::application_shell::application_ui_zoom_font_size(px(16.), level);
        let rail = cx.debug_bounds("document-workspace-right-rail").unwrap();
        let select = cx.debug_bounds("document-workspace-select-tool").unwrap();
        let hand = cx.debug_bounds("document-workspace-pan-tool").unwrap();
        assert!(
            (rail.size.width - rem * 5.5).abs() < px(1.),
            "zoom preserves the two-column width"
        );
        assert_eq!(select.top(), hand.top());
        assert!(
            select.left() > rail.left() && hand.right() < rail.right(),
            "zoomed tools fit their rail"
        );
    }
    observed.update(cx, |workspace, cx| {
        workspace.begin_save(document_id, cx).unwrap();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let actions = cx.debug_bounds("document-workspace-rail-actions").unwrap();
    cx.simulate_click(actions.center(), gpui::Modifiers::default());
    let rectangle = cx
        .debug_bounds("document-workspace-rectangle-tool")
        .unwrap();
    cx.simulate_event(gpui::MouseDownEvent {
        position: rectangle.center(),
        click_count: 2,
        ..Default::default()
    });
    cx.simulate_event(gpui::MouseUpEvent {
        position: rectangle.center(),
        click_count: 2,
        ..Default::default()
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("document-workspace-toolbar-scroll")
            .is_none(),
        "saving disables the properties toggle and tool double-click"
    );
}

#[gpui::test]
fn highlight_defaults_panel_events_are_document_scoped_and_disabled_safe(cx: &mut TestAppContext) {
    use butter_paper::annotation_adapter::AnnotationTool;
    use butter_paper::document_workspace::PenAnnotationDefaults;
    use butter_paper::highlight_defaults_panel::HighlightDefaultsEvent;
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let workspace =
        cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/documents/highlight-defaults-a.pdf"), cx)
    });
    cx.run_until_parked();
    let first = workspace.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    let defaults = PenAnnotationDefaults {
        color: "#00ff00".into(),
        width_pt: 18.,
        opacity: 0.5,
    };
    let event = HighlightDefaultsEvent::Change {
        document_id: first,
        defaults: defaults.clone(),
    };
    workspace.update(cx, |workspace, cx| {
        assert!(
            !workspace
                .apply_highlight_defaults_event(&event, cx)
                .unwrap(),
            "inactive tool cannot receive defaults edits"
        );
        workspace
            .set_annotation_tool(first, AnnotationTool::Highlight, cx)
            .unwrap();
        let before = workspace.annotation_snapshot(first, cx).unwrap();
        assert!(
            workspace
                .apply_highlight_defaults_event(&event, cx)
                .unwrap()
        );
        assert_eq!(
            workspace.highlight_defaults(first, cx),
            Some(defaults.clone())
        );
        assert_eq!(
            workspace.annotation_snapshot(first, cx).unwrap(),
            before,
            "editing defaults must not modify existing annotations or history"
        );
        let invalid = HighlightDefaultsEvent::Change {
            document_id: first,
            defaults: PenAnnotationDefaults {
                width_pt: f64::NAN,
                ..defaults.clone()
            },
        };
        assert!(
            workspace
                .apply_highlight_defaults_event(&invalid, cx)
                .is_err()
        );
        assert_eq!(
            workspace.highlight_defaults(first, cx),
            Some(defaults.clone())
        );
    });
    workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/documents/highlight-defaults-b.pdf"), cx)
    });
    cx.run_until_parked();
    let second = workspace.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    assert_ne!(first, second);
    workspace.update(cx, |workspace, cx| {
        workspace
            .set_annotation_tool(second, AnnotationTool::Highlight, cx)
            .unwrap();
        assert!(
            !workspace
                .apply_highlight_defaults_event(&event, cx)
                .unwrap(),
            "stale document event must be ignored"
        );
        workspace.begin_save(second, cx).unwrap();
        let current = HighlightDefaultsEvent::Change {
            document_id: second,
            defaults,
        };
        assert!(
            !workspace
                .apply_highlight_defaults_event(&current, cx)
                .unwrap(),
            "saving rejects defaults mutation"
        );
    });
}

#[gpui::test]
fn pending_thumbnail_result_cannot_repopulate_a_closed_rotated_or_replaced_document(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    for invalidation in ["rotation", "close", "resource replacement"] {
        let backend = Arc::new(RecordingBackend {
            page_count: 30,
            ..Default::default()
        });
        let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
        workspace.update(cx, |workspace, cx| {
            workspace.open_path(PathBuf::from("/documents/sidebar-stale.pdf"), cx)
        });
        cx.run_until_parked();
        let id = workspace.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
        workspace.update(cx, |workspace, cx| {
            workspace.request_thumbnail_preview(id, 24, false, cx);
            if invalidation == "close" {
                assert!(workspace.close_document(id, cx));
            } else if invalidation == "resource replacement" {
                let save = workspace.begin_save(id, cx).unwrap();
                let reopened = backend
                    .open(&OpenDocumentRequest {
                        document_id: id,
                        generation: 2,
                        path: PathBuf::from("/documents/sidebar-stale.pdf"),
                    })
                    .unwrap();
                assert_eq!(
                    workspace.apply_save_result(
                        &save,
                        Ok(SavedNativeDocument::new(reopened, save.annotation_revision)),
                        cx
                    ),
                    butter_paper::document_workspace::ApplyDisposition::Applied
                );
            } else {
                workspace
                    .begin_page_rotation(
                        id,
                        24,
                        butter_paper::annotation_model::PageRotationDirection::Right,
                        cx,
                    )
                    .unwrap();
            }
        });
        cx.run_until_parked();
        workspace.read_with(cx, |workspace, cx| {
            if invalidation == "close" {
                assert!(workspace.session(id, cx).is_none());
            } else {
                assert!(
                    workspace
                        .session(id, cx)
                        .unwrap()
                        .read(cx)
                        .thumbnail_base_raster(24)
                        .is_none(),
                    "stale pixels must not be installed after {invalidation}"
                );
            }
        });
    }
}

#[gpui::test]
fn workspace_empty_left_rail_is_inert(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let workspace =
        cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    let (_, cx) = cx.add_window_view(move |window, cx| Root::new(workspace, window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let toggle = cx
        .debug_bounds("document-left-rail-pages")
        .expect("disabled toggle remains visible");
    cx.simulate_click(toggle.center(), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("document-thumbnail-strip").is_none());
    assert!(cx.debug_bounds("document-left-rail").is_some());
}

#[gpui::test]
fn thumbnail_click_scrolls_the_continuous_canvas_to_its_page(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let backend = Arc::new(RecordingBackend {
        page_count: 3,
        ..Default::default()
    });
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/documents/sidebar-navigation.pdf"), cx)
    });
    cx.run_until_parked();
    let observed = workspace.clone();
    workspace.update(cx, |workspace, cx| {
        let id = workspace.active_document_id().unwrap();
        workspace.set_page_view_mode(
            id,
            butter_paper::page_view_control::PageViewMode::Continuous,
            cx,
        );
    });
    let close = cx.new(move |_| ApplicationCloseWorkspace::new(workspace, backend));
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let toggle = cx.debug_bounds("document-left-rail-pages").unwrap();
    cx.simulate_click(toggle.center(), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let second = cx.debug_bounds("document-1-thumbnail-1").unwrap();
    cx.simulate_click(second.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    // Exercise the subsequent viewport refresh too: the old implementation
    // briefly selected page 2, then restored page 1 from the unchanged scroll.
    for _ in 0..3 {
        observed.update(cx, |workspace, cx| {
            let id = workspace.active_document_id().unwrap();
            workspace
                .refresh_viewport_async(id, 700., 500., 1., cx)
                .unwrap();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
    observed.read_with(cx, |workspace, cx| {
        let id = workspace.active_document_id().unwrap();
        assert_eq!(
            workspace.session(id, cx).unwrap().read(cx).current_page(),
            1
        );
        assert!(workspace.document_view_state(id, cx).unwrap().scroll().1 > 0.);
    });
}

#[gpui::test]
fn sidebar_loads_visible_late_pages_and_retries_failure_without_navigation(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let backend = Arc::new(RecordingBackend {
        page_count: 30,
        ..Default::default()
    });
    backend.preview_failure.store(true, Ordering::Release);
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/documents/sidebar-late-pages.pdf"), cx)
    });
    cx.run_until_parked();
    let observed = workspace.clone();
    let close = cx.new({
        let backend = backend.clone();
        move |_| ApplicationCloseWorkspace::new(workspace, backend)
    });
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let toggle = cx.debug_bounds("document-left-rail-pages").unwrap();
    cx.simulate_click(toggle.center(), gpui::Modifiers::default());
    observed.update(cx, |workspace, cx| {
        workspace.scroll_thumbnail_to_page(24, cx);
    });
    for _ in 0..4 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
    let retry = cx
        .debug_bounds("thumbnail-24-retry")
        .expect("failed visible preview offers retry");
    assert_eq!(
        backend.preview_failures.load(Ordering::Acquire),
        1,
        "redraws must not retry automatically"
    );
    backend.preview_failure.store(false, Ordering::Release);
    cx.simulate_click(retry.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("thumbnail-24-retry").is_none());
    observed.read_with(cx, |workspace, cx| {
        let session = workspace
            .session(workspace.active_document_id().unwrap(), cx)
            .unwrap()
            .read(cx);
        assert!(session.thumbnail_base_raster(24).is_some());
        assert_eq!(
            session.current_page(),
            0,
            "preview loading must not navigate the canvas"
        );
        assert!(
            session.thumbnail_base_raster(15).is_none(),
            "off-screen middle pages stay lazy"
        );
    });
}

#[gpui::test]
fn sidebar_draws_the_rows_a_fling_stops_on_and_not_the_rows_it_passes(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let backend = Arc::new(RecordingBackend {
        page_count: 60,
        ..Default::default()
    });
    let workspace = cx.new(|cx| {
        let mut workspace = DocumentWorkspace::with_opener(backend.clone(), cx);
        workspace.set_thumbnail_scroll_settle(std::time::Duration::from_millis(100));
        workspace
    });
    workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/documents/sidebar-fling.pdf"), cx)
    });
    cx.run_until_parked();
    let observed = workspace.clone();
    let close = cx.new({
        let backend = backend.clone();
        move |_| ApplicationCloseWorkspace::new(workspace, backend)
    });
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let toggle = cx.debug_bounds("document-left-rail-pages").unwrap();
    cx.simulate_click(toggle.center(), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let opened = backend.thumbnail_renders.lock().unwrap().len();
    assert!(opened > 0, "the rows first shown are drawn at once");
    // A fling: the rows shown change on every frame.
    for page in [10, 20, 30, 40, 50] {
        observed.update(cx, |workspace, cx| {
            workspace.scroll_thumbnail_to_page(page, cx);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
    assert_eq!(
        backend.thumbnail_renders.lock().unwrap().len(),
        opened,
        "no thumbnail is drawn for rows a fling passes"
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(150));
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let drawn = backend.thumbnail_renders.lock().unwrap()[opened..].to_vec();
    assert!(!drawn.is_empty(), "the rows the fling stopped on are drawn");
    assert!(
        drawn.iter().all(|page| *page >= 40),
        "only rows near where the list stopped are drawn: {drawn:?}"
    );
    observed.read_with(cx, |workspace, cx| {
        let session = workspace
            .session(workspace.active_document_id().unwrap(), cx)
            .unwrap()
            .read(cx);
        assert!(session.thumbnail_base_raster(50).is_some());
        assert!(session.thumbnail_base_raster(25).is_none());
    });
}

#[gpui::test]
fn arriving_thumbnails_leave_the_canvas_tiles_alone(cx: &mut TestAppContext) {
    // Each thumbnail once redrew every canvas tile on Windows and Linux,
    // flashing the page while the sidebar filled in.
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let backend = Arc::new(RecordingBackend {
        page_count: 12,
        ..Default::default()
    });
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/documents/canvas-steady.pdf"), cx)
    });
    cx.run_until_parked();
    let observed = workspace.clone();
    let close = cx.new({
        let backend = backend.clone();
        move |_| ApplicationCloseWorkspace::new(workspace, backend)
    });
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    for _ in 0..2 {
        observed.update(cx, |workspace, cx| {
            let id = workspace.active_document_id().unwrap();
            workspace
                .refresh_viewport_async(id, 700., 500., 1., cx)
                .unwrap();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
    let tiles = backend.tile_renders.load(Ordering::Acquire);
    assert!(tiles > 0, "the canvas drew its tiles");
    for page in 1..8 {
        observed.update(cx, |workspace, cx| {
            let id = workspace.active_document_id().unwrap();
            workspace.request_thumbnail_preview(id, page, false, cx);
        });
        cx.run_until_parked();
        // As the next frame would (the test platform runs no frame
        // callbacks): plan the canvas, drawing whatever it lacks.
        observed.update(cx, |workspace, cx| {
            let id = workspace.active_document_id().unwrap();
            workspace
                .refresh_viewport_async(id, 700., 500., 1., cx)
                .unwrap();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
    observed.read_with(cx, |workspace, cx| {
        let session = workspace
            .session(workspace.active_document_id().unwrap(), cx)
            .unwrap()
            .read(cx);
        assert!((1..8).all(|page| session.thumbnail_base_raster(page).is_some()));
    });
    assert_eq!(
        backend.tile_renders.load(Ordering::Acquire),
        tiles,
        "thumbnails arriving must not make the canvas draw its tiles again"
    );
}

#[gpui::test]
fn clicking_a_drawn_thumbnail_moves_to_its_page_at_once(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let backend = Arc::new(RecordingBackend {
        page_count: 6,
        ..Default::default()
    });
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/documents/instant-page.pdf"), cx)
    });
    cx.run_until_parked();
    let observed = workspace.clone();
    let close = cx.new({
        let backend = backend.clone();
        move |_| ApplicationCloseWorkspace::new(workspace, backend)
    });
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let toggle = cx.debug_bounds("document-left-rail-pages").unwrap();
    cx.simulate_click(toggle.center(), gpui::Modifiers::default());
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
    let session = |cx: &mut gpui::VisualTestContext| {
        observed.read_with(cx, |workspace, cx| {
            let session = workspace
                .session(workspace.active_document_id().unwrap(), cx)
                .unwrap()
                .read(cx);
            (
                session.current_page(),
                session.thumbnail_base_raster(2).is_some(),
                session.base_raster_of(2).is_some(),
            )
        })
    };
    assert_eq!(session(cx), (0, true, false), "page 3's thumbnail is drawn");
    let row = cx.debug_bounds("document-1-thumbnail-2").unwrap();
    // Dispatched without letting the render run, as a slow page would be.
    cx.update(|window, cx| {
        for event in [
            gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                button: gpui::MouseButton::Left,
                position: row.center(),
                modifiers: gpui::Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }),
            gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                button: gpui::MouseButton::Left,
                position: row.center(),
                modifiers: gpui::Modifiers::default(),
                click_count: 1,
            }),
        ] {
            window.dispatch_event(event, cx);
        }
    });
    assert_eq!(
        session(cx),
        (2, true, false),
        "the page changes on the click, before its render arrives"
    );
    cx.run_until_parked();
    assert_eq!(
        session(cx),
        (2, true, true),
        "then its render replaces the thumbnail"
    );
}

#[test]
fn sidebar_scale_label_uses_the_ratio_not_the_name() {
    use butter_paper::annotation_model::{PageScale, ScalePrecision, ScaleUnit};
    use butter_paper::page_scale_catalog::{STANDARD_SCALES, scale_label};
    let scale = PageScale::custom(
        0,
        "A custom scale",
        ScaleUnit::Cm,
        ScaleUnit::M,
        1.,
        0.5,
        None,
        ScalePrecision::default(),
    )
    .unwrap();
    assert_eq!(scale_label(&scale), "1:50");
    let imperial = PageScale::custom(
        0,
        "Feet",
        ScaleUnit::In,
        ScaleUnit::Ft,
        1.,
        1.,
        None,
        ScalePrecision::default(),
    )
    .unwrap();
    assert_eq!(scale_label(&imperial), "1\" = 1'-0\"");
    for standard in STANDARD_SCALES {
        let unit = if standard.system.imperial() {
            ScaleUnit::Ft
        } else {
            ScaleUnit::M
        };
        let scale = standard.page_scale(0, unit, ScalePrecision::default());
        assert_eq!(scale_label(&scale), standard.label);
    }
}

#[gpui::test]
fn sidebar_reveals_page_selected_by_external_canvas_scroll(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let backend = Arc::new(RecordingBackend {
        page_count: 30,
        ..Default::default()
    });
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/documents/sidebar-canvas-scroll.pdf"), cx)
    });
    cx.run_until_parked();
    let observed = workspace.clone();
    let close = cx.new(move |_| ApplicationCloseWorkspace::new(workspace, backend));
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let toggle = cx.debug_bounds("document-left-rail-pages").unwrap();
    cx.simulate_click(toggle.center(), gpui::Modifiers::default());
    observed.update(cx, |workspace, cx| {
        let id = workspace.active_document_id().unwrap();
        workspace.set_page_view_mode(
            id,
            butter_paper::page_view_control::PageViewMode::Continuous,
            cx,
        );
        workspace
            .refresh_viewport_async(id, 700., 500., 1., cx)
            .unwrap();
        workspace.set_viewport_scroll(id, 0., 18000., cx);
        workspace
            .refresh_viewport_async(id, 700., 500., 1., cx)
            .unwrap();
    });
    cx.run_until_parked();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let page = observed.read_with(cx, |workspace, cx| {
        workspace
            .session(workspace.active_document_id().unwrap(), cx)
            .unwrap()
            .read(cx)
            .current_page()
    });
    assert!(
        page > 3,
        "the canvas moved beyond initially visible thumbnails"
    );
    let row_id: &'static str = Box::leak(format!("document-1-thumbnail-{page}").into_boxed_str());
    let row = cx
        .debug_bounds(row_id)
        .expect("selected page materialised in sidebar");
    assert!(
        cx.debug_bounds("document-thumbnail-strip")
            .unwrap()
            .contains(&row.center()),
        "selected thumbnail is revealed without sidebar input"
    );
}

#[derive(Default)]
struct RecordingBackend {
    released: Arc<AtomicBool>,
    page_count: usize,
    preview_failure: Arc<AtomicBool>,
    preview_failures: Arc<std::sync::atomic::AtomicUsize>,
    /// Pages whose thumbnails were drawn, in order.
    thumbnail_renders: Arc<std::sync::Mutex<Vec<u32>>>,
    tile_renders: Arc<std::sync::atomic::AtomicUsize>,
}

impl NativeDocumentOpener for RecordingBackend {
    fn open(&self, request: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        OpenedNativeDocument::new(
            request
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("fixture.pdf"),
            vec![(612., 792.); self.page_count.max(1)],
            raster(32, 40),
            vec![ThumbnailSurface::new(0, raster(8, 10))],
            Arc::new(RecordingResource {
                released: self.released.clone(),
                preview_failure: self.preview_failure.clone(),
                preview_failures: self.preview_failures.clone(),
                thumbnail_renders: self.thumbnail_renders.clone(),
                tile_renders: self.tile_renders.clone(),
            }),
        )
    }
}

impl NativeDocumentSaver for RecordingBackend {
    fn save(&self, _: &SaveDocumentRequest) -> Result<SavedNativeDocument, String> {
        Err("saving is outside this native-ingress test".into())
    }
}

struct RecordingResource {
    released: Arc<AtomicBool>,
    preview_failure: Arc<AtomicBool>,
    preview_failures: Arc<std::sync::atomic::AtomicUsize>,
    thumbnail_renders: Arc<std::sync::Mutex<Vec<u32>>>,
    tile_renders: Arc<std::sync::atomic::AtomicUsize>,
}

impl NativeDocumentResource for RecordingResource {
    fn worker_pid(&self) -> Option<u32> {
        None
    }

    fn render_page(&self, page: u32, width: u32) -> Result<RasterSurface, String> {
        if page == 24 && self.preview_failure.load(Ordering::Acquire) {
            self.preview_failures.fetch_add(1, Ordering::AcqRel);
            return Err("Injected thumbnail failure".into());
        }
        Ok(raster(width, width.max(1)))
    }

    fn render_thumbnail(
        &self,
        page: u32,
        width: u32,
        _paper: butter_paper::paper_appearance::PaperAppearance,
    ) -> Result<RasterSurface, String> {
        self.thumbnail_renders.lock().unwrap().push(page);
        self.render_page(page, width)
    }

    fn render_tile(&self, request: TileRequest) -> Result<RasterSurface, String> {
        self.tile_renders.fetch_add(1, Ordering::AcqRel);
        Ok(raster(
            request.crop.width.max(1) as u32,
            request.crop.height.max(1) as u32,
        ))
    }

    fn close(&self) -> Result<(), String> {
        self.released.store(true, Ordering::Release);
        Ok(())
    }

    fn is_released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }
}

fn raster(width: u32, height: u32) -> RasterSurface {
    RasterSurface::new(
        width,
        height,
        vec![0xff; width as usize * height as usize * 4],
    )
    .unwrap()
}

fn menu_names(menus: &[Menu]) -> Vec<&str> {
    menus.iter().map(|menu| menu.name.as_ref()).collect()
}

fn menu<'a>(menus: &'a [Menu], name: &str) -> &'a Menu {
    menus.iter().find(|menu| menu.name == name).unwrap()
}

fn menu_item_names(menu: &Menu) -> Vec<&str> {
    menu.items
        .iter()
        .map(|item| match item {
            MenuItem::Action { name, .. } => name.as_ref(),
            MenuItem::Separator => "<separator>",
            _ => "<other>",
        })
        .collect()
}

fn item<'a>(menu: &'a Menu, name: &str) -> &'a MenuItem {
    menu.items
        .iter()
        .find(|item| matches!(item, MenuItem::Action { name: actual, .. } if actual == name))
        .unwrap()
}

fn assert_action(item: &MenuItem, matches: impl FnOnce(&dyn Action) -> bool) {
    match item {
        MenuItem::Action { action, .. } => assert!(matches(action.as_ref())),
        _ => panic!("expected an action menu item"),
    }
}

#[gpui::test]
fn production_update_button_is_pinned_in_empty_and_document_workspaces(cx: &mut TestAppContext) {
    use butter_paper::{
        application_settings::ApplicationUpdatePresentation,
        application_shell::{DownloadUpdate, OpenUpdateSettings, RestartToUpdate},
        document_tab_bar::TEMPLATE_PICKER_ID,
        native_updater::ReleaseVersion,
    };
    use std::{cell::Cell, rc::Rc};
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let opened = Rc::new(Cell::new(0));
    let downloaded = Rc::new(Cell::new(0));
    let restarted = Rc::new(Cell::new(0));
    cx.update(|cx| {
        let count = opened.clone();
        cx.on_action(move |_: &OpenUpdateSettings, _| count.set(count.get() + 1));
        let count = downloaded.clone();
        cx.on_action(move |_: &DownloadUpdate, _| count.set(count.get() + 1));
        let count = restarted.clone();
        cx.on_action(move |_: &RestartToUpdate, _| count.set(count.get() + 1));
    });
    let workspace =
        cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    let (_, cx) = cx.add_window_view({
        let workspace = workspace.clone();
        move |window, cx| Root::new(workspace, window, cx)
    });
    let version = ReleaseVersion::stable(0, 1, 5);
    for has_document in [false, true] {
        if has_document {
            workspace.update(cx, |workspace, cx| {
                workspace.open_path(PathBuf::from("/documents/update-review.pdf"), cx)
            });
            cx.run_until_parked();
        }
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            cx.debug_bounds("document-status-bar").is_some(),
            has_document
        );
        for status in [
            UpdateMenuStatus::Available(version),
            UpdateMenuStatus::Downloading,
            UpdateMenuStatus::Ready(version),
        ] {
            cx.update(|_, cx| {
                cx.set_global(ApplicationUpdatePresentation {
                    state: UpdateMenuState {
                        status,
                        frequency: None,
                    },
                    ..Default::default()
                })
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let update = cx.debug_bounds("application-update-button");
            // Without the in-app menu bar the tab strip carries the status.
            if !matches!(status, UpdateMenuStatus::Ready(_)) {
                assert!(update.is_some(), "{status:?} shows in the tab strip");
                cx.update(|_, cx| {
                    cx.set_global(butter_paper::application_settings::InAppMenuBarShown(true))
                });
                cx.update(|window, cx| window.draw(cx).clear(cx));
                assert!(
                    cx.debug_bounds("application-update-lane").is_none(),
                    "the menu bar carries the status when it is shown"
                );
                cx.update(|_, cx| {
                    cx.set_global(butter_paper::application_settings::InAppMenuBarShown(false))
                });
                continue;
            }
            let update = update.expect("ready update affordance");
            let template = cx.debug_bounds(TEMPLATE_PICKER_ID).unwrap();
            let lane = cx.debug_bounds("application-update-lane").unwrap();
            let tabs = cx
                .debug_bounds("document-workspace-native-session-tab-bar")
                .unwrap();
            assert_eq!(lane.top(), tabs.top());
            assert_eq!(lane.bottom(), tabs.bottom());
            // The update follows the tab row's buttons, at the strip's end.
            assert!(lane.left() >= template.right());
            assert!(update.left() > lane.left());
            assert_eq!(update.size.height, template.size.height);
            let width = cx.update(|window, _| window.viewport_size().width);
            assert!(update.right() <= width);
            let before = (opened.get(), downloaded.get(), restarted.get());
            cx.simulate_click(update.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            assert_eq!(
                (opened.get(), downloaded.get(), restarted.get()),
                before,
                "opening the popup must not restart"
            );
            cx.simulate_keystrokes("down down enter");
            cx.run_until_parked();
            assert_eq!(
                (opened.get(), downloaded.get(), restarted.get()),
                (before.0, before.1, before.2 + 1)
            );
        }
        cx.simulate_resize(gpui::size(px(800.), px(650.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let update = cx.debug_bounds("application-update-button").unwrap();
        assert!(update.right() <= px(800.));
        assert_eq!(
            update.size.height,
            cx.debug_bounds(TEMPLATE_PICKER_ID).unwrap().size.height
        );
        cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    }
}

/// A PDF dropped anywhere on a window opens as a new tab in that window,
/// several in drop order: on the tab strip, on the page, on the New Tab
/// page and on an empty window. Another window keeps its tabs.
#[gpui::test]
fn pdfs_dropped_on_a_window_open_as_tabs_in_that_window(cx: &mut TestAppContext) {
    use butter_paper::document_workspace::{DOCUMENT_EMPTY_ID, DOCUMENT_PAGE_ID};
    cx.update(|cx| {
        gpui_component::init(cx);
        cx.set_reduce_motion(true);
    });
    let backend = Arc::new(RecordingBackend::default());
    // A window with a document, and one with none.
    let window = |open: Option<&str>, cx: &mut TestAppContext| {
        let workspace = cx.new({
            let backend = backend.clone();
            move |cx| DocumentWorkspace::with_opener(backend, cx)
        });
        if let Some(path) = open {
            workspace.update(cx, |workspace, cx| {
                workspace.open_path(PathBuf::from(path), cx)
            });
            cx.run_until_parked();
        }
        let close = cx.new({
            let workspace = workspace.clone();
            let backend = backend.clone();
            move |_| ApplicationCloseWorkspace::new(workspace, backend)
        });
        let (_, window_cx) = cx.add_window_view(move |window, cx| {
            let shell =
                cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
            Root::new(shell, window, cx)
        });
        let window_cx = window_cx.clone();
        (workspace, window_cx)
    };
    let (other, mut other_cx) = window(Some("/tmp/other-window.pdf"), cx);
    let (workspace, mut window_cx) = window(Some("/tmp/this-window.pdf"), cx);
    let (empty, mut empty_cx) = window(None, cx);
    let paths = |workspace: &gpui::Entity<DocumentWorkspace>, cx: &mut gpui::VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .sessions()
                .iter()
                .map(|session| session.read(cx).path().to_owned())
                .collect::<Vec<_>>()
        })
    };
    let drop = |cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>, files: &[&str]| {
        cx.simulate_event(FileDropEvent::Entered {
            position: at,
            paths: ExternalPaths(files.iter().map(PathBuf::from).collect()),
        });
        cx.simulate_event(FileDropEvent::Submit { position: at });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };
    for cx in [&mut other_cx, &mut window_cx, &mut empty_cx] {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let strip = window_cx
        .debug_bounds(butter_paper::document_workspace::DOCUMENT_SESSION_TABS_ID)
        .unwrap()
        .center();
    drop(
        &mut window_cx,
        strip,
        &["/tmp/strip-b.pdf", "/tmp/strip-a.pdf"],
    );
    let page = window_cx.debug_bounds(DOCUMENT_PAGE_ID).unwrap().center();
    drop(&mut window_cx, page, &["/tmp/page.pdf"]);
    workspace.update(&mut window_cx, |workspace, cx| workspace.open_new_tab(cx));
    window_cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(workspace.read_with(&window_cx, |workspace, _| workspace.is_new_tab_open()));
    drop(&mut window_cx, page, &["/tmp/new-tab.pdf"]);
    assert_eq!(
        paths(&workspace, &mut window_cx),
        [
            "/tmp/this-window.pdf",
            "/tmp/strip-b.pdf",
            "/tmp/strip-a.pdf",
            "/tmp/page.pdf",
            "/tmp/new-tab.pdf",
        ]
        .map(PathBuf::from),
        "each drop opens in this window, in drop order"
    );
    let centre = empty_cx.debug_bounds(DOCUMENT_EMPTY_ID).unwrap().center();
    drop(&mut empty_cx, centre, &["/tmp/empty.pdf"]);
    assert_eq!(
        paths(&empty, &mut empty_cx),
        [PathBuf::from("/tmp/empty.pdf")]
    );
    assert_eq!(
        paths(&other, &mut other_cx),
        [PathBuf::from("/tmp/other-window.pdf")],
        "the other window keeps its tabs"
    );
    // A drag from another app sends the window no mouse moves: a drop
    // after a key press must open as well.
    window_cx.simulate_keystrokes("escape");
    window_cx.run_until_parked();
    drop(&mut window_cx, page, &["/tmp/after-key.pdf"]);
    assert_eq!(
        paths(&workspace, &mut window_cx).last(),
        Some(&PathBuf::from("/tmp/after-key.pdf")),
        "a drop after a key press opens"
    );
}

/// The native menu bar and the in-window menu (Windows and Linux) list the
/// same page views, Cover Page Alone and Split View, checked alike.
#[test]
fn both_view_menus_list_the_same_page_view_items() {
    let state = NativeApplicationMenuState {
        has_active_document: true,
        document_ready: true,
        two_pages_view_checked: true,
        cover_page_alone_checked: true,
        split_view_checked: true,
        ..Default::default()
    };
    let shell = ApplicationMenuShellState::default();
    let native = build_native_application_menus_with_shell(state, shell);
    let in_window = build_in_window_application_menus(state, shell);
    let view_items = |menus: &[Menu]| {
        let names = menu_item_names(menu(menus, "View"));
        let start = names
            .iter()
            .position(|name| *name == "Continuous View")
            .expect("the View menu lists Continuous View");
        let end = names
            .iter()
            .position(|name| *name == "Split View")
            .expect("the View menu lists Split View");
        names[start..=end]
            .iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>()
    };
    let expected = [
        "Continuous View",
        "Single Page View",
        "Two Pages View",
        "Cover Page Alone",
        "<separator>",
        "Split View",
    ];
    assert_eq!(view_items(&native), expected);
    assert_eq!(view_items(&in_window), expected);
    for menus in [&native, &in_window] {
        let view = menu(menus, "View");
        assert!(!item(view, "Continuous View").is_checked());
        assert!(item(view, "Two Pages View").is_checked());
        assert!(item(view, "Cover Page Alone").is_checked());
        assert!(item(view, "Split View").is_checked());
        assert_action(item(view, "Two Pages View"), |action| {
            action.as_any().is::<TwoPagesView>()
        });
        assert_action(item(view, "Split View"), |action| {
            action.as_any().is::<ToggleSplitView>()
        });
    }
}
