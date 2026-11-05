//! A PDF whose changes can't go back to it (another app's copy, a read-only
//! file): its first change opens a prompt offering Downloads or a Save
//! panel, and another app's copy stays out of Open Recent.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use butter_paper::annotation_model::{MarkupId, PdfPoint};
use butter_paper::document_resource::{
    NativeDocumentResource, OpenedNativeDocument, RasterSurface, ThumbnailSurface,
};
use butter_paper::document_workspace::{
    DOCUMENT_SOURCE_PLACE_CHOOSE_ID, DOCUMENT_SOURCE_PLACE_DIALOG_ID,
    DOCUMENT_SOURCE_PLACE_DOWNLOADS_ID, DOCUMENT_SOURCE_PLACE_KEEP_EDITING_ID, DocumentWorkspace,
    NativeDocumentOpener, OpenDocumentRequest, init_document_workspace_actions,
};
use butter_paper::recent_documents::RecentDocuments;
use butter_paper::save_places::{CopyOwner, CopyRoots, SavePlaces, SourcePlace};
use gpui::{AppContext as _, Modifiers, TestAppContext};
use gpui_component::Root;

fn raster(width: u32, height: u32) -> RasterSurface {
    RasterSurface::new(
        width,
        height,
        vec![0xff; width as usize * height as usize * 4],
    )
    .unwrap()
}

struct Resource(AtomicBool);

impl NativeDocumentResource for Resource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4400)
    }

    fn render_page(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
        Ok(raster(width, page_index + 2))
    }

    fn render_thumbnail(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
        Ok(raster(width, page_index + 2))
    }

    fn render_tile(
        &self,
        request: butter_paper::viewer::TileRequest,
    ) -> Result<RasterSurface, String> {
        Ok(raster(
            request.crop.width as u32,
            request.crop.height as u32,
        ))
    }

    fn close(&self) -> Result<(), String> {
        self.0.store(true, Ordering::Release);
        Ok(())
    }

    fn is_released(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

struct Opener;

impl NativeDocumentOpener for Opener {
    fn open(&self, request: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        use sha2::Digest as _;
        let bytes = std::fs::read(&request.path).map_err(|error| error.to_string())?;
        let digest: [u8; 32] = sha2::Sha256::digest(&bytes).into();
        OpenedNativeDocument::new(
            "report.pdf",
            vec![(595., 842.)],
            raster(32, 40),
            vec![ThumbnailSurface::new(0, raster(8, 10))],
            Arc::new(Resource(AtomicBool::new(false))),
        )
        .map(|opened| opened.with_source_sha256(digest))
    }
}

struct Shell(gpui::Entity<DocumentWorkspace>);

impl gpui::Render for Shell {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{ParentElement as _, Styled as _};
        let dialogs = Root::render_dialog_layer(window, cx);
        gpui::div()
            .size_full()
            .child(self.0.clone())
            .children(dialogs)
    }
}

#[path = "support/dialog_contract.rs"]
mod dialog_contract;

/// Paints until any dialog has finished opening or closing.
fn redraw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    dialog_contract::settle(cx);
}

fn prompt_open(cx: &mut gpui::VisualTestContext) -> bool {
    cx.debug_bounds(DOCUMENT_SOURCE_PLACE_DIALOG_ID).is_some()
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is not painted"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    redraw(cx);
}

fn scratch(name: &str) -> PathBuf {
    let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-scratch/source-place-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(scratch.join("Downloads")).unwrap();
    // Resolved and without Windows' `\\?\` prefix, as opened documents'
    // paths are.
    dunce::canonicalize(&scratch).unwrap()
}

/// The fixture copied to `pdf` and opened there, with Mail's copies in
/// `scratch/Mail Downloads` and Downloads at `scratch/Downloads`.
fn open<'a>(
    cx: &'a mut TestAppContext,
    scratch: &Path,
    pdf: &Path,
) -> (
    gpui::Entity<DocumentWorkspace>,
    gpui::Entity<RecentDocuments>,
    &'a mut gpui::VisualTestContext,
) {
    open_with(cx, scratch, pdf, |_| {})
}

/// [`open`], after `prepare` has had the copied PDF.
fn open_with<'a>(
    cx: &'a mut TestAppContext,
    scratch: &Path,
    pdf: &Path,
    prepare: impl FnOnce(&Path),
) -> (
    gpui::Entity<DocumentWorkspace>,
    gpui::Entity<RecentDocuments>,
    &'a mut gpui::VisualTestContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    std::fs::create_dir_all(pdf.parent().unwrap()).unwrap();
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/text-edit/word-table.pdf"),
        pdf,
    )
    .unwrap();
    prepare(pdf);
    let places = SavePlaces {
        copies: CopyRoots {
            copies: vec![scratch.join("Mail Downloads")],
            own_files: Vec::new(),
        },
        downloads: Some(scratch.join("Downloads")),
    };
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let slot = slot.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(Opener), cx));
            slot.replace(Some(workspace.clone()));
            let shell = cx.new(|_| Shell(workspace));
            Root::new(shell, window, cx)
        }
    });
    let workspace = slot.borrow_mut().take().unwrap();
    let recent = cx.update(|_, cx| cx.new(|_| RecentDocuments::default()));
    cx.update(|window, _| window.activate_window());
    workspace.update(cx, |workspace, cx| {
        workspace.set_save_places(places);
        workspace.bind_recent_documents(recent.clone(), cx);
        workspace.open_path(pdf.to_path_buf(), cx)
    });
    redraw(cx);
    (workspace, recent, cx)
}

/// Leaves the open document with a rectangle not yet saved.
fn edit(workspace: &gpui::Entity<DocumentWorkspace>, cx: &mut gpui::VisualTestContext) {
    workspace
        .update(cx, |workspace, cx| {
            let document_id = workspace.active_document_id().unwrap();
            workspace.create_rectangle(
                document_id,
                0,
                MarkupId::new("source-place:rectangle").unwrap(),
                PdfPoint::new(72., 96.).unwrap(),
                PdfPoint::new(216., 192.).unwrap(),
                cx,
            )
        })
        .unwrap();
    redraw(cx);
}

fn active_path(
    workspace: &gpui::Entity<DocumentWorkspace>,
    cx: &mut gpui::VisualTestContext,
) -> PathBuf {
    workspace.update(cx, |workspace, cx| {
        let document_id = workspace.active_document_id().unwrap();
        workspace
            .session(document_id, cx)
            .unwrap()
            .read(cx)
            .path()
            .to_path_buf()
    })
}

fn mail_copy(scratch: &Path) -> PathBuf {
    scratch.join("Mail Downloads/415D1A6D-BBB5-41C0-8F26-432ED18F9FF9/report.pdf")
}

#[gpui::test]
fn mails_copy_asks_for_a_place_once_changed_and_saves_to_downloads(cx: &mut TestAppContext) {
    let scratch = scratch("downloads");
    let pdf = mail_copy(&scratch);
    let (workspace, recent, cx) = open(cx, &scratch, &pdf);
    let original = std::fs::read(&pdf).unwrap();
    workspace.update(cx, |workspace, cx| {
        let document_id = workspace.active_document_id().unwrap();
        assert_eq!(
            workspace.document_source_place(document_id, cx),
            Some(SourcePlace::AppCopy(CopyOwner::Mail))
        );
        assert!(workspace.document_requires_save_as(document_id, cx));
    });
    assert!(
        recent.read_with(cx, |recent, _| recent.paths().is_empty()),
        "Mail's copy stays out of Open Recent"
    );
    // Reading it needs nothing.
    assert!(cx.debug_bounds(DOCUMENT_SOURCE_PLACE_DIALOG_ID).is_none());

    edit(&workspace, cx);
    assert!(cx.debug_bounds(DOCUMENT_SOURCE_PLACE_DIALOG_ID).is_some());
    click(cx, DOCUMENT_SOURCE_PLACE_DOWNLOADS_ID);

    let saved = scratch.join("Downloads/report.pdf");
    assert_eq!(active_path(&workspace, cx), saved);
    assert_ne!(std::fs::read(&saved).unwrap(), original);
    assert_eq!(
        std::fs::read(&pdf).unwrap(),
        original,
        "Mail's copy is untouched"
    );
    assert!(cx.debug_bounds(DOCUMENT_SOURCE_PLACE_DIALOG_ID).is_none());
    workspace.update(cx, |workspace, cx| {
        let document_id = workspace.active_document_id().unwrap();
        assert_eq!(
            workspace.annotation_status().as_deref(),
            Some("Saved “report.pdf” in Downloads"),
            "the status bar says where it went"
        );
        assert_eq!(workspace.document_source_place(document_id, cx), None);
        assert!(!workspace.document_requires_save_as(document_id, cx));
    });
    assert_eq!(
        recent.read_with(cx, |recent, _| recent.paths().to_vec()),
        vec![saved],
        "the saved PDF is the one to reopen"
    );
}

#[gpui::test]
fn save_to_downloads_keeps_a_pdf_already_there(cx: &mut TestAppContext) {
    let scratch = scratch("numbered");
    std::fs::write(scratch.join("Downloads/report.pdf"), b"mine").unwrap();
    let pdf = mail_copy(&scratch);
    let (workspace, _, cx) = open(cx, &scratch, &pdf);
    edit(&workspace, cx);
    click(cx, DOCUMENT_SOURCE_PLACE_DOWNLOADS_ID);
    assert_eq!(
        active_path(&workspace, cx),
        scratch.join("Downloads/report 2.pdf")
    );
    assert_eq!(
        std::fs::read(scratch.join("Downloads/report.pdf")).unwrap(),
        b"mine"
    );
}

#[gpui::test]
fn choose_where_to_save_starts_in_downloads_under_its_own_name(cx: &mut TestAppContext) {
    let scratch = scratch("choose");
    let pdf = mail_copy(&scratch);
    let (workspace, _, cx) = open(cx, &scratch, &pdf);
    edit(&workspace, cx);
    click(cx, DOCUMENT_SOURCE_PLACE_CHOOSE_ID);
    let downloads = scratch.join("Downloads");
    let chosen = scratch.join("Projects/report.pdf");
    std::fs::create_dir_all(chosen.parent().unwrap()).unwrap();
    let answer = chosen.clone();
    cx.simulate_new_path_selection(move |directory| {
        assert_eq!(directory, downloads);
        Some(answer)
    });
    redraw(cx);
    assert_eq!(active_path(&workspace, cx), chosen);
    assert!(chosen.exists());
}

#[gpui::test]
fn keep_editing_keeps_the_changes_and_save_still_asks(cx: &mut TestAppContext) {
    let scratch = scratch("later");
    let pdf = mail_copy(&scratch);
    let (workspace, _, cx) = open(cx, &scratch, &pdf);
    edit(&workspace, cx);
    click(cx, DOCUMENT_SOURCE_PLACE_KEEP_EDITING_ID);
    assert!(!prompt_open(cx));
    workspace.update(cx, |workspace, cx| {
        let document_id = workspace.active_document_id().unwrap();
        let snapshot = workspace.annotation_snapshot(document_id, cx).unwrap();
        assert_eq!(
            snapshot.annotation_order,
            vec![MarkupId::new("source-place:rectangle").unwrap()],
            "the change is kept"
        );
        assert!(snapshot.dirty, "and still unsaved");
    });
    // More changes don't ask again.
    workspace
        .update(cx, |workspace, cx| {
            let document_id = workspace.active_document_id().unwrap();
            workspace.create_rectangle(
                document_id,
                0,
                MarkupId::new("source-place:rectangle-2").unwrap(),
                PdfPoint::new(300., 96.).unwrap(),
                PdfPoint::new(400., 192.).unwrap(),
                cx,
            )
        })
        .unwrap();
    redraw(cx);
    assert!(!prompt_open(cx));
    workspace.update(cx, |workspace, cx| {
        let document_id = workspace.active_document_id().unwrap();
        // Save still asks for a place.
        assert!(workspace.document_requires_save_as(document_id, cx));
    });
}

#[gpui::test]
fn the_prompt_keeps_the_dialog_dismissal_contract(cx: &mut TestAppContext) {
    let scratch = scratch("contract");
    let (workspace, _, cx) = open(cx, &scratch, &mail_copy(&scratch));
    edit(&workspace, cx);
    // Each reopening is another of Mail's copies, changed.
    let mut copies = 0;
    let reopen = {
        let scratch = scratch.clone();
        move |cx: &mut gpui::VisualTestContext| {
            copies += 1;
            let pdf = scratch.join(format!("Mail Downloads/{copies}/report.pdf"));
            std::fs::create_dir_all(pdf.parent().unwrap()).unwrap();
            std::fs::copy(
                scratch.join("Mail Downloads/415D1A6D-BBB5-41C0-8F26-432ED18F9FF9/report.pdf"),
                &pdf,
            )
            .unwrap();
            workspace.update(cx, |workspace, cx| workspace.open_path(pdf, cx));
            redraw(cx);
            edit(&workspace, cx);
        }
    };
    dialog_contract::assert_dialog_dismissal(
        cx,
        "Where to save",
        DOCUMENT_SOURCE_PLACE_DIALOG_ID,
        DOCUMENT_SOURCE_PLACE_KEEP_EDITING_ID,
        dialog_contract::Outside::Closes,
        reopen,
    );
}

#[gpui::test]
fn a_pdf_in_the_persons_own_folder_saves_in_place(cx: &mut TestAppContext) {
    let scratch = scratch("own");
    let pdf = scratch.join("Documents/report.pdf");
    let (workspace, recent, cx) = open(cx, &scratch, &pdf);
    edit(&workspace, cx);
    assert!(cx.debug_bounds(DOCUMENT_SOURCE_PLACE_DIALOG_ID).is_none());
    workspace.update(cx, |workspace, cx| {
        let document_id = workspace.active_document_id().unwrap();
        assert_eq!(workspace.document_source_place(document_id, cx), None);
        assert!(!workspace.document_requires_save_as(document_id, cx));
    });
    assert_eq!(
        recent.read_with(cx, |recent, _| recent.paths().to_vec()),
        vec![pdf]
    );
}

#[cfg(unix)]
#[gpui::test]
fn a_read_only_pdf_asks_for_a_place_once_changed(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;
    // Root may write to anything.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let scratch = scratch("read-only");
    let pdf = scratch.join("Documents/report.pdf");
    let (workspace, recent, cx) = open_with(cx, &scratch, &pdf, |pdf| {
        std::fs::set_permissions(pdf, std::fs::Permissions::from_mode(0o444)).unwrap();
    });
    assert!(cx.debug_bounds(DOCUMENT_SOURCE_PLACE_DIALOG_ID).is_none());
    edit(&workspace, cx);
    assert!(cx.debug_bounds(DOCUMENT_SOURCE_PLACE_DIALOG_ID).is_some());
    workspace.update(cx, |workspace, cx| {
        let document_id = workspace.active_document_id().unwrap();
        assert_eq!(
            workspace.document_source_place(document_id, cx),
            Some(SourcePlace::ReadOnlyFile)
        );
    });
    // The person's own file, which stays in Open Recent.
    assert_eq!(
        recent.read_with(cx, |recent, _| recent.paths().to_vec()),
        vec![pdf.clone()]
    );
    std::fs::set_permissions(&pdf, std::fs::Permissions::from_mode(0o644)).unwrap();
}
