use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use butter_paper_gpui_component_compat::{
    application_close_workspace::{
        ApplicationCloseShell, ApplicationCloseWorkspace, RequestApplicationClose,
        register_application_close_action,
    },
    document_workspace::{
        DocumentOpenBatchRequest, DocumentOpenOrigin, DocumentWorkspace, NativeDocumentOpener,
        NativeDocumentResource, NativeDocumentSaver, OpenDocumentRequest, OpenPdf,
        OpenedNativeDocument, RasterSurface, Save, SaveAs, SaveDocumentRequest,
        SavedNativeDocument, ThumbnailSurface,
    },
    native_application::{
        NativeApplicationMenuState, NativeDocumentIngress, build_native_application_menus,
    },
};
use butter_paper_gpui_gallery::{
    annotation_model::{MarkupId, PdfPoint},
    viewer::TileRequest,
};
use gpui::{
    Action, AppContext as _, ExternalPaths, FileDropEvent, Menu, MenuItem, TestAppContext, point,
    px,
};
use gpui_component::Root;

#[test]
fn native_application_menu_uses_document_actions_and_never_exposes_raw_quit() {
    let menus = build_native_application_menus(NativeApplicationMenuState {
        has_active_document: true,
        save_busy: false,
    });

    assert_eq!(
        menu_names(&menus),
        ["Butter Paper", "File", "Edit", "Window"]
    );
    let app = menu(&menus, "Butter Paper");
    assert_action(item(app, "Quit Butter Paper"), |action| {
        action.as_any().is::<RequestApplicationClose>()
    });
    let file = menu(&menus, "File");
    assert_action(item(file, "Open…"), |action| {
        action.as_any().is::<OpenPdf>()
    });
    assert_action(item(file, "Save"), |action| action.as_any().is::<Save>());
    assert_action(item(file, "Save As…"), |action| {
        action.as_any().is::<SaveAs>()
    });
    assert_action(item(file, "Close Window"), |action| {
        action.as_any().is::<RequestApplicationClose>()
    });
    assert!(!item(file, "Save").is_disabled());
    assert!(!item(file, "Save As…").is_disabled());

    let disabled = build_native_application_menus(NativeApplicationMenuState::default());
    let file = menu(&disabled, "File");
    assert!(item(file, "Save").is_disabled());
    assert!(item(file, "Save As…").is_disabled());

    let busy = build_native_application_menus(NativeApplicationMenuState {
        has_active_document: true,
        save_busy: true,
    });
    let file = menu(&busy, "File");
    assert!(item(file, "Save").is_disabled());
    assert!(item(file, "Save As…").is_disabled());
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

#[derive(Default)]
struct RecordingBackend {
    released: Arc<AtomicBool>,
}

impl NativeDocumentOpener for RecordingBackend {
    fn open(&self, request: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        OpenedNativeDocument::new(
            request
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("fixture.pdf"),
            vec![(612., 792.)],
            raster(32, 40),
            vec![ThumbnailSurface::new(0, raster(8, 10))],
            Arc::new(RecordingResource {
                released: self.released.clone(),
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
}

impl NativeDocumentResource for RecordingResource {
    fn worker_pid(&self) -> Option<u32> {
        None
    }

    fn render_page(&self, _: u32, width: u32) -> Result<RasterSurface, String> {
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
