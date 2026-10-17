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

use butter_paper::document_workspace::{
    ActualSize, ContinuousView, DOCUMENT_VIEWPORT_ID, DocumentId, DocumentWorkspace, FitPage,
    FitWidth, NativeDocumentOpener, NativeDocumentResource, NavigateDown, NavigateEnd,
    NavigateHome, NavigateNextPage, NavigatePageDown, NavigatePageUp, NavigatePreviousPage,
    NavigateUp, OpenDocumentRequest, OpenedNativeDocument, RasterSurface, SinglePageView,
    ThumbnailSurface, ZoomIn, ZoomOut, document_session_tab_id, document_viewer_page_id,
    init_document_workspace_actions,
};
use butter_paper::native_document_view_state::ViewerZoomPreset;
use butter_paper::page_view_control::{
    CONTINUOUS_PRIMARY_ID, PageViewMode, SINGLE_PAGE_PRIMARY_ID, WheelBehavior,
};
use gpui::{
    AppContext as _, Bounds, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels,
    ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext, point, px, size,
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

struct Resource(Arc<AtomicBool>);

impl NativeDocumentResource for Resource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4343)
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
            Arc::new(Resource(Arc::new(AtomicBool::new(false)))),
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

    /// Notches of a discrete mouse wheel; negative turns it down.
    fn notch(&mut self, lines: f32) {
        self.settle();
        let at = self.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap().center();
        self.cx.simulate_mouse_move(at, None, Modifiers::default());
        self.cx.simulate_event(ScrollWheelEvent {
            position: at,
            delta: ScrollDelta::Lines(point(0., lines)),
            ..Default::default()
        });
        self.settle();
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
        |h| h.notch(-3.),
        |h| h.notch(3.),
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

/// The viewer's blank space around the pages at a zoom (`PAGE_GAP`).
fn page_gap(zoom: f32) -> f32 {
    24. * zoom / 100.
}

#[gpui::test]
fn wheel_scrolling_stops_at_the_first_and_last_pages(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.set_zoom(200.);
    let gap = page_gap(200.);
    for _ in 0..200 {
        h.wheel(0., -400., false);
    }
    let bottom = h.frame();
    let last = bottom.page(4).expect("the last page is in view");
    assert!(
        (f32::from(bottom.viewport.bottom() - last.bottom()) - gap).abs() <= TOLERANCE,
        "wheel scrolling down stops at the last page's bottom margin: {bottom:?}"
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
        "wheel scrolling up stops at the first page's top margin: {top:?}"
    );
}

#[gpui::test]
fn horizontal_wheel_scrolling_stops_at_the_page_sides(cx: &mut TestAppContext) {
    let mut h = setup(cx, &MIXED_PAGES);
    h.set_zoom(300.);
    h.go_to_page(1);
    let gap = page_gap(300.);
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
        f32::from(panned_top - panned.viewport.top()) > page_gap(200.) + 300.,
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
