//! The Edit PDF Content tool in the workspace: a press in page text starts the
//! inline editor, typing (including tool shortcut letters) edits the line,
//! the worker is told what to render, and Enter records one undoable edit.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use butter_paper::annotation_adapter::{AnnotationAdapter, AnnotationTool};
use butter_paper::annotation_model::PageRotation;
use butter_paper::document_recovery_store::{
    DocumentRecoveryStore, RecoveryPublication, RecoverySourceKind,
};
use butter_paper::document_resource::{
    DEFAULT_THUMBNAIL_WIDTH, NativeDocumentResource, OpenedNativeDocument, RasterSurface,
    ThumbnailSurface,
};
use butter_paper::document_workspace::{
    DOCUMENT_SIGNED_TEXT_EDIT_CANCEL_ID, DOCUMENT_SIGNED_TEXT_EDIT_CONFIRM_ID, DeferredStartupOpen,
    DocumentWorkspace, NativeDocumentOpener, OpenDocumentRequest, StartupRecoveryAvailability,
    StartupRecoveryItem, document_annotation_layer_id, init_document_workspace_actions,
};
use butter_paper::page_geometry::{PageCoordinateSpace, PdfRect, Rotation};
use butter_paper::page_view_control::PageViewMode;
use butter_paper::pdf_text_edit::{
    PageTextEdit, PageTextLayer, TextEditLimits, TextEditRun, extract_page_text,
};
use butter_paper::pdf_worker::DocumentEdits;
use gpui::{AppContext as _, Modifiers, TestAppContext, point, px};
use gpui_component::Root;
use lopdf::{Document, Object, Stream, dictionary};
use sha2::{Digest as _, Sha256};

fn raster(width: u32, height: u32) -> RasterSurface {
    RasterSurface::new(
        width,
        height,
        vec![0xff; width as usize * height as usize * 4],
    )
    .unwrap()
}

/// One page with "Hello world" in 40pt Helvetica, baseline at y = 400.
fn page_text_layer() -> PageTextLayer {
    text_page(
        b"BT /F1 40 Tf 50 400 Td (Hello world) Tj ET",
        lopdf::Dictionary::new(),
    )
}

/// One page drawn by `content`, with Helvetica as `/F1` and `page` entries
/// added to the page dictionary.
fn text_page(content: &[u8], page: lopdf::Dictionary) -> PageTextLayer {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let content_id = document.add_object(Stream::new(dictionary! {}, content.to_vec()));
    let mut page_dictionary = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
    };
    for (key, value) in page.into_iter() {
        page_dictionary.set(key, value);
    }
    let page_id = document.add_object(page_dictionary);
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    extract_page_text(&document, 0, &TextEditLimits::default()).unwrap()
}

struct PageTextResource {
    layer: PageTextLayer,
    shown: Arc<Mutex<Vec<Vec<PageTextEdit>>>>,
    thumbnail_renders: Arc<AtomicUsize>,
    signed: bool,
    released: AtomicBool,
}

impl NativeDocumentResource for PageTextResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4300)
    }

    fn render_page(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
        if width == DEFAULT_THUMBNAIL_WIDTH {
            self.thumbnail_renders.fetch_add(1, Ordering::AcqRel);
        }
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

    fn digitally_signed(&self) -> bool {
        self.signed
    }

    fn page_text(&self, page_index: u32) -> Result<PageTextLayer, String> {
        assert_eq!(page_index, 0);
        Ok(self.layer.clone())
    }

    fn set_document_edits(&self, edits: &DocumentEdits) -> Result<(), String> {
        self.shown.lock().unwrap().push(edits.text.clone());
        Ok(())
    }

    fn close(&self) -> Result<(), String> {
        self.released.store(true, Ordering::Release);
        Ok(())
    }

    fn is_released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }
}

#[gpui::test]
fn edit_pdf_content_types_shortcut_letters_into_the_line_and_commits_one_undoable_edit(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(DocumentWorkspace::new);
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let shown = Arc::new(Mutex::new(Vec::new()));
    let thumbnail_renders = Arc::new(AtomicUsize::new(0));
    let signed = false;
    let resource = Arc::new(PageTextResource {
        layer: page_text_layer(),
        shown: shown.clone(),
        thumbnail_renders: thumbnail_renders.clone(),
        signed,
        released: AtomicBool::new(false),
    });
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("text.pdf"), cx)
    });
    let document_id = request.document_id;
    workspace.update(cx, |workspace, cx| {
        let opened = OpenedNativeDocument::new(
            "text.pdf",
            vec![(612., 792.)],
            raster(32, 40),
            vec![ThumbnailSurface::new(0, raster(8, 10))],
            resource,
        )
        .unwrap();
        workspace.apply_open_result(&request, Ok(opened), cx);
        workspace.set_view_configuration(document_id, PageViewMode::SinglePage, 100., cx);
        workspace
            .refresh_viewport_async(document_id, 800., 600., 1., cx)
            .unwrap();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_annotation_tool(document_id, AnnotationTool::EditPdfContent, cx)
        })
        .unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::EditPdfContent)
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));

    // Just after "Hello" (five 40pt Helvetica glyphs from x = 50), on the
    // baseline's upper half.
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx.debug_bounds(layer_id).unwrap();
    // The page is centred in its layer at a uniform scale.
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + (layer.size.width - px(612. * scale)) / 2.,
        layer.origin.y + (layer.size.height - px(792. * scale)) / 2.,
    );
    let hello_width = 0.556 * 3. + 0.222 * 2.;
    let at = point(
        origin.x + px((50. + 40. * hello_width as f32 + 1.) * scale),
        origin.y + px((792. - 410.) * scale),
    );
    cx.simulate_mouse_move(at, None, Modifiers::default());
    cx.run_until_parked();
    cx.simulate_mouse_move(at, None, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_click(at, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let renders_before = thumbnail_renders.load(Ordering::Acquire);
    // Every letter here is also a tool shortcut.
    cx.simulate_keystrokes("space g a l v a n i s e d backspace");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::EditPdfContent),
        "typed letters must not switch tools"
    );
    let live = shown.lock().unwrap().last().cloned().unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].text(), "Hello galvanise world");
    let renders_after_typing = thumbnail_renders.load(Ordering::Acquire);
    assert!(
        renders_after_typing > renders_before,
        "the page thumbnail shows the edited text"
    );
    assert!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(document_id, cx)
            .unwrap()
            .page_text_edits
            .is_empty()),
        "the line is recorded once editing finishes"
    );

    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let snapshot = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(document_id, cx).unwrap()
    });
    assert_eq!(snapshot.page_text_edits, live);
    assert!(snapshot.dirty);

    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace
            .annotation_snapshot(document_id, cx)
            .unwrap()
            .page_text_edits
            .is_empty()
    }));
    assert_eq!(
        shown.lock().unwrap().last().unwrap(),
        &Vec::<PageTextEdit>::new(),
        "undo restores the rendered page"
    );
    assert!(
        thumbnail_renders.load(Ordering::Acquire) > renders_after_typing,
        "undo restores the thumbnail"
    );
}

/// Opens one page of page text in Edit PDF Content, with the page's coordinate
/// space, and returns the workspace, the document and the edits the worker
/// was asked to show.
fn edit_pdf_content_workspace(
    cx: &mut TestAppContext,
    layer: PageTextLayer,
    space: PageCoordinateSpace,
    rotation: PageRotation,
    signed: bool,
) -> (
    gpui::Entity<DocumentWorkspace>,
    butter_paper::document_workspace::DocumentId,
    Arc<Mutex<Vec<Vec<PageTextEdit>>>>,
    &mut gpui::VisualTestContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(DocumentWorkspace::new);
            workspace_slot.replace(Some(workspace.clone()));
            let shell = cx.new(|_| DialogShell(workspace));
            Root::new(shell, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let shown = Arc::new(Mutex::new(Vec::new()));
    let thumbnail_renders = Arc::new(AtomicUsize::new(0));
    let resource = Arc::new(PageTextResource {
        layer,
        shown: shown.clone(),
        thumbnail_renders: thumbnail_renders.clone(),
        signed,
        released: AtomicBool::new(false),
    });
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("text.pdf"), cx)
    });
    let document_id = request.document_id;
    let size = space.display_size_points();
    workspace.update(cx, |workspace, cx| {
        let opened = OpenedNativeDocument::new(
            "text.pdf",
            vec![(size.0 as f32, size.1 as f32)],
            raster(32, 40),
            vec![ThumbnailSurface::new(0, raster(8, 10))],
            resource,
        )
        .unwrap()
        .with_page_rotations(vec![rotation])
        .with_page_coordinate_spaces(vec![space]);
        workspace.apply_open_result(&request, Ok(opened), cx);
        workspace.set_view_configuration(document_id, PageViewMode::SinglePage, 100., cx);
        workspace
            .refresh_viewport_async(document_id, 800., 600., 1., cx)
            .unwrap();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_annotation_tool(document_id, AnnotationTool::EditPdfContent, cx)
        })
        .unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (workspace, document_id, shown, cx)
}

/// Clicks page point (`x`, `y`) on a page shown with `space`, computing the
/// window position from the page's rotation, view box and user unit.
fn click_page_point(
    cx: &mut gpui::VisualTestContext,
    document_id: butter_paper::document_workspace::DocumentId,
    space: PageCoordinateSpace,
    rotation: Rotation,
    (x, y): (f64, f64),
) {
    let view = space.view_box();
    let (left, bottom) = (view.x, view.y);
    let (right, top) = (view.x + view.width, view.y + view.height);
    // Page points to display points, before scaling: the page is turned
    // clockwise by its rotation.
    let (local_x, local_y) = match rotation {
        Rotation::Degrees0 => (x - left, top - y),
        Rotation::Degrees90 => (y - bottom, x - left),
        Rotation::Degrees180 => (right - x, y - bottom),
        Rotation::Degrees270 => (top - y, right - x),
    };
    let unit = space.user_unit();
    let (display_width, display_height) = space.display_size_points();
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx.debug_bounds(layer_id).unwrap();
    let scale = (f32::from(layer.size.width) / display_width as f32)
        .min(f32::from(layer.size.height) / display_height as f32);
    let origin = point(
        layer.origin.x + (layer.size.width - px(display_width as f32 * scale)) / 2.,
        layer.origin.y + (layer.size.height - px(display_height as f32 * scale)) / 2.,
    );
    let at = point(
        origin.x + px((local_x * unit) as f32 * scale),
        origin.y + px((local_y * unit) as f32 * scale),
    );
    cx.simulate_mouse_move(at, None, Modifiers::default());
    cx.run_until_parked();
    cx.simulate_mouse_move(at, None, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_click(at, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

/// Edit PDF Content on a page with `rotation`, a crop box and a user unit: a click
/// just after "Hello" puts the caret there.
fn caret_lands_on_the_clicked_glyph(cx: &mut TestAppContext, rotation: Rotation, user_unit: f64) {
    let crop = PdfRect::new(100., 120., 400., 560.).unwrap();
    let mut page = lopdf::Dictionary::new();
    page.set(
        "CropBox",
        vec![100.into(), 120.into(), 500.into(), 680.into()],
    );
    let degrees = match rotation {
        Rotation::Degrees0 => 0,
        Rotation::Degrees90 => 90,
        Rotation::Degrees180 => 180,
        Rotation::Degrees270 => 270,
    };
    page.set("Rotate", degrees);
    page.set("UserUnit", user_unit);
    let layer = text_page(b"BT /F1 30 Tf 150 400 Td (Hello world) Tj ET", page);
    let space = PageCoordinateSpace::new(
        PdfRect::new(0., 0., 612., 792.).unwrap(),
        crop,
        rotation,
        user_unit,
    )
    .unwrap();
    let page_rotation = match rotation {
        Rotation::Degrees0 => PageRotation::Degrees0,
        Rotation::Degrees90 => PageRotation::Degrees90,
        Rotation::Degrees180 => PageRotation::Degrees180,
        Rotation::Degrees270 => PageRotation::Degrees270,
    };
    let (workspace, document_id, shown, cx) =
        edit_pdf_content_workspace(cx, layer, space, page_rotation, false);
    let hello_width = 0.556 * 3. + 0.222 * 2.;
    click_page_point(
        cx,
        document_id,
        space,
        rotation,
        (150. + 30. * hello_width + 0.5, 410.),
    );
    cx.simulate_keystrokes("X");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let live = shown.lock().unwrap().last().cloned();
    assert_eq!(
        live.map(|edits| edits.iter().map(PageTextEdit::text).collect::<Vec<_>>()),
        Some(vec!["HelloX world".to_owned()]),
        "{rotation:?}, user unit {user_unit}"
    );
    drop(workspace);
}

#[gpui::test]
fn caret_lands_on_the_clicked_glyph_on_a_cropped_page_with_a_user_unit(cx: &mut TestAppContext) {
    caret_lands_on_the_clicked_glyph(cx, Rotation::Degrees0, 2.);
}

#[gpui::test]
fn caret_lands_on_the_clicked_glyph_on_a_page_rotated_90(cx: &mut TestAppContext) {
    caret_lands_on_the_clicked_glyph(cx, Rotation::Degrees90, 1.);
}

#[gpui::test]
fn caret_lands_on_the_clicked_glyph_on_a_page_rotated_180(cx: &mut TestAppContext) {
    caret_lands_on_the_clicked_glyph(cx, Rotation::Degrees180, 1.);
}

#[gpui::test]
fn caret_lands_on_the_clicked_glyph_on_a_page_rotated_270(cx: &mut TestAppContext) {
    caret_lands_on_the_clicked_glyph(cx, Rotation::Degrees270, 1.);
}

/// A worker that records, in order, the edits it is asked to show and the
/// tiles it renders.
struct RecordingTextResource {
    layer: PageTextLayer,
    events: Arc<Mutex<Vec<String>>>,
}

impl NativeDocumentResource for RecordingTextResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4301)
    }

    fn render_page(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
        Ok(raster(width, page_index + 2))
    }

    fn render_tile(
        &self,
        request: butter_paper::viewer::TileRequest,
    ) -> Result<RasterSurface, String> {
        self.events.lock().unwrap().push("tile".into());
        Ok(raster(
            request.crop.width as u32,
            request.crop.height as u32,
        ))
    }

    fn page_text(&self, _: u32) -> Result<PageTextLayer, String> {
        Ok(self.layer.clone())
    }

    fn set_document_edits(&self, edits: &DocumentEdits) -> Result<(), String> {
        self.events.lock().unwrap().push(format!(
            "edits {:?}",
            edits
                .text
                .iter()
                .map(PageTextEdit::text)
                .collect::<Vec<_>>()
        ));
        Ok(())
    }

    fn close(&self) -> Result<(), String> {
        Ok(())
    }

    fn is_released(&self) -> bool {
        false
    }
}

struct RecordingTextOpener {
    sha256: [u8; 32],
    layer: PageTextLayer,
    events: Arc<Mutex<Vec<String>>>,
}

impl NativeDocumentOpener for RecordingTextOpener {
    fn open(&self, _: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        Ok(OpenedNativeDocument::new(
            "recovered.pdf",
            vec![(612., 792.)],
            raster(32, 40),
            vec![ThumbnailSurface::new(0, raster(8, 10))],
            Arc::new(RecordingTextResource {
                layer: self.layer.clone(),
                events: self.events.clone(),
            }),
        )?
        .with_source_sha256(self.sha256))
    }
}

#[gpui::test]
fn recovered_text_edits_are_shown_and_stay_undoable(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let root = std::env::temp_dir().join(format!(
        "bp-text-edit-recovery-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir(&root).unwrap();
    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _scratch = Scratch(root.clone());
    let source = root.join("edited.pdf");
    let base_pdf = b"%PDF-1.7\ntext edit recovery base\n%%EOF\n";
    let source_sha256: [u8; 32] = Sha256::digest(base_pdf).into();

    // An unsaved session that changed "Hello world".
    let layer = page_text_layer();
    let line = &layer.lines[0];
    let edit = PageTextEdit {
        page_index: 0,
        anchor: line.anchor,
        original: line.original_runs(),
        runs: vec![TextEditRun {
            style: line.styles[0].clone(),
            text: "Hello recovered world".into(),
        }],
        layout: Default::default(),
    };
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations_with_document_state(
            1,
            Vec::new(),
            Vec::new(),
            vec![(0, PageRotation::Degrees0)],
        )
        .unwrap();
    assert!(
        adapter
            .set_document_page_text_edit(1, edit.clone())
            .unwrap()
    );
    let timeline = adapter.encode_document_recovery_timeline(1).unwrap();
    let store = Arc::new(DocumentRecoveryStore::open(&root).unwrap());
    let id = store
        .publish_new(&RecoveryPublication {
            source_path: &source,
            source_kind: RecoverySourceKind::Opened,
            source_sha256,
            base_pdf,
            timeline: &timeline,
            current_revision: 1,
            saved_revision: 0,
            requires_save_as: false,
        })
        .unwrap();
    let recovered = store.load(id).unwrap().unwrap();

    let events = Arc::new(Mutex::new(Vec::new()));
    let opener = Arc::new(RecordingTextOpener {
        sha256: source_sha256,
        layer,
        events: events.clone(),
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(opener, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    workspace.update(cx, |workspace, cx| {
        workspace.bind_document_recovery_store(store.clone());
        workspace.defer_startup_open(DeferredStartupOpen::None);
        workspace.finish_startup_recovery_inspection(
            vec![StartupRecoveryItem {
                id,
                authority: Some(recovered.authority),
                source_path: Some(source),
                current_revision: Some(1),
                saved_revision: Some(0),
                availability: StartupRecoveryAvailability::OpenedSourceNeedsVerification,
            }],
            cx,
        );
        workspace.recover_startup_opened_source(id, cx);
    });
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    let document_id =
        workspace.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    let snapshot = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(document_id, cx).unwrap()
    });
    assert!(snapshot.dirty);
    assert_eq!(snapshot.page_text_edits, vec![edit]);
    let shown = events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| event.starts_with("edits"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        shown.last().map(String::as_str),
        Some("edits [\"Hello recovered world\"]"),
        "the recovered edit is shown"
    );
    // The document opens with the edit applied, before anything is drawn.
    assert_eq!(
        events.lock().unwrap().first().map(String::as_str),
        Some("edits [\"Hello recovered world\"]")
    );

    // Undo reaches the unedited text.
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    for _ in 0..2 {
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .rfind(|event| event.starts_with("edits"))
            .map(String::as_str),
        Some("edits []")
    );
}

#[gpui::test]
fn editing_a_signed_pdf_asks_first(cx: &mut TestAppContext) {
    let layer = page_text_layer();
    let space = PageCoordinateSpace::new(
        PdfRect::new(0., 0., 612., 792.).unwrap(),
        PdfRect::new(0., 0., 612., 792.).unwrap(),
        Rotation::Degrees0,
        1.,
    )
    .unwrap();
    let (_workspace, document_id, shown, cx) =
        edit_pdf_content_workspace(cx, layer, space, PageRotation::Degrees0, true);
    let hello_width = 0.556 * 3. + 0.222 * 2.;
    let at = (50. + 40. * hello_width + 1., 410.);
    click_page_point(cx, document_id, space, Rotation::Degrees0, at);
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(DOCUMENT_SIGNED_TEXT_EDIT_CONFIRM_ID)
            .is_some(),
        "a signed PDF asks before its text is edited"
    );

    // Cancelling leaves the text alone, and the next press asks again.
    let cancel = cx
        .debug_bounds(DOCUMENT_SIGNED_TEXT_EDIT_CANCEL_ID)
        .unwrap();
    cx.simulate_click(cancel.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(DOCUMENT_SIGNED_TEXT_EDIT_CONFIRM_ID)
            .is_none()
    );
    assert!(shown.lock().unwrap().is_empty());
    click_page_point(cx, document_id, space, Rotation::Degrees0, at);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let confirm = cx
        .debug_bounds(DOCUMENT_SIGNED_TEXT_EDIT_CONFIRM_ID)
        .expect("asked again");

    // After confirming, a click edits the text.
    cx.simulate_click(confirm.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(DOCUMENT_SIGNED_TEXT_EDIT_CONFIRM_ID)
            .is_none()
    );
    click_page_point(cx, document_id, space, Rotation::Degrees0, at);
    cx.simulate_keystrokes("X");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert_eq!(
        shown.lock().unwrap().last().map(|edits| edits[0].text()),
        Some("HelloX world".to_owned())
    );
}

/// Renders the workspace with the dialog layer, as the application shell does.
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

#[gpui::test]
fn input_methods_compose_then_commit_into_the_line(cx: &mut TestAppContext) {
    let layer = page_text_layer();
    let space = PageCoordinateSpace::new(
        PdfRect::new(0., 0., 612., 792.).unwrap(),
        PdfRect::new(0., 0., 612., 792.).unwrap(),
        Rotation::Degrees0,
        1.,
    )
    .unwrap();
    let (workspace, document_id, shown, cx) =
        edit_pdf_content_workspace(cx, layer, space, PageRotation::Degrees0, false);
    let hello_width = 0.556 * 3. + 0.222 * 2.;
    click_page_point(
        cx,
        document_id,
        space,
        Rotation::Degrees0,
        (50. + 40. * hello_width + 1., 410.),
    );
    let input_method = |text: &str, compose: bool, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                assert!(workspace.page_text_input_method(text, compose, window, cx));
            })
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        (
            workspace.read_with(cx, |workspace, cx| workspace.page_text_composing(cx)),
            shown.lock().unwrap().last().map(|edits| edits[0].text()),
        )
    };
    // A pinyin input method composes Latin letters, then commits the
    // character.
    assert_eq!(
        input_method("zh", true, cx),
        (Some("zh".into()), Some("Hellozh world".into()))
    );
    assert_eq!(
        input_method("zhong", true, cx),
        (Some("zhong".into()), Some("Hellozhong world".into()))
    );
    assert_eq!(
        input_method("中", false, cx),
        (None, Some("Hello中 world".into()))
    );
    // A dead key composes the accent, then the letter replaces it.
    assert_eq!(
        input_method("´", true, cx),
        (Some("´".into()), Some("Hello中´ world".into()))
    );
    assert_eq!(
        input_method("é", false, cx),
        (None, Some("Hello中é world".into()))
    );
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let snapshot = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(document_id, cx).unwrap()
    });
    assert_eq!(snapshot.page_text_edits[0].text(), "Hello中é world");
}

#[gpui::test]
fn typing_in_a_paragraph_rewraps_its_lines_as_one_undo_step(cx: &mut TestAppContext) {
    // Three left-aligned lines, 14 points apart.
    let layer = text_page(
        b"BT /F1 12 Tf 72 700 Td (The quick brown fox) Tj 0 -14 Td (jumps over the lazy) Tj 0 -14 Td (dog today.) Tj ET",
        lopdf::Dictionary::new(),
    );
    let space = PageCoordinateSpace::new(
        PdfRect::new(0., 0., 612., 792.).unwrap(),
        PdfRect::new(0., 0., 612., 792.).unwrap(),
        Rotation::Degrees0,
        1.,
    )
    .unwrap();
    let (workspace, document_id, shown, cx) =
        edit_pdf_content_workspace(cx, layer, space, PageRotation::Degrees0, false);
    // Just after "The", on the first line.
    click_page_point(
        cx,
        document_id,
        space,
        Rotation::Degrees0,
        (72. + 12. * 1.723 + 0.3, 704.),
    );
    cx.simulate_keystrokes("space v e r y space v e r y");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let live = shown.lock().unwrap().last().cloned().unwrap();
    assert!(live.len() >= 2, "words move onto later lines: {live:#?}");
    // Top to bottom.
    let mut lines = live.clone();
    lines.sort_by_key(|edit| std::cmp::Reverse(edit.anchor.y_centipoints));
    assert_eq!(lines[0].text(), "The very very quick");
    let read = lines
        .iter()
        .flat_map(|edit| {
            std::iter::once(edit.text()).chain(edit.layout.extra_lines.iter().map(|extra| {
                extra
                    .runs
                    .iter()
                    .map(|run| run.text.as_str())
                    .collect::<String>()
            }))
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(read.ends_with("lazy dog today."), "{read}");

    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let snapshot = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(document_id, cx).unwrap()
    });
    assert_eq!(snapshot.page_text_edits, live);
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace
            .annotation_snapshot(document_id, cx)
            .unwrap()
            .page_text_edits
            .is_empty()
    }));
}

/// Clicks the centre of the element with debug selector `id`.
fn click_selector(cx: &mut gpui::VisualTestContext, id: &'static str) {
    let bounds = cx
        .debug_bounds(id)
        .unwrap_or_else(|| panic!("{id} is not drawn"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

#[gpui::test]
fn text_properties_restyle_the_selection_and_align_the_paragraph(cx: &mut TestAppContext) {
    use butter_paper::edit_pdf_content_property_inspector::{
        EDIT_PDF_CONTENT_INSPECTOR_ALIGN_RIGHT_ID, EDIT_PDF_CONTENT_INSPECTOR_BOLD_ID,
        EDIT_PDF_CONTENT_INSPECTOR_SIZE_ID, EDIT_PDF_CONTENT_PROPERTY_INSPECTOR_ID,
    };
    use butter_paper::pdf_text_blocks::TextBlockAlignment;
    let layer = text_page(
        b"BT /F1 12 Tf 72 700 Td (The quick brown fox) Tj 0 -14 Td (jumps over the lazy) Tj 0 -14 Td (dog today.) Tj ET",
        lopdf::Dictionary::new(),
    );
    let space = PageCoordinateSpace::new(
        PdfRect::new(0., 0., 612., 792.).unwrap(),
        PdfRect::new(0., 0., 612., 792.).unwrap(),
        Rotation::Degrees0,
        1.,
    )
    .unwrap();
    let (workspace, document_id, shown, cx) =
        edit_pdf_content_workspace(cx, layer, space, PageRotation::Degrees0, false);
    assert!(
        cx.debug_bounds(EDIT_PDF_CONTENT_PROPERTY_INSPECTOR_ID)
            .is_none()
    );
    // Just before "quick", then select it.
    click_page_point(
        cx,
        document_id,
        space,
        Rotation::Degrees0,
        (72. + 12. * 2.001 + 0.3, 704.),
    );
    cx.simulate_keystrokes("shift-right shift-right shift-right shift-right shift-right");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    // Editing text does not open the properties; the properties button does,
    // as for other tools.
    assert!(
        cx.debug_bounds(EDIT_PDF_CONTENT_PROPERTY_INSPECTOR_ID)
            .is_none()
    );
    click_selector(cx, "document-workspace-rail-actions");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let snapshot = workspace.read_with(cx, |workspace, cx| {
        workspace
            .edit_pdf_content_property_inspector()
            .and_then(|inspector| inspector.read(cx).snapshot().cloned())
    });
    let snapshot = snapshot.expect("the opened inspector shows the edited text");
    assert_eq!(snapshot.family, None);
    assert_eq!(snapshot.size, 12.);
    assert!(!snapshot.bold && snapshot.selection);
    assert_eq!(snapshot.alignment, TextBlockAlignment::Left);

    // Bold applies to the selected word only, in the bundled Helvetica match.
    click_selector(cx, EDIT_PDF_CONTENT_INSPECTOR_BOLD_ID);
    let live = shown.lock().unwrap().last().cloned().unwrap();
    let first = live
        .iter()
        .find(|edit| edit.text().starts_with("The"))
        .unwrap();
    let bold = first
        .runs
        .iter()
        .find(|run| run.text == "quick")
        .unwrap_or_else(|| panic!("{:#?}", first.runs));
    let face = bold.style.face.as_ref().unwrap();
    assert!(face.fallback.bold && face.postscript_name.is_none());
    assert!(
        first
            .runs
            .iter()
            .all(|run| run.text == "quick" || run.style.face.is_none())
    );

    // A size typed into the inspector applies on Enter.
    let size = cx.debug_bounds(EDIT_PDF_CONTENT_INSPECTOR_SIZE_ID).unwrap();
    cx.simulate_click(size.center(), Modifiers::default());
    cx.simulate_keystrokes("secondary-a 1 8 enter");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let live = shown.lock().unwrap().last().cloned().unwrap();
    let sizes = live
        .iter()
        .flat_map(|edit| &edit.runs)
        .filter(|run| run.text.contains("quick"))
        .map(|run| run.style.font_size)
        .collect::<Vec<_>>();
    assert_eq!(sizes, vec![18.], "{live:#?}");

    // Alignment applies to the whole paragraph and is kept with it.
    click_selector(cx, EDIT_PDF_CONTENT_INSPECTOR_ALIGN_RIGHT_ID);
    let live = shown.lock().unwrap().last().cloned().unwrap();
    let first = live
        .iter()
        .max_by_key(|edit| edit.anchor.y_centipoints)
        .unwrap();
    assert_eq!(
        first.layout.paragraph.alignment,
        Some(TextBlockAlignment::Right)
    );
    assert!(
        live.iter()
            .filter(|edit| !edit.text().is_empty())
            .all(|edit| edit.layout.fit.shift > 0.),
        "{live:#?}"
    );

    // One undoable change when editing ends.
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    // The properties stay open, asking for text to edit.
    assert!(
        cx.debug_bounds(EDIT_PDF_CONTENT_PROPERTY_INSPECTOR_ID)
            .is_some()
    );
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace
            .edit_pdf_content_property_inspector()
            .is_some_and(|inspector| inspector.read(cx).snapshot().is_none())
    }));
    let saved = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(document_id, cx).unwrap()
    });
    assert_eq!(saved.page_text_edits, live);
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace
            .annotation_snapshot(document_id, cx)
            .unwrap()
            .page_text_edits
            .is_empty()
    }));
}
