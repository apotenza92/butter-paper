//! Convert to Word Document…: one dialog asks, shows the conversion's
//! progress and then offers to show or open the Word document.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use butter_paper::annotation_model::{MarkupId, PdfPoint};
use butter_paper::document_resource::{
    NativeDocumentResource, OpenedNativeDocument, RasterSurface, ThumbnailSurface,
};
use butter_paper::document_workspace::{
    DOCUMENT_WORD_CONVERT_ID, DOCUMENT_WORD_DIALOG_ID, DOCUMENT_WORD_KEEP_BOTH_ID,
    DOCUMENT_WORD_OPEN_ID, DOCUMENT_WORD_REPLACE_ID, DOCUMENT_WORD_SHOW_ID, DocumentWorkspace,
    NativeDocumentOpener, OpenDocumentRequest, WordDialogStage, init_document_workspace_actions,
};
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

struct DialogShell(gpui::Entity<DocumentWorkspace>);

impl gpui::Render for DialogShell {
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

fn redraw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is not painted"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    redraw(cx);
}

/// A copy of a two-page fixture opened in a workspace, and its folder.
fn open<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
) -> (
    gpui::Entity<DocumentWorkspace>,
    PathBuf,
    &'a mut gpui::VisualTestContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-scratch/word-dialog-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let pdf = scratch.join("report.pdf");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/text-edit/word-table.pdf"),
        &pdf,
    )
    .unwrap();
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(Opener), cx));
            workspace_slot.replace(Some(workspace.clone()));
            let shell = cx.new(|_| DialogShell(workspace));
            Root::new(shell, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    workspace.update(cx, |workspace, cx| workspace.open_path(pdf, cx));
    redraw(cx);
    workspace.update(cx, |workspace, cx| {
        workspace.convert_active_document_to_word(cx)
    });
    redraw(cx);
    (workspace, scratch, cx)
}

fn converted(
    workspace: &gpui::Entity<DocumentWorkspace>,
    cx: &mut gpui::VisualTestContext,
) -> PathBuf {
    workspace.update(cx, |workspace, cx| {
        let dialog = workspace.word_dialog().expect("the dialog stays open");
        match dialog.read(cx).stage() {
            WordDialogStage::Converted(converted) => converted.docx.clone(),
            WordDialogStage::Failed(error) => panic!("conversion failed: {error}"),
            _ => panic!("still converting"),
        }
    })
}

#[gpui::test]
fn the_dialog_converts_and_then_offers_to_show_or_open(cx: &mut TestAppContext) {
    let (workspace, scratch, cx) = open(cx, "convert");
    assert!(cx.debug_bounds(DOCUMENT_WORD_DIALOG_ID).is_some());
    assert!(
        cx.debug_bounds(DOCUMENT_WORD_OPEN_ID).is_none(),
        "nothing converts until asked"
    );
    assert!(!scratch.join("report.docx").exists());

    click(cx, DOCUMENT_WORD_CONVERT_ID);
    assert_eq!(converted(&workspace, cx), scratch.join("report.docx"));
    assert!(scratch.join("report.docx").exists());
    assert!(cx.debug_bounds(DOCUMENT_WORD_DIALOG_ID).is_some());
    assert!(cx.debug_bounds(DOCUMENT_WORD_SHOW_ID).is_some());
    assert!(cx.debug_bounds(DOCUMENT_WORD_OPEN_ID).is_some());
    assert!(cx.debug_bounds(DOCUMENT_WORD_CONVERT_ID).is_none());

    cx.simulate_keystrokes("escape");
    redraw(cx);
    assert!(cx.debug_bounds(DOCUMENT_WORD_DIALOG_ID).is_none());
}

#[gpui::test]
fn an_existing_word_document_is_replaced_or_kept(cx: &mut TestAppContext) {
    let (workspace, scratch, cx) = open(cx, "exists");
    // Asked about after the dialog opened: Convert finds it and asks.
    std::fs::write(scratch.join("report.docx"), b"earlier").unwrap();
    click(cx, DOCUMENT_WORD_CONVERT_ID);
    assert!(cx.debug_bounds(DOCUMENT_WORD_REPLACE_ID).is_some());
    assert_eq!(
        std::fs::read(scratch.join("report.docx")).unwrap(),
        b"earlier"
    );

    click(cx, DOCUMENT_WORD_KEEP_BOTH_ID);
    assert_eq!(converted(&workspace, cx), scratch.join("report 2.docx"));
    assert_eq!(
        std::fs::read(scratch.join("report.docx")).unwrap(),
        b"earlier"
    );
    cx.simulate_keystrokes("escape");
    redraw(cx);

    workspace.update(cx, |workspace, cx| {
        workspace.convert_active_document_to_word(cx)
    });
    redraw(cx);
    click(cx, DOCUMENT_WORD_REPLACE_ID);
    assert_eq!(converted(&workspace, cx), scratch.join("report.docx"));
    assert_ne!(
        std::fs::read(scratch.join("report.docx")).unwrap(),
        b"earlier"
    );
}

/// Leaves the open document with a rectangle not yet saved.
fn edit(workspace: &gpui::Entity<DocumentWorkspace>, cx: &mut gpui::VisualTestContext) {
    workspace
        .update(cx, |workspace, cx| {
            let document_id = workspace.active_document_id().unwrap();
            workspace.create_rectangle(
                document_id,
                0,
                MarkupId::new("word:rectangle").unwrap(),
                PdfPoint::new(72., 96.).unwrap(),
                PdfPoint::new(216., 192.).unwrap(),
                cx,
            )
        })
        .unwrap();
    redraw(cx);
}

fn stage_is(
    workspace: &gpui::Entity<DocumentWorkspace>,
    cx: &mut gpui::VisualTestContext,
    expected: fn(&WordDialogStage) -> bool,
) -> bool {
    workspace.update(cx, |workspace, cx| {
        workspace
            .word_dialog()
            .is_some_and(|dialog| expected(dialog.read(cx).stage()))
    })
}

#[gpui::test]
fn unsaved_changes_are_saved_and_then_converted(cx: &mut TestAppContext) {
    let (workspace, scratch, cx) = open(cx, "unsaved");
    cx.simulate_keystrokes("escape");
    redraw(cx);
    edit(&workspace, cx);
    workspace.update(cx, |workspace, cx| {
        workspace.convert_active_document_to_word(cx)
    });
    redraw(cx);

    assert!(cx.debug_bounds(DOCUMENT_WORD_CONVERT_ID).is_some());
    click(cx, DOCUMENT_WORD_CONVERT_ID);
    assert_eq!(converted(&workspace, cx), scratch.join("report.docx"));
    // The PDF was saved with the rectangle before it converted.
    assert_ne!(
        std::fs::read(scratch.join("report.pdf")).unwrap(),
        std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/text-edit/word-table.pdf")
        )
        .unwrap()
    );
}

#[gpui::test]
fn a_failed_save_says_why_and_converts_nothing(cx: &mut TestAppContext) {
    let (workspace, scratch, cx) = open(cx, "save-fails");
    cx.simulate_keystrokes("escape");
    redraw(cx);
    edit(&workspace, cx);
    // Changed behind Butter Paper's back, so saving over it is refused.
    std::fs::write(scratch.join("report.pdf"), b"%PDF-1.7 changed elsewhere").unwrap();
    workspace.update(cx, |workspace, cx| {
        workspace.convert_active_document_to_word(cx)
    });
    redraw(cx);

    click(cx, DOCUMENT_WORD_CONVERT_ID);
    let error = workspace.update(cx, |workspace, cx| {
        match workspace.word_dialog().unwrap().read(cx).stage() {
            WordDialogStage::Failed(error) => error.clone(),
            _ => panic!("the save should have failed"),
        }
    });
    assert!(
        error.starts_with("The PDF could not be saved, so it was not converted: "),
        "{error}"
    );
    assert!(!scratch.join("report.docx").exists());
}

#[gpui::test]
fn closing_while_it_saves_forgets_the_conversion(cx: &mut TestAppContext) {
    let (workspace, scratch, cx) = open(cx, "close-saving");
    cx.simulate_keystrokes("escape");
    redraw(cx);
    edit(&workspace, cx);
    workspace.update(cx, |workspace, cx| {
        workspace.convert_active_document_to_word(cx)
    });
    redraw(cx);

    // Pressed, then closed before the save finishes: events sent straight
    // to the window, as simulated ones wait for background work.
    let center = cx.debug_bounds(DOCUMENT_WORD_CONVERT_ID).unwrap().center();
    cx.update(|window, cx| {
        for event in [
            gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                button: gpui::MouseButton::Left,
                position: center,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }),
            gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                button: gpui::MouseButton::Left,
                position: center,
                modifiers: Modifiers::default(),
                click_count: 1,
            }),
        ] {
            window.dispatch_event(event, cx);
        }
    });
    assert!(stage_is(&workspace, cx, |stage| matches!(
        stage,
        WordDialogStage::Saving
    )));
    cx.update(|window, cx| {
        window.dispatch_keystroke(gpui::Keystroke::parse("escape").unwrap(), cx);
        window.draw(cx).clear(cx);
    });
    assert!(workspace.read_with(cx, |workspace, _| workspace.word_dialog().is_none()));
    workspace.update(cx, |workspace, cx| {
        workspace.convert_active_document_to_word(cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    redraw(cx);

    // The reopened dialog asks again rather than converting on its own.
    assert!(stage_is(&workspace, cx, |stage| matches!(
        stage,
        WordDialogStage::Ready
    )));
    assert!(!scratch.join("report.docx").exists());
}

#[path = "support/dialog_contract.rs"]
mod dialog_contract;

#[gpui::test]
fn the_word_dialog_keeps_the_dialog_dismissal_contract(cx: &mut TestAppContext) {
    use butter_paper::document_workspace::DOCUMENT_WORD_CANCEL_ID;
    let (workspace, _scratch, cx) = open(cx, "contract");
    let reopen = move |cx: &mut gpui::VisualTestContext| {
        workspace.update(cx, |workspace, cx| {
            workspace.convert_active_document_to_word(cx)
        });
        redraw(cx);
    };
    dialog_contract::assert_dialog_dismissal(
        cx,
        "Convert to Word document",
        DOCUMENT_WORD_DIALOG_ID,
        DOCUMENT_WORD_CANCEL_ID,
        dialog_contract::Outside::Closes,
        reopen,
    );
}

/// A folder the app may not save in (macOS protects Downloads, Documents
/// and the Desktop; here, one without write permission): Convert says so,
/// and a place chosen in the Save panel takes the Word document.
#[cfg(unix)]
#[gpui::test]
fn a_folder_that_refuses_the_word_document_offers_a_save_panel(cx: &mut TestAppContext) {
    use butter_paper::document_workspace::DOCUMENT_WORD_CHOOSE_ID;
    use std::os::unix::fs::PermissionsExt as _;
    let (workspace, scratch, cx) = open(cx, "not-permitted");
    let protected = std::fs::Permissions::from_mode(0o555);
    std::fs::set_permissions(&scratch, protected).unwrap();
    click(cx, DOCUMENT_WORD_CONVERT_ID);
    std::fs::set_permissions(&scratch, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        stage_is(&workspace, cx, |stage| matches!(
            stage,
            WordDialogStage::NotPermitted
        )),
        "the refusal is told apart from a failed conversion"
    );
    assert!(!scratch.join("report.docx").exists());
    assert!(
        std::fs::read_dir(&scratch).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".part")),
        "nothing is left behind"
    );

    let elsewhere = scratch.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let chosen = elsewhere.join("report.docx");
    let answer = chosen.clone();
    // The Save panel starts in the PDF's folder.
    let folder = scratch.clone();
    click(cx, DOCUMENT_WORD_CHOOSE_ID);
    cx.simulate_new_path_selection(move |directory| {
        assert_eq!(directory, folder);
        Some(answer)
    });
    redraw(cx);
    assert_eq!(converted(&workspace, cx), chosen);
    assert!(chosen.exists());
}

#[test]
fn a_pdf_saved_from_a_word_document_converts_to_its_name() {
    use butter_paper::docx_export::docx_path_for;
    assert_eq!(
        docx_path_for(std::path::Path::new("/d/CV_Sep2026.docx.pdf")),
        PathBuf::from("/d/CV_Sep2026.docx")
    );
    assert_eq!(
        docx_path_for(std::path::Path::new("/d/report.pdf")),
        PathBuf::from("/d/report.docx")
    );
    assert_eq!(
        docx_path_for(std::path::Path::new("/d/.docx.pdf")),
        PathBuf::from("/d/.docx.docx")
    );
}
