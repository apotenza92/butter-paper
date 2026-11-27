//! Two Pages view and Split View, checked against the painted page
//! geometry: pairs, the cover alone, fits, the current page, rotation and
//! drawing on the right page; and two panes of one document, each with its
//! own view and one shared history.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use butter_paper::document_session::PaneSide;
use butter_paper::document_workspace::{
    ContinuousView, DOCUMENT_VIEWPORT_ID, DocumentId, DocumentWorkspace, FitPage, FitWidth,
    NativeDocumentOpener, NativeDocumentResource, NavigateEnd, NavigateNextPage,
    NavigatePreviousPage, OpenDocumentRequest, OpenedNativeDocument, RasterSurface,
    RotatePageRight, SPLIT_ACTIVE_RING_ID, SPLIT_DIVIDER_ID, SPLIT_OTHER_VIEWPORT_ID,
    SPLIT_PANE_LEFT_ID, SPLIT_PANE_RIGHT_ID, ThumbnailSurface, ToggleCoverPageAlone,
    ToggleSplitView, TwoPagesView, ZoomIn, document_viewer_page_id,
    init_document_workspace_actions,
};
use butter_paper::native_document_view_state::ViewerZoomPreset;
use butter_paper::page_view_control::PageViewMode;
use gpui::{
    AppContext as _, Bounds, Modifiers, MouseButton, Pixels, ScrollDelta, ScrollWheelEvent,
    TestAppContext, VisualTestContext, point, px, size,
};
use gpui_component::Root;

const TOLERANCE: f32 = 2.;
const LETTER: (f32, f32) = (612., 792.);
const UNIFORM_PAGES: [(f32, f32); 5] = [LETTER; 5];
/// Letter beside tabloid landscape, then letter beside a small card.
const MIXED_PAGES: [(f32, f32); 4] = [LETTER, (1224., 792.), LETTER, (300., 400.)];

fn raster(width: u32, height: u32) -> RasterSurface {
    RasterSurface::new(
        width.max(1),
        height.max(1),
        vec![0xff; width.max(1) as usize * height.max(1) as usize * 4],
    )
    .unwrap()
}

/// A document's release, and whether it is digitally signed.
struct Resource(Arc<AtomicBool>, bool);

impl NativeDocumentResource for Resource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4343)
    }

    fn digitally_signed(&self) -> bool {
        self.1
    }

    fn render_page(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
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

struct Opener(&'static [(f32, f32)]);

impl NativeDocumentOpener for Opener {
    fn open(&self, request: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        let pages = self.0;
        OpenedNativeDocument::new(
            request.path.file_name().unwrap().to_string_lossy(),
            pages.to_vec(),
            raster(32, 40),
            (0..pages.len() as u32)
                .map(|page| ThumbnailSurface::new(page, raster(8, 10)))
                .collect(),
            Arc::new(Resource(
                Arc::new(AtomicBool::new(false)),
                request
                    .path
                    .file_name()
                    .is_some_and(|name| name.as_encoded_bytes().starts_with(b"signed")),
            )),
        )
    }
}

struct Harness<'a> {
    cx: &'a mut VisualTestContext,
    workspace: gpui::Entity<DocumentWorkspace>,
    document: DocumentId,
}

fn setup<'a>(cx: &'a mut TestAppContext, pages: &'static [(f32, f32)]) -> Harness<'a> {
    cx.update(|cx| {
        gpui_component::init(cx);
        cx.set_reduce_motion(true);
        init_document_workspace_actions(cx);
    });
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let slot = slot.clone();
        move |window, cx| {
            let workspace =
                cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(Opener(pages)), cx));
            slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = slot.borrow_mut().take().unwrap();
    cx.simulate_resize(size(px(1280.), px(800.)));
    let document = workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("pages.pdf"), cx)
    });
    cx.run_until_parked();
    let focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| {
        window.activate_window();
        focus.focus(window, cx);
    });
    let mut harness = Harness {
        cx,
        workspace,
        document,
    };
    harness.settle();
    harness
}

impl Harness<'_> {
    /// Draws and re-plans both panes until nothing changes, as the app's
    /// next-frame refreshes do (the test platform runs none).
    fn settle(&mut self) {
        let document = self.document;
        for _ in 0..12 {
            self.cx.update(|window, cx| window.draw(cx).clear(cx));
            if let Some(viewport) = self.cx.debug_bounds(DOCUMENT_VIEWPORT_ID) {
                self.workspace.update(self.cx, |workspace, cx| {
                    let _ = workspace.refresh_viewport_async(
                        document,
                        f32::from(viewport.size.width),
                        f32::from(viewport.size.height),
                        1.,
                        cx,
                    );
                });
            }
            if let Some(viewport) = self.cx.debug_bounds(SPLIT_OTHER_VIEWPORT_ID) {
                self.workspace.update(self.cx, |workspace, cx| {
                    let _ = workspace.refresh_other_pane(
                        document,
                        f32::from(viewport.size.width),
                        f32::from(viewport.size.height),
                        1.,
                        cx,
                    );
                });
            }
            self.cx.run_until_parked();
        }
        self.cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn action(&mut self, action: impl gpui::Action) {
        self.cx.dispatch_action(action);
        self.settle();
    }

    fn page_count(&mut self) -> u32 {
        let document = self.document;
        self.workspace.read_with(self.cx, |workspace, cx| {
            workspace
                .session(document, cx)
                .unwrap()
                .read(cx)
                .page_count() as u32
        })
    }

    /// Where each page of the active pane paints.
    fn pages(&mut self) -> Vec<(u32, Bounds<Pixels>)> {
        let document = self.document;
        (0..self.page_count())
            .filter_map(|page| {
                let id: &'static str =
                    Box::leak(document_viewer_page_id(document, page as usize).into_boxed_str());
                self.cx.debug_bounds(id).map(|bounds| (page, bounds))
            })
            .collect()
    }

    fn page(&mut self, page: u32) -> Bounds<Pixels> {
        self.pages()
            .into_iter()
            .find(|(index, _)| *index == page)
            .unwrap_or_else(|| panic!("page {page} is not painted"))
            .1
    }

    fn current_page(&mut self) -> u32 {
        let document = self.document;
        self.workspace.read_with(self.cx, |workspace, cx| {
            workspace
                .session(document, cx)
                .unwrap()
                .read(cx)
                .current_page()
        })
    }

    fn view(&mut self) -> butter_paper::native_document_view_state::NativeDocumentViewState {
        let document = self.document;
        self.workspace.read_with(self.cx, |workspace, cx| {
            workspace.document_view_state(document, cx).unwrap()
        })
    }

    fn other(
        &mut self,
    ) -> (
        butter_paper::native_document_view_state::NativeDocumentViewState,
        u32,
        (f32, f32),
    ) {
        let document = self.document;
        self.workspace.read_with(self.cx, |workspace, cx| {
            workspace
                .other_pane_view(document, cx)
                .expect("Split View is on")
        })
    }

    fn split(&mut self) -> Option<(PaneSide, f32)> {
        let document = self.document;
        self.workspace.read_with(self.cx, |workspace, cx| {
            workspace.split_view_state(document, cx)
        })
    }

    fn rectangles(&mut self) -> Vec<u32> {
        let document = self.document;
        self.workspace
            .read_with(self.cx, |workspace, cx| {
                workspace.annotation_snapshot(document, cx)
            })
            .unwrap()
            .rectangles
            .iter()
            .map(|rectangle| rectangle.page_index)
            .collect()
    }

    fn drag(&mut self, from: gpui::Point<Pixels>, to: gpui::Point<Pixels>) {
        self.cx
            .simulate_mouse_move(from, None, Modifiers::default());
        self.cx
            .simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
        self.cx
            .simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
        self.cx
            .simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
        self.settle();
    }

    fn wheel(&mut self, at: gpui::Point<Pixels>, dy: f32) {
        self.cx.simulate_mouse_move(at, None, Modifiers::default());
        self.cx.simulate_event(ScrollWheelEvent {
            position: at,
            delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
            ..Default::default()
        });
        self.settle();
    }
}

fn near(a: Pixels, b: Pixels) -> bool {
    (f32::from(a) - f32::from(b)).abs() <= TOLERANCE
}

#[gpui::test]
fn two_pages_shows_pairs_side_by_side_and_the_cover_alone(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(TwoPagesView);
    assert_eq!(h.view().mode(), PageViewMode::TwoPages);
    // The view centre falls between the pair: page 1 stays current.
    assert_eq!(h.current_page(), 0);
    let (first, second) = (h.page(0), h.page(1));
    assert!(near(first.top(), second.top()), "{first:?} {second:?}");
    assert!(second.left() > first.right(), "page 2 right of page 1");
    let viewport = h.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap();
    // The pair, Fit Width by default, fills the view across and is centred.
    assert!(near(
        first.left() - viewport.left(),
        viewport.right() - second.right()
    ));

    h.action(ToggleCoverPageAlone);
    assert!(h.view().cover_alone());
    assert_eq!(h.current_page(), 0, "the cover stays current and in view");
    let pages = h.pages();
    let cover = h.page(0);
    assert!(
        pages
            .iter()
            .all(|(page, bounds)| *page == 0 || !near(bounds.top(), cover.top())),
        "the cover shares its row: {pages:?}"
    );
    // The cover sits in the right column, as in a book: its left edge
    // half the gap right of the centre of the two-column row.
    assert!(
        cover.left() > viewport.center().x && f32::from(cover.left() - viewport.center().x) < 24.,
        "the cover is in the right column: {cover:?} in {viewport:?}"
    );
    if let (Some((_, two)), Some((_, three))) = (
        pages.iter().find(|(page, _)| *page == 1),
        pages.iter().find(|(page, _)| *page == 2),
    ) {
        assert!(near(two.top(), three.top()) && three.left() > two.right());
        assert!(near(three.left(), cover.left()), "page 3 below the cover");
    }
    h.action(ToggleCoverPageAlone);
    assert!(!h.view().cover_alone());
}

#[gpui::test]
fn two_pages_fits_the_pair_and_aligns_mixed_sizes_at_their_tops(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.action(TwoPagesView);
    h.action(FitWidth);
    let viewport = h.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap();
    let (letter, tabloid) = (h.page(0), h.page(1));
    assert!(near(letter.top(), tabloid.top()), "tops meet");
    assert!(
        (f32::from(tabloid.size.width) / f32::from(letter.size.width) - 2.).abs() < 0.01,
        "each page keeps its own size"
    );
    let row = f32::from(tabloid.right() - letter.left());
    assert!(
        (row - f32::from(viewport.size.width)).abs()
            <= 0.002 * f32::from(viewport.size.width) + TOLERANCE,
        "Fit Width fits the pair: {row} in {viewport:?}"
    );

    h.action(FitPage);
    assert_eq!(h.view().zoom_preset(), ViewerZoomPreset::FitPage);
    let (letter, tabloid) = (h.page(0), h.page(1));
    for page in [letter, tabloid] {
        assert!(
            page.left() >= viewport.left() - px(TOLERANCE)
                && page.right() <= viewport.right() + px(TOLERANCE)
                && page.top() >= viewport.top() - px(TOLERANCE)
                && page.bottom() <= viewport.bottom() + px(TOLERANCE),
            "Fit Page shows the whole pair: {page:?} in {viewport:?}"
        );
    }
    assert!(near(
        letter.left() - viewport.left(),
        viewport.right() - tabloid.right()
    ));
}

#[gpui::test]
fn two_pages_keeps_the_page_gone_to_current_and_moves_by_rows(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(TwoPagesView);
    h.action(FitPage);
    assert_eq!(h.current_page(), 0);
    // Next turns a spread: to the first page of the next row, which comes
    // into view and is current.
    h.action(NavigateNextPage);
    assert_eq!(h.current_page(), 2, "pages 3–4, page 3 current");
    let viewport = h.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap();
    let (two, three) = (h.page(2), h.page(3));
    assert!(near(two.top(), three.top()));
    assert!(
        two.top() >= viewport.top() - px(TOLERANCE)
            && two.bottom() <= viewport.bottom() + px(TOLERANCE)
    );
    h.action(NavigateNextPage);
    assert_eq!(h.current_page(), 4, "the last row: page 5 alone");
    let navigator = h
        .workspace
        .read_with(h.cx, |workspace, _| workspace.page_navigator().clone());
    assert!(!navigator.read_with(h.cx, |navigator, _| navigator.can_go_forward()));
    h.action(NavigateNextPage);
    assert_eq!(h.current_page(), 4, "no row after the last");
    h.action(NavigatePreviousPage);
    assert_eq!(h.current_page(), 2);
    h.action(NavigatePreviousPage);
    assert_eq!(h.current_page(), 0);
    assert!(!navigator.read_with(h.cx, |navigator, _| navigator.can_go_back()));
    // End goes to the first page of the last row; with the cover alone
    // (rows 1, 2–3, 4–5) that is page 4.
    h.action(NavigateEnd);
    assert_eq!(h.current_page(), 4);
    h.action(ToggleCoverPageAlone);
    h.action(NavigateEnd);
    assert_eq!(h.current_page(), 3);
    h.action(NavigatePreviousPage);
    assert_eq!(h.current_page(), 1, "pages 2–3, page 2 current");
    h.action(NavigatePreviousPage);
    assert_eq!(h.current_page(), 0, "the cover");
    h.action(ToggleCoverPageAlone);

    // The page field still goes to any page, showing its row.
    navigator.update(h.cx, |_, cx| {
        cx.emit(butter_paper::page_navigator::PageNavigatorEvent::GoTo(3))
    });
    h.settle();
    assert_eq!(h.current_page(), 3, "page 4, the right page of its row");
    let viewport = h.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap();
    let four = h.page(3);
    assert!(
        four.top() >= viewport.top() - px(TOLERANCE)
            && four.bottom() <= viewport.bottom() + px(TOLERANCE)
    );
    // Next from there turns to the next row, page 5.
    h.action(NavigateNextPage);
    assert_eq!(h.current_page(), 4);

    // Back to Continuous, the current page stays, and Next goes one page.
    h.action(ContinuousView);
    assert_eq!(h.view().mode(), PageViewMode::Continuous);
    assert_eq!(h.current_page(), 4);
    h.action(NavigatePreviousPage);
    assert_eq!(h.current_page(), 3);
}

#[gpui::test]
fn two_pages_turns_one_page_of_a_pair_on_its_own(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(TwoPagesView);
    h.action(FitPage);
    // A click on the right page makes it current.
    let right = h.page(1).center();
    h.cx.simulate_click(right, Modifiers::default());
    h.settle();
    assert_eq!(h.current_page(), 1);
    h.action(RotatePageRight);
    let (left, right) = (h.page(0), h.page(1));
    assert!(left.size.height > left.size.width, "page 1 stays upright");
    assert!(
        right.size.width > right.size.height,
        "page 2 turned: {right:?}"
    );
    assert!(near(left.top(), right.top()) && right.left() > left.right());
}

#[gpui::test]
fn a_rectangle_drawn_on_the_right_page_lands_on_it(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(TwoPagesView);
    h.action(FitPage);
    h.cx.simulate_keystrokes("r");
    h.settle();
    let right = h.page(1);
    let from = point(right.left() + px(40.), right.top() + px(60.));
    h.drag(from, point(from.x + px(80.), from.y + px(60.)));
    assert_eq!(h.rectangles(), vec![1]);
    // And on the left page of the pair.
    h.cx.simulate_keystrokes("r");
    h.settle();
    let left = h.page(0);
    let from = point(left.left() + px(40.), left.top() + px(60.));
    h.drag(from, point(from.x + px(80.), from.y + px(60.)));
    let mut pages = h.rectangles();
    pages.sort();
    assert_eq!(pages, vec![0, 1]);
}

#[gpui::test]
fn split_view_shows_two_panes_with_their_own_zoom_and_scroll(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(ToggleSplitView);
    assert_eq!(h.split(), Some((PaneSide::Left, 0.5)));
    let left = h.cx.debug_bounds(SPLIT_PANE_LEFT_ID).unwrap();
    let right = h.cx.debug_bounds(SPLIT_PANE_RIGHT_ID).unwrap();
    assert!(
        near(left.size.width, right.size.width),
        "{left:?} {right:?}"
    );
    let ring = h.cx.debug_bounds(SPLIT_ACTIVE_RING_ID).unwrap();
    assert!(
        near(ring.left(), left.left()),
        "the left pane is ringed active"
    );
    let other_viewport = h.cx.debug_bounds(SPLIT_OTHER_VIEWPORT_ID).unwrap();
    assert!(other_viewport.left() >= right.left() - px(TOLERANCE));
    let other_pages = h.workspace.read_with(h.cx, |workspace, cx| {
        workspace.other_pane_visible_pages(workspace.active_document_id().unwrap(), cx)
    });
    assert!(
        other_pages.is_some_and(|pages| !pages.is_empty()),
        "the other pane lays out pages"
    );

    let (other_before, _, other_scroll) = h.other();
    h.action(ZoomIn);
    h.action(ZoomIn);
    let active_zoom = h.view().zoom_percent();
    let (other_after, _, other_scroll_after) = h.other();
    assert!(active_zoom > other_after.zoom_percent() + 1.);
    assert_eq!(other_before.zoom_percent(), other_after.zoom_percent());
    let viewport = h.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap();
    let scroll_before = h.view().scroll();
    h.wheel(viewport.center(), -300.);
    assert!(
        h.view().scroll().1 > scroll_before.1,
        "the active pane scrolled"
    );
    assert_eq!(other_scroll, other_scroll_after);
    assert_eq!(h.other().2, other_scroll_after, "the other pane did not");

    h.action(ToggleSplitView);
    assert_eq!(h.split(), None);
    assert!(
        (h.view().zoom_percent() - active_zoom).abs() < 0.01,
        "the active pane's view stays"
    );
    assert!(h.cx.debug_bounds(SPLIT_OTHER_VIEWPORT_ID).is_none());
}

#[gpui::test]
fn split_view_panes_share_edits_and_undo_and_the_toolbar_acts_on_the_active_one(
    cx: &mut TestAppContext,
) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(ToggleSplitView);
    h.cx.simulate_keystrokes("r");
    h.settle();
    let page = h.page(0);
    let from = point(page.left() + px(40.), page.top() + px(60.));
    h.drag(from, point(from.x + px(80.), from.y + px(60.)));
    assert_eq!(h.rectangles(), vec![0]);
    let document = h.document;
    let scene = h.workspace.read_with(h.cx, |workspace, cx| {
        workspace.thumbnail_annotation_scene(document, 0, cx)
    });
    assert_eq!(
        scene.rectangles.len(),
        1,
        "the other pane draws the same markup"
    );

    // A press in the right pane makes it active and draws nothing.
    let right = h.cx.debug_bounds(SPLIT_PANE_RIGHT_ID).unwrap();
    h.cx.simulate_keystrokes("r");
    h.settle();
    h.drag(
        right.center(),
        point(right.center().x + px(60.), right.center().y + px(40.)),
    );
    assert_eq!(h.split().map(|(side, _)| side), Some(PaneSide::Right));
    assert_eq!(h.rectangles(), vec![0], "the activating press drew nothing");
    let ring = h.cx.debug_bounds(SPLIT_ACTIVE_RING_ID).unwrap();
    assert!(
        ring.left() >= right.left() - px(TOLERANCE),
        "the ring moved right"
    );

    // Undo there undoes the edit made in the left pane.
    let dirty = |h: &mut Harness<'_>| {
        h.workspace
            .read_with(h.cx, |workspace, cx| {
                workspace.annotation_snapshot(document, cx)
            })
            .unwrap()
            .dirty
    };
    assert!(dirty(&mut h));
    h.workspace.update(h.cx, |workspace, cx| {
        workspace.undo_annotations(document, cx).unwrap();
    });
    h.settle();
    assert!(h.rectangles().is_empty());

    // The toolbar's zoom acts on the right pane only.
    let left_zoom = h.other().0.zoom_percent();
    h.action(ZoomIn);
    assert!((h.other().0.zoom_percent() - left_zoom).abs() < 0.01);
    assert!(h.view().zoom_percent() > left_zoom + 1.);
    assert_eq!(h.view().zoom_preset(), ViewerZoomPreset::Manual);
    assert_ne!(h.other().0.zoom_preset(), ViewerZoomPreset::Manual);
}

#[gpui::test]
fn the_split_view_divider_drags_and_moves_from_the_keyboard(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(ToggleSplitView);
    let divider = h.cx.debug_bounds(SPLIT_DIVIDER_ID).unwrap();
    let left_before = h.cx.debug_bounds(SPLIT_PANE_LEFT_ID).unwrap().size.width;
    let start = divider.center();
    h.drag(start, point(start.x + px(120.), start.y));
    let left_after = h.cx.debug_bounds(SPLIT_PANE_LEFT_ID).unwrap().size.width;
    assert!(
        (f32::from(left_after - left_before) - 120.).abs() <= TOLERANCE,
        "{left_before:?} → {left_after:?}"
    );
    // Dragging far stops at the least pane width.
    let divider = h.cx.debug_bounds(SPLIT_DIVIDER_ID).unwrap();
    h.drag(divider.center(), point(px(1_270.), divider.center().y));
    let right = h.cx.debug_bounds(SPLIT_PANE_RIGHT_ID).unwrap();
    assert!(
        (f32::from(right.size.width) - butter_paper::document_session::MIN_PANE_WIDTH).abs()
            <= TOLERANCE,
        "{right:?}"
    );

    // The divider has the focus after the drag: Left moves it 16 px, Enter
    // puts it back in the middle.
    let before = h.cx.debug_bounds(SPLIT_PANE_LEFT_ID).unwrap().size.width;
    h.cx.simulate_keystrokes("left");
    h.settle();
    let after = h.cx.debug_bounds(SPLIT_PANE_LEFT_ID).unwrap().size.width;
    assert!(
        (f32::from(before - after) - 16.).abs() <= TOLERANCE,
        "{before:?} → {after:?}"
    );
    h.cx.simulate_keystrokes("enter");
    h.settle();
    let left = h.cx.debug_bounds(SPLIT_PANE_LEFT_ID).unwrap();
    let right = h.cx.debug_bounds(SPLIT_PANE_RIGHT_ID).unwrap();
    assert!(near(left.size.width, right.size.width));
    // The pages did not move because of the keys: the divider took them.
    assert_eq!(h.split().map(|(side, _)| side), Some(PaneSide::Left));
}
