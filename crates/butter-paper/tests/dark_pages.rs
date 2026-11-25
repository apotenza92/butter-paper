//! Dark pages, step 2: the view path. A switch of the workspace's paper
//! draws the tiles, the page image and the thumbnails again with the new
//! paper. Paths that must give the document's real colours have no paper
//! argument, so they stay Real.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use butter_paper::document_resource::{
    NativeDocumentResource, OpenedNativeDocument, RasterSurface, ThumbnailSurface,
};
use butter_paper::document_workspace::{
    DOCUMENT_THUMBNAIL_STRIP_ID, DocumentWorkspace, NativeDocumentOpener, OpenDocumentRequest,
    init_document_workspace_actions,
};
use butter_paper::paper_appearance::PaperAppearance;
use gpui::{AppContext as _, Modifiers, TestAppContext};
use gpui_component::Root;

/// What the view asked the resource for, and with which paper.
#[derive(Default)]
struct Calls {
    tiles: Vec<PaperAppearance>,
    view_pages: Vec<PaperAppearance>,
    thumbnails: Vec<PaperAppearance>,
    /// `render_page`: real colours, no paper.
    real_pages: usize,
    /// The picture coverage a Dark page image reports.
    coverage: Option<f32>,
}

struct PaperResource {
    calls: Arc<Mutex<Calls>>,
    released: AtomicBool,
}

/// A raster of plain paper: white for Real, `#1E1E1E` for Dark.
fn paper_raster(width: u32, height: u32, paper: PaperAppearance) -> RasterSurface {
    let pixel = match paper {
        PaperAppearance::Real => [0xff; 4],
        PaperAppearance::Dark => [0x1e, 0x1e, 0x1e, 0xff],
    };
    RasterSurface::new(
        width,
        height,
        pixel.repeat(width as usize * height as usize),
    )
    .unwrap()
}

impl NativeDocumentResource for PaperResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4500)
    }

    fn render_page(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
        self.calls.lock().unwrap().real_pages += 1;
        Ok(paper_raster(width, page_index + 2, PaperAppearance::Real))
    }

    fn render_view_page(
        &self,
        page_index: u32,
        width: u32,
        paper: PaperAppearance,
    ) -> Result<RasterSurface, String> {
        let mut calls = self.calls.lock().unwrap();
        calls.view_pages.push(paper);
        let coverage = calls.coverage.filter(|_| paper == PaperAppearance::Dark);
        Ok(paper_raster(width, page_index + 2, paper).with_picture_coverage(coverage))
    }

    fn render_thumbnail(
        &self,
        page_index: u32,
        width: u32,
        paper: PaperAppearance,
    ) -> Result<RasterSurface, String> {
        self.calls.lock().unwrap().thumbnails.push(paper);
        Ok(paper_raster(width, page_index + 2, paper))
    }

    fn render_tile(
        &self,
        request: butter_paper::viewer::TileRequest,
    ) -> Result<RasterSurface, String> {
        self.calls.lock().unwrap().tiles.push(request.paper);
        Ok(paper_raster(
            request.crop.width as u32,
            request.crop.height as u32,
            request.paper,
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

struct PaperOpener(Arc<Mutex<Calls>>);

impl NativeDocumentOpener for PaperOpener {
    fn open(&self, _: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        OpenedNativeDocument::new(
            "dark-pages.pdf",
            vec![(612., 792.); 3],
            paper_raster(32, 40, PaperAppearance::Real),
            (0..3)
                .map(|page| ThumbnailSurface::new(page, paper_raster(8, 10, PaperAppearance::Real)))
                .collect(),
            Arc::new(PaperResource {
                calls: self.0.clone(),
                released: AtomicBool::new(false),
            }),
        )
    }
}

#[gpui::test]
fn a_paper_switch_redraws_the_view_with_the_new_paper(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        cx.set_reduce_motion(true);
    });
    cx.update(init_document_workspace_actions);
    let calls = Arc::new(Mutex::new(Calls::default()));
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let calls = calls.clone();
        move |window, cx| {
            let workspace =
                cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(PaperOpener(calls)), cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let document_id = workspace.update(cx, |workspace, cx| {
        workspace.open_path("dark-pages.pdf".into(), cx)
    });
    cx.run_until_parked();
    let draw = |cx: &mut gpui::VisualTestContext| {
        workspace.update(cx, |workspace, cx| {
            workspace
                .refresh_viewport_async(document_id, 800., 600., 1., cx)
                .unwrap();
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    };
    draw(cx);
    // The Pages sidebar, so the thumbnails are drawn.
    if cx.debug_bounds(DOCUMENT_THUMBNAIL_STRIP_ID).is_none() {
        let pages = cx
            .debug_bounds("document-left-rail-pages")
            .expect("Pages must be available in the left rail");
        cx.simulate_click(pages.center(), Modifiers::default());
        cx.run_until_parked();
    }
    draw(cx);
    {
        let calls = calls.lock().unwrap();
        assert!(!calls.tiles.is_empty(), "the view draws tiles");
        assert!(
            calls
                .tiles
                .iter()
                .all(|paper| *paper == PaperAppearance::Real)
        );
        assert!(
            calls
                .view_pages
                .iter()
                .all(|paper| *paper == PaperAppearance::Real)
        );
    }
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.paper_appearance()),
        PaperAppearance::Real,
        "real colours by default"
    );

    *calls.lock().unwrap() = Calls::default();
    workspace.update(cx, |workspace, cx| {
        workspace.set_paper_appearance(PaperAppearance::Dark, cx);
    });
    cx.run_until_parked();
    draw(cx);
    {
        let calls = calls.lock().unwrap();
        assert!(!calls.tiles.is_empty(), "every tile is drawn again");
        assert!(
            calls
                .tiles
                .iter()
                .all(|paper| *paper == PaperAppearance::Dark)
        );
        assert_eq!(
            calls.view_pages,
            vec![PaperAppearance::Dark],
            "the page image under the tiles is drawn again, dark"
        );
        assert!(!calls.thumbnails.is_empty(), "thumbnails are drawn again");
        assert!(
            calls
                .thumbnails
                .iter()
                .all(|paper| *paper == PaperAppearance::Dark)
        );
        assert_eq!(calls.real_pages, 0, "the view never asks for real colours");
    }

    // The evidence render stands for the paths that must stay real: it has
    // no paper argument.
    let real_pages = calls.lock().unwrap().real_pages;
    let request = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_page_navigation(document_id, 0, cx)
        })
        .unwrap();
    workspace
        .read_with(cx, |workspace, cx| {
            workspace.render_page_request_for_evidence(&request, cx)
        })
        .unwrap();
    assert_eq!(calls.lock().unwrap().real_pages, real_pages + 1);

    *calls.lock().unwrap() = Calls::default();
    workspace.update(cx, |workspace, cx| {
        workspace.set_paper_appearance(PaperAppearance::Real, cx);
    });
    cx.run_until_parked();
    draw(cx);
    let calls = calls.lock().unwrap();
    assert!(!calls.tiles.is_empty());
    assert!(
        calls
            .tiles
            .iter()
            .all(|paper| *paper == PaperAppearance::Real)
    );
    assert!(
        calls
            .view_pages
            .iter()
            .all(|paper| *paper == PaperAppearance::Real)
    );
}

/// Step 3: Cmd+Shift+D on macOS and Ctrl+Shift+D on Windows and Linux are
/// bound to the toggle, once, and nothing else uses those keys.
#[gpui::test]
fn the_page_appearance_shortcut_is_bound_once_and_reaches_its_action(cx: &mut TestAppContext) {
    use butter_paper::application_shell::{
        TogglePageAppearance, init_application_shell_actions, toggle_page_appearance_shortcut,
    };
    cx.update(|cx| {
        gpui_component::init(cx);
        cx.set_reduce_motion(true);
        init_application_shell_actions(cx);
        init_document_workspace_actions(cx);
    });
    let shortcut = toggle_page_appearance_shortcut(cfg!(target_os = "macos"));
    cx.update(|cx| {
        let expected = gpui::KeyBinding::new(shortcut, TogglePageAppearance, None);
        let keymap = cx.key_bindings();
        let keymap = keymap.borrow();
        let same_keys = keymap
            .bindings()
            .filter(|binding| binding.keystrokes() == expected.keystrokes())
            .collect::<Vec<_>>();
        assert_eq!(same_keys.len(), 1, "{shortcut} is used by one binding only");
        assert!(same_keys[0].action().as_any().is::<TogglePageAppearance>());
        assert_eq!(
            keymap.bindings_for_action(&TogglePageAppearance).count(),
            1,
            "bound for this platform only"
        );
    });

    let toggled = std::rc::Rc::new(std::cell::Cell::new(0));
    cx.update({
        let toggled = toggled.clone();
        move |cx| {
            cx.on_action(move |_: &TogglePageAppearance, _| toggled.set(toggled.get() + 1));
        }
    });
    let calls = Arc::new(Mutex::new(Calls::default()));
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace =
                cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(PaperOpener(calls)), cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    workspace.update(cx, |workspace, cx| {
        workspace.open_path("dark-pages.pdf".into(), cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let focus = workspace.read(cx).focus_handle();
        window.focus(&focus, cx);
        window.draw(cx).clear(cx);
    });
    cx.simulate_keystrokes(shortcut);
    assert_eq!(toggled.get(), 1, "the shortcut reaches the document window");
}

/// A page that is mostly picture (a scan) shows the status line on dark
/// paper only: from the worker's report for the page image on screen.
#[gpui::test]
fn the_status_bar_says_pictures_keep_their_colours_for_a_mostly_picture_page(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        cx.set_reduce_motion(true);
    });
    cx.update(init_document_workspace_actions);
    let calls = Arc::new(Mutex::new(Calls::default()));
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let calls = calls.clone();
        move |window, cx| {
            let workspace =
                cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(PaperOpener(calls)), cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let document_id = workspace.update(cx, |workspace, cx| {
        workspace.open_path("dark-pages.pdf".into(), cx)
    });
    cx.run_until_parked();
    let note = |cx: &mut gpui::VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| workspace.pictures_keep_colours_note(cx))
    };
    let draw = |cx: &mut gpui::VisualTestContext| {
        workspace.update(cx, |workspace, cx| {
            workspace
                .refresh_viewport_async(document_id, 800., 600., 1., cx)
                .unwrap();
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    };
    draw(cx);
    assert_eq!(note(cx), None, "light pages say nothing");

    // A scan: the worker reports that pictures cover 95% of the page.
    calls.lock().unwrap().coverage = Some(0.95);
    workspace.update(cx, |workspace, cx| {
        workspace.set_paper_appearance(PaperAppearance::Dark, cx)
    });
    cx.run_until_parked();
    draw(cx);
    assert_eq!(
        note(cx),
        Some("Pictures keep their real colours in Dark pages")
    );
    // The line leads the status bar's text, where truncation keeps it.
    if let Some(tree) = cx.update(|window, _| window.debug_a11y_tree_json()) {
        assert!(
            tree.contains("Pictures keep their real colours in Dark pages ·"),
            "{tree}"
        );
    }

    // Back to light pages: the line goes.
    workspace.update(cx, |workspace, cx| {
        workspace.set_paper_appearance(PaperAppearance::Real, cx)
    });
    cx.run_until_parked();
    draw(cx);
    assert_eq!(note(cx), None);

    // A drawing: pictures cover a little of the page.
    calls.lock().unwrap().coverage = Some(0.1);
    workspace.update(cx, |workspace, cx| {
        workspace.set_paper_appearance(PaperAppearance::Dark, cx)
    });
    cx.run_until_parked();
    draw(cx);
    assert_eq!(note(cx), None, "a drawing keeps the usual guidance");
}
