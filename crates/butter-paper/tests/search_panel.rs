//! The search panel in the workspace, driven by the same key bindings and
//! pointer events as the live app: Cmd+Shift+F or Ctrl+Shift+F and the rail
//! button open it in place of Page Thumbnails, typing searches every open
//! document (or the one in view), results group by document and page, and a
//! click or Enter goes to a result, in another tab or window when it is
//! there. The search reads page text through a stand-in worker, so these
//! tests run with no PDFium; the real worker is covered by `search_engine`.

use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use butter_paper::annotation_adapter::AnnotationTool;
use butter_paper::annotation_model::{
    Annotation, MarkupId, PdfRect, TextBoxAnnotation, TextBoxStyle,
};
use butter_paper::document_workspace::{
    DOCUMENT_LEFT_RAIL_SEARCH_ID, DOCUMENT_SEARCH_CLOSE_ID, DOCUMENT_SEARCH_COLLAPSE_ID,
    DOCUMENT_SEARCH_CONTROLS_ID, DOCUMENT_SEARCH_EXPLAIN_ID, DOCUMENT_SEARCH_HEADER_ID,
    DOCUMENT_SEARCH_INPUT_ID, DOCUMENT_SEARCH_MATCH_CASE_ID, DOCUMENT_SEARCH_MODE_TEXT_ID,
    DOCUMENT_SEARCH_MODE_VISUAL_ID, DOCUMENT_SEARCH_MORE_CONTEXT_ID, DOCUMENT_SEARCH_PANEL_ID,
    DOCUMENT_SEARCH_SCOPE_ID, DOCUMENT_SEARCH_SORT_ID, DocumentId, DocumentWorkspace, LeftPanel,
    NativeDocumentResource, OpenedNativeDocument, RasterSurface, SearchPanelRow,
    SearchPanelSummary, ThumbnailSurface, init_document_workspace_actions, search_panel_row_id,
    search_panel_shortcut,
};
use butter_paper::page_view_control::PageViewMode;
use butter_paper::pdf_text_edit::{PageTextLayer, TextEditLimits, extract_page_text};
use butter_paper::search_markups::MarkupMetadata;
use butter_paper::search_run::SearchSort;
use butter_paper::search_scope::{SearchScope, set_workspace_directory};
use gpui::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, TestAppContext, VisualTestContext,
};
use gpui_component::Root;
use lopdf::{Document, Stream, dictionary};

const PAGE_WIDTH: f32 = 612.;
const PAGE_HEIGHT: f32 = 792.;

/// Three pages of text, as the find bar tests have.
const DOORS: [&[u8]; 3] = [
    b"BT /F1 20 Tf 50 700 Td (The fire door is open) Tj ET \
      BT /F1 20 Tf 50 500 Td (Another FIRE DOOR here) Tj ET",
    b"BT /F1 12 Tf 50 700 Td (A wrapped fire) Tj ET \
      BT /F1 12 Tf 50 686 Td (door rated sixty) Tj ET",
    b"BT /F1 20 Tf 50 700 Td (Fire doors and a fire door) Tj ET",
];

/// Two pages: one with text, one with none (a scan).
const PLAN: [&[u8]; 2] = [
    b"BT /F1 20 Tf 50 700 Td (Fire exit door to stair 2) Tj ET",
    b"",
];

fn raster(width: u32, height: u32) -> RasterSurface {
    RasterSurface::new(
        width,
        height,
        vec![0xff; width as usize * height as usize * 4],
    )
    .unwrap()
}

fn page_layers(pages: &[&[u8]]) -> Vec<PageTextLayer> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let mut kids = Vec::new();
    for content in pages {
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
            "Count" => pages.len() as i64,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    (0..pages.len() as u32)
        .map(|page| extract_page_text(&document, page, &TextEditLimits::default()).unwrap())
        .collect()
}

struct StandInResource {
    layers: Vec<PageTextLayer>,
    released: AtomicBool,
}

impl NativeDocumentResource for StandInResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4401)
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

/// A Text Box on page 0 of the plan, by Alex, with a reply by Sam.
fn plan_markups(opened: OpenedNativeDocument) -> OpenedNativeDocument {
    let text_box = TextBoxAnnotation::new(
        MarkupId::new("pdf:plan-note").unwrap(),
        0,
        PdfRect::new(300., 400., 200., 40.).unwrap(),
        "Check the fire door rating",
        TextBoxStyle::new("Helvetica", 12., "#111827", 1.).unwrap(),
    )
    .unwrap();
    opened
        .with_annotations(vec![Annotation::TextBox(text_box)])
        .with_markup_metadata(vec![
            MarkupMetadata {
                name: "pdf:plan-note".into(),
                object: Some((10, 0)),
                page_index: 0,
                subtype: "FreeText".into(),
                subject: Some("Text Box".into()),
                author: Some("Alex".into()),
                contents: Some("Check the fire door rating".into()),
                modified: Some("D:20261101120000".into()),
                rect: Some([300., 400., 500., 440.]),
                in_model: true,
                ..Default::default()
            },
            MarkupMetadata {
                name: "pdf:plan-reply".into(),
                object: Some((11, 0)),
                page_index: 0,
                subtype: "Text".into(),
                author: Some("Sam".into()),
                contents: Some("Agreed, 60 min".into()),
                in_reply_to: Some((10, 0)),
                ..Default::default()
            },
        ])
}

fn open_document(
    workspace: &Entity<DocumentWorkspace>,
    cx: &mut VisualTestContext,
    name: &str,
    pages: &[&[u8]],
    with_markups: bool,
) -> DocumentId {
    let resource = Arc::new(StandInResource {
        layers: page_layers(pages),
        released: AtomicBool::new(false),
    });
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from(name), cx)
    });
    let document_id = request.document_id;
    workspace.update(cx, |workspace, cx| {
        let mut opened = OpenedNativeDocument::new(
            name,
            vec![(PAGE_WIDTH, PAGE_HEIGHT); pages.len()],
            raster(32, 40),
            (0..pages.len() as u32)
                .map(|page| ThumbnailSurface::new(page, raster(8, 10)))
                .collect(),
            resource,
        )
        .unwrap();
        if with_markups {
            opened = plan_markups(opened);
        }
        workspace.apply_open_result(&request, Ok(opened), cx);
        workspace.set_view_configuration(document_id, PageViewMode::Continuous, 100., cx);
        workspace
            .refresh_viewport_async(document_id, 800., 600., 1., cx)
            .unwrap();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    document_id
}

struct Fixture<'a> {
    workspace: Entity<DocumentWorkspace>,
    doors: DocumentId,
    plan: DocumentId,
    ids: Arc<AtomicU64>,
    cx: &'a mut VisualTestContext,
}

/// One window with two tabs: doors.pdf, then plan.pdf (active).
fn setup(cx: &mut TestAppContext) -> Fixture<'_> {
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
    let ids = Arc::new(AtomicU64::new(1));
    workspace.update(cx, |workspace, _| workspace.share_document_ids(ids.clone()));
    let doors = open_document(&workspace, cx, "doors.pdf", &DOORS, false);
    let plan = open_document(&workspace, cx, "plan.pdf", &PLAN, true);
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
        doors,
        plan,
        ids,
        cx,
    }
}

impl Fixture<'_> {
    fn draw(&mut self) {
        self.cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn summary(&mut self) -> Option<SearchPanelSummary> {
        self.workspace
            .read_with(self.cx, |workspace, cx| workspace.search_panel_summary(cx))
    }

    fn wait_for(
        &mut self,
        what: &str,
        done: impl Fn(&SearchPanelSummary) -> bool,
    ) -> SearchPanelSummary {
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

    fn open_panel(&mut self) {
        self.cx
            .simulate_keystrokes(search_panel_shortcut(cfg!(target_os = "macos")));
        self.draw();
    }

    /// Types letters, digits and spaces as key presses, through the app's
    /// key bindings, and other characters as text input.
    fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            if character == ' ' {
                self.cx.simulate_keystrokes("space");
            } else if character.is_ascii_alphanumeric() {
                self.cx.simulate_keystrokes(&character.to_string());
            } else {
                self.cx.simulate_input(&character.to_string());
            }
        }
        self.draw();
    }

    /// Select all in the field with the platform's own key, then delete. On
    /// macOS, Ctrl+A moves to the line start and drops the selection, so
    /// the two keys must not follow each other.
    fn clear_query(&mut self) {
        let select_all = if cfg!(target_os = "macos") {
            "cmd-a"
        } else {
            "ctrl-a"
        };
        self.cx.simulate_keystrokes(select_all);
        self.cx.simulate_keystrokes("backspace");
        self.draw();
    }

    /// Searches the document in view, the panel's first scope.
    fn search(&mut self, text: &str) -> SearchPanelSummary {
        self.open_panel();
        self.type_text(text);
        self.wait_for("the search", |summary| {
            summary.finished && summary.searched == text && !summary.summary.is_empty()
        })
    }

    /// Searches every open document: All open documents, chosen in the
    /// scope menu, then the query.
    fn search_all(&mut self, text: &str) -> SearchPanelSummary {
        self.open_panel();
        self.choose_scope(SearchScope::AllOpenDocuments);
        self.search(text)
    }

    /// Chooses a scope in the stock Select by keyboard, as a person does:
    /// it opens on the current scope, and the arrows move to the other.
    fn choose_scope(&mut self, scope: SearchScope) {
        let current = self.summary().map(|summary| summary.scope).unwrap();
        if current == scope {
            return;
        }
        self.click(DOCUMENT_SEARCH_SCOPE_ID);
        let from = SearchScope::ALL.iter().position(|other| *other == current);
        let to = SearchScope::ALL.iter().position(|other| *other == scope);
        let step = if to > from { "down" } else { "up" };
        self.cx.simulate_keystrokes(&format!("{step} enter"));
        self.draw();
        assert_eq!(self.summary().unwrap().scope, scope);
    }

    fn click(&mut self, id: &'static str) {
        let bounds = self.cx.debug_bounds(id).expect("the control is drawn");
        self.cx
            .simulate_click(bounds.center(), Modifiers::default());
        self.draw();
    }

    fn click_row(&mut self, index: usize) {
        let id: &'static str = Box::leak(search_panel_row_id(index).into_boxed_str());
        self.click(id);
    }

    fn active(&mut self) -> Option<DocumentId> {
        self.workspace
            .read_with(self.cx, |workspace, _| workspace.active_document_id())
    }

    fn left_panel(&mut self) -> Option<LeftPanel> {
        self.workspace
            .read_with(self.cx, |workspace, _| workspace.left_panel())
    }

    fn focus_in_results(&mut self) -> bool {
        let workspace = self.workspace.clone();
        self.cx
            .update(|window, cx| workspace.read(cx).search_results_focused(window))
    }
}

fn row_index(summary: &SearchPanelSummary, find: impl Fn(&SearchPanelRow) -> bool) -> usize {
    summary
        .rows
        .iter()
        .position(find)
        .unwrap_or_else(|| panic!("no such row in {:?}", summary.rows))
}

fn document_rows(summary: &SearchPanelSummary) -> Vec<(String, usize)> {
    summary
        .rows
        .iter()
        .filter_map(|row| match row {
            SearchPanelRow::Document { name, count, .. } => Some((name.clone(), *count)),
            _ => None,
        })
        .collect()
}

#[gpui::test]
fn the_shortcut_opens_search_in_place_of_thumbnails_at_the_same_width(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    f.click("document-left-rail-pages");
    assert_eq!(f.left_panel(), Some(LeftPanel::Thumbnails));
    let thumbnails = f.cx.debug_bounds("document-left-sidebar").unwrap();
    f.open_panel();
    assert_eq!(f.left_panel(), Some(LeftPanel::Search));
    assert!(f.cx.debug_bounds("document-left-sidebar").is_none());
    let panel = f.cx.debug_bounds(DOCUMENT_SEARCH_PANEL_ID).unwrap();
    assert_eq!(panel.left(), thumbnails.left());
    assert!(
        (panel.size.width - thumbnails.size.width).abs() < gpui::px(1.),
        "one width for both: {panel:?} {thumbnails:?}"
    );
    // The focus is in the field: typed letters reach it and switch no tool.
    f.type_text("vrtec");
    assert_eq!(f.summary().unwrap().text, "vrtec");
    let plan = f.plan;
    let tool = f
        .workspace
        .read_with(f.cx, |workspace, cx| workspace.annotation_tool(plan, cx));
    assert_eq!(tool, Some(AnnotationTool::Select));
}

/// The sidebar yields to properties in a narrow window. The shortcut then
/// keeps the keyboard on the page: a field that is not drawn must not take
/// it, or typed keys reach nothing. The panel shows when properties close.
#[gpui::test]
fn the_shortcut_leaves_the_keyboard_on_the_page_while_the_sidebar_yields(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    f.cx.simulate_resize(gpui::size(gpui::px(1024.), gpui::px(700.)));
    f.draw();
    f.click("document-workspace-rail-actions");
    f.draw();
    f.open_panel();
    f.draw();
    assert_eq!(f.left_panel(), Some(LeftPanel::Search));
    assert!(
        f.cx.debug_bounds(DOCUMENT_SEARCH_PANEL_ID).is_none(),
        "the sidebar yields to properties"
    );
    // A tool key still reaches the page.
    f.cx.simulate_keystrokes("r");
    f.draw();
    let plan = f.plan;
    let tool = f
        .workspace
        .read_with(f.cx, |workspace, cx| workspace.annotation_tool(plan, cx));
    assert_eq!(tool, Some(AnnotationTool::Rectangle));
    assert_eq!(f.summary().unwrap().text, "");

    f.click("document-workspace-rail-actions");
    f.draw();
    f.draw();
    assert!(
        f.cx.debug_bounds(DOCUMENT_SEARCH_PANEL_ID).is_some(),
        "the panel shows when properties close"
    );
}

#[gpui::test]
fn the_rail_buttons_open_close_and_switch_the_sidebar(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    assert_eq!(f.left_panel(), None);
    f.click(DOCUMENT_LEFT_RAIL_SEARCH_ID);
    assert_eq!(f.left_panel(), Some(LeftPanel::Search));
    assert!(f.cx.debug_bounds(DOCUMENT_SEARCH_PANEL_ID).is_some());
    f.click(DOCUMENT_LEFT_RAIL_SEARCH_ID);
    assert_eq!(f.left_panel(), None, "the open panel's button closes it");
    assert!(f.cx.debug_bounds(DOCUMENT_SEARCH_PANEL_ID).is_none());
    f.click("document-left-rail-pages");
    assert_eq!(f.left_panel(), Some(LeftPanel::Thumbnails));
    f.click(DOCUMENT_LEFT_RAIL_SEARCH_ID);
    assert_eq!(
        f.left_panel(),
        Some(LeftPanel::Search),
        "the other button switches the panel"
    );
    f.click("document-left-rail-pages");
    assert_eq!(f.left_panel(), Some(LeftPanel::Thumbnails));
}

#[gpui::test]
fn results_group_by_document_then_page_with_counts(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    let summary = f.search_all("\"fire door\"");
    // doors.pdf: page 1 twice (two paragraphs), page 2 once across two
    // lines, page 3 once: its two matches share one snippet, so one row.
    // plan.pdf: its Text Box ("Fire exit door" is not the phrase).
    assert_eq!(summary.hits, 5);
    assert_eq!(summary.summary, "5 results in 2 documents");
    // This window's tabs, in tab order.
    assert_eq!(
        document_rows(&summary),
        [("doors.pdf".to_owned(), 4), ("plan.pdf".to_owned(), 1)]
    );
    let page_3 = row_index(&summary, |row| {
        matches!(row, SearchPanelRow::Page { page_number: 3, .. })
    });
    match &summary.rows[page_3 + 1] {
        SearchPanelRow::Text { matched, marks, .. } => {
            let marked = marks
                .iter()
                .map(|range| matched[range.clone()].to_owned())
                .collect::<Vec<_>>();
            assert_eq!(marked, ["Fire door", "fire door"], "{matched:?}");
        }
        other => panic!("one row for both matches, not {other:?}"),
    }
    assert_eq!(
        summary.rows[1],
        SearchPanelRow::Page {
            page_number: 1,
            count: 2,
            collapsed: false
        }
    );
    match &summary.rows[2] {
        SearchPanelRow::Text {
            before,
            matched,
            after,
            ..
        } => {
            assert_eq!(before, "The ");
            assert_eq!(matched, "fire door");
            assert!(after.starts_with(" is open"), "{after:?}");
        }
        other => panic!("a text match, not {other:?}"),
    }
    // The page with no text is counted under the list.
    assert_eq!(summary.footer, ["1 page has no searchable text."]);
}

#[gpui::test]
fn a_markup_row_shows_subject_author_text_and_replies(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    let summary = f.search("author:Sam");
    assert_eq!(summary.hits, 1, "the markup with Sam's reply");
    let markup = summary
        .rows
        .iter()
        .find_map(|row| match row {
            SearchPanelRow::Markup {
                heading, replies, ..
            } => Some((heading.clone(), replies.clone())),
            _ => None,
        })
        .unwrap();
    assert_eq!(markup.0, "Text Box · Alex");
    assert_eq!(markup.1, ["Sam: Agreed, 60 min"]);
    // A match in a reply says which reply.
    f.clear_query();
    f.type_text("agreed");
    let summary = f.wait_for("the reply search", |summary| {
        summary.finished && summary.searched == "agreed"
    });
    let reply = summary.rows.iter().find_map(|row| match row {
        SearchPanelRow::Markup { reply, matched, .. } => Some((*reply, matched.clone())),
        _ => None,
    });
    assert_eq!(reply, Some((Some(0), "Agreed".to_owned())));
}

/// The scope is a stock Select: This document first, and the panel starts
/// there; All open documents is second.
#[gpui::test]
fn the_scope_starts_at_this_document_and_switches_to_all_and_back(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    let summary = f.search("\"fire door\"");
    assert_eq!(summary.scope, SearchScope::ThisDocument);
    // plan.pdf is the tab in view.
    assert_eq!(document_rows(&summary), [("plan.pdf".to_owned(), 1)]);
    assert_eq!(summary.summary, "1 result in 1 document");
    f.choose_scope(SearchScope::AllOpenDocuments);
    let summary = f.wait_for("all documents", |summary| {
        summary.scope == SearchScope::AllOpenDocuments && summary.finished
    });
    assert_eq!(document_rows(&summary).len(), 2);
    f.choose_scope(SearchScope::ThisDocument);
    let summary = f.wait_for("this document", |summary| {
        summary.scope == SearchScope::ThisDocument && summary.finished
    });
    assert_eq!(document_rows(&summary), [("plan.pdf".to_owned(), 1)]);
    // The panel keeps the chosen scope while it is closed and opened again.
    f.choose_scope(SearchScope::AllOpenDocuments);
    f.click(DOCUMENT_LEFT_RAIL_SEARCH_ID);
    f.click(DOCUMENT_LEFT_RAIL_SEARCH_ID);
    assert_eq!(f.summary().unwrap().scope, SearchScope::AllOpenDocuments);
}

/// The panel's controls are the app's medium size, as the properties
/// panels and the toolbar: 32 px buttons, field and menu, in a header,
/// controls, list layout with the grid's 8 px margins.
#[gpui::test]
fn the_panel_uses_medium_controls_on_the_grid(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    f.open_panel();
    let panel = f.cx.debug_bounds(DOCUMENT_SEARCH_PANEL_ID).unwrap();
    let header = f.cx.debug_bounds(DOCUMENT_SEARCH_HEADER_ID).unwrap();
    assert_eq!(header.size.height, gpui::px(48.), "the sidebar header bar");
    assert_eq!(header.top(), panel.top());
    let controls = f.cx.debug_bounds(DOCUMENT_SEARCH_CONTROLS_ID).unwrap();
    assert_eq!(controls.top(), header.bottom());
    for id in [
        DOCUMENT_SEARCH_MODE_TEXT_ID,
        DOCUMENT_SEARCH_MODE_VISUAL_ID,
        DOCUMENT_SEARCH_INPUT_ID,
        DOCUMENT_SEARCH_MATCH_CASE_ID,
        DOCUMENT_SEARCH_EXPLAIN_ID,
        DOCUMENT_SEARCH_COLLAPSE_ID,
        DOCUMENT_SEARCH_MORE_CONTEXT_ID,
        DOCUMENT_SEARCH_SORT_ID,
        DOCUMENT_SEARCH_SCOPE_ID,
    ] {
        let bounds = f.cx.debug_bounds(id).unwrap();
        assert_eq!(bounds.size.height, gpui::px(32.), "{id} is medium");
        assert!(
            bounds.left() >= panel.left() + gpui::px(8.)
                && bounds.right() <= panel.right() - gpui::px(8.),
            "{id} inside the margins: {bounds:?} {panel:?}"
        );
    }
    let mode = f.cx.debug_bounds(DOCUMENT_SEARCH_MODE_TEXT_ID).unwrap();
    assert_eq!(mode.left(), panel.left() + gpui::px(8.));
    // The X in the header closes the panel and gives the page the keyboard.
    f.click(DOCUMENT_SEARCH_CLOSE_ID);
    assert_eq!(f.left_panel(), None);
    assert!(f.cx.debug_bounds(DOCUMENT_SEARCH_PANEL_ID).is_none());
    assert!(!query_focused(&mut f));
    f.cx.simulate_keystrokes("r");
    f.draw();
    let plan = f.plan;
    let tool = f
        .workspace
        .read_with(f.cx, |workspace, cx| workspace.annotation_tool(plan, cx));
    assert_eq!(
        tool,
        Some(AnnotationTool::Rectangle),
        "the keys reach the page"
    );
}

#[gpui::test]
fn explain_collapse_more_context_match_case_and_sort(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    f.search_all("\"fire door\"");
    // Explain search reads the query.
    f.click(DOCUMENT_SEARCH_EXPLAIN_ID);
    let explanation = f.summary().unwrap().explanation.unwrap();
    assert!(explanation.starts_with("Finds"), "{explanation}");
    assert!(explanation.contains("fire") && explanation.contains("door"));
    // Collapse results: documents and pages only.
    f.click(DOCUMENT_SEARCH_COLLAPSE_ID);
    let summary = f.summary().unwrap();
    assert!(summary.collapse);
    assert!(summary.rows.iter().all(|row| matches!(
        row,
        SearchPanelRow::Document { .. }
            | SearchPanelRow::Page {
                collapsed: true,
                ..
            }
    )));
    assert_eq!(summary.rows.len(), 2 + 3 + 1);
    f.click(DOCUMENT_SEARCH_COLLAPSE_ID);
    // More context keeps the whole text before a match.
    let before = |summary: &SearchPanelSummary| {
        summary
            .rows
            .iter()
            .filter_map(|row| match row {
                SearchPanelRow::Text { before, .. } => Some(before.clone()),
                _ => None,
            })
            .max_by_key(|before| before.chars().count())
            .unwrap()
    };
    let short = before(&f.summary().unwrap());
    f.click(DOCUMENT_SEARCH_MORE_CONTEXT_ID);
    let long = before(&f.summary().unwrap());
    assert!(f.summary().unwrap().more_context);
    assert!(long.chars().count() >= short.chars().count());
    // Match case searches again.
    f.click(DOCUMENT_SEARCH_MATCH_CASE_ID);
    let summary = f.wait_for("Match case", |summary| {
        summary.match_case && summary.finished
    });
    // "The fire door", the wrapped one, "a fire door", and the Text Box.
    assert_eq!(summary.hits, 4);
    // The sort menu opens from its button; a sort orders the documents.
    f.click(DOCUMENT_SEARCH_SORT_ID);
    f.cx.simulate_keystrokes("escape");
    f.draw();
    f.workspace.update(f.cx, |workspace, cx| {
        workspace.set_search_sort(SearchSort::NameDescending, cx)
    });
    let summary = f.summary().unwrap();
    assert_eq!(summary.sort, SearchSort::NameDescending);
    assert_eq!(document_rows(&summary)[0].0, "plan.pdf");
}

#[gpui::test]
fn a_group_collapses_and_expands_by_click(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    let summary = f.search_all("\"fire door\"");
    let rows = summary.rows.len();
    f.click_row(0);
    let summary = f.summary().unwrap();
    assert_eq!(
        summary.rows[0],
        SearchPanelRow::Document {
            name: "doors.pdf".into(),
            count: 4,
            collapsed: true
        }
    );
    assert_eq!(summary.rows.len(), rows - 7, "its 3 pages and 4 rows hide");
    f.click_row(0);
    assert_eq!(f.summary().unwrap().rows.len(), rows);
}

#[gpui::test]
fn a_click_on_a_result_goes_to_its_tab_and_marks_it(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    let summary = f.search_all("\"fire door\"");
    assert_eq!(f.active(), Some(f.plan));
    // The third doors.pdf match: page 2, across two lines.
    let index = row_index(&summary, |row| {
        matches!(row, SearchPanelRow::Page { page_number: 2, .. })
    }) + 1;
    f.click_row(index);
    assert_eq!(f.active(), Some(f.doors), "its tab comes to the front");
    let doors = f.doors;
    let highlights = f
        .workspace
        .read_with(f.cx, |workspace, cx| workspace.find_highlights(doors, cx));
    let page = highlights
        .iter()
        .find(|page| page.page_index == 1)
        .expect("the page is marked");
    assert_eq!(page.current.len(), 2, "both lines of the match");
    assert_eq!(f.summary().unwrap().opened, Some(index));
    assert!(f.focus_in_results(), "the panel keeps the focus");
    // The find bar does not open.
    let bar = f
        .workspace
        .read_with(f.cx, |workspace, cx| workspace.find_bar_summary(doors, cx));
    assert!(bar.is_none_or(|bar| !bar.open));
    // Escape on the page clears the marks once nothing else is left.
    f.cx.update(|window, cx| {
        let focus = f.workspace.read(cx).focus_handle();
        window.focus(&focus, cx);
    });
    f.cx.simulate_keystrokes("escape");
    f.draw();
    let highlights = f
        .workspace
        .read_with(f.cx, |workspace, cx| workspace.find_highlights(doors, cx));
    assert!(highlights.is_empty());
}

#[gpui::test]
fn a_markup_result_selects_the_markup(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    let summary = f.search_all("rating");
    let index = row_index(&summary, |row| matches!(row, SearchPanelRow::Markup { .. }));
    // Another tab first, so the result switches back.
    let doors = f.doors;
    f.workspace
        .update(f.cx, |workspace, cx| workspace.activate_document(doors, cx));
    f.click_row(index);
    assert_eq!(f.active(), Some(f.plan));
    let plan = f.plan;
    let selected = f.workspace.read_with(f.cx, |workspace, cx| {
        workspace
            .annotation_snapshot(plan, cx)
            .and_then(|snapshot| snapshot.selected_id)
    });
    assert_eq!(
        selected.map(|id| id.as_str().to_owned()),
        Some("pdf:plan-note".to_owned())
    );
}

#[gpui::test]
fn the_keyboard_moves_through_the_results(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    let summary = f.search_all("\"fire door\"");
    // Down in the field: the first result.
    f.cx.simulate_keystrokes("down");
    f.draw();
    assert!(f.focus_in_results());
    assert_eq!(f.summary().unwrap().selected, Some(0));
    f.cx.simulate_keystrokes("down down");
    f.draw();
    assert_eq!(f.summary().unwrap().selected, Some(2));
    assert!(matches!(summary.rows[2], SearchPanelRow::Text { .. }));
    // Left on a hit goes to its page; Left again collapses the page.
    f.cx.simulate_keystrokes("left");
    f.draw();
    assert_eq!(f.summary().unwrap().selected, Some(1));
    f.cx.simulate_keystrokes("left");
    f.draw();
    let summary = f.summary().unwrap();
    assert!(matches!(
        summary.rows[1],
        SearchPanelRow::Page {
            collapsed: true,
            ..
        }
    ));
    f.cx.simulate_keystrokes("right");
    f.draw();
    assert!(matches!(
        f.summary().unwrap().rows[1],
        SearchPanelRow::Page {
            collapsed: false,
            ..
        }
    ));
    // Enter opens the selected hit; Up goes back.
    f.cx.simulate_keystrokes("down enter");
    f.draw();
    assert_eq!(f.summary().unwrap().opened, Some(2));
    assert_eq!(f.active(), Some(f.doors));
    assert!(f.focus_in_results(), "the arrows go on");
    f.cx.simulate_keystrokes("up");
    f.draw();
    assert_eq!(f.summary().unwrap().selected, Some(1));
    // Space opens too.
    f.cx.simulate_keystrokes("down down space");
    f.draw();
    assert_eq!(f.summary().unwrap().opened, Some(3));
}

#[gpui::test]
fn escape_in_the_field_clears_the_query_then_returns_to_the_page(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    f.search("fire");
    f.click(DOCUMENT_SEARCH_INPUT_ID);
    f.cx.simulate_keystrokes("escape");
    f.draw();
    let summary = f.summary().unwrap();
    assert_eq!(summary.text, "");
    assert!(summary.rows.is_empty());
    assert!(summary.open);
    f.cx.simulate_keystrokes("escape");
    f.draw();
    let on_page = f.cx.update(|window, cx| {
        f.workspace
            .read(cx)
            .focus_handle()
            .contains_focused(window, cx)
    });
    assert!(on_page, "the page has the focus");
    assert!(f.summary().unwrap().open, "the sidebar stays open");
}

#[gpui::test]
fn the_query_language_reaches_the_panel(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    f.open_panel();
    f.click(DOCUMENT_SEARCH_EXPLAIN_ID);
    f.type_text("file:plan door");
    let summary = f.wait_for("file filter", |summary| {
        summary.finished && summary.searched == "file:plan door"
    });
    assert_eq!(document_rows(&summary), [("plan.pdf".to_owned(), 2)]);
    let explanation = summary.explanation.unwrap();
    assert!(explanation.contains("plan"), "{explanation}");
    // An invalid regular expression says so and finds nothing.
    f.clear_query();
    f.type_text("/(/");
    let summary = f.wait_for("bad regex", |summary| {
        summary.finished && summary.searched == "/(/"
    });
    assert_eq!(summary.hits, 0);
    assert_eq!(summary.summary, "No results");
    assert_eq!(
        summary.explanation.as_deref(),
        Some("Invalid regular expression")
    );
}

#[gpui::test]
fn a_search_stops_at_the_cap_and_says_so(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    // A page of 10,200 one-letter words, each too far from the next to
    // share its row.
    let words = format!("x {} ", "a".repeat(41)).repeat(10_200);
    let content = format!("BT /F1 1 Tf 10 700 Td ({words}) Tj ET").into_bytes();
    let workspace = f.workspace.clone();
    open_document(&workspace, f.cx, "many.pdf", &[content.as_slice()], false);
    f.open_panel();
    f.click(DOCUMENT_SEARCH_INPUT_ID);
    f.type_text("x");
    let summary = f.wait_for("the cap", |summary| {
        summary.finished && summary.searched == "x"
    });
    assert_eq!(summary.hits, 10_000);
    assert_eq!(
        summary.footer,
        ["First 10,000 results. Narrow the search to see more."]
    );
}

/// A second window with its own workspace and plan.pdf, both windows known
/// to searches as the app's binary makes them.
fn second_window(f: &mut Fixture<'_>) -> (AnyWindowHandle, Entity<DocumentWorkspace>, DocumentId) {
    let slot = Rc::new(RefCell::new(None));
    let handle = f.cx.cx.add_window({
        let slot = slot.clone();
        move |window, cx| {
            let workspace = cx.new(DocumentWorkspace::new);
            slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let handle: AnyWindowHandle = handle.into();
    let workspace = slot.borrow_mut().take().unwrap();
    let ids = f.ids.clone();
    workspace.update(f.cx, |workspace, _| workspace.share_document_ids(ids));
    let mut cx2 = VisualTestContext::from_window(handle, &f.cx.cx);
    let document = open_document(&workspace, &mut cx2, "plan.pdf", &PLAN, false);
    let first = f.cx.update(|window, _| window.window_handle());
    let (one, two) = (f.workspace.downgrade(), workspace.downgrade());
    f.cx.update(|_, cx| {
        set_workspace_directory(
            move |_| vec![(first, one.clone()), (handle, two.clone())],
            cx,
        );
    });
    (handle, workspace, document)
}

#[gpui::test]
fn all_open_documents_takes_in_every_window(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    let (handle, other, other_plan) = second_window(&mut f);
    assert_ne!(other_plan, f.plan, "ids are unique across windows");
    let summary = f.search_all("door");
    assert_eq!(
        document_rows(&summary),
        [
            ("doors.pdf".to_owned(), 4),
            ("plan.pdf · window 1".to_owned(), 2),
            ("plan.pdf · window 2".to_owned(), 1),
        ]
    );
    // A result in the other window brings it forward and shows it there.
    let index = row_index(
        &summary,
        |row| matches!(row, SearchPanelRow::Document { name, .. } if name == "plan.pdf · window 2"),
    ) + 2;
    f.click_row(index);
    let mut cx2 = VisualTestContext::from_window(handle, &f.cx.cx);
    cx2.update(|window, cx| window.draw(cx).clear(cx));
    let shown = other.read_with(&cx2, |workspace, cx| {
        (
            workspace.active_document_id(),
            workspace.find_highlights(other_plan, cx),
        )
    });
    assert_eq!(shown.0, Some(other_plan));
    assert!(
        shown.1.iter().any(|page| !page.current.is_empty()),
        "the match is marked in the other window"
    );
    // This document leaves the other window out, and All takes it back.
    f.choose_scope(SearchScope::ThisDocument);
    let summary = f.wait_for("this document", |summary| {
        summary.scope == SearchScope::ThisDocument && summary.finished
    });
    assert_eq!(document_rows(&summary).len(), 1);
    f.choose_scope(SearchScope::AllOpenDocuments);
    let summary = f.wait_for("all documents", |summary| {
        summary.scope == SearchScope::AllOpenDocuments && summary.finished
    });
    assert_eq!(document_rows(&summary).len(), 3);
}

#[gpui::test]
fn the_shortcut_reaches_the_panel_from_inside_a_text_field(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    // The find bar's field has the focus.
    f.cx.simulate_keystrokes(butter_paper::document_workspace::find_shortcut(cfg!(
        target_os = "macos"
    )));
    f.draw();
    f.open_panel();
    assert_eq!(f.left_panel(), Some(LeftPanel::Search));
    f.type_text("exit");
    assert_eq!(
        f.summary().unwrap().text,
        "exit",
        "the panel's field has the focus"
    );
}

#[gpui::test]
fn a_change_in_another_window_runs_an_all_documents_search_again(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    let (handle, other, other_plan) = second_window(&mut f);
    let summary = f.search_all("rating");
    // plan.pdf of window 1 has the Text Box; window 2's plan.pdf has none.
    assert_eq!(
        document_rows(&summary),
        [("plan.pdf · window 1".to_owned(), 1)]
    );
    // A Text Box with the word is added in window 2, with no draw of
    // window 1: the panel watches the other window.
    let text_box = TextBoxAnnotation::new(
        MarkupId::new("pdf:other-window-note").unwrap(),
        0,
        PdfRect::new(300., 300., 200., 40.).unwrap(),
        "Door rating to check",
        TextBoxStyle::new("Helvetica", 12., "#111827", 1.).unwrap(),
    )
    .unwrap();
    let mut cx2 = VisualTestContext::from_window(handle, &f.cx.cx);
    other
        .update(&mut cx2, |workspace, cx| {
            workspace.create_text_box(other_plan, text_box, cx)
        })
        .unwrap();
    let summary = f.wait_for("the search to run again", |summary| {
        summary.finished && summary.hits == 2
    });
    assert_eq!(
        document_rows(&summary),
        [
            ("plan.pdf · window 1".to_owned(), 1),
            ("plan.pdf · window 2".to_owned(), 1),
        ]
    );
}

/// An edit undone within the wait leaves the results as they are, and a
/// tab opened later is still searched: the wait does not stay on.
#[gpui::test]
fn after_an_undone_edit_a_new_tab_is_still_searched(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    let summary = f.search_all("rating");
    assert_eq!(summary.hits, 1);
    let plan = f.plan;
    let text_box = TextBoxAnnotation::new(
        MarkupId::new("pdf:undone-note").unwrap(),
        0,
        PdfRect::new(300., 300., 200., 40.).unwrap(),
        "Rating undone",
        TextBoxStyle::new("Helvetica", 12., "#111827", 1.).unwrap(),
    )
    .unwrap();
    f.workspace
        .update(f.cx, |workspace, cx| {
            workspace.create_text_box(plan, text_box, cx)
        })
        .unwrap();
    f.draw();
    f.workspace
        .update(f.cx, |workspace, cx| workspace.undo_annotations(plan, cx))
        .unwrap();
    f.draw();
    // Past the wait: the search found nothing changed.
    for _ in 0..20 {
        f.cx.executor().advance_clock(Duration::from_millis(40));
        f.cx.run_until_parked();
        f.draw();
    }
    assert_eq!(f.summary().unwrap().hits, 1);
    // A new tab with the word is searched.
    let workspace = f.workspace.clone();
    open_document(
        &workspace,
        f.cx,
        "ratings.pdf",
        &[b"BT /F1 20 Tf 50 700 Td (Fire rating schedule) Tj ET"],
        false,
    );
    let summary = f.wait_for("the new tab to be searched", |summary| {
        summary.finished && summary.hits == 2
    });
    assert!(
        document_rows(&summary)
            .iter()
            .any(|(name, count)| name.starts_with("ratings.pdf") && *count == 1),
        "{:?}",
        summary.rows
    );
}

fn query_focused(f: &mut Fixture<'_>) -> bool {
    let workspace = f.workspace.clone();
    f.cx.update(|window, cx| workspace.read(cx).search_query_focused(window, cx))
}

/// Escape gives the focus to the page at once, and the field does not take
/// it back in the frames after the panel opened.
#[gpui::test]
fn escape_right_after_opening_leaves_the_focus_on_the_page(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    f.open_panel();
    assert!(query_focused(&mut f), "the shortcut focuses the field");
    f.cx.simulate_keystrokes("escape");
    for _ in 0..5 {
        f.draw();
    }
    assert!(!query_focused(&mut f), "the field took the focus back");
}

/// A click on the page right after the panel opened leaves the focus on
/// the page.
#[gpui::test]
fn a_click_out_right_after_opening_leaves_the_focus_on_the_page(cx: &mut TestAppContext) {
    let mut f = setup(cx);
    f.open_panel();
    assert!(query_focused(&mut f), "the shortcut focuses the field");
    f.click(butter_paper::document_workspace::DOCUMENT_VIEWPORT_ID);
    for _ in 0..5 {
        f.draw();
    }
    assert!(!query_focused(&mut f), "the field took the focus back");
}
