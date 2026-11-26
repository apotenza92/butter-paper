//! The find bar in the workspace, driven by the same key bindings and
//! pointer events as the live app: Cmd+F or Ctrl+F opens it, typing finds
//! as it goes, Enter and Shift+Enter step and wrap, Match case and Match
//! whole word narrow it, and Escape closes it and clears the highlights.
//! The search reads page text through a stand-in worker, so these tests run
//! with no PDFium; the real worker is covered by `search_engine`.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use butter_paper::annotation_adapter::AnnotationTool;
use butter_paper::annotation_model::{
    Annotation, MarkupId, PdfRect, TextBoxAnnotation, TextBoxStyle,
};
use butter_paper::document_workspace::{
    DOCUMENT_FIND_BAR_ID, DOCUMENT_FIND_CLOSE_ID, DOCUMENT_FIND_COUNT_ID, DOCUMENT_FIND_INPUT_ID,
    DOCUMENT_FIND_MATCH_CASE_ID, DOCUMENT_FIND_NEXT_ID, DOCUMENT_FIND_OPTIONS_ID,
    DOCUMENT_FIND_PREVIOUS_ID, DOCUMENT_FIND_STEPS_ID, DOCUMENT_FIND_WHOLE_WORD_ID,
    DOCUMENT_PAGE_ID, DOCUMENT_VIEWPORT_ID, DocumentId, DocumentWorkspace, FindBarSummary,
    NativeDocumentResource, OpenedNativeDocument, RasterSurface, ThumbnailSurface,
    document_annotation_layer_id, find_shortcut, init_document_workspace_actions,
};
use butter_paper::page_view_control::PageViewMode;
use butter_paper::pdf_text_edit::{PageTextLayer, TextEditLimits, extract_page_text};
use gpui::{AppContext as _, Modifiers, TestAppContext, VisualTestContext, point, px};
use gpui_component::{Root, WindowExt as _};
use lopdf::{Document, Stream, dictionary};

const PAGE_WIDTH: f32 = 612.;
const PAGE_HEIGHT: f32 = 792.;

fn raster(width: u32, height: u32) -> RasterSurface {
    RasterSurface::new(
        width,
        height,
        vec![0xff; width as usize * height as usize * 4],
    )
    .unwrap()
}

/// Three pages of Helvetica text:
///
/// 0. "The fire door is open" and, lower, "Another FIRE DOOR here"
/// 1. "A wrapped fire" over "door rated sixty", two lines of one paragraph
/// 2. "Fire doors and a fire door"
const PAGES: [&[u8]; 3] = [
    b"BT /F1 20 Tf 50 700 Td (The fire door is open) Tj ET \
      BT /F1 20 Tf 50 500 Td (Another FIRE DOOR here) Tj ET",
    b"BT /F1 12 Tf 50 700 Td (A wrapped fire) Tj ET \
      BT /F1 12 Tf 50 686 Td (door rated sixty) Tj ET",
    b"BT /F1 20 Tf 50 700 Td (Fire doors and a fire door) Tj ET",
];

fn page_layers() -> Vec<PageTextLayer> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let mut kids = Vec::new();
    for content in PAGES {
        let content_id = document.add_object(Stream::new(dictionary! {}, content.to_vec()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        });
        kids.push(page_id.into());
    }
    document.objects.insert(
        pages_id,
        lopdf::Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => PAGES.len() as i64,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    (0..PAGES.len() as u32)
        .map(|page| extract_page_text(&document, page, &TextEditLimits::default()).unwrap())
        .collect()
}

struct FindResource {
    layers: Vec<PageTextLayer>,
    released: AtomicBool,
}

impl NativeDocumentResource for FindResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4400)
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
        self.layers
            .get(page_index as usize)
            .cloned()
            .ok_or_else(|| "no such page".to_owned())
    }

    fn close(&self) -> Result<(), String> {
        self.released.store(true, Ordering::Release);
        Ok(())
    }

    fn is_released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }
}

struct Fixture<'a> {
    workspace: gpui::Entity<DocumentWorkspace>,
    document_id: DocumentId,
    cx: &'a mut VisualTestContext,
}

fn setup(cx: &mut TestAppContext, mode: PageViewMode, with_markup: bool) -> Fixture<'_> {
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
    let resource = Arc::new(FindResource {
        layers: page_layers(),
        released: AtomicBool::new(false),
    });
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("doors.pdf"), cx)
    });
    let document_id = request.document_id;
    workspace.update(cx, |workspace, cx| {
        let mut opened = OpenedNativeDocument::new(
            "doors.pdf",
            vec![(PAGE_WIDTH, PAGE_HEIGHT); PAGES.len()],
            raster(32, 40),
            (0..PAGES.len() as u32)
                .map(|page| ThumbnailSurface::new(page, raster(8, 10)))
                .collect(),
            resource,
        )
        .unwrap();
        if with_markup {
            let text_box = TextBoxAnnotation::new(
                MarkupId::new("pdf:find-text-box").unwrap(),
                1,
                PdfRect::new(300., 400., 200., 40.).unwrap(),
                "Check the fire door rating",
                TextBoxStyle::new("Helvetica", 12., "#111827", 1.).unwrap(),
            )
            .unwrap();
            opened = opened.with_annotations(vec![Annotation::TextBox(text_box)]);
        }
        workspace.apply_open_result(&request, Ok(opened), cx);
        workspace.set_view_configuration(document_id, mode, 100., cx);
        workspace
            .refresh_viewport_async(document_id, 800., 600., 1., cx)
            .unwrap();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    workspace.update(cx, |workspace, cx| {
        workspace.set_fit_preset(
            document_id,
            butter_paper::document_workspace::ViewerFitPreset::Page,
            cx,
        );
    });
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
    // The workspace holds the focus, as after a click on the page.
    cx.update(|window, cx| {
        let focus = workspace.read(cx).focus_handle();
        window.focus(&focus, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    Fixture {
        workspace,
        document_id,
        cx,
    }
}

impl Fixture<'_> {
    fn draw(&mut self) {
        self.cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    /// Draws and re-plans the viewport as the app's next-frame refresh
    /// does, which the test platform does not run, until the view rests.
    fn settle_view(&mut self) {
        let id = self.document_id;
        for _ in 0..12 {
            self.draw();
            if let Some(viewport) = self.cx.debug_bounds(DOCUMENT_VIEWPORT_ID) {
                self.workspace.update(self.cx, |workspace, cx| {
                    let _ = workspace.refresh_viewport_async(
                        id,
                        f32::from(viewport.size.width),
                        f32::from(viewport.size.height),
                        1.,
                        cx,
                    );
                });
            }
            self.cx.run_until_parked();
        }
        self.draw();
    }

    fn summary(&mut self) -> Option<FindBarSummary> {
        let id = self.document_id;
        self.workspace
            .read_with(self.cx, |workspace, cx| workspace.find_bar_summary(id, cx))
    }

    /// Lets the search thread and the timers run until `done` holds.
    fn wait_for(&mut self, what: &str, done: impl Fn(&FindBarSummary) -> bool) -> FindBarSummary {
        for _ in 0..600 {
            self.cx.executor().advance_clock(Duration::from_millis(40));
            self.cx.run_until_parked();
            self.draw();
            if let Some(summary) = self.summary()
                && done(&summary)
            {
                return summary;
            }
            std::thread::sleep(Duration::from_millis(4));
        }
        panic!("waited for {what}, found {:?}", self.summary());
    }

    fn wait_for_done(&mut self) -> FindBarSummary {
        self.wait_for("the search to finish", |summary| {
            summary.open && summary.finished
        })
    }

    fn open_bar(&mut self) {
        self.cx
            .simulate_keystrokes(find_shortcut(cfg!(target_os = "macos")));
        self.draw();
    }

    /// Opens the bar and searches `text`, waiting for its results.
    fn find(&mut self, text: &str) -> FindBarSummary {
        self.open_bar();
        self.type_text(text);
        self.wait_for_done()
    }

    fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            let key = if character == ' ' {
                "space".to_owned()
            } else {
                character.to_string()
            };
            self.cx.simulate_keystrokes(&key);
        }
        self.draw();
    }

    fn count_text(&mut self) -> String {
        self.summary()
            .map(|summary| summary.count)
            .unwrap_or_default()
    }

    fn tool(&mut self) -> Option<AnnotationTool> {
        let id = self.document_id;
        self.workspace
            .read_with(self.cx, |workspace, cx| workspace.annotation_tool(id, cx))
    }

    fn click(&mut self, id: &'static str) {
        let bounds = self.cx.debug_bounds(id).expect("the control is drawn");
        self.cx
            .simulate_click(bounds.center(), Modifiers::default());
        self.draw();
    }
}

#[gpui::test]
fn the_platform_shortcut_opens_the_bar_at_the_top_right_of_the_canvas(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    assert!(f.cx.debug_bounds(DOCUMENT_FIND_BAR_ID).is_none());
    f.open_bar();
    let bar =
        f.cx.debug_bounds(DOCUMENT_FIND_BAR_ID)
            .expect("the bar opens");
    let canvas = f.cx.debug_bounds(DOCUMENT_PAGE_ID).unwrap();
    assert!(
        (bar.top() - canvas.top() - px(8.)).abs() < px(0.5),
        "8 px from the top: {bar:?} in {canvas:?}"
    );
    assert!(
        (canvas.right() - bar.right() - px(8.)).abs() < px(0.5),
        "8 px from the right: {bar:?} in {canvas:?}"
    );
    for id in [
        DOCUMENT_FIND_INPUT_ID,
        DOCUMENT_FIND_COUNT_ID,
        DOCUMENT_FIND_MATCH_CASE_ID,
        DOCUMENT_FIND_WHOLE_WORD_ID,
        DOCUMENT_FIND_PREVIOUS_ID,
        DOCUMENT_FIND_NEXT_ID,
        DOCUMENT_FIND_CLOSE_ID,
    ] {
        let control =
            f.cx.debug_bounds(id)
                .unwrap_or_else(|| panic!("{id} is drawn"));
        assert!(
            control.left() >= bar.left() && control.right() <= bar.right(),
            "{id} is inside the bar"
        );
    }
    let summary = f.summary().unwrap();
    assert!(summary.open && summary.text.is_empty() && summary.count.is_empty());
}

/// The bar holds the app's medium controls, as the toolbar: a 48 px bar
/// (8 + 32 + 8), 32 px field and buttons, the pairs of buttons 4 px apart
/// and the groups 8 px apart.
#[gpui::test]
fn the_bar_uses_medium_controls_on_the_grid(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    f.settle_view();
    f.open_bar();
    let bar = f.cx.debug_bounds(DOCUMENT_FIND_BAR_ID).unwrap();
    // Its border is inside the bar's 48 px.
    assert!((bar.size.height - px(48.)).abs() < px(0.5), "{bar:?}");
    assert!((bar.size.width - px(520.)).abs() < px(0.5), "{bar:?}");
    for id in [
        DOCUMENT_FIND_INPUT_ID,
        DOCUMENT_FIND_MATCH_CASE_ID,
        DOCUMENT_FIND_WHOLE_WORD_ID,
        DOCUMENT_FIND_PREVIOUS_ID,
        DOCUMENT_FIND_NEXT_ID,
        butter_paper::document_workspace::DOCUMENT_FIND_OPEN_IN_PANEL_ID,
    ] {
        let control = f.cx.debug_bounds(id).unwrap();
        assert_eq!(control.size.height, px(32.), "{id} is medium");
        assert!(
            (control.center().y - bar.center().y).abs() < px(0.5),
            "{id} on the bar's centre line"
        );
    }
    let input = f.cx.debug_bounds(DOCUMENT_FIND_INPUT_ID).unwrap();
    assert!(
        (input.left() - bar.left() - px(8.)).abs() < px(1.5),
        "8 px inset"
    );
    // The X is the app's one close size, on the same centre line.
    let close = f.cx.debug_bounds(DOCUMENT_FIND_CLOSE_ID).unwrap();
    assert_eq!(close.size.height, px(24.));
    assert!((close.center().y - bar.center().y).abs() < px(0.5));
    assert!(
        (bar.right() - close.right() - px(8.)).abs() < px(1.5),
        "8 px inset"
    );
    let gap = |a: &'static str, b: &'static str, f: &mut Fixture<'_>| {
        let a = f.cx.debug_bounds(a).unwrap();
        let b = f.cx.debug_bounds(b).unwrap();
        b.left() - a.right()
    };
    assert_eq!(
        gap(
            DOCUMENT_FIND_MATCH_CASE_ID,
            DOCUMENT_FIND_WHOLE_WORD_ID,
            &mut f
        ),
        px(4.)
    );
    assert_eq!(
        gap(DOCUMENT_FIND_PREVIOUS_ID, DOCUMENT_FIND_NEXT_ID, &mut f),
        px(4.)
    );
    assert_eq!(
        gap(DOCUMENT_FIND_OPTIONS_ID, DOCUMENT_FIND_STEPS_ID, &mut f),
        px(8.)
    );
    assert_eq!(
        gap(
            DOCUMENT_FIND_STEPS_ID,
            butter_paper::document_workspace::DOCUMENT_FIND_OPEN_IN_PANEL_ID,
            &mut f
        ),
        px(8.)
    );
    assert_eq!(
        gap(
            butter_paper::document_workspace::DOCUMENT_FIND_OPEN_IN_PANEL_ID,
            DOCUMENT_FIND_CLOSE_ID,
            &mut f
        ),
        px(8.)
    );
}

#[gpui::test]
fn typed_letters_reach_the_field_and_switch_no_tool(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.open_bar();
    // Every letter is a tool shortcut, and none may act.
    f.type_text("vrtec");
    assert_eq!(f.summary().unwrap().text, "vrtec");
    assert_eq!(f.tool(), Some(AnnotationTool::Select));
}

#[gpui::test]
fn the_shortcut_reaches_the_workspace_from_inside_a_text_field(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.open_bar();
    f.type_text("fire");
    f.cx.simulate_keystrokes("left left");
    // The stock field binds the same keys to a search of its own: ours is
    // the one that acts, and it selects the text for the next query.
    f.cx.simulate_keystrokes(find_shortcut(cfg!(target_os = "macos")));
    f.draw();
    f.type_text("door");
    assert_eq!(
        f.summary().unwrap().text,
        "door",
        "the whole text was selected, so typing replaced it"
    );
}

#[gpui::test]
fn typing_finds_as_it_goes_and_counts_the_matches(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.open_bar();
    f.type_text("fire door");
    let summary = f.wait_for_done();
    // Page 0 twice (the second in capitals), page 1 across two lines, and
    // page 2 in "Fire doors" and "fire door".
    assert_eq!(summary.hits, 5);
    assert_eq!(summary.count, "1 of 5");
    assert_eq!(summary.current, Some((0, 1)));
    // A longer query narrows the same search.
    f.type_text("s");
    let summary = f.wait_for("the narrower search", |summary| {
        summary.finished && summary.text == "fire doors" && summary.hits == 1
    });
    assert_eq!(summary.count, "1 of 1");
    assert_eq!(summary.current, Some((2, 1)));
}

#[gpui::test]
fn enter_and_shift_enter_step_and_wrap(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    let summary = f.find("fire door");
    assert_eq!(summary.count, "1 of 5");
    for (expected, page) in [("2 of 5", 0), ("3 of 5", 1), ("4 of 5", 2), ("5 of 5", 2)] {
        f.cx.simulate_keystrokes("enter");
        f.draw();
        let summary = f.summary().unwrap();
        assert_eq!(summary.count, expected);
        assert_eq!(summary.current.map(|current| current.0), Some(page));
    }
    f.cx.simulate_keystrokes("enter");
    assert_eq!(
        f.count_text(),
        "1 of 5",
        "Enter wraps from the last to the first"
    );
    f.cx.simulate_keystrokes("shift-enter");
    assert_eq!(
        f.count_text(),
        "5 of 5",
        "Shift+Enter wraps from the first to the last"
    );
    f.cx.simulate_keystrokes("shift-enter");
    assert_eq!(f.count_text(), "4 of 5");
    // The chevrons do the same.
    f.click(DOCUMENT_FIND_NEXT_ID);
    assert_eq!(f.count_text(), "5 of 5");
    f.click(DOCUMENT_FIND_PREVIOUS_ID);
    assert_eq!(f.count_text(), "4 of 5");
}

#[gpui::test]
fn match_case_and_whole_word_narrow_the_search(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    assert_eq!(f.find("fire door").hits, 5);
    f.click(DOCUMENT_FIND_MATCH_CASE_ID);
    let summary = f.wait_for("Match case", |summary| {
        summary.match_case && summary.finished
    });
    // "The fire door", "fire door" across the lines, and the last of page 2.
    assert_eq!(summary.hits, 3);
    f.click(DOCUMENT_FIND_MATCH_CASE_ID);
    f.wait_for("Match case off", |summary| {
        !summary.match_case && summary.finished && summary.hits == 5
    });
    f.click(DOCUMENT_FIND_WHOLE_WORD_ID);
    let summary = f.wait_for("Whole word", |summary| {
        summary.whole_word && summary.finished
    });
    // "Fire doors" is not the word "door".
    assert_eq!(summary.hits, 4);
    // Alt+C and Alt+W (Cmd+Option on macOS) act with the focus in the bar.
    let (case_keys, word_keys) = if cfg!(target_os = "macos") {
        ("cmd-alt-c", "cmd-alt-w")
    } else {
        ("alt-c", "alt-w")
    };
    f.click(DOCUMENT_FIND_INPUT_ID);
    f.cx.simulate_keystrokes(case_keys);
    f.wait_for("Match case by key", |summary| {
        summary.match_case && summary.finished
    });
    f.cx.simulate_keystrokes(word_keys);
    let summary = f.wait_for("Whole word off by key", |summary| {
        !summary.whole_word && summary.finished
    });
    assert_eq!(summary.hits, 3);
}

#[gpui::test]
fn the_count_says_when_nothing_is_found(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    let summary = f.find("zzzz");
    assert_eq!(summary.hits, 0);
    assert_eq!(summary.count, "No results");
    assert_eq!(summary.current, None);
    // An empty field shows no count.
    f.cx.simulate_keystrokes("backspace backspace backspace backspace");
    let summary = f.wait_for("the empty query", |summary| summary.text.is_empty());
    assert_eq!(summary.count, "");
}

#[gpui::test]
fn the_count_says_searching_until_the_first_match(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.open_bar();
    f.type_text("fire");
    // The search starts after the query rests, so before that no result
    // exists yet.
    let summary = f.summary().unwrap();
    assert_eq!(summary.hits, 0);
    assert_eq!(summary.count, "Searching…");
}

#[gpui::test]
fn escape_closes_the_bar_clears_the_highlights_and_returns_the_focus(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.find("fire door");
    let document_id = f.document_id;
    assert!(
        !f.workspace
            .read_with(f.cx, |workspace, cx| workspace
                .find_highlights(document_id, cx))
            .is_empty()
    );
    f.cx.simulate_keystrokes("escape");
    f.draw();
    assert!(f.cx.debug_bounds(DOCUMENT_FIND_BAR_ID).is_none());
    assert!(!f.summary().unwrap().open);
    assert!(
        f.workspace
            .read_with(f.cx, |workspace, cx| workspace
                .find_highlights(document_id, cx))
            .is_empty(),
        "closing clears every highlight"
    );
    let focused = f.cx.update(|window, cx| {
        f.workspace
            .read(cx)
            .focus_handle()
            .contains_focused(window, cx)
    });
    assert!(focused, "the document has the focus again");
    // The tool is untouched.
    assert_eq!(f.tool(), Some(AnnotationTool::Select));
    // Opened again, the field keeps the text, selected, and searches it.
    f.open_bar();
    f.wait_for_done();
    assert_eq!(f.summary().unwrap().text, "fire door");
}

#[gpui::test]
fn the_x_closes_the_bar(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.find("fire");
    f.click(DOCUMENT_FIND_CLOSE_ID);
    assert!(f.cx.debug_bounds(DOCUMENT_FIND_BAR_ID).is_none());
    assert!(!f.summary().unwrap().open);
}

#[gpui::test]
fn the_current_match_is_drawn_stronger_than_the_others(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.find("fire door");
    let document_id = f.document_id;
    let highlights = f.workspace.read_with(f.cx, |workspace, cx| {
        workspace.find_highlights(document_id, cx)
    });
    let page0 = highlights
        .iter()
        .find(|page| page.page_index == 0)
        .expect("page 0 is in view");
    // The first match is the current one: "fire door" on the first line, at
    // 20 pt Helvetica after "The " from x = 50.
    assert_eq!(page0.current.len(), 1);
    let quad = page0.current[0];
    let (left, right) = (quad[0].0, quad[1].0);
    let em = 20.;
    assert!(
        (left - (50. + 2.001 * em)).abs() < 1.5,
        "the match starts after \"The \": {left}"
    );
    assert!(
        (right - left - 3.668 * em).abs() < 2.,
        "the match is as wide as \"fire door\": {}",
        right - left
    );
    assert!(
        quad[0].1 < 700. && quad[2].1 > 700.,
        "the box holds the baseline: {quad:?}"
    );
    // The other match on that page is a plain match.
    assert_eq!(page0.matches.len(), 1);
    assert!(page0.matches[0][0].1 < 500. && page0.matches[0][2].1 > 500.);
}

#[gpui::test]
fn a_phrase_found_across_two_lines_highlights_both(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.find("fire door");
    // Step to the third, on page 1.
    f.cx.simulate_keystrokes("enter enter");
    f.draw();
    let document_id = f.document_id;
    let highlights = f.workspace.read_with(f.cx, |workspace, cx| {
        workspace.find_highlights(document_id, cx)
    });
    let page1 = highlights
        .iter()
        .find(|page| page.page_index == 1)
        .expect("page 1 is highlighted");
    assert_eq!(page1.current.len(), 2, "one box on each line");
}

#[gpui::test]
fn stepping_to_a_match_off_screen_scrolls_it_into_view_in_continuous_view(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.find("fire door");
    let before = f.workspace.read_with(f.cx, |workspace, cx| {
        workspace
            .document_view_state(f.document_id, cx)
            .unwrap()
            .scroll()
    });
    // To the last match, on page 2, far below the first screen.
    f.cx.simulate_keystrokes("shift-enter");
    f.settle_view();
    let after = f.workspace.read_with(f.cx, |workspace, cx| {
        workspace
            .document_view_state(f.document_id, cx)
            .unwrap()
            .scroll()
    });
    assert!(
        after.1 > before.1,
        "the pages scrolled down: {before:?} to {after:?}"
    );
    let viewport = f.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap();
    let layer_id: &'static str =
        Box::leak(document_annotation_layer_id(f.document_id, 2).into_boxed_str());
    let layer = f.cx.debug_bounds(layer_id).expect("page 2 is drawn");
    // The match on page 2 is at y = 700 of 792, on a page drawn at `scale`.
    let scale =
        (f32::from(layer.size.width) / PAGE_WIDTH).min(f32::from(layer.size.height) / PAGE_HEIGHT);
    let top = layer.origin.y + (layer.size.height - px(PAGE_HEIGHT * scale)) / 2.;
    let match_y = top + px((PAGE_HEIGHT - 700.) * scale);
    let bar = f.cx.debug_bounds(DOCUMENT_FIND_BAR_ID).unwrap();
    assert!(
        match_y > bar.bottom() && match_y < viewport.bottom(),
        "the match at {match_y:?} is in the viewport {viewport:?}, below the bar {bar:?}"
    );
}

#[gpui::test]
fn a_match_scrolled_in_from_above_stops_below_the_bar(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.find("fire door");
    f.settle_view();
    let id = f.document_id;
    // Scroll the first match (page 0, first line) out above the view.
    let scroll = f.workspace.read_with(f.cx, |workspace, cx| {
        workspace.document_view_state(id, cx).unwrap().scroll()
    });
    f.workspace.update(f.cx, |workspace, cx| {
        workspace.set_viewport_scroll(id, scroll.0, scroll.1 + 200., cx);
    });
    f.settle_view();
    let viewport = f.cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap();
    let moved = f.workspace.read_with(f.cx, |workspace, cx| {
        workspace.document_view_state(id, cx).unwrap().scroll()
    });
    assert!(
        moved.1 > scroll.1 + 100.,
        "the view scrolled down: {scroll:?} to {moved:?}"
    );
    // Away and back: the first match is revealed again, under the bar's
    // lower edge and not behind it.
    f.cx.simulate_keystrokes("shift-enter");
    f.settle_view();
    f.cx.simulate_keystrokes("enter");
    f.settle_view();
    let bar = f.cx.debug_bounds(DOCUMENT_FIND_BAR_ID).unwrap();
    let shown = page_point(&mut f, 0, 100., 706.);
    assert_eq!(f.summary().unwrap().current, Some((0, 1)));
    assert!(
        shown.y > bar.bottom() && shown.y < viewport.bottom(),
        "the match at {shown:?} shows below the bar {bar:?} in {viewport:?}"
    );
}

#[gpui::test]
fn stepping_to_a_match_on_another_page_turns_to_it_in_single_page_view(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::SinglePage, false);
    f.find("fire door");
    let page = |f: &mut Fixture<'_>| {
        let id = f.document_id;
        f.workspace.read_with(f.cx, |workspace, cx| {
            workspace.session(id, cx).unwrap().read(cx).current_page()
        })
    };
    assert_eq!(page(&mut f), 0);
    f.cx.simulate_keystrokes("enter enter");
    f.settle_view();
    assert_eq!(page(&mut f), 1, "the third match is on page 1");
    f.cx.simulate_keystrokes("enter");
    f.settle_view();
    assert_eq!(page(&mut f), 2);
}

#[gpui::test]
fn markups_are_found_and_outlined(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, true);
    let summary = f.find("fire door");
    // The five in page text, and the Text Box on page 1 after the text of
    // its page.
    assert_eq!(summary.hits, 6);
    f.cx.simulate_keystrokes("enter enter enter");
    f.draw();
    let summary = f.summary().unwrap();
    assert_eq!(summary.current, Some((1, 4)));
    let document_id = f.document_id;
    let highlights = f.workspace.read_with(f.cx, |workspace, cx| {
        workspace.find_highlights(document_id, cx)
    });
    let page1 = highlights.iter().find(|page| page.page_index == 1).unwrap();
    assert_eq!(
        page1.outline_current.len(),
        1,
        "an outline round the markup"
    );
    assert!(page1.current.is_empty(), "no text match is current");
    // Finding a markup does not select it, so typing never edits it.
    let selected = f.workspace.read_with(f.cx, |workspace, cx| {
        workspace
            .annotation_snapshot(document_id, cx)
            .and_then(|snapshot| snapshot.selected_id)
    });
    assert_eq!(selected, None);
}

#[gpui::test]
fn a_press_on_the_bar_starts_no_gesture_on_the_page(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    // A window in which the bar is over the page.
    f.cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    f.settle_view();
    f.find("fire");
    f.settle_view();
    let id = f.document_id;
    // The bar's left padding, where no control is, over the page.
    let bar = f.cx.debug_bounds(DOCUMENT_FIND_BAR_ID).unwrap();
    let at = point(bar.left() + px(2.), bar.center().y);
    let to = point(at.x - px(30.), at.y + px(40.));
    let layer_id: &'static str = Box::leak(document_annotation_layer_id(id, 0).into_boxed_str());
    let page = f.cx.debug_bounds(layer_id).unwrap();
    assert!(
        page.contains(&at) && page.contains(&to),
        "the drag is over the page: {page:?} {at:?} {to:?}"
    );
    let drag = |f: &mut Fixture<'_>| {
        // The page has the focus, as after a click on it, and a drawing
        // tool is armed.
        f.cx.update(|window, cx| {
            let focus = f.workspace.read(cx).focus_handle();
            window.focus(&focus, cx);
        });
        f.workspace.update(f.cx, |workspace, cx| {
            workspace
                .set_annotation_tool(id, AnnotationTool::Rectangle, cx)
                .unwrap();
        });
        f.draw();
        // Two clicks, as a rectangle is drawn: its first corner, then its
        // opposite one.
        f.cx.simulate_click(at, Modifiers::default());
        f.cx.simulate_mouse_move(to, None, Modifiers::default());
        f.cx.simulate_click(to, Modifiers::default());
        f.draw();
        f.workspace
            .read_with(f.cx, |workspace, cx| workspace.annotation_snapshot(id, cx))
            .unwrap()
            .rectangles
            .len()
    };
    assert_eq!(drag(&mut f), 0, "the press on the bar drew nothing");
    assert!(f.summary().unwrap().open, "the bar stays open");
    // The same drag with the bar closed draws a rectangle, so the point
    // is on the page and the tool does draw there.
    f.click(DOCUMENT_FIND_CLOSE_ID);
    assert_eq!(drag(&mut f), 1, "the control: the page takes the drag");
}

/// The window point of page point (`x`, `y`) of page `page`.
fn page_point(f: &mut Fixture<'_>, page: u32, x: f32, y: f32) -> gpui::Point<gpui::Pixels> {
    let layer_id: &'static str =
        Box::leak(document_annotation_layer_id(f.document_id, page).into_boxed_str());
    let layer = f.cx.debug_bounds(layer_id).expect("the page is drawn");
    let scale =
        (f32::from(layer.size.width) / PAGE_WIDTH).min(f32::from(layer.size.height) / PAGE_HEIGHT);
    let origin = point(
        layer.origin.x + (layer.size.width - px(PAGE_WIDTH * scale)) / 2.,
        layer.origin.y + (layer.size.height - px(PAGE_HEIGHT * scale)) / 2.,
    );
    point(
        origin.x + px(x * scale),
        origin.y + px((PAGE_HEIGHT - y) * scale),
    )
}

#[gpui::test]
fn selected_page_text_seeds_the_query_and_replaces_the_old_one(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    // The words "fire door" of the first line, selected by dragging over
    // them: from just before "fire" to just after "door".
    let from = page_point(&mut f, 0, 90., 706.);
    let to = page_point(&mut f, 0, 163., 706.);
    f.cx.simulate_mouse_move(from, None, Modifiers::default());
    f.cx.simulate_event(gpui::MouseDownEvent {
        button: gpui::MouseButton::Left,
        position: from,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    f.cx.simulate_mouse_move(to, Some(gpui::MouseButton::Left), Modifiers::default());
    f.cx.simulate_event(gpui::MouseUpEvent {
        button: gpui::MouseButton::Left,
        position: to,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    f.draw();
    let id = f.document_id;
    let selected = f
        .workspace
        .read_with(f.cx, |workspace, cx| workspace.selected_page_text(id, cx))
        .expect("dragging over text selects it");
    assert!(selected.contains("fire"), "selected {selected:?}");
    f.open_bar();
    let summary = f.wait_for_done();
    assert_eq!(summary.text, selected.lines().next().unwrap().trim());
    assert!(summary.hits > 0);
    // With a field already holding a query, the selection replaces it.
    f.cx.simulate_keystrokes("escape");
    f.draw();
    f.open_bar();
    f.type_text("zzz");
    f.wait_for_done();
    f.cx.update(|window, cx| {
        let focus = f.workspace.read(cx).focus_handle();
        window.focus(&focus, cx);
    });
    f.open_bar();
    assert_eq!(
        f.summary().unwrap().text,
        selected.lines().next().unwrap().trim()
    );
}

#[gpui::test]
fn f3_and_shift_f3_step_with_the_focus_in_the_document(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.find("fire door");
    f.cx.update(|window, cx| {
        let focus = f.workspace.read(cx).focus_handle();
        window.focus(&focus, cx);
    });
    let (next, previous) = if cfg!(target_os = "macos") {
        ("cmd-g", "cmd-shift-g")
    } else {
        ("f3", "shift-f3")
    };
    f.cx.simulate_keystrokes(next);
    assert_eq!(f.count_text(), "2 of 5");
    f.cx.simulate_keystrokes(previous);
    f.cx.simulate_keystrokes(previous);
    assert_eq!(f.count_text(), "5 of 5", "the previous match wraps");
}

#[gpui::test]
fn an_edit_makes_the_search_run_again_once_it_pauses(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    assert_eq!(f.find("fire door").hits, 5);
    let id = f.document_id;
    let text_box = TextBoxAnnotation::new(
        MarkupId::new("pdf:added-text-box").unwrap(),
        0,
        PdfRect::new(300., 300., 200., 40.).unwrap(),
        "Fire door added later",
        TextBoxStyle::new("Helvetica", 12., "#111827", 1.).unwrap(),
    )
    .unwrap();
    f.workspace
        .update(f.cx, |workspace, cx| {
            workspace.create_text_box(id, text_box, cx)
        })
        .unwrap();
    let summary = f.wait_for("the search to run again", |summary| {
        summary.finished && summary.hits == 6
    });
    assert!(summary.count.ends_with("of 6"), "{}", summary.count);
}

fn added_text_box(name: &str) -> TextBoxAnnotation {
    TextBoxAnnotation::new(
        MarkupId::new(name).unwrap(),
        0,
        PdfRect::new(300., 300., 200., 40.).unwrap(),
        "Fire door added later",
        TextBoxStyle::new("Helvetica", 12., "#111827", 1.).unwrap(),
    )
    .unwrap()
}

/// Lets the clock pass `millis` with no search to wait for.
fn pass(f: &mut Fixture<'_>, millis: u64) {
    for _ in 0..millis.div_ceil(40) {
        f.cx.executor().advance_clock(Duration::from_millis(40));
        f.cx.run_until_parked();
        f.draw();
    }
}

/// An edit undone within the wait changes nothing, and a later edit still
/// makes the search run again: the wait does not stay on.
#[gpui::test]
fn after_an_undone_edit_a_later_edit_still_searches_again(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    assert_eq!(f.find("fire door").hits, 5);
    let id = f.document_id;
    f.workspace
        .update(f.cx, |workspace, cx| {
            workspace.create_text_box(id, added_text_box("pdf:undone-text-box"), cx)
        })
        .unwrap();
    f.draw();
    f.workspace
        .update(f.cx, |workspace, cx| workspace.undo_annotations(id, cx))
        .unwrap();
    f.draw();
    pass(&mut f, 800);
    assert_eq!(f.summary().unwrap().hits, 5);
    f.workspace
        .update(f.cx, |workspace, cx| {
            workspace.create_text_box(id, added_text_box("pdf:added-text-box"), cx)
        })
        .unwrap();
    f.wait_for("the search to run again", |summary| {
        summary.finished && summary.hits == 6
    });
}

/// A letter typed and deleted within the wait changes nothing, and a later
/// edit still makes the search run again.
#[gpui::test]
fn after_a_typed_and_deleted_letter_an_edit_still_searches_again(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    assert_eq!(f.find("fire door").hits, 5);
    f.type_text("x");
    f.cx.simulate_keystrokes("backspace");
    f.draw();
    pass(&mut f, 400);
    assert_eq!(f.summary().unwrap().hits, 5);
    let id = f.document_id;
    f.workspace
        .update(f.cx, |workspace, cx| {
            workspace.create_text_box(id, added_text_box("pdf:added-text-box"), cx)
        })
        .unwrap();
    f.wait_for("the search to run again", |summary| {
        summary.finished && summary.hits == 6
    });
}

/// The toolbar's Undo acts on the document even while the find field has
/// the focus (owner decision), and is enabled from the document's history.
#[gpui::test]
fn toolbar_undo_undoes_the_markup_while_the_find_field_has_the_focus(cx: &mut TestAppContext) {
    use butter_paper::viewer_toolbar_strip::{HistoryState, VIEWER_UNDO_ID};
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.open_bar();
    let history = |f: &mut Fixture<'_>| {
        f.draw();
        f.workspace.read_with(f.cx, |workspace, cx| {
            workspace.viewer_toolbar().read(cx).history()
        })
    };
    assert!(f.cx.update(|window, cx| window.has_focused_input(cx)));
    assert_eq!(
        history(&mut f),
        HistoryState::default(),
        "no document history yet"
    );
    let id = f.document_id;
    f.workspace
        .update(f.cx, |workspace, cx| {
            workspace.create_text_box(id, added_text_box("pdf:toolbar-undo"), cx)
        })
        .unwrap();
    f.open_bar();
    f.type_text("door");
    assert!(f.cx.update(|window, cx| window.has_focused_input(cx)));
    assert_eq!(
        history(&mut f),
        HistoryState {
            can_undo: true,
            can_redo: false
        }
    );
    let text_boxes = |f: &mut Fixture<'_>| {
        f.workspace
            .read_with(f.cx, |workspace, cx| workspace.annotation_snapshot(id, cx))
            .unwrap()
            .text_boxes
            .len()
    };
    assert_eq!(text_boxes(&mut f), 1);
    let undo = f.cx.debug_bounds(VIEWER_UNDO_ID).unwrap();
    f.cx.simulate_click(undo.center(), Modifiers::default());
    f.draw();
    assert_eq!(
        text_boxes(&mut f),
        0,
        "the toolbar's Undo undid the Text Box"
    );
    assert_eq!(f.summary().unwrap().text, "door", "the field kept its text");
}

#[gpui::test]
fn results_reach_the_bar_on_its_timer_only(cx: &mut TestAppContext) {
    // The search thread ends on its own time; the bar takes its results
    // only when the window's clock moves, so a test sees no race.
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.open_bar();
    f.type_text("fire door");
    // The query rests (150 ms), and the search starts; the bar's first
    // look at its results is 50 ms later.
    f.cx.executor().advance_clock(Duration::from_millis(150));
    f.cx.run_until_parked();
    // Long enough for the search thread to finish its pages.
    std::thread::sleep(Duration::from_millis(300));
    f.cx.run_until_parked();
    let before = f.summary().unwrap();
    assert!(
        !before.finished,
        "nothing arrived while the clock stood: {before:?}"
    );
    assert_eq!(before.hits, 0);
    f.cx.executor().advance_clock(Duration::from_millis(60));
    f.cx.run_until_parked();
    let after = f.summary().unwrap();
    assert!(after.finished, "one tick delivers all that arrived");
    assert_eq!(after.hits, 5);
}

#[gpui::test]
fn a_closed_document_takes_its_bar_with_it(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.find("fire");
    let id = f.document_id;
    assert!(
        f.workspace
            .update(f.cx, |workspace, cx| workspace.close_document(id, cx))
    );
    f.draw();
    assert!(f.cx.debug_bounds(DOCUMENT_FIND_BAR_ID).is_none());
    assert_eq!(f.summary(), None);
}

#[gpui::test]
fn the_bar_fits_the_narrowest_canvas(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, false);
    f.cx.simulate_resize(gpui::size(px(560.), px(600.)));
    f.open_bar();
    f.draw();
    f.draw();
    let bar = f.cx.debug_bounds(DOCUMENT_FIND_BAR_ID).unwrap();
    let canvas = f.cx.debug_bounds(DOCUMENT_PAGE_ID).unwrap();
    assert!(
        bar.left() >= canvas.left() && bar.right() <= canvas.right(),
        "the bar {bar:?} is inside the canvas {canvas:?}"
    );
    for id in [
        DOCUMENT_FIND_INPUT_ID,
        DOCUMENT_FIND_CLOSE_ID,
        DOCUMENT_FIND_NEXT_ID,
    ] {
        let control = f.cx.debug_bounds(id).unwrap();
        assert!(
            control.right() <= bar.right() + px(0.5),
            "{id} is inside the bar"
        );
    }
}

#[gpui::test]
fn escape_on_the_page_clears_a_selection_or_a_tool_before_it_closes_the_bar(
    cx: &mut TestAppContext,
) {
    let mut f = setup(cx, PageViewMode::Continuous, true);
    f.find("fire door");
    let id = f.document_id;
    let focus_page = |f: &mut Fixture<'_>| {
        f.cx.update(|window, cx| {
            let focus = f.workspace.read(cx).focus_handle();
            window.focus(&focus, cx);
        });
        f.draw();
    };
    let selected = |f: &mut Fixture<'_>| {
        f.workspace.read_with(f.cx, |workspace, cx| {
            workspace
                .annotation_snapshot(id, cx)
                .and_then(|snapshot| snapshot.selected_id)
        })
    };
    // A markup is selected and the page has the focus: Escape clears the
    // selection and leaves the bar open.
    focus_page(&mut f);
    f.workspace.update(f.cx, |workspace, cx| {
        assert!(workspace.select_annotation(id, &MarkupId::new("pdf:find-text-box").unwrap(), cx));
    });
    assert!(selected(&mut f).is_some());
    f.cx.simulate_keystrokes("escape");
    f.draw();
    assert_eq!(selected(&mut f), None, "the selection goes first");
    assert!(f.summary().unwrap().open, "the bar stays open");
    // A drawing tool is armed: Escape returns to Select, the bar stays.
    f.workspace.update(f.cx, |workspace, cx| {
        workspace
            .set_annotation_tool(id, AnnotationTool::Rectangle, cx)
            .unwrap();
    });
    f.cx.simulate_keystrokes("escape");
    f.draw();
    assert_eq!(f.tool(), Some(AnnotationTool::Select));
    assert!(f.summary().unwrap().open, "the bar stays open");
    // Nothing else is left: Escape closes the bar.
    f.cx.simulate_keystrokes("escape");
    f.draw();
    assert!(!f.summary().unwrap().open, "then the bar closes");
    assert!(f.cx.debug_bounds(DOCUMENT_FIND_BAR_ID).is_none());
}

#[gpui::test]
fn escape_in_the_bar_closes_it_whatever_the_page_holds(cx: &mut TestAppContext) {
    let mut f = setup(cx, PageViewMode::Continuous, true);
    let id = f.document_id;
    f.workspace.update(f.cx, |workspace, cx| {
        assert!(workspace.select_annotation(id, &MarkupId::new("pdf:find-text-box").unwrap(), cx));
    });
    // The bar opens with the focus in its field.
    f.find("fire");
    f.cx.simulate_keystrokes("escape");
    f.draw();
    assert!(!f.summary().unwrap().open);
}
