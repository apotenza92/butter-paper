//! Pages arranged from Page Thumbnails: added, deleted, moved, duplicated,
//! copied and pasted, each as one undoable change that takes the page's
//! markups along.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use butter_paper::annotation_model::{MarkupId, PdfPoint};
use butter_paper::document_resource::{
    NativeDocumentResource, OpenedNativeDocument, RasterSurface, ThumbnailSurface,
};
use butter_paper::document_workspace::{
    DocumentId, DocumentWorkspace, MergeSource, init_document_workspace_actions,
};
use butter_paper::page_view_control::PageViewMode;
use butter_paper::pdf_page_structure::PageSlot;
use butter_paper::pdf_text_edit::PageTextLayer;
use butter_paper::pdf_worker::DocumentEdits;
use gpui::{AppContext as _, TestAppContext};
use gpui_component::Root;

/// Page sizes, each page its own: A is portrait, B landscape, C square.
const SIZES: [(f32, f32); 3] = [(612., 792.), (792., 612.), (500., 500.)];

fn raster(width: u32, height: u32) -> RasterSurface {
    RasterSurface::new(
        width,
        height,
        vec![0xff; width as usize * height as usize * 4],
    )
    .unwrap()
}

#[derive(Default)]
struct Recording {
    released: AtomicBool,
    edits: Mutex<Vec<DocumentEdits>>,
}

struct PagesResource(Arc<Recording>);

impl NativeDocumentResource for PagesResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4303)
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

    fn page_text(&self, page_index: u32) -> Result<PageTextLayer, String> {
        Ok(PageTextLayer {
            page_index,
            ..Default::default()
        })
    }

    fn set_document_edits(&self, edits: &DocumentEdits) -> Result<(), String> {
        self.0.edits.lock().unwrap().push(edits.clone());
        Ok(())
    }

    fn close(&self) -> Result<(), String> {
        self.0.released.store(true, Ordering::Release);
        Ok(())
    }

    fn is_released(&self) -> bool {
        self.0.released.load(Ordering::Acquire)
    }
}

struct Opened<'a> {
    workspace: gpui::Entity<DocumentWorkspace>,
    document_id: DocumentId,
    recording: Arc<Recording>,
    cx: &'a mut gpui::VisualTestContext,
}

/// A workspace showing a three-page document, with a rectangle on each
/// page named after it.
fn three_pages(cx: &mut TestAppContext) -> Opened<'_> {
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("pages.pdf"), cx)
    });
    let document_id = request.document_id;
    let recording = Arc::new(Recording::default());
    workspace.update(cx, |workspace, cx| {
        let opened = OpenedNativeDocument::new(
            "pages.pdf",
            SIZES.to_vec(),
            raster(32, 40),
            (0..3)
                .map(|page| ThumbnailSurface::new(page, raster(8 + page, 10)))
                .collect(),
            Arc::new(PagesResource(recording.clone())),
        )
        .unwrap();
        workspace.apply_open_result(&request, Ok(opened), cx);
        workspace.set_view_configuration(document_id, PageViewMode::SinglePage, 100., cx);
        for (page, name) in ["a", "b", "c"].into_iter().enumerate() {
            workspace
                .create_rectangle(
                    document_id,
                    page as u32,
                    MarkupId::new(name).unwrap(),
                    PdfPoint { x: 10., y: 10. },
                    PdfPoint { x: 60., y: 60. },
                    cx,
                )
                .unwrap();
        }
    });
    cx.run_until_parked();
    Opened {
        workspace,
        document_id,
        recording,
        cx,
    }
}

impl Opened<'_> {
    /// Each rectangle's page, by name.
    fn markups(&mut self) -> Vec<(String, u32)> {
        let mut markups = self
            .workspace
            .read_with(self.cx, |workspace, cx| {
                workspace.annotation_snapshot(self.document_id, cx)
            })
            .unwrap()
            .rectangles
            .iter()
            .map(|rectangle| (rectangle.id.as_str().to_owned(), rectangle.page_index))
            .collect::<Vec<_>>();
        markups.sort_by_key(|(_, page)| *page);
        markups
    }

    fn sizes(&mut self) -> Vec<(f32, f32)> {
        let document_id = self.document_id;
        self.workspace.read_with(self.cx, |workspace, cx| {
            let session = workspace.session(document_id, cx).unwrap().read(cx);
            (0..).map_while(|page| session.page_size(page)).collect()
        })
    }

    fn current_page(&mut self) -> u32 {
        let document_id = self.document_id;
        self.workspace.read_with(self.cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .unwrap()
                .read(cx)
                .current_page()
        })
    }

    fn thumbnail_widths(&mut self) -> Vec<Option<u32>> {
        let document_id = self.document_id;
        self.workspace.read_with(self.cx, |workspace, cx| {
            let session = workspace.session(document_id, cx).unwrap().read(cx);
            (0..)
                .map_while(|page| session.page_size(page).map(|_| page))
                .map(|page| {
                    session
                        .thumbnail_base_raster(page)
                        .map(|raster| raster.width())
                })
                .collect()
        })
    }

    fn page_slots(&mut self) -> Vec<PageSlot> {
        self.workspace
            .read_with(self.cx, |workspace, cx| {
                workspace.annotation_snapshot(self.document_id, cx)
            })
            .unwrap()
            .page_slots
    }

    fn update<T>(
        &mut self,
        change: impl FnOnce(
            &mut DocumentWorkspace,
            DocumentId,
            &mut gpui::Context<DocumentWorkspace>,
        ) -> T,
    ) -> T {
        let document_id = self.document_id;
        let result = self
            .workspace
            .update(self.cx, |workspace, cx| change(workspace, document_id, cx));
        self.cx.run_until_parked();
        result
    }
}

#[gpui::test]
fn moved_pages_take_their_size_thumbnail_and_markups(cx: &mut TestAppContext) {
    let mut opened = three_pages(cx);
    opened
        .update(|workspace, document_id, cx| workspace.move_pages(document_id, &[2], 0, cx))
        .unwrap();
    assert_eq!(opened.sizes(), [SIZES[2], SIZES[0], SIZES[1]]);
    assert_eq!(
        opened.markups(),
        [("c".into(), 0), ("a".into(), 1), ("b".into(), 2)]
    );
    // Thumbnails follow their pages: page C's was 10 pixels wide.
    assert_eq!(opened.thumbnail_widths(), [Some(10), Some(8), Some(9)]);
    assert_eq!(opened.current_page(), 0, "the moved page is shown");
    assert_eq!(
        opened.page_slots(),
        [
            PageSlot::Source { index: 2 },
            PageSlot::Source { index: 0 },
            PageSlot::Source { index: 1 },
        ]
    );
    // The worker is sent the new order to draw.
    let sent = opened
        .recording
        .edits
        .lock()
        .unwrap()
        .last()
        .cloned()
        .unwrap();
    assert_eq!(sent.pages, opened.page_slots());

    // One undo puts everything back.
    opened
        .update(|workspace, document_id, cx| workspace.undo_annotations(document_id, cx))
        .unwrap();
    assert_eq!(opened.sizes(), SIZES);
    assert_eq!(
        opened.markups(),
        [("a".into(), 0), ("b".into(), 1), ("c".into(), 2)]
    );
    assert_eq!(opened.thumbnail_widths(), [Some(8), Some(9), Some(10)]);
    assert!(opened.page_slots().is_empty());
    opened
        .update(|workspace, document_id, cx| workspace.redo_annotations(document_id, cx))
        .unwrap();
    assert_eq!(opened.sizes(), [SIZES[2], SIZES[0], SIZES[1]]);
}

#[gpui::test]
fn a_blank_page_is_added_the_size_of_its_neighbour(cx: &mut TestAppContext) {
    let mut opened = three_pages(cx);
    opened
        .update(|workspace, document_id, cx| workspace.insert_blank_page(document_id, 1, cx))
        .unwrap();
    assert_eq!(opened.sizes(), [SIZES[0], SIZES[1], SIZES[1], SIZES[2]]);
    assert_eq!(opened.current_page(), 1, "the new page is shown");
    assert!(
        opened.thumbnail_widths()[1].is_some(),
        "a blank page shows white at once"
    );
    assert_eq!(
        opened.markups(),
        [("a".into(), 0), ("b".into(), 2), ("c".into(), 3)]
    );
    // At the end.
    opened
        .update(|workspace, document_id, cx| workspace.insert_blank_page(document_id, 4, cx))
        .unwrap();
    assert_eq!(opened.sizes().len(), 5);
    assert_eq!(opened.current_page(), 4);
}

#[gpui::test]
fn deleted_pages_lose_their_markups_but_the_last_page_stays(cx: &mut TestAppContext) {
    let mut opened = three_pages(cx);
    opened
        .update(|workspace, document_id, cx| workspace.delete_pages(document_id, &[0, 2], cx))
        .unwrap();
    assert_eq!(opened.sizes(), [SIZES[1]]);
    assert_eq!(opened.markups(), [("b".into(), 0)]);
    let refused =
        opened.update(|workspace, document_id, cx| workspace.delete_pages(document_id, &[0], cx));
    assert!(refused.is_err(), "a document keeps one page");
    opened
        .update(|workspace, document_id, cx| workspace.undo_annotations(document_id, cx))
        .unwrap();
    assert_eq!(opened.markups().len(), 3);
}

#[gpui::test]
fn duplicated_pages_have_copies_of_their_markups(cx: &mut TestAppContext) {
    let mut opened = three_pages(cx);
    opened
        .update(|workspace, document_id, cx| workspace.duplicate_pages(document_id, &[0], cx))
        .unwrap();
    assert_eq!(opened.sizes(), [SIZES[0], SIZES[0], SIZES[1], SIZES[2]]);
    let markups = opened.markups();
    assert_eq!(markups.len(), 4);
    assert_eq!(markups[0], ("a".into(), 0));
    assert_eq!(markups[1].1, 1, "the copy has a copy of the markup");
    assert_ne!(markups[1].0, "a", "under a name of its own");
    assert_eq!(
        opened.page_slots()[1],
        PageSlot::Copy { index: 0 },
        "the copy is drawn from the same file page"
    );
    assert_eq!(opened.current_page(), 1);
}

#[gpui::test]
fn cut_pages_move_where_they_are_pasted_and_copies_paste_again(cx: &mut TestAppContext) {
    let mut opened = three_pages(cx);
    opened.update(|workspace, document_id, cx| workspace.copy_pages(document_id, &[0], true, cx));
    opened
        .update(|workspace, document_id, cx| workspace.paste_pages(document_id, 2, cx))
        .unwrap();
    assert_eq!(
        opened.markups(),
        [("b".into(), 0), ("c".into(), 1), ("a".into(), 2)]
    );
    let again =
        opened.update(|workspace, document_id, cx| workspace.paste_pages(document_id, 0, cx));
    assert!(again.is_err(), "cut pages paste once");

    opened.update(|workspace, document_id, cx| workspace.copy_pages(document_id, &[1], false, cx));
    for _ in 0..2 {
        opened
            .update(|workspace, document_id, cx| workspace.paste_pages(document_id, 0, cx))
            .unwrap();
    }
    assert_eq!(opened.sizes().len(), 5);
    assert_eq!(
        opened.sizes(),
        [SIZES[1], SIZES[2], SIZES[2], SIZES[2], SIZES[0]]
    );

    // Rearranging the pages since forgets what was copied.
    opened
        .update(|workspace, document_id, cx| workspace.move_pages(document_id, &[4], 0, cx))
        .unwrap();
    let stale =
        opened.update(|workspace, document_id, cx| workspace.paste_pages(document_id, 0, cx));
    assert!(stale.is_err());
}

struct StubOpener;

impl butter_paper::document_workspace::NativeDocumentOpener for StubOpener {
    fn open(
        &self,
        request: &butter_paper::document_workspace::OpenDocumentRequest,
    ) -> Result<OpenedNativeDocument, String> {
        let pages = lopdf::Document::load(&request.path)
            .map_err(|error| error.to_string())?
            .get_pages()
            .len() as u32;
        OpenedNativeDocument::new(
            "saved.pdf",
            vec![(612., 792.); pages as usize],
            raster(4, 4),
            (0..pages)
                .map(|page| ThumbnailSurface::new(page, raster(4, 4)))
                .collect(),
            Arc::new(PagesResource(Arc::default())),
        )
    }
}

/// A file's pages saved rearranged: reordered, with a copy and a blank
/// page, each page keeping its own markups and a copy getting copies.
#[test]
fn a_save_writes_the_pages_as_arranged() {
    use butter_paper::annotation_adapter::AnnotationAdapter;
    use butter_paper::annotation_model::ArrangedPage;
    use butter_paper::document_workspace::{
        NativeDocumentSaver as _, PdfDocumentSaver, SaveDestination, SaveDocumentRequest,
    };
    use butter_paper::pdf_engine::PdfPersistenceSession;
    use butter_paper::pdf_file_authority::SaveAsTargetAuthority;

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = manifest.join("tests/fixtures/bp-annotation-all-v1.pdf");
    let scratch = manifest.join("target/test-scratch");
    std::fs::create_dir_all(&scratch).unwrap();
    let target = std::fs::canonicalize(&scratch)
        .unwrap()
        .join(format!("arranged-pages-{}.pdf", std::process::id()));
    let _ = std::fs::remove_file(&target);

    let file = PdfPersistenceSession::open(&source).unwrap();
    let count = file.page_count() as u32;
    assert!(count >= 2, "the fixture has pages to rearrange");
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations_with_page_scale_state(
            1,
            file.annotations_in_document_order(),
            file.page_scales()
                .iter()
                .map(|scale| (scale.page_index, scale.clone()))
                .collect(),
            file.page_rotations()
                .iter()
                .map(|(page, rotation)| (*page, *rotation))
                .collect(),
        )
        .unwrap();
    // The last page first, then the others, a copy of the first and a
    // blank page.
    let mut arranged = vec![ArrangedPage::Shown {
        index: count - 1,
        copy: false,
    }];
    arranged.extend((0..count - 1).map(|index| ArrangedPage::Shown { index, copy: false }));
    arranged.push(ArrangedPage::Shown {
        index: 0,
        copy: true,
    });
    arranged.push(ArrangedPage::Blank {
        width: 300.,
        height: 200.,
    });
    adapter.arrange_document_pages(1, count, &arranged).unwrap();
    let snapshot = adapter.snapshot(1).unwrap();
    let originals = file.annotations_in_document_order();
    // Each markup where its page went, and the copies on the copy.
    let mut expected = originals
        .iter()
        .map(|annotation| {
            let page = annotation.page_index();
            let moved = if page == count - 1 { 0 } else { page + 1 };
            (annotation.id().clone(), moved)
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    for id in &snapshot.annotation_order {
        expected.entry(id.clone()).or_insert(count);
    }

    PdfDocumentSaver::new(Arc::new(StubOpener))
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(7),
            generation: 1,
            source_path: source.clone(),
            destination: SaveDestination::NewTarget(
                SaveAsTargetAuthority::bind(target.clone(), &source).unwrap(),
            ),
            current_page: 0,
            annotation_revision: snapshot.revision,
            annotations: snapshot,
            expected_source_sha256: None,
        })
        .unwrap();

    let saved = PdfPersistenceSession::open(&target).unwrap();
    assert_eq!(saved.page_count() as u32, count + 2);
    let saved_pages = saved
        .annotations_in_document_order()
        .into_iter()
        .map(|annotation| (annotation.id().clone(), annotation.page_index()))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(saved_pages, expected, "each markup is on its page");
    // Every markup of the first page is copied onto the copy.
    let on_first = originals
        .iter()
        .filter(|annotation| annotation.page_index() == 0)
        .count();
    let on_copy = saved_pages.values().filter(|page| **page == count).count();
    assert_eq!(on_copy, on_first);
    // The pages' content is the pages': the copy draws as the first.
    let read = |path: &std::path::Path| lopdf::Document::load(path).unwrap();
    let (before, after) = (read(&source), read(&target));
    let content = |document: &lopdf::Document, page: u32| {
        let id = document.get_pages()[&(page + 1)];
        document.get_page_content(id)
    };
    assert_eq!(content(&after, 0), content(&before, count - 1));
    assert_eq!(content(&after, 1), content(&before, 0));
    assert_eq!(content(&after, count), content(&before, 0));
    let blank = after
        .get_dictionary(after.get_pages()[&(count + 2)])
        .unwrap();
    let media_box = blank.get(b"MediaBox").unwrap().as_array().unwrap();
    assert_eq!(media_box[2].as_float().unwrap(), 300.);
    std::fs::remove_file(target).unwrap();
}

/// Opens any PDF as its page count of letter pages.
struct CountingOpener;

impl butter_paper::document_workspace::NativeDocumentOpener for CountingOpener {
    fn open(
        &self,
        request: &butter_paper::document_workspace::OpenDocumentRequest,
    ) -> Result<OpenedNativeDocument, String> {
        let pages = lopdf::Document::load(&request.path)
            .map_err(|error| error.to_string())?
            .get_pages()
            .len() as u32;
        OpenedNativeDocument::new(
            request.path.file_name().unwrap().to_string_lossy(),
            vec![(612., 792.); pages as usize],
            raster(4, 4),
            (0..pages)
                .map(|page| ThumbnailSurface::new(page, raster(4, 4)))
                .collect(),
            Arc::new(PagesResource(Arc::default())),
        )
    }
}

#[gpui::test]
fn merge_documents_lists_open_and_dropped_pdfs_and_merges_them_in_order(cx: &mut TestAppContext) {
    use butter_paper::generated_document::GeneratedDocumentStore;
    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join(format!("target/test-scratch/merge-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let store = GeneratedDocumentStore::new(root.join("generated")).unwrap();
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| {
                DocumentWorkspace::with_opener_and_generated_store(
                    Arc::new(CountingOpener),
                    store,
                    cx,
                )
            });
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let files = ["bp-multi-page-v1.pdf", "bp-annotation-all-v1.pdf"]
        .map(|name| manifest.join("tests/fixtures").join(name));
    let pages = files
        .iter()
        .map(|file| lopdf::Document::load(file).unwrap().get_pages().len())
        .sum::<usize>();
    let ids = files
        .clone()
        .map(|file| workspace.update(cx, |workspace, cx| workspace.open_path(file, cx)));
    cx.run_until_parked();
    // Merge Documents lists the open documents, ticked, in tab order; a
    // PDF dropped on the window while it is open joins the list.
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| workspace.open_merge_dialog(window, cx))
    });
    let third = manifest.join("tests/fixtures/bp-coordinate-space-v1.pdf");
    let pages = pages + lopdf::Document::load(&third).unwrap().get_pages().len();
    let opened_before = workspace.read_with(cx, |workspace, _| workspace.sessions().len());
    workspace.update(cx, |workspace, cx| {
        workspace.drop_paths(
            vec![third.clone()],
            gpui::point(gpui::px(10.), gpui::px(10.)),
            cx,
        )
    });
    cx.run_until_parked();
    let dialog = workspace
        .read_with(cx, |workspace, _| workspace.merge_dialog())
        .expect("the dialog is open");
    let chosen = dialog.read_with(cx, |dialog, _| dialog.chosen());
    assert_eq!(
        chosen,
        vec![
            MergeSource::Open(ids[0]),
            MergeSource::Open(ids[1]),
            MergeSource::File(third),
        ]
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        opened_before,
        "a PDF dropped on the dialog is listed, not opened"
    );
    workspace
        .update(cx, |workspace, cx| workspace.merge_sources(chosen, cx))
        .unwrap();
    cx.run_until_parked();
    let merged = workspace.read_with(cx, |workspace, cx| {
        let merged = workspace.active_document_id().unwrap();
        assert!(!ids.contains(&merged), "the merge opens as a new tab");
        let session = workspace.session(merged, cx).unwrap().read(cx);
        (0..).map_while(|page| session.page_size(page)).count()
    });
    assert_eq!(merged, pages, "every page of every document");
    std::fs::remove_dir_all(root).unwrap();
}
