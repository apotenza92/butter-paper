use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use butter_paper_gpui_migration::{
    annotation_model::{MarkupId, PdfPoint},
    application_close_workspace::{
        ApplicationCloseShell, ApplicationCloseWorkspace, RequestApplicationClose, RequestApplicationQuit,
        register_application_close_action,
    },
    application_shell::{
        MakeInterfaceBigger, MakeInterfaceSmaller, ResetInterfaceSize, ToggleApplicationFullScreen,
        ToggleApplicationMenuBar,
    },
    document_workspace::{
        ActualSize, CloseDocument, ContinuousView, DocumentOpenBatchRequest, DocumentOpenOrigin,
        DocumentWorkspace, FitPage, FitWidth, NativeDocumentOpener, NativeDocumentResource,
        NativeDocumentSaver, NavigateNextPage, NavigatePreviousPage, NewFromTemplate,
        OpenDocumentRequest, OpenPdf, OpenedNativeDocument, RasterSurface, RotatePageLeft,
        RotatePageRight, Save, SaveAs, SaveDocumentAsTemplate, SaveDocumentRequest,
        SavedNativeDocument, SinglePageView, ThumbnailSurface, ZoomIn, ZoomOut,
    },
    native_application::{
        ApplicationMenuShellState, NativeApplicationMenuState, NativeDocumentIngress,
        build_in_window_application_menus, build_native_application_menus,
        build_native_application_menus_with_shell,
    },
    viewer::TileRequest,
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
    });

    assert_eq!(
        menu_names(&menus),
        [
            "GPUI Migration",
            "File",
            "Edit",
            "Document",
            "View",
            "Window",
        ]
    );
    let app = menu(&menus, "GPUI Migration");
    assert_action(item(app, "Quit GPUI Migration"), |action| {
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
            "Select All",
        ]
    );
    assert_action(item(edit, "Undo"), |action| action.as_any().is::<Undo>());
    assert_action(item(edit, "Redo"), |action| action.as_any().is::<Redo>());
    assert_action(item(edit, "Cut"), |action| action.as_any().is::<Cut>());
    assert_action(item(edit, "Copy"), |action| action.as_any().is::<Copy>());
    assert_action(item(edit, "Paste"), |action| action.as_any().is::<Paste>());
    assert_action(item(edit, "Delete"), |action| {
        action.as_any().is::<Delete>()
    });
    assert_action(item(edit, "Select All"), |action| {
        action.as_any().is::<SelectAll>()
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
        ]
    );
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
    assert!(item(document, "Previous Page").is_disabled());
    assert!(!item(document, "Next Page").is_disabled());
    assert!(!item(document, "Rotate Left").is_disabled());
    assert!(!item(document, "Rotate Right").is_disabled());

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
    for name in ["Previous Page", "Next Page", "Rotate Left", "Rotate Right"] {
        assert!(
            item(menu(&disabled, "Document"), name).is_disabled(),
            "{name} must be disabled"
        );
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
        },
    );

    assert_eq!(
        menu_names(&menus),
        ["GPUI Migration", "File", "Edit", "View"]
    );
    assert_eq!(
        menu_item_names(menu(&menus, "File")),
        [
            "New from Template…",
            "Open…",
            "<separator>",
            "Save",
            "Save As…",
            "Save Document as Template…",
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
            "Select All",
        ]
    );
    assert_eq!(
        menu_item_names(menu(&menus, "View")),
        [
            "Show Menu Bar in App Windows",
            "<separator>",
            "Make Interface Bigger",
            "Make Interface Smaller",
            "Reset Interface Size",
            "<separator>",
            "Toggle Full Screen",
        ]
    );
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
    assert_action(
        item(menu(&menus, "View"), "Make Interface Bigger"),
        |action| action.as_any().is::<MakeInterfaceBigger>(),
    );
    assert_action(
        item(menu(&menus, "View"), "Make Interface Smaller"),
        |action| action.as_any().is::<MakeInterfaceSmaller>(),
    );
    assert_action(
        item(menu(&menus, "View"), "Reset Interface Size"),
        |action| action.as_any().is::<ResetInterfaceSize>(),
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
            ..Default::default()
        },
        ApplicationMenuShellState {
            menu_bar_visible: false,
            menu_bar_visibility_supported: true,
        },
    );

    assert!(
        menu(&menus, "Edit")
            .items
            .iter()
            .any(|item| matches!(item, MenuItem::Action { name, .. } if name == "Delete"))
    );
    assert_eq!(
        item(menu(&menus, "View"), "Show Menu Bar in App Windows").is_checked(),
        false
    );
    assert_action(
        item(menu(&menus, "View"), "Show Menu Bar in App Windows"),
        |action| action.as_any().is::<ToggleApplicationMenuBar>(),
    );
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
    cx.update(gpui_component::init);
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
    cx.update(gpui_component::init);
    let backend = Arc::new(RecordingBackend::default());
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
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

    let dropped = PathBuf::from("/tmp/drop-fixture.pdf");
    for expected_sessions in [1, 2] {
        cx.simulate_event(FileDropEvent::Entered {
            position: point(px(20.), px(20.)),
            paths: ExternalPaths(
                [dropped.clone(), PathBuf::from("/tmp/notes.txt")]
                    .into_iter()
                    .collect(),
            ),
        });
        cx.simulate_event(FileDropEvent::Submit {
            position: point(px(20.), px(20.)),
        });
        cx.run_until_parked();
        assert_eq!(
            workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
            expected_sessions,
            "a separate drop of the same PDF must force a new document tab"
        );
    }

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
fn workspace_tab_hover_truncates_and_restores_without_moving_tabs(cx: &mut TestAppContext) {
    use butter_paper_gpui_migration::document_workspace::document_session_tab_id;
    cx.update(gpui_component::init);
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    let first = workspace.update(cx, |workspace, cx| workspace.open_path(PathBuf::from("/tmp/first-document.pdf"), cx));
    let second = workspace.update(cx, |workspace, cx| workspace.open_path(PathBuf::from("/tmp/second-document.pdf"), cx));
    cx.run_until_parked();
    let (_, cx) = cx.add_window_view(move |window, cx| Root::new(workspace, window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let outside = point(px(600.), px(500.));
    let strip = cx.debug_bounds(butter_paper_gpui_migration::document_workspace::DOCUMENT_SESSION_TABS_ID).unwrap();
    let first_bounds = cx.debug_bounds("document-1-session-tab").unwrap();
    let second_bounds = cx.debug_bounds("document-2-session-tab").unwrap();
    let viewport = cx.debug_bounds("document-workspace-native-session-tab-bar").unwrap();
    let gap = second_bounds.left() - first_bounds.right();
    assert_eq!(first_bounds.left() - strip.left(), gap, "leading inset must match tab gap");
    assert!(viewport.right() > second_bounds.right(), "unused tab space stays before pinned actions");
    assert_eq!(strip.right() - cx.debug_bounds("document-tab-template-picker").unwrap().right(), gap, "actions stay pinned to trailing inset even with two tabs");
    assert!(cx.debug_bounds("document-tabs-overflow-lane").is_none(), "no overflow lane when tabs fit");
    assert_eq!(cx.debug_bounds("document-tab-open").unwrap().left() - viewport.right(), gap, "action group inset must match tab gap");
    cx.simulate_mouse_move(outside, None, gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    for appearance in [gpui::WindowAppearance::Light, gpui::WindowAppearance::Dark] {
        cx.update(|window, cx| gpui_component::Theme::change(appearance, Some(window), cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        for id in [first, second] {
            // The selector API requires static strings; these eight bounded
            // fixture selectors are retained only for the test process.
            let tab_id: &'static str = Box::leak(document_session_tab_id(id).into_boxed_str());
            let label_id: &'static str = Box::leak(format!("{tab_id}-visible-label").into_boxed_str());
            let tab = cx.debug_bounds(tab_id).expect("tab rendered");
            let close_id: &'static str = Box::leak(butter_paper_gpui_migration::document_workspace::document_session_close_id(id).into_boxed_str());
            let close = cx.debug_bounds(close_id).expect("close rendered");
            assert_eq!(close.size.width, close.size.height, "circular close needs a square frame");
            assert_eq!(close.size.width, px(24.), "stock small close target");
            assert_eq!(tab.right() - close.right(), (tab.size.height - close.size.height) / 2., "close has equal top/bottom/trailing inset");
            assert!((close.center().y - tab.center().y).abs() <= px(0.5), "close must share the tab's vertical centre");
            assert!((close.center().x - (tab.right() - tab.size.height / 2.)).abs() <= px(0.5), "close must be concentric with the tab end cap: tab={tab:?}, close={close:?}");
            let label = cx.debug_bounds(label_id).expect("label rendered");
            cx.simulate_mouse_move(tab.center(), None, gpui::Modifiers::default());
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(cx.debug_bounds(tab_id).unwrap(), tab, "hover must not resize or move tab");
            assert!(cx.debug_bounds(label_id).unwrap().size.width < label.size.width, "hover must truncate visible label");
            cx.simulate_mouse_move(outside, None, gpui::Modifiers::default());
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(cx.debug_bounds(label_id).unwrap(), label, "exit must restore full label");
        }
    }
}

#[gpui::test]
fn empty_workspace_retains_tab_bar_boundaries_and_pinned_actions(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    let (_, cx) = cx.add_window_view(move |window, cx| Root::new(workspace, window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let strip = cx.debug_bounds(butter_paper_gpui_migration::document_workspace::DOCUMENT_SESSION_TABS_ID).unwrap();
    let divider = cx.debug_bounds("document-workspace-native-session-tab-bar").unwrap();
    let open = cx.debug_bounds("document-tab-open").unwrap();
    let picker = cx.debug_bounds("document-tab-template-picker").unwrap();
    assert_eq!(strip.size.height, px(49.), "empty row retains 32px tab frame, 16px padding and bottom border");
    assert_eq!(divider.top(), strip.top());
    assert_eq!(divider.bottom(), strip.bottom() - px(1.));
    assert_eq!(open.left() - divider.right(), px(12.));
    assert_eq!(strip.right() - picker.right(), px(12.));
    assert!(cx.debug_bounds("document-tabs-overflow-lane").is_none());
}

#[gpui::test]
fn workspace_tabs_reorder_with_one_pointer_move_and_cancel_outside(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    let ids = (0..3).map(|ix| workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from(format!("/tmp/drag-sheet-{ix}.pdf")), cx)
    })).collect::<Vec<_>>();
    cx.run_until_parked();
    let observed = workspace.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| Root::new(workspace, window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let active = observed.read_with(cx, |workspace, _| workspace.active_document_id());
    let source = cx.debug_bounds("document-1-session-tab").unwrap().center();
    let target = cx.debug_bounds("document-3-session-tab").unwrap().center();
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_move(target, Some(gpui::MouseButton::Left), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(observed.read_with(cx, |workspace, cx| workspace.session_order(cx)), vec![ids[1], ids[2], ids[0]]);
    assert_eq!(observed.read_with(cx, |workspace, _| workspace.active_document_id()), active, "reordering must preserve active document identity");
    let source = cx.debug_bounds("document-1-session-tab").unwrap().center();
    let target = cx.debug_bounds("document-2-session-tab").unwrap().center();
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_move(target, Some(gpui::MouseButton::Left), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let outside = point(target.x, target.y + px(150.));
    cx.simulate_mouse_up(outside, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(observed.read_with(cx, |workspace, cx| workspace.session_order(cx)), vec![ids[1], ids[2], ids[0]], "release outside must cancel, even without a final move event");
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_move(target, Some(gpui::MouseButton::Left), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(observed.read_with(cx, |workspace, cx| workspace.session_order(cx)), ids, "reverse drag restores original order");
}

#[gpui::test]
fn workspace_integration_crowded_tabs_keep_actions_pinned_for_both_wheel_axes(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    for ix in 0..12 {
        workspace.update(cx, |workspace, cx| workspace.open_path(PathBuf::from(format!("/tmp/integration-long-drawing-sheet-{ix}.pdf")), cx));
    }
    cx.run_until_parked();
    let (_, cx) = cx.add_window_view(move |window, cx| Root::new(workspace, window, cx));
    cx.simulate_resize(gpui::size(px(900.), px(800.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let ids = ["document-tab-open", "document-tab-new-pdf", "document-tab-template-picker", "document-tabs-overflow"];
    let before = ids.map(|id| cx.debug_bounds(id).unwrap());
    assert!(before[2].right() <= px(900.));
    let tabs = cx.debug_bounds(butter_paper_gpui_migration::document_workspace::DOCUMENT_SESSION_TABS_ID).unwrap();
    let cut = cx.debug_bounds("document-workspace-native-session-tab-bar").unwrap();
    assert!(cx.debug_bounds("document-tabs-actions-separator").is_none());
    let overflow = before[3];
    assert_eq!(overflow.size, before[0].size, "overflow must use the same stock button size as Open");
    assert_eq!(overflow.top(), before[0].top());
    let lane = cx.debug_bounds("document-tabs-overflow-lane").unwrap();
    assert_eq!(lane.right() - px(1.) - overflow.right(), overflow.left() - cut.right(), "overflow has equal clearance to both dividers");
    assert_eq!(before[0].left() - lane.right(), overflow.left() - cut.right(), "equal spacing outside lane");
    for boundary in [cut, lane] {
        assert_eq!(boundary.top(), tabs.top(), "tab boundaries must start at the strip top");
        assert_eq!(boundary.bottom(), tabs.bottom() - px(1.), "tab boundaries must meet the bottom border");
    }
    assert!(cut.right() < overflow.left());
    let target = point(tabs.left() + px(60.), tabs.center().y);
    let mut positions = Vec::new();
    for delta in [point(px(0.), px(2000.)), point(px(0.), px(-160.)), point(px(160.), px(0.)), point(px(-160.), px(0.))] {
        cx.simulate_event(gpui::ScrollWheelEvent { position: target, delta: gpui::ScrollDelta::Pixels(delta), modifiers: gpui::Modifiers::default(), touch_phase: gpui::TouchPhase::Moved });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        positions.push(cx.debug_bounds("document-1-session-tab").unwrap().left());
        for (id, expected) in ids.into_iter().zip(before) {
            assert_eq!(cx.debug_bounds(id).unwrap(), expected, "wheel scrolling moved pinned {id}");
        }
    }
    assert!(positions[1] < positions[0], "vertical wheel must actually scroll tabs");
    assert!(positions[2] > positions[1], "horizontal wheel must scroll back");
    assert!(positions[3] < positions[2], "opposite horizontal wheel must scroll forward");
}

#[gpui::test]
fn workspace_integration_sidebar_matrix_preserves_canvas_boundaries(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let backend = Arc::new(RecordingBackend::default());
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| workspace.open_path(PathBuf::from("/tmp/integration-fixture.pdf"), cx));
    cx.run_until_parked();
    let observed = workspace.clone();
    let close = cx.new(move |_| ApplicationCloseWorkspace::new(workspace, backend));
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    let id = observed.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    let revision = observed.read_with(cx, |workspace, cx| workspace.annotation_snapshot(id, cx).unwrap().revision);
    for width in [1200., 900., 1500.] {
        cx.simulate_resize(gpui::size(px(width), px(800.)));
        // Closed/closed -> left -> both -> right -> closed/closed.
        for toggle in [None, Some("document-left-rail-pages"), Some("document-workspace-rail-actions"), Some("document-left-rail-pages"), Some("document-workspace-rail-actions")] {
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
            let canvas = cx.debug_bounds(butter_paper_gpui_migration::document_workspace::DOCUMENT_PAGE_ID).unwrap();
            let leading = cx.debug_bounds("document-left-sidebar-header").map_or(left.right(), |b| b.right());
            let trailing = cx.debug_bounds("document-workspace-properties-sidebar").map_or(right.left(), |b| b.left());
            assert_eq!(toolbar.left(), leading, "toolbar leading boundary at width {width}");
            assert_eq!(toolbar.right(), trailing, "toolbar trailing boundary at width {width}");
            assert_eq!(toolbar.top(), left.top());
            assert_eq!(toolbar.top(), right.top());
            assert_eq!(canvas.left(), toolbar.left());
            assert_eq!(canvas.right(), toolbar.right());
            assert_eq!(canvas.top(), toolbar.bottom());
            assert_eq!(right.right(), px(width));
            let first = cx.debug_bounds("viewer-zoom-controls").unwrap();
            let last = cx.debug_bounds("single-page-view-split").unwrap();
            if last.right() - first.left() <= toolbar.size.width {
                let centre = (first.left() + last.right()) / 2.;
                assert!((centre - toolbar.center().x).abs() <= px(0.5), "toolbar controls not centred at width {width}");
            } else {
                cx.simulate_event(gpui::ScrollWheelEvent {
                    position: toolbar.center(),
                    delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-1000.))),
                    modifiers: gpui::Modifiers::default(),
                    touch_phase: gpui::TouchPhase::Moved,
                });
                cx.update(|window, cx| window.draw(cx).clear(cx));
                let revealed = cx.debug_bounds("single-page-view-split").unwrap();
                assert!(revealed.right() <= toolbar.right(), "vertical wheel must reveal final toolbar dropdown at width {width}");
                assert!(revealed.left() >= toolbar.left());
            }
            assert_eq!(observed.read_with(cx, |workspace, _| workspace.active_document_id()), Some(id));
            assert_eq!(observed.read_with(cx, |workspace, cx| workspace.annotation_snapshot(id, cx).unwrap().revision), revision);
        }
    }
}

#[gpui::test]
fn workspace_sidebar_dividers_resize_independently_and_double_click_restores_defaults(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let backend = Arc::new(RecordingBackend::default());
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| workspace.open_path(PathBuf::from("/tmp/sidebar-reset.pdf"), cx));
    cx.run_until_parked();
    let observed = workspace.clone();
    let close = cx.new(move |_| ApplicationCloseWorkspace::new(workspace, backend));
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    cx.simulate_resize(gpui::size(px(1500.), px(800.)));
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..5 {
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }
    };
    settle(cx);
    let id = observed.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    let revision = observed.read_with(cx, |workspace, cx| workspace.annotation_snapshot(id, cx).unwrap().revision);
    for toggle in ["document-left-rail-pages", "document-workspace-rail-actions"] {
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
        let start = point(if left { panel.right() } else { panel.left() }, panel.top() + px(100.));
        let end = point(if left { panel.left() + px(width) } else { panel.right() - px(width) }, start.y);
        cx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_move(start + point(px(if left { 8. } else { -8. }), px(0.)), Some(gpui::MouseButton::Left), gpui::Modifiers::default());
        settle(cx);
        cx.simulate_mouse_move(end, Some(gpui::MouseButton::Left), gpui::Modifiers::default());
        settle(cx);
        cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
        settle(cx);
        assert_eq!(cx.debug_bounds(selector).unwrap().size.width, px(width), "drag {selector}");
        assert_eq!(cx.debug_bounds(other).unwrap().size.width, other_width, "drag must not change {other}");
    }
    // Window resizing and hiding/reopening a neighbour must preserve preferences.
    for width in [1000., 1700., 1500.] {
        cx.simulate_resize(gpui::size(px(width), px(800.)));
        settle(cx);
        assert_eq!(cx.debug_bounds(left_id).unwrap().size.width, px(360.));
        assert_eq!(cx.debug_bounds(right_id).unwrap().size.width, px(400.));
    }
    // The exact fit boundary has no extra rounding margin or oscillation.
    for (width, visible) in [(991., false), (992., true), (991., false), (1000., true)] {
        cx.simulate_resize(gpui::size(px(width), px(800.)));
        settle(cx);
        assert_eq!(cx.debug_bounds(left_id).is_some(), visible, "thumbnail fit threshold {width}");
    }
    // A temporarily hidden sidebar returns at its custom width, not the default.
    cx.simulate_resize(gpui::size(px(900.), px(800.)));
    settle(cx);
    assert!(cx.debug_bounds(left_id).is_none());
    let toggle = cx.debug_bounds("document-left-rail-pages").unwrap().center();
    cx.simulate_click(toggle, gpui::Modifiers::default()); // disabled while it cannot fit
    cx.simulate_resize(gpui::size(px(1500.), px(800.)));
    settle(cx);
    assert_eq!(cx.debug_bounds(left_id).unwrap().size.width, px(360.));
    assert_eq!(cx.debug_bounds(right_id).unwrap().size.width, px(400.));
    for toggle in ["document-left-rail-pages", "document-left-rail-pages", "document-workspace-rail-actions", "document-workspace-rail-actions"] {
        let position = cx.debug_bounds(toggle).unwrap().center();
        cx.simulate_click(position, gpui::Modifiers::default());
        settle(cx);
    }
    assert_eq!(cx.debug_bounds(left_id).unwrap().size.width, px(360.));
    assert_eq!(cx.debug_bounds(right_id).unwrap().size.width, px(400.));
    // Double-clicking ordinary sidebar content must not reset its width.
    let header = cx.debug_bounds("document-left-sidebar-header").unwrap().center();
    cx.simulate_event(gpui::MouseDownEvent { position: header, button: gpui::MouseButton::Left, click_count: 2, ..Default::default() });
    cx.simulate_event(gpui::MouseUpEvent { position: header, button: gpui::MouseButton::Left, click_count: 2, ..Default::default() });
    settle(cx);
    assert_eq!(cx.debug_bounds(left_id).unwrap().size.width, px(360.));
    for (selector, other) in [(left_id, right_id), (right_id, left_id)] {
        let panel = cx.debug_bounds(selector).unwrap();
        let other_width = cx.debug_bounds(other).unwrap().size.width;
        let position = point(if selector == left_id { panel.right() } else { panel.left() }, panel.top() + px(100.));
        cx.simulate_event(gpui::MouseDownEvent { position, button: gpui::MouseButton::Left, click_count: 2, ..Default::default() });
        cx.simulate_event(gpui::MouseUpEvent { position, button: gpui::MouseButton::Left, click_count: 2, ..Default::default() });
        settle(cx);
        assert_eq!(cx.debug_bounds(selector).unwrap().size.width, px(300.), "reset {selector}");
        assert_eq!(cx.debug_bounds(other).unwrap().size.width, other_width, "reset must not change {other}");
    }
    cx.simulate_resize(gpui::size(px(1800.), px(1000.)));
    for level in [-3, 3, 0] {
        cx.update(|window, cx| {
            butter_paper_gpui_migration::application_shell::apply_application_ui_zoom(level, px(16.), window, cx);
            observed.update(cx, |workspace, cx| workspace.reset_right_rail_pixel_sizes(cx));
        });
        settle(cx);
        let rem = butter_paper_gpui_migration::application_shell::application_ui_zoom_font_size(px(16.), level);
        for selector in [left_id, right_id] {
            assert!((cx.debug_bounds(selector).unwrap().size.width - rem * 18.75).abs() < px(1.), "default follows interface scale {level}: {selector}");
        }
        let select = cx.debug_bounds("document-workspace-select-tool").unwrap();
        let hand = cx.debug_bounds("document-workspace-pan-tool").unwrap();
        assert_eq!(select.top(), hand.top(), "two-column rail must not wrap at interface zoom {level}");
        let toolbar = cx.debug_bounds("viewer-toolbar").unwrap();
        assert_eq!(toolbar.left(), cx.debug_bounds(left_id).unwrap().right());
        assert_eq!(toolbar.right(), cx.debug_bounds(right_id).unwrap().left());
        if level == 3 {
            cx.simulate_resize(gpui::size(px(900.), px(1000.)));
            settle(cx);
            assert!(cx.debug_bounds(left_id).is_none(), "narrow/high-zoom fallback collapses thumbnails");
            let toolbar = cx.debug_bounds("viewer-toolbar").unwrap();
            assert_eq!(toolbar.left(), cx.debug_bounds("document-left-rail").unwrap().right());
            assert_eq!(toolbar.right(), cx.debug_bounds(right_id).unwrap().left());
            assert!(toolbar.size.width >= px(100.));
            assert!(cx.debug_bounds("document-workspace-right-rail").unwrap().right() <= px(900.));
            let properties_toggle = cx.debug_bounds("document-workspace-rail-actions").unwrap().center();
            cx.simulate_click(properties_toggle, gpui::Modifiers::default());
            settle(cx);
            assert!(cx.debug_bounds(right_id).is_none(), "properties toggle closes the panel at high zoom");
            assert!(cx.debug_bounds(left_id).is_some(), "closing properties makes room for thumbnails");
            cx.simulate_click(properties_toggle, gpui::Modifiers::default());
            settle(cx);
            assert!(cx.debug_bounds(left_id).is_none());
            cx.simulate_resize(gpui::size(px(1800.), px(1000.)));
            settle(cx);
            assert!((cx.debug_bounds(left_id).unwrap().size.width - rem * 18.75).abs() < px(1.));
        }
    }
    // Explicitly closing thumbnails overrides automatic restoration.
    let toggle = cx.debug_bounds("document-left-rail-pages").unwrap().center();
    cx.simulate_click(toggle, gpui::Modifiers::default());
    for width in [900., 1800.] {
        cx.simulate_resize(gpui::size(px(width), px(1000.)));
        settle(cx);
        assert!(cx.debug_bounds(left_id).is_none());
    }
    assert_eq!(observed.read_with(cx, |workspace, _| workspace.active_document_id()), Some(id));
    assert_eq!(observed.read_with(cx, |workspace, cx| workspace.annotation_snapshot(id, cx).unwrap().revision), revision);
}

#[gpui::test]
fn workspace_left_rail_toggles_existing_panel_without_moving_rail(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let backend = Arc::new(RecordingBackend::default());
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| workspace.open_path(PathBuf::from("/tmp/rail-fixture.pdf"), cx));
    cx.run_until_parked();
    let observed = workspace.clone();
    let close = cx.new(move |_| ApplicationCloseWorkspace::new(workspace, backend));
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let rail = cx.debug_bounds("document-left-rail").expect("rail rendered");
    let toggle = cx.debug_bounds("document-left-rail-pages").expect("toggle rendered");
    assert_eq!(rail.size.width, px(48.));
    assert_eq!(toggle.size.width, px(32.));
    assert_eq!(toggle.size.height, px(32.));
    assert!(toggle.origin.y >= rail.origin.y + px(8.));
    assert!(cx.debug_bounds("document-thumbnail-strip").is_none());
    cx.simulate_click(toggle.center(), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("document-thumbnail-strip").is_some());
    let header = cx.debug_bounds("document-left-sidebar-header").expect("sidebar header");
    let toolbar = cx.debug_bounds("viewer-toolbar").expect("viewer toolbar");
    assert_eq!(header.origin.y, rail.origin.y, "sidebar starts directly below tabs");
    assert_eq!(header.origin.y, toolbar.origin.y, "sidebar header sits beside viewer controls");
    assert_eq!(header.size.height, toolbar.size.height, "header and viewer band share a height");
    assert!(toolbar.origin.x >= header.origin.x + header.size.width, "viewer is in the canvas column");
    let row = cx.debug_bounds("document-1-thumbnail-0").expect("thumbnail row");
    let preview = cx.debug_bounds("thumbnail-preview-0").expect("thumbnail preview");
    assert!(preview.origin.y + preview.size.height <= row.origin.y + row.size.height, "preview must stay inside its card");
    assert!(row.origin.x > header.origin.x, "cards have a leading inset");
    let scale = cx.debug_bounds("thumbnail-0-scale").expect("row scale button");
    cx.simulate_click(scale.center(), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("page-scale-dialog").is_some(), "nested row action must receive pointer input");
    // Stock dialogs animate for 250ms; measure/click the settled surface.
    cx.executor().advance_clock(std::time::Duration::from_millis(250));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let apply = cx.debug_bounds("page-scale-apply").unwrap();
    cx.simulate_click(apply.center(), gpui::Modifiers::default());
    cx.executor().advance_clock(std::time::Duration::from_millis(250));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("page-scale-dialog").is_none());
    let badge = cx.debug_bounds("thumbnail-0-scale-badge").expect("applied scale appears in sidebar");
    assert!(badge.right() <= scale.left(), "badge must not overlap actions");
    assert_eq!(cx.debug_bounds("thumbnail-0-scale").unwrap(), scale, "badge must not move actions");
    let rotation = observed.update(cx, |workspace, cx| {
        workspace.begin_page_rotation(workspace.active_document_id().unwrap(), 0,
            butter_paper_gpui_migration::annotation_model::PageRotationDirection::Right, cx).unwrap()
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let revision = observed.read_with(cx, |workspace, cx| workspace.annotation_snapshot(rotation.document_id, cx).unwrap().revision);
    for selector in ["thumbnail-0-scale", "thumbnail-0-rotate-left", "thumbnail-0-rotate-right"] {
        let bounds = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(bounds.center(), gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("page-scale-dialog").is_none(), "busy scale trigger is inert");
    }
    assert_eq!(observed.read_with(cx, |workspace, cx| workspace.annotation_snapshot(rotation.document_id, cx).unwrap().revision), revision, "busy rotation buttons cannot submit another mutation");
    observed.update(cx, |workspace, cx| { workspace.apply_page_rotation_result(&rotation, Err("Injected rotation failure".into()), cx); });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_click(scale.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("page-scale-dialog").is_some(), "row actions recover after failure");
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let save = observed.update(cx, |workspace, cx| workspace.begin_save(rotation.document_id, cx).unwrap());
    let save_revision = observed.read_with(cx, |workspace, cx| workspace.annotation_snapshot(rotation.document_id, cx).unwrap().revision);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    for selector in ["thumbnail-0-scale", "thumbnail-0-rotate-left", "thumbnail-0-rotate-right"] {
        let bounds = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(bounds.center(), gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("page-scale-dialog").is_none(), "save blocks mutation controls");
    }
    assert_eq!(observed.read_with(cx, |workspace, cx| workspace.annotation_snapshot(rotation.document_id, cx).unwrap().revision), save_revision);
    observed.update(cx, |workspace, cx| { workspace.apply_save_result(&save, Err("Injected save failure".into()), cx); });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(cx.debug_bounds("document-left-rail").unwrap(), rail);
    cx.simulate_click(toggle.center(), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("document-thumbnail-strip").is_none());
    assert_eq!(cx.debug_bounds("document-left-rail-pages").unwrap(), toggle);
}

#[gpui::test]
fn workspace_right_rail_replaces_horizontal_tools_and_keeps_canvas_band_between_rails(cx: &mut TestAppContext) {
    use butter_paper_gpui_migration::annotation_adapter::AnnotationTool;
    cx.update(gpui_component::init);
    let backend = Arc::new(RecordingBackend::default());
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| workspace.open_path(PathBuf::from("/tmp/right-rail-fixture.pdf"), cx));
    cx.run_until_parked();
    let observed = workspace.clone();
    let close = cx.new(move |_| ApplicationCloseWorkspace::new(workspace, backend));
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let rail = cx.debug_bounds("document-workspace-right-rail").expect("right rail");
    let left = cx.debug_bounds("document-left-rail").unwrap();
    let toolbar = cx.debug_bounds("viewer-toolbar").unwrap();
    assert_eq!(rail.size.width, px(84.));
    assert_eq!(rail.top(), left.top(), "both rails meet the tab band");
    assert_eq!(rail.top(), toolbar.top(), "viewer toolbar must sit beside the rail");
    assert_eq!(toolbar.right(), rail.left(), "viewer band ends at the rail boundary");
    assert_eq!(rail.bottom(), left.bottom());
    assert!(cx.debug_bounds("document-workspace-toolbar-scroll").is_none(), "legacy horizontal strip is gone at rest");
    let select = cx.debug_bounds("document-workspace-select-tool").unwrap();
    let hand = cx.debug_bounds("document-workspace-pan-tool").unwrap();
    assert_eq!(select.size, gpui::size(px(32.), px(32.)));
    assert_eq!(hand.top(), select.top(), "General tools share a two-column row");
    assert_eq!(hand.left() - select.right(), px(8.));
    assert!(select.left() >= rail.left() + px(5.));
    assert!(hand.right() <= rail.right() - px(5.));
    let document_id = observed.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    let revision = observed.read_with(cx, |workspace, cx| workspace.annotation_snapshot(document_id, cx).unwrap().revision);
    cx.simulate_click(hand.center(), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let viewport = cx.debug_bounds("document-native-viewport").unwrap();
    let start = viewport.center();
    let end = start - point(px(20.), px(80.));
    let before = observed.read_with(cx, |workspace, cx| workspace.document_view_state(document_id, cx).unwrap().scroll());
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_move(end, Some(gpui::MouseButton::Left), gpui::Modifiers::default());
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let after = observed.read_with(cx, |workspace, cx| workspace.document_view_state(document_id, cx).unwrap().scroll());
    assert!(after.1 > before.1, "Hand drag scrolls the canvas");
    assert_eq!(observed.read_with(cx, |workspace, cx| workspace.annotation_snapshot(document_id, cx).unwrap().revision), revision, "Hand must not create or move annotations");
    for (id, tool) in [("document-workspace-text-box-tool", AnnotationTool::TextBox), ("document-workspace-arrow-tool", AnnotationTool::Arrow), ("document-workspace-rectangle-tool", AnnotationTool::Rectangle), ("document-workspace-select-tool", AnnotationTool::Select)] {
        let button = cx.debug_bounds(id).expect("tool visible");
        cx.simulate_click(button.center(), gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(observed.read_with(cx, |workspace, cx| workspace.annotation_tool(workspace.active_document_id().unwrap(), cx)), Some(tool));
    }
    let pages = cx.debug_bounds("document-left-rail-pages").unwrap();
    cx.simulate_click(pages.center(), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(cx.debug_bounds("document-workspace-right-rail").unwrap(), rail, "opening thumbnails cannot displace the right rail");
    let actions = cx.debug_bounds("document-workspace-rail-actions").unwrap();
    cx.simulate_click(actions.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("document-workspace-properties-sidebar").is_some(), "the canonical properties sidebar opens beside the rail");
    assert!(cx.debug_bounds("document-workspace-toolbar-scroll").is_none(), "the interim document-actions stack is not a second properties pane");
    assert_eq!(cx.debug_bounds("document-workspace-right-rail").unwrap(), rail);
    cx.simulate_click(actions.center(), gpui::Modifiers::default());
    cx.simulate_resize(gpui::size(px(900.), px(440.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let short_rail = cx.debug_bounds("document-workspace-right-rail").unwrap();
    let pinned = cx.debug_bounds("document-workspace-rail-actions").unwrap();
    assert!(pinned.top() >= short_rail.top() && pinned.bottom() <= short_rail.top() + px(48.));
    assert_eq!(short_rail.right(), px(900.), "rail stays on the window edge when resized");
    let scroller = cx.debug_bounds("document-workspace-right-rail-scroll").unwrap();
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
    let area = cx.debug_bounds(butter_paper_gpui_migration::document_workspace::DOCUMENT_AREA_TOOL_ID).unwrap_or_else(|| panic!("Area not revealed: scroll offset={offset:?}, scroller={scroller:?}"));
    assert!(area.top() >= scroller.top() && area.bottom() <= scroller.bottom(), "scrolling reveals the final Measure tool");
    assert_eq!(cx.debug_bounds("document-workspace-rail-actions").unwrap(), pinned, "scrolling cannot move pinned controls");
}

#[gpui::test]
fn right_rail_resize_snaps_columns_and_double_click_toggles_properties(cx: &mut TestAppContext) {
    use butter_paper_gpui_migration::annotation_adapter::AnnotationTool;
    cx.update(gpui_component::init);
    let backend = Arc::new(RecordingBackend::default());
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| workspace.open_path(PathBuf::from("/tmp/rail-interactions.pdf"), cx));
    cx.run_until_parked();
    let observed = workspace.clone();
    let close = cx.new(move |_| ApplicationCloseWorkspace::new(workspace, backend));
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| ApplicationCloseShell::new_for_native_window(close, window, cx));
        Root::new(shell, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let initial = cx.debug_bounds("document-workspace-right-rail").unwrap();
    let cloud_plus = cx.debug_bounds(butter_paper_gpui_migration::document_workspace::DOCUMENT_CLOUD_PLUS_TOOL_ID).unwrap();
    let callout = cx.debug_bounds(butter_paper_gpui_migration::document_workspace::DOCUMENT_CALLOUT_TOOL_ID).unwrap();
    assert_eq!(cloud_plus.size, gpui::size(px(32.), px(32.)), "overscan must not enlarge the button");
    assert_eq!(cloud_plus.top(), callout.top(), "composite icon cannot wrap its two-column row");
    // Drag the stock divider, not an application setter. Include a threshold
    // move and a frame before the destination so GPUI starts its drag session.
    for (requested_width, expected_width) in [(170., 164.), (20., 44.), (600., 324.), (84., 84.)] {
        let rail = cx.debug_bounds("document-workspace-right-rail").unwrap();
        let start = point(rail.left(), rail.top() + px(100.));
        let end = point(rail.right() - px(requested_width), start.y);
        cx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_move(start - point(px(8.), px(0.)), Some(gpui::MouseButton::Left), gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_move(end, Some(gpui::MouseButton::Left), gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let resized = cx.debug_bounds("document-workspace-right-rail").unwrap();
        assert_eq!(resized.size.width, px(expected_width), "requested {requested_width}");
        assert_eq!(resized.right(), initial.right(), "right edge stays pinned");
        assert_eq!(cx.debug_bounds("viewer-toolbar").unwrap().right(), resized.left());
        let select = cx.debug_bounds("document-workspace-select-tool").unwrap();
        let hand = cx.debug_bounds("document-workspace-pan-tool").unwrap();
        let actions = cx.debug_bounds("document-workspace-rail-actions").unwrap();
        let scroller = cx.debug_bounds("document-workspace-right-rail-scroll").unwrap();
        assert!(actions.bottom() <= scroller.top(), "pinned controls cannot clip into the scroll body");
        if expected_width == 44. {
            assert_eq!(select.left(), hand.left());
            assert_eq!(hand.top() - select.bottom(), px(8.));
            assert_eq!(scroller.top() - resized.top(), px(88.));
        } else {
            assert_eq!(select.top(), hand.top());
            assert_eq!((select.left() + hand.right()) / 2., resized.center().x + px(0.5));
        }
    }
    let document_id = observed.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    for (selector, tool) in [("document-workspace-rectangle-tool", AnnotationTool::Rectangle), ("document-workspace-highlight-tool", AnnotationTool::Highlight)] {
    for expected_open in [true, false] {
        let button = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(button.center(), gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(cx.debug_bounds("document-workspace-properties-sidebar").is_some(), !expected_open, "single click only selects");
        cx.simulate_event(gpui::MouseDownEvent { position: button.center(), button: gpui::MouseButton::Left, click_count: 2, ..Default::default() });
        cx.simulate_event(gpui::MouseUpEvent { position: button.center(), button: gpui::MouseButton::Left, click_count: 2, ..Default::default() });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(cx.debug_bounds("document-workspace-properties-sidebar").is_some(), expected_open);
        assert_eq!(observed.read_with(cx, |workspace, cx| workspace.annotation_tool(document_id, cx)), Some(tool));
        assert_eq!(cx.debug_bounds("document-workspace-right-rail").unwrap(), initial);
        if tool == AnnotationTool::Highlight && expected_open {
            let sidebar = cx.debug_bounds("document-workspace-properties-sidebar").unwrap();
            assert_eq!(sidebar.top(), initial.top(), "properties begins below tabs beside the canvas toolbar");
            assert_eq!(sidebar.bottom(), initial.bottom());
            assert_eq!(sidebar.right(), initial.left());
            for (requested_width, expected_width) in [(200., 240.), (500., 420.), (300., 300.)] {
                let sidebar = cx.debug_bounds("document-workspace-properties-sidebar").unwrap();
                let start = point(sidebar.left(), sidebar.top() + px(100.));
                let end = point(sidebar.right() - px(requested_width), start.y);
                cx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
                cx.simulate_mouse_move(start - point(px(8.), px(0.)), Some(gpui::MouseButton::Left), gpui::Modifiers::default());
                cx.update(|window, cx| window.draw(cx).clear(cx));
                cx.simulate_mouse_move(end, Some(gpui::MouseButton::Left), gpui::Modifiers::default());
                cx.update(|window, cx| window.draw(cx).clear(cx));
                cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
                cx.run_until_parked();
                cx.update(|window, cx| window.draw(cx).clear(cx));
                let resized = cx.debug_bounds("document-workspace-properties-sidebar").unwrap();
                assert_eq!(resized.size.width, px(expected_width), "properties clamp {requested_width}");
                assert_eq!(resized.right(), initial.left(), "properties resize keeps rail pinned");
                assert_eq!(cx.debug_bounds("highlight-defaults-panel-opacity-input").unwrap().size.width, px(80.));
            }
            observed.update(cx, |workspace, cx| workspace.set_highlight_defaults(document_id, "#00ff00", 18., 0.5, cx).unwrap());
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let before = observed.read_with(cx, |workspace, cx| workspace.highlight_defaults(document_id, cx).unwrap());
            let accordion = cx.debug_bounds("highlight-defaults-panel-accordion").unwrap();
            cx.simulate_click(gpui::point(accordion.center().x, accordion.bottom() - px(16.)), gpui::Modifiers::default());
            cx.executor().advance_clock(std::time::Duration::from_millis(250));
            cx.update(|window, cx| window.draw(cx).clear(cx));
            for confirm in [false, true] {
                let reset = cx.debug_bounds("highlight-defaults-panel-reset-button").unwrap();
                cx.simulate_click(reset.center(), gpui::Modifiers::default());
                cx.executor().advance_clock(std::time::Duration::from_millis(250));
                cx.update(|window, cx| window.draw(cx).clear(cx));
                let action = cx.debug_bounds(if confirm { "highlight-defaults-panel-reset-confirm" } else { "highlight-defaults-panel-reset-cancel" }).unwrap();
                cx.simulate_click(action.center(), gpui::Modifiers::default());
                cx.update(|window, cx| window.draw(cx).clear(cx));
                let after = observed.read_with(cx, |workspace, cx| workspace.highlight_defaults(document_id, cx).unwrap());
                if confirm {
                    assert_eq!(after.color, "#ffff00");
                    assert_eq!(after.width_pt, 12.);
                    assert_eq!(after.opacity, 1.);
                } else { assert_eq!(after, before, "cancelling reset preserves defaults"); }
            }
        }
    }
    }
    for level in [2, -2, 0] {
        cx.update(|window, cx| {
            butter_paper_gpui_migration::application_shell::apply_application_ui_zoom(level, px(16.), window, cx);
            observed.update(cx, |workspace, cx| workspace.reset_right_rail_pixel_sizes(cx));
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let rem = butter_paper_gpui_migration::application_shell::application_ui_zoom_font_size(px(16.), level);
        let rail = cx.debug_bounds("document-workspace-right-rail").unwrap();
        let select = cx.debug_bounds("document-workspace-select-tool").unwrap();
        let hand = cx.debug_bounds("document-workspace-pan-tool").unwrap();
        assert!((rail.size.width - rem * 5.25).abs() < px(1.), "zoom preserves the two-column width");
        assert_eq!(select.top(), hand.top());
        assert!(select.left() > rail.left() && hand.right() < rail.right(), "zoomed tools fit their rail");
    }
    observed.update(cx, |workspace, cx| { workspace.begin_save(document_id, cx).unwrap(); });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let actions = cx.debug_bounds("document-workspace-rail-actions").unwrap();
    cx.simulate_click(actions.center(), gpui::Modifiers::default());
    let rectangle = cx.debug_bounds("document-workspace-rectangle-tool").unwrap();
    cx.simulate_event(gpui::MouseDownEvent { position: rectangle.center(), click_count: 2, ..Default::default() });
    cx.simulate_event(gpui::MouseUpEvent { position: rectangle.center(), click_count: 2, ..Default::default() });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("document-workspace-toolbar-scroll").is_none(), "saving disables the properties toggle and tool double-click");
}

#[gpui::test]
fn highlight_defaults_panel_events_are_document_scoped_and_disabled_safe(cx: &mut TestAppContext) {
    use butter_paper_gpui_migration::annotation_adapter::AnnotationTool;
    use butter_paper_gpui_migration::highlight_defaults_panel::HighlightDefaultsEvent;
    use butter_paper_gpui_migration::document_workspace::PenAnnotationDefaults;
    cx.update(gpui_component::init);
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    workspace.update(cx, |workspace, cx| workspace.open_path(PathBuf::from("/tmp/highlight-defaults-a.pdf"), cx));
    cx.run_until_parked();
    let first = workspace.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    let defaults = PenAnnotationDefaults { color: "#00ff00".into(), width_pt: 18., opacity: 0.5 };
    let event = HighlightDefaultsEvent::Change { document_id: first, defaults: defaults.clone() };
    workspace.update(cx, |workspace, cx| {
        assert!(!workspace.apply_highlight_defaults_event(&event, cx).unwrap(), "inactive tool cannot receive defaults edits");
        workspace.set_annotation_tool(first, AnnotationTool::Highlight, cx).unwrap();
        let before = workspace.annotation_snapshot(first, cx).unwrap();
        assert!(workspace.apply_highlight_defaults_event(&event, cx).unwrap());
        assert_eq!(workspace.highlight_defaults(first, cx), Some(defaults.clone()));
        assert_eq!(workspace.annotation_snapshot(first, cx).unwrap(), before, "editing defaults must not modify existing annotations or history");
        let invalid = HighlightDefaultsEvent::Change { document_id: first, defaults: PenAnnotationDefaults { width_pt: f64::NAN, ..defaults.clone() } };
        assert!(workspace.apply_highlight_defaults_event(&invalid, cx).is_err());
        assert_eq!(workspace.highlight_defaults(first, cx), Some(defaults.clone()));
    });
    workspace.update(cx, |workspace, cx| workspace.open_path(PathBuf::from("/tmp/highlight-defaults-b.pdf"), cx));
    cx.run_until_parked();
    let second = workspace.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    assert_ne!(first, second);
    workspace.update(cx, |workspace, cx| {
        workspace.set_annotation_tool(second, AnnotationTool::Highlight, cx).unwrap();
        assert!(!workspace.apply_highlight_defaults_event(&event, cx).unwrap(), "stale document event must be ignored");
        workspace.begin_save(second, cx).unwrap();
        let current = HighlightDefaultsEvent::Change { document_id: second, defaults };
        assert!(!workspace.apply_highlight_defaults_event(&current, cx).unwrap(), "saving rejects defaults mutation");
    });
}

#[gpui::test]
fn pending_thumbnail_result_cannot_repopulate_a_closed_rotated_or_replaced_document(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    for invalidation in ["rotation", "close", "resource replacement"] {
        let backend = Arc::new(RecordingBackend { page_count: 30, ..Default::default() });
        let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
        workspace.update(cx, |workspace, cx| workspace.open_path(PathBuf::from("/tmp/sidebar-stale.pdf"), cx));
        cx.run_until_parked();
        let id = workspace.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
        workspace.update(cx, |workspace, cx| {
            workspace.request_thumbnail_preview(id, 24, false, cx);
            if invalidation == "close" {
                assert!(workspace.close_document(id, cx));
            } else if invalidation == "resource replacement" {
                let save = workspace.begin_save(id, cx).unwrap();
                let reopened = backend.open(&OpenDocumentRequest { document_id: id, generation: 2, path: PathBuf::from("/tmp/sidebar-stale.pdf") }).unwrap();
                assert_eq!(workspace.apply_save_result(&save, Ok(SavedNativeDocument::new(reopened, save.annotation_revision)), cx), butter_paper_gpui_migration::document_workspace::ApplyDisposition::Applied);
            } else {
                workspace.begin_page_rotation(id, 24, butter_paper_gpui_migration::annotation_model::PageRotationDirection::Right, cx).unwrap();
            }
        });
        cx.run_until_parked();
        workspace.read_with(cx, |workspace, cx| {
            if invalidation == "close" { assert!(workspace.session(id, cx).is_none()); }
            else { assert!(workspace.session(id, cx).unwrap().read(cx).thumbnail_base_raster(24).is_none(), "stale pixels must not be installed after {invalidation}"); }
        });
    }
}

#[gpui::test]
fn workspace_empty_left_rail_is_inert(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RecordingBackend::default()), cx));
    let (_, cx) = cx.add_window_view(move |window, cx| Root::new(workspace, window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let toggle = cx.debug_bounds("document-left-rail-pages").expect("disabled toggle remains visible");
    cx.simulate_click(toggle.center(), gpui::Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("document-thumbnail-strip").is_none());
    assert!(cx.debug_bounds("document-left-rail").is_some());
}

#[gpui::test]
fn thumbnail_click_scrolls_the_continuous_canvas_to_its_page(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let backend = Arc::new(RecordingBackend { page_count: 3, ..Default::default() });
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| workspace.open_path(PathBuf::from("/tmp/sidebar-navigation.pdf"), cx));
    cx.run_until_parked();
    let observed = workspace.clone();
    workspace.update(cx, |workspace, cx| {
        let id = workspace.active_document_id().unwrap();
        workspace.set_page_view_mode(id, butter_paper_gpui_migration::page_view_control::PageViewMode::Continuous, cx);
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
            workspace.refresh_viewport_async(id, 700., 500., 1., cx).unwrap();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
    observed.read_with(cx, |workspace, cx| {
        let id = workspace.active_document_id().unwrap();
        assert_eq!(workspace.session(id, cx).unwrap().read(cx).current_page(), 1);
        assert!(workspace.document_view_state(id, cx).unwrap().scroll().1 > 0.);
    });
}

#[gpui::test]
fn sidebar_loads_visible_late_pages_and_retries_failure_without_navigation(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let backend = Arc::new(RecordingBackend { page_count: 30, ..Default::default() });
    backend.preview_failure.store(true, Ordering::Release);
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| workspace.open_path(PathBuf::from("/tmp/sidebar-late-pages.pdf"), cx));
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
    observed.update(cx, |workspace, cx| { workspace.scroll_thumbnail_to_page(24, cx); });
    for _ in 0..4 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
    let retry = cx.debug_bounds("thumbnail-24-retry").expect("failed visible preview offers retry");
    assert_eq!(backend.preview_failures.load(Ordering::Acquire), 1, "redraws must not retry automatically");
    backend.preview_failure.store(false, Ordering::Release);
    cx.simulate_click(retry.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("thumbnail-24-retry").is_none());
    observed.read_with(cx, |workspace, cx| {
        let session = workspace.session(workspace.active_document_id().unwrap(), cx).unwrap().read(cx);
        assert!(session.thumbnail_base_raster(24).is_some());
        assert_eq!(session.current_page(), 0, "preview loading must not navigate the canvas");
        assert!(session.thumbnail_base_raster(15).is_none(), "off-screen middle pages stay lazy");
    });
}

#[test]
fn sidebar_scale_ratio_uses_units_not_preset_name() {
    use butter_paper_gpui_migration::annotation_model::{PageScale, ScaleUnit, ScalePrecision};
    let scale = PageScale::custom(0, "A custom scale", ScaleUnit::Cm, ScaleUnit::M, 1., 0.5, None, ScalePrecision::default()).unwrap();
    assert_eq!(scale.ratio_label(), "1:50");
    let imperial = PageScale::custom(0, "Feet", ScaleUnit::In, ScaleUnit::Ft, 1., 1., None, ScalePrecision::default()).unwrap();
    assert_eq!(imperial.ratio_label(), "1:12");
    for preset in butter_paper_gpui_migration::annotation_model::built_in_scale_presets() {
        let scale = PageScale::from_factors(0, preset.source, preset.name.clone(), preset.pdf_units, preset.real_units, preset.scale_x, preset.scale_y, ScalePrecision::default()).unwrap();
        assert_eq!(scale.ratio_label(), preset.name);
    }
}

#[gpui::test]
fn sidebar_reveals_page_selected_by_external_canvas_scroll(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let backend = Arc::new(RecordingBackend { page_count: 30, ..Default::default() });
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    workspace.update(cx, |workspace, cx| workspace.open_path(PathBuf::from("/tmp/sidebar-canvas-scroll.pdf"), cx));
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
        workspace.set_page_view_mode(id, butter_paper_gpui_migration::page_view_control::PageViewMode::Continuous, cx);
        workspace.refresh_viewport_async(id, 700., 500., 1., cx).unwrap();
        workspace.set_viewport_scroll(id, 0., 18000., cx);
        workspace.refresh_viewport_async(id, 700., 500., 1., cx).unwrap();
    });
    cx.run_until_parked();
    for _ in 0..2 { cx.update(|window, cx| window.draw(cx).clear(cx)); }
    let page = observed.read_with(cx, |workspace, cx| workspace.session(workspace.active_document_id().unwrap(), cx).unwrap().read(cx).current_page());
    assert!(page > 3, "the canvas moved beyond initially visible thumbnails");
    let row_id: &'static str = Box::leak(format!("document-1-thumbnail-{page}").into_boxed_str());
    let row = cx.debug_bounds(row_id).expect("selected page materialised in sidebar");
    assert!(cx.debug_bounds("document-thumbnail-strip").unwrap().contains(&row.center()), "selected thumbnail is revealed without sidebar input");
}

#[derive(Default)]
struct RecordingBackend {
    released: Arc<AtomicBool>,
    page_count: usize,
    preview_failure: Arc<AtomicBool>,
    preview_failures: Arc<std::sync::atomic::AtomicUsize>,
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

    fn render_tile(&self, request: TileRequest) -> Result<RasterSurface, String> {
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
