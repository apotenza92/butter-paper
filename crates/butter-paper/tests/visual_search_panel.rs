//! Visual search in the search panel, driven by the same pointer events and
//! key bindings as the live app: the Visual mode, Draw box with the Snapshot
//! tool, Paste picture (from the test clipboard, never the desktop's), the
//! similarity slider, the options, Stop, opening and stepping through hits,
//! Dark pages, and a document that closes during a search.
//!
//! A stand-in worker draws each page as a search asks for it: a white page
//! with "F" shapes (they differ at every turn and mirror) and one "E" (a
//! near miss). It records what it was asked for, so the tests can see that a
//! search asks for the pages in their real colours, and that the slider
//! needs no new drawing.

use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use butter_paper::annotation_adapter::AnnotationTool;
use butter_paper::annotation_model::PageRotation;
use butter_paper::document_workspace::{
    DOCUMENT_SEARCH_FOOTER_ID, DOCUMENT_SEARCH_MODE_TEXT_ID, DOCUMENT_SEARCH_MODE_VISUAL_ID,
    DOCUMENT_SEARCH_PANEL_ID, DOCUMENT_SEARCH_SCOPE_THIS_ID, DOCUMENT_SEARCH_TRY_VISUAL_ID,
    DOCUMENT_VISUAL_CLEAR_ID, DOCUMENT_VISUAL_DRAW_BOX_ID, DOCUMENT_VISUAL_MIRRORED_ID,
    DOCUMENT_VISUAL_PASTE_ID, DOCUMENT_VISUAL_SIMILARITY_ID, DOCUMENT_VISUAL_STOP_ID,
    DOCUMENT_VISUAL_TEMPLATE_ID, DocumentId, DocumentWorkspace, NativeDocumentResource,
    OpenedNativeDocument, RasterSurface, SearchMode, TemplateOrigin, ThumbnailSurface,
    VisualPanelRow, VisualPanelSummary, document_annotation_layer_id,
    init_document_workspace_actions, search_panel_shortcut, visual_tile_id,
};
use butter_paper::page_view_control::PageViewMode;
use butter_paper::paper_appearance::PaperAppearance;
use butter_paper::pdf_text_edit::PageTextLayer;
use butter_paper::search_scope::set_workspace_directory;
use gpui::{
    AppContext as _, ClipboardItem, Entity, Image, ImageFormat, Modifiers, TestAppContext,
    VisualTestContext, point, px,
};
use gpui_component::Root;

const PAGE_WIDTH: f32 = 612.;
const PAGE_HEIGHT: f32 = 792.;

/// A shape on a page: top-left in viewport points, its side, clockwise
/// quarter turns after a left-to-right mirror.
#[derive(Clone, Copy, Debug)]
struct Mark {
    x: f32,
    y: f32,
    side: f32,
    turn: u8,
    mirrored: bool,
    /// An "E": an "F" with a foot.
    foot: bool,
}

const fn f(x: f32, y: f32, side: f32, turn: u8, mirrored: bool) -> Mark {
    Mark {
        x,
        y,
        side,
        turn,
        mirrored,
        foot: false,
    }
}

/// The "F" in a unit box, y down: stem, top bar, middle bar.
const F_BARS: [(f32, f32, f32, f32); 3] = [
    (0.10, 0.05, 0.30, 0.95),
    (0.10, 0.05, 0.90, 0.25),
    (0.10, 0.45, 0.65, 0.60),
];
const FOOT: (f32, f32, f32, f32) = (0.10, 0.70, 0.90, 0.95);

impl Mark {
    fn map(self, (u, v): (f32, f32)) -> (f32, f32) {
        let u = if self.mirrored { 1. - u } else { u };
        let (mut du, mut dv) = (u - 0.5, v - 0.5);
        for _ in 0..self.turn {
            (du, dv) = (-dv, du);
        }
        (
            self.x + (du + 0.5) * self.side,
            self.y + (dv + 0.5) * self.side,
        )
    }

    /// Its bars as viewport rectangles: left, top, right, bottom.
    fn bars(self) -> Vec<(f32, f32, f32, f32)> {
        let mut bars = F_BARS.to_vec();
        if self.foot {
            bars.push(FOOT);
        }
        bars.into_iter()
            .map(|(u0, v0, u1, v1)| {
                let (a, b) = (self.map((u0, v0)), self.map((u1, v1)));
                (a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1))
            })
            .collect()
    }
}

/// A white page with `marks`, at `scale` pixels per point, as BGRA.
fn draw(marks: &[Mark], scale: f32) -> RasterSurface {
    let width = (PAGE_WIDTH * scale).ceil() as usize;
    let height = (PAGE_HEIGHT * scale).ceil() as usize;
    let mut pixels = vec![0xff; width * height * 4];
    for mark in marks {
        for (left, top, right, bottom) in mark.bars() {
            let x0 = (left * scale).round().max(0.) as usize;
            let x1 = ((right * scale).round() as usize).min(width);
            let y0 = (top * scale).round().max(0.) as usize;
            let y1 = ((bottom * scale).round() as usize).min(height);
            for y in y0..y1 {
                for x in x0..x1 {
                    pixels[(y * width + x) * 4..][..3].fill(0x20);
                }
            }
        }
    }
    RasterSurface::new(width as u32, height as u32, pixels).unwrap()
}

/// A PNG of one "F" at `scale` pixels per point, with a white margin.
fn picture_png(side: f32, scale: f32) -> Vec<u8> {
    let size = ((side + 12.) * scale).ceil() as u32;
    let mark = f(6., 6., side, 0, false);
    let mut image = image::RgbaImage::from_pixel(size, size, image::Rgba([255, 255, 255, 255]));
    for (left, top, right, bottom) in mark.bars() {
        for y in (top * scale).round() as u32..(bottom * scale).round() as u32 {
            for x in (left * scale).round() as u32..(right * scale).round() as u32 {
                image.put_pixel(x, y, image::Rgba([32, 32, 32, 255]));
            }
        }
    }
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    png
}

#[derive(Default)]
struct Calls {
    /// Pages a search drew, and at which scale.
    search: Vec<(u32, f32)>,
    /// The paper the view asked for.
    view: Vec<PaperAppearance>,
}

struct SymbolResource {
    pages: Vec<Vec<Mark>>,
    calls: Arc<Mutex<Calls>>,
    delay: Duration,
    released: AtomicBool,
}

fn paper(width: u32, height: u32, paper: PaperAppearance) -> RasterSurface {
    let value = match paper {
        PaperAppearance::Real => 0xff,
        PaperAppearance::Dark => 0x1e,
    };
    RasterSurface::new(
        width,
        height,
        vec![value; width as usize * height as usize * 4],
    )
    .unwrap()
}

impl NativeDocumentResource for SymbolResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4402)
    }

    fn render_page(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
        Ok(paper(width, page_index + 2, PaperAppearance::Real))
    }

    fn render_view_page(
        &self,
        page_index: u32,
        width: u32,
        appearance: PaperAppearance,
    ) -> Result<RasterSurface, String> {
        self.calls.lock().unwrap().view.push(appearance);
        Ok(paper(width, page_index + 2, appearance))
    }

    fn render_tile(
        &self,
        request: butter_paper::viewer::TileRequest,
    ) -> Result<RasterSurface, String> {
        self.calls.lock().unwrap().view.push(request.paper);
        Ok(paper(
            request.crop.width as u32,
            request.crop.height as u32,
            request.paper,
        ))
    }

    fn render_search_page(
        &self,
        page_index: u32,
        pixels_per_point: f32,
        rotation: PageRotation,
    ) -> Result<RasterSurface, String> {
        assert_eq!(rotation, PageRotation::Degrees0);
        std::thread::sleep(self.delay);
        self.calls
            .lock()
            .unwrap()
            .search
            .push((page_index, pixels_per_point));
        Ok(draw(
            self.pages.get(page_index as usize).ok_or("no such page")?,
            pixels_per_point,
        ))
    }

    fn page_text(&self, _page_index: u32) -> Result<PageTextLayer, String> {
        Ok(PageTextLayer::default())
    }

    fn close(&self) -> Result<(), String> {
        self.released.store(true, Ordering::Release);
        Ok(())
    }

    fn is_released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }
}

/// plan.pdf, page 1: the source "F", one turned, one turned and mirrored,
/// one mirrored, and an "E". Page 2: an "F" turned 270° and 8 % larger.
fn plan_pages() -> Vec<Vec<Mark>> {
    vec![
        vec![
            f(100., 100., 24., 0, false),
            f(300.4, 100.3, 24., 1, false),
            f(100.2, 300.6, 24., 2, true),
            f(300.7, 300.2, 24., 0, true),
            Mark {
                foot: true,
                ..f(450.5, 450.5, 24., 0, false)
            },
        ],
        vec![f(200.3, 500.6, 26., 3, false)],
    ]
}

struct Fixture<'a> {
    workspace: Entity<DocumentWorkspace>,
    plan: DocumentId,
    other: DocumentId,
    plan_calls: Arc<Mutex<Calls>>,
    other_calls: Arc<Mutex<Calls>>,
    cx: &'a mut VisualTestContext,
}

fn open(
    workspace: &Entity<DocumentWorkspace>,
    cx: &mut VisualTestContext,
    name: &str,
    pages: Vec<Vec<Mark>>,
    delay: Duration,
) -> (DocumentId, Arc<Mutex<Calls>>) {
    let calls = Arc::new(Mutex::new(Calls::default()));
    let count = pages.len();
    let resource = Arc::new(SymbolResource {
        pages,
        calls: calls.clone(),
        delay,
        released: AtomicBool::new(false),
    });
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from(name), cx)
    });
    let document_id = request.document_id;
    workspace.update(cx, |workspace, cx| {
        let opened = OpenedNativeDocument::new(
            name,
            vec![(PAGE_WIDTH, PAGE_HEIGHT); count],
            paper(32, 40, PaperAppearance::Real),
            (0..count as u32)
                .map(|page| ThumbnailSurface::new(page, paper(8, 10, PaperAppearance::Real)))
                .collect(),
            resource,
        )
        .unwrap();
        workspace.apply_open_result(&request, Ok(opened), cx);
        workspace.set_view_configuration(document_id, PageViewMode::Continuous, 100., cx);
        workspace
            .refresh_viewport_async(document_id, 800., 600., 1., cx)
            .unwrap();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    (document_id, calls)
}

/// One window with two tabs: other.pdf (one "F" on each page), then
/// plan.pdf (active).
fn setup(cx: &mut TestAppContext, other_pages: usize, delay: Duration) -> Fixture<'_> {
    cx.update(|cx| {
        gpui_component::init(cx);
        cx.set_reduce_motion(true);
        init_document_workspace_actions(cx);
    });
    let slot = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let slot = slot.clone();
        move |window, cx| {
            let workspace = cx.new(DocumentWorkspace::new);
            slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    workspace.update(cx, |workspace, _| {
        workspace.share_document_ids(Arc::new(AtomicU64::new(1)))
    });
    let first = cx.update(|window, _| window.window_handle());
    let weak = workspace.downgrade();
    cx.update(|_, cx| set_workspace_directory(move |_| vec![(first, weak.clone())], cx));
    let mut other_doc = vec![vec![f(150.5, 150.5, 24., 0, false)]];
    other_doc.extend((1..other_pages).map(|_| vec![f(150.5, 150.5, 24., 0, false)]));
    let (other, other_calls) = open(&workspace, cx, "other.pdf", other_doc, delay);
    let (plan, plan_calls) = open(&workspace, cx, "plan.pdf", plan_pages(), Duration::ZERO);
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        let focus = workspace.read(cx).focus_handle();
        window.focus(&focus, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    Fixture {
        workspace,
        plan,
        other,
        plan_calls,
        other_calls,
        cx,
    }
}

impl Fixture<'_> {
    fn draw(&mut self) {
        self.cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn summary(&mut self) -> VisualPanelSummary {
        self.workspace
            .read_with(self.cx, |workspace, cx| workspace.visual_panel_summary(cx))
            .expect("the panel was opened")
    }

    fn wait_for(
        &mut self,
        what: &str,
        done: impl Fn(&VisualPanelSummary) -> bool,
    ) -> VisualPanelSummary {
        for _ in 0..800 {
            self.cx.executor().advance_clock(Duration::from_millis(40));
            self.cx.run_until_parked();
            self.draw();
            let summary = self.summary();
            if done(&summary) {
                return summary;
            }
            std::thread::sleep(Duration::from_millis(4));
        }
        panic!("waited for {what}, found {:?}", self.summary());
    }

    fn click(&mut self, id: &'static str) {
        let bounds = self.cx.debug_bounds(id).expect("the control is drawn");
        self.cx
            .simulate_click(bounds.center(), Modifiers::default());
        self.draw();
    }

    fn click_tile(&mut self, index: usize) {
        let id: &'static str = Box::leak(visual_tile_id(index).into_boxed_str());
        self.click(id);
    }

    fn open_visual(&mut self) {
        self.cx
            .simulate_keystrokes(search_panel_shortcut(cfg!(target_os = "macos")));
        self.draw();
        self.click(DOCUMENT_SEARCH_MODE_VISUAL_ID);
    }

    fn tool(&mut self) -> Option<AnnotationTool> {
        let plan = self.plan;
        self.workspace
            .read_with(self.cx, |workspace, cx| workspace.annotation_tool(plan, cx))
    }

    /// Draws a box with the Snapshot tool on page 1 of plan.pdf, from one
    /// viewport point (top-left points) to another: press, then press, as
    /// the tool takes it.
    fn draw_box(&mut self, from: (f32, f32), to: (f32, f32)) {
        let layer_id = Box::leak(document_annotation_layer_id(self.plan, 0).into_boxed_str());
        let layer = self.cx.debug_bounds(layer_id).expect("the page is painted");
        let scale = f32::from(layer.size.width) / PAGE_WIDTH;
        let project = |(x, y): (f32, f32)| {
            point(
                layer.origin.x + px(x * scale),
                layer.origin.y + px(y * scale),
            )
        };
        self.cx.simulate_click(project(from), Modifiers::default());
        self.cx
            .simulate_mouse_move(project(to), None, Modifiers::default());
        self.cx.simulate_click(project(to), Modifiers::default());
        self.cx.run_until_parked();
        self.draw();
    }

    /// Draw box, round the source "F", and the search to its end.
    fn search_by_box(&mut self) -> VisualPanelSummary {
        self.open_visual();
        self.click(DOCUMENT_VISUAL_DRAW_BOX_ID);
        assert_eq!(self.tool(), Some(AnnotationTool::Snapshot));
        self.draw_box((94., 94.), (130., 130.));
        self.wait_for("the search", |summary| {
            summary.finished && summary.template.is_some()
        })
    }
}

fn hits_on(summary: &VisualPanelSummary) -> Vec<(String, usize)> {
    summary
        .rows
        .iter()
        .filter_map(|row| match row {
            VisualPanelRow::Document { name, count, .. } => Some((name.clone(), *count)),
            _ => None,
        })
        .collect()
}

#[gpui::test]
fn visual_mode_starts_with_no_picture_and_its_controls(cx: &mut TestAppContext) {
    let mut f = setup(cx, 1, Duration::ZERO);
    f.open_visual();
    let summary = f.summary();
    assert_eq!(summary.mode, SearchMode::Visual);
    assert_eq!(summary.template, None);
    assert!(summary.rows.is_empty());
    assert_eq!(summary.similarity, 80);
    assert!(summary.turns && summary.sizes && summary.mirrored);
    for id in [
        DOCUMENT_SEARCH_PANEL_ID,
        DOCUMENT_VISUAL_TEMPLATE_ID,
        DOCUMENT_VISUAL_DRAW_BOX_ID,
        DOCUMENT_VISUAL_PASTE_ID,
        DOCUMENT_VISUAL_SIMILARITY_ID,
        DOCUMENT_VISUAL_MIRRORED_ID,
    ] {
        assert!(f.cx.debug_bounds(id).is_some(), "{id} is drawn");
    }
    assert!(f.cx.debug_bounds(DOCUMENT_VISUAL_CLEAR_ID).is_none());
    // The option check boxes are the small stock size, as the panel's other
    // controls are: their text is as small as the Text mode's options.
    let mirrored = f.cx.debug_bounds(DOCUMENT_VISUAL_MIRRORED_ID).unwrap();
    assert!(
        mirrored.size.height <= gpui::px(14.),
        "a small check box: {mirrored:?}"
    );
    // Nothing is drawn for a search without a picture.
    assert!(f.plan_calls.lock().unwrap().search.is_empty());
    // The Text switch goes back.
    f.click(DOCUMENT_SEARCH_MODE_TEXT_ID);
    assert_eq!(f.summary().mode, SearchMode::Text);
}

#[gpui::test]
fn a_box_drawn_on_the_page_finds_each_copy_and_leaves_no_markup(cx: &mut TestAppContext) {
    let mut f = setup(cx, 1, Duration::ZERO);
    let summary = f.search_by_box();
    assert!(
        matches!(
            summary.template,
            Some(TemplateOrigin::PageBox { page_index: 0, .. })
        ),
        "{summary:?}"
    );
    assert_eq!(summary.template_text[0], "plan.pdf, page 1");
    // plan.pdf: the source, the turned, mirrored and larger copies; the
    // "E" stays under 80 %. other.pdf: its "F".
    assert_eq!(
        hits_on(&summary),
        [("other.pdf".to_owned(), 1), ("plan.pdf".to_owned(), 5)],
        "{summary:#?}"
    );
    assert_eq!(summary.summary, "6 matches on 3 pages in 2 documents");
    let sources = summary.shown.iter().filter(|tile| tile.source).count();
    assert_eq!(sources, 1, "the box is listed as the source");
    assert!(
        summary
            .shown
            .iter()
            .any(|tile| tile.rotation == 90 && !tile.mirrored)
    );
    assert!(
        summary
            .shown
            .iter()
            .any(|tile| tile.rotation == 180 && tile.mirrored)
    );
    assert!(
        summary
            .shown
            .iter()
            .any(|tile| tile.rotation == 0 && tile.mirrored)
    );
    // Page 2's "F" is 8 % larger and turned 270°.
    assert!(summary.shown.iter().any(|tile| tile.rotation == 270));
    // Rows: document, page, its pictures (three to a row), in tab order.
    assert!(matches!(summary.rows[3], VisualPanelRow::Document { .. }));
    assert!(matches!(
        summary.rows[4],
        VisualPanelRow::Page {
            page_number: 1,
            count: 4,
            ..
        }
    ));
    assert!(matches!(&summary.rows[5], VisualPanelRow::Tiles(tiles) if tiles.len() == 3));
    assert!(matches!(&summary.rows[6], VisualPanelRow::Tiles(tiles) if tiles.len() == 1));
    // The box copied nothing and left no markup; the tool is back.
    let plan = f.plan;
    let snapshot = f
        .workspace
        .read_with(f.cx, |workspace, cx| {
            workspace.annotation_snapshot(plan, cx)
        })
        .unwrap();
    assert!(snapshot.snapshots.is_empty());
    assert!(f.cx.read_from_clipboard().is_none());
    assert_eq!(f.tool(), Some(AnnotationTool::Select));
    // The box's page was drawn at the box's scale, then at the scale for
    // what it holds; the search drew every page once at that scale, the
    // source page from the cache.
    let plan_calls = f.plan_calls.lock().unwrap();
    let last = plan_calls.search.last().unwrap().1;
    let at_last = plan_calls
        .search
        .iter()
        .filter(|(_, scale)| *scale == last)
        .map(|(page, _)| *page)
        .collect::<Vec<_>>();
    assert_eq!(at_last, [0, 1], "{:?}", plan_calls.search);
    assert!(plan_calls.search.len() <= 3, "{:?}", plan_calls.search);
    drop(plan_calls);
    assert_eq!(f.other_calls.lock().unwrap().search.len(), 1);
}

#[gpui::test]
fn the_similarity_slider_filters_with_no_new_search(cx: &mut TestAppContext) {
    let mut f = setup(cx, 1, Duration::ZERO);
    let summary = f.search_by_box();
    let drawn = f.plan_calls.lock().unwrap().search.len();
    let at_80 = summary.hits;
    let workspace = f.workspace.clone();
    f.cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.set_visual_similarity_percent(60., window, cx)
        })
    });
    f.draw();
    let loose = f.summary();
    assert_eq!(loose.similarity, 70, "the slider goes down to 70 % only");
    // The "E" (at 450, 450 on the page, 24 pt) shows at 70 %, with weaker
    // part matches.
    let near_e = |tile: &butter_paper::document_workspace::VisualTileView| {
        tile.bounds.is_some_and(|[x0, y0, x1, y1]| {
            let (x, y) = ((x0 + x1) / 2., PAGE_HEIGHT as f64 - (y0 + y1) / 2.);
            (x - 462.5).abs() < 6. && (y - 462.5).abs() < 6.
        })
    };
    assert!(!summary.shown.iter().any(near_e), "not at 80 %");
    assert!(
        loose.shown.iter().any(near_e),
        "the E shows at 70 %: {loose:#?}"
    );
    assert!(loose.hits > at_80);
    f.cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.set_visual_similarity_percent(99., window, cx)
        })
    });
    f.draw();
    let strict = f.summary();
    assert!(strict.hits < at_80, "{strict:#?}");
    assert!(
        strict.shown.iter().any(|tile| tile.source),
        "the source stays"
    );
    assert_eq!(
        f.plan_calls.lock().unwrap().search.len(),
        drawn,
        "no page was drawn again"
    );
    assert!(strict.finished);
}

#[gpui::test]
fn a_hit_opens_outlined_and_the_keyboard_steps_through_the_hits(cx: &mut TestAppContext) {
    let mut f = setup(cx, 1, Duration::ZERO);
    let summary = f.search_by_box();
    // other.pdf's hit (the first): its tab comes forward, outlined.
    assert!(summary.shown[0].bounds.is_some());
    f.click_tile(0);
    let other = f.other;
    let (active, highlights) = f.workspace.read_with(f.cx, |workspace, cx| {
        (
            workspace.active_document_id(),
            workspace.find_highlights(other, cx),
        )
    });
    assert_eq!(active, Some(other));
    assert!(
        highlights
            .iter()
            .any(|page| page.page_index == 0 && page.outline_current.len() == 1),
        "{highlights:?}"
    );
    assert_eq!(f.summary().opened, Some(0));
    // The list keeps the focus: Down passes plan.pdf's document and page
    // rows to its first hit, and Enter opens it.
    for _ in 0..3 {
        f.cx.simulate_keystrokes("down");
        f.draw();
    }
    let summary = f.summary();
    assert_eq!(summary.selected_hit, Some(1), "{summary:#?}");
    f.cx.simulate_keystrokes("enter");
    f.draw();
    assert_eq!(f.summary().opened, Some(1));
    let plan = f.plan;
    let (active, highlights) = f.workspace.read_with(f.cx, |workspace, cx| {
        (
            workspace.active_document_id(),
            workspace.find_highlights(plan, cx),
        )
    });
    assert_eq!(active, Some(plan));
    let outlined = highlights
        .iter()
        .map(|page| page.outlines.len() + page.outline_current.len())
        .sum::<usize>();
    assert!(outlined >= 2, "the page's other hits are outlined too");
    // Left goes from a hit to its page.
    f.cx.simulate_keystrokes("left");
    f.draw();
    assert_eq!(f.summary().selected_hit, None);
}

#[gpui::test]
fn stop_ends_the_search_and_keeps_what_it_found(cx: &mut TestAppContext) {
    let mut f = setup(cx, 40, Duration::from_millis(25));
    f.open_visual();
    f.click(DOCUMENT_VISUAL_DRAW_BOX_ID);
    f.draw_box((94., 94.), (130., 130.));
    let running = f.wait_for("the first pages", |summary| {
        summary.running && summary.pages.0 >= 3
    });
    assert!(
        running.progress.starts_with("Searching page "),
        "{running:?}"
    );
    assert!(f.cx.debug_bounds(DOCUMENT_VISUAL_STOP_ID).is_some());
    f.click(DOCUMENT_VISUAL_STOP_ID);
    let stopped = f.wait_for("the stop", |summary| summary.finished);
    assert!(stopped.cancelled);
    assert!(
        stopped.progress.starts_with("Stopped after "),
        "{stopped:?}"
    );
    assert!(stopped.pages.0 < stopped.pages.1);
    assert!(stopped.hits >= 4, "what it found stays: {stopped:#?}");
    let drawn = f.other_calls.lock().unwrap().search.len();
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        f.other_calls.lock().unwrap().search.len() <= drawn + 1,
        "no more pages are drawn"
    );
    assert!(f.cx.debug_bounds(DOCUMENT_VISUAL_STOP_ID).is_none());
}

#[gpui::test]
fn closing_a_document_stops_its_part_of_the_search(cx: &mut TestAppContext) {
    let mut f = setup(cx, 40, Duration::from_millis(25));
    f.open_visual();
    f.click(DOCUMENT_VISUAL_DRAW_BOX_ID);
    f.draw_box((94., 94.), (130., 130.));
    f.wait_for("the first pages", |summary| summary.pages.0 >= 4);
    let other = f.other;
    f.workspace
        .update(f.cx, |workspace, cx| workspace.close_document(other, cx));
    let drawn = f.other_calls.lock().unwrap().search.len();
    let summary = f.wait_for("the search to end", |summary| summary.finished);
    assert!(drawn < 40);
    assert!(f.other_calls.lock().unwrap().search.len() <= drawn + 1);
    assert_eq!(hits_on(&summary), [("plan.pdf".to_owned(), 5)]);
}

#[gpui::test]
fn an_option_runs_the_search_again(cx: &mut TestAppContext) {
    let mut f = setup(cx, 1, Duration::ZERO);
    let strong =
        |summary: &VisualPanelSummary| summary.shown.iter().filter(|tile| tile.score >= 90).count();
    let summary = f.search_by_box();
    let with_mirrored = strong(&summary);
    f.click(DOCUMENT_VISUAL_MIRRORED_ID);
    let summary = f.wait_for("the new search", |summary| {
        !summary.mirrored && summary.finished
    });
    // The two mirrored copies now match only as turned ones, weakly.
    assert_eq!(strong(&summary), with_mirrored - 2, "{summary:#?}");
    assert!(summary.shown.iter().all(|tile| !tile.mirrored));
    // This document leaves other.pdf out.
    f.click(DOCUMENT_SEARCH_SCOPE_THIS_ID);
    let summary = f.wait_for("this document", |summary| {
        summary.finished && hits_on(summary).len() == 1
    });
    assert_eq!(hits_on(&summary)[0].0, "plan.pdf");
    // The X clears the picture and the results.
    f.click(DOCUMENT_VISUAL_CLEAR_ID);
    let summary = f.summary();
    assert_eq!(summary.template, None);
    assert!(summary.rows.is_empty());
}

#[gpui::test]
fn a_pasted_picture_is_found_at_the_zoom_in_view(cx: &mut TestAppContext) {
    let mut f = setup(cx, 1, Duration::ZERO);
    f.open_visual();
    // No picture on the clipboard: the panel says what to do.
    f.click(DOCUMENT_VISUAL_PASTE_ID);
    assert_eq!(
        f.summary().error.as_deref(),
        Some("Copy a picture first, such as a screenshot.")
    );
    // A screenshot of the page in view, on the test clipboard: the page's
    // pixels for each point as painted (the test views have a device scale
    // of 1).
    let layer_id = Box::leak(document_annotation_layer_id(f.plan, 0).into_boxed_str());
    let layer = f.cx.debug_bounds(layer_id).expect("the page is painted");
    let scale = f32::from(layer.size.width) / PAGE_WIDTH;
    let png = picture_png(24., scale);
    f.cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(
        ImageFormat::Png,
        png,
    )));
    f.click(DOCUMENT_VISUAL_PASTE_ID);
    let summary = f.wait_for("the search", |summary| {
        summary.finished && summary.template.is_some()
    });
    assert_eq!(summary.template, Some(TemplateOrigin::Pasted));
    assert_eq!(summary.error, None);
    assert_eq!(
        hits_on(&summary),
        [("other.pdf".to_owned(), 1), ("plan.pdf".to_owned(), 5)],
        "{summary:#?}"
    );
    assert!(
        summary.shown.iter().all(|tile| !tile.source),
        "no source box"
    );
}

#[gpui::test]
fn dark_pages_outline_the_hit_and_the_search_draws_real_colours(cx: &mut TestAppContext) {
    let mut f = setup(cx, 1, Duration::ZERO);
    f.workspace.update(f.cx, |workspace, cx| {
        workspace.set_paper_appearance(PaperAppearance::Dark, cx)
    });
    for _ in 0..2 {
        f.draw();
        f.cx.run_until_parked();
    }
    f.plan_calls.lock().unwrap().view.clear();
    let summary = f.search_by_box();
    assert_eq!(summary.hits, 6, "{summary:#?}");
    // The view asked for dark pages; the search drew its own pages, which
    // the worker draws in their real colours.
    assert!(
        f.plan_calls
            .lock()
            .unwrap()
            .view
            .iter()
            .all(|paper| *paper == PaperAppearance::Dark)
    );
    assert!(!f.plan_calls.lock().unwrap().search.is_empty());
    f.click_tile(1);
    let plan = f.plan;
    let highlights = f
        .workspace
        .read_with(f.cx, |workspace, cx| workspace.find_highlights(plan, cx));
    assert!(
        highlights
            .iter()
            .any(|page| page.outline_current.len() == 1)
    );
}

#[gpui::test]
fn escape_stops_drawing_the_box(cx: &mut TestAppContext) {
    let mut f = setup(cx, 1, Duration::ZERO);
    f.open_visual();
    f.click(DOCUMENT_VISUAL_DRAW_BOX_ID);
    assert_eq!(f.tool(), Some(AnnotationTool::Snapshot));
    // The prompt over the page says what to draw.
    assert!(
        f.cx.debug_bounds(butter_paper::document_workspace::DOCUMENT_SIGNATURE_BOX_PROMPT_ID)
            .is_some()
    );
    // The page has the keys.
    let workspace = f.workspace.clone();
    assert!(f.cx.update(|window, cx| workspace.read(cx).focus_handle().is_focused(window)));
    f.cx.simulate_keystrokes("escape");
    f.draw();
    assert_eq!(f.tool(), Some(AnnotationTool::Select));
    assert_eq!(f.summary().template, None);
}

#[gpui::test]
fn the_text_footer_offers_visual_search_for_pages_without_text(cx: &mut TestAppContext) {
    let mut f = setup(cx, 1, Duration::ZERO);
    f.cx.simulate_keystrokes(search_panel_shortcut(cfg!(target_os = "macos")));
    f.draw();
    f.cx.simulate_keystrokes("d o o r");
    f.draw();
    for _ in 0..100 {
        f.cx.executor().advance_clock(Duration::from_millis(40));
        f.cx.run_until_parked();
        f.draw();
        if f.cx.debug_bounds(DOCUMENT_SEARCH_TRY_VISUAL_ID).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    assert!(f.cx.debug_bounds(DOCUMENT_SEARCH_FOOTER_ID).is_some());
    f.click(DOCUMENT_SEARCH_TRY_VISUAL_ID);
    assert_eq!(f.summary().mode, SearchMode::Visual);
}
