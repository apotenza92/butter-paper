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

    fn render_thumbnail(
        &self,
        page_index: u32,
        width: u32,
        _paper: butter_paper::paper_appearance::PaperAppearance,
    ) -> Result<RasterSurface, String> {
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
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
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
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
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

#[gpui::test]
fn merge_documents_cancel_closes_the_dialog(cx: &mut TestAppContext) {
    use butter_paper::document_workspace::DOCUMENT_MERGE_CANCEL_ID;
    use butter_paper::generated_document::GeneratedDocumentStore;
    use gpui_component::WindowExt as _;
    use std::time::Duration;
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
        init_document_workspace_actions(cx);
    });
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join(format!(
        "target/test-scratch/merge-cancel-{}",
        std::process::id()
    ));
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
            let shell = cx.new(|_| MergeModalShell(workspace));
            Root::new(shell, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let ids = ["bp-multi-page-v1.pdf", "bp-annotation-all-v1.pdf"].map(|name| {
        let file = manifest.join("tests/fixtures").join(name);
        workspace.update(cx, |workspace, cx| workspace.open_path(file, cx))
    });
    cx.run_until_parked();
    let settle = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    };
    // Opened as File ▸ Merge Documents… and a tab's menu open it: on the
    // next frame.
    workspace.update(cx, |workspace, cx| workspace.request_merge_dialog(cx));
    settle(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    // A press on the dimmed backdrop, outside the dialog, closes it.
    let dialog = cx.debug_bounds("document-merge-dialog").unwrap();
    cx.simulate_click(
        gpui::point(gpui::px(100.), dialog.bottom() + gpui::px(60.)),
        gpui::Modifiers::none(),
    );
    settle(cx);
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "a click outside closes Merge Documents"
    );
    workspace.update(cx, |workspace, cx| workspace.request_merge_dialog(cx));
    settle(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    let cancel = cx.debug_bounds(DOCUMENT_MERGE_CANCEL_ID).unwrap();
    cx.simulate_click(cancel.center(), gpui::Modifiers::none());
    settle(cx);
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Cancel closes Merge Documents"
    );
    // File ▸ Merge Documents… dispatches the action to the focused workspace.
    let workspace_focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| {
        window.activate_window();
        workspace_focus.focus(window, cx);
    });
    settle(cx);
    cx.dispatch_action(butter_paper::document_workspace::MergeDocuments);
    settle(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    let check = cx.debug_bounds("document-merge-check-0").unwrap();
    cx.simulate_click(check.center(), gpui::Modifiers::none());
    settle(cx);
    let cancel = cx.debug_bounds(DOCUMENT_MERGE_CANCEL_ID).unwrap();
    cx.simulate_click(cancel.center(), gpui::Modifiers::none());
    settle(cx);
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Cancel closes Merge Documents opened from the menu, after ticking a row"
    );
    // A tab's menu: right-click the tab, choose Merge Documents….
    let tab_id = butter_paper::document_workspace::document_session_tab_id(ids[0]);
    let tab = cx.debug_bounds(tab_id.leak()).unwrap();
    let press = gpui::point(tab.left() + gpui::px(6.), tab.center().y);
    cx.simulate_event(gpui::MouseDownEvent {
        button: gpui::MouseButton::Right,
        position: press,
        modifiers: gpui::Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(gpui::MouseUpEvent {
        button: gpui::MouseButton::Right,
        position: press,
        modifiers: gpui::Modifiers::none(),
        click_count: 1,
    });
    settle(cx);
    // Its first item sits just below and right of the press.
    let item = gpui::point(press.x + gpui::px(40.), press.y + gpui::px(16.));
    cx.simulate_click(item, gpui::Modifiers::none());
    settle(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the tab menu opens Merge Documents"
    );
    let cancel = cx.debug_bounds(DOCUMENT_MERGE_CANCEL_ID).unwrap();
    cx.simulate_click(cancel.center(), gpui::Modifiers::none());
    settle(cx);
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Cancel closes Merge Documents opened from a tab's menu"
    );
    std::fs::remove_dir_all(root).unwrap();
}

// Root holds the open dialogs; like the application's window, the test window
// paints that layer so the stock dialog controls can be clicked.
struct MergeModalShell(gpui::Entity<DocumentWorkspace>);
impl gpui::Render for MergeModalShell {
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
fn merge_documents_keeps_focus_when_a_focused_row_is_removed(cx: &mut TestAppContext) {
    use butter_paper::document_workspace::DOCUMENT_MERGE_CANCEL_ID;
    use butter_paper::generated_document::GeneratedDocumentStore;
    use gpui_component::WindowExt as _;
    use std::time::Duration;
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
        init_document_workspace_actions(cx);
    });
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join(format!(
        "target/test-scratch/merge-focus-{}",
        std::process::id()
    ));
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
            let shell = cx.new(|cx| MergeFocusShell::new(workspace, window, cx));
            Root::new(shell, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    for name in ["bp-multi-page-v1.pdf", "bp-annotation-all-v1.pdf"] {
        let file = manifest.join("tests/fixtures").join(name);
        workspace.update(cx, |workspace, cx| workspace.open_path(file, cx));
    }
    cx.run_until_parked();
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..3 {
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            cx.executor().advance_clock(Duration::from_millis(300));
            cx.run_until_parked();
        }
    };
    workspace.update(cx, |workspace, cx| workspace.request_merge_dialog(cx));
    settle(cx);
    let dialog = workspace
        .read_with(cx, |workspace, _| workspace.merge_dialog())
        .unwrap();
    let added = manifest.join("tests/fixtures/bp-coordinate-space-v1.pdf");
    dialog.update(cx, |dialog, cx| dialog.add_paths(vec![added], cx));
    settle(cx);
    // Keyboard users reach the added row's remove button with Tab and press
    // it; the button goes with its row.
    cx.simulate_keystrokes("tab tab tab tab");
    settle(cx);
    let space = gpui::Keystroke::parse("space").unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: space.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke: space });
    settle(cx);
    assert_eq!(
        dialog.read_with(cx, |dialog, _| dialog.chosen().len()),
        2,
        "the added row is removed"
    );
    let workspace_focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    assert!(
        !cx.update(|window, cx| workspace_focus.contains_focused(window, cx)),
        "focus stays in the dialog, not the document behind it"
    );
    let cancel = cx.debug_bounds(DOCUMENT_MERGE_CANCEL_ID).unwrap();
    cx.simulate_click(cancel.center(), gpui::Modifiers::none());
    settle(cx);
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Cancel still closes the dialog"
    );
    std::fs::remove_dir_all(root).unwrap();
}

// The application window's composition: Root's dialog layer painted over the
// workspace, and lost focus restored as the application restores it.
struct MergeFocusShell {
    workspace: gpui::Entity<DocumentWorkspace>,
    _focus_lost: gpui::Subscription,
}
impl MergeFocusShell {
    fn new(
        workspace: gpui::Entity<DocumentWorkspace>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let focus_lost = cx.on_focus_lost(window, |shell, window, cx| {
            let document = shell.workspace.read(cx).focus_handle();
            butter_paper::application_shell::restore_lost_focus(&document, window, cx);
        });
        Self {
            workspace,
            _focus_lost: focus_lost,
        }
    }
}
impl gpui::Render for MergeFocusShell {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{ParentElement as _, Styled as _};
        let dialogs = Root::render_dialog_layer(window, cx);
        gpui::div()
            .size_full()
            .child(self.workspace.clone())
            .children(dialogs)
    }
}

#[path = "support/dialog_contract.rs"]
mod dialog_contract;

#[gpui::test]
fn merge_documents_keeps_the_dialog_dismissal_contract(cx: &mut TestAppContext) {
    use butter_paper::document_workspace::{DOCUMENT_MERGE_CANCEL_ID, DOCUMENT_MERGE_DIALOG_ID};
    use butter_paper::generated_document::GeneratedDocumentStore;
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
        init_document_workspace_actions(cx);
    });
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join(format!(
        "target/test-scratch/merge-contract-{}",
        std::process::id()
    ));
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
            let shell = cx.new(|_| MergeModalShell(workspace));
            Root::new(shell, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    for name in ["bp-multi-page-v1.pdf", "bp-annotation-all-v1.pdf"] {
        let file = manifest.join("tests/fixtures").join(name);
        workspace.update(cx, |workspace, cx| workspace.open_path(file, cx));
    }
    cx.run_until_parked();
    let reopen = {
        let workspace = workspace.clone();
        move |cx: &mut gpui::VisualTestContext| {
            workspace.update(cx, |workspace, cx| workspace.request_merge_dialog(cx));
            dialog_contract::settle(cx);
        }
    };
    reopen.clone()(cx);
    dialog_contract::assert_dialog_dismissal(
        cx,
        "Merge documents",
        DOCUMENT_MERGE_DIALOG_ID,
        DOCUMENT_MERGE_CANCEL_ID,
        dialog_contract::Outside::Closes,
        reopen,
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[gpui::test]
fn the_page_scale_panel_drops_from_the_scale_and_closes_like_a_popup(cx: &mut TestAppContext) {
    use butter_paper::generated_document::GeneratedDocumentStore;
    use butter_paper::page_scale_control::{
        PAGE_SCALE_BODY_ID, PAGE_SCALE_CALIBRATE_ID, PAGE_SCALE_CANCEL_ID, PAGE_SCALE_CLOSE_ID,
        PAGE_SCALE_CUSTOM_PAPER_ID, PAGE_SCALE_CUSTOM_PAPER_UNIT_ID, PAGE_SCALE_CUSTOM_REAL_ID,
        PAGE_SCALE_DIALOG_ID, PAGE_SCALE_GROUP_ID, PAGE_SCALE_PRECISION_ID, PAGE_SCALE_SELECT_ID,
        PAGE_SCALE_UNIT_ID, ScaleGroup,
    };
    use butter_paper::viewer_toolbar_strip::{VIEWER_SCALE_ID, VIEWER_SCALE_MENU_ID};
    use gpui::{Modifiers, point, px};
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
        init_document_workspace_actions(cx);
    });
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join(format!(
        "target/test-scratch/scale-panel-{}",
        std::process::id()
    ));
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
            let shell = cx.new(|_| MergeModalShell(workspace));
            Root::new(shell, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let file = manifest.join("tests/fixtures/bp-multi-page-v1.pdf");
    workspace.update(cx, |workspace, cx| workspace.open_path(file, cx));
    cx.run_until_parked();
    let settle = |cx: &mut gpui::VisualTestContext| {
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };
    let open = |cx: &mut gpui::VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .page_scale_control()
                .is_some_and(|control| control.read(cx).is_open())
        })
    };
    settle(cx);
    let menu = cx
        .debug_bounds(VIEWER_SCALE_MENU_ID)
        .expect("the toolbar shows the page's Scale");
    let reopen = |cx: &mut gpui::VisualTestContext, after: &str| {
        cx.simulate_click(menu.center(), Modifiers::none());
        settle(cx);
        assert!(open(cx), "the Scale menu reopens the panel after {after}");
    };
    reopen(cx, "opening the document");

    // It drops down from the Scale, inside the window.
    let scale = cx.debug_bounds(VIEWER_SCALE_ID).unwrap();
    let panel = cx.debug_bounds(PAGE_SCALE_DIALOG_ID).unwrap();
    let viewport = cx.update(|window, _| window.viewport_size());
    assert!(
        panel.top() >= scale.bottom(),
        "the panel drops below the Scale"
    );
    // The panel's content sits inside the stock popover's 12px padding,
    // whose surface ends at the Scale's right edge.
    assert!(
        panel.right() <= scale.right() && scale.right() - panel.right() <= px(13.),
        "and ends at its right edge"
    );
    assert!(panel.right() <= viewport.width && panel.bottom() <= viewport.height);

    // The body scrolls, so it clips at its edge: every control in it keeps
    // room inside that edge for its 3px focus ring, and lines up with the
    // header all the same.
    let ring_room = |cx: &mut gpui::VisualTestContext, ids: &[&'static str]| {
        let body = cx.debug_bounds(PAGE_SCALE_BODY_ID).unwrap();
        for id in ids {
            let control = cx
                .debug_bounds(id)
                .unwrap_or_else(|| panic!("{id} is shown"));
            assert!(
                control.left() - body.left() >= px(3.)
                    && body.right() - control.right() >= px(3.)
                    && control.top() - body.top() >= px(3.),
                "{id} at {control:?} leaves room for its focus ring in {body:?}"
            );
        }
    };
    ring_room(
        cx,
        &[
            PAGE_SCALE_GROUP_ID,
            PAGE_SCALE_SELECT_ID,
            PAGE_SCALE_CALIBRATE_ID,
        ],
    );
    let group = cx.debug_bounds(PAGE_SCALE_GROUP_ID).unwrap();
    assert_eq!(
        group.left(),
        panel.left(),
        "the body lines up with the header"
    );
    let control = workspace
        .read_with(cx, |workspace, _| workspace.page_scale_control())
        .unwrap();
    cx.update(|window, cx| {
        control.update(cx, |control, cx| {
            control.choose_group_for_test(ScaleGroup::Custom, window, cx)
        })
    });
    settle(cx);
    assert!(
        cx.debug_bounds(PAGE_SCALE_SELECT_ID).is_none(),
        "Custom shows its ratio in place of a list of scales"
    );
    ring_room(
        cx,
        &[
            PAGE_SCALE_GROUP_ID,
            PAGE_SCALE_CUSTOM_PAPER_ID,
            PAGE_SCALE_CUSTOM_PAPER_UNIT_ID,
            PAGE_SCALE_CUSTOM_REAL_ID,
            PAGE_SCALE_UNIT_ID,
            PAGE_SCALE_PRECISION_ID,
        ],
    );

    // The menu button closes it again.
    cx.simulate_click(menu.center(), Modifiers::none());
    settle(cx);
    assert!(!open(cx), "the Scale menu closes the panel");
    reopen(cx, "the Scale menu");

    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(!open(cx), "Escape closes it");
    reopen(cx, "Escape");

    for (button, name) in [
        (PAGE_SCALE_CLOSE_ID, "its X"),
        (PAGE_SCALE_CANCEL_ID, "Cancel"),
    ] {
        let bounds = cx.debug_bounds(button).unwrap();
        cx.simulate_click(bounds.center(), Modifiers::none());
        settle(cx);
        assert!(!open(cx), "{name} closes it");
        reopen(cx, name);
    }

    // A press on the page beside it closes it, and only closes it.
    let panel = cx.debug_bounds(PAGE_SCALE_DIALOG_ID).unwrap();
    let beside = point(panel.right() + px(40.), panel.bottom() - px(20.));
    assert!(beside.x < viewport.width, "leaves page beside it to press");
    cx.simulate_click(beside, Modifiers::none());
    settle(cx);
    assert!(!open(cx), "a press outside it closes it");
    std::fs::remove_dir_all(root).unwrap();
}

impl Opened<'_> {
    /// Each page's rotation, in order.
    fn rotations(&mut self) -> Vec<butter_paper::annotation_model::PageRotation> {
        let document_id = self.document_id;
        self.workspace.read_with(self.cx, |workspace, cx| {
            let session = workspace.session(document_id, cx).unwrap().read(cx);
            (0..3)
                .map(|page| session.page_rotation(page).unwrap())
                .collect()
        })
    }

    fn undo_depth(&mut self) -> usize {
        self.workspace
            .read_with(self.cx, |workspace, cx| {
                workspace.annotation_snapshot(self.document_id, cx)
            })
            .unwrap()
            .undo_depth
    }

    fn dispatch(&mut self, action: Box<dyn gpui::Action>) {
        let focus = self
            .workspace
            .read_with(self.cx, |workspace, _| workspace.focus_handle());
        self.cx.update(|window, cx| {
            focus.focus(window, cx);
            window.dispatch_action(action, cx);
        });
        self.cx.run_until_parked();
        self.cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    /// Shows Page Thumbnails.
    fn show_thumbnails(&mut self) {
        self.cx.update(|window, cx| window.draw(cx).clear(cx));
        let toggle = self.cx.debug_bounds("document-left-rail-pages").unwrap();
        self.cx
            .simulate_click(toggle.center(), gpui::Modifiers::default());
        self.cx.run_until_parked();
        self.cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn click(&mut self, id: &str, modifiers: gpui::Modifiers) {
        let id: &'static str = Box::leak(id.to_owned().into_boxed_str());
        let target = self
            .cx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("{id} is shown"));
        self.cx.simulate_click(target.center(), modifiers);
        self.cx.run_until_parked();
        self.cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

/// Rotate All Pages turns every page, whatever is selected, as one undo
/// step, from the Document menu's actions and from Page Thumbnails' header;
/// the page in view stays in view.
#[gpui::test]
fn rotate_all_pages_turns_every_page_as_one_undo_step(cx: &mut TestAppContext) {
    use butter_paper::annotation_model::PageRotation;
    use butter_paper::document_workspace::{
        DOCUMENT_THUMBNAIL_ROTATE_ALL_LEFT_ID, DOCUMENT_THUMBNAIL_ROTATE_ALL_RIGHT_ID,
        RotateAllPagesLeft, RotateAllPagesRight,
    };
    let mut opened = three_pages(cx);
    opened
        .update(|workspace, document_id, cx| workspace.activate_thumbnail_page(document_id, 1, cx));
    let depth = opened.undo_depth();
    assert_eq!(opened.rotations(), [PageRotation::Degrees0; 3]);
    let widths = opened.thumbnail_widths();
    opened.dispatch(Box::new(RotateAllPagesRight));
    assert_eq!(opened.rotations(), [PageRotation::Degrees90; 3]);
    assert_eq!(opened.undo_depth(), depth + 1, "one undo step");
    assert_eq!(opened.current_page(), 1, "the page in view stays");
    // Every thumbnail turns: its pixels are 10 high, so now 10 wide.
    assert_eq!(opened.thumbnail_widths(), [Some(10); 3]);
    // One undo turns them all back, and redo turns them again.
    opened
        .update(|workspace, document_id, cx| workspace.undo_annotations(document_id, cx))
        .unwrap();
    assert_eq!(opened.rotations(), [PageRotation::Degrees0; 3]);
    assert_eq!(opened.thumbnail_widths(), widths);
    opened
        .update(|workspace, document_id, cx| workspace.redo_annotations(document_id, cx))
        .unwrap();
    assert_eq!(opened.rotations(), [PageRotation::Degrees90; 3]);
    opened.dispatch(Box::new(RotateAllPagesLeft));
    assert_eq!(opened.rotations(), [PageRotation::Degrees0; 3]);

    // Page Thumbnails' header buttons turn every page too, with one page
    // selected.
    opened.show_thumbnails();
    let document_id = opened.document_id;
    opened.click(
        &butter_paper::document_workspace::document_thumbnail_id(document_id, 2),
        gpui::Modifiers::default(),
    );
    let depth = opened.undo_depth();
    opened.click(
        DOCUMENT_THUMBNAIL_ROTATE_ALL_LEFT_ID,
        gpui::Modifiers::default(),
    );
    assert_eq!(opened.rotations(), [PageRotation::Degrees270; 3]);
    // The header keeps all three buttons inside Page Thumbnails.
    let header = opened
        .cx
        .debug_bounds("document-left-sidebar-header")
        .unwrap();
    for id in [
        DOCUMENT_THUMBNAIL_ROTATE_ALL_LEFT_ID,
        DOCUMENT_THUMBNAIL_ROTATE_ALL_RIGHT_ID,
        butter_paper::document_workspace::DOCUMENT_THUMBNAIL_ADD_PAGE_ID,
    ] {
        let button = opened.cx.debug_bounds(id).unwrap();
        assert!(
            button.left() >= header.left() && button.right() <= header.right(),
            "{id} {button:?} outside {header:?}"
        );
    }
    opened.click(
        DOCUMENT_THUMBNAIL_ROTATE_ALL_RIGHT_ID,
        gpui::Modifiers::default(),
    );
    assert_eq!(opened.rotations(), [PageRotation::Degrees0; 3]);
    opened.click(
        DOCUMENT_THUMBNAIL_ROTATE_ALL_RIGHT_ID,
        gpui::Modifiers::default(),
    );
    assert_eq!(opened.rotations(), [PageRotation::Degrees90; 3]);
    assert_eq!(opened.undo_depth(), depth + 3);
}

/// Cmd+A (Ctrl+A on Windows and Linux) in Page Thumbnails selects every
/// page, and Escape clears the selection; elsewhere Select All still
/// selects the page's markups.
#[gpui::test]
fn select_all_in_page_thumbnails_selects_every_page_and_escape_clears(cx: &mut TestAppContext) {
    let select_all = if cfg!(target_os = "macos") {
        "cmd-a"
    } else {
        "ctrl-a"
    };
    let mut opened = three_pages(cx);
    opened.show_thumbnails();
    let document_id = opened.document_id;
    let selected = |opened: &mut Opened<'_>| {
        opened.workspace.read_with(opened.cx, |workspace, cx| {
            workspace.selected_pages(document_id, cx)
        })
    };
    let selected_markup = |opened: &mut Opened<'_>| {
        opened
            .workspace
            .read_with(opened.cx, |workspace, cx| {
                workspace.annotation_snapshot(document_id, cx)
            })
            .unwrap()
            .selected_id
            .map(|id| id.as_str().to_owned())
    };
    opened.click(
        &butter_paper::document_workspace::document_thumbnail_id(document_id, 1),
        gpui::Modifiers::default(),
    );
    assert_eq!(selected(&mut opened), [1]);
    let markup_before = selected_markup(&mut opened);
    assert_ne!(markup_before.as_deref(), Some("a"));
    opened.cx.simulate_keystrokes(select_all);
    opened.cx.run_until_parked();
    assert_eq!(selected(&mut opened), [0, 1, 2]);
    assert_eq!(
        selected_markup(&mut opened),
        markup_before,
        "the markups are left as they were"
    );
    assert_eq!(opened.current_page(), 1, "the page in view stays");
    // Escape clears it: page actions act on the page in view again.
    opened.click(
        &butter_paper::document_workspace::document_thumbnail_id(document_id, 2),
        gpui::Modifiers::secondary_key(),
    );
    assert_eq!(selected(&mut opened), [0, 1]);
    opened.cx.simulate_keystrokes("escape");
    opened.cx.run_until_parked();
    assert_eq!(selected(&mut opened), [opened.current_page()]);
    opened.cx.simulate_keystrokes(select_all);
    opened.cx.run_until_parked();
    assert_eq!(selected(&mut opened), [0, 1, 2]);
    // On the page, Select All selects the page's markups, as before.
    opened
        .update(|workspace, document_id, cx| workspace.activate_thumbnail_page(document_id, 0, cx));
    let focus = opened
        .workspace
        .read_with(opened.cx, |workspace, _| workspace.focus_handle());
    opened.cx.update(|window, cx| focus.focus(window, cx));
    opened.cx.simulate_keystrokes(select_all);
    opened.cx.run_until_parked();
    assert_eq!(
        selected_markup(&mut opened).as_deref(),
        Some("a"),
        "the page's rectangle"
    );
}
