//! Zoom, pan and page-view behaviour, checked against the painted page
//! geometry rather than internal scroll values.
//!
//! Every check reads where pages actually land in the viewport after the
//! workspace has settled, so a stale scroll offset, a jump during a zoom or a
//! page lost off-screen fails here even if the view state looks consistent.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use butter_paper::application_shell::{PageTurningPreference, ZoomSpeedPreference};
use butter_paper::document_workspace::{
    ActualSize, ContinuousView, DOCUMENT_VIEWPORT_ID, DocumentId, DocumentOpenBatchRequest,
    DocumentOpenOrigin, DocumentWorkspace, FitPage, FitWidth, NativeDocumentOpener,
    NativeDocumentResource, NavigateDown, NavigateEnd, NavigateHome, NavigateNextPage,
    NavigatePageDown, NavigatePageUp, NavigatePreviousPage, NavigateUp, OpenDocumentRequest,
    OpenedNativeDocument, RasterSurface, SinglePageView, ThumbnailSurface, ZoomIn, ZoomOut,
    document_session_tab_id, document_viewer_page_id, init_document_workspace_actions,
};
use butter_paper::mouse_wheel::{NOTCH_LINES, PageTurning, ZoomSpeed};
use butter_paper::native_document_view_state::{RememberedPageView, RestartZoom, ViewerZoomPreset};
use butter_paper::page_view_control::{
    CONTINUOUS_PRIMARY_ID, PageViewMode, SINGLE_PAGE_PRIMARY_ID, WheelBehavior,
};
use gpui::{
    AppContext as _, Bounds, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels,
    ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase, VisualTestContext, point, px, size,
};
use gpui_component::Root;

/// `PAN_KEEP_VISIBLE_PX` in the viewer.
const KEEP: f32 = 48.;
/// Painted geometry is compared to within a pixel or two of rounding.
const TOLERANCE: f32 = 2.;

#[gpui::test]
fn stationary_pointer_keeps_visible_tool_cursors_after_keyboard_shortcuts(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    let at = h.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap().center();
    h.cx.simulate_mouse_move(at, None, Modifiers::default());
    for (shortcut, expected) in [
        ("r", gpui::CursorStyle::Crosshair),
        ("v", gpui::CursorStyle::Arrow),
        ("e", gpui::CursorStyle::Crosshair),
        ("v", gpui::CursorStyle::Arrow),
    ] {
        h.cx.simulate_keystrokes(shortcut);
        h.settle();
        assert_eq!(
            h.cx.update(|_, cx| cx.cursor_hide_mode()),
            gpui::CursorHideMode::Never
        );
        assert_eq!(h.cx.update(|window, _| window.mouse_position()), at);
        assert_eq!(
            h.workspace
                .read_with(h.cx, |workspace, _| workspace.viewport_cursor()),
            expected
        );
    }
}

/// Letter portrait, tabloid landscape, letter portrait, a small card and
/// letter portrait: mixed sizes expose fits that follow the current page.
const MIXED_PAGES: [(f32, f32); 5] = [
    (612., 792.),
    (1224., 792.),
    (612., 792.),
    (300., 400.),
    (612., 792.),
];
const UNIFORM_PAGES: [(f32, f32); 5] = [(612., 792.); 5];

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
        let pages = if request
            .path
            .file_name()
            .is_some_and(|name| name.as_encoded_bytes().starts_with(b"uniform"))
        {
            &UNIFORM_PAGES[..]
        } else {
            self.0
        };
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
    /// The last settle ended with the viewer still re-planning.
    unsettled: bool,
}

#[derive(Clone, PartialEq)]
struct Frame {
    mode: PageViewMode,
    preset: ViewerZoomPreset,
    zoom: f32,
    current_page: u32,
    scroll: (f32, f32),
    /// The viewer was still re-planning when this frame was taken.
    unsettled: bool,
    window: gpui::Size<Pixels>,
    viewport: Bounds<Pixels>,
    /// Painted bounds of every page the viewer laid out this frame.
    pages: Vec<(u32, Bounds<Pixels>)>,
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let v = self.viewport;
        write!(
            f,
            "{}{:?} {:?} {}% page {} scroll ({:.0}, {:.0}) viewport [{:.0},{:.0} {:.0}x{:.0}] pages",
            if self.unsettled { "UNSETTLED " } else { "" },
            self.mode,
            self.preset,
            self.zoom,
            self.current_page,
            self.scroll.0,
            self.scroll.1,
            f32::from(v.left()),
            f32::from(v.top()),
            f32::from(v.size.width),
            f32::from(v.size.height),
        )?;
        for (page, b) in &self.pages {
            write!(
                f,
                " p{page}[{:.0},{:.0}→{:.0},{:.0}]",
                f32::from(b.left()),
                f32::from(b.top()),
                f32::from(b.right()),
                f32::from(b.bottom())
            )?;
        }
        Ok(())
    }
}

impl Frame {
    fn page(&self, index: u32) -> Option<Bounds<Pixels>> {
        self.pages
            .iter()
            .find(|(page, _)| *page == index)
            .map(|(_, bounds)| *bounds)
    }

    /// The part of a page inside the viewport, as (width, height).
    fn visible(&self, index: u32) -> (f32, f32) {
        let Some(page) = self.page(index) else {
            return (0., 0.);
        };
        let left = page.left().max(self.viewport.left());
        let right = page.right().min(self.viewport.right());
        let top = page.top().max(self.viewport.top());
        let bottom = page.bottom().min(self.viewport.bottom());
        (
            f32::from(right - left).max(0.),
            f32::from(bottom - top).max(0.),
        )
    }

    /// Some page keeps at least the pan strip in view on both axes.
    fn some_page_in_view(&self) -> bool {
        self.pages.iter().any(|(page, _)| {
            let (width, height) = self.visible(*page);
            width >= KEEP - TOLERANCE && height >= KEEP - TOLERANCE
        })
    }

    fn whole_page_in_view(&self, index: u32) -> bool {
        self.page(index).is_some_and(|page| {
            page.left() >= self.viewport.left() - px(TOLERANCE)
                && page.right() <= self.viewport.right() + px(TOLERANCE)
                && page.top() >= self.viewport.top() - px(TOLERANCE)
                && page.bottom() <= self.viewport.bottom() + px(TOLERANCE)
        })
    }

    /// Left and right blank space beside a page.
    fn horizontal_gaps(&self, index: u32) -> (f32, f32) {
        let page = self.page(index).unwrap();
        (
            f32::from(page.left() - self.viewport.left()),
            f32::from(self.viewport.right() - page.right()),
        )
    }

    /// The page and the fraction across it under a viewport point.
    fn document_point(&self, at: gpui::Point<Pixels>) -> Option<(u32, f32, f32)> {
        self.pages.iter().find_map(|(page, bounds)| {
            bounds.contains(&at).then(|| {
                (
                    *page,
                    f32::from(at.x - bounds.left()) / f32::from(bounds.size.width),
                    f32::from(at.y - bounds.top()) / f32::from(bounds.size.height),
                )
            })
        })
    }

    /// Where a document point now paints, if its page is laid out.
    fn screen_point(&self, (page, fx, fy): (u32, f32, f32)) -> Option<gpui::Point<Pixels>> {
        let bounds = self.page(page)?;
        Some(point(
            bounds.left() + bounds.size.width * fx,
            bounds.top() + bounds.size.height * fy,
        ))
    }
}

fn distance(a: gpui::Point<Pixels>, b: gpui::Point<Pixels>) -> f32 {
    f32::from(a.x - b.x).hypot(f32::from(a.y - b.y))
}

fn setup<'a>(cx: &'a mut TestAppContext, pages: &'static [(f32, f32)]) -> Harness<'a> {
    setup_tabs(cx, pages, &["mixed.pdf"]).0
}

fn setup_tabs<'a>(
    cx: &'a mut TestAppContext,
    pages: &'static [(f32, f32)],
    names: &[&str],
) -> (Harness<'a>, Vec<DocumentId>) {
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
            let workspace =
                cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(Opener(pages)), cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.simulate_resize(size(px(1280.), px(800.)));
    let documents = names
        .iter()
        .map(|name| {
            let id = workspace.update(cx, |workspace, cx| {
                workspace.open_path(PathBuf::from(name), cx)
            });
            cx.run_until_parked();
            id
        })
        .collect::<Vec<_>>();
    let focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| {
        window.activate_window();
        focus.focus(window, cx);
    });
    let mut harness = Harness {
        cx,
        workspace,
        document: *documents.last().unwrap(),
        unsettled: false,
    };
    harness.settle();
    (harness, documents)
}

impl Harness<'_> {
    /// Draws and drains work until the viewer stops re-planning.
    ///
    /// The test platform runs no next-frame callbacks, so this re-plans the
    /// painted viewport the way the app's next-frame refresh does.
    fn settle(&mut self) {
        let mut previous = None;
        for _ in 0..16 {
            self.cx.update(|window, cx| window.draw(cx).clear(cx));
            let document = self.document;
            let generation = self.workspace.read_with(self.cx, |workspace, cx| {
                workspace
                    .viewer_snapshot(document, cx)
                    .map(|snapshot| snapshot.generation)
            });
            if generation.is_some_and(|generation| generation != 0) && generation == previous {
                self.unsettled = false;
                return;
            }
            previous = generation;
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
            self.cx.run_until_parked();
        }
        // Still re-planning after many frames: record it and carry on.
        self.unsettled = true;
    }

    fn frame(&mut self) -> Frame {
        self.settle();
        let document = self.document;
        let (view, current_page, page_count) =
            self.workspace.read_with(self.cx, |workspace, cx| {
                let session = workspace.session(document, cx).unwrap().read(cx);
                (
                    workspace.document_view_state(document, cx).unwrap(),
                    session.current_page(),
                    session.page_count() as u32,
                )
            });
        let viewport = self.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap();
        let pages = (0..page_count)
            .filter_map(|page| {
                let id: &'static str =
                    Box::leak(document_viewer_page_id(document, page as usize).into_boxed_str());
                self.cx.debug_bounds(id).map(|bounds| (page, bounds))
            })
            .collect();
        Frame {
            mode: view.mode(),
            preset: view.zoom_preset(),
            zoom: view.zoom_percent(),
            current_page,
            scroll: view.scroll(),
            unsettled: self.unsettled,
            window: self.cx.update(|window, _| window.viewport_size()),
            viewport,
            pages,
        }
    }

    fn action(&mut self, action: impl gpui::Action) {
        self.cx.dispatch_action(action);
        self.settle();
    }

    fn click(&mut self, id: &'static str) {
        self.settle();
        let target = self.cx.debug_bounds(id).unwrap().center();
        self.cx.simulate_click(target, Modifiers::default());
        self.settle();
    }

    fn double_click(&mut self, id: &'static str) {
        self.settle();
        let target = self.cx.debug_bounds(id).unwrap().center();
        for click_count in 1..=2 {
            self.cx.simulate_event(MouseDownEvent {
                button: MouseButton::Left,
                position: target,
                modifiers: Modifiers::default(),
                click_count,
                first_mouse: false,
            });
            self.cx.simulate_event(MouseUpEvent {
                button: MouseButton::Left,
                position: target,
                modifiers: Modifiers::default(),
                click_count,
            });
        }
        self.settle();
    }

    /// Middle-button drag: the pages follow the pointer by (dx, dy).
    fn pan(&mut self, dx: f32, dy: f32) {
        self.settle();
        let start = self.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap().center();
        let end = point(start.x + px(dx), start.y + px(dy));
        self.cx
            .simulate_mouse_down(start, MouseButton::Middle, Modifiers::default());
        self.cx
            .simulate_mouse_move(end, Some(MouseButton::Middle), Modifiers::default());
        self.cx
            .simulate_mouse_up(end, MouseButton::Middle, Modifiers::default());
        self.settle();
    }

    /// A middle-button press and release without moving.
    fn middle_click(&mut self) {
        self.settle();
        let at = self.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap().center();
        self.cx
            .simulate_mouse_down(at, MouseButton::Middle, Modifiers::default());
        self.cx
            .simulate_mouse_up(at, MouseButton::Middle, Modifiers::default());
        self.settle();
    }

    fn middle_double_click(&mut self) {
        self.middle_click();
        self.middle_click();
    }

    fn wheel(&mut self, dx: f32, dy: f32, control: bool) {
        self.settle();
        let at = self.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap().center();
        self.wheel_at(at, dx, dy, control);
    }

    fn wheel_at(&mut self, at: gpui::Point<Pixels>, dx: f32, dy: f32, control: bool) {
        // GPUI routes wheel events to the hovered hitbox.
        self.cx.simulate_mouse_move(at, None, Modifiers::default());
        self.cx.simulate_event(ScrollWheelEvent {
            position: at,
            delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
            modifiers: Modifiers {
                control,
                ..Modifiers::default()
            },
            ..Default::default()
        });
        self.settle();
    }

    /// One two-finger swipe, in three events; negative swipes towards the
    /// next page.
    fn swipe(&mut self, dy: f32) {
        self.settle();
        let at = self.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap().center();
        self.cx.simulate_mouse_move(at, None, Modifiers::default());
        for (touch_phase, delta) in [
            (TouchPhase::Started, dy / 2.),
            (TouchPhase::Moved, dy / 2.),
            (TouchPhase::Ended, 0.),
        ] {
            self.cx.simulate_event(ScrollWheelEvent {
                position: at,
                delta: ScrollDelta::Pixels(point(px(0.), px(delta))),
                touch_phase,
                ..Default::default()
            });
        }
        self.settle();
    }

    /// Notches of a discrete mouse wheel after a clear pause; negative turns
    /// it down.
    fn notch(&mut self, notches: f32) {
        self.notch_after(600, notches);
    }

    /// Notches of a discrete mouse wheel `gap_ms` after the last event.
    fn notch_after(&mut self, gap_ms: u64, notches: f32) {
        self.settle();
        self.cx
            .executor()
            .advance_clock(std::time::Duration::from_millis(gap_ms));
        let at = self.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap().center();
        self.cx.simulate_mouse_move(at, None, Modifiers::default());
        self.cx.simulate_event(ScrollWheelEvent {
            position: at,
            delta: ScrollDelta::Lines(point(0., notches * NOTCH_LINES)),
            ..Default::default()
        });
        self.settle();
    }

    fn set_page_turning(&mut self, turning: PageTurning) {
        self.cx
            .update(|_, cx| cx.set_global(PageTurningPreference(turning)));
    }

    fn remembered(&mut self, page: u32) -> Option<RememberedPageView> {
        let document = self.document;
        self.workspace.read_with(self.cx, |workspace, cx| {
            workspace
                .document_view_state(document, cx)
                .unwrap()
                .page_view(page)
        })
    }

    fn single_page_wheel_scrolls(&mut self) {
        let document = self.document;
        self.workspace.update(self.cx, |workspace, cx| {
            workspace.set_document_wheel_behavior(
                document,
                PageViewMode::SinglePage,
                WheelBehavior::Scroll,
                cx,
            );
        });
        self.settle();
    }

    fn set_zoom(&mut self, zoom: f32) {
        let document = self.document;
        self.workspace.update(self.cx, |workspace, cx| {
            let mode = workspace.document_view_state(document, cx).unwrap().mode();
            workspace.set_view_configuration(document, mode, zoom, cx);
        });
        self.settle();
    }

    fn go_to_page(&mut self, page: u32) {
        for _ in 0..8 {
            let current = self.frame().current_page;
            if current == page {
                return;
            }
            if current < page {
                self.action(NavigateNextPage);
            } else {
                self.action(NavigatePreviousPage);
            }
        }
        panic!("could not reach page {page}: {:?}", self.frame());
    }

    fn switch_to(&mut self, document: DocumentId) {
        let id: &'static str = Box::leak(document_session_tab_id(document).into_boxed_str());
        self.click(id);
        self.document = document;
        self.settle();
    }
}

// ---------------------------------------------------------------------------
// Fit presets
// ---------------------------------------------------------------------------

#[gpui::test]
fn fit_page_frames_the_whole_current_page_after_panning_into_blank_space(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.action(SinglePageView);
    h.go_to_page(2);
    h.action(FitPage);
    let fitted = h.frame();
    assert!(
        fitted.whole_page_in_view(2),
        "Fit Page shows the whole page: {fitted:?}"
    );

    h.pan(500., 350.);
    h.action(FitPage);
    let refitted = h.frame();
    assert!(
        refitted.whole_page_in_view(2),
        "Fit Page again after panning away brings the whole page back: {refitted:?}"
    );
    let (left, right) = refitted.horizontal_gaps(2);
    assert!(
        (left - right).abs() <= TOLERANCE * 2.,
        "Fit Page centres the page horizontally: gaps {left} / {right}"
    );
}

#[gpui::test]
fn fit_width_fills_and_centres_the_current_page_after_zooming_and_panning(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.go_to_page(2);
    h.set_zoom(400.);
    h.pan(-300., 0.);
    h.action(FitWidth);
    let fitted = h.frame();
    assert_eq!(fitted.preset, ViewerZoomPreset::FitWidth);
    assert_eq!(fitted.current_page, 2, "Fit Width keeps the current page");
    let (left, right) = fitted.horizontal_gaps(2);
    assert!(
        left >= -TOLERANCE && right >= -TOLERANCE && (left - right).abs() <= TOLERANCE * 2.,
        "Fit Width shows the page's full width, centred: gaps {left} / {right}, {fitted:?}"
    );
    assert!(
        fitted.visible(2).1 >= f32::from(fitted.viewport.size.height) * 0.5,
        "Fit Width keeps the current page filling the view: {fitted:?}"
    );
}

#[gpui::test]
fn fit_page_after_a_window_resize_still_frames_the_page(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.action(SinglePageView);
    h.go_to_page(1);
    h.action(FitPage);
    for (width, height) in [(900., 600.), (1600., 1000.), (700., 900.)] {
        h.cx.simulate_resize(size(px(width), px(height)));
        let frame = h.frame();
        assert!(
            frame.whole_page_in_view(1),
            "Fit Page refits on resize to {width}x{height}: {frame:?}"
        );
    }
}

#[gpui::test]
fn fit_width_refit_on_a_narrower_window_stays_on_the_first_page(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(FitWidth);
    let top = h.frame();
    assert!(top.scroll.1 >= -TOLERANCE, "starts at the top: {top:?}");
    // Each narrower window zooms out about the view centre; the refit must
    // not leave blank space above the first page beyond its margin.
    for width in [1100., 950., 830.] {
        h.cx.simulate_resize(size(px(width), px(800.)));
        let frame = h.frame();
        assert_eq!(frame.preset, ViewerZoomPreset::FitWidth);
        assert!(
            frame.scroll.1 >= -TOLERANCE,
            "Fit Width refit at {width} scrolled above the first page: {frame:?}"
        );
    }
}

#[gpui::test]
fn fit_width_scrolling_across_mixed_page_widths_does_not_jump(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.action(FitWidth);
    let mut before = h.frame();
    let step = 60.;
    let mut jumps = Vec::new();
    let mut checked = 0;
    for _ in 0..120 {
        let centre = before.viewport.center();
        let Some(anchor) = before.document_point(centre) else {
            h.wheel(0., -step, false);
            before = h.frame();
            continue;
        };
        h.wheel(0., -step, false);
        let after = h.frame();
        if let Some(moved_to) = after.screen_point(anchor) {
            checked += 1;
            // A wheel step moves the page under the centre up by the step.
            let expected = point(centre.x, centre.y - px(step));
            let error = distance(moved_to, expected);
            if error > 4. {
                jumps.push(format!(
                    "page {} zoom {}→{} moved {error:.0}px off",
                    anchor.0, before.zoom, after.zoom
                ));
            }
        }
        if after.current_page as usize == MIXED_PAGES.len() - 1 {
            break;
        }
        before = after;
    }
    assert!(checked >= 20, "only {checked} scroll steps were measurable");
    assert!(
        jumps.is_empty(),
        "scrolling in Fit Width must not jump: {jumps:#?}"
    );
}

// ---------------------------------------------------------------------------
// Page modes
// ---------------------------------------------------------------------------

#[gpui::test]
fn continuous_to_single_page_keeps_the_current_page_in_view(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.go_to_page(3);
    let continuous = h.frame();
    assert_eq!(continuous.current_page, 3);
    h.action(SinglePageView);
    let single = h.frame();
    assert_eq!(single.mode, PageViewMode::SinglePage);
    assert_eq!(single.current_page, 3, "the page survives the mode switch");
    assert!(
        single.visible(3).1 >= f32::from(single.viewport.size.height) * 0.5,
        "single page mode shows the page that was current: {single:?}"
    );
}

#[gpui::test]
fn single_page_to_continuous_keeps_the_current_page_in_view(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(SinglePageView);
    h.go_to_page(3);
    h.action(ContinuousView);
    let continuous = h.frame();
    assert_eq!(continuous.mode, PageViewMode::Continuous);
    assert_eq!(
        continuous.current_page, 3,
        "the page survives the mode switch"
    );
    assert!(
        continuous.visible(3).1 >= f32::from(continuous.viewport.size.height) * 0.5,
        "continuous mode opens on the page that was current: {continuous:?}"
    );
}

#[gpui::test]
fn mode_round_trip_returns_to_the_same_place(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.set_zoom(150.);
    h.go_to_page(2);
    h.wheel(0., -200., false);
    let start = h.frame();
    h.action(SinglePageView);
    h.action(ContinuousView);
    let back = h.frame();
    assert_eq!(back.current_page, start.current_page);
    let page = start.current_page;
    let (before, after) = (start.page(page).unwrap(), back.page(page).unwrap());
    assert!(
        (f32::from(before.top() - after.top())).abs() <= 1.5 * KEEP,
        "continuous → single → continuous keeps roughly the same view: {before:?} → {after:?}"
    );
}

#[gpui::test]
fn double_clicking_continuous_fits_width_on_the_current_page(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(SinglePageView);
    h.go_to_page(2);
    h.set_zoom(300.);
    h.pan(400., 300.);
    h.double_click(CONTINUOUS_PRIMARY_ID);
    let frame = h.frame();
    assert_eq!(frame.mode, PageViewMode::Continuous);
    assert_eq!(frame.preset, ViewerZoomPreset::FitWidth);
    assert_eq!(frame.current_page, 2);
    let (left, right) = frame.horizontal_gaps(2);
    assert!(
        left >= -TOLERANCE && right >= -TOLERANCE && (left - right).abs() <= TOLERANCE * 2.,
        "double-click Continuous shows the full page width: gaps {left} / {right}, {frame:?}"
    );
    assert!(
        frame.visible(2).1 >= f32::from(frame.viewport.size.height) * 0.5,
        "double-click Continuous keeps the current page in view: {frame:?}"
    );
}

#[gpui::test]
fn double_clicking_single_page_fits_the_whole_current_page(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.set_zoom(300.);
    h.go_to_page(3);
    h.pan(-250., -400.);
    h.double_click(SINGLE_PAGE_PRIMARY_ID);
    let frame = h.frame();
    assert_eq!(frame.mode, PageViewMode::SinglePage);
    assert_eq!(frame.preset, ViewerZoomPreset::FitPage);
    assert_eq!(frame.current_page, 3);
    assert!(
        frame.whole_page_in_view(3),
        "double-click Single Page shows the whole current page: {frame:?}"
    );
}

#[gpui::test]
fn page_view_buttons_switch_modes_with_one_click(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.click(SINGLE_PAGE_PRIMARY_ID);
    assert_eq!(h.frame().mode, PageViewMode::SinglePage);
    h.click(CONTINUOUS_PRIMARY_ID);
    assert_eq!(h.frame().mode, PageViewMode::Continuous);
}

// ---------------------------------------------------------------------------
// Zoom
// ---------------------------------------------------------------------------

fn assert_zoom_keeps_centre(h: &mut Harness<'_>, label: &str, zoom: impl FnOnce(&mut Harness<'_>)) {
    let before = h.frame();
    let centre = before.viewport.center();
    let anchor = before
        .document_point(centre)
        .unwrap_or_else(|| panic!("{label}: no page under the viewport centre: {before:?}"));
    zoom(h);
    let after = h.frame();
    assert!(
        (after.zoom - before.zoom).abs() > 0.5,
        "{label} changes the zoom: {} → {}",
        before.zoom,
        after.zoom
    );
    let moved_to = after
        .screen_point(anchor)
        .unwrap_or_else(|| panic!("{label}: the centred page left the layout: {after:?}"));
    let error = distance(moved_to, centre);
    assert!(
        error <= 4.,
        "{label} keeps the document point at the viewport centre ({} → {}%): it moved {error:.0}px",
        before.zoom,
        after.zoom
    );
}

#[gpui::test]
fn zoom_in_and_out_commands_zoom_about_the_view_centre(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.set_zoom(200.);
    h.go_to_page(1);
    h.wheel(-150., -300., false);
    assert_zoom_keeps_centre(&mut h, "Zoom In", |h| h.action(ZoomIn));
    assert_zoom_keeps_centre(&mut h, "Zoom Out", |h| h.action(ZoomOut));
    assert_zoom_keeps_centre(&mut h, "Actual Size", |h| h.action(ActualSize));
}

#[gpui::test]
fn zoom_commands_in_single_page_mode_keep_the_page_in_view(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(SinglePageView);
    h.go_to_page(2);
    h.action(FitPage);
    for _ in 0..6 {
        h.action(ZoomIn);
        let frame = h.frame();
        assert!(
            frame.visible(2).0 > 0. && frame.visible(2).1 > 0.,
            "zooming in keeps the page in view at {}%: {frame:?}",
            frame.zoom
        );
    }
    for _ in 0..12 {
        h.action(ZoomOut);
        let frame = h.frame();
        assert!(
            frame.whole_page_in_view(2) || frame.some_page_in_view(),
            "zooming out keeps the page in view at {}%: {frame:?}",
            frame.zoom
        );
    }
}

#[gpui::test]
fn control_wheel_zoom_far_out_and_back_never_loses_the_pages(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.go_to_page(2);
    let pointer = {
        let frame = h.frame();
        point(
            frame.viewport.left() + px(120.),
            frame.viewport.top() + px(90.),
        )
    };
    for step in 0..40 {
        h.wheel_at(pointer, 0., -120., true);
        let frame = h.frame();
        assert!(
            frame.some_page_in_view(),
            "zooming out (step {step}, {}%) keeps a page in view: {frame:?}",
            frame.zoom
        );
    }
    for step in 0..40 {
        h.wheel_at(pointer, 0., 120., true);
        let frame = h.frame();
        assert!(
            frame.some_page_in_view(),
            "zooming back in (step {step}, {}%) keeps a page in view: {frame:?}",
            frame.zoom
        );
    }
}

#[gpui::test]
fn rapid_pinch_events_keep_the_painted_page_point_fixed(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.set_zoom(150.);
    h.go_to_page(2);
    h.wheel(0., -300., false);
    let before = h.frame();
    let at = before.viewport.center();
    let anchor = before.document_point(at).unwrap();
    h.cx.simulate_mouse_move(at, None, Modifiers::default());
    // A trackpad can deliver several magnification steps before the next frame.
    h.cx.update(|window, cx| {
        for _ in 0..5 {
            window.dispatch_event(
                gpui::PlatformInput::Pinch(gpui::PinchEvent {
                    position: at,
                    delta: 0.04,
                    phase: gpui::TouchPhase::Moved,
                    ..Default::default()
                }),
                cx,
            );
        }
    });
    let after = h.frame();
    assert!(
        distance(after.screen_point(anchor).unwrap(), at) <= 4.,
        "pinch burst moved the painted point: {before:?} → {after:?}"
    );
    assert_eq!(after.current_page, 2);
}

#[gpui::test]
fn scrolled_pages_behind_chrome_cannot_start_canvas_selection(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.set_zoom(200.);
    h.go_to_page(2);
    h.wheel(0., -300., false);
    let viewport = h.frame().viewport;
    let primary = h.cx.debug_bounds(CONTINUOUS_PRIMARY_ID).unwrap();
    let caret = point(primary.right() + px(10.), primary.center().y);
    h.cx.simulate_click(caret, Modifiers::default());
    h.cx.simulate_keystrokes("escape");
    h.cx.simulate_mouse_move(viewport.center(), None, Modifiers::default());
    let document = h.document;
    assert!(
        h.workspace
            .read_with(h.cx, |workspace, cx| workspace
                .active_selection_marquee(document, cx))
            .is_none()
    );
    let status = h.cx.debug_bounds("document-status-bar").unwrap();
    h.cx.simulate_click(status.center(), Modifiers::default());
    h.cx.simulate_mouse_move(viewport.center(), None, Modifiers::default());
    assert!(
        h.workspace
            .read_with(h.cx, |workspace, cx| workspace
                .active_selection_marquee(document, cx))
            .is_none()
    );
}

#[gpui::test]
fn contextual_status_bar_spans_the_window_and_stays_below_the_viewport(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    for width in [1280., 800., 1600.] {
        h.cx.simulate_resize(size(px(width), px(800.)));
        let frame = h.frame();
        let status = h.cx.debug_bounds("document-status-bar").unwrap();
        assert_eq!(status.left(), px(0.));
        assert_eq!(status.size.width, frame.window.width);
        assert!(status.top() >= frame.viewport.bottom());
        assert_eq!(status.bottom(), frame.window.height);
        assert!(status.size.height < px(40.));
    }
}

/// Scrolls a zoomed-in Single Page with `step` until the page turns,
/// checking it reached its edge first, and returns the frames before and
/// after the turn.
fn scroll_until_the_page_turns(h: &mut Harness, step: impl Fn(&mut Harness)) -> (Frame, Frame) {
    let mut before = h.frame();
    for _ in 0..200 {
        step(h);
        let after = h.frame();
        if after.current_page != before.current_page {
            return (before, after);
        }
        before = after;
    }
    panic!("the page never turned: {before:?}");
}

fn single_page_scrolling_past_the_edge_turns_to_the_same_place_on_the_next_page(
    cx: &mut TestAppContext,
    down: impl Fn(&mut Harness),
    up: impl Fn(&mut Harness),
) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(SinglePageView);
    h.single_page_wheel_scrolls();
    h.go_to_page(1);
    h.set_zoom(300.);
    h.pan(-300., 0.);
    let start = h.frame();

    let (last, next) = scroll_until_the_page_turns(&mut h, &down);
    let (left, bottom) = (last.page(1).unwrap().left(), last.page(1).unwrap().bottom());
    assert!(
        bottom <= last.viewport.bottom() + px(TOLERANCE),
        "the page scrolls until its bottom is in view before turning: {last:?}"
    );
    assert_eq!(next.current_page, 2);
    assert_eq!(next.zoom, start.zoom, "the zoom is kept");
    let top = next.page(2).unwrap();
    assert!(
        (f32::from(top.top() - next.viewport.top())).abs() <= TOLERANCE,
        "the next page shows from its top edge: {next:?}"
    );
    assert!(
        (f32::from(top.left() - left)).abs() <= TOLERANCE,
        "the next page keeps the horizontal position: {last:?} → {next:?}"
    );

    let (first, previous) = scroll_until_the_page_turns(&mut h, &up);
    assert!(
        first.page(2).unwrap().top() >= first.viewport.top() - px(TOLERANCE),
        "the page scrolls back until its top is in view before turning: {first:?}"
    );
    assert_eq!(previous.current_page, 1);
    let page = previous.page(1).unwrap();
    assert!(
        (f32::from(previous.viewport.bottom() - page.bottom())).abs() <= TOLERANCE,
        "going back shows the previous page from its bottom edge: {previous:?}"
    );
    assert!(
        (f32::from(page.left() - left)).abs() <= TOLERANCE,
        "going back keeps the horizontal position: {previous:?}"
    );
}

#[gpui::test]
fn single_page_wheel_scrolls_a_zoomed_page_then_turns_to_the_same_place(cx: &mut TestAppContext) {
    single_page_scrolling_past_the_edge_turns_to_the_same_place_on_the_next_page(
        cx,
        |h| h.notch(-1.),
        |h| h.notch(1.),
    );
}

#[gpui::test]
fn single_page_arrow_keys_scroll_a_zoomed_page_then_turn_to_the_same_place(
    cx: &mut TestAppContext,
) {
    single_page_scrolling_past_the_edge_turns_to_the_same_place_on_the_next_page(
        cx,
        |h| h.action(NavigateDown),
        |h| h.action(NavigateUp),
    );
}

#[gpui::test]
fn single_page_page_keys_scroll_a_zoomed_page_then_turn_to_the_same_place(cx: &mut TestAppContext) {
    single_page_scrolling_past_the_edge_turns_to_the_same_place_on_the_next_page(
        cx,
        |h| h.action(NavigatePageDown),
        |h| h.action(NavigatePageUp),
    );
}

#[gpui::test]
fn single_page_fit_page_turns_a_page_per_notch_or_key_and_stays_framed(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.action(SinglePageView);
    h.single_page_wheel_scrolls();
    h.action(FitPage);
    let opened = h.frame();
    assert_eq!(opened.current_page, 0);
    let steps: [(&str, fn(&mut Harness), u32); 6] = [
        ("wheel down", |h| h.notch(-1.), 1),
        ("arrow down", |h| h.action(NavigateDown), 2),
        ("page down", |h| h.action(NavigatePageDown), 3),
        ("wheel up", |h| h.notch(1.), 2),
        ("arrow up", |h| h.action(NavigateUp), 1),
        ("page up", |h| h.action(NavigatePageUp), 0),
    ];
    for (name, step, page) in steps {
        step(&mut h);
        let frame = h.frame();
        assert_eq!(frame.current_page, page, "{name} turns one page: {frame:?}");
        assert!(
            frame.whole_page_in_view(page),
            "{name} keeps the whole page in view: {frame:?}"
        );
        let shown = frame.page(page).unwrap();
        let (above, below) = (
            f32::from(shown.top() - frame.viewport.top()),
            f32::from(frame.viewport.bottom() - shown.bottom()),
        );
        assert!(
            (above - below).abs() <= 2. * TOLERANCE || frame.zoom != opened.zoom,
            "{name} keeps the page centred, without a jump: {frame:?}"
        );
    }
}

#[gpui::test]
fn trackpad_scroll_ignores_mouse_zoom_preference_in_single_page(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(SinglePageView);
    h.set_zoom(200.);
    h.go_to_page(2);
    let before = h.frame();
    h.wheel(0., -120., false);
    let after = h.frame();
    assert_eq!(after.zoom, before.zoom);
    assert_eq!(after.current_page, before.current_page);
    assert!(after.scroll.1 > before.scroll.1);
    h.wheel(0., 120., true);
    assert!(h.frame().zoom > after.zoom);
}

#[gpui::test]
fn a_two_finger_swipe_turns_a_fit_page_in_single_page_view(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.action(SinglePageView);
    h.action(FitPage);
    assert_eq!(h.frame().current_page, 0);

    h.swipe(-60.);
    let frame = h.frame();
    assert_eq!(frame.current_page, 1, "swiping down turns on: {frame:?}");
    assert!(frame.whole_page_in_view(1), "{frame:?}");

    // The swipe's momentum neither turns another page nor shifts this one.
    let turned = h.frame();
    for _ in 0..12 {
        h.wheel(0., -40., false);
    }
    let after = h.frame();
    assert_eq!(after.current_page, 1, "one swipe, one page: {after:?}");
    assert_eq!(after.scroll, turned.scroll);

    h.swipe(-60.);
    assert_eq!(h.frame().current_page, 2);
    h.swipe(60.);
    assert_eq!(h.frame().current_page, 1, "swiping up turns back");
}

#[gpui::test]
fn a_two_finger_swipe_scrolls_a_zoomed_page_to_its_edge_then_the_next_swipe_turns(
    cx: &mut TestAppContext,
) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(SinglePageView);
    h.set_zoom(200.);
    h.go_to_page(1);

    // A swipe far longer than the page stops at the page's edge.
    h.swipe(-20_000.);
    let edge = h.frame();
    assert_eq!(edge.current_page, 1, "{edge:?}");
    let shown = edge.page(1).unwrap();
    assert!(
        f32::from(shown.bottom() - edge.viewport.bottom()).abs() <= page_edge_margin(200.) + 1.,
        "the page's bottom edge is in view: {edge:?}"
    );

    h.swipe(-60.);
    assert_eq!(h.frame().current_page, 2, "the next swipe turns the page");
}

#[gpui::test]
fn zoomed_out_pages_stay_centred_horizontally(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.set_zoom(25.);
    let frame = h.frame();
    let (left, right) = frame.horizontal_gaps(0);
    assert!(
        (left - right).abs() <= TOLERANCE * 2.,
        "a page narrower than the view is centred: gaps {left} / {right}"
    );
}

// ---------------------------------------------------------------------------
// Panning extents
// ---------------------------------------------------------------------------

fn assert_pan_extents(h: &mut Harness<'_>, label: &str) {
    let far = 20_000.;
    // Each far pan must stop with a strip of a real page in view, and the
    // strip is the pan limit: the next pan back moves the pages at once.
    let checks: [(&str, (f32, f32)); 4] = [
        ("right", (far, 0.)),
        ("left", (-far, 0.)),
        ("down", (0., far)),
        ("up", (0., -far * 10.)),
    ];
    let mut failures = Vec::new();
    for (direction, (dx, dy)) in checks {
        h.pan(dx, dy);
        let stopped = h.frame();
        if !stopped.some_page_in_view() {
            failures.push(format!(
                "panning {direction} leaves less than {KEEP}px of any page in view: {stopped:?}"
            ));
            continue;
        }
        let page = stopped
            .pages
            .iter()
            .find(|(page, _)| {
                let (width, height) = stopped.visible(*page);
                width >= KEEP - TOLERANCE && height >= KEEP - TOLERANCE
            })
            .unwrap()
            .0;
        let strip = match direction {
            "right" => stopped.visible(page).0,
            "left" => stopped.visible(page).0,
            "down" => stopped.visible(page).1,
            _ => stopped.visible(page).1,
        };
        if (strip - KEEP).abs() > TOLERANCE {
            failures.push(format!(
                "panning {direction} stops with {strip:.0}px of page {page} in view, not {KEEP}: {stopped:?}"
            ));
        }
        // `signum` of zero is one, so name the axis explicitly.
        let back = if dx != 0. {
            (-dx.signum() * 100., 0.)
        } else {
            (0., -dy.signum() * 100.)
        };
        h.pan(back.0, back.1);
        let moved = h.frame();
        let shift = moved.page(page).map(|bounds| {
            let before = stopped.page(page).unwrap();
            (
                f32::from(bounds.left() - before.left()),
                f32::from(bounds.top() - before.top()),
            )
        });
        if shift.is_none_or(|shift| {
            (shift.0 - back.0).abs() > TOLERANCE || (shift.1 - back.1).abs() > TOLERANCE
        }) {
            failures.push(format!(
                "after stopping {direction}, panning back 100px moved page {page} by {shift:?}: {moved:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{label}:\n{}", failures.join("\n"));
}

#[gpui::test]
fn pan_extents_hold_in_continuous_mode_at_fit_width(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.action(FitWidth);
    assert_pan_extents(&mut h, "continuous fit width");
}

#[gpui::test]
fn pan_extents_hold_in_continuous_mode_zoomed_in(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.set_zoom(400.);
    assert_pan_extents(&mut h, "continuous 400%");
}

#[gpui::test]
fn pan_extents_hold_in_continuous_mode_zoomed_out(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.set_zoom(10.);
    assert_pan_extents(&mut h, "continuous 10%");
}

#[gpui::test]
fn pan_extents_hold_in_single_page_mode(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.action(SinglePageView);
    h.go_to_page(1);
    h.action(FitPage);
    assert_pan_extents(&mut h, "single page fit page");
    h.set_zoom(400.);
    assert_pan_extents(&mut h, "single page 400%");
}

/// The viewer's blank space around the pages: none, as the pages meet the
/// viewport's edges (only the space between pages is a gap).
fn page_edge_margin(_zoom: f32) -> f32 {
    0.
}

#[gpui::test]
fn wheel_scrolling_stops_at_the_first_and_last_pages(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.set_zoom(200.);
    let gap = page_edge_margin(200.);
    for _ in 0..200 {
        h.wheel(0., -400., false);
    }
    let bottom = h.frame();
    let last = bottom.page(4).expect("the last page is in view");
    assert!(
        (f32::from(bottom.viewport.bottom() - last.bottom()) - gap).abs() <= TOLERANCE,
        "wheel scrolling down stops at the last page's bottom edge: {bottom:?}"
    );
    h.wheel(0., 300., false);
    let nudged = h.frame();
    let nudged_last = nudged.page(4).unwrap();
    assert!(
        (f32::from(nudged_last.bottom() - last.bottom()) - 300.).abs() <= TOLERANCE,
        "at the bottom, scrolling back up moves at once: {bottom:?} → {nudged:?}"
    );
    for _ in 0..200 {
        h.wheel(400., 400., false);
    }
    let top = h.frame();
    assert_eq!(
        top.current_page, 0,
        "scrolling all the way up returns to page 1"
    );
    let first = top.page(0).unwrap();
    assert!(
        (f32::from(first.top() - top.viewport.top()) - gap).abs() <= TOLERANCE,
        "wheel scrolling up stops at the first page's top edge: {top:?}"
    );
}

#[gpui::test]
fn horizontal_wheel_scrolling_stops_at_the_page_sides(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.set_zoom(300.);
    h.go_to_page(1);
    let gap = page_edge_margin(300.);
    for _ in 0..80 {
        h.wheel(-400., 0., false);
    }
    let right = h.frame();
    let (_, right_gap) = right.horizontal_gaps(1);
    assert!(
        (right_gap - gap).abs() <= TOLERANCE,
        "wheel scrolling right stops at the widest page's side margin: {right:?}"
    );
    for _ in 0..80 {
        h.wheel(400., 0., false);
    }
    let left = h.frame();
    let (left_gap, _) = left.horizontal_gaps(1);
    assert!(
        (left_gap - gap).abs() <= TOLERANCE,
        "wheel scrolling left stops at the widest page's side margin: {left:?}"
    );
}

#[gpui::test]
fn wheel_scrolling_never_goes_further_into_blank_space_a_pan_reached(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.set_zoom(200.);
    h.pan(0., 400.);
    let panned = h.frame();
    let panned_top = panned.page(0).unwrap().top();
    assert!(
        f32::from(panned_top - panned.viewport.top()) > page_edge_margin(200.) + 300.,
        "a pan reaches the blank space above the first page: {panned:?}"
    );
    h.wheel(0., 300., false);
    let further = h.frame();
    assert!(
        (f32::from(further.page(0).unwrap().top() - panned_top)).abs() <= TOLERANCE,
        "the wheel goes no further into the blank space: {panned:?} → {further:?}"
    );
    h.wheel(0., -200., false);
    let back = h.frame();
    assert!(
        (f32::from(panned_top - back.page(0).unwrap().top()) - 200.).abs() <= TOLERANCE,
        "the wheel scrolls back towards the pages: {panned:?} → {back:?}"
    );
}

#[gpui::test]
fn middle_double_click_fits_width_in_continuous_mode(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.set_zoom(300.);
    h.go_to_page(2);
    h.pan(300., 200.);
    h.middle_double_click();
    let frame = h.frame();
    assert_eq!(frame.mode, PageViewMode::Continuous);
    assert_eq!(frame.preset, ViewerZoomPreset::FitWidth);
    assert_eq!(frame.current_page, 2);
    let (left, right) = frame.horizontal_gaps(2);
    assert!(
        left >= -TOLERANCE && right >= -TOLERANCE && (left - right).abs() <= TOLERANCE * 2.,
        "a middle double-click shows the full page width: gaps {left} / {right}, {frame:?}"
    );
}

#[gpui::test]
fn middle_double_click_fits_the_page_in_single_page_mode(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(SinglePageView);
    h.go_to_page(3);
    h.set_zoom(300.);
    h.pan(-250., -400.);
    h.middle_double_click();
    let frame = h.frame();
    assert_eq!(frame.mode, PageViewMode::SinglePage);
    assert_eq!(frame.preset, ViewerZoomPreset::FitPage);
    assert_eq!(frame.current_page, 3);
    assert!(
        frame.whole_page_in_view(3),
        "a middle double-click shows the whole page: {frame:?}"
    );
}

#[gpui::test]
fn a_single_middle_click_leaves_the_view_alone(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.set_zoom(300.);
    let before = h.frame();
    h.middle_click();
    let after = h.frame();
    assert_eq!(after.preset, ViewerZoomPreset::Manual, "{after:?}");
    assert_eq!(after.zoom, before.zoom);
    assert_eq!(after.page(0), before.page(0), "{before:?} → {after:?}");
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

#[gpui::test]
fn next_page_brings_the_page_top_into_view_even_when_panned_sideways(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.set_zoom(200.);
    h.pan(900., 0.);
    for target in 1..4 {
        h.action(NavigateNextPage);
        let frame = h.frame();
        assert_eq!(frame.current_page, target);
        let page = frame.page(target).unwrap();
        assert!(
            page.top() >= frame.viewport.top() - px(TOLERANCE)
                && page.top() <= frame.viewport.top() + px(80.),
            "Next Page puts page {target}'s top at the top of the view: {frame:?}"
        );
        assert!(
            frame.visible(target).0 >= KEEP - TOLERANCE,
            "Next Page shows page {target} horizontally: {frame:?}"
        );
    }
}

#[gpui::test]
fn home_and_end_reach_the_first_and_last_pages(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.set_zoom(150.);
    h.action(NavigateEnd);
    let end = h.frame();
    let last = MIXED_PAGES.len() as u32 - 1;
    assert_eq!(end.current_page, last);
    assert!(end.visible(last).1 > 0., "End shows the last page: {end:?}");
    h.action(NavigateHome);
    let home = h.frame();
    assert_eq!(home.current_page, 0);
    let first = home.page(0).unwrap();
    assert!(
        first.top() >= home.viewport.top() - px(TOLERANCE)
            && first.top() <= home.viewport.top() + px(80.),
        "Home shows the top of the first page: {home:?}"
    );
}

#[gpui::test]
fn page_down_moves_by_most_of_a_view(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.set_zoom(200.);
    let before = h.frame();
    h.action(NavigatePageDown);
    let after = h.frame();
    let moved = f32::from(before.page(0).unwrap().top() - after.page(0).unwrap().top());
    let expected = f32::from(before.viewport.size.height) * 0.9;
    assert!(
        (moved - expected).abs() <= TOLERANCE,
        "Page Down scrolls 90% of the view: moved {moved}, expected {expected}"
    );
}

#[gpui::test]
fn single_page_navigation_frames_each_page_like_the_last(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.action(SinglePageView);
    h.action(FitPage);
    for target in 1..MIXED_PAGES.len() as u32 {
        h.action(NavigateNextPage);
        let frame = h.frame();
        assert_eq!(frame.current_page, target);
        assert!(
            frame.whole_page_in_view(target),
            "Fit Page frames page {target} after Next Page: {frame:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

#[gpui::test]
fn each_tab_keeps_its_own_view_through_switches(cx: &mut TestAppContext) {
    let (mut h, documents) = setup_tabs(cx, &MIXED_PAGES, &["first.pdf", "second.pdf"]);
    let (first, second) = (documents[0], documents[1]);

    h.switch_to(first);
    h.action(SinglePageView);
    h.go_to_page(1);
    h.set_zoom(250.);
    h.pan(-120., 200.);
    let first_view = h.frame();

    h.switch_to(second);
    h.go_to_page(2);
    h.wheel(-60., -150., false);
    let second_view = h.frame();

    for round in 0..2 {
        h.switch_to(first);
        let frame = h.frame();
        assert_eq!(
            (frame.mode, frame.zoom, frame.current_page, frame.scroll),
            (
                first_view.mode,
                first_view.zoom,
                first_view.current_page,
                first_view.scroll
            ),
            "round {round}: the first tab keeps its view"
        );
        assert_eq!(
            frame.pages, first_view.pages,
            "round {round}: first tab paints the same"
        );

        h.switch_to(second);
        let frame = h.frame();
        assert_eq!(
            (frame.mode, frame.zoom, frame.current_page, frame.scroll),
            (
                second_view.mode,
                second_view.zoom,
                second_view.current_page,
                second_view.scroll
            ),
            "round {round}: the second tab keeps its view"
        );
        assert_eq!(
            frame.pages, second_view.pages,
            "round {round}: second tab paints the same"
        );
    }
}

#[gpui::test]
fn a_view_panned_into_blank_space_survives_a_tab_switch(cx: &mut TestAppContext) {
    let (mut h, documents) = setup_tabs(cx, &MIXED_PAGES, &["first.pdf", "second.pdf"]);
    h.switch_to(documents[0]);
    h.set_zoom(100.);
    h.pan(400., 300.);
    let panned = h.frame();
    assert!(panned.scroll.0 < 0. && panned.scroll.1 < 0., "{panned:?}");
    h.switch_to(documents[1]);
    h.switch_to(documents[0]);
    let back = h.frame();
    assert_eq!(back.scroll, panned.scroll);
    assert_eq!(back.pages, panned.pages);
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

fn assert_viewport_inside_window(h: &mut Harness<'_>, label: &str) {
    let frame = h.frame();
    assert!(
        frame.viewport.bottom() <= frame.window.height
            && frame.viewport.right() <= frame.window.width,
        "{label}: the viewport must fit in the {:?} window, not run past its edge: {frame:?}",
        frame.window
    );
}

#[gpui::test]
fn the_viewport_stays_inside_the_window_in_every_mode(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    assert_viewport_inside_window(&mut h, "continuous fit width");
    h.set_zoom(400.);
    assert_viewport_inside_window(&mut h, "continuous 400%");
    h.action(SinglePageView);
    h.action(FitPage);
    assert_viewport_inside_window(&mut h, "single page fit page");
    h.set_zoom(400.);
    assert_viewport_inside_window(&mut h, "single page 400%");
}

#[gpui::test]
fn the_viewer_settles_on_one_layout_in_every_mode(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    let mut unsettled = Vec::new();
    for mode in [PageViewMode::Continuous, PageViewMode::SinglePage] {
        match mode {
            PageViewMode::Continuous => h.action(ContinuousView),
            PageViewMode::SinglePage => h.action(SinglePageView),
        }
        for page in 0..MIXED_PAGES.len() as u32 {
            h.go_to_page(page);
            for fit in ["width", "page", "manual"] {
                match fit {
                    "width" => h.action(FitWidth),
                    "page" => h.action(FitPage),
                    _ => h.set_zoom(130.),
                }
                let frame = h.frame();
                if frame.unsettled {
                    unsettled.push(format!("{mode:?} page {page} fit {fit}: {frame:?}"));
                }
            }
        }
    }
    assert!(
        unsettled.is_empty(),
        "the viewer keeps re-planning instead of settling:\n{}",
        unsettled.join("\n")
    );
}

#[gpui::test]
fn panning_sideways_past_a_narrow_page_keeps_a_page_in_view(cx: &mut TestAppContext) {
    // The landscape page is wider than the portrait page at the top; the pan
    // limit must follow the pages in view, not the widest page in the file.
    let mut h = setup(cx, &MIXED_PAGES);
    h.set_zoom(300.);
    for dx in [20_000., -20_000.] {
        h.pan(dx, 0.);
        let frame = h.frame();
        assert!(
            frame.some_page_in_view(),
            "panning {} at the top of a mixed-size file keeps a page in view: {frame:?}",
            if dx > 0. { "right" } else { "left" }
        );
    }
}

#[gpui::test]
fn continuous_navigation_can_land_on_a_short_page(cx: &mut TestAppContext) {
    for fit in [false, true] {
        let mut h = setup(cx, &MIXED_PAGES);
        if fit {
            h.action(FitWidth);
        } else {
            h.set_zoom(100.);
        }
        for _ in 0..3 {
            h.action(NavigateNextPage);
        }
        let frame = h.frame();
        assert_eq!(
            frame.current_page, 3,
            "Next Page reaches the short page 4 and stays there (Fit Width: {fit}): {frame:?}"
        );
    }
}

#[gpui::test]
fn fit_width_in_continuous_mode_keeps_one_zoom_while_scrolling(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.action(FitWidth);
    let start = h.frame();
    let mut zooms = vec![start.zoom];
    for _ in 0..60 {
        h.wheel(0., -200., false);
        let zoom = h.frame().zoom;
        if zooms.last() != Some(&zoom) {
            zooms.push(zoom);
        }
    }
    assert_eq!(
        zooms.len(),
        1,
        "scrolling a mixed-size file in Fit Width changes the zoom under the reader: {zooms:?}"
    );
}

#[gpui::test]
fn opening_a_mixed_size_file_centres_the_first_page_in_fit_width(cx: &mut TestAppContext) {
    // The landscape page widens the layout; the first page still opens
    // filling the view's width, centred.
    let mut h = setup(cx, &MIXED_PAGES);
    let frame = h.frame();
    assert_eq!(frame.preset, ViewerZoomPreset::FitWidth);
    let (left, right) = frame.horizontal_gaps(0);
    assert!(
        left >= -TOLERANCE && right >= -TOLERANCE && (left - right).abs() <= TOLERANCE * 2.,
        "the first page opens centred: gaps {left} / {right}, {frame:?}"
    );
}

#[gpui::test]
fn page_raster_and_annotations_share_the_full_paper_bounds(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    for single in [true, false] {
        if single {
            h.action(SinglePageView);
        } else {
            h.action(ContinuousView);
        }
        for preset in 0..3 {
            match preset {
                0 => h.action(FitWidth),
                1 => h.action(FitPage),
                _ => h.action(ZoomIn),
            }
            let frame = h.frame();
            let index = frame.current_page as usize;
            let page_id = document_viewer_page_id(h.document, index);
            let page =
                h.cx.debug_bounds(Box::leak(page_id.clone().into_boxed_str()))
                    .unwrap();
            let image =
                h.cx.debug_bounds(Box::leak(format!("{page_id}-base-image").into_boxed_str()))
                    .unwrap();
            assert_eq!(image, page, "Base raster must reach every paper edge");
            let annotation_id = butter_paper::document_workspace::document_annotation_layer_id(
                h.document,
                index as u32,
            );
            let annotations =
                h.cx.debug_bounds(Box::leak(annotation_id.into_boxed_str()))
                    .unwrap();
            assert_eq!(
                annotations, page,
                "Annotation coordinates must use the same paper bounds"
            );
        }
    }
}

/// The page a page turn shows on its first frame: the height of the raster
/// under it (the test renders encode the page as `page + 2`), or `None`
/// while only its coarse thumbnail stands in.
fn first_frame_raster_height(h: &mut Harness<'_>, lines: f32) -> Option<u32> {
    // The page on screen sharpens (preview tiles promote on a timer), and
    // only then are the pages beside it drawn.
    h.cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    h.settle();
    let at = h.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap().center();
    h.cx.simulate_mouse_move(at, None, Modifiers::default());
    // Dispatched without running background work, so the page's own render
    // is still on the way, as on the frame a turn first draws.
    h.cx.update(|window, cx| {
        window.dispatch_event(
            gpui::PlatformInput::ScrollWheel(ScrollWheelEvent {
                position: at,
                delta: ScrollDelta::Lines(point(0., lines)),
                ..Default::default()
            }),
            cx,
        )
    });
    let document = h.document;
    let height = h.workspace.read_with(h.cx, |workspace, cx| {
        workspace
            .session(document, cx)
            .unwrap()
            .read(cx)
            .current_base_raster()
            .map(|raster| raster.height())
    });
    h.settle();
    height
}

#[gpui::test]
fn single_page_turns_show_the_next_page_sharp_on_their_first_frame(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(SinglePageView);
    h.action(FitPage);
    h.single_page_wheel_scrolls();
    assert_eq!(h.frame().current_page, 0);

    // The page beside the current one was drawn ahead: a turn shows it as
    // its own render at once, not its thumbnail scaled up.
    assert_eq!(first_frame_raster_height(&mut h, -3.), Some(1 + 2));
    assert_eq!(h.frame().current_page, 1);
    assert_eq!(first_frame_raster_height(&mut h, -3.), Some(2 + 2));
    // The page just left is kept: turning back is as sharp.
    assert_eq!(first_frame_raster_height(&mut h, 3.), Some(1 + 2));
    assert_eq!(h.frame().current_page, 1);
}

fn navigator_label(h: &mut Harness<'_>) -> String {
    h.workspace.read_with(h.cx, |workspace, cx| {
        workspace.page_navigator().read(cx).indicator_label()
    })
}

fn navigator_can_go(h: &mut Harness<'_>) -> (bool, bool) {
    h.workspace.read_with(h.cx, |workspace, cx| {
        let navigator = workspace.page_navigator().read(cx);
        (navigator.can_go_back(), navigator.can_go_forward())
    })
}

#[gpui::test]
fn the_page_navigator_turns_jumps_and_takes_a_typed_page(cx: &mut TestAppContext) {
    use butter_paper::page_navigator::{
        PAGE_FIRST_ID, PAGE_INDICATOR_ID, PAGE_INPUT_ID, PAGE_LAST_ID, PAGE_NEXT_ID,
        PAGE_PREVIOUS_ID,
    };
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(SinglePageView);
    assert_eq!(navigator_label(&mut h), "1 / 5");
    assert_eq!(navigator_can_go(&mut h), (false, true));
    // A disabled button takes no press.
    h.click(PAGE_PREVIOUS_ID);
    assert_eq!(h.frame().current_page, 0);

    h.click(PAGE_NEXT_ID);
    assert_eq!(h.frame().current_page, 1);
    assert_eq!(navigator_label(&mut h), "2 / 5");
    assert_eq!(navigator_can_go(&mut h), (true, true));
    h.click(PAGE_LAST_ID);
    assert_eq!(h.frame().current_page, 4);
    assert_eq!(navigator_label(&mut h), "5 / 5");
    assert_eq!(navigator_can_go(&mut h), (true, false));
    h.click(PAGE_PREVIOUS_ID);
    assert_eq!(h.frame().current_page, 3);
    h.click(PAGE_FIRST_ID);
    assert_eq!(h.frame().current_page, 0);

    // Clicking the page number opens it for typing, selected, so a page
    // typed replaces it; Enter goes there.
    h.click(PAGE_INDICATOR_ID);
    assert!(h.cx.debug_bounds(PAGE_INPUT_ID).is_some());
    h.cx.simulate_input("3");
    h.cx.simulate_keystrokes("enter");
    h.settle();
    assert_eq!(h.frame().current_page, 2);
    assert!(h.cx.debug_bounds(PAGE_INPUT_ID).is_none());
    assert_eq!(navigator_label(&mut h), "3 / 5");

    // A page past the end goes to the last.
    h.click(PAGE_INDICATOR_ID);
    h.cx.simulate_input("40");
    h.cx.simulate_keystrokes("enter");
    h.settle();
    assert_eq!(h.frame().current_page, 4);

    // Letters that are tool shortcuts type into the field instead, and
    // Escape keeps the page shown.
    let tool = h.workspace.read_with(h.cx, |workspace, cx| {
        workspace.annotation_tool(h.document, cx)
    });
    h.click(PAGE_INDICATOR_ID);
    h.cx.simulate_input("ve");
    h.cx.simulate_keystrokes("escape");
    h.settle();
    assert!(h.cx.debug_bounds(PAGE_INPUT_ID).is_none());
    assert_eq!(h.frame().current_page, 4);
    assert_eq!(
        h.workspace.read_with(h.cx, |workspace, cx| workspace
            .annotation_tool(h.document, cx)),
        tool
    );

    // Continuous view goes to the page too.
    h.action(ContinuousView);
    h.click(PAGE_FIRST_ID);
    let frame = h.frame();
    assert_eq!(frame.current_page, 0, "{frame:?}");
    h.click(PAGE_INDICATOR_ID);
    h.cx.simulate_input("4");
    h.cx.simulate_keystrokes("enter");
    let frame = h.frame();
    assert_eq!(frame.current_page, 3, "{frame:?}");
    assert!(
        frame
            .page(3)
            .is_some_and(|page| page.top() <= frame.viewport.top() + px(TOLERANCE)
                && page.bottom() > frame.viewport.top()),
        "the page typed is in view: {frame:?}"
    );
}

#[gpui::test]
fn a_narrow_toolbar_keeps_the_page_field_and_drops_the_page_buttons(cx: &mut TestAppContext) {
    use butter_paper::page_navigator::{
        PAGE_FIRST_ID, PAGE_INDICATOR_ID, PAGE_LAST_ID, PAGE_NEXT_ID, PAGE_PREVIOUS_ID,
    };
    // A four-digit page count: the widest page field the least window holds.
    let mut h = setup(cx, &PAGES_1000);
    let shown = |h: &mut Harness<'_>| {
        [
            PAGE_FIRST_ID,
            PAGE_PREVIOUS_ID,
            PAGE_INDICATOR_ID,
            PAGE_NEXT_ID,
            PAGE_LAST_ID,
        ]
        .map(|id| h.cx.debug_bounds(id).is_some())
    };
    assert_eq!(shown(&mut h), [true; 5]);
    // The least window: the page field alone, and the toolbar still fits.
    h.cx.simulate_resize(butter_paper::window_title_bar::DOCUMENT_WINDOW_MIN_SIZE);
    h.settle();
    h.settle();
    assert_eq!(shown(&mut h), [false, false, true, false, false]);
    let toolbar = h.cx.debug_bounds("viewer-toolbar").unwrap();
    let content = h.cx.debug_bounds("viewer-toolbar-content").unwrap();
    assert_eq!(
        content.size.width, toolbar.size.width,
        "the toolbar scrolls"
    );
    // The whole toolbar changes form at once. Just narrower than the
    // compact form needs, the narrowest; at the middle form's width, the
    // zoom steps with the page field alone; at the full form's width, every
    // page button too.
    let window_at = |fit| {
        16. * butter_paper::viewer_toolbar_strip::viewer_toolbar_rems(fit, false, 4)
            + f32::from(butter_paper::window_title_bar::DOCUMENT_WINDOW_MIN_SIZE.width)
            - 16. * butter_paper::viewer_toolbar_strip::VIEWER_TOOLBAR_MIN_REMS
    };
    use butter_paper::viewer_toolbar_strip::ViewerToolbarFit;
    let full = window_at(ViewerToolbarFit::Full);
    let middle = window_at(ViewerToolbarFit::Middle);
    let compact = window_at(ViewerToolbarFit::Compact);
    let fit_buttons = |h: &mut Harness<'_>| {
        h.cx.debug_bounds(butter_paper::viewer_toolbar_strip::FIT_WIDTH_ID)
            .is_some()
    };
    let zoom_steps = |h: &mut Harness<'_>| {
        h.cx.debug_bounds(butter_paper::zoom_control::ZOOM_OUT_ID)
            .is_some()
    };
    let page_field = [false, false, true, false, false];
    for (width, pages, fits, steps) in [
        (compact - 1., page_field, false, false),
        (compact, page_field, false, false),
        (middle - 1., page_field, true, false),
        (middle, page_field, true, true),
        (full - 1., page_field, true, true),
        (full, [true; 5], true, true),
    ] {
        h.cx.simulate_resize(size(px(width), px(800.)));
        h.settle();
        h.settle();
        assert_eq!(shown(&mut h), pages, "{width}");
        assert_eq!(fit_buttons(&mut h), fits, "{width}");
        assert_eq!(zoom_steps(&mut h), steps, "{width}");
        let toolbar = h.cx.debug_bounds("viewer-toolbar").unwrap();
        let content = h.cx.debug_bounds("viewer-toolbar-content").unwrap();
        assert_eq!(
            content.size.width, toolbar.size.width,
            "the toolbar scrolls at {width}"
        );
    }
}

static PAGES_12: [(f32, f32); 12] = [(612., 792.); 12];

/// Half a 1920-pixel screen, as macOS tiles a window.
const HALF_1080P_MAC_WIDTH: f32 = 960.;

#[gpui::test]
fn a_960_px_window_shows_the_middle_toolbar(cx: &mut TestAppContext) {
    use butter_paper::page_navigator::{
        PAGE_FIRST_ID, PAGE_INDICATOR_ID, PAGE_LAST_ID, PAGE_NEXT_ID, PAGE_PREVIOUS_ID,
    };
    use butter_paper::viewer_toolbar_strip::{
        EDIT_PDF_ID, FIT_WIDTH_ID, VIEWER_HISTORY_GROUP_ID, VIEWER_PAGE_LAYOUT_ID,
        VIEWER_PAGES_GROUP_ID, VIEWER_SCALE_ID, VIEWER_SCALE_MENU_ID, VIEWER_TOOLBAR_CONTENT_ID,
        VIEWER_TOOLBAR_ID, VIEWER_ZOOM_GROUP_ID,
    };
    // Page Thumbnails start collapsed.
    let mut h = setup(cx, &PAGES_12);
    h.cx.simulate_resize(size(px(HALF_1080P_MAC_WIDTH), px(900.)));
    h.settle();
    h.settle();
    let toolbar = h.cx.debug_bounds(VIEWER_TOOLBAR_ID).unwrap();
    let content = h.cx.debug_bounds(VIEWER_TOOLBAR_CONTENT_ID).unwrap();
    assert_eq!(
        content.size.width, toolbar.size.width,
        "the toolbar scrolls at 960 px"
    );
    // The middle form: the page field alone, with the zoom steps and the
    // fit and page-view buttons.
    for id in [PAGE_FIRST_ID, PAGE_PREVIOUS_ID, PAGE_NEXT_ID, PAGE_LAST_ID] {
        assert!(h.cx.debug_bounds(id).is_none(), "{id} shows at 960 px");
    }
    for id in [butter_paper::zoom_control::ZOOM_OUT_ID, FIT_WIDTH_ID] {
        assert!(h.cx.debug_bounds(id).is_some(), "{id} hidden at 960 px");
    }
    // Every group shows whole, in order, inside the toolbar.
    let order = [
        VIEWER_HISTORY_GROUP_ID,
        VIEWER_PAGES_GROUP_ID,
        VIEWER_ZOOM_GROUP_ID,
        VIEWER_PAGE_LAYOUT_ID,
        EDIT_PDF_ID,
        VIEWER_SCALE_ID,
    ]
    .map(|id| {
        h.cx.debug_bounds(id)
            .unwrap_or_else(|| panic!("{id} hidden at 960 px"))
    });
    for pair in order.windows(2) {
        assert!(pair[0].right() <= pair[1].left(), "{order:?}");
    }
    assert!(order[0].left() >= toolbar.left() + px(8.));
    assert!(order[5].right() <= toolbar.right() - px(8.));
    for id in [PAGE_INDICATOR_ID, VIEWER_SCALE_MENU_ID] {
        let bounds =
            h.cx.debug_bounds(id)
                .unwrap_or_else(|| panic!("{id} hidden at 960 px"));
        assert!(
            bounds.left() >= toolbar.left() && bounds.right() <= toolbar.right(),
            "{id}"
        );
    }
    // The page field still goes to a page, as the keys still turn them.
    h.click(PAGE_INDICATOR_ID);
    h.cx.simulate_input("4");
    h.cx.simulate_keystrokes("enter");
    h.settle();
    assert_eq!(h.frame().current_page, 3);
    h.action(NavigateNextPage);
    h.settle();
    assert_eq!(h.frame().current_page, 4);
}

static PAGES_999: [(f32, f32); 999] = [(612., 792.); 999];
static PAGES_1000: [(f32, f32); 1000] = [(612., 792.); 1000];

/// Half a 1920-pixel screen, less Windows' invisible resize borders.
const HALF_1080P_WIDTH: f32 = 944.;

#[gpui::test]
fn the_whole_toolbar_fits_half_a_1080p_screen_with_sidebars_collapsed_999(cx: &mut TestAppContext) {
    whole_toolbar_fits_half_1080p(cx, &PAGES_999);
}

#[gpui::test]
fn the_whole_toolbar_fits_half_a_1080p_screen_with_sidebars_collapsed_1000(
    cx: &mut TestAppContext,
) {
    whole_toolbar_fits_half_1080p(cx, &PAGES_1000);
}

#[gpui::test]
fn a_signed_document_s_whole_toolbar_fits_half_a_1080p_screen(cx: &mut TestAppContext) {
    use butter_paper::viewer_toolbar_strip::{EDIT_PDF_ID, VIEWER_SIGNATURES_ID};
    // A real signed file, so its signatures check, shown as a document of
    // a thousand pages.
    let signed = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/signatures/signed-rsa.pdf"
    );
    let (mut h, _) = setup_tabs(cx, &PAGES_1000, &[signed]);
    let wide_edit_pdf = h.cx.debug_bounds(EDIT_PDF_ID).unwrap();
    whole_toolbar_fits(&mut h, "signed");
    // Edit PDF is the same icon toggle in every form.
    let edit_pdf = h.cx.debug_bounds(EDIT_PDF_ID).unwrap();
    assert_eq!(edit_pdf.size, wide_edit_pdf.size);
    let toolbar = h.cx.debug_bounds("viewer-toolbar").unwrap();
    let controls =
        h.cx.debug_bounds(butter_paper::viewer_toolbar_strip::VIEWER_TOOLBAR_CONTROLS_ID)
            .unwrap();
    let signatures =
        h.cx.debug_bounds(VIEWER_SIGNATURES_ID)
            .expect("the signatures button shows");
    assert!(
        signatures.left() >= controls.right() && signatures.right() <= toolbar.right(),
        "{signatures:?} beside {controls:?} in {toolbar:?}"
    );
}

fn whole_toolbar_fits_half_1080p(cx: &mut TestAppContext, pages: &'static [(f32, f32)]) {
    let mut h = setup(cx, pages);
    whole_toolbar_fits(&mut h, &pages.len().to_string());
}

fn whole_toolbar_fits(h: &mut Harness<'_>, document: &str) {
    use butter_paper::page_navigator::{
        PAGE_FIRST_ID, PAGE_INDICATOR_ID, PAGE_LAST_ID, PAGE_NEXT_ID, PAGE_PREVIOUS_ID,
    };
    use butter_paper::viewer_toolbar_strip::{
        EDIT_PDF_ID, VIEWER_SCALE_ID, VIEWER_TOOLBAR_CONTENT_ID, VIEWER_TOOLBAR_CONTROLS_ID,
        VIEWER_TOOLBAR_ID,
    };
    h.cx.simulate_resize(size(px(HALF_1080P_WIDTH), px(1000.)));
    h.settle();
    h.settle();
    let toolbar = h.cx.debug_bounds(VIEWER_TOOLBAR_ID).unwrap();
    let content = h.cx.debug_bounds(VIEWER_TOOLBAR_CONTENT_ID).unwrap();
    assert_eq!(
        content.size.width, toolbar.size.width,
        "the toolbar scrolls"
    );
    let controls = h.cx.debug_bounds(VIEWER_TOOLBAR_CONTROLS_ID).unwrap();
    let edit_pdf = h.cx.debug_bounds(EDIT_PDF_ID).unwrap();
    assert!(
        controls.right() <= edit_pdf.left(),
        "Edit PDF overlaps the controls"
    );
    assert!(edit_pdf.right() <= toolbar.right());
    // Narrow, the page navigator shows its page field alone (decided
    // 2026-11-11, to make room for the page's Scale beside Edit PDF).
    for id in [PAGE_FIRST_ID, PAGE_PREVIOUS_ID, PAGE_NEXT_ID, PAGE_LAST_ID] {
        assert!(
            h.cx.debug_bounds(id).is_none(),
            "{id} shows at half 1080p for {document}: the toolbar is not compact"
        );
    }
    let scale = h.cx.debug_bounds(VIEWER_SCALE_ID).unwrap();
    assert!(
        edit_pdf.right() <= scale.left() && scale.right() <= toolbar.right(),
        "the Scale overlaps at half 1080p for {document}"
    );
    // Every group shows whole; narrow, the fit and page-view buttons are in
    // menus.
    for id in [
        PAGE_INDICATOR_ID,
        "viewer-zoom-menu",
        butter_paper::viewer_toolbar_strip::VIEWER_PAGE_LAYOUT_ID,
    ] {
        let bounds =
            h.cx.debug_bounds(id)
                .unwrap_or_else(|| panic!("{id} hidden at half 1080p for {document}"));
        assert!(
            bounds.left() >= toolbar.left() && bounds.right() <= toolbar.right(),
            "{id}"
        );
    }
}

/// Sweeps the window from its least width to wide in 8px steps, with each
/// combination of Page Thumbnails and the properties panel, and checks that
/// every viewer toolbar group shows whole in one row without scrolling.
fn every_toolbar_group_shows_at_every_window_width(h: &mut Harness<'_>, signed: bool) {
    use butter_paper::document_workspace::DOCUMENT_THUMBNAIL_STRIP_ID;
    use butter_paper::page_navigator::PAGE_INDICATOR_ID;
    use butter_paper::viewer_toolbar_strip::{
        EDIT_PDF_ID, VIEWER_PAGE_LAYOUT_ID, VIEWER_SCALE_ID, VIEWER_SCALE_MENU_ID,
        VIEWER_SIGNATURES_ID, VIEWER_TOOLBAR_CONTENT_ID, VIEWER_TOOLBAR_ID,
        VIEWER_TOOLBAR_NARROWEST_REMS,
    };
    use butter_paper::window_title_bar::DOCUMENT_WINDOW_MIN_SIZE;
    let least = f32::from(DOCUMENT_WINDOW_MIN_SIZE.width);
    let toggle = |h: &mut Harness<'_>, id: &'static str| {
        h.cx.simulate_resize(size(px(1600.), px(800.)));
        h.cx.update(|window, cx| window.draw(cx).clear(cx));
        let target = h.cx.debug_bounds(id).unwrap().center();
        h.cx.simulate_click(target, gpui::Modifiers::default());
        h.cx.run_until_parked();
    };
    // Closed; thumbnails; thumbnails and properties; properties.
    for (step, (thumbnails, properties)) in [
        (None, (false, false)),
        (Some("document-left-rail-pages"), (true, false)),
        (Some("document-workspace-rail-actions"), (true, true)),
        (Some("document-left-rail-pages"), (false, true)),
    ] {
        if let Some(id) = step {
            toggle(h, id);
        }
        let mut width = least;
        while width <= 1600. {
            h.cx.simulate_resize(size(px(width), px(800.)));
            // The toolbar's form follows the width laid out last frame.
            h.cx.update(|window, cx| window.draw(cx).clear(cx));
            h.cx.update(|window, cx| window.draw(cx).clear(cx));
            let case = format!(
                "at {width} with thumbnails {thumbnails}, properties {properties}, signed \
                 {signed}"
            );
            let toolbar = h.cx.debug_bounds(VIEWER_TOOLBAR_ID).unwrap();
            let content = h.cx.debug_bounds(VIEWER_TOOLBAR_CONTENT_ID).unwrap();
            assert!(
                toolbar.size.width >= px(16. * VIEWER_TOOLBAR_NARROWEST_REMS) - px(1.),
                "the panels squeeze the toolbar {case}: {toolbar:?}"
            );
            assert_eq!(content.size.width, toolbar.size.width, "overflow {case}");
            let mut groups = vec![
                PAGE_INDICATOR_ID,
                "viewer-zoom-menu",
                VIEWER_PAGE_LAYOUT_ID,
                EDIT_PDF_ID,
                VIEWER_SCALE_ID,
            ];
            if signed {
                groups.push(VIEWER_SIGNATURES_ID);
            }
            let bounds = groups
                .iter()
                .map(|id| {
                    h.cx.debug_bounds(id)
                        .unwrap_or_else(|| panic!("{id} hidden {case}"))
                })
                .collect::<Vec<_>>();
            for (id, group) in groups.iter().zip(&bounds) {
                assert!(
                    group.left() >= toolbar.left() && group.right() <= toolbar.right(),
                    "{id} cut {case}: {group:?} in {toolbar:?}"
                );
            }
            for pair in bounds.windows(2) {
                assert!(
                    pair[0].right() <= pair[1].left(),
                    "overlap {case}: {bounds:?}"
                );
            }
            assert!(
                h.cx.debug_bounds(VIEWER_SCALE_MENU_ID).is_some(),
                "the Scale's menu hidden {case}"
            );
            // Page Thumbnails alone open at every window width.
            if thumbnails && !properties {
                assert!(
                    h.cx.debug_bounds(DOCUMENT_THUMBNAIL_STRIP_ID).is_some(),
                    "thumbnails hidden {case}"
                );
            }
            width += 8.;
        }
    }
}

#[gpui::test]
fn every_toolbar_group_shows_at_every_window_width_and_panel_combination(cx: &mut TestAppContext) {
    let mut h = setup(cx, &PAGES_1000);
    every_toolbar_group_shows_at_every_window_width(&mut h, false);
}

#[gpui::test]
fn every_toolbar_group_shows_at_every_window_width_for_a_signed_document(cx: &mut TestAppContext) {
    let signed = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/signatures/signed-rsa.pdf"
    );
    let (mut h, _) = setup_tabs(cx, &PAGES_1000, &[signed]);
    every_toolbar_group_shows_at_every_window_width(&mut h, true);
}

/// The point at a fraction across a laid-out page, in window pixels.
fn page_point(h: &mut Harness<'_>, page: u32, fx: f32, fy: f32) -> gpui::Point<Pixels> {
    let frame = h.frame();
    let at = frame
        .screen_point((page, fx, fy))
        .unwrap_or_else(|| panic!("page {page} is laid out: {frame:?}"));
    assert!(frame.viewport.contains(&at), "{at:?} is in view: {frame:?}");
    at
}

fn box_armed(h: &mut Harness<'_>) -> bool {
    let document = h.document;
    h.workspace.read_with(h.cx, |workspace, cx| {
        workspace.active_selection_marquee(document, cx).is_some()
    })
}

fn rectangle_count_in(h: &mut Harness<'_>) -> usize {
    let document = h.document;
    h.workspace
        .read_with(h.cx, |workspace, cx| {
            workspace.annotation_snapshot(document, cx)
        })
        .unwrap()
        .rectangles
        .len()
}

fn press_at(h: &mut Harness<'_>, at: gpui::Point<Pixels>) {
    h.cx.simulate_click(at, Modifiers::default());
    h.settle();
}

fn drag_on_page(h: &mut Harness<'_>, from: gpui::Point<Pixels>) {
    let to = point(from.x + px(80.), from.y + px(60.));
    h.cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    h.cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    h.cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    h.settle();
}

fn open_page_field(h: &mut Harness<'_>, typed: &str) {
    use butter_paper::page_navigator::{PAGE_INDICATOR_ID, PAGE_INPUT_ID};
    h.click(PAGE_INDICATOR_ID);
    assert!(h.cx.debug_bounds(PAGE_INPUT_ID).is_some());
    h.cx.simulate_input(typed);
    h.settle();
}

fn page_field_open(h: &mut Harness<'_>) -> bool {
    h.cx.debug_bounds(butter_paper::page_navigator::PAGE_INPUT_ID)
        .is_some()
}

/// T1 and T8: with the page field open, a press on the page only closes the
/// field. The pointer shows the arrow and the status bar says so first.
#[gpui::test]
fn a_page_press_only_closes_the_page_field(cx: &mut TestAppContext) {
    use butter_paper::document_workspace::CLICK_OUT_TYPING_STATUS;
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(SinglePageView);
    let empty = page_point(&mut h, 0, 0.5, 0.3);
    let document = h.document;
    let status = |h: &mut Harness<'_>| {
        h.workspace.read_with(h.cx, |workspace, cx| {
            workspace.click_out_status(document, cx)
        })
    };
    assert_eq!(status(&mut h), None);

    open_page_field(&mut h, "3");
    h.cx.simulate_mouse_move(empty, None, Modifiers::default());
    h.cx.simulate_keystrokes("r");
    h.settle();
    assert!(page_field_open(&mut h), "r types into the field");
    assert_eq!(status(&mut h), Some(CLICK_OUT_TYPING_STATUS));
    assert_eq!(
        h.workspace
            .read_with(h.cx, |workspace, _| workspace.viewport_cursor()),
        gpui::CursorStyle::Arrow,
        "the pointer promises no gesture"
    );

    press_at(&mut h, empty);
    assert!(!page_field_open(&mut h), "the press closed the field");
    assert_eq!(h.frame().current_page, 0, "the page stays");
    assert!(!box_armed(&mut h), "the press armed no selection box");
    assert_eq!(status(&mut h), None);

    press_at(&mut h, empty);
    assert!(box_armed(&mut h), "the next press arms a box");
    h.cx.simulate_keystrokes("escape");
    h.settle();
    assert!(!box_armed(&mut h));
}

/// T2 (Q1): a focused text field stops drawing tools too.
#[gpui::test]
fn a_page_drag_with_a_drawing_tool_only_closes_the_page_field(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(SinglePageView);
    h.cx.simulate_keystrokes("r");
    h.settle();
    let from = page_point(&mut h, 0, 0.3, 0.3);
    let before = rectangle_count_in(&mut h);

    open_page_field(&mut h, "2");
    drag_on_page(&mut h, from);
    assert!(!page_field_open(&mut h));
    assert_eq!(
        rectangle_count_in(&mut h),
        before,
        "the click-out drew nothing"
    );
    assert_eq!(h.frame().current_page, 0, "the typed page was not taken");

    drag_on_page(&mut h, from);
    assert_eq!(
        rectangle_count_in(&mut h),
        before + 1,
        "the next drag draws"
    );
}

/// T10: a path draft survives a page-field click-out, and the next press
/// places its point.
#[gpui::test]
fn a_path_draft_survives_a_page_field_click_out(cx: &mut TestAppContext) {
    use butter_paper::annotation_adapter::AnnotationTool;
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(SinglePageView);
    let document = h.document;
    h.workspace.update(h.cx, |workspace, cx| {
        workspace
            .set_annotation_tool(document, AnnotationTool::Polyline, cx)
            .unwrap()
    });
    h.settle();
    let draft_points = |h: &mut Harness<'_>| {
        h.workspace.read_with(h.cx, |workspace, cx| {
            workspace
                .annotation_scene(document, 0, cx)
                .vertex_paths
                .iter()
                .find(|path| path.draft)
                .map(|path| path.points.len())
        })
    };
    for (fx, fy) in [(0.2, 0.1), (0.4, 0.2)] {
        let at = page_point(&mut h, 0, fx, fy);
        press_at(&mut h, at);
    }
    let placed = draft_points(&mut h).expect("a draft is open");

    open_page_field(&mut h, "4");
    let next = page_point(&mut h, 0, 0.6, 0.3);
    press_at(&mut h, next);
    assert!(!page_field_open(&mut h));
    assert_eq!(draft_points(&mut h), Some(placed), "no point was placed");

    press_at(&mut h, next);
    assert_eq!(
        draft_points(&mut h).map(|points| points > placed),
        Some(true),
        "the next press places a point"
    );
}

/// Item 14: in Continuous view a press on a page makes it the current page,
/// with no scroll, even when the press only dismisses. A scroll makes the
/// page in view current again.
#[gpui::test]
fn a_press_picks_the_current_page_in_continuous_view(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(ContinuousView);
    h.set_zoom(30.);
    let start = h.frame();
    let in_view = start.current_page;
    let other = (0..5)
        .find(|page| {
            *page != in_view && {
                let (width, height) = start.visible(*page);
                width > 40. && height > 120.
            }
        })
        .unwrap_or_else(|| panic!("a second page is in view: {start:?}"));
    let label = |page: u32| format!("{} / 5", page + 1);
    assert_eq!(navigator_label(&mut h), label(in_view));

    let at = page_point(&mut h, other, 0.5, 0.2);
    press_at(&mut h, at);
    let picked = h.frame();
    assert_eq!(picked.current_page, other, "{picked:?}");
    assert_eq!(picked.scroll, start.scroll, "picking moves nothing");
    assert_eq!(navigator_label(&mut h), label(other));
    h.cx.simulate_keystrokes("escape");
    h.settle();

    // A press that only closes the page field picks the page too.
    let at = page_point(&mut h, in_view, 0.5, 0.2);
    open_page_field(&mut h, "5");
    press_at(&mut h, at);
    assert!(!page_field_open(&mut h));
    assert!(!box_armed(&mut h), "the click-out armed no box");
    let frame = h.frame();
    assert_eq!(frame.current_page, in_view, "{frame:?}");
    assert_eq!(frame.scroll, start.scroll);

    // Picked again, then scrolled away and back: the page level with the
    // view centre is current again.
    let at = page_point(&mut h, other, 0.5, 0.2);
    press_at(&mut h, at);
    h.cx.simulate_keystrokes("escape");
    assert_eq!(h.frame().current_page, other);
    h.wheel(0., -30., false);
    let scrolled = h.frame();
    assert_ne!(scrolled.scroll, start.scroll, "the wheel scrolled");
    let centre = scrolled
        .document_point(scrolled.viewport.center())
        .map(|(page, ..)| page)
        .unwrap_or_else(|| panic!("a page is at the view centre: {scrolled:?}"));
    assert_ne!(centre, other, "{scrolled:?}");
    assert_eq!(scrolled.current_page, centre, "{scrolled:?}");
    assert_eq!(navigator_label(&mut h), label(centre));
}

/// Item 14: Single Page view shows one page, which stays current.
#[gpui::test]
fn a_press_in_single_page_view_keeps_the_current_page(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(SinglePageView);
    h.go_to_page(2);
    let before = h.frame();
    let at = page_point(&mut h, 2, 0.5, 0.3);
    press_at(&mut h, at);
    let after = h.frame();
    assert_eq!(after.current_page, 2);
    assert_eq!(after.scroll, before.scroll);
}

const MANY_PAGES: [(f32, f32); 12] = [(612., 792.); 12];

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= TOLERANCE
}

#[gpui::test]
fn single_page_view_remembers_each_pages_zoom_and_position(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(SinglePageView);
    h.go_to_page(1);
    h.set_zoom(300.);
    h.pan(-300., -400.);
    let first = h.frame();

    // A page not seen before keeps the zoom and shows from its top edge.
    h.go_to_page(2);
    let fresh = h.frame();
    assert_eq!(fresh.zoom, first.zoom, "{fresh:?}");
    assert!(
        near(
            f32::from(fresh.page(2).unwrap().top() - fresh.viewport.top()),
            0.
        ),
        "{fresh:?}"
    );
    h.set_zoom(150.);
    h.pan(0., -100.);
    let second = h.frame();

    // Back to each page: the zoom and position it was left at.
    h.go_to_page(1);
    let back = h.frame();
    assert_eq!(back.zoom, first.zoom, "{back:?}");
    assert!(
        near(back.scroll.0, first.scroll.0) && near(back.scroll.1, first.scroll.1),
        "{first:?} → {back:?}"
    );
    h.go_to_page(2);
    let again = h.frame();
    assert_eq!(again.zoom, second.zoom, "{again:?}");
    assert!(
        near(again.scroll.0, second.scroll.0) && near(again.scroll.1, second.scroll.1),
        "{second:?} → {again:?}"
    );

    // Continuous view has one view; switching back keeps the memory.
    h.action(ContinuousView);
    h.action(SinglePageView);
    h.go_to_page(1);
    assert_eq!(h.frame().zoom, first.zoom);

    // Fit Page replaces the view page 1 was left at.
    h.action(FitPage);
    let fitted = h.frame();
    assert_eq!(h.remembered(1), None);
    h.go_to_page(2);
    assert_eq!(h.frame().zoom, second.zoom);
    h.go_to_page(1);
    let refitted = h.frame();
    assert_eq!(refitted.preset, ViewerZoomPreset::FitPage, "{refitted:?}");
    assert_eq!(refitted.zoom, fitted.zoom);
}

#[gpui::test]
fn a_file_gets_its_remembered_pages_back_when_it_opens_again(cx: &mut TestAppContext) {
    let (mut h, documents) = setup_tabs(
        cx,
        &UNIFORM_PAGES,
        &["/documents/memory-a.pdf", "/documents/memory-b.pdf"],
    );
    let (first, second) = (documents[0], documents[1]);
    h.switch_to(first);
    h.action(SinglePageView);
    h.go_to_page(1);
    h.set_zoom(300.);
    h.pan(-300., -400.);
    let left = h.frame();
    h.go_to_page(2);
    let remembered = h.remembered(1).expect("page 1 is remembered while open");

    // The other file keeps nothing of it.
    h.switch_to(second);
    h.action(SinglePageView);
    h.go_to_page(1);
    h.go_to_page(2);
    assert!(h.remembered(1).is_some_and(|view| view != remembered));

    // Closing the file's tab and opening it again brings the pages back.
    h.switch_to(first);
    let workspace = h.workspace.clone();
    h.cx.update(|_, cx| {
        workspace.update(cx, |workspace, cx| workspace.close_document(first, cx));
    });
    h.cx.run_until_parked();
    h.workspace.update(h.cx, |workspace, cx| {
        workspace.open_documents(
            DocumentOpenBatchRequest::new(
                DocumentOpenOrigin::Menu,
                [PathBuf::from("/documents/memory-a.pdf")],
            ),
            cx,
        )
    });
    h.cx.run_until_parked();
    let reopened = h.workspace.read_with(h.cx, |workspace, cx| {
        *workspace.session_order(cx).last().unwrap()
    });
    h.switch_to(reopened);
    // It opens on the page left, in Single Page view, with that page's view.
    let opened = h.frame();
    assert_eq!(opened.mode, PageViewMode::SinglePage, "{opened:?}");
    assert_eq!(opened.current_page, 2, "{opened:?}");
    assert_eq!(opened.zoom, left.zoom, "{opened:?}");
    assert_eq!(h.remembered(1), Some(remembered));
    h.go_to_page(1);
    let back = h.frame();
    assert_eq!(back.zoom, left.zoom, "{back:?}");
    assert!(
        near(back.scroll.0, left.scroll.0) && near(back.scroll.1, left.scroll.1),
        "{left:?} → {back:?}"
    );

    // A file never seen opens with no memory.
    let fresh = h.workspace.update(h.cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("/documents/memory-c.pdf"), cx)
    });
    h.cx.run_until_parked();
    h.document = fresh;
    assert_eq!(h.remembered(1), None);
}

#[gpui::test]
fn quitting_keeps_the_pages_of_every_open_tab(cx: &mut TestAppContext) {
    let (mut h, documents) = setup_tabs(
        cx,
        &UNIFORM_PAGES,
        &["/documents/quit-a.pdf", "/documents/quit-b.pdf"],
    );
    let memory =
        h.cx.update(|_, cx| cx.new(|_| butter_paper::page_memory::PageMemory::default()));
    h.workspace.update(h.cx, |workspace, cx| {
        workspace.bind_page_memory(memory.clone(), cx)
    });
    for document in documents {
        h.switch_to(document);
        h.action(SinglePageView);
        h.go_to_page(1);
        h.set_zoom(250.);
        h.go_to_page(2);
    }
    // Nothing is kept while the tabs stay open.
    assert!(memory.read_with(h.cx, |memory, _| memory.paths().count()) == 0);
    h.workspace
        .update(h.cx, |workspace, cx| workspace.keep_all_page_memory(cx));
    for name in ["/documents/quit-a.pdf", "/documents/quit-b.pdf"] {
        let view = memory
            .read_with(h.cx, |memory, _| {
                memory.restart_view(std::path::Path::new(name))
            })
            .unwrap_or_else(|| panic!("{name} keeps its pages"));
        assert_eq!(view.current_page(), 2, "{name}");
        assert_eq!(view.page_views().len(), 2, "{name}");
    }
}

#[gpui::test]
fn rotating_pages_forgets_their_remembered_views(cx: &mut TestAppContext) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(SinglePageView);
    for page in [1, 2, 3] {
        h.go_to_page(page);
        h.set_zoom(200. + page as f32 * 10.);
    }
    h.go_to_page(0);
    assert!(h.remembered(1).is_some() && h.remembered(2).is_some());
    let document = h.document;
    // One page turned: only its view is forgotten.
    h.workspace.update(h.cx, |workspace, cx| {
        workspace.rotate_pages(
            document,
            vec![2],
            butter_paper::annotation_model::PageRotationDirection::Right,
            cx,
        )
    });
    h.settle();
    assert_eq!(h.remembered(2), None);
    assert_eq!(
        h.remembered(1).map(|view| view.zoom()),
        Some(RestartZoom::Manual(210.))
    );
    // Undo turns it back: the view seen since is forgotten too.
    h.go_to_page(3);
    h.go_to_page(2);
    h.go_to_page(0);
    assert!(h.remembered(2).is_some());
    h.workspace.update(h.cx, |workspace, cx| {
        workspace.undo_annotations(document, cx).unwrap()
    });
    h.settle();
    assert_eq!(h.remembered(2), None);
    assert!(h.remembered(3).is_some());
    // Every page turned: nothing is remembered.
    h.workspace.update(h.cx, |workspace, cx| {
        workspace
            .rotate_all_pages(
                document,
                butter_paper::annotation_model::PageRotationDirection::Left,
                cx,
            )
            .unwrap()
    });
    h.settle();
    for page in 0..5 {
        assert_eq!(h.remembered(page), None, "page {page}");
    }
}

/// Single Page view at Fit Page with the wheel set to Scroll.
fn wheel_turning_setup(cx: &mut TestAppContext, turning: PageTurning) -> Harness<'_> {
    let mut h = setup(cx, &MANY_PAGES);
    h.action(SinglePageView);
    h.single_page_wheel_scrolls();
    h.action(FitPage);
    h.set_page_turning(turning);
    h
}

/// Notches down, each `gap_ms` after the last; the first after a pause.
fn spin(h: &mut Harness, gap_ms: u64, notches: usize) {
    for index in 0..notches {
        h.notch_after(if index == 0 { 600 } else { gap_ms }, -1.);
    }
}

fn wheel_bursts_turn(cx: &mut TestAppContext, turning: PageTurning, expected: [u32; 3]) {
    let mut h = wheel_turning_setup(cx, turning);
    // A fast spin: ten notches 10 ms apart.
    spin(&mut h, 10, 10);
    let fast = h.frame().current_page;
    // No turn waits behind the wheel once it stops.
    h.cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    assert_eq!(h.frame().current_page, fast, "{turning:?}");
    // A second fast spin after a pause.
    spin(&mut h, 10, 4);
    let two = h.frame().current_page;
    // A slow scroll: a notch each 250 ms.
    spin(&mut h, 250, 4);
    let slow = h.frame().current_page;
    assert_eq!(
        [fast, two - fast, slow - two],
        expected,
        "{turning:?}: pages for a fast spin, another spin, a slow scroll"
    );
}

#[gpui::test]
fn responsive_turns_a_page_each_notch_except_during_a_turn(cx: &mut TestAppContext) {
    wheel_bursts_turn(cx, PageTurning::Responsive, [3, 1, 4]);
}

#[gpui::test]
fn balanced_turns_one_page_each_spin(cx: &mut TestAppContext) {
    wheel_bursts_turn(cx, PageTurning::Balanced, [1, 1, 4]);
}

#[gpui::test]
fn deliberate_turns_one_page_after_each_clear_pause(cx: &mut TestAppContext) {
    wheel_bursts_turn(cx, PageTurning::Deliberate, [1, 1, 1]);
}

#[gpui::test]
fn small_wheel_movements_add_up_to_one_notch(cx: &mut TestAppContext) {
    let mut h = wheel_turning_setup(cx, PageTurning::Responsive);
    for index in 0..3 {
        h.notch_after(if index == 0 { 600 } else { 5 }, -0.25);
        assert_eq!(h.frame().current_page, 0, "a quarter notch waits");
    }
    h.notch_after(5, -0.25);
    assert_eq!(h.frame().current_page, 1, "four quarters are one notch");
}

#[gpui::test]
fn zoomed_in_the_wheel_stops_at_the_edge_or_turns_as_set(cx: &mut TestAppContext) {
    for (turning, turns_in_the_spin) in [
        (PageTurning::Responsive, true),
        (PageTurning::Balanced, false),
        (PageTurning::Deliberate, false),
    ] {
        let mut cx = cx.clone();
        let mut h = wheel_turning_setup(&mut cx, turning);
        h.go_to_page(1);
        h.set_zoom(300.);
        // One long spin, a notch each 20 ms: far more than the page holds.
        spin(&mut h, 20, 60);
        let frame = h.frame();
        if turns_in_the_spin {
            assert!(frame.current_page > 1, "{turning:?}: {frame:?}");
            continue;
        }
        assert_eq!(frame.current_page, 1, "{turning:?} stops at the edge");
        let page = frame.page(1).unwrap();
        assert!(
            page.bottom() <= frame.viewport.bottom() + px(TOLERANCE),
            "{turning:?}: the page's bottom edge is in view: {frame:?}"
        );
        // A new spin turns the page.
        spin(&mut h, 20, 1);
        assert_eq!(h.frame().current_page, 2, "{turning:?}");
    }
}

#[gpui::test]
fn a_wheel_zoom_moves_smoothly_about_the_pointer_unless_reduce_motion_is_set(
    cx: &mut TestAppContext,
) {
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.set_zoom(100.);
    for (speed, factor) in [(ZoomSpeed::Fine, 1.10), (ZoomSpeed::Coarse, 1.35)] {
        h.cx.update(|_, cx| cx.set_global(ZoomSpeedPreference(speed)));
        let before = h.frame().zoom;
        let at =
            h.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap().center() + point(px(120.), px(80.));
        // With reduce motion set the zoom is there at once.
        h.cx.simulate_mouse_move(at, None, Modifiers::default());
        h.cx.simulate_event(ScrollWheelEvent {
            position: at,
            delta: ScrollDelta::Lines(point(0., NOTCH_LINES)),
            modifiers: Modifiers {
                control: true,
                ..Modifiers::default()
            },
            ..Default::default()
        });
        let zoomed = h.frame().zoom;
        assert!(
            (zoomed - before * factor).abs() < 0.2,
            "{speed:?}: {before} → {zoomed}"
        );
    }

    // Motion on: the zoom moves over frames to the same end.
    h.cx.update(|_, cx| cx.set_reduce_motion(false));
    h.cx.update(|_, cx| cx.set_global(ZoomSpeedPreference(ZoomSpeed::Standard)));
    let start = h.frame();
    let at = start.viewport.center() + point(px(120.), px(80.));
    h.cx.simulate_mouse_move(at, None, Modifiers::default());
    for _ in 0..2 {
        h.cx.simulate_event(ScrollWheelEvent {
            position: at,
            delta: ScrollDelta::Lines(point(0., NOTCH_LINES)),
            modifiers: Modifiers {
                control: true,
                ..Modifiers::default()
            },
            ..Default::default()
        });
    }
    h.cx.update(|window, cx| window.draw(cx).clear(cx));
    let early = h.workspace.read_with(h.cx, |workspace, cx| {
        workspace
            .document_view_state(start_document(workspace), cx)
            .unwrap()
            .zoom_percent()
    });
    assert!(early < start.zoom * 1.44 - 1., "the zoom moves: {early}");
    for _ in 0..20 {
        h.cx.executor()
            .advance_clock(std::time::Duration::from_millis(20));
        h.cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let end = h.frame();
    // Two notches during the motion: the second goes on from the first's end.
    assert!(
        (end.zoom - start.zoom * 1.44).abs() < 0.2,
        "{} → {}",
        start.zoom,
        end.zoom
    );
}

fn start_document(workspace: &DocumentWorkspace) -> DocumentId {
    workspace.active_document_id().unwrap()
}

// ---------------------------------------------------------------------------
// Rotate All Pages
// ---------------------------------------------------------------------------

/// Rotate All Pages, and its Undo and Redo, change the height of the pages
/// above the view in Continuous view at a free zoom: the view stays on the
/// page it showed.
#[gpui::test]
fn rotate_all_pages_keeps_the_view_on_its_page_in_continuous_view(cx: &mut TestAppContext) {
    use butter_paper::annotation_model::PageRotationDirection;
    let mut h = setup(cx, &UNIFORM_PAGES);
    h.action(ContinuousView);
    h.set_zoom(80.);
    h.go_to_page(3);
    let before = h.frame();
    assert_eq!(before.current_page, 3);
    let centre_page = |frame: &Frame| {
        let centre = frame.viewport.center();
        frame
            .pages
            .iter()
            .find(|(_, bounds)| bounds.contains(&centre))
            .map(|(page, _)| *page)
    };
    assert_eq!(centre_page(&before), Some(3), "{before:?}");
    let document = h.document;
    h.workspace.update(h.cx, |workspace, cx| {
        workspace
            .rotate_all_pages(document, PageRotationDirection::Right, cx)
            .unwrap();
    });
    let turned = h.frame();
    assert_eq!(
        centre_page(&turned),
        Some(3),
        "after Rotate All: {turned:?}"
    );
    assert_eq!(turned.current_page, 3);
    for undo in [true, false] {
        h.workspace.update(h.cx, |workspace, cx| {
            if undo {
                workspace.undo_annotations(document, cx).unwrap();
            } else {
                workspace.redo_annotations(document, cx).unwrap();
            }
        });
        let frame = h.frame();
        assert_eq!(centre_page(&frame), Some(3), "undo {undo}: {frame:?}");
    }
}
